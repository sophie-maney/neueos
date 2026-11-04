//! tty backend: the compositor owns a seat, a GPU and its displays.
//!
//! DRM/KMS (atomic, through Smithay's `DrmOutputManager`), GBM buffers, a GLES renderer on
//! EGL/GBM, libinput for input devices, udev to find the GPU, and libseat (seatd or logind)
//! for device access and VT switching (Ctrl+Alt+F1…F12).
//!
//! One GPU drives every connected connector at its preferred mode, each on its own CRTC
//! with its own render and vblank cycle (task 1.7). udev change events on the GPU rescan
//! the connectors: new ones become outputs, disconnected ones are removed, and a changed
//! preferred mode (a monitor switch, or a resized VM window) is applied with a modeset.
//! Several GPUs are a later task. The pointer (XCursor theme, task 1.9) is drawn on the
//! cursor plane where the hardware has one (the DRM compositor falls back to compositing
//! it otherwise). A full-screen client buffer can go straight onto the primary plane, and
//! outputs run at a variable refresh rate when `adaptive-sync` asks for it (task 1.15c,
//! ADR 0020, `scanout.rs` and `vrr.rs`).
//!
//! Outputs are drawn only when something changed (task 1.15b, ADR 0019): each has a
//! [`Redraw`] that [`State::queue_redraw_all`] and the animated-cursor timer mark, and an
//! empty frame's callbacks go out at an estimated vblank. `RUST_LOG=neue_comp::tty=trace`
//! logs every render with the output's running counts.
//!
//! `NEUE_DRM_DEVICE=/dev/dri/cardN` overrides the GPU choice.
//!
//! Adapted from Smithay's `anvil/src/udev.rs` (MIT), reduced to a single GPU.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use neue_config::compositor::{ClickMethod, TouchpadConfig};
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{
    DrmDevice, DrmDeviceFd, DrmEvent, DrmEventMetadata, DrmEventTime, DrmNode, NodeType, VrrSupport,
};
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::input::InputEvent;
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{self, UdevBackend, UdevEvent};
use smithay::desktop::utils::{OutputPresentationFeedback, send_frames_surface_tree};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::{Mode, Output, PhysicalProperties};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{Device as _, ModeTypeFlags, connector, crtc};
use smithay::reexports::input::{self, Libinput};
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{DeviceFd, IsAlive, Physical, Point, Rectangle, Scale};
use smithay::wayland::compositor::with_states;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::drm_syncobj::{DrmSyncobjState, supports_syncobj_eventfd};
use smithay::wayland::presentation::Refresh;
use tracing::{debug, error, info, trace, warn};

use crate::Error;
use crate::cursor;
use crate::redraw::Redraw;
use crate::render::CLEAR_COLOR;
use crate::scanout::{self, FRAME_FLAGS, OutputFeedback};
use crate::state::{Backend, State};
use crate::vrr;

/// Primary plane formats, tried in order. 8-bit only until colour management (PLAN §3.1).
const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Abgr8888];
/// How far (logical pixels) from an output the pointer can be and still have its image
/// drawn there: more than any cursor is tall.
const CURSOR_REACH: i32 = 128;

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
/// Per-frame user data handed back on vblank: the presentation feedback of the surfaces
/// in the frame (task 1.14).
type FrameData = OutputPresentationFeedback;
type OutputManager = DrmOutputManager<Allocator, Exporter, FrameData, DrmDeviceFd>;
type TtyOutput = DrmOutput<Allocator, Exporter, FrameData, DrmDeviceFd>;

smithay::backend::renderer::element::render_elements! {
    /// Everything drawn on a tty output, topmost first.
    pub TtyElement<=GlesRenderer>;
    Output=crate::render::OutputElement,
    Cursor=MemoryRenderBufferRenderElement<GlesRenderer>,
    /// A client-drawn cursor or the drag icon.
    CursorSurface=WaylandSurfaceRenderElement<GlesRenderer>,
}

/// One lit connector: its output, the DRM output that scans it out, and its frame state.
struct OutputSurface {
    output: Output,
    connector: connector::Handle,
    drm_output: TtyOutput,
    /// When the output is drawn next.
    redraw: Redraw,
    /// The estimated-vblank timer of an empty frame, while it is armed.
    estimated_vblank: Option<RegistrationToken>,
    /// The timer for an animated cursor's next image, while it is armed.
    cursor_timer: Option<RegistrationToken>,
    /// Per-surface dmabuf feedback for this output (none without a render node).
    feedback: Option<OutputFeedback>,
    /// What the connector can do for variable refresh.
    vrr_support: vrr::Support,
    /// The CRTC runs at a variable rate (as last set).
    vrr_active: bool,
    /// The `adaptive-sync` mode last checked against the connector, to warn once per change.
    vrr_checked: Option<neue_config::compositor::AdaptiveSync>,
}

impl OutputSurface {
    /// Disarm the timers (the surface goes away, or its waits are reset).
    fn cancel_timers(&mut self, handle: &LoopHandle<'static, State>) {
        for token in [self.estimated_vblank.take(), self.cursor_timer.take()]
            .into_iter()
            .flatten()
        {
            handle.remove(token);
        }
    }
}

