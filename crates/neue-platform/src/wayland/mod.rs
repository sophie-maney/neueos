//! smithay-client-toolkit driver for Masonry's `RenderRoot` (ADR 0002 §4, ADR 0033).
//!
//! Runs an [`App`] on Wayland, with the same API as the winit driver (ADR 0032):
//! - xdg-toplevels (server-side title bars) and wlr-layer-shell surfaces, opened and
//!   closed at run time through [`Ctx`];
//! - fractional scale with viewporter, or integer buffer scale without them;
//! - drawing paced by frame callbacks, per surface;
//! - cursor shapes (`wp_cursor_shape_v1`, else the xcursor theme);
//! - keyboard with repeat and a full key table, pointer with click counting;
//! - text-input-v3 into Masonry's IME events;
//! - Masonry layers (menus, tooltips) as xdg-popups: still laid out and hit-tested by
//!   Masonry in window coordinates, but painted into the popup's own `wl_surface`, with
//!   pointer input on the popup translated back into window coordinates. Placement per
//!   layer kind, so the compositor flips them at screen edges; `xdg_popup.reposition`;
//!   submenus nest on the menu they open from;
//! - the text clipboard and pasting the primary selection;
//! - xdg-activation (Masonry's `TakeFocus`, `XDG_ACTIVATION_TOKEN` at start-up),
//!   parent windows and modal dialogs (xdg-dialog);
//! - AccessKit through `accesskit_unix` (AT-SPI);
//! - the menu-bar address for the global menu (`org_kde_kwin_appmenu`, ADR 0047);
//! - where a window's own chrome ends, for sheets (`neue_window_v1`, ADR 0064).

mod access;
mod clipboard;
mod desktop;
mod handle;
mod input;
mod keymap;
mod popup;
mod seat;
mod shell;

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use masonry_core::app::{
    RenderRoot, RenderRootOptions, RenderRootSignal, VisualLayerKind, WindowSizePolicy,
};
use masonry_core::core::{
    CursorIcon, ErasedAction, Ime, LayerType, NewWidget, TextEvent, Widget, WidgetId,
};
use masonry_core::core::{ResizeDirection, WindowEvent};
use masonry_core::dpi::PhysicalSize;
use masonry_core::imaging::record::Scene;
use masonry_core::kurbo::{Affine, Point, Rect, Vec2};
use masonry_core::peniko::Color;
use masonry_imaging::{Layer as ImagingLayer, PreparedFrame};
use neue_protocols::neue_window::client::{
    neue_window_manager_v1::NeueWindowManagerV1, neue_window_v1::NeueWindowV1,
};
use smithay_client_toolkit::activation::{ActivationHandler, ActivationState, RequestData};
use smithay_client_toolkit::compositor::{CompositorState, FrameCallbackData, Region};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::output::OutputState;
use smithay_client_toolkit::reexports::calloop::channel::{self as calloop_channel, Sender};
use smithay_client_toolkit::reexports::calloop::ping as calloop_ping;
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::SeatState;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{self, LayerShell, LayerSurface};
use smithay_client_toolkit::shell::xdg::popup::Popup;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowDecorations};
use smithay_client_toolkit::shell::xdg::{XdgShell, XdgSurface as _};
use smithay_client_toolkit::shm::Shm;
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::{GlobalList, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_surface};
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
use wayland_protocols::xdg::dialog::v1::client::{
    xdg_dialog_v1::XdgDialogV1, xdg_wm_dialog_v1::XdgWmDialogV1,
};
use wayland_protocols::xdg::shell::client::xdg_toplevel;
use wayland_protocols_plasma::appmenu::client::{
    org_kde_kwin_appmenu::OrgKdeKwinAppmenu, org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager,
};

use crate::app::{DriverWindow, Registry};
use crate::gpu::{Gpu, GpuError, SurfaceTarget};
use crate::{
    Anchor, App, ChromeHeight, Ctx, Decorations, LayerKeyboard, LayerSpec, MenuAddress, Options,
    PopoverAnchor, ShellLayer, Waker, WindowId, WindowKind, WindowSpec,
};
use access::{Access, AccessEvent};
use clipboard::Clipboard;
use popup::Placement;

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
}

impl From<GpuError> for Error {
    fn from(err: GpuError) -> Self {
        Self::Gpu(err.0)
    }
}

/// Connects to the compositor named by the environment and runs `app` until it exits or
/// its last window closes.
pub fn run(app: &mut dyn App, options: Options) -> Result<(), Error> {
    run_with_connection(Connection::connect_to_env()?, app, options)
}

