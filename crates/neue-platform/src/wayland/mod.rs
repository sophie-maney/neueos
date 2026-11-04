//! smithay-client-toolkit driver for Masonry's `RenderRoot` (ADR 0002 §4).
//!
//! **Spike (task 2.2).** It proves the pieces fit: xdg-toplevel and layer-shell surfaces,
//! wgpu (vello_gpu), fractional scale with viewporter, text-input-v3 into Masonry's IME
//! events, and Masonry layers realised as xdg-popups. Task 2.5 hardens it (cursor shape,
//! key repeat, a full key table, headless tests); task 2.6 adds popup positioning for all
//! layer kinds, clipboard and AccessKit.
//!
//! One [`RenderRoot`] per surface given to [`run`]. Each Masonry layer it creates (menus,
//! tooltips) becomes an xdg-popup of that surface: the layer is still laid out and hit-tested
//! by Masonry in window coordinates, but its paint goes to the popup's own `wl_surface`, and
//! pointer input on the popup is translated back into window coordinates.

mod handle;
mod input;

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use masonry_core::app::VisualLayerKind;
use masonry_core::app::{RenderRoot, RenderRootOptions, RenderRootSignal, WindowSizePolicy};
use masonry_core::core::keyboard::KeyState;
use masonry_core::core::{
    ErasedAction, Ime, Modifiers, NewWidget, PointerButtonEvent, PointerEvent as MasonryPointer,
    PointerScrollEvent, PointerState, PointerUpdate, ScrollDelta, TextEvent, Widget, WidgetId,
    WindowEvent,
};
use masonry_core::dpi::{PhysicalPosition, PhysicalSize};
use masonry_core::imaging::record::Scene;
use masonry_core::kurbo::{Affine, Point, Rect, Vec2};
use masonry_core::peniko::Color;
use masonry_core::ui_events::pointer::PointerButtons;
use masonry_imaging::{Layer as ImagingLayer, PreparedFrame};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, FrameCallbackData};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, RawModifiers};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgShell, XdgSurface as _};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::{GlobalList, registry_queue_init};
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use wayland_protocols::xdg::shell::client::xdg_positioner;

use crate::gpu::{Gpu, GpuError, SurfaceTarget};

/// Driver errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't connect to the Wayland display: {0}")]
    Connect(#[from] wayland_client::ConnectError),
    #[error("can't read the Wayland globals: {0}")]
    Globals(#[from] wayland_client::globals::GlobalError),
    #[error("required Wayland global missing: {0}")]
    Bind(#[from] wayland_client::globals::BindError),
    #[error("required Wayland global missing: {0}")]
    Global(#[from] smithay_client_toolkit::error::GlobalError),
    #[error("wgpu: {0}")]
    Gpu(String),
    #[error("event loop: {0}")]
    Loop(String),
    #[error("no surfaces to show")]
    NoSurfaces,
}

impl From<GpuError> for Error {
    fn from(err: GpuError) -> Self {
        Self::Gpu(err.0)
    }
}

/// What kind of surface to create.
#[derive(Debug, Clone)]
pub enum SurfaceRole {
    /// An xdg-toplevel with server-side decorations.
    Toplevel {
        title: String,
        app_id: String,
        /// Logical size used until the compositor picks one.
        width: u32,
        height: u32,
    },
    /// A wlr-layer-shell surface on the top layer, anchored to the top edge, centred.
    Layer {
        namespace: String,
        width: u32,
        height: u32,
        margin_top: i32,
    },
}

/// One surface and its widget tree.
#[derive(Debug)]
pub struct SurfaceSpec {
    pub role: SurfaceRole,
    pub root: NewWidget<dyn Widget>,
    pub background: Color,
}

pub use crate::Options;

/// Called for every widget action: (surface index, widget, action).
pub type ActionHandler = Box<dyn FnMut(usize, WidgetId, ErasedAction)>;

/// Connects to the compositor, maps every surface and runs until a toplevel is closed or a
/// widget requests exit.
pub fn run(
    surfaces: Vec<SurfaceSpec>,
    options: Options,
    on_action: ActionHandler,
) -> Result<(), Error> {
    if surfaces.is_empty() {
        return Err(Error::NoSurfaces);
    }
    let conn = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<'static, State> =
        EventLoop::try_new().map_err(|e| Error::Loop(e.to_string()))?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| Error::Loop(e.to_string()))?;

    let globals_ = Globals::bind(&globals, &qh)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
        Box::new(handle::Display(conn.clone())),
    ));

    // wl_surfaces and wgpu surfaces first: the adapter must be compatible with one.
    let mut pending = Vec::new();
    for spec in surfaces {
        let wl = globals_.compositor.create_surface(&qh);
        let wgpu_surface = handle::create_surface(&instance, &conn, &wl)?;
        pending.push((spec, wl, wgpu_surface));
    }
    let gpu = Gpu::new(instance, &pending[0].2)?;

    let mut state = State {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        g: globals_,
        conn: conn.clone(),
        qh: qh.clone(),
        gpu,
        windows: Vec::new(),
        options,
        on_action,
        seat: None,
        pointer: None,
        keyboard: None,
        text_input: None,
        ti_focus: None,
        kb_focus: None,
        kb_pending_leave: None,
        modifiers: Modifiers::empty(),
        buttons: PointerButtons::default(),
        last_serial: None,
        exit: false,
    };
    for (spec, wl, wgpu_surface) in pending {
        state.add_window(spec, wl, wgpu_surface);
    }
    // sctk binds the seats that exist at start-up without calling `new_seat` for them.
    let seats: Vec<_> = state.seats.seats().collect();
    for seat in seats {
        state.adopt_seat(seat);
    }

    while !state.exit {
        event_loop
            .dispatch(Some(Duration::from_millis(500)), &mut state)
            .map_err(|e| Error::Loop(e.to_string()))?;
        state.flush_focus_leave();
        state.process_signals();
        state.draw_dirty();
        if let Err(err) = conn.flush() {
            tracing::warn!(%err, "flushing the Wayland connection failed");
        }
    }
    Ok(())
}

