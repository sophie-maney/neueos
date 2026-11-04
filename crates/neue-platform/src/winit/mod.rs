//! winit driver for Masonry's `RenderRoot` (task 2.4, ADR 0032).
//!
//! Replaces `masonry_winit` with our own loop, so the macOS dev path and the Wayland driver
//! share one renderer (`crate::gpu`) and one app-facing shape. It handles:
//! - several windows, opened and closed at run time through [`Ctx`];
//! - a wgpu surface per window, scale-factor changes and resizes;
//! - pointer and keyboard input (translated by `ui-events-winit`), IME, cursors;
//! - a plain-text clipboard (the macOS pasteboard);
//! - animation frames, timed per window;
//! - AccessKit through `accesskit_winit` (VoiceOver).
//!
//! macOS only (ADR 0032): on Linux, winit would bring X11 crates, and apps use the
//! Wayland driver.
//!
//! Masonry layers (menus, tooltips) are painted as overlays inside their window, as
//! `masonry_winit` does; only the Wayland driver turns them into popups.
//!
//! The loop exits when a widget requests it, when the app calls [`Ctx::exit`], or when the
//! last window closes.

mod clipboard;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use accesskit_winit::Adapter;
use masonry_core::app::{
    RenderRoot, RenderRootOptions, RenderRootSignal, VisualLayerKind, WindowSizePolicy,
};
use masonry_core::core::keyboard::{Key, KeyState};
use masonry_core::core::{Ime, KeyboardEvent, ResizeDirection, TextEvent, WindowEvent};
use masonry_core::peniko::Color;
use masonry_imaging::{Layer as ImagingLayer, PreparedFrame};
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{Ime as WinitIme, WindowEvent as WinitEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{
    ResizeDirection as WinitResizeDirection, Window as WinitWindow, WindowAttributes,
    WindowId as WinitId,
};

use crate::app::{DriverWindow, Registry};
use crate::gpu::{Gpu, GpuError, SurfaceTarget};
use crate::{App, Ctx, Options, Waker, WindowId, WindowKind, WindowSpec};
use clipboard::Clipboard;

/// Driver errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("event loop: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("can't create a window: {0}")]
    Window(#[from] winit::error::OsError),
    #[error("can't create a wgpu surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("wgpu: {0}")]
    Gpu(String),
}

impl From<GpuError> for Error {
    fn from(err: GpuError) -> Self {
        Self::Gpu(err.0)
    }
}

/// Runs `app` until it exits or its last window closes.
pub fn run(app: &mut dyn App, options: Options) -> Result<(), Error> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    // The display handle lets wgpu's GL backend work under Wayland; Metal ignores it.
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
        Box::new(event_loop.owned_display_handle()),
    ));
    let proxy = event_loop.create_proxy();
    let waker = Waker::new(move || {
        // Fails only once the loop has ended, when nobody is left to wake.
        let _ = proxy.send_event(UserEvent::Wake);
    });
    let mut driver = Driver {
        shared: Shared::new(waker),
        by_winit: HashMap::new(),
        gpu: None,
        instance,
        proxy: event_loop.create_proxy(),
        clipboard: Clipboard::new(),
        options,
        app,
        started: false,
        error: None,
    };
    event_loop.run_app(&mut driver)?;
    match driver.error.take() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Events sent to the loop from other threads.
#[derive(Debug)]
enum UserEvent {
    /// An AccessKit request.
    Access(accesskit_winit::Event),
    /// [`Waker::wake`].
    Wake,
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}

/// Window state the app can reach through [`Ctx`].
type Shared = Registry<Win>;

impl DriverWindow for Win {
    fn render_root(&mut self) -> &mut RenderRoot {
        &mut self.root
    }
}

/// One open window.
struct Win {
    // Field order is drop order: the wgpu surface goes before the window it draws to.
    target: SurfaceTarget,
    accesskit: Adapter,
    reducer: WindowEventReducer,
    root: RenderRoot,
    signals: Receiver<RenderRootSignal>,
    background: Color,
    /// When the last animation frame was drawn, while widgets are animating.
    last_anim: Option<Instant>,
    handle: Arc<WinitWindow>,
}

impl std::fmt::Debug for Win {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Win")
            .field("target", &self.target)
            .field("winit_id", &self.handle.id())
            .finish_non_exhaustive()
    }
}

struct Driver<'a> {
    // Field order is drop order: windows and their surfaces before the device.
    shared: Shared,
    /// winit's ids of open windows. Entries of closed windows are dropped in `pump`.
    by_winit: HashMap<WinitId, WindowId>,
    gpu: Option<Gpu>,
    instance: wgpu::Instance,
    proxy: EventLoopProxy<UserEvent>,
    clipboard: Clipboard,
    options: Options,
    app: &'a mut dyn App,
    started: bool,
    /// The first fatal error; [`run`] returns it after the loop ends.
    error: Option<Error>,
}