/// Like [`run`], on a connection the caller made (headless compositor tests hand the
/// driver one end of a socket pair).
pub fn run_with_connection(
    conn: Connection,
    app: &mut dyn App,
    options: Options,
) -> Result<(), Error> {
    let (globals, queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<'static, State> =
        EventLoop::try_new().map_err(|e| Error::Loop(e.to_string()))?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| Error::Loop(e.to_string()))?;

    // AccessKit's adapters call in from their own thread.
    let (access_tx, access_rx) = calloop_channel::channel();
    event_loop
        .handle()
        .insert_source(access_rx, |event, (), state: &mut State| {
            if let calloop_channel::Event::Msg((window, event)) = event {
                state.access_event(window, event);
            }
        })
        .map_err(|e| Error::Loop(e.to_string()))?;

    // `Waker::wake` pings the loop out of `dispatch`; the loop then calls `App::on_wake`.
    let (ping, ping_source) = calloop_ping::make_ping().map_err(|e| Error::Loop(e.to_string()))?;
    event_loop
        .handle()
        .insert_source(ping_source, |(), (), _: &mut State| {})
        .map_err(|e| Error::Loop(e.to_string()))?;
    let waker = Waker::new(move || ping.ping());

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
        Box::new(handle::Display(conn.clone())),
    ));
    let mut state = State {
        reg: Registry::new(waker),
        events: Vec::new(),
        gpu: None,
        instance,
        g: Globals::bind(&globals, &qh)?,
        shm: Shm::bind(&globals, &qh)?,
        clipboard: Clipboard::bind(&globals, &qh),
        access_tx,
        // Activates the first toplevel. Left in the environment: removing it is unsafe
        // with threads around, and a used token is worthless to children.
        startup_token: std::env::var("XDG_ACTIVATION_TOKEN")
            .ok()
            .filter(|t| !t.is_empty()),
        reposition_token: 0,
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        desktop: desktop::Desktop::bind(&globals, &qh),
        input: seat::Input::default(),
        loop_handle: event_loop.handle(),
        qh,
        conn: conn.clone(),
        options,
    };
    state.reg.has_shell = state.desktop.has_shell();
    // sctk binds the seats that exist at start-up without calling `new_seat` for them.
    let seats: Vec<_> = state.seats.seats().collect();
    for seat in seats {
        state.adopt_seat(seat);
    }

    app.on_start(&mut Ctx::new(&mut state.reg));
    state.pump(app)?;
    while !state.reg.done() {
        // Nothing to do without an event: animation is paced by frame callbacks.
        event_loop
            .dispatch(None, &mut state)
            .map_err(|e| Error::Loop(e.to_string()))?;
        state.flush_focus_leave();
        if state.reg.waker.take() {
            app.on_wake(&mut Ctx::new(&mut state.reg));
        }
        state.pump(app)?;
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
    activation: Option<ActivationState>,
    dialog: Option<XdgWmDialogV1>,
    appmenu: Option<OrgKdeKwinAppmenuManager>,
    window: Option<NeueWindowManagerV1>,
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
        let activation = ActivationState::bind(globals, qh).ok();
        let dialog = globals.bind(qh, 1..=1, NoEvents).ok();
        let appmenu = globals.bind(qh, 1..=2, NoEvents).ok();
        let window = globals.bind(qh, 1..=1, NoEvents).ok();
        optional("zwlr_layer_shell_v1", layer_shell.is_some());
        optional("wp_fractional_scale_manager_v1", fractional.is_some());
        optional("wp_viewporter", viewporter.is_some());
        optional("zwp_text_input_manager_v3", text_input.is_some());
        optional("xdg_activation_v1", activation.is_some());
        optional("xdg_wm_dialog_v1", dialog.is_some());
        optional("org_kde_kwin_appmenu_manager", appmenu.is_some());
        optional("neue_window_manager_v1", window.is_some());
        Ok(Self {
            compositor: CompositorState::bind(globals, qh)?,
            xdg: XdgShell::bind(globals, qh)?,
            layer_shell,
            // Fractional scale needs a viewport to set the logical size.
            fractional: fractional.filter(|_| viewporter.is_some()),
            viewporter,
            text_input,
            activation,
            dialog,
            appmenu,
            window,
        })
    }
}

/// A `wl_surface` with its wgpu target, scale objects and frame-callback state.
struct Surf {
    // Field order is drop order: the wgpu surface goes before the wl_surface's role object
    // (owned by the window or popup holding this).
    target: Option<SurfaceTarget>,
    wgpu_surface: Option<wgpu::Surface<'static>>,
    wl: wl_surface::WlSurface,
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    /// Logical size.
    size: (u32, u32),
    /// Integer buffer scale, used without fractional scale.
    int_scale: i32,
    /// A frame callback was requested and hasn't fired: presenting now could block.
    frame_pending: bool,
    /// The surface missed a draw while its frame callback was pending.
    stale: bool,
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
            fractional,
            size: (1, 1),
            int_scale: 1,
            frame_pending: false,
            stale: false,
        }
    }

    fn physical(&self, scale: f64) -> (u32, u32) {
        physical(self.size, scale)
    }

    /// Whether a frame can be presented now.
    fn ready(&self) -> bool {
        self.target.is_some() && !self.frame_pending
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

    /// Presents `frame`, with a frame callback in the same commit. Never blocks: the
    /// caller checks [`Surf::ready`] first, so the GL or Vulkan swapchain's own wait for
    /// the previous frame is already over.
    fn present(&mut self, gpu: &mut Gpu, qh: &QueueHandle<State>, frame: PreparedFrame<'_>) {
        let Some(target) = &mut self.target else {
            return;
        };
        self.wl.frame(qh, FrameCallbackData(self.wl.clone()));
        self.frame_pending = true;
        self.stale = false;
        if !gpu.render(target, frame, || {}) {
            // Nothing was committed; commit the callback request on its own, or it would
            // wait for a commit that never comes.
            self.wl.commit();
        }
    }
}

impl Drop for Surf {
    fn drop(&mut self) {
        if let Some(viewport) = &self.viewport {
            viewport.destroy();
        }
        if let Some(fractional) = &self.fractional {
            fractional.destroy();
        }
    }
}

/// Physical pixels for a logical size at `scale`, at least 1×1.
fn physical(size: (u32, u32), scale: f64) -> (u32, u32) {
    let px = |l: u32| ((f64::from(l) * scale).round() as u32).max(1);
    (px(size.0), px(size.1))
}

enum Shell {
    Toplevel(Window),
    Layer(LayerSurface),
}

/// A Masonry layer shown as an xdg-popup.
struct PopupLayer {
    // Field order is drop order: the surface's wgpu target before the popup's wl_surface.
    surf: Surf,
    /// Owns the xdg_popup and its wl_surface: dropping it destroys both.
    popup: Popup,
    root_id: WidgetId,
    /// The menu this one opened from (its xdg parent), or `None` for the window.
    parent: Option<WidgetId>,
    placement: Placement,
    /// Where Masonry asked for the layer, in window coordinates.
    requested: Point,
    /// Popup surface's top-left in window (logical) coordinates, as configured by the
    /// compositor.
    origin: Point,
    /// Masonry's layer position minus its bounding-box origin.
    layer_offset: Vec2,
    configured: bool,
}