/// Globals beyond what sctk's state objects bind.
struct Globals {
    compositor: CompositorState,
    xdg: XdgShell,
    layer_shell: Option<LayerShell>,
    fractional: Option<WpFractionalScaleManagerV1>,
    viewporter: Option<WpViewporter>,
    text_input: Option<ZwpTextInputManagerV3>,
}

impl Globals {
    fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Result<Self, Error> {
        let optional = |name: &str, present: bool| {
            if !present {
                tracing::warn!("compositor lacks {name}");
            }
        };
        let layer_shell = LayerShell::bind(globals, qh).ok();
        let fractional = globals.bind(qh, 1..=1, NoEvents).ok();
        let viewporter = globals.bind(qh, 1..=1, NoEvents).ok();
        let text_input = globals.bind(qh, 1..=1, NoEvents).ok();
        optional("zwlr_layer_shell_v1", layer_shell.is_some());
        optional("wp_fractional_scale_manager_v1", fractional.is_some());
        optional("wp_viewporter", viewporter.is_some());
        optional("zwp_text_input_manager_v3", text_input.is_some());
        Ok(Self {
            compositor: CompositorState::bind(globals, qh)?,
            xdg: XdgShell::bind(globals, qh)?,
            layer_shell,
            // Fractional scale needs a viewport to set the logical size.
            fractional: fractional.filter(|_| viewporter.is_some()),
            viewporter,
            text_input,
        })
    }
}

/// A `wl_surface` with its wgpu target and scale objects.
struct Surf {
    // Field order is drop order: the wgpu surface goes before the wl_surface's role object.
    target: Option<SurfaceTarget>,
    wgpu_surface: Option<wgpu::Surface<'static>>,
    wl: wl_surface::WlSurface,
    viewport: Option<WpViewport>,
    _fractional: Option<WpFractionalScaleV1>,
    /// Logical size.
    size: (u32, u32),
    /// Integer buffer scale, used without fractional scale.
    int_scale: i32,
}

impl Surf {
    fn new(
        g: &Globals,
        qh: &QueueHandle<State>,
        wl: wl_surface::WlSurface,
        wgpu_surface: wgpu::Surface<'static>,
        scaled: bool,
    ) -> Self {
        let viewport = g
            .viewporter
            .as_ref()
            .map(|v| v.get_viewport(&wl, qh, NoEvents));
        let fractional = if scaled {
            g.fractional
                .as_ref()
                .map(|f| f.get_fractional_scale(&wl, qh, FractionalData(wl.clone())))
        } else {
            None
        };
        Self {
            target: None,
            wgpu_surface: Some(wgpu_surface),
            wl,
            viewport,
            _fractional: fractional,
            size: (1, 1),
            int_scale: 1,
        }
    }

    fn physical(&self, scale: f64) -> (u32, u32) {
        let px = |l: u32| ((f64::from(l) * scale).round() as u32).max(1);
        (px(self.size.0), px(self.size.1))
    }

