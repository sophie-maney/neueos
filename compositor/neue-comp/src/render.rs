//! What goes on an output, shared by the winit, tty and headless backends and by
//! screencopy.
//!
//! Like Smithay's `space_render_elements`, but each window is followed by its server-side
//! title bar, so a bar stacks with its window instead of floating above every window.
//! Layer-shell surfaces are drawn in Smithay's order: overlay and top above the windows,
//! bottom and background below. On an output covered by a full-screen window (ADR 0018)
//! only overlay stays above; top goes below the windows, and a black backdrop fills what
//! a smaller full-screen window leaves.

use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::{GlesRenderbuffer, GlesRenderer};
use smithay::backend::renderer::utils::CommitCounter;
use smithay::backend::renderer::{Bind, Blit, ExportMem, Offscreen, Renderer, TextureFilter};
use smithay::desktop::{LayerMap, LayerSurface, Space, Window, layer_map_for_output};
use smithay::output::Output;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::session_lock::LockSurface;
use smithay::wayland::shell::wlr_layer::Layer;

use crate::decorations::{self, Chrome, frame_geometry};
use crate::fullscreen::fullscreen_window_on;

/// Background behind all windows until the desktop picture lands (Phase 3).
pub const CLEAR_COLOR: [f32; 4] = [0.18, 0.2, 0.24, 1.0];

/// What a full-screen window smaller than its output is shown on (xdg-shell asks for a
/// border fill; black, as everyone does).
const FULLSCREEN_BACKDROP: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

smithay::backend::renderer::element::render_elements! {
    /// Windows, their title bars and layer surfaces on one output, topmost first.
    pub OutputElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Chrome=MemoryRenderBufferRenderElement<GlesRenderer>,
    Backdrop=SolidColorRenderElement,
}

smithay::backend::renderer::element::render_elements! {
    /// Everything on an output including the pointer, topmost first: what the tty backend
    /// scans out and what a screencopy with cursors shows.
    pub ScreenElement<=GlesRenderer>;
    Output=OutputElement,
    Cursor=MemoryRenderBufferRenderElement<GlesRenderer>,
    /// A client-drawn cursor or the drag icon.
    CursorSurface=WaylandSurfaceRenderElement<GlesRenderer>,
}

/// Everything on `output`, topmost first. Empty if the output is not in `space`.
pub fn output_elements(
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    chrome: &mut Chrome,
    output: &Output,
) -> Vec<OutputElement> {
    let Some(output_geo) = space.output_geometry(output) else {
        return Vec::new();
    };
    let output_scale = output.current_scale().fractional_scale();
    let scale = Scale::from(output_scale);
    let mut elements = Vec::new();

    // A locked session shows the lock surface and nothing else (the clear colour where
    // there is none).
    if crate::lock::with_output_lock(output, |lock| lock.locked) {
        if let Some(surface) = crate::lock::with_output_lock(output, |lock| lock.surface.clone())
            .filter(LockSurface::alive)
        {
            elements.extend(
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    surface.wl_surface(),
                    (0, 0),
                    scale,
                    1.0,
                    Kind::Unspecified,
                )
                .into_iter()
                .map(OutputElement::Surface),
            );
        }
        return elements;
    }

    let fullscreen = fullscreen_window_on(space, output);
    let layer_map = layer_map_for_output(output);
    let below_windows = |layer: Layer| match layer {
        Layer::Background | Layer::Bottom => true,
        Layer::Top => fullscreen.is_some(),
        Layer::Overlay => false,
    };
    let (lower, upper): (Vec<&LayerSurface>, Vec<&LayerSurface>) = layer_map
        .layers()
        .rev()
        .partition(|s| below_windows(s.layer()));
    push_layers(&mut elements, renderer, &layer_map, upper, scale);

    for window in space.elements().rev() {
        let Some(geometry) = space.element_geometry(window) else {
            continue;
        };
        let decorated = decorations::has_title_bar(window);
        let frame = frame_geometry(window, geometry);
        let bbox = space
            .element_bbox(window)
            .map_or(frame, |bbox| bbox.merge(frame));
        if !output_geo.overlaps(bbox) {
            continue;
        }
        let render_location = geometry.loc - window.geometry().loc - output_geo.loc;
        elements.extend(
            window
                .render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    render_location.to_physical_precise_round(output_scale),
                    scale,
                    1.0,
                )
                .into_iter()
                .map(OutputElement::Surface),
        );
        if decorated {
            let bar: Point<i32, Logical> = frame.loc - output_geo.loc;
            elements.extend(
                chrome
                    .elements(
                        renderer,
                        window,
                        bar.to_f64().to_physical(scale),
                        frame.size.w,
                        output_scale,
                    )
                    .into_iter()
                    .map(OutputElement::Chrome),
            );
        }
        if fullscreen.as_ref() == Some(window) && geometry.size != output_geo.size {
            elements.push(OutputElement::Backdrop(backdrop(output, output_geo, scale)));
        }
    }

    push_layers(&mut elements, renderer, &layer_map, lower, scale);
    elements
}