impl Driver<'_> {
    /// Creates queued windows and handles every pending `RenderRoot` signal, until neither
    /// is left (a signal's action can open windows, and new windows emit signals).
    fn pump(&mut self, event_loop: &ActiveEventLoop) {
        loop {
            for (id, spec) in std::mem::take(&mut self.shared.opening) {
                if let Err(err) = self.create_window(event_loop, id, spec) {
                    tracing::error!(%err, %id, "can't open window");
                    self.error.get_or_insert(err);
                    self.shared.exit = true;
                }
            }
            let mut handled = false;
            let ids: Vec<WindowId> = self.shared.windows.keys().copied().collect();
            for id in ids {
                while let Some(signal) = self
                    .shared
                    .windows
                    .get(&id)
                    .and_then(|w| w.signals.try_recv().ok())
                {
                    handled = true;
                    self.handle_signal(id, signal);
                }
            }
            if !handled && self.shared.opening.is_empty() {
                break;
            }
        }
        let windows = &self.shared.windows;
        self.by_winit.retain(|_, id| windows.contains_key(id));
        if self.started && self.shared.done() {
            event_loop.exit();
        }
    }

    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        spec: WindowSpec,
    ) -> Result<(), Error> {
        if let WindowKind::Layer(layer) = &spec.kind {
            tracing::warn!(namespace = %layer.namespace, "no layer shell on macOS; opening a window");
        }
        if spec.parent.is_some() || spec.modal {
            tracing::debug!(%id, "parent and modal windows are Wayland only; opening a plain window");
        }
        let attributes = WindowAttributes::default()
            .with_title(spec.title)
            .with_inner_size(LogicalSize::new(spec.width, spec.height))
            // Shown after the first frame, and AccessKit must attach to a hidden window.
            .with_visible(false);
        let handle = Arc::new(event_loop.create_window(attributes)?);
        let accesskit = Adapter::with_event_loop_proxy(event_loop, &handle, self.proxy.clone());
        // Same instance either way; the GPU's copy once it exists.
        let instance = self
            .gpu
            .as_ref()
            .map_or(&self.instance, |gpu| &gpu.instance);
        let surface = instance.create_surface(Arc::clone(&handle))?;
        let gpu = match &mut self.gpu {
            Some(gpu) => gpu,
            // The adapter must be able to present to the first window's surface.
            None => self.gpu.insert(Gpu::new(self.instance.clone(), &surface)?),
        };
        let size = handle.inner_size();
        let target = SurfaceTarget::new(gpu, surface, size.width.max(1), size.height.max(1))?;

        let (tx, signals) = mpsc::channel();
        let mut root = RenderRoot::new(
            spec.root,
            move |signal| {
                // The receiver lives as long as the window; a send error means it's gone.
                let _ = tx.send(signal);
            },
            RenderRootOptions {
                default_properties: Arc::clone(&self.options.default_properties),
                use_system_fonts: false,
                size_policy: WindowSizePolicy::User,
                size,
                scale_factor: handle.scale_factor(),
                test_font: self.options.font.clone(),
            },
        );
        if spec.focus_fallback.is_some() && !root.set_focus_fallback(spec.focus_fallback) {
            tracing::warn!(%id, "focus fallback isn't in the window's tree");
        }
        tracing::debug!(%id, winit_id = ?handle.id(), ?size, "opened window");
        self.by_winit.insert(handle.id(), id);
        self.shared.windows.insert(
            id,
            Win {
                target,
                accesskit,
                reducer: WindowEventReducer::default(),
                root,
                signals,
                background: spec.background,
                last_anim: None,
                handle: Arc::clone(&handle),
            },
        );
        // Lay out and paint before showing. macOS reports a hidden window's surface as
        // occluded, so the frame may be skipped there; the redraw after showing covers it.
        self.redraw(id);
        handle.set_visible(true);
        handle.request_redraw();
        Ok(())
    }

    fn handle_signal(&mut self, id: WindowId, signal: RenderRootSignal) {
        if let RenderRootSignal::Action(action, widget) = signal {
            tracing::trace!(%id, %widget, action = action.type_name(), "action");
            let app = &mut *self.app;
            app.on_action(&mut Ctx::new(&mut self.shared), id, widget, action);
            return;
        }
        let Some(win) = self.shared.windows.get_mut(&id) else {
            return;
        };
        let handle = &win.handle;
        match signal {
            RenderRootSignal::Action(..) => {}
            RenderRootSignal::StartIme => handle.set_ime_allowed(true),
            RenderRootSignal::EndIme => handle.set_ime_allowed(false),
            RenderRootSignal::ImeMoved(position, size) => {
                handle.set_ime_cursor_area(position, size);
            }
            RenderRootSignal::ClipboardStore(text) => self.clipboard.set(text),
            RenderRootSignal::RequestRedraw | RenderRootSignal::RequestAnimFrame => {
                handle.request_redraw();
            }
            RenderRootSignal::TakeFocus => handle.focus_window(),
            RenderRootSignal::SetCursor(cursor) => handle.set_cursor(cursor),
            RenderRootSignal::SetSize(size) => {
                // `None` means the request is pending and a `Resized` event will follow.
                if let Some(size) = handle.request_inner_size(size) {
                    win.root.handle_window_event(WindowEvent::Resize(size));
                }
            }
            RenderRootSignal::SetTitle(title) => handle.set_title(&title),
            RenderRootSignal::DragWindow => {
                if let Err(err) = handle.drag_window() {
                    tracing::debug!(%err, "can't start a window drag");
                }
            }
            RenderRootSignal::DragResizeWindow(direction) => {
                if let Err(err) = handle.drag_resize_window(resize_direction(direction)) {
                    tracing::debug!(%err, "can't start a window resize");
                }
            }
            RenderRootSignal::ToggleMaximized => handle.set_maximized(!handle.is_maximized()),
            RenderRootSignal::Minimize => handle.set_minimized(true),
            RenderRootSignal::Exit => self.shared.exit = true,
            RenderRootSignal::ShowWindowMenu(position) => handle.show_window_menu(position),
            RenderRootSignal::WidgetSelectedInInspector(widget) => {
                tracing::info!(%id, %widget, "widget selected in the inspector");
            }
            RenderRootSignal::NewLayer(_kind, root, origin) => win.root.add_layer(root, origin),
            RenderRootSignal::RemoveLayer(root) => win.root.remove_layer(root),
            RenderRootSignal::RepositionLayer(root, origin) => {
                win.root.reposition_layer(root, origin);
            }
        }
    }

    /// Runs an animation frame, paints the window and pushes the access tree.
    fn redraw(&mut self, id: WindowId) {
        let (Some(gpu), Some(win)) = (self.gpu.as_mut(), self.shared.windows.get_mut(&id)) else {
            return;
        };
        let size = win.root.size();
        if size.width == 0 || size.height == 0 {
            // Minimised: nothing to draw, and wgpu can't configure an empty surface.
            return;
        }
        win.target.resize(gpu, size.width, size.height);

        let now = Instant::now();
        let elapsed = win.last_anim.map(|t| now - t).unwrap_or_default();
        win.root
            .handle_window_event(WindowEvent::AnimFrame(elapsed));
        win.last_anim = win.root.needs_anim().then_some(now);

        let (layers, tree_update) = win.root.redraw();
        let overlays: Vec<_> = layers
            .overlay_layers()
            .filter_map(|layer| match &layer.kind {
                VisualLayerKind::Scene(scene) => Some(ImagingLayer {
                    scene,
                    transform: layer.transform,
                }),
                _ => None,
            })
            .collect();
        if let Some(root_layer) = layers.root_layer()
            && let VisualLayerKind::Scene(root_scene) = &root_layer.kind
        {
            let frame = PreparedFrame::new(
                size.width,
                size.height,
                win.handle.scale_factor(),
                win.background,
                root_scene,
                &overlays,
            );
            let handle = &win.handle;
            gpu.render(&win.target, frame, || handle.pre_present_notify());
        } else {
            tracing::warn!(%id, "paint produced no root scene");
        }
        if let Some(tree_update) = tree_update {
            win.accesskit.update_if_active(|| tree_update);
        }
    }
}