    /// Applies size and scale: viewport destination with fractional scale, otherwise an
    /// integer buffer scale.
    fn apply(&mut self, gpu: &Gpu, scale: f64, fractional: bool) {
        let (w, h) = self.physical(scale);
        match (&self.viewport, fractional) {
            (Some(viewport), true) => {
                self.wl.set_buffer_scale(1);
                viewport.set_destination(self.size.0 as i32, self.size.1 as i32);
            }
            _ => self.wl.set_buffer_scale(self.int_scale),
        }
        if let Some(target) = &mut self.target {
            target.resize(gpu, w, h);
        } else if let Some(surface) = self.wgpu_surface.take() {
            match SurfaceTarget::new(gpu, surface, w, h) {
                Ok(target) => self.target = Some(target),
                Err(err) => tracing::error!(%err, "can't configure the wgpu surface"),
            }
        }
    }
}

enum Shell {
    Toplevel(Window),
    Layer(LayerSurface),
}

/// A Masonry layer shown as an xdg-popup.
struct PopupLayer {
    surf: Surf,
    /// Owns the xdg_popup: dropping it destroys the popup.
    _popup: Popup,
    root_id: WidgetId,
    /// Popup top-left in window (logical) coordinates, as configured by the compositor.
    origin: Point,
    /// Masonry's layer position minus its bounding-box origin.
    layer_offset: Vec2,
    configured: bool,
}

struct Win {
    surf: Surf,
    popups: Vec<PopupLayer>,
    shell: Shell,
    root: RenderRoot,
    signals: Receiver<RenderRootSignal>,
    base_root: WidgetId,
    /// Layers that couldn't become popups; painted into the window.
    inline_layers: Vec<WidgetId>,
    background: Color,
    default_size: (u32, u32),
    scale: f64,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    last_anim: Option<Instant>,
    ime_allowed: bool,
    ime_area: Rect,
}

struct State {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    g: Globals,
    #[allow(dead_code, reason = "kept alive for the raw display handle wgpu holds")]
    conn: Connection,
    qh: QueueHandle<State>,
    gpu: Gpu,
    windows: Vec<Win>,
    options: Options,
    on_action: ActionHandler,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    text_input: Option<TextInput>,
    /// Window whose surface has text-input focus.
    ti_focus: Option<usize>,
    kb_focus: Option<usize>,
    /// Window whose surface lost keyboard focus; reported to Masonry after the event batch
    /// unless one of its surfaces gets focus back.
    kb_pending_leave: Option<usize>,
    modifiers: Modifiers,
    buttons: PointerButtons,
    /// Latest input serial, for popup grabs and window moves.
    last_serial: Option<u32>,
    exit: bool,
}

/// text-input-v3 object plus the state between `done` events.
struct TextInput {
    ti: ZwpTextInputV3,
    enabled: bool,
    pending_preedit: Option<(String, Option<(usize, usize)>)>,
    pending_commit: Option<String>,
}

impl State {
    fn add_window(
        &mut self,
        spec: SurfaceSpec,
        wl: wl_surface::WlSurface,
        wgpu_surface: wgpu::Surface<'static>,
    ) {
        let qh = self.qh.clone();
        let surf = Surf::new(&self.g, &qh, wl.clone(), wgpu_surface, true);
        let (shell, default_size) = match spec.role {
            SurfaceRole::Toplevel {
                title,
                app_id,
                width,
                height,
            } => {
                let window = self
                    .g
                    .xdg
                    .create_window(wl, WindowDecorations::ServerDefault, &qh);
                window.set_title(title);
                window.set_app_id(app_id);
                window.commit();
                (Shell::Toplevel(window), (width, height))
            }
            SurfaceRole::Layer {
                namespace,
                width,
                height,
                margin_top,
            } => {
                let Some(layer_shell) = &self.g.layer_shell else {
                    tracing::error!("no layer-shell; skipping surface {namespace}");
                    return;
                };
                let layer =
                    layer_shell.create_layer_surface(&qh, wl, Layer::Top, Some(namespace), None);
                layer.set_anchor(Anchor::TOP);
                layer.set_size(width, height);
                layer.set_margin(margin_top, 0, 0, 0);
                layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
                layer.commit();
                (Shell::Layer(layer), (width, height))
            }
        };
        let (tx, rx) = mpsc::channel();
        let root = RenderRoot::new(
            spec.root,
            move |signal| {
                // The receiver lives as long as the window; a send error means it's gone.
                let _ = tx.send(signal);
            },
            RenderRootOptions {
                default_properties: Arc::clone(&self.options.default_properties),
                use_system_fonts: false,
                size_policy: WindowSizePolicy::User,
                size: PhysicalSize::new(default_size.0, default_size.1),
                scale_factor: 1.0,
                test_font: self.options.font.clone(),
            },
        );
        let base_root = root.get_layer_root(0).id();
        self.windows.push(Win {
            surf,
            popups: Vec::new(),
            shell,
            root,
            signals: rx,
            base_root,
            inline_layers: Vec::new(),
            background: spec.background,
            default_size,
            scale: 1.0,
            configured: false,
            dirty: false,
            frame_pending: false,
            last_anim: None,
            ime_allowed: false,
            ime_area: Rect::ZERO,
        });
    }

