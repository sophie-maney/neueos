//! Server-side title bars (task 1.10, ADR 0013).
//!
//! - **Negotiation:** `xdg-decoration` and KDE's older `org_kde_kwin_server_decoration`
//!   (GTK3, older Qt). We prefer server-side and say so by default; a client that asks for
//!   client-side decorations gets them. Clients that bind neither draw their own.
//! - **Geometry:** the space keeps placing a window's client geometry; the title bar sits
//!   [`TITLE_BAR_HEIGHT`] above it. Placement, zoom and move constraints use the work area
//!   minus the bar ([`State::content_area`]), so the bar stays on screen.
//! - **Drawing:** the bar comes from `neue-paint` on `vello_cpu`, as three slices (left
//!   corner with the traffic lights, a stretchable 1 px column, right corner) rendered once
//!   per style and scale and shared by every window, plus the title rendered per window.
//!   Each window wraps them in its own `MemoryRenderBuffer`s, so damage tracking sees
//!   distinct elements; a resize only stretches the middle slice and moves the right one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use neue_paint::Variant;
use neue_paint::Pixmap;
use neue_paint::cpu::{CpuRenderer, Quality};
use neue_paint::materials::title_bar::{
    self, TITLE_FONT_SIZE, TitleBarSlice, TitleBarStyle, TrafficLight,
};
use neue_paint::text::TextEngine;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::Window;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as XdgMode;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
    Mode as KdeMode, OrgKdeKwinServerDecoration,
};
use smithay::reexports::wayland_server::WEnum;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::xdg::ToplevelSurface;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use tracing::{debug, warn};

use crate::state::State;

/// Height of a server-side title bar, in logical pixels.
pub const TITLE_BAR_HEIGHT: i32 = {
    // The material's height is a whole number of logical pixels.
    #[allow(clippy::cast_possible_truncation, reason = "22.0 is exact")]
    let h = title_bar::TITLE_BAR_HEIGHT as i32;
    h
};

/// The mode a surface asked for through the KDE protocol, kept in its data map.
#[derive(Debug, Default)]
struct KdeDecoration(RefCell<Option<KdeMode>>);

/// The window whose traffic lights the pointer is over (or pressing), for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Hover {
    pub window: Window,
    /// The button held down, while the pointer is still over it.
    pub pressed: Option<TrafficLight>,
}

/// Shared title-bar rendering (the CPU renderer, the text engine and the slice cache) and
/// the pointer's interaction state with the chrome (task 1.11, ADR 0014).
#[derive(Debug)]
pub struct Chrome {
    renderer: CpuRenderer,
    text: TextEngine,
    /// Left, middle and right slices per style and scale (`f64::to_bits`).
    slices: HashMap<(TitleBarStyle, u64), Arc<[Pixmap; 3]>>,
    /// Theme variant of every title bar: `appearance.toml`'s (light, blue, standard until
    /// it is read).
    pub variant: Variant,
    /// The traffic lights under the pointer, if any.
    pub hover: Option<Hover>,
    /// The pointer is over some chrome (so the cursor is ours, not a client's).
    pub pointer_over: bool,
    /// The last title-bar click that did not start a drag: window and time (ms), for
    /// double-click detection.
    pub last_click: Option<(Window, u32)>,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            renderer: CpuRenderer::new(Quality::Fast),
            text: TextEngine::new(),
            slices: HashMap::new(),
            variant: Variant::default(),
            hover: None,
            pointer_over: false,
            last_click: None,
        }
    }
}

/// A window's title-bar buffers, kept in its user data. Keyed by scale (`f64::to_bits`),
/// so a window spanning outputs of different scales keeps one set per output instead of
/// re-uploading every frame.
#[derive(Debug, Default)]
struct WindowChrome {
    /// The style the slices were made for, and one buffer per slice.
    slices: HashMap<u64, (TitleBarStyle, [Slab; 3])>,
    titles: HashMap<u64, TitleBuffer>,
}