/// One open window.
struct Win {
    // Field order is drop order: popups, then the window's wgpu target, then its role.
    popups: Vec<PopupLayer>,
    surf: Surf,
    shell: Shell,
    root: RenderRoot,
    signals: Receiver<RenderRootSignal>,
    base_root: WidgetId,
    /// Layers that couldn't become popups; painted into the window.
    inline_layers: Vec<WidgetId>,
    /// Layers whose popup the compositor dismissed; removed after Masonry had its say.
    dismissed: Vec<WidgetId>,
    background: Color,
    /// Logical size used when the compositor leaves the size to us: the spec's, then the
    /// last floating size.
    free_size: (u32, u32),
    /// The last configure fixed the size (maximised, tiled, or an interactive resize).
    server_sized: bool,
    maximized: bool,
    scale: f64,
    configured: bool,
    dirty: bool,
    last_anim: Option<Instant>,
    ime_allowed: bool,
    ime_area: Rect,
    cursor: CursorIcon,
    accesskit: Access,
    /// The xdg-dialog object of a modal toplevel.
    dialog: Option<XdgDialogV1>,
    /// Where the menu bar is exported, once the app has set an address.
    appmenu: Option<OrgKdeKwinAppmenu>,
    /// Where the window's own chrome ends (toplevels with chrome only), the height last
    /// sent, and the object it went through.
    chrome_height: Option<ChromeHeight>,
    chrome_sent: Option<u32>,
    neue_window: Option<NeueWindowV1>,
}

impl Win {
    /// Removes the popup at `pi` and the popups nested on it, newest first, and returns
    /// their layers.
    fn take_popups(&mut self, pi: usize) -> Vec<WidgetId> {
        let mut gone = vec![self.popups[pi].root_id];
        for p in &self.popups[pi + 1..] {
            if p.parent.is_some_and(|parent| gone.contains(&parent)) {
                gone.push(p.root_id);
            }
        }
        // xdg-shell forbids destroying a popup under another one.
        for i in (pi..self.popups.len()).rev() {
            if gone.contains(&self.popups[i].root_id) {
                self.popups.remove(i);
            }
        }
        gone
    }

    /// Tells the compositor where the window's chrome ends if that changed since it was last
    /// told; it applies with the next commit of the window's surface, so in step with the
    /// frame that draws the chrome at that height.
    fn sync_chrome(&mut self, manager: Option<&NeueWindowManagerV1>, qh: &QueueHandle<State>) {
        let (Some(chrome), Some(manager), Shell::Toplevel(window)) =
            (&self.chrome_height, manager, &self.shell)
        else {
            return;
        };
        let height = chrome.get();
        if self.chrome_sent == Some(height) {
            return;
        }
        self.neue_window
            .get_or_insert_with(|| manager.get_window(window.wl_surface(), qh, NoEvents))
            .set_chrome_height(height);
        self.chrome_sent = Some(height);
    }

    /// Window-coordinate origin of the surface a popup is placed against.
    fn parent_origin(&self, parent: Option<WidgetId>) -> Vec2 {
        parent
            .and_then(|parent| self.popups.iter().find(|p| p.root_id == parent))
            .map_or(Vec2::ZERO, |p| p.origin.to_vec2() + p.geometry())
    }
}

impl PopupLayer {
    /// Where the popup's window geometry starts within its surface (a popover's shadow
    /// lies outside it): what the compositor positions, and what popups nested on this one
    /// are placed relative to.
    fn geometry(&self) -> Vec2 {
        let i = self.placement.insets;
        Vec2::new(f64::from(i.left), f64::from(i.top))
    }
}

impl Drop for Win {
    fn drop(&mut self) {
        // Newest popup first: xdg-shell forbids destroying a popup under another one.
        while self.popups.pop().is_some() {}
        if let Some(dialog) = self.dialog.take() {
            dialog.destroy();
        }
        if let Some(appmenu) = self.appmenu.take() {
            release_appmenu(appmenu);
        }
        if let Some(window) = self.neue_window.take() {
            window.destroy();
        }
    }
}

/// Destroys an appmenu object, which clears the address. Version 1 has no destructor; the
/// object then lives until its surface does.
fn release_appmenu(appmenu: OrgKdeKwinAppmenu) {
    if appmenu.version() >= 2 {
        appmenu.release();
    }
}

impl DriverWindow for Win {
    fn render_root(&mut self) -> &mut RenderRoot {
        &mut self.root
    }
}

/// Something for the app to hear about once the event batch is handled.
enum AppEvent {
    Action(WindowId, WidgetId, ErasedAction),
    CloseRequested(WindowId),
    Output(crate::desktop::OutputEvent),
    Shell(crate::desktop::ShellEvent),
}

struct State {
    // Field order is drop order: windows (and their wgpu surfaces) before the device.
    reg: Registry<Win>,
    events: Vec<AppEvent>,
    gpu: Option<Gpu>,
    instance: wgpu::Instance,
    g: Globals,
    shm: Shm,
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    /// Outputs as the app knows them, and `neue_shell_v1`.
    desktop: desktop::Desktop,
    input: seat::Input,
    clipboard: Clipboard,
    access_tx: Sender<(WindowId, AccessEvent)>,
    /// `XDG_ACTIVATION_TOKEN` from our launcher, until the first toplevel uses it.
    startup_token: Option<String>,
    /// Serial for `xdg_popup.reposition`.
    reposition_token: u32,
    loop_handle: LoopHandle<'static, State>,
    qh: QueueHandle<State>,
    /// Kept alive for the raw display handle wgpu holds.
    conn: Connection,
    options: Options,
}