/// Append the render elements of layer `surfaces`, topmost first.
fn push_layers(
    elements: &mut Vec<OutputElement>,
    renderer: &mut GlesRenderer,
    layer_map: &LayerMap,
    surfaces: Vec<&LayerSurface>,
    scale: Scale<f64>,
) {
    for surface in surfaces {
        let Some(geo) = layer_map.layer_geometry(surface) else {
            continue;
        };
        elements.extend(
            AsRenderElements::<GlesRenderer>::render_elements::<
                WaylandSurfaceRenderElement<GlesRenderer>,
            >(
                surface,
                renderer,
                geo.loc.to_physical_precise_round(scale),
                scale,
                1.0,
            )
            .into_iter()
            .map(OutputElement::Surface),
        );
    }
}

/// The black fill behind a full-screen window that is smaller than `output`. The id is
/// kept per output so the damage tracker sees the same element from frame to frame.
fn backdrop(
    output: &Output,
    output_geo: Rectangle<i32, Logical>,
    scale: Scale<f64>,
) -> SolidColorRenderElement {
    let id = output
        .user_data()
        .get_or_insert(|| BackdropId(Id::new()))
        .0
        .clone();
    let size = output_geo.size.to_f64().to_physical(scale).to_i32_round();
    SolidColorRenderElement::new(
        id,
        Rectangle::from_size(size),
        CommitCounter::default(),
        FULLSCREEN_BACKDROP,
        Kind::Unspecified,
    )
}

/// The surfaces of `window` (and its popups) with its geometry's top-left corner at the
/// origin, drawn at `scale`: what a toplevel capture shows. No title bar.
pub fn window_content_elements(
    renderer: &mut GlesRenderer,
    window: &Window,
    scale: f64,
) -> Vec<OutputElement> {
    let location: Point<i32, Logical> = Point::from((0, 0)) - window.geometry().loc;
    window
        .render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
            renderer,
            location.to_physical_precise_round(scale),
            Scale::from(scale),
            1.0,
        )
        .into_iter()
        .map(OutputElement::Surface)
        .collect()
}

/// Render element id of an output's full-screen backdrop.
struct BackdropId(Id);

/// Pixel format of [`OffscreenOutput`]'s buffer. RGBA8 renderbuffers are core GLES 3.
const OFFSCREEN_FORMAT: Fourcc = Fourcc::Abgr8888;
/// Format [`OffscreenOutput::read_back`] returns: `wl_shm`'s mandatory ARGB8888.
const READ_FORMAT: Fourcc = Fourcc::Argb8888;

/// An output drawn into an offscreen buffer rather than onto a screen: a headless output
/// (ADR 0021) or one screencopy session (ADR 0022). The buffer is kept between frames, so
/// after the first frame only damage is redrawn, and the damage is what changed since the
/// previous frame of this buffer.
pub struct OffscreenOutput {
    buffer: GlesRenderbuffer,
    size: Size<i32, Physical>,
    scale: f64,
    damage_tracker: OutputDamageTracker,
    /// The buffer holds the previous frame (buffer age 1). False until the first render.
    drawn: bool,
    /// What the buffer is cleared to under the elements (premultiplied).
    clear: [f32; 4],
}

impl std::fmt::Debug for OffscreenOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OffscreenOutput")
            .field("size", &self.size)
            .field("scale", &self.scale)
            .field("drawn", &self.drawn)
            .finish_non_exhaustive()
    }
}