/// The rendered title of one window.
#[derive(Debug)]
struct TitleBuffer {
    text: String,
    style: TitleBarStyle,
    /// Room it was laid out for; only matters when it had to be truncated.
    max_width: f64,
    truncated: bool,
    /// Logical width of the text itself (for centring).
    text_width: f64,
    /// Logical width of the buffer (text plus shadow).
    width: f64,
    /// `None` for an empty title.
    buffer: Option<Slab>,
}

/// A rendered piece of chrome and its size in device pixels.
#[derive(Debug, Clone)]
struct Slab {
    buffer: MemoryRenderBuffer,
    size: Size<f64, Logical>,
}

impl Slab {
    /// Wrap a premultiplied RGBA8 pixmap in a buffer of scale 1. Element sizes are given
    /// in logical pixels and the source in buffer pixels, so fractional scales work too.
    fn new(pixmap: &Pixmap) -> Self {
        let (w, h) = (i32::from(pixmap.width()), i32::from(pixmap.height()));
        Self {
            buffer: MemoryRenderBuffer::from_slice(
                pixmap.data_as_u8_slice(),
                // RGBA bytes in memory are DRM's little-endian ABGR8888.
                Fourcc::Abgr8888,
                (w, h),
                1,
                Transform::Normal,
                None,
            ),
            size: Size::from((f64::from(w), f64::from(h))),
        }
    }
}

fn with_window_chrome<T>(window: &Window, f: impl FnOnce(&mut WindowChrome) -> T) -> T {
    let user_data = window.user_data();
    user_data.insert_if_missing(|| RefCell::new(WindowChrome::default()));
    // Inserted just above; only the compositor thread touches window user data.
    match user_data.get::<RefCell<WindowChrome>>() {
        Some(data) => f(&mut data.borrow_mut()),
        None => unreachable!("WindowChrome was just inserted"),
    }
}

impl Chrome {
    /// The three slices for `style` at `scale`, rendering them on first use. Styles with
    /// the pointer over the buttons only re-render the left slice; the others come from
    /// the plain style.
    fn slices(&mut self, style: TitleBarStyle, scale: f64) -> Option<Arc<[Pixmap; 3]>> {
        let key = (style, scale.to_bits());
        if let Some(slices) = self.slices.get(&key) {
            return Some(Arc::clone(slices));
        }
        let start = Instant::now();
        let mut render = |slice| {
            self.renderer
                .render(&title_bar::title_bar_slice_scene(slice, style), scale)
                .map_err(|err| warn!(%err, "rendering a title bar slice failed"))
                .ok()
        };
        let slices = if style == style.without_interaction() {
            let [left, middle, right] = TitleBarSlice::ALL.map(&mut render);
            Arc::new([left?, middle?, right?])
        } else {
            let left = render(TitleBarSlice::Left)?;
            let plain = self.slices(style.without_interaction(), scale)?;
            Arc::new([left, plain[1].clone(), plain[2].clone()])
        };
        debug!(
            ?style,
            scale,
            micros = start.elapsed().as_micros(),
            "rendered title bar slices"
        );
        self.slices.insert(key, Arc::clone(&slices));
        Some(slices)
    }

    /// Make sure `window`'s title buffer shows `text` in `style` at `scale` within
    /// `max_width`, re-rendering only when something visible changed.
    fn update_title(
        &mut self,
        window: &Window,
        text: &str,
        style: TitleBarStyle,
        scale: f64,
        max_width: f64,
    ) {
        let fresh = with_window_chrome(window, |c| {
            c.titles.get(&scale.to_bits()).is_some_and(|t| {
                t.text == text
                    && t.style == style
                    && if t.truncated {
                        t.max_width == max_width
                    } else {
                        t.text_width <= max_width
                    }
            })
        });
        if fresh {
            return;
        }
        let start = Instant::now();
        let line = self.text.line_truncated(text, TITLE_FONT_SIZE, max_width);
        let buffer = if line.runs().is_empty() {
            None
        } else {
            match self
                .renderer
                .render(&title_bar::title_text_scene(&line, style), scale)
            {
                Ok(pixmap) => Some(Slab::new(&pixmap)),
                Err(err) => {
                    warn!(%err, "rendering a window title failed");
                    None
                }
            }
        };
        debug!(
            chars = text.chars().count(),
            scale,
            micros = start.elapsed().as_micros(),
            "rendered window title"
        );
        let title = TitleBuffer {
            text: text.to_owned(),
            style,
            max_width,
            truncated: line.is_truncated(),
            text_width: line.width(),
            width: title_bar::title_text_width(&line),
            buffer,
        };
        with_window_chrome(window, |c| c.titles.insert(scale.to_bits(), title));
    }