impl State {
    /// Opens queued windows, handles every pending `RenderRoot` signal and tells the app
    /// about actions and close requests, until none of these is left (an action can open
    /// windows, and new windows emit signals).
    fn pump(&mut self, app: &mut dyn App) -> Result<(), Error> {
        loop {
            let mut progress = false;
            for (id, spec) in std::mem::take(&mut self.reg.opening) {
                progress = true;
                self.create_window(id, spec)?;
            }
            for (id, address) in std::mem::take(&mut self.reg.menu_addresses) {
                self.set_menu_address(id, address);
            }
            self.send_shell_requests();
            if let Some(text) = self.reg.primary_selection.take() {
                progress = true;
                self.store_primary(text);
            }
            let ids: Vec<WindowId> = self.reg.windows.keys().copied().collect();
            for id in ids {
                while let Some(signal) = self
                    .reg
                    .windows
                    .get(&id)
                    .and_then(|w| w.signals.try_recv().ok())
                {
                    progress = true;
                    self.handle_signal(id, signal);
                }
                self.remove_dismissed(id);
            }
            for event in std::mem::take(&mut self.events) {
                progress = true;
                let cx = &mut Ctx::new(&mut self.reg);
                match event {
                    AppEvent::Action(id, widget, action) => {
                        tracing::trace!(%id, %widget, action = action.type_name(), "action");
                        app.on_action(cx, id, widget, action);
                    }
                    AppEvent::CloseRequested(id) => app.on_close_requested(cx, id),
                    AppEvent::Output(event) => app.on_output(cx, event),
                    AppEvent::Shell(event) => app.on_shell(cx, event),
                }
            }
            if !progress {
                return Ok(());
            }
        }
    }

    fn create_window(&mut self, id: WindowId, spec: WindowSpec) -> Result<(), Error> {
        let qh = self.qh.clone();
        if matches!(spec.kind, WindowKind::Layer(_)) && self.g.layer_shell.is_none() {
            tracing::error!(%id, "the compositor has no layer shell; not opening the window");
            return Ok(());
        }
        let wl = self.g.compositor.create_surface(&qh);
        let wgpu_surface = handle::create_surface(self.instance(), &self.conn, &wl)?;
        if self.gpu.is_none() {
            // The adapter must be able to present to the first window's surface.
            self.gpu = Some(Gpu::new(self.instance.clone(), &wgpu_surface)?);
        }
        let surf = Surf::new(&self.g, &qh, wl.clone(), wgpu_surface, true);
        let size = (spec.width, spec.height);
        let mut dialog = None;
        let toplevel = !matches!(spec.kind, WindowKind::Layer(_));
        let shell = match (spec.kind, &self.g.layer_shell) {
            (WindowKind::Layer(layer), Some(layer_shell)) => {
                let output = layer.output.and_then(|o| {
                    let found = self.desktop.wl_output(o);
                    if found.is_none() {
                        tracing::warn!(%id, ?o, "layer surface's output is gone; compositor picks");
                    }
                    found
                });
                Shell::Layer(create_layer(layer_shell, &qh, wl, &layer, size, output))
            }
            _ => {
                let decorations = match spec.decorations {
                    Decorations::Server => WindowDecorations::ServerDefault,
                    // neue-comp grants an explicit client-side request (ADR 0013, ADR 0055).
                    Decorations::Client => WindowDecorations::RequestClient,
                };
                let window = self.g.xdg.create_window(wl.clone(), decorations, &qh);
                window.set_title(spec.title);
                if !spec.app_id.is_empty() {
                    window.set_app_id(spec.app_id);
                }
                let parent = spec
                    .parent
                    .and_then(|p| self.reg.windows.get(&p))
                    .and_then(|p| match &p.shell {
                        Shell::Toplevel(parent) => Some(parent),
                        Shell::Layer(_) => None,
                    });
                if let Some(parent) = parent {
                    window.set_parent(Some(parent));
                } else if spec.parent.is_some() {
                    tracing::warn!(%id, "the parent isn't an open toplevel; ignoring it");
                }
                if spec.modal {
                    match &self.g.dialog {
                        Some(wm) => {
                            let d = wm.get_xdg_dialog(window.xdg_toplevel(), &qh, NoEvents);
                            d.set_modal();
                            dialog = Some(d);
                        }
                        None => tracing::warn!(%id, "no xdg-dialog: the window isn't modal"),
                    }
                }
                // The initial commit, without a buffer, asks for the first configure.
                window.commit();
                if let (Some(token), Some(activation)) =
                    (self.startup_token.take(), &self.g.activation)
                {
                    tracing::debug!(%id, "activating with the launcher's token");
                    activation.activate::<State>(&wl, token);
                }
                Shell::Toplevel(window)
            }
        };
        let (tx, signals) = mpsc::channel();
        let mut root = RenderRoot::new(
            spec.root,
            move |signal| {
                // The receiver lives as long as the window; a send error means it's gone.
                let _ = tx.send(signal);
            },
            RenderRootOptions {
                default_properties: spec
                    .default_properties
                    .clone()
                    .unwrap_or_else(|| Arc::clone(&self.options.default_properties)),
                use_system_fonts: false,
                size_policy: WindowSizePolicy::User,
                // Until the compositor configures the window and tells us its scale.
                size: PhysicalSize::new(size.0.max(1), size.1.max(1)),
                scale_factor: 1.0,
                test_font: self.options.font.clone(),
            },
        );
        if spec.focus_fallback.is_some() && !root.set_focus_fallback(spec.focus_fallback) {
            tracing::warn!(%id, "focus fallback isn't in the window's tree");
        }
        let base_root = root.get_layer_root(0).id();
        tracing::debug!(%id, ?size, "opened window");
        self.reg.windows.insert(
            id,
            Win {
                popups: Vec::new(),
                surf,
                shell,
                root,
                signals,
                base_root,
                inline_layers: Vec::new(),
                dismissed: Vec::new(),
                background: spec.background,
                free_size: size,
                server_sized: false,
                maximized: false,
                scale: 1.0,
                configured: false,
                dirty: false,
                last_anim: None,
                ime_allowed: false,
                ime_area: Rect::ZERO,
                cursor: CursorIcon::Default,
                accesskit: Access::new(id, self.access_tx.clone()),
                dialog,
                appmenu: None,
                chrome_height: spec.chrome_height.filter(|_| toplevel),
                chrome_sent: None,
                neue_window: None,
            },
        );
        Ok(())
    }