/// What a connector rescan changed, for the state to apply.
#[derive(Default)]
struct Changes {
    added: Vec<Output>,
    removed: Vec<Output>,
    mode_changed: Vec<Output>,
}

/// State owned by the tty backend.
///
/// Drop order matters: the DRM device restores the previous display state when its last
/// handle goes, which needs DRM master, so the session (which holds it) must outlive every
/// handle. Fields drop in declaration order and the session is last; the DRM and udev event
/// sources also hold handles and are removed from the loop in `Drop`.
pub struct TtyData {
    /// Lit connectors, in the order they were found.
    surfaces: Vec<OutputSurface>,
    output_manager: OutputManager,
    renderer: GlesRenderer,
    loop_handle: LoopHandle<'static, State>,
    drm_source: Option<RegistrationToken>,
    udev_source: Option<RegistrationToken>,
    /// The GPU's primary node, to match udev events; also the scanout device.
    node: DrmNode,
    /// The node clients allocate on, for dmabuf feedback.
    render_node: Option<DrmNode>,
    _dmabuf_global: DmabufGlobal,
    /// `linux-drm-syncobj-v1` (explicit sync), when the GPU supports `syncobj_eventfd`.
    pub(crate) syncobj_state: Option<DrmSyncobjState>,
    /// XCursor theme and the built-in arrow fallback (task 1.9).
    cursors: cursor::Themes,
    /// The session is active (our VT is in front). False while switched away.
    active: bool,
    /// The touchpads libinput has reported, to configure them again on a config reload.
    touchpads: Vec<input::Device>,
    libinput: Libinput,
    session: LibSeatSession,
}

impl fmt::Debug for TtyData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let outputs: Vec<String> = self.surfaces.iter().map(|s| s.output.name()).collect();
        f.debug_struct("TtyData")
            .field("seat", &self.session.seat())
            .field("outputs", &outputs)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl Drop for TtyData {
    fn drop(&mut self) {
        for token in [self.drm_source.take(), self.udev_source.take()]
            .into_iter()
            .flatten()
        {
            self.loop_handle.remove(token);
        }
    }
}

impl TtyData {
    /// The lit outputs, in the order they were found.
    pub fn outputs(&self) -> Vec<Output> {
        self.surfaces.iter().map(|s| s.output.clone()).collect()
    }

    pub fn renderer(&mut self) -> &mut GlesRenderer {
        &mut self.renderer
    }

    /// Switch to virtual terminal `vt` (Ctrl+Alt+F`vt`).
    pub fn switch_vt(&mut self, vt: i32) {
        info!(vt, "switching VT");
        if let Err(err) = self.session.change_vt(vt) {
            warn!(vt, %err, "VT switch failed");
        }
    }

    /// What `output`'s connector can do for variable refresh, and whether it is on now.
    pub fn adaptive_sync(&self, output: &Output) -> (vrr::Support, bool) {
        self.surfaces
            .iter()
            .find(|s| s.output == *output)
            .map_or((vrr::Support::None, false), |s| {
                (s.vrr_support, s.vrr_active)
            })
    }

    fn surface_mut(&mut self, crtc: crtc::Handle) -> Option<&mut OutputSurface> {
        self.surfaces
            .iter_mut()
            .find(|s| s.drm_output.crtc() == crtc)
    }

    /// Bring the lit connectors in line with what is connected now.
    fn sync_connectors(&mut self) -> Changes {
        let mut changes = Changes::default();
        let connected = connected_connectors(self.output_manager.device());

        // Unplugged: drop their DRM output (which turns the CRTC off).
        let before = self.surfaces.len();
        let handle = &self.loop_handle;
        self.surfaces.retain_mut(|surface| {
            let keep = connected.iter().any(|c| c.handle() == surface.connector);
            if !keep {
                info!(output = surface.output.name(), "connector unplugged");
                surface.cancel_timers(handle);
                changes.removed.push(surface.output.clone());
            }
            keep
        });
        if self.surfaces.len() != before
            && let Err(err) = self
                .output_manager
                .lock()
                .try_to_restore_modifiers::<_, TtyElement>(
                    &mut self.renderer,
                    &DrmOutputRenderElements::default(),
                )
        {
            warn!(?err, "restoring modifiers after an unplug failed");
        }

        // Still there with a different preferred mode: modeset.
        for surface in &mut self.surfaces {
            let Some(info) = connected.iter().find(|c| c.handle() == surface.connector) else {
                continue;
            };
            let Some(drm_mode) = preferred_mode(info) else {
                continue;
            };
            let mode = Mode::from(drm_mode);
            if surface.output.current_mode() == Some(mode) {
                continue;
            }
            match surface.drm_output.use_mode::<_, TtyElement>(
                drm_mode,
                &mut self.renderer,
                &DrmOutputRenderElements::default(),
            ) {
                Ok(()) => {
                    surface
                        .output
                        .change_current_state(Some(mode), None, None, None);
                    surface.output.set_preferred(mode);
                    changes.mode_changed.push(surface.output.clone());
                }
                Err(err) => warn!(output = surface.output.name(), ?err, "modeset failed"),
            }
        }

        // Newly plugged.
        for info in &connected {
            if self.surfaces.iter().any(|s| s.connector == info.handle()) {
                continue;
            }
            match self.light_connector(info) {
                Ok(surface) => {
                    changes.added.push(surface.output.clone());
                    self.surfaces.push(surface);
                }
                Err(err) => warn!(connector = ?info.interface(), %err, "cannot light connector"),
            }
        }
        changes
    }