    /// Finds the window (and popup) that owns `surface`.
    fn locate(&self, surface: &wl_surface::WlSurface) -> Option<(usize, Option<usize>)> {
        self.windows.iter().enumerate().find_map(|(wi, w)| {
            if &w.surf.wl == surface {
                return Some((wi, None));
            }
            w.popups
                .iter()
                .position(|p| &p.surf.wl == surface)
                .map(|pi| (wi, Some(pi)))
        })
    }

    fn configure_window(&mut self, wi: usize, size: (u32, u32)) {
        let fractional = self.g.fractional.is_some();
        let w = &mut self.windows[wi];
        w.surf.size = size;
        w.surf.apply(&self.gpu, w.scale, fractional);
        let (pw, ph) = w.surf.physical(w.scale);
        w.root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(pw, ph)));
        w.configured = true;
        w.dirty = true;
    }

    fn set_scale(&mut self, wi: usize, scale: f64) {
        let fractional = self.g.fractional.is_some();
        let w = &mut self.windows[wi];
        if (w.scale - scale).abs() < f64::EPSILON {
            return;
        }
        tracing::info!(window = wi, scale, "scale changed");
        w.scale = scale;
        w.root.handle_window_event(WindowEvent::Rescale(scale));
        if w.configured {
            w.surf.apply(&self.gpu, scale, fractional);
            let (pw, ph) = w.surf.physical(scale);
            w.root
                .handle_window_event(WindowEvent::Resize(PhysicalSize::new(pw, ph)));
            for p in &mut w.popups {
                if p.configured {
                    p.surf.int_scale = w.surf.int_scale;
                    p.surf.apply(&self.gpu, scale, fractional);
                }
            }
        }
        w.dirty = true;
    }

    // --- Signals -----------------------------------------------------------------------

    fn process_signals(&mut self) {
        loop {
            let mut any = false;
            for wi in 0..self.windows.len() {
                let signals: Vec<_> = self.windows[wi].signals.try_iter().collect();
                for signal in signals {
                    any = true;
                    self.handle_signal(wi, signal);
                }
            }
            if !any {
                break;
            }
        }
    }

    fn handle_signal(&mut self, wi: usize, signal: RenderRootSignal) {
        match signal {
            RenderRootSignal::Action(action, id) => (self.on_action)(wi, id, action),
            RenderRootSignal::RequestRedraw | RenderRootSignal::RequestAnimFrame => {
                self.windows[wi].dirty = true;
            }
            RenderRootSignal::StartIme => {
                self.windows[wi].ime_allowed = true;
                self.sync_text_input();
            }
            RenderRootSignal::EndIme => {
                self.windows[wi].ime_allowed = false;
                self.sync_text_input();
            }
            RenderRootSignal::ImeMoved(pos, size) => {
                self.windows[wi].ime_area =
                    Rect::from_origin_size((pos.x, pos.y), (size.width, size.height));
                self.sync_text_input();
            }
            RenderRootSignal::SetTitle(title) => {
                if let Shell::Toplevel(window) = &self.windows[wi].shell {
                    window.set_title(title);
                }
            }
            RenderRootSignal::DragWindow => {
                if let (Shell::Toplevel(window), Some(seat), Some(serial)) =
                    (&self.windows[wi].shell, &self.seat, self.last_serial)
                {
                    window.move_(seat, serial);
                }
            }
            RenderRootSignal::Exit => self.exit = true,
            RenderRootSignal::NewLayer(kind, widget, pos) => {
                tracing::debug!(?kind, ?pos, "new Masonry layer");
                self.open_popup(wi, widget, pos);
            }
            RenderRootSignal::RemoveLayer(id) => {
                let w = &mut self.windows[wi];
                w.root.remove_layer(id);
                w.popups.retain(|p| p.root_id != id);
                w.inline_layers.retain(|l| *l != id);
                w.dirty = true;
            }
            RenderRootSignal::RepositionLayer(id, pos) => {
                // Spike: Masonry moves the layer; the popup keeps its first placement.
                // Task 2.6 maps this to xdg_popup.reposition.
                self.windows[wi].root.reposition_layer(id, pos);
            }
            other => tracing::trace!(?other, "unhandled signal"),
        }
    }

    /// Adds `widget` as a Masonry layer and shows it in an xdg-popup at its bounding box.
    fn open_popup(&mut self, wi: usize, widget: NewWidget<dyn Widget>, pos: Point) {
        let id = widget.id();
        let qh = self.qh.clone();
        let w = &mut self.windows[wi];
        w.root.add_layer(widget, pos);
        let bbox = w
            .root
            .get_widget(id)
            .map(|r| r.ctx().bounding_box())
            .unwrap_or_else(|| Rect::from_origin_size(pos, (1.0, 1.0)))
            .expand();
        let width = (bbox.width() as u32).max(1);
        let height = (bbox.height() as u32).max(1);

        let positioner = match XdgPositioner::new(&self.g.xdg) {
            Ok(p) => p,
            Err(err) => {
                tracing::error!(%err, "no xdg_positioner; painting the layer inline");
                w.inline_layers.push(id);
                return;
            }
        };
        positioner.set_size(width as i32, height as i32);
        positioner.set_anchor_rect(bbox.x0 as i32, bbox.y0 as i32, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        positioner.set_constraint_adjustment(
            xdg_positioner::ConstraintAdjustment::FlipY
                | xdg_positioner::ConstraintAdjustment::SlideX
                | xdg_positioner::ConstraintAdjustment::SlideY,
        );

        let wl = self.g.compositor.create_surface(&qh);
        let wgpu_surface = match handle::create_surface(&self.gpu.instance, &self.conn, &wl) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(%err, "no wgpu surface for the popup; painting the layer inline");
                w.inline_layers.push(id);
                return;
            }
        };
        let parent = match &w.shell {
            Shell::Toplevel(window) => Some(window.xdg_surface()),
            Shell::Layer(_) => None,
        };
        let popup = match Popup::from_surface(parent, &positioner, &qh, wl.clone(), &self.g.xdg) {
            Ok(p) => p,
            Err(err) => {
                tracing::error!(%err, "can't create an xdg-popup; painting the layer inline");
                w.inline_layers.push(id);
                return;
            }
        };
        if let Shell::Layer(layer) = &w.shell {
            layer.get_popup(popup.xdg_popup());
        }
        // Grab before the initial commit, so the compositor dismisses the popup on an
        // outside click (xdg_popup.grab must precede mapping).
        if let (Some(seat), Some(serial)) = (&self.seat, self.last_serial) {
            popup.xdg_popup().grab(seat, serial);
        }
        let mut surf = Surf::new(&self.g, &qh, wl, wgpu_surface, false);
        surf.size = (width, height);
        surf.int_scale = w.surf.int_scale;
        popup.wl_surface().commit();
        w.popups.push(PopupLayer {
            surf,
            _popup: popup,
            root_id: id,
            origin: bbox.origin(),
            layer_offset: pos - bbox.origin(),
            configured: false,
        });
    }

    // --- text-input-v3 ------------------------------------------------------------------

    /// Enables or disables text input for the focused window to match Masonry's IME state.
    fn sync_text_input(&mut self) {
        let Some(ti) = &mut self.text_input else {
            return;
        };
        let want = self.ti_focus.filter(|&wi| self.windows[wi].ime_allowed);
        match (want, ti.enabled) {
            (Some(wi), _) => {
                let w = &mut self.windows[wi];
                if !ti.enabled {
                    ti.ti.enable();
                    ti.ti.set_content_type(
                        zwp_text_input_v3::ContentHint::None,
                        zwp_text_input_v3::ContentPurpose::Normal,
                    );
                    ti.enabled = true;
                    w.root.handle_text_event(TextEvent::Ime(Ime::Enabled));
                }
                let a = w.ime_area;
                ti.ti.set_cursor_rectangle(
                    a.x0 as i32,
                    a.y0 as i32,
                    (a.width() as i32).max(1),
                    (a.height() as i32).max(1),
                );
                ti.ti.commit();
            }
            (None, true) => {
                ti.ti.disable();
                ti.ti.commit();
                ti.enabled = false;
            }
            (None, false) => {}
        }
    }

    fn text_input_done(&mut self) {
        let Some(ti) = &mut self.text_input else {
            return;
        };
        let commit = ti.pending_commit.take();
        let preedit = ti.pending_preedit.take();
        let Some(wi) = self.ti_focus else { return };
        let root = &mut self.windows[wi].root;
        if let Some(text) = commit {
            tracing::debug!(%text, "text-input-v3 commit");
            root.handle_text_event(TextEvent::Ime(Ime::Preedit(String::new(), None)));
            root.handle_text_event(TextEvent::Ime(Ime::Commit(text)));
        }
        let (text, cursor) = preedit.unwrap_or_default();
        if !text.is_empty() {
            tracing::debug!(%text, ?cursor, "text-input-v3 preedit");
        }
        root.handle_text_event(TextEvent::Ime(Ime::Preedit(text, cursor)));
    }

    // --- Drawing ----------------------------------------------------------------------

    fn draw_dirty(&mut self) {
        for wi in 0..self.windows.len() {
            let w = &self.windows[wi];
            if w.dirty && w.configured && !w.frame_pending {
                self.draw(wi);
            }
        }
    }

    fn draw(&mut self, wi: usize) {
        let w = &mut self.windows[wi];
        let now = Instant::now();
        let elapsed = w.last_anim.map(|t| now - t).unwrap_or_default();
        w.root.handle_window_event(WindowEvent::AnimFrame(elapsed));
        w.last_anim = w.root.needs_anim().then_some(now);

        let (plan, _access) = w.root.redraw();

        // Split the plan: each layer root starts a group; sub-layers follow their root.
        let mut main: Vec<ImagingLayer<'_>> = Vec::new();
        let mut per_popup: Vec<Vec<ImagingLayer<'_>>> =
            w.popups.iter().map(|_| Vec::new()).collect();
        let mut current: Option<usize> = None;
        for layer in &plan.layers {
            if layer.widget_id == w.base_root || w.inline_layers.contains(&layer.widget_id) {
                current = None;
            } else if let Some(pi) = w.popups.iter().position(|p| p.root_id == layer.widget_id) {
                current = Some(pi);
            }
            let VisualLayerKind::Scene(scene) = &layer.kind else {
                continue;
            };
            match current {
                None => main.push(ImagingLayer {
                    scene,
                    transform: layer.transform,
                }),
                Some(pi) => per_popup[pi].push(ImagingLayer {
                    scene,
                    transform: Affine::translate(-w.popups[pi].origin.to_vec2()) * layer.transform,
                }),
            }
        }

        let empty = Scene::new();
        if let Some(target) = &w.surf.target {
            let (pw, ph) = w.surf.physical(w.scale);
            // Ask for the next frame before presenting: present commits the surface.
            w.surf
                .wl
                .frame(&self.qh, FrameCallbackData(w.surf.wl.clone()));
            w.frame_pending = true;
            let frame = PreparedFrame::new(pw, ph, w.scale, w.background, &empty, &main);
            self.gpu.render(target, frame, || {});
        }
        for (p, layers) in w.popups.iter().zip(&per_popup) {
            let (Some(target), true) = (&p.surf.target, p.configured) else {
                continue;
            };
            let (pw, ph) = p.surf.physical(w.scale);
            let frame = PreparedFrame::new(pw, ph, w.scale, Color::TRANSPARENT, &empty, layers);
            self.gpu.render(target, frame, || {});
        }
        w.dirty = false;
    }

    // --- Input ------------------------------------------------------------------------

    fn pointer_state(&self, wi: usize, pos: Point, time: u32) -> PointerState {
        let scale = self.windows[wi].scale;
        PointerState {
            time: u64::from(time) * 1_000_000,
            position: PhysicalPosition::new(pos.x * scale, pos.y * scale),
            buttons: self.buttons,
            modifiers: self.modifiers,
            count: 1,
            scale_factor: scale,
            ..PointerState::default()
        }
    }
}