    /// Points the compositor at `id`'s exported menu bar, or clears it. Toplevels only.
    fn set_menu_address(&mut self, id: WindowId, address: Option<MenuAddress>) {
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        let Shell::Toplevel(window) = &w.shell else {
            tracing::debug!(%id, "only toplevels have a menu bar address");
            return;
        };
        match address {
            Some(MenuAddress { service, path }) => {
                let Some(manager) = &self.g.appmenu else {
                    tracing::debug!(%id, "no org_kde_kwin_appmenu_manager: no global menu");
                    return;
                };
                let qh = &self.qh;
                w.appmenu
                    .get_or_insert_with(|| manager.create(window.wl_surface(), qh, NoEvents))
                    .set_address(service, path);
            }
            None => {
                if let Some(appmenu) = w.appmenu.take() {
                    release_appmenu(appmenu);
                }
            }
        }
    }

    /// The wgpu instance: the GPU's copy once it exists (the same instance either way).
    fn instance(&self) -> &wgpu::Instance {
        gpu_instance(&self.gpu, &self.instance)
    }

    /// Finds the window (and popup) that owns `surface`.
    fn locate(&self, surface: &wl_surface::WlSurface) -> Option<(WindowId, Option<usize>)> {
        self.reg.windows.iter().find_map(|(&id, w)| {
            if &w.surf.wl == surface {
                return Some((id, None));
            }
            w.popups
                .iter()
                .position(|p| &p.surf.wl == surface)
                .map(|pi| (id, Some(pi)))
        })
    }