    /// Pick a free CRTC for `info` and start scanning out on it.
    fn light_connector(&mut self, info: &connector::Info) -> Result<OutputSurface, String> {
        let used: Vec<crtc::Handle> = self.surfaces.iter().map(|s| s.drm_output.crtc()).collect();
        let drm = self.output_manager.device();
        let crtc = pick_crtc(drm, info, &used).ok_or("no free CRTC")?;
        let drm_mode = preferred_mode(info).ok_or("no modes")?;
        let mut planes = drm
            .planes(&crtc)
            .map_err(|err| format!("querying planes: {err}"))?;
        // Direct scanout uses the primary plane only (ADR 0020). Not offering overlays also
        // sidesteps the known overlay breakage on NVIDIA that anvil works around.
        planes.overlay.clear();

        let output = create_output(info, drm_mode);
        let drm_output = self
            .output_manager
            .lock()
            .initialize_output::<_, TtyElement>(
                crtc,
                drm_mode,
                &[info.handle()],
                &output,
                Some(planes),
                &mut self.renderer,
                &DrmOutputRenderElements::default(),
            )
            .map_err(|err| format!("initialising the DRM output: {err:?}"))?;
        let (plane_formats, vrr_support) = drm_output.with_compositor(|compositor| {
            let support = match compositor.vrr_supported(info.handle()) {
                Ok(VrrSupport::Supported) => vrr::Support::Seamless,
                Ok(VrrSupport::RequiresModeset) => vrr::Support::NeedsModeset,
                Ok(VrrSupport::NotSupported) => vrr::Support::None,
                Err(err) => {
                    debug!(?err, "querying VRR support failed");
                    vrr::Support::None
                }
            };
            (compositor.surface().plane_info().formats.clone(), support)
        });
        let feedback = self.render_node.and_then(|render_node| {
            scanout::output_feedback(
                render_node,
                self.node,
                &self.renderer.dmabuf_formats(),
                &plane_formats,
            )
        });
        info!(output = output.name(), ?crtc, mode = ?drm_mode, vrr = ?vrr_support, "connector lit");
        Ok(OutputSurface {
            output,
            connector: info.handle(),
            drm_output,
            redraw: Redraw::new(),
            estimated_vblank: None,
            cursor_timer: None,
            feedback,
            vrr_support,
            vrr_active: false,
            vrr_checked: None,
        })
    }
}

/// Apply `[touchpad]` to `device` if it is a touchpad (libinput says so by its tap
/// support). Settings a device does not support are skipped.
fn configure_touchpad(device: &mut input::Device, config: &TouchpadConfig) {
    use smithay::reexports::input::{ClickMethod as LibinputClick, DragLockState};
    let drag_lock = if config.drag_lock {
        DragLockState::EnabledTimeout
    } else {
        DragLockState::Disabled
    };
    let click = match config.click_method {
        ClickMethod::Clickfinger => LibinputClick::Clickfinger,
        ClickMethod::ButtonAreas => LibinputClick::ButtonAreas,
    };
    let name = device.name().into_owned();
    let results = [
        (
            "tap-to-click",
            device.config_tap_set_enabled(config.tap_to_click),
        ),
        (
            "tap-and-drag",
            device.config_tap_set_drag_enabled(config.tap_and_drag),
        ),
        (
            "drag-lock",
            device.config_tap_set_drag_lock_enabled(drag_lock),
        ),
        (
            "disable-while-typing",
            device.config_dwt_set_enabled(config.disable_while_typing),
        ),
        ("click-method", device.config_click_set_method(click)),
    ];
    for (setting, result) in results {
        if let Err(err) = result {
            debug!(device = name, setting, ?err, "touchpad setting not applied");
        }
    }
}

/// A libinput device appeared: remember it if it is a touchpad, and configure it.
fn on_device_added(state: &mut State, device: &input::Device) {
    if device.config_tap_finger_count() == 0 {
        return;
    }
    let mut device = device.clone();
    configure_touchpad(&mut device, &state.config.active.touchpad);
    info!(device = %device.name(), "touchpad configured");
    if let Some(tty) = tty(state) {
        tty.touchpads.push(device);
    }
}

impl TtyData {
    /// Draw every output again soon: something that may show on any of them changed.
    pub fn queue_redraw_all(&mut self) {
        let crtcs: Vec<crtc::Handle> = self.surfaces.iter().map(|s| s.drm_output.crtc()).collect();
        for crtc in crtcs {
            queue_redraw(self, crtc);
        }
    }