impl OffscreenOutput {
    /// A buffer of `size` physical pixels (the output's size after its transform) for
    /// elements made at `scale`. Drawn upright: transform normal.
    pub fn new(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        scale: f64,
    ) -> Result<Self, String> {
        Self::with_clear(renderer, size, scale, CLEAR_COLOR)
    }

    /// Like [`Self::new`], cleared to `clear` (premultiplied RGBA) instead of
    /// [`CLEAR_COLOR`]: transparent for a window capture.
    pub fn with_clear(
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        scale: f64,
        clear: [f32; 4],
    ) -> Result<Self, String> {
        let buffer = renderer
            .create_buffer(OFFSCREEN_FORMAT, (size.w, size.h).into())
            .map_err(|err| format!("creating the offscreen buffer: {err:?}"))?;
        Ok(Self {
            buffer,
            size,
            scale,
            damage_tracker: OutputDamageTracker::new(size, scale, Transform::Normal),
            drawn: false,
            clear,
        })
    }

    /// Whether this buffer is still right for an output of `size` at `scale`.
    pub fn fits(&self, size: Size<i32, Physical>, scale: f64) -> bool {
        self.size == size && self.scale == scale
    }

    pub fn size(&self) -> Size<i32, Physical> {
        self.size
    }

    /// Draw `elements` (topmost first) on the clear colour and return what changed since the
    /// previous call: everything the first time, nothing if nothing changed (then nothing
    /// is drawn).
    pub fn render<E: RenderElement<GlesRenderer>>(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &[E],
    ) -> Result<Vec<Rectangle<i32, Physical>>, String> {
        let mut framebuffer = renderer
            .bind(&mut self.buffer)
            .map_err(|err| format!("binding the offscreen buffer: {err:?}"))?;
        let age = usize::from(self.drawn);
        let result = self
            .damage_tracker
            .render_output(renderer, &mut framebuffer, age, elements, self.clear)
            .map_err(|err| format!("rendering: {err:?}"))?;
        self.drawn = true;
        Ok(result.damage.cloned().unwrap_or_default())
    }

    /// The pixels of the last frame as ARGB8888 (`wl_shm`: bytes B, G, R, A in memory),
    /// top row first, rows packed (`4 * width` bytes each).
    pub fn read_back(&mut self, renderer: &mut GlesRenderer) -> Result<Vec<u8>, String> {
        let framebuffer = renderer
            .bind(&mut self.buffer)
            .map_err(|err| format!("binding the offscreen buffer: {err:?}"))?;
        let region = Rectangle::from_size((self.size.w, self.size.h).into());
        let mapping = renderer
            .copy_framebuffer(&framebuffer, region, READ_FORMAT)
            .map_err(|err| format!("reading the pixels back: {err:?}"))?;
        let bytes = renderer
            .map_texture(&mapping)
            .map_err(|err| format!("mapping the pixels: {err:?}"))?;
        // Smithay's GLES frame flips y when drawing into an offscreen target, so the rows
        // read back already start at the top. (`TextureMapping::flipped` says true: it
        // describes GL's bottom-up convention, not what was drawn. The tests check this.)
        Ok(bytes.to_vec())
    }

    /// Copy the last frame into the top-left corner of `dmabuf` (a client's buffer, at
    /// least as large) and wait until the GPU is done, so the client may read it at once.
    pub fn blit_into(
        &mut self,
        renderer: &mut GlesRenderer,
        dmabuf: &mut Dmabuf,
    ) -> Result<(), String> {
        let source = renderer
            .bind(&mut self.buffer)
            .map_err(|err| format!("binding the offscreen buffer: {err:?}"))?;
        let mut target = renderer
            .bind(dmabuf)
            .map_err(|err| format!("binding the client's dmabuf: {err:?}"))?;
        let rect = Rectangle::from_size(self.size);
        let sync = renderer
            .blit(&source, &mut target, rect, rect, TextureFilter::Nearest)
            .map_err(|err| format!("copying into the client's dmabuf: {err:?}"))?;
        renderer
            .wait(&sync)
            .map_err(|err| format!("waiting for the copy: {err:?}"))
    }
}