    /// Render elements for `window`'s title bar, whose top-left corner is at `location`
    /// (physical, relative to the output) and which is `width` logical pixels wide.
    /// Topmost first.
    pub fn elements(
        &mut self,
        renderer: &mut GlesRenderer,
        window: &Window,
        location: Point<f64, Physical>,
        width: i32,
        scale: f64,
    ) -> Vec<MemoryRenderBufferRenderElement<GlesRenderer>> {
        let (hover, pressed) = match &self.hover {
            Some(h) if h.window == *window => (true, h.pressed),
            _ => (false, None),
        };
        let style = TitleBarStyle {
            variant: self.variant,
            focused: is_activated(window),
            hover,
            pressed,
        };
        let Some(slices) = self.slices(style, scale) else {
            return Vec::new();
        };
        let key = scale.to_bits();
        let buffers = with_window_chrome(window, |c| match c.slices.get(&key) {
            Some((s, buffers)) if *s == style => buffers.clone(),
            _ => {
                let buffers = [0, 1, 2].map(|i| Slab::new(&slices[i]));
                c.slices.insert(key, (style, buffers.clone()));
                buffers
            }
        });
        let width = f64::from(width).max(title_bar::MIN_WIDTH);
        let (title, _) = crate::wm::title_and_app_id(window);
        self.update_title(
            window,
            title.as_deref().unwrap_or_default(),
            style.without_interaction(),
            scale,
            title_bar::title_max_width(width),
        );

        let scale = Scale::from(scale);
        let height = f64::from(TITLE_BAR_HEIGHT);
        let at = |x: f64| location + Point::<f64, Logical>::from((x, 0.0)).to_physical(scale);
        let middle_width = width - title_bar::LEFT_SLICE_WIDTH - title_bar::RIGHT_SLICE_WIDTH;
        // Where each slab goes and how wide it is drawn, in logical pixels.
        let mut parts: Vec<(Point<f64, Physical>, &Slab, f64)> = vec![
            (at(0.0), &buffers[0], title_bar::LEFT_SLICE_WIDTH),
            (at(title_bar::LEFT_SLICE_WIDTH), &buffers[1], middle_width),
            (
                at(width - title_bar::RIGHT_SLICE_WIDTH),
                &buffers[2],
                title_bar::RIGHT_SLICE_WIDTH,
            ),
        ];
        let title = with_window_chrome(window, |c| {
            c.titles
                .get(&key)
                .and_then(|t| Some((t.buffer.clone()?, t.text_width, t.width)))
        });
        if let Some((buffer, text_width, buffer_width)) = &title {
            let x = title_bar::title_x(width, *text_width);
            parts.push((at(x), buffer, *buffer_width));
        }

        let mut elements = Vec::with_capacity(parts.len());
        // The title goes on top, so iterate in reverse.
        for (location, slab, logical_width) in parts.into_iter().rev() {
            // Sizes are whole logical pixels: slices and titles are made that way.
            #[allow(clippy::cast_possible_truncation, reason = "whole logical pixels")]
            let size = Size::<i32, Logical>::from((logical_width.ceil() as i32, height as i32));
            // The source is the whole buffer, in its own pixels (buffer scale 1).
            let src = Rectangle::from_size(slab.size);
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                location,
                &slab.buffer,
                None,
                Some(src),
                Some(size),
                Kind::Unspecified,
            ) {
                Ok(element) => elements.push(element),
                Err(err) => warn!(?err, "uploading a title bar slice failed"),
            }
        }
        elements
    }
}

/// Whether `window` is the activated (focused) one, as last configured.
fn is_activated(window: &Window) -> bool {
    window.toplevel().is_some_and(|t| {
        t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Activated))
    })
}