    /// Apply a changed `[touchpad]` section to the touchpads that are plugged in.
    pub fn configure_touchpads(&mut self, config: &TouchpadConfig) {
        for device in &mut self.touchpads {
            configure_touchpad(device, config);
        }
    }
}

/// Open the seat, the primary GPU and its connected displays, and register the session,
/// DRM, udev and libinput event sources on `handle`.
pub fn init(
    dh: &DisplayHandle,
    dmabuf_state: &mut DmabufState,
    handle: &LoopHandle<'static, State>,
) -> Result<Backend, Error> {
    let (mut session, session_notifier) = LibSeatSession::new().map_err(Error::init(
        "opening a libseat session (is seatd or logind running?)",
    ))?;
    let seat = session.seat();
    info!(seat, "session opened");

    // The GPU: the one the firmware booted with (boot_vga), else the first one on the seat.
    let gpu_path = match std::env::var_os("NEUE_DRM_DEVICE") {
        Some(path) => PathBuf::from(path),
        None => udev::primary_gpu(&seat)
            .ok()
            .flatten()
            .or_else(|| udev::all_gpus(&seat).ok()?.into_iter().next())
            .ok_or_else(|| Error::init("finding a GPU")("no DRM device on this seat"))?,
    };
    info!(path = %gpu_path.display(), "using GPU");

    let fd = session
        .open(
            &gpu_path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(Error::init("opening the DRM device through the session"))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (drm, drm_notifier) =
        DrmDevice::new(fd.clone(), true).map_err(Error::init("initialising DRM"))?;
    let gbm = GbmDevice::new(fd.clone()).map_err(Error::init("creating the GBM device"))?;

    // The workspace denies `unsafe_code`; these two constructors are unavoidable for a GPU
    // renderer (anvil and niri make the same calls).
    // SAFETY: this is the only EGLDisplay for this GBM device, created and owned by
    // Smithay; nothing else in the process calls eglGetPlatformDisplay/eglTerminate on it.
    #[allow(unsafe_code)] // see SAFETY
    let egl_display =
        unsafe { EGLDisplay::new(gbm.clone()) }.map_err(Error::init("creating the EGL display"))?;
    let context = EGLContext::new(&egl_display).map_err(Error::init("creating the EGL context"))?;
    // SAFETY: the context was just created and is only ever used from this thread (the
    // compositor is single-threaded, AGENTS.md).
    #[allow(unsafe_code)] // see SAFETY
    let renderer =
        unsafe { GlesRenderer::new(context) }.map_err(Error::init("creating the GLES renderer"))?;

    let render_node = EGLDevice::device_for_display(&egl_display)
        .ok()
        .and_then(|device| device.try_get_render_node().ok().flatten())
        .or_else(|| {
            DrmNode::from_file(&fd)
                .ok()?
                .node_with_type(NodeType::Render)?
                .ok()
        });

    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(gbm.clone(), render_node.into());
    let render_formats: FormatSet = renderer.egl_context().dmabuf_render_formats().clone();

    let node = DrmNode::from_file(&fd).map_err(Error::init("identifying the DRM device"))?;
    let output_manager = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        COLOR_FORMATS,
        render_formats,
    );
    let dmabuf_global = create_dmabuf_global(dh, dmabuf_state, &renderer, render_node);
    // Explicit sync needs to poll timeline points, which takes `syncobj_eventfd` (Linux
    // 6.6). Without it the global stays away and clients use implicit sync.
    let syncobj_state = if supports_syncobj_eventfd(&fd) {
        info!("explicit sync (linux-drm-syncobj-v1) enabled");
        Some(DrmSyncobjState::new::<State>(dh, fd.clone()))
    } else {
        info!("GPU lacks syncobj_eventfd; explicit sync disabled");
        None
    };

    // Input: libinput on the same seat, opening devices through the session.
    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat)
        .map_err(|()| Error::init("assigning libinput to the seat")("udev_assign_seat failed"))?;

    handle
        .insert_source(
            LibinputInputBackend::new(libinput.clone()),
            |event, _, state| {
                match &event {
                    InputEvent::DeviceAdded { device } => on_device_added(state, device),
                    InputEvent::DeviceRemoved { device } => {
                        if let Some(tty) = tty(state) {
                            tty.touchpads.retain(|known| known != device);
                        }
                    }
                    _ => {}
                }
                state.process_input_event(event);
                // The pointer moved, a title-bar button lit up, a window was dragged…
                if let Some(tty) = tty(state) {
                    tty.queue_redraw_all();
                }
            },
        )
        .map_err(|e| e.error)
        .map_err(Error::init("adding the libinput source"))?;
    handle
        .insert_source(session_notifier, |event, _, state| {
            on_session_event(event, state)
        })
        .map_err(|e| e.error)
        .map_err(Error::init("adding the session source"))?;
    let drm_source = handle
        .insert_source(drm_notifier, |event, metadata, state| match event {
            DrmEvent::VBlank(crtc) => on_vblank(state, crtc, metadata.take()),
            DrmEvent::Error(err) => error!(?err, "DRM error"),
        })
        .map_err(|e| e.error)
        .map_err(Error::init("adding the DRM event source"))?;
    // Connector hotplug arrives as a udev "change" event on the GPU.
    let udev = UdevBackend::new(&seat).map_err(Error::init("watching udev for hotplug"))?;
    let udev_source = handle
        .insert_source(udev, |event, _, state| on_udev_event(event, state))
        .map_err(|e| e.error)
        .map_err(Error::init("adding the udev event source"))?;

    let mut data = TtyData {
        session,
        libinput,
        loop_handle: handle.clone(),
        drm_source: Some(drm_source),
        udev_source: Some(udev_source),
        node,
        render_node,
        renderer,
        output_manager,
        surfaces: Vec::new(),
        _dmabuf_global: dmabuf_global,
        syncobj_state,
        cursors: cursor::Themes::from_env(),
        active: true,
        touchpads: Vec::new(),
    };
    // Initial outputs; `State::new` adds them to the layout.
    let changes = data.sync_connectors();
    if changes.added.is_empty() {
        warn!("no connected display yet; waiting for a hotplug");
    }
    // First frames once the state exists.
    handle.insert_idle(|state| {
        if let Some(tty) = tty(state) {
            tty.queue_redraw_all();
        }
    });
    info!(outputs = ?data.outputs().iter().map(Output::name).collect::<Vec<_>>(), "tty backend ready");
    Ok(Backend::Tty(Box::new(data)))
}