// --- sctk handlers ----------------------------------------------------------------------

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        // Only without fractional scale; otherwise wp_fractional_scale_v1 decides.
        if self.g.fractional.is_some() {
            return;
        }
        if let Some((wi, None)) = self.locate(surface) {
            self.windows[wi].surf.int_scale = factor;
            self.set_scale(wi, f64::from(factor));
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if let Some((wi, None)) = self.locate(surface) {
            let w = &mut self.windows[wi];
            w.frame_pending = false;
            if w.root.needs_anim() {
                w.dirty = true;
            }
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for State {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let Some((wi, None)) = self.locate(window.wl_surface()) else {
            return;
        };
        let (dw, dh) = self.windows[wi].default_size;
        let size = (
            configure.new_size.0.map_or(dw, |v| v.get()),
            configure.new_size.1.map_or(dh, |v| v.get()),
        );
        self.configure_window(wi, size);
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        tracing::info!("layer surface closed by the compositor");
        if let Some((wi, None)) = self.locate(layer.wl_surface()) {
            self.windows[wi].configured = false;
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some((wi, None)) = self.locate(layer.wl_surface()) else {
            return;
        };
        let (dw, dh) = self.windows[wi].default_size;
        let pick = |v: u32, d: u32| if v == 0 { d } else { v };
        let size = (
            pick(configure.new_size.0, dw),
            pick(configure.new_size.1, dh),
        );
        self.configure_window(wi, size);
    }
}

impl PopupHandler for State {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        let Some((wi, Some(pi))) = self.locate(popup.wl_surface()) else {
            return;
        };
        let fractional = self.g.fractional.is_some();
        let w = &mut self.windows[wi];
        let p = &mut w.popups[pi];
        let origin = Point::new(f64::from(config.position.0), f64::from(config.position.1));
        tracing::debug!(
            ?origin,
            width = config.width,
            height = config.height,
            "popup configured"
        );
        if origin != p.origin {
            // The compositor flipped or slid the popup: move the Masonry layer to match.
            p.origin = origin;
            w.root.reposition_layer(p.root_id, origin + p.layer_offset);
        }
        let p = &mut w.popups[pi];
        p.surf.size = (config.width.max(1) as u32, config.height.max(1) as u32);
        p.surf.apply(&self.gpu, w.scale, fractional);
        p.configured = true;
        w.dirty = true;
        // A popup's first frame must not wait for the parent's frame callback.
        w.frame_pending = false;
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        let Some((wi, Some(pi))) = self.locate(popup.wl_surface()) else {
            return;
        };
        let id = self.windows[wi].popups[pi].root_id;
        tracing::debug!("popup dismissed by the compositor");
        // Masonry's menu layers close themselves on a pointer cancel.
        self.windows[wi]
            .root
            .handle_pointer_event(MasonryPointer::Cancel(input::MOUSE));
        self.process_signals();
        let w = &mut self.windows[wi];
        if w.popups.iter().any(|p| p.root_id == id) {
            w.root.remove_layer(id);
            w.popups.retain(|p| p.root_id != id);
            w.dirty = true;
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.adopt_seat(seat);
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard if self.keyboard.is_none() => {
                match self.seats.get_keyboard(qh, &seat, None) {
                    Ok(k) => self.keyboard = Some(k),
                    Err(err) => tracing::error!(%err, "no keyboard"),
                }
            }
            Capability::Pointer if self.pointer.is_none() => {
                match self.seats.get_pointer(qh, &seat) {
                    Ok(p) => self.pointer = Some(p),
                    Err(err) => tracing::error!(%err, "no pointer"),
                }
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Keyboard => {
                if let Some(k) = self.keyboard.take() {
                    k.release();
                }
            }
            Capability::Pointer => {
                if let Some(p) = self.pointer.take() {
                    p.release();
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        serial: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.last_serial = Some(serial);
        let Some((wi, _)) = self.locate(surface) else {
            return;
        };
        self.kb_focus = Some(wi);
        // Focus moving between a window and its own popups isn't a window focus change:
        // Masonry's menus close when their window loses focus.
        if self.kb_pending_leave.take() != Some(wi) {
            self.flush_focus_leave();
            self.windows[wi]
                .root
                .handle_text_event(TextEvent::WindowFocusChange(true));
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        // Deferred: a matching `enter` on the same window may follow in this batch.
        if let Some((wi, _)) = self.locate(surface) {
            self.flush_focus_leave();
            self.kb_pending_leave = Some(wi);
            if self.kb_focus == Some(wi) {
                self.kb_focus = None;
            }
        }
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.last_serial = Some(serial);
        self.key(&event, KeyState::Down);
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, KeyState::Down);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event, KeyState::Up);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.modifiers = input::modifiers(&modifiers);
    }
}

impl State {
    /// Reports a deferred keyboard leave to Masonry.
    fn flush_focus_leave(&mut self) {
        if let Some(wi) = self.kb_pending_leave.take() {
            self.windows[wi]
                .root
                .handle_text_event(TextEvent::WindowFocusChange(false));
        }
    }

    /// Uses the first seat: creates its text-input object. Pointer and keyboard come from
    /// `new_capability`.
    fn adopt_seat(&mut self, seat: wl_seat::WlSeat) {
        if self.seat.is_some() {
            return;
        }
        if let Some(mgr) = &self.g.text_input {
            self.text_input = Some(TextInput {
                ti: mgr.get_text_input(&seat, &self.qh, TextInputData),
                enabled: false,
                pending_preedit: None,
                pending_commit: None,
            });
        }
        self.seat = Some(seat);
    }

    fn key(&mut self, event: &KeyEvent, state: KeyState) {
        let Some(wi) = self.kb_focus else { return };
        let ev = input::key_event(event, state, self.modifiers);
        self.windows[wi]
            .root
            .handle_text_event(TextEvent::Keyboard(ev));
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some((wi, popup)) = self.locate(&event.surface) else {
                continue;
            };
            let offset = popup.map_or(Vec2::ZERO, |pi| {
                self.windows[wi].popups[pi].origin.to_vec2()
            });
            let pos = Point::new(event.position.0, event.position.1) + offset;
            let masonry = match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.last_serial = Some(serial);
                    MasonryPointer::Enter(input::MOUSE)
                }
                PointerEventKind::Leave { .. } => MasonryPointer::Leave(input::MOUSE),
                PointerEventKind::Motion { time } => MasonryPointer::Move(PointerUpdate {
                    pointer: input::MOUSE,
                    current: self.pointer_state(wi, pos, time),
                    coalesced: Vec::new(),
                    predicted: Vec::new(),
                }),
                PointerEventKind::Press {
                    time,
                    button,
                    serial,
                } => {
                    self.last_serial = Some(serial);
                    let button = input::button(button);
                    if let Some(b) = button {
                        self.buttons.insert(b);
                    }
                    MasonryPointer::Down(PointerButtonEvent {
                        button,
                        pointer: input::MOUSE,
                        state: self.pointer_state(wi, pos, time),
                    })
                }
                PointerEventKind::Release {
                    time,
                    button,
                    serial,
                } => {
                    self.last_serial = Some(serial);
                    let button = input::button(button);
                    if let Some(b) = button {
                        self.buttons.remove(b);
                    }
                    MasonryPointer::Up(PointerButtonEvent {
                        button,
                        pointer: input::MOUSE,
                        state: self.pointer_state(wi, pos, time),
                    })
                }
                PointerEventKind::Axis {
                    time,
                    horizontal,
                    vertical,
                    ..
                } => {
                    let scale = self.windows[wi].scale;
                    MasonryPointer::Scroll(PointerScrollEvent {
                        pointer: input::MOUSE,
                        delta: ScrollDelta::PixelDelta(PhysicalPosition::new(
                            -horizontal.absolute * scale,
                            -vertical.absolute * scale,
                        )),
                        state: self.pointer_state(wi, pos, time),
                    })
                }
            };
            self.windows[wi].root.handle_pointer_event(masonry);
        }
    }
}

delegate_registry!(State);

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_dispatch2!(State);

// --- Protocols sctk doesn't wrap ----------------------------------------------------------

/// User data for objects that send no events we use.
struct NoEvents;

impl<I: Proxy> Dispatch2<I, State> for NoEvents {
    fn event(&self, _: &mut State, _: &I, _: I::Event, _: &Connection, _: &QueueHandle<State>) {}
}

/// User data for a surface's `wp_fractional_scale_v1`.
struct FractionalData(wl_surface::WlSurface);

impl Dispatch2<WpFractionalScaleV1, State> for FractionalData {
    fn event(
        &self,
        state: &mut State,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event
            && let Some((wi, None)) = state.locate(&self.0)
        {
            // The protocol sends the scale in 120ths.
            state.set_scale(wi, f64::from(scale) / 120.0);
        }
    }
}

/// User data for the seat's `zwp_text_input_v3`.
struct TextInputData;

impl Dispatch2<ZwpTextInputV3, State> for TextInputData {
    fn event(
        &self,
        state: &mut State,
        _: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use zwp_text_input_v3::Event;
        match event {
            Event::Enter { surface } => {
                state.ti_focus = state.locate(&surface).map(|(wi, _)| wi);
                tracing::debug!(window = ?state.ti_focus, "text-input-v3 enter");
                state.sync_text_input();
            }
            Event::Leave { .. } => {
                tracing::debug!("text-input-v3 leave");
                if let Some(ti) = &mut state.text_input
                    && ti.enabled
                {
                    ti.ti.disable();
                    ti.ti.commit();
                    ti.enabled = false;
                }
                state.ti_focus = None;
            }
            Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                if let Some(ti) = &mut state.text_input {
                    let cursor = (cursor_begin >= 0 && cursor_end >= 0)
                        .then_some((cursor_begin as usize, cursor_end as usize));
                    ti.pending_preedit = Some((text.unwrap_or_default(), cursor));
                }
            }
            Event::CommitString { text } => {
                if let Some(ti) = &mut state.text_input {
                    ti.pending_commit = text;
                }
            }
            // Masonry has no surrounding-text API yet (task 2.14).
            Event::DeleteSurroundingText { .. } => {}
            Event::Done { .. } => state.text_input_done(),
            _ => {}
        }
    }
}