impl ApplicationHandler<UserEvent> for Driver<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        let app = &mut *self.app;
        app.on_start(&mut Ctx::new(&mut self.shared));
        self.pump(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, winit_id: WinitId, event: WinitEvent) {
        let Some(&id) = self.by_winit.get(&winit_id) else {
            return;
        };
        let Some(win) = self.shared.windows.get_mut(&id) else {
            return;
        };
        win.accesskit.process_event(&win.handle, &event);

        // Synthetic key events repeat keys held while focus arrived; Masonry doesn't want them.
        let synthetic = matches!(
            event,
            WinitEvent::KeyboardInput {
                is_synthetic: true,
                ..
            }
        );
        if !synthetic {
            match win.reducer.reduce(win.handle.scale_factor(), &event) {
                Some(WindowEventTranslation::Keyboard(key)) if is_paste(&key) => {
                    if let Some(text) = self.clipboard.get() {
                        win.root.handle_text_event(TextEvent::ClipboardPaste(text));
                    }
                }
                Some(WindowEventTranslation::Keyboard(key)) => {
                    win.root.handle_text_event(TextEvent::Keyboard(key));
                }
                Some(WindowEventTranslation::Pointer(pointer)) => {
                    win.root.handle_pointer_event(pointer);
                }
                None => {}
            }
        }

        match event {
            WinitEvent::ScaleFactorChanged { scale_factor, .. } => {
                win.root
                    .handle_window_event(WindowEvent::Rescale(scale_factor));
            }
            WinitEvent::Resized(size) => {
                win.root.handle_window_event(WindowEvent::Resize(size));
                win.handle.request_redraw();
            }
            WinitEvent::Ime(ime) => {
                win.root.handle_text_event(TextEvent::Ime(ime_event(ime)));
            }
            WinitEvent::Focused(focused) => {
                win.root
                    .handle_text_event(TextEvent::WindowFocusChange(focused));
            }
            WinitEvent::RedrawRequested => self.redraw(id),
            WinitEvent::CloseRequested => {
                let app = &mut *self.app;
                app.on_close_requested(&mut Ctx::new(&mut self.shared), id);
            }
            _ => {}
        }
        self.pump(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let event = match event {
            UserEvent::Access(event) => event,
            UserEvent::Wake => {
                if self.shared.waker.take() {
                    let app = &mut *self.app;
                    app.on_wake(&mut Ctx::new(&mut self.shared));
                    self.pump(event_loop);
                }
                return;
            }
        };
        let window = self
            .by_winit
            .get(&event.window_id)
            .and_then(|id| self.shared.windows.get_mut(id));
        let Some(win) = window else {
            return;
        };
        match event.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                win.root.handle_window_event(WindowEvent::EnableAccessTree);
                // The tree goes out with the next frame.
                win.handle.request_redraw();
            }
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                win.root.handle_access_event(request);
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                win.root.handle_window_event(WindowEvent::DisableAccessTree);
            }
        }
        self.pump(event_loop);
    }
}

