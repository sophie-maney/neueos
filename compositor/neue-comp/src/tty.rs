//! tty backend: the compositor owns a seat, a GPU and its displays.
//!
//! DRM/KMS (atomic, through Smithay's `DrmOutputManager`), GBM buffers, a GLES renderer on
//! EGL/GBM, libinput for input devices, udev to find the GPU, and libseat (seatd or logind)
//! for device access and VT switching (Ctrl+Alt+F1…F12).
//!
//! Scope of task 1.4: one GPU, one output (the first connected connector, at its preferred
//! mode), no hotplug. Multi-monitor and hotplug are task 1.7, direct scanout and VRR 1.15,
//! XCursor themes 1.9. The pointer is drawn on the cursor plane where the hardware has one
//! (the DRM compositor falls back to compositing it otherwise).
//!
//! `NEUE_DRM_DEVICE=/dev/dri/cardN` overrides the GPU choice.
//!
//! Adapted from Smithay's `anvil/src/udev.rs` (MIT), reduced to a single GPU and output.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::FrameFlags;
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType};
use smithay::backend::egl::{EGLContext, EGLDevice, EGLDisplay};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev;
use smithay::desktop::space::{SpaceRenderElements, space_render_elements};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::{Mode, Output, PhysicalProperties};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, RegistrationToken};
use smithay::reexports::drm::control::{Device as _, ModeTypeFlags, connector, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{DeviceFd, IsAlive, Physical, Point, Scale};
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

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
/// Per-frame user data handed back on vblank. Presentation feedback is task 1.14.
type FrameData = ();
type OutputManager = DrmOutputManager<Allocator, Exporter, FrameData, DrmDeviceFd>;
type TtyOutput = DrmOutput<Allocator, Exporter, FrameData, DrmDeviceFd>;

smithay::backend::renderer::element::render_elements! {
    /// Everything drawn on a tty output, topmost first.
    pub TtyElement<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    Cursor=MemoryRenderBufferRenderElement<GlesRenderer>,
    CursorSurface=WaylandSurfaceRenderElement<GlesRenderer>,
}

/// State owned by the tty backend.
///
/// Drop order matters: the DRM device restores the previous display state when its last
/// handle goes, which needs DRM master, so the session (which holds it) must outlive every
/// handle. Fields drop in declaration order and the session is last; the DRM event source
/// also holds a handle and is removed from the loop in `Drop`.
pub struct TtyData {
    drm_output: TtyOutput,
    output_manager: OutputManager,
    renderer: GlesRenderer,
    loop_handle: LoopHandle<'static, State>,
    drm_source: Option<RegistrationToken>,
    output: Output,
    _dmabuf_global: DmabufGlobal,
    /// The built-in arrow, used until XCursor themes (task 1.9).
    default_cursor: MemoryRenderBuffer,
    /// The session is active (our VT is in front). False while switched away.
    active: bool,
    /// A frame was queued and its vblank has not arrived yet.
    frame_pending: bool,
    /// A render is already scheduled (idle callback or timer).
    render_scheduled: bool,
    libinput: Libinput,
    session: LibSeatSession,
}

impl fmt::Debug for TtyData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TtyData")
            .field("seat", &self.session.seat())
            .field("output", &self.output.name())
            .field("active", &self.active)
            .field("frame_pending", &self.frame_pending)
            .finish_non_exhaustive()
    }
}

impl Drop for TtyData {
    fn drop(&mut self) {
        if let Some(token) = self.drm_source.take() {
            self.loop_handle.remove(token);
        }
    }
}

impl TtyData {
    pub fn output(&self) -> &Output {
        &self.output
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
}

/// Open the seat, the primary GPU and its first connected display, and register the
/// session, DRM and libinput event sources on `handle`.
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
    let mut renderer =
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

    let (connector, crtc) = pick_connector(&drm)?;
    let drm_mode = preferred_mode(&connector)
        .ok_or_else(|| Error::init("choosing a display mode")("the connector has no modes"))?;
    let mut planes = drm
        .planes(&crtc)
        .map_err(Error::init("querying the CRTC's planes"))?;
    // Overlays are for direct scanout (task 1.15). Not offering them also sidesteps the
    // known overlay breakage on NVIDIA that anvil works around.
    planes.overlay.clear();

    let output = create_output(dh, &connector, drm_mode);
    let mut output_manager = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        COLOR_FORMATS,
        render_formats,
    );
    let drm_output = output_manager
        .lock()
        .initialize_output::<_, TtyElement>(
            crtc,
            drm_mode,
            &[connector.handle()],
            &output,
            Some(planes),
            &mut renderer,
            &DrmOutputRenderElements::default(),
        )
        .map_err(|err| format!("{err:?}"))
        .map_err(Error::init("initialising the DRM output"))?;

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
            |event, _, state| state.process_input_event(event),
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
            DrmEvent::VBlank(_crtc) => on_vblank(state),
            DrmEvent::Error(err) => error!(?err, "DRM error"),
        })
        .map_err(|e| e.error)
        .map_err(Error::init("adding the DRM event source"))?;
    // First frame once the state exists.
    handle.insert_idle(render);

    let data = TtyData {
        session,
        libinput,
        loop_handle: handle.clone(),
        drm_source: Some(drm_source),
        renderer,
        output_manager,
        drm_output,
        output,
        _dmabuf_global: dmabuf_global,
        default_cursor: cursor::default_cursor(),
        active: true,
        frame_pending: false,
        render_scheduled: true,
    };
    info!(output = data.output.name(), mode = ?drm_mode, "tty backend ready");
    Ok(Backend::Tty(Box::new(data)))
}

