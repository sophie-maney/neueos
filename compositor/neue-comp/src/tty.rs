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
//! Several GPUs, direct scanout and VRR are task 1.15. The pointer (XCursor theme, task 1.9) is
//! drawn on the cursor plane where the hardware has one (the DRM compositor falls back to
//! compositing it otherwise).
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
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType};
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
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::{Mode, Output, PhysicalProperties};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{Device as _, ModeTypeFlags, connector, crtc};
use smithay::reexports::input::{self, Libinput};
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{DeviceFd, IsAlive, Physical, Point, Rectangle, Scale};
use smithay::wayland::compositor::with_states;
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState};
use tracing::{debug, error, info, warn};

use crate::Error;
use crate::cursor;
use crate::state::{Backend, State};

/// Background behind all windows until the desktop picture lands (Phase 3). Same as winit.
const CLEAR_COLOR: [f32; 4] = [0.18, 0.2, 0.24, 1.0];
/// Primary plane formats, tried in order. 8-bit only until colour management (PLAN §3.1).
const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Abgr8888];
/// Overlay and primary-plane scanout of client buffers is task 1.15; until then only the
/// cursor goes on its own plane and everything else is composited.
const FRAME_FLAGS: FrameFlags = FrameFlags::ALLOW_CURSOR_PLANE_SCANOUT;
/// How far (logical pixels) from an output the pointer can be and still have its image
/// drawn there: more than any cursor is tall.
const CURSOR_REACH: i32 = 128;

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
/// Per-frame user data handed back on vblank. Presentation feedback is task 1.14.
type FrameData = ();
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
    /// A frame was queued and its vblank has not arrived yet.
    frame_pending: bool,
    /// A render is already scheduled (idle callback or timer).
    render_scheduled: bool,
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
    /// The GPU, to match udev events.
    node: DrmNode,
    _dmabuf_global: DmabufGlobal,
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
        self.surfaces.retain(|surface| {
            let keep = connected.iter().any(|c| c.handle() == surface.connector);
            if !keep {
                info!(output = surface.output.name(), "connector unplugged");
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
        // Overlays are for direct scanout (task 1.15). Not offering them also sidesteps the
        // known overlay breakage on NVIDIA that anvil works around.
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
        info!(output = output.name(), ?crtc, mode = ?drm_mode, "connector lit");
        Ok(OutputSurface {
            output,
            connector: info.handle(),
            drm_output,
            frame_pending: false,
            render_scheduled: false,
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
        .insert_source(drm_notifier, |event, _, state| match event {
            DrmEvent::VBlank(crtc) => on_vblank(state, crtc),
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
        renderer,
        output_manager,
        surfaces: Vec::new(),
        _dmabuf_global: dmabuf_global,
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
            schedule_all(tty);
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
    let feedback = render_node.and_then(|node| {
        DmabufFeedbackBuilder::new(node.dev_id(), formats.clone())
            .build()
            .map_err(|err| warn!(?err, "building dmabuf feedback failed"))
            .ok()
    });
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
            for surface in &mut tty.surfaces {
                // The vblank for a frame queued before the switch never arrives.
                surface.frame_pending = false;
                surface.drm_output.reset_buffers();
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
        // One GPU for now (task 1.15 adds more).
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
        schedule_all(data);
    }
}

fn on_vblank(state: &mut State, crtc: crtc::Handle) {
    let Some(tty) = tty(state) else { return };
    let Some(surface) = tty.surface_mut(crtc) else {
        return; // unplugged since the frame was queued
    };
    surface.frame_pending = false;
    if let Err(err) = surface.drm_output.frame_submitted() {
        warn!(?err, "frame_submitted failed");
    }
    schedule_render(tty, crtc, Duration::ZERO);
}

/// Render every output soon (after hotplug or a session switch: the layout may have moved).
fn schedule_all(tty: &mut TtyData) {
    let crtcs: Vec<crtc::Handle> = tty.surfaces.iter().map(|s| s.drm_output.crtc()).collect();
    for crtc in crtcs {
        schedule_render(tty, crtc, Duration::ZERO);
    }
}

/// Run [`render`] for `crtc` after `delay`, unless a render is already scheduled.
fn schedule_render(tty: &mut TtyData, crtc: crtc::Handle, delay: Duration) {
    let handle = tty.loop_handle.clone();
    let Some(surface) = tty.surface_mut(crtc) else {
        return;
    };
    if surface.render_scheduled {
        return;
    }
    surface.render_scheduled = true;
    let result = handle.insert_source(Timer::from_duration(delay), move |_, _, state| {
        render(state, crtc);
        TimeoutAction::Drop
    });
    if let Err(err) = result {
        surface.render_scheduled = false;
        error!(err = %err.error, "scheduling a render failed");
    }
}

/// Compose a frame for `crtc` and queue it for its next vblank. Without damage nothing is
/// queued and the next attempt is one refresh interval later (as in anvil; render-on-commit
/// and a damage review come with task 1.15).
fn render(state: &mut State, crtc: crtc::Handle) {
    let time = state.start_time.elapsed();
    let pointer_location = state.pointer.current_location();
    let State {
        backend,
        space,
        cursor_status,
        dnd_icon,
        chrome,
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
    surface.render_scheduled = false;
    if !active || surface.frame_pending {
        return;
    }

    let output = surface.output.clone();
    let scale = Scale::from(output.current_scale().fractional_scale());
    let Some(output_geo) = space.output_geometry(&output) else {
        return; // not in the layout yet
    };

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

    let rendered =
        match surface
            .drm_output
            .render_frame(renderer, &elements, CLEAR_COLOR, FRAME_FLAGS)
        {
            Ok(result) => !result.is_empty,
            Err(err) => {
                warn!(?err, "rendering failed");
                false
            }
        };
    if rendered {
        match surface.drm_output.queue_frame(()) {
            Ok(()) => surface.frame_pending = true,
            Err(err) => warn!(?err, "queueing the frame failed"),
        }
    }
    let frame_pending = surface.frame_pending;

    // Frame callbacks, so clients draw their next frame.
    crate::outputs::send_frames(space, &output, time);
    if let CursorImageStatus::Surface(surface) = &*cursor_status
        && output_geo.to_f64().contains(pointer_location)
    {
        send_frames_surface_tree(surface, &output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }

    if let Some(icon) = dnd_icon.as_ref().filter(|i| i.surface.alive())
        && output_geo.to_f64().contains(pointer_location)
    {
        send_frames_surface_tree(
            &icon.surface,
            &output,
            time,
            Some(Duration::ZERO),
            |_, _| Some(output.clone()),
        );
    }

    if !frame_pending {
        let refresh_mhz = output.current_mode().map_or(60_000, |mode| mode.refresh);
        let interval = Duration::from_micros(1_000_000_000 / u64::from(refresh_mhz.max(1) as u32));
        schedule_render(tty, crtc, interval);
    }
}