    /// Applies a configured logical size to a window.
    fn configure_window(&mut self, id: WindowId, size: (u32, u32)) {
        let fractional = self.g.fractional.is_some();
        let (Some(gpu), Some(w)) = (&self.gpu, self.reg.windows.get_mut(&id)) else {
            return;
        };
        w.surf.size = size;
        w.surf.apply(gpu, w.scale, fractional);
        let (pw, ph) = w.surf.physical(w.scale);
        w.root
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(pw, ph)));
        w.accesskit.set_size((pw, ph));
        w.configured = true;
        w.dirty = true;
    }

    fn set_scale(&mut self, id: WindowId, scale: f64) {
        let fractional = self.g.fractional.is_some();
        let (Some(gpu), Some(w)) = (&self.gpu, self.reg.windows.get_mut(&id)) else {
            return;
        };
        if (w.scale - scale).abs() < f64::EPSILON {
            return;
        }
        tracing::debug!(%id, scale, "scale changed");
        w.scale = scale;
        w.root.handle_window_event(WindowEvent::Rescale(scale));
        if w.configured {
            w.surf.apply(gpu, scale, fractional);
            let (pw, ph) = w.surf.physical(scale);
            w.root
                .handle_window_event(WindowEvent::Resize(PhysicalSize::new(pw, ph)));
            w.accesskit.set_size((pw, ph));
            for p in &mut w.popups {
                p.surf.int_scale = w.surf.int_scale;
                if p.configured {
                    p.surf.apply(gpu, scale, fractional);
                }
            }
        }
        w.dirty = true;
    }

    // --- Signals -----------------------------------------------------------------------

    fn handle_signal(&mut self, id: WindowId, signal: RenderRootSignal) {
        if let RenderRootSignal::Action(action, widget) = signal {
            self.events.push(AppEvent::Action(id, widget, action));
            return;
        }
        let seat = self.input.seat.clone();
        let serial = self.input.last_serial;
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        let toplevel = match &w.shell {
            Shell::Toplevel(window) => Some(window),
            Shell::Layer(_) => None,
        };
        match signal {
            RenderRootSignal::Action(..) => {}
            RenderRootSignal::RequestRedraw | RenderRootSignal::RequestAnimFrame => {
                w.dirty = true;
            }
            RenderRootSignal::StartIme => {
                w.ime_allowed = true;
                self.sync_text_input();
            }
            RenderRootSignal::EndIme => {
                w.ime_allowed = false;
                self.sync_text_input();
            }
            RenderRootSignal::ImeMoved(pos, size) => {
                w.ime_area = Rect::from_origin_size((pos.x, pos.y), (size.width, size.height));
                self.sync_text_input();
            }
            RenderRootSignal::SetCursor(cursor) => {
                w.cursor = cursor;
                if self.input.pointer_focus == Some(id) {
                    self.input.set_cursor(&self.conn, cursor);
                }
            }
            RenderRootSignal::SetSize(size) => {
                let logical = (
                    (f64::from(size.width) / w.scale).round() as u32,
                    (f64::from(size.height) / w.scale).round() as u32,
                );
                match &w.shell {
                    Shell::Toplevel(_) => {
                        w.free_size = logical;
                        if w.configured && !w.server_sized {
                            self.configure_window(id, logical);
                        }
                    }
                    Shell::Layer(layer) => {
                        layer.set_size(logical.0, logical.1);
                        layer.commit();
                    }
                }
            }
            RenderRootSignal::SetTitle(title) => {
                if let Some(window) = toplevel {
                    window.set_title(title);
                }
            }
            RenderRootSignal::DragWindow => {
                if let (Some(window), Some(seat), Some(serial)) = (toplevel, &seat, serial) {
                    window.move_(seat, serial);
                }
            }
            RenderRootSignal::DragResizeWindow(direction) => {
                if let (Some(window), Some(seat), Some(serial)) = (toplevel, &seat, serial) {
                    window.resize(seat, serial, resize_edge(direction));
                }
            }
            RenderRootSignal::ToggleMaximized => {
                if let Some(window) = toplevel {
                    if w.maximized {
                        window.unset_maximized();
                    } else {
                        window.set_maximized();
                    }
                }
            }
            RenderRootSignal::Minimize => {
                if let Some(window) = toplevel {
                    window.set_minimized();
                }
            }
            RenderRootSignal::ShowWindowMenu(pos) => {
                if let (Some(window), Some(seat), Some(serial)) = (toplevel, &seat, serial) {
                    window.show_window_menu(seat, serial, (pos.x as i32, pos.y as i32));
                }
            }
            RenderRootSignal::Exit => self.reg.exit = true,
            RenderRootSignal::NewLayer(kind, widget, pos) => {
                tracing::debug!(?kind, ?pos, "new Masonry layer");
                self.open_popup(id, &kind, widget, pos);
            }
            RenderRootSignal::RemoveLayer(layer) => {
                // A layer can be removed twice: a menu's submenus go with it (Masonry's
                // attached layers), and a dismissed popup's nested ones are removed too.
                if w.root.get_widget(layer).is_some() {
                    w.root.remove_layer(layer);
                }
                if let Some(pi) = w.popups.iter().position(|p| p.root_id == layer) {
                    // Menus nested on this one go with it.
                    let nested = w.take_popups(pi).into_iter().filter(|l| *l != layer);
                    w.dismissed.extend(nested);
                }
                w.inline_layers.retain(|l| *l != layer);
                w.dismissed.retain(|l| *l != layer);
                w.dirty = true;
            }
            RenderRootSignal::RepositionLayer(layer, pos) => {
                w.root.reposition_layer(layer, pos);
                self.reposition_popup(id, layer, pos);
            }
            RenderRootSignal::ClipboardStore(text) => self.store_clipboard(text),
            RenderRootSignal::TakeFocus => self.request_activation(id),
            RenderRootSignal::WidgetSelectedInInspector(widget) => {
                tracing::info!(%id, %widget, "widget selected in the inspector");
            }
        }
    }

    /// Removes layers whose popup the compositor dismissed and Masonry didn't close.
    fn remove_dismissed(&mut self, id: WindowId) {
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        for layer in std::mem::take(&mut w.dismissed) {
            if w.root.get_widget(layer).is_some() {
                w.root.remove_layer(layer);
                w.dirty = true;
            }
        }
    }

    /// Adds `widget` as a Masonry layer and shows it in an xdg-popup at its bounding box,
    /// placed for its `kind` (see [`popup`]).
    fn open_popup(
        &mut self,
        id: WindowId,
        kind: &LayerType,
        widget: NewWidget<dyn Widget>,
        pos: Point,
    ) {
        let layer = widget.id();
        let qh = self.qh.clone();
        let (seat, serial) = (self.input.seat.clone(), self.input.last_serial);
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        w.root.add_layer(widget, pos);
        w.dirty = true;
        let bbox = w
            .root
            .get_widget(layer)
            .map(|r| r.ctx().bounding_box())
            .unwrap_or_else(|| Rect::from_origin_size(pos, (1.0, 1.0)));
        let popover = w
            .root
            .get_widget(layer)
            .and_then(|root| root.get_prop::<PopoverAnchor>().0);
        let border = w.root.get_widget(layer).map_or(bbox, |r| {
            r.ctx()
                .window_transform()
                .transform_rect_bbox(r.ctx().border_box())
        });
        let placement = match popover {
            Some(request) => Placement::popover(request, bbox),
            None => {
                let owner = popup::owner_rect(w.root.get_layer_root(0), pos);
                Placement::new(kind, pos, bbox, border, owner)
            }
        };
        // A menu opened from a menu nests on it: a grab must come from the topmost grabbing
        // popup. Everything else hangs off the window.
        let parent = placement
            .grab
            .then(|| w.popups.iter().rev().find(|p| p.placement.grab))
            .flatten();
        let parent_id = parent.map(|p| p.root_id);
        let parent_surface = match (parent, &w.shell) {
            (Some(p), _) => Some(p.popup.xdg_surface()),
            (None, Shell::Toplevel(window)) => Some(window.xdg_surface()),
            (None, Shell::Layer(_)) => None,
        };

        let positioner = match placement.positioner(&self.g.xdg, w.parent_origin(parent_id)) {
            Ok(p) => p,
            Err(err) => {
                tracing::error!(%err, "no xdg_positioner; painting the layer inline");
                w.inline_layers.push(layer);
                return;
            }
        };
        let wl = self.g.compositor.create_surface(&qh);
        let wgpu_surface = match handle::create_surface(
            gpu_instance(&self.gpu, &self.instance),
            &self.conn,
            &wl,
        ) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!(%err, "no wgpu surface for the popup; painting the layer inline");
                w.inline_layers.push(layer);
                return;
            }
        };
        let popup =
            match Popup::from_surface(parent_surface, &positioner, &qh, wl.clone(), &self.g.xdg) {
                Ok(p) => p,
                Err(err) => {
                    tracing::error!(%err, "can't create an xdg-popup; painting the layer inline");
                    w.inline_layers.push(layer);
                    return;
                }
            };
        if let (None, Shell::Layer(layer_surface)) = (parent_id, &w.shell) {
            layer_surface.get_popup(popup.xdg_popup());
        }
        if placement.grab {
            // Before the initial commit, so the compositor dismisses the popup on an
            // outside click (xdg_popup.grab must precede mapping).
            if let (Some(seat), Some(serial)) = (&seat, serial) {
                popup.xdg_popup().grab(seat, serial);
            }
        } else {
            // Tooltips let the pointer through to the window.
            match Region::new(&self.g.compositor) {
                Ok(region) => wl.set_input_region(Some(region.wl_region())),
                Err(err) => tracing::debug!(%err, "no wl_region for the tooltip"),
            }
        }
        let insets = placement.insets;
        if insets.any() {
            // The shadow is outside the window geometry: the compositor places the box.
            popup.xdg_surface().set_window_geometry(
                insets.left,
                insets.top,
                placement.size.0,
                placement.size.1,
            );
        }
        let mut surf = Surf::new(&self.g, &qh, wl, wgpu_surface, false);
        surf.size = (
            (placement.size.0 + insets.horizontal()) as u32,
            (placement.size.1 + insets.vertical()) as u32,
        );
        surf.int_scale = w.surf.int_scale;
        popup.wl_surface().commit();
        w.popups.push(PopupLayer {
            surf,
            popup,
            root_id: layer,
            parent: parent_id,
            placement,
            requested: pos,
            origin: bbox.origin(),
            layer_offset: pos - bbox.origin(),
            configured: false,
        });
    }

    /// Follows Masonry moving a layer: places its popup again (`xdg_popup.reposition`).
    fn reposition_popup(&mut self, id: WindowId, layer: WidgetId, pos: Point) {
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        let Some(pi) = w.popups.iter().position(|p| p.root_id == layer) else {
            return;
        };
        let parent_origin = w.parent_origin(w.popups[pi].parent);
        let p = &mut w.popups[pi];
        if p.placement.popover {
            // Placed by its anchor; the compositor's configure moves the layer.
            return;
        }
        p.placement = p.placement.moved(pos - p.requested);
        p.requested = pos;
        if p.popup.xdg_popup().version() < 3 {
            tracing::debug!("xdg_popup.reposition needs xdg_wm_base v3; the popup stays");
            return;
        }
        match p.placement.positioner(&self.g.xdg, parent_origin) {
            Ok(positioner) => {
                self.reposition_token = self.reposition_token.wrapping_add(1);
                p.popup.reposition(&positioner, self.reposition_token);
            }
            Err(err) => tracing::error!(%err, "no xdg_positioner; the popup stays"),
        }
    }

    /// Asks the compositor to focus window `id` (Masonry's `TakeFocus`): xdg-activation,
    /// with the latest input serial and the focused surface as proof of user intent.
    fn request_activation(&mut self, id: WindowId) {
        let Some(activation) = &self.g.activation else {
            tracing::debug!(%id, "no xdg-activation: can't take focus");
            return;
        };
        let surface = self
            .input
            .kb_focus
            .and_then(|f| self.reg.windows.get(&f))
            .map(|w| w.surf.wl.clone());
        activation.request_token(
            &self.qh,
            RequestData {
                app_id: None,
                seat_and_serial: self.input.seat.clone().zip(self.input.last_serial),
                surface,
                udata: id,
            },
        );
    }

    // --- text-input-v3 ------------------------------------------------------------------

    /// Enables or disables text input for the focused window to match Masonry's IME state.
    fn sync_text_input(&mut self) {
        let Some(ti) = &mut self.input.text_input else {
            return;
        };
        let focused = self
            .input
            .ti_focus
            .and_then(|id| self.reg.windows.get_mut(&id))
            .filter(|w| w.ime_allowed);
        match focused {
            Some(w) => {
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
            None if ti.enabled => {
                ti.ti.disable();
                ti.ti.commit();
                ti.enabled = false;
            }
            None => {}
        }
    }

    fn text_input_done(&mut self) {
        let Some(ti) = &mut self.input.text_input else {
            return;
        };
        let commit = ti.pending_commit.take();
        let preedit = ti.pending_preedit.take();
        let Some(w) = self
            .input
            .ti_focus
            .and_then(|id| self.reg.windows.get_mut(&id))
        else {
            return;
        };
        if let Some(text) = commit {
            tracing::debug!(%text, "text-input-v3 commit");
            w.root
                .handle_text_event(TextEvent::Ime(Ime::Preedit(String::new(), None)));
            w.root.handle_text_event(TextEvent::Ime(Ime::Commit(text)));
        }
        let (text, cursor) = preedit.unwrap_or_default();
        w.root
            .handle_text_event(TextEvent::Ime(Ime::Preedit(text, cursor)));
    }

    // --- Drawing ----------------------------------------------------------------------

    fn draw_dirty(&mut self) {
        let ids: Vec<WindowId> = self
            .reg
            .windows
            .iter()
            .filter(|(_, w)| {
                w.dirty
                    && w.configured
                    && (w.surf.ready() || w.popups.iter().any(|p| p.configured && p.surf.ready()))
            })
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            self.draw(id);
        }
    }

    /// Runs an animation frame and paints every surface of the window that can take a
    /// frame; the others are marked stale and drawn when their frame callback fires.
    fn draw(&mut self, id: WindowId) {
        let (Some(gpu), Some(w)) = (self.gpu.as_mut(), self.reg.windows.get_mut(&id)) else {
            return;
        };
        let now = Instant::now();
        let elapsed = w.last_anim.map(|t| now - t).unwrap_or_default();
        w.root.handle_window_event(WindowEvent::AnimFrame(elapsed));
        w.last_anim = w.root.needs_anim().then_some(now);

        let (plan, access) = w.root.redraw();
        if let Some(update) = access {
            w.accesskit.update(update);
        }
        w.sync_chrome(self.g.window.as_ref(), &self.qh);

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
        if w.surf.ready() {
            let (pw, ph) = w.surf.physical(w.scale);
            let frame = PreparedFrame::new(pw, ph, w.scale, w.background, &empty, &main);
            w.surf.present(gpu, &self.qh, frame);
        } else {
            w.surf.stale = true;
        }
        for (p, layers) in w.popups.iter_mut().zip(&per_popup) {
            if !p.configured {
                continue;
            }
            if p.surf.ready() {
                let (pw, ph) = p.surf.physical(w.scale);
                let frame = PreparedFrame::new(pw, ph, w.scale, Color::TRANSPARENT, &empty, layers);
                p.surf.present(gpu, &self.qh, frame);
            } else {
                p.surf.stale = true;
            }
        }
        w.dirty = false;
    }
}

