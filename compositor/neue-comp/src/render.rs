//! What goes on an output, shared by the winit, tty and headless backends.
//!
//! Like Smithay's `space_render_elements`, but each window is followed by its server-side
//! title bar, so a bar stacks with its window instead of floating above every window.
//! Layer-shell surfaces are drawn in Smithay's order: overlay and top above the windows,
//! bottom and background below. On an output covered by a full-screen window (ADR 0018)
//! only overlay stays above; top goes below the windows, and a black backdrop fills what
//! a smaller full-screen window leaves.

use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::element::Id;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::CommitCounter;
use smithay::desktop::{LayerMap, LayerSurface, Space, Window, layer_map_for_output};
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, Scale};
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

/// Render element id of an output's full-screen backdrop.
struct BackdropId(Id);