/// The connected connectors that have at least one mode.
fn connected_connectors(drm: &DrmDevice) -> Vec<connector::Info> {
    let resources = match drm.resource_handles() {
        Ok(resources) => resources,
        Err(err) => {
            warn!(%err, "reading DRM resources failed");
            return Vec::new();
        }
    };
    resources
        .connectors()
        .iter()
        .filter_map(|&handle| match drm.get_connector(handle, true) {
            Ok(info) => Some(info),
            Err(err) => {
                warn!(?handle, %err, "skipping connector");
                None
            }
        })
        .filter(|info| info.state() == connector::State::Connected && !info.modes().is_empty())
        .collect()
}

/// A CRTC that can drive `info` and is not in `used`: the one it is already on (no
/// flicker), else any it supports.
fn pick_crtc(
    drm: &DrmDevice,
    info: &connector::Info,
    used: &[crtc::Handle],
) -> Option<crtc::Handle> {
    let resources = drm.resource_handles().ok()?;
    let current = info
        .current_encoder()
        .and_then(|encoder| drm.get_encoder(encoder).ok())
        .and_then(|encoder| encoder.crtc())
        .filter(|crtc| !used.contains(crtc));
    let crtc = current.or_else(|| {
        info.encoders()
            .iter()
            .filter_map(|&encoder| drm.get_encoder(encoder).ok())
            .flat_map(|encoder| resources.filter_crtcs(encoder.possible_crtcs()))
            .find(|crtc| !used.contains(crtc))
    });
    debug!(connector = ?info.interface(), ?crtc, "picked CRTC");
    crtc
}

/// The connector's preferred mode, else its first one.
fn preferred_mode(connector: &connector::Info) -> Option<smithay::reexports::drm::control::Mode> {
    let modes = connector.modes();
    modes
        .iter()
        .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .or_else(|| modes.first())
        .copied()
}

fn create_output(
    connector: &connector::Info,
    drm_mode: smithay::reexports::drm::control::Mode,
) -> Output {
    let name = format!(
        "{}-{}",
        connector.interface().as_str(),
        connector.interface_id()
    );
    let (width_mm, height_mm) = connector.size().unwrap_or((0, 0));
    // EDID make/model/serial needs libdisplay-info (a C library); until an ADR adds it the
    // output only has its connector name.
    let output = Output::new(
        name,
        PhysicalProperties {
            size: (
                i32::try_from(width_mm).unwrap_or(0),
                i32::try_from(height_mm).unwrap_or(0),
            )
                .into(),
            subpixel: connector.subpixel().into(),
            make: "Unknown".into(),
            model: "Unknown".into(),
            serial_number: "Unknown".into(),
        },
    );
    let mode = Mode::from(drm_mode);
    output.change_current_state(Some(mode), None, None, None);
    output.set_preferred(mode);
    output
}

/// linux-dmabuf v4 with feedback pointing at our render node; v3 without one.
fn create_dmabuf_global(
    dh: &DisplayHandle,
    dmabuf_state: &mut DmabufState,
    renderer: &GlesRenderer,
    render_node: Option<DrmNode>,
) -> DmabufGlobal {
    let formats = renderer.dmabuf_formats();
    let feedback = render_node.and_then(|node| scanout::render_feedback(node, &formats));
    match feedback {
        Some(feedback) => dmabuf_state.create_global_with_default_feedback::<State>(dh, &feedback),
        None => {
            warn!("no render node; linux-dmabuf falls back to v3");
            dmabuf_state.create_global::<State>(dh, formats)
        }
    }
}