/// The GPU's wgpu instance once it exists, `fallback` before (the same instance either
/// way). A free function so callers can hold other fields of `State` mutably.
fn gpu_instance<'a>(gpu: &'a Option<Gpu>, fallback: &'a wgpu::Instance) -> &'a wgpu::Instance {
    gpu.as_ref().map_or(fallback, |gpu| &gpu.instance)
}

/// Creates a layer surface placed as `spec` says; the initial commit asks for a configure.
fn create_layer(
    layer_shell: &LayerShell,
    qh: &QueueHandle<State>,
    wl: wl_surface::WlSurface,
    spec: &LayerSpec,
    size: (u32, u32),
    output: Option<&wl_output::WlOutput>,
) -> LayerSurface {
    let level = match spec.layer {
        ShellLayer::Background => wlr_layer::Layer::Background,
        ShellLayer::Bottom => wlr_layer::Layer::Bottom,
        ShellLayer::Top => wlr_layer::Layer::Top,
        ShellLayer::Overlay => wlr_layer::Layer::Overlay,
    };
    let layer =
        layer_shell.create_layer_surface(qh, wl, level, Some(spec.namespace.clone()), output);
    layer.set_anchor(layer_anchor(spec.anchor));
    layer.set_size(size.0, size.1);
    let m = spec.margin;
    layer.set_margin(m.top, m.right, m.bottom, m.left);
    layer.set_exclusive_zone(spec.exclusive_zone);
    layer.set_keyboard_interactivity(match spec.keyboard {
        LayerKeyboard::None => wlr_layer::KeyboardInteractivity::None,
        LayerKeyboard::Exclusive => wlr_layer::KeyboardInteractivity::Exclusive,
        LayerKeyboard::OnDemand => wlr_layer::KeyboardInteractivity::OnDemand,
    });
    layer.commit();
    layer
}