/// Whether we draw `window`'s title bar.
pub fn is_server_decorated(window: &Window) -> bool {
    let Some(toplevel) = window.toplevel() else {
        return false;
    };
    let xdg = toplevel.with_pending_state(|s| s.decoration_mode);
    if xdg == Some(XdgMode::ServerSide) {
        return true;
    }
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<KdeDecoration>()
            .is_some_and(|d| *d.0.borrow() == Some(KdeMode::Server))
    })
}

/// Whether `window` shows our title bar: server-side decorated and not full screen.
pub fn has_title_bar(window: &Window) -> bool {
    is_server_decorated(window) && !State::is_fullscreen(window)
}

/// Height of `window`'s title bar, 0 if it draws its own or is full screen.
pub fn title_bar_height(window: &Window) -> i32 {
    if has_title_bar(window) {
        TITLE_BAR_HEIGHT
    } else {
        0
    }
}

/// `window`'s frame: its client `geometry` plus the title bar, global logical coordinates.
pub fn frame_geometry(
    window: &Window,
    geometry: Rectangle<i32, Logical>,
) -> Rectangle<i32, Logical> {
    let bar = title_bar_height(window);
    Rectangle::new(
        (geometry.loc.x, geometry.loc.y - bar).into(),
        (geometry.size.w, geometry.size.h + bar).into(),
    )
}

impl State {
    /// The part of `work` where `window`'s client area may go: below its title bar.
    pub fn content_area(
        &self,
        window: &Window,
        work: Rectangle<i32, Logical>,
    ) -> Rectangle<i32, Logical> {
        let bar = title_bar_height(window).min(work.size.h);
        Rectangle::new(
            (work.loc.x, work.loc.y + bar).into(),
            (work.size.w, work.size.h - bar).into(),
        )
    }

    /// The window whose toplevel is `toplevel`, mapped, minimised or not mapped yet.
    fn decorated_window(&self, toplevel: &ToplevelSurface) -> Option<Window> {
        self.window_for_surface(toplevel.wl_surface()).or_else(|| {
            self.unmapped
                .iter()
                .find(|w| w.toplevel() == Some(toplevel))
                .cloned()
        })
    }

    /// The decoration mode of `surface`'s window changed: refit a zoomed window.
    fn decoration_changed(&mut self, surface: &WlSurface) {
        let Some(window) = self.window_for_surface(surface) else {
            return;
        };
        if Self::with_wm_data(&window, |d| d.zoomed) {
            self.refit_zoomed(&window);
        }
        self.notify_window_changed(&window);
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|s| s.decoration_mode = Some(XdgMode::ServerSide));
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: XdgMode) {
        toplevel.with_pending_state(|s| s.decoration_mode = Some(mode));
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        if self.decorated_window(&toplevel).is_some() {
            self.decoration_changed(toplevel.wl_surface());
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        XdgDecorationHandler::request_mode(self, toplevel, XdgMode::ServerSide);
    }
}

impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
    }

    fn new_decoration(&mut self, surface: &WlSurface, decoration: &OrgKdeKwinServerDecoration) {
        set_kde_mode(surface, Some(KdeMode::Server));
        decoration.mode(KdeMode::Server);
        self.decoration_changed(surface);
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        let WEnum::Value(mode) = mode else {
            return;
        };
        // Clients may echo our mode back; only answer a change, so there is no loop.
        let previous = with_states(surface, |states| {
            states
                .data_map
                .get::<KdeDecoration>()
                .and_then(|d| *d.0.borrow())
        });
        set_kde_mode(surface, Some(mode));
        if previous != Some(mode) {
            decoration.mode(mode);
            self.decoration_changed(surface);
        }
    }

    fn release(&mut self, _decoration: &OrgKdeKwinServerDecoration, surface: &WlSurface) {
        set_kde_mode(surface, None);
        self.decoration_changed(surface);
    }
}

fn set_kde_mode(surface: &WlSurface, mode: Option<KdeMode>) {
    with_states(surface, |states| {
        states.data_map.insert_if_missing(KdeDecoration::default);
        if let Some(d) = states.data_map.get::<KdeDecoration>() {
            *d.0.borrow_mut() = mode;
        }
    });
}