fn tty(state: &mut State) -> Option<&mut TtyData> {
    match &mut state.backend {
        Backend::Tty(data) => Some(data),
        #[allow(unreachable_patterns)] // only the tty variant exists in tty-only builds
        _ => None,
    }
}

fn on_session_event(event: SessionEvent, state: &mut State) {
    let Some(tty) = tty(state) else { return };
    match event {
        SessionEvent::PauseSession => {
            info!("session paused (switched away)");
            tty.active = false;
            tty.libinput.suspend();
            tty.output_manager.pause();
        }
        SessionEvent::ActivateSession => {
            info!("session resumed");
            tty.active = true;
            if let Err(err) = tty.libinput.resume() {
                error!(?err, "resuming libinput failed");
            }
            // Keep the connectors as they are (no flicker). Another DRM master may have
            // left a different state; the first commit then fails its test and Smithay
            // falls back to a full modeset.
            if let Err(err) = tty.output_manager.lock().activate(false) {
                error!(%err, "reactivating DRM failed");
            }
            let mut crtcs = Vec::new();
            for surface in &mut tty.surfaces {
                // The vblank for a frame queued before the switch never arrives.
                surface.cancel_timers(&tty.loop_handle);
                surface.drm_output.reset_buffers();
                if surface.redraw.reset() {
                    crtcs.push(surface.drm_output.crtc());
                }
            }
            for crtc in crtcs {
                schedule_render(tty, crtc);
            }
            // Displays may have been plugged or unplugged while we were away.
            rescan(state);
        }
    }
}

fn on_udev_event(event: UdevEvent, state: &mut State) {
    let Some(tty) = tty(state) else { return };
    match event {
        UdevEvent::Changed { device_id } if device_id == tty.node.dev_id() => {
            debug!("GPU changed: rescanning connectors");
            rescan(state);
        }
        // One GPU for now (several GPUs are a later task).
        _ => {}
    }
}

/// Apply connector changes to the layout and redraw every output.
fn rescan(state: &mut State) {
    let Some(data) = tty(state) else { return };
    if !data.active {
        return; // ActivateSession rescans
    }
    let changes = data.sync_connectors();
    for output in &changes.removed {
        state.remove_output(output);
    }
    for output in &changes.mode_changed {
        state.output_mode_changed(output);
    }
    for output in changes.added {
        state.add_output(output);
    }
    if let Some(data) = tty(state) {
        data.queue_redraw_all();
    }
}

fn on_vblank(state: &mut State, crtc: crtc::Handle, metadata: Option<DrmEventMetadata>) {
    // The kernel's flip timestamp when it gives a monotonic one, else now.
    let (time, flags) = match metadata.map(|m| (m.time, m.sequence)) {
        Some((DrmEventTime::Monotonic(time), _)) if !time.is_zero() => (
            time.into(),
            wp_presentation_feedback::Kind::Vsync
                | wp_presentation_feedback::Kind::HwClock
                | wp_presentation_feedback::Kind::HwCompletion,
        ),
        _ => (state.clock.now(), wp_presentation_feedback::Kind::Vsync),
    };
    let sequence = metadata.map_or(0, |m| u64::from(m.sequence));
    let Some(tty) = tty(state) else { return };
    let Some(surface) = tty.surface_mut(crtc) else {
        return; // unplugged since the frame was queued
    };
    let again = surface.redraw.vblank(time.into());
    let refresh = surface
        .output
        .current_mode()
        .map_or(Refresh::Unknown, |mode| {
            match crate::presentation::refresh_interval(mode.refresh) {
                // The mode's rate is the fastest a variable-rate output goes.
                Refresh::Fixed(interval) if surface.vrr_active => Refresh::Variable(interval),
                refresh => refresh,
            }
        });
    match surface.drm_output.frame_submitted() {
        Ok(Some(mut feedback)) => feedback.presented(time, refresh, sequence, flags),
        Ok(None) => {}
        Err(err) => warn!(?err, "frame_submitted failed"),
    }
    if again {
        schedule_render(tty, crtc);
    }
}

/// Mark `crtc`'s output as changed and schedule a render if it was idle.
fn queue_redraw(tty: &mut TtyData, crtc: crtc::Handle) {
    if tty
        .surface_mut(crtc)
        .is_some_and(|surface| surface.redraw.queue())
    {
        schedule_render(tty, crtc);
    }
}

/// Run [`render`] for `crtc` in the next loop iteration, after the clients' requests of
/// this one are dispatched (an idle callback would run before them).
fn schedule_render(tty: &mut TtyData, crtc: crtc::Handle) {
    let result = tty
        .loop_handle
        .insert_source(Timer::immediate(), move |_, _, state| {
            render(state, crtc);
            TimeoutAction::Drop
        });
    if let Err(err) = result {
        error!(err = %err.error, "scheduling a render failed");
        if let Some(surface) = tty.surface_mut(crtc) {
            surface.redraw.skip_render();
        }
    }
}