/// Cmd-V. The driver answers it with the clipboard's text, as Masonry's text widgets expect.
fn is_paste(key: &KeyboardEvent) -> bool {
    key.state == KeyState::Down
        && key.modifiers.meta()
        && matches!(&key.key, Key::Character(c) if c.eq_ignore_ascii_case("v"))
}

fn ime_event(ime: WinitIme) -> Ime {
    match ime {
        WinitIme::Enabled => Ime::Enabled,
        WinitIme::Disabled => Ime::Disabled,
        WinitIme::Preedit(text, cursor) => Ime::Preedit(text, cursor),
        WinitIme::Commit(text) => Ime::Commit(text),
    }
}

fn resize_direction(direction: ResizeDirection) -> WinitResizeDirection {
    match direction {
        ResizeDirection::North => WinitResizeDirection::North,
        ResizeDirection::NorthEast => WinitResizeDirection::NorthEast,
        ResizeDirection::East => WinitResizeDirection::East,
        ResizeDirection::SouthEast => WinitResizeDirection::SouthEast,
        ResizeDirection::South => WinitResizeDirection::South,
        ResizeDirection::SouthWest => WinitResizeDirection::SouthWest,
        ResizeDirection::West => WinitResizeDirection::West,
        ResizeDirection::NorthWest => WinitResizeDirection::NorthWest,
    }
}

#[cfg(test)]
mod tests {
    use masonry_core::core::Modifiers;
    use masonry_core::core::keyboard::{Code, Location};

    use super::*;

    fn key(text: &str, modifiers: Modifiers, state: KeyState) -> KeyboardEvent {
        KeyboardEvent {
            state,
            key: Key::Character(text.into()),
            code: Code::KeyV,
            location: Location::Standard,
            modifiers,
            repeat: false,
            is_composing: false,
        }
    }

    #[test]
    fn paste_is_command_v() {
        let command = Modifiers::META;
        assert!(is_paste(&key("v", command, KeyState::Down)));
        assert!(is_paste(&key(
            "V",
            command | Modifiers::SHIFT,
            KeyState::Down
        )));
        assert!(!is_paste(&key("v", command, KeyState::Up)));
        assert!(!is_paste(&key("v", Modifiers::empty(), KeyState::Down)));
        assert!(!is_paste(&key("c", command, KeyState::Down)));
    }

    #[test]
    fn ime_events_map_one_to_one() {
        assert_eq!(ime_event(WinitIme::Enabled), Ime::Enabled);
        assert_eq!(
            ime_event(WinitIme::Preedit("にほ".into(), Some((0, 3)))),
            Ime::Preedit("にほ".into(), Some((0, 3)))
        );
        assert_eq!(
            ime_event(WinitIme::Commit("日本".into())),
            Ime::Commit("日本".into())
        );
        assert_eq!(ime_event(WinitIme::Disabled), Ime::Disabled);
    }
}