/// The first connected connector with at least one mode, and a CRTC that can drive it.
fn pick_connector(drm: &DrmDevice) -> Result<(connector::Info, crtc::Handle), Error> {
    let resources = drm
        .resource_handles()
        .map_err(Error::init("reading DRM resources"))?;
    for &handle in resources.connectors() {
        let info = match drm.get_connector(handle, true) {
            Ok(info) => info,
            Err(err) => {
                warn!(?handle, %err, "skipping connector");
                continue;
            }
        };
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        // Prefer the CRTC the connector is already on (no flicker), else any it supports.
        let current = info
            .current_encoder()
            .and_then(|encoder| drm.get_encoder(encoder).ok())
            .and_then(|encoder| encoder.crtc());
        let crtc = current.or_else(|| {
            info.encoders()
                .iter()
                .filter_map(|&encoder| drm.get_encoder(encoder).ok())
                .flat_map(|encoder| resources.filter_crtcs(encoder.possible_crtcs()))
                .next()
        });
        match crtc {
            Some(crtc) => {
                debug!(connector = ?info.interface(), ?crtc, "picked connector");
                return Ok((info, crtc));
            }
            None => warn!(connector = ?info.interface(), "no CRTC for connector"),
        }
    }
    Err(Error::init("finding a display")(
        "no connected connector with a usable CRTC",
    ))
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
    dh: &DisplayHandle,
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
    // The output global lives as long as the display; we never remove it (hotplug is 1.7).
    let _global = output.create_global::<State>(dh);
    let mode = Mode::from(drm_mode);
    output.change_current_state(Some(mode), None, None, Some((0, 0).into()));
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
            // The vblank for a frame queued before the switch never arrives.
            tty.frame_pending = false;
            tty.drm_output.reset_buffers();
            schedule_render(tty, Duration::ZERO);
        }
    }
}

fn on_vblank(state: &mut State) {
    let Some(tty) = tty(state) else { return };
    tty.frame_pending = false;
    if let Err(err) = tty.drm_output.frame_submitted() {
        warn!(?err, "frame_submitted failed");
    }
    schedule_render(tty, Duration::ZERO);
}

/// Run [`render`] after `delay`, unless a render is already scheduled.
fn schedule_render(tty: &mut TtyData, delay: Duration) {
    if tty.render_scheduled {
        return;
    }
    tty.render_scheduled = true;
    let result = tty
        .loop_handle
        .insert_source(Timer::from_duration(delay), |_, _, state| {
            render(state);
            TimeoutAction::Drop
        });
    if let Err(err) = result {
        tty.render_scheduled = false;
        error!(err = %err.error, "scheduling a render failed");
    }
}

/// Compose a frame and queue it for the next vblank. Without damage nothing is queued and
/// the next attempt is one refresh interval later (as in anvil; render-on-commit and a
/// damage review come with task 1.15).
fn render(state: &mut State) {
    let time = state.start_time.elapsed();
    let pointer_location = state.pointer.current_location();
    let State {
        backend,
        space,
        cursor_status,
        ..
    } = state;
    let Backend::Tty(tty) = backend else { return };
    tty.render_scheduled = false;
    if !tty.active || tty.frame_pending {
        return;
    }

    let output = tty.output.clone();
    let scale = Scale::from(output.current_scale().fractional_scale());
    let output_origin = space
        .output_geometry(&output)
        .map(|geometry| geometry.loc)
        .unwrap_or_default();

    // Topmost first: the cursor, then the windows.
    let mut elements: Vec<TtyElement> = Vec::new();
    if let CursorImageStatus::Surface(surface) = &*cursor_status
        && !surface.alive()
    {
        *cursor_status = CursorImageStatus::default_named();
    }
    let cursor_pos = pointer_location - output_origin.to_f64();
    match &*cursor_status {
        CursorImageStatus::Hidden => {}
        CursorImageStatus::Named(_) => {
            let location: Point<f64, Physical> =
                (cursor_pos - cursor::HOTSPOT.to_f64()).to_physical(scale);
            match MemoryRenderBufferRenderElement::from_buffer(
                &mut tty.renderer,
                location,
                &tty.default_cursor,
                None,
                None,
                None,
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
                &mut tty.renderer,
                surface,
                location,
                scale,
                1.0,
                Kind::Cursor,
            ));
        }
    }
    match space_render_elements(&mut tty.renderer, [&*space], &output, 1.0) {
        Ok(space_elements) => elements.extend(space_elements.into_iter().map(TtyElement::Space)),
        Err(err) => warn!(?err, "output has no mode"),
    }

    let rendered =
        match tty
            .drm_output
            .render_frame(&mut tty.renderer, &elements, CLEAR_COLOR, FRAME_FLAGS)
        {
            Ok(result) => !result.is_empty,
            Err(err) => {
                warn!(?err, "rendering failed");
                false
            }
        };
    if rendered {
        match tty.drm_output.queue_frame(()) {
            Ok(()) => tty.frame_pending = true,
            Err(err) => warn!(?err, "queueing the frame failed"),
        }
    }

    // Frame callbacks, so clients draw their next frame.
    for window in space.elements() {
        window.send_frame(&output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
    if let CursorImageStatus::Surface(surface) = &*cursor_status {
        send_frames_surface_tree(surface, &output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }

    if !tty.frame_pending {
        let refresh_mhz = output.current_mode().map_or(60_000, |mode| mode.refresh);
        let interval = Duration::from_micros(1_000_000_000 / u64::from(refresh_mhz.max(1) as u32));
        schedule_render(tty, interval);
    }
}