/// The output's refresh interval (60 Hz when the mode does not say).
fn refresh_interval(output: &Output) -> Duration {
    let refresh_mhz = output
        .current_mode()
        .map_or(60_000, |mode| mode.refresh)
        .max(1);
    Duration::from_nanos(1_000_000_000_000 / u64::from(refresh_mhz.unsigned_abs()))
}

/// The estimated vblank after an empty frame: send that frame's callbacks, then render
/// again if something changed meanwhile.
fn on_estimated_vblank(state: &mut State, crtc: crtc::Handle) {
    let Some(tty) = tty(state) else { return };
    let Some(surface) = tty.surface_mut(crtc) else {
        return;
    };
    surface.estimated_vblank = None;
    let again = surface.redraw.estimated_vblank_passed();
    let output = surface.output.clone();
    send_frame_callbacks(state, &output);
    if again && let Some(tty) = self::tty(state) {
        schedule_render(tty, crtc);
    }
}

/// Frame callbacks for everything shown on `output`, so clients draw their next frame.
fn send_frame_callbacks(state: &State, output: &Output) {
    let time = state.start_time.elapsed();
    crate::outputs::send_frames(&state.space, output, time);
    let Some(output_geo) = state.space.output_geometry(output) else {
        return;
    };
    if !output_geo
        .to_f64()
        .contains(state.pointer.current_location())
    {
        return;
    }
    let cursor = match &state.cursor_status {
        CursorImageStatus::Surface(surface) => Some(surface),
        _ => None,
    };
    let icon = state
        .dnd_icon
        .as_ref()
        .map(|icon| &icon.surface)
        .filter(|surface| surface.alive());
    for surface in cursor.into_iter().chain(icon) {
        send_frames_surface_tree(surface, output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
}

/// Set the CRTC's variable refresh to what `mode` asks for (`covered`: a full-screen window
/// covers the output) from the next frame on. True if that changed it.
fn update_vrr(
    surface: &mut OutputSurface,
    mode: neue_config::compositor::AdaptiveSync,
    covered: bool,
) -> bool {
    if surface.vrr_checked != Some(mode) {
        surface.vrr_checked = Some(mode);
        if let Some(reason) = vrr::unmet(mode, surface.vrr_support) {
            warn!(
                output = surface.output.name(),
                ?mode,
                reason,
                "adaptive sync not applied"
            );
        }
    }
    let wanted = vrr::wanted(mode, surface.vrr_support, covered);
    if wanted == surface.vrr_active {
        return false;
    }
    match surface
        .drm_output
        .with_compositor(|compositor| compositor.use_vrr(wanted))
    {
        Ok(()) => {
            info!(
                output = surface.output.name(),
                active = wanted,
                "adaptive sync switched"
            );
            surface.vrr_active = wanted;
            true
        }
        Err(err) => {
            // Treat the connector as unable, so this is not retried every frame.
            if surface.vrr_support != vrr::Support::None {
                warn!(
                    output = surface.output.name(),
                    ?err,
                    "switching adaptive sync failed"
                );
            }
            surface.vrr_support = vrr::Support::None;
            false
        }
    }
}

/// Compose a frame for `crtc` and queue it for its next vblank. Without damage nothing is
/// queued and the frame callbacks wait for the estimated vblank.
fn render(state: &mut State, crtc: crtc::Handle) {
    let time = state.start_time.elapsed();
    let now: Duration = state.clock.now().into();
    let pointer_location = state.pointer.current_location();
    let State {
        backend,
        space,
        cursor_status,
        dnd_icon,
        chrome,
        config,
        ..
    } = state;
    let Backend::Tty(tty) = backend else { return };
    let active = tty.active;
    let TtyData {
        surfaces,
        renderer,
        cursors,
        ..
    } = &mut **tty;
    let Some(surface) = surfaces.iter_mut().find(|s| s.drm_output.crtc() == crtc) else {
        return;
    };
    let output = surface.output.clone();
    let Some(output_geo) = space.output_geometry(&output).filter(|_| active) else {
        // Inactive (ActivateSession resets) or not in the layout yet (add_output queues).
        surface.redraw.skip_render();
        return;
    };
    if !surface.redraw.start_render() {
        return;
    }
    let scale = Scale::from(output.current_scale().fractional_scale());

    // Topmost first: the cursor (if it is on this output), then the windows.
    let mut elements: Vec<TtyElement> = Vec::new();
    if let CursorImageStatus::Surface(surface) = &*cursor_status
        && !surface.alive()
    {
        *cursor_status = CursorImageStatus::default_named();
    }
    // Generous bounds, so a cursor straddling two outputs shows on both.
    let cursor_near = output_geo.overlaps(Rectangle::new(
        pointer_location.to_i32_floor() - Point::from((CURSOR_REACH, CURSOR_REACH)),
        (2 * CURSOR_REACH, 2 * CURSOR_REACH).into(),
    ));
    let cursor_pos = pointer_location - output_geo.loc.to_f64();
    // When an animated cursor shows its next image.
    let mut cursor_change = None;
    match &*cursor_status {
        CursorImageStatus::Hidden => {}
        _ if !cursor_near => {}
        CursorImageStatus::Named(icon) => {
            let frame = cursors.frame(*icon, scale.x, time);
            let location: Point<f64, Physical> =
                cursor_pos.to_physical(scale) - frame.hotspot.to_f64();
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                location,
                &frame.buffer,
                None,
                None,
                Some(frame.logical_size(scale.x)),
                Kind::Cursor,
            ) {
                Ok(element) => elements.push(TtyElement::Cursor(element)),
                Err(err) => warn!(?err, "uploading the cursor failed"),
            }
            cursor_change = cursors.until_next_frame(*icon, scale.x, time);
        }
        CursorImageStatus::Surface(surface) => {
            let hotspot = with_states(surface, |states| {
                states
                    .data_map
                    .get::<CursorImageSurfaceData>()
                    .and_then(|data| data.lock().ok().map(|attrs| attrs.hotspot))
                    .unwrap_or_default()
            });
            let location = (cursor_pos - hotspot.to_f64())
                .to_physical(scale)
                .to_i32_round();
            elements.extend(render_elements_from_surface_tree::<_, TtyElement>(
                renderer,
                surface,
                location,
                scale,
                1.0,
                Kind::Cursor,
            ));
        }
    }
    // The drag icon rides just under the cursor.
    if let Some(icon) = dnd_icon.as_ref().filter(|i| i.surface.alive())
        && cursor_near
    {
        let location = (cursor_pos + icon.offset.to_f64())
            .to_physical(scale)
            .to_i32_round();
        elements.extend(render_elements_from_surface_tree::<_, TtyElement>(
            renderer,
            &icon.surface,
            location,
            scale,
            1.0,
            Kind::Unspecified,
        ));
    }
    elements.extend(
        crate::render::output_elements(renderer, space, chrome, &output)
            .into_iter()
            .map(TtyElement::Output),
    );

    let vrr_mode = config
        .active
        .outputs
        .get(&output.name())
        .map_or_else(Default::default, |c| c.adaptive_sync);
    let covered = !crate::lock::with_output_lock(&output, |lock| lock.locked)
        && crate::fullscreen::fullscreen_window_on(space, &output).is_some();
    let vrr_changed = update_vrr(surface, vrr_mode, covered);

    let rendered =
        match surface
            .drm_output
            .render_frame(renderer, &elements, CLEAR_COLOR, FRAME_FLAGS)
        {
            Ok(result) => {
                crate::presentation::update_primary_scanout_output(
                    space,
                    &output,
                    cursor_status,
                    dnd_icon.as_ref(),
                    &result.states,
                );
                if let Some(feedback) = &surface.feedback {
                    scanout::send_feedback(space, &output, &result.states, feedback);
                }
                // Feedback goes with the frame only if there is one: without damage the
                // callbacks wait for a later frame (or are discarded by the next commit).
                (!result.is_empty)
                    .then(|| crate::presentation::take_feedback(space, &output, &result.states))
            }
            Err(err) => {
                warn!(?err, "rendering failed");
                None
            }
        };
    let queued = rendered.is_some_and(|feedback| match surface.drm_output.queue_frame(feedback) {
        Ok(()) => true,
        Err(err) => {
            warn!(?err, "queueing the frame failed");
            false
        }
    });
    surface.redraw.rendered(queued);
    trace!(
        output = output.name(),
        queued,
        renders = surface.redraw.renders(),
        frames = surface.redraw.frames(),
        "rendered"
    );
    let estimate = (!queued).then(|| {
        surface
            .redraw
            .until_estimated_vblank(now, refresh_interval(&output))
    });

    // A real frame: callbacks now, so clients draw the next one while this one scans out.
    if queued {
        send_frame_callbacks(state, &output);
    }
    if vrr_changed {
        state.report_outputs();
    }
    let Some(tty) = self::tty(state) else { return };
    let handle = tty.loop_handle.clone();
    let Some(surface) = tty.surface_mut(crtc) else {
        return;
    };
    if let Some(delay) = estimate {
        let result = handle.insert_source(Timer::from_duration(delay), move |_, _, state| {
            on_estimated_vblank(state, crtc);
            TimeoutAction::Drop
        });
        match result {
            Ok(token) => surface.estimated_vblank = Some(token),
            Err(err) => {
                // Without the timer the output would wait forever: go idle instead.
                error!(err = %err.error, "arming the estimated vblank failed");
                surface.redraw.estimated_vblank_passed();
            }
        }
    }
    if let Some(delay) = cursor_change
        && surface.cursor_timer.is_none()
    {
        let result = handle.insert_source(Timer::from_duration(delay), move |_, _, state| {
            if let Some(tty) = self::tty(state) {
                if let Some(surface) = tty.surface_mut(crtc) {
                    surface.cursor_timer = None;
                }
                queue_redraw(tty, crtc);
            }
            TimeoutAction::Drop
        });
        match result {
            Ok(token) => surface.cursor_timer = Some(token),
            Err(err) => warn!(err = %err.error, "arming the cursor animation timer failed"),
        }
    }
}