fn layer_anchor(anchor: Anchor) -> wlr_layer::Anchor {
    let mut out = wlr_layer::Anchor::empty();
    out.set(wlr_layer::Anchor::TOP, anchor.top);
    out.set(wlr_layer::Anchor::BOTTOM, anchor.bottom);
    out.set(wlr_layer::Anchor::LEFT, anchor.left);
    out.set(wlr_layer::Anchor::RIGHT, anchor.right);
    out
}

fn resize_edge(direction: ResizeDirection) -> xdg_toplevel::ResizeEdge {
    use xdg_toplevel::ResizeEdge;
    match direction {
        ResizeDirection::North => ResizeEdge::Top,
        ResizeDirection::NorthEast => ResizeEdge::TopRight,
        ResizeDirection::East => ResizeEdge::Right,
        ResizeDirection::SouthEast => ResizeEdge::BottomRight,
        ResizeDirection::South => ResizeEdge::Bottom,
        ResizeDirection::SouthWest => ResizeEdge::BottomLeft,
        ResizeDirection::West => ResizeEdge::Left,
        ResizeDirection::NorthWest => ResizeEdge::TopLeft,
    }
}

impl ActivationHandler for State {
    type RequestUdata = WindowId;

    fn new_token(&mut self, token: String, data: &RequestData<WindowId>) {
        if let (Some(activation), Some(w)) = (&self.g.activation, self.reg.windows.get(&data.udata))
        {
            activation.activate::<State>(&w.surf.wl, token);
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
            && let Some((id, None)) = state.locate(&self.0)
        {
            // The protocol sends the scale in 120ths.
            state.set_scale(id, f64::from(scale) / 120.0);
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
                state.input.ti_focus = state.locate(&surface).map(|(id, _)| id);
                tracing::debug!(window = ?state.input.ti_focus, "text-input-v3 enter");
                state.sync_text_input();
            }
            Event::Leave { .. } => {
                tracing::debug!("text-input-v3 leave");
                let focus = state.input.ti_focus.take();
                if let Some(ti) = &mut state.input.text_input
                    && ti.enabled
                {
                    ti.ti.disable();
                    ti.ti.commit();
                    ti.enabled = false;
                    if let Some(w) = focus.and_then(|id| state.reg.windows.get_mut(&id)) {
                        w.root.handle_text_event(TextEvent::Ime(Ime::Disabled));
                    }
                }
            }
            Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                if let Some(ti) = &mut state.input.text_input {
                    let cursor = (cursor_begin >= 0 && cursor_end >= 0)
                        .then_some((cursor_begin as usize, cursor_end as usize));
                    ti.pending_preedit = Some((text.unwrap_or_default(), cursor));
                }
            }
            Event::CommitString { text } => {
                if let Some(ti) = &mut state.input.text_input {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_size_rounds_and_is_never_empty() {
        assert_eq!(physical((440, 360), 1.5), (660, 540));
        assert_eq!(physical((101, 1), 1.25), (126, 1));
        assert_eq!(physical((0, 0), 2.0), (1, 1));
    }

    #[test]
    fn layer_anchor_maps_every_edge() {
        let all = Anchor {
            top: true,
            bottom: true,
            left: true,
            right: true,
        };
        assert_eq!(layer_anchor(all), wlr_layer::Anchor::all());
        assert_eq!(layer_anchor(Anchor::TOP), wlr_layer::Anchor::TOP);
        assert_eq!(
            layer_anchor(Anchor::TOP_EDGE),
            wlr_layer::Anchor::TOP | wlr_layer::Anchor::LEFT | wlr_layer::Anchor::RIGHT
        );
        assert_eq!(layer_anchor(Anchor::default()), wlr_layer::Anchor::empty());
    }

    #[test]
    fn resize_edges_match_directions() {
        assert_eq!(
            resize_edge(ResizeDirection::NorthWest),
            xdg_toplevel::ResizeEdge::TopLeft
        );
        assert_eq!(
            resize_edge(ResizeDirection::South),
            xdg_toplevel::ResizeEdge::Bottom
        );
    }
}
