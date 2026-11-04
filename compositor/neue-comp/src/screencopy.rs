//! Screencopy: `ext-image-capture-source-v1` with output sources and
//! `ext-image-copy-capture-v1` (task 1.16b, ADR 0022). `grim`, `wf-recorder` and the
//! screencast portal capture outputs through these.
//!
//! Smithay runs the protocol (sessions, frames, buffer checks against the constraints);
//! this module draws. Each session has its own [`OffscreenOutput`]: a frame draws the
//! output's elements into it with its own damage tracker, then copies the result into the
//! client's buffer (read back into shm, or blitted into a dmabuf). The damage reported is
//! what changed since the session's previous frame. A frame that would report no damage
//! waits until the output changes (the backends call [`State::capture_waiting_frames`]
//! after drawing an output), as the protocol allows, so a recording client does not spin.
//!
//! Captures are upright (transform normal) and the size of the output's current mode
//! after its transform. The cursor is drawn when the client asks for it (`paint_cursors`)
//! and the backend draws one itself: the tty backend. Only unsandboxed clients see the
//! globals (ADR 0017).

use std::fmt;

use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::{Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{BufferType, buffer_type};
use smithay::output::{Output, WeakOutput};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{Buffer as BufferCoords, Physical, Rectangle, Size, Transform};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::image_capture_source::{
    ImageCaptureSource, ImageCaptureSourceHandler, ImageCaptureSourceState,
    OutputCaptureSourceHandler, OutputCaptureSourceState,
};
use smithay::wayland::image_copy_capture::{
    BufferConstraints, CaptureFailureReason, DmabufConstraints, Frame, FrameRef,
    ImageCopyCaptureHandler, ImageCopyCaptureState, Session, SessionRef,
};
use smithay::wayland::shm::with_buffer_contents_mut;
use tracing::{debug, warn};

use crate::render::{OffscreenOutput, ScreenElement};
use crate::security::is_unrestricted;
use crate::state::{Backend, State};

/// `wl_shm` formats a capture can be copied into. Both have the same layout in memory,
/// which is what [`OffscreenOutput::read_back`] returns.
const SHM_FORMATS: [wl_shm::Format; 2] = [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888];
/// dmabuf formats offered, if the renderer can draw into them: 8-bit RGB, what capture
/// clients allocate.
const DMABUF_FORMATS: [Fourcc; 4] = [
    Fourcc::Argb8888,
    Fourcc::Xrgb8888,
    Fourcc::Abgr8888,
    Fourcc::Xbgr8888,
];

/// The screencopy globals and the open capture sessions.
pub struct Screencopy {
    _source_state: ImageCaptureSourceState,
    output_source_state: OutputCaptureSourceState,
    copy_state: ImageCopyCaptureState,
    sessions: Vec<CaptureSession>,
}

impl fmt::Debug for Screencopy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Screencopy")
            .field("sessions", &self.sessions.len())
            .finish_non_exhaustive()
    }
}

impl Screencopy {
    /// Create the globals, visible to unsandboxed clients only.
    pub fn new(dh: &DisplayHandle) -> Self {
        Self {
            _source_state: ImageCaptureSourceState::new(),
            output_source_state: OutputCaptureSourceState::new_with_filter::<State, _>(
                dh,
                is_unrestricted,
            ),
            copy_state: ImageCopyCaptureState::new_with_filter::<State, _>(dh, is_unrestricted),
            sessions: Vec::new(),
        }
    }
}

/// One client's capture of one output. Dropping it stops the session.
struct CaptureSession {
    session: Session,
    output: WeakOutput,
    /// The size last sent in the buffer constraints.
    size: Size<i32, Physical>,
    /// Where this session's frames are drawn; made on the first frame and remade when the
    /// output's size or scale changes.
    target: Option<OffscreenOutput>,
    /// A frame the client asked for that waits for the output to change.
    waiting: Option<Frame>,
}

/// The output an image capture source shows, if it is an output source whose output still
/// exists.
fn source_output(source: &ImageCaptureSource) -> Option<Output> {
    source.user_data().get::<WeakOutput>()?.upgrade()
}

/// Size of a capture of `output`: its current mode after its transform.
fn capture_size(output: &Output) -> Option<Size<i32, Physical>> {
    let mode = output.current_mode()?;
    Some(output.current_transform().transform_size(mode.size))
}

#[cfg(feature = "tty")]
type Cursors = crate::cursor::Themes;
/// Without the tty backend nothing draws a cursor.
#[cfg(not(feature = "tty"))]
type Cursors = std::convert::Infallible;

/// The renderer captures are drawn with, and the cursor themes if the backend draws the
/// cursor itself. `None` if the backend has no renderer (headless without
/// [`crate::Options::headless_renderer`]): then nothing can be captured.
fn capture_renderer(backend: &mut Backend) -> Option<(&mut GlesRenderer, Option<&mut Cursors>)> {
    match backend {
        #[cfg(feature = "winit")]
        Backend::Winit(data) => Some((data.renderer(), None)),
        Backend::Headless(data) => data.renderer().map(|renderer| (renderer, None)),
        #[cfg(feature = "tty")]
        Backend::Tty(data) => {
            let (renderer, cursors) = data.capture_parts();
            Some((renderer, Some(cursors)))
        }
    }
}

/// The render node and formats clients may allocate capture dmabufs with, if the backend
/// has a render node.
fn dmabuf_constraints(backend: &Backend) -> Option<DmabufConstraints> {
    #[cfg(feature = "tty")]
    if let Backend::Tty(data) = backend {
        let (node, formats) = data.capture_dmabuf()?;
        return offered_formats(node, &formats);
    }
    let _ = backend; // only the tty backend has a render node for captures
    None
}

/// The [`DMABUF_FORMATS`] the renderer can draw into, with their modifiers.
#[cfg_attr(not(feature = "tty"), allow(dead_code))] // used by the tty backend only
fn offered_formats(node: DrmNode, formats: &FormatSet) -> Option<DmabufConstraints> {
    let formats: Vec<(Fourcc, Vec<Modifier>)> = DMABUF_FORMATS
        .into_iter()
        .map(|code| {
            let modifiers: Vec<Modifier> = formats
                .iter()
                .filter(|f| f.code == code)
                .map(|f| f.modifier)
                .collect();
            (code, modifiers)
        })
        .filter(|(_, modifiers)| !modifiers.is_empty())
        .collect();
    (!formats.is_empty()).then_some(DmabufConstraints { node, formats })
}

impl State {
    /// Buffer constraints for capturing `output`, or `None` if it cannot be captured.
    fn capture_constraints_for(&mut self, output: &Output) -> Option<BufferConstraints> {
        let size = capture_size(output)?;
        capture_renderer(&mut self.backend)?;
        Some(BufferConstraints {
            size: (size.w, size.h).into(),
            shm: SHM_FORMATS.to_vec(),
            dma: dmabuf_constraints(&self.backend),
        })
    }

    /// Outputs changed: stop the sessions of outputs that left the layout, and send new
    /// buffer constraints to those whose output changed size (failing a waiting frame,
    /// whose buffer no longer fits). Called from `report_outputs`.
    pub(crate) fn refresh_capture_sessions(&mut self) {
        let outputs: Vec<Output> = self.outputs().cloned().collect();
        let before = self.screencopy.sessions.len();
        self.screencopy.sessions.retain(|s| {
            s.output
                .upgrade()
                .is_some_and(|output| outputs.contains(&output))
        });
        if self.screencopy.sessions.len() != before {
            debug!(
                stopped = before - self.screencopy.sessions.len(),
                "screencopy sessions of removed outputs stopped"
            );
        }
        for index in 0..self.screencopy.sessions.len() {
            let session = &self.screencopy.sessions[index];
            let Some(output) = session.output.upgrade() else {
                continue;
            };
            let size = capture_size(&output);
            if size == Some(session.size) {
                continue;
            }
            let constraints = self.capture_constraints_for(&output);
            let session = &mut self.screencopy.sessions[index];
            match (size, constraints) {
                (Some(size), Some(constraints)) => {
                    session.size = size;
                    session.session.update_constraints(constraints);
                    if let Some(frame) = session.waiting.take() {
                        frame.fail(CaptureFailureReason::BufferConstraints);
                    }
                }
                // No mode any more: nothing to capture.
                _ => {
                    self.screencopy.sessions.swap_remove(index);
                    return self.refresh_capture_sessions();
                }
            }
        }
    }

    /// `output` was drawn: give the frames waiting on it another try.
    pub(crate) fn capture_waiting_frames(&mut self, output: &Output) {
        let waiting: Vec<usize> = self
            .screencopy
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.waiting.is_some() && s.output == *output)
            .map(|(index, _)| index)
            .collect();
        for index in waiting {
            self.capture(index);
        }
    }

    /// Draw session `index`'s output and, if anything changed since its previous frame,
    /// complete the waiting frame. Otherwise the frame keeps waiting.
    fn capture(&mut self, index: usize) {
        let time = self.start_time.elapsed();
        let now = self.clock.now();
        let pointer_location = self.pointer.current_location();
        let State {
            backend,
            space,
            chrome,
            screencopy,
            cursor_status,
            dnd_icon,
            ..
        } = self;
        let Some(session) = screencopy.sessions.get_mut(index) else {
            return;
        };
        let Some(frame) = session.waiting.take() else {
            return;
        };
        let Some(output) = session.output.upgrade() else {
            frame.fail(CaptureFailureReason::Stopped);
            return;
        };
        let (Some(size), Some(output_geo)) =
            (capture_size(&output), space.output_geometry(&output))
        else {
            // Not laid out (yet): wait for `refresh_capture_sessions`.
            session.waiting = Some(frame);
            return;
        };
        let Some((renderer, cursors)) = capture_renderer(backend) else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };
        let scale = output.current_scale().fractional_scale();
        let mut target = match session.target.take() {
            Some(target) if target.fits(size, scale) => target,
            _ => match OffscreenOutput::new(renderer, size, scale) {
                Ok(target) => target,
                Err(err) => {
                    warn!(err, "screencopy failed");
                    frame.fail(CaptureFailureReason::Unknown);
                    return;
                }
            },
        };

        let mut elements: Vec<ScreenElement> = Vec::new();
        #[cfg(feature = "tty")]
        if let Some(cursors) = cursors
            && session.session.draw_cursor()
        {
            let pointer = crate::tty::PointerLook {
                status: cursor_status,
                dnd_icon: dnd_icon.as_ref(),
                location: pointer_location,
            };
            let scale = smithay::utils::Scale::from(scale);
            elements.extend(
                crate::tty::pointer_elements(renderer, cursors, &pointer, output_geo, scale, time)
                    .0,
            );
        }
        #[cfg(not(feature = "tty"))]
        let _ = (
            cursors,
            cursor_status,
            dnd_icon,
            pointer_location,
            time,
            output_geo,
        );
        elements.extend(
            crate::render::output_elements(renderer, space, chrome, &output)
                .into_iter()
                .map(ScreenElement::Output),
        );

        let damage = target.render(renderer, &elements);
        let result = damage.and_then(|damage| {
            if damage.is_empty() {
                return Ok(None);
            }
            copy_into(&mut target, renderer, &frame.buffer()).map(|()| Some(damage))
        });
        session.target = Some(target);
        match result {
            Ok(Some(damage)) => {
                let damage: Vec<Rectangle<i32, BufferCoords>> = damage
                    .iter()
                    .map(|r| Rectangle::new((r.loc.x, r.loc.y).into(), (r.size.w, r.size.h).into()))
                    .collect();
                frame.success(Transform::Normal, damage, now);
            }
            Ok(None) => session.waiting = Some(frame),
            Err(err) => {
                warn!(err, "screencopy failed");
                frame.fail(CaptureFailureReason::Unknown);
            }
        }
    }
}

/// Copy `target`'s last frame into the client's `buffer`, whose type, format and size
/// Smithay has checked against the constraints.
fn copy_into(
    target: &mut OffscreenOutput,
    renderer: &mut GlesRenderer,
    buffer: &WlBuffer,
) -> Result<(), String> {
    match buffer_type(buffer) {
        Some(BufferType::Shm) => {
            let pixels = target.read_back(renderer)?;
            copy_into_shm(&pixels, target.size(), buffer)
        }
        Some(BufferType::Dma) => {
            let mut dmabuf = get_dmabuf(buffer)
                .map_err(|err| format!("not a dmabuf: {err:?}"))?
                .clone();
            target.blit_into(renderer, &mut dmabuf)
        }
        other => Err(format!("unsupported buffer type {other:?}")),
    }
}

/// Write `pixels` (rows packed, `size` physical pixels) into the top-left corner of the shm
/// `buffer`, row by row with the buffer's stride.
fn copy_into_shm(
    pixels: &[u8],
    size: Size<i32, Physical>,
    buffer: &WlBuffer,
) -> Result<(), String> {
    let width = usize::try_from(size.w).map_err(|_| "negative width")?;
    let height = usize::try_from(size.h).map_err(|_| "negative height")?;
    let row = width * 4;
    if pixels.len() < row * height {
        return Err(format!("{} bytes read back for {size:?}", pixels.len()));
    }
    with_buffer_contents_mut(buffer, |ptr, len, data| {
        let offset = usize::try_from(data.offset).map_err(|_| "negative offset")?;
        let stride = usize::try_from(data.stride).map_err(|_| "negative stride")?;
        // Smithay checked the buffer is at least `size`; check the layout fits the pool.
        let end = height
            .checked_sub(1)
            .map_or(0, |last| offset + last * stride + row);
        if stride < row || end > len {
            return Err(format!(
                "shm buffer too small: stride {stride}, pool {len} bytes, need {end}"
            ));
        }
        for (y, source) in pixels.chunks_exact(row).take(height).enumerate() {
            // SAFETY: `offset + y * stride + row <= end <= len` (checked above), so the
            // destination lies inside the pool mapping Smithay hands us; `source` is our
            // own buffer, so the two cannot overlap. We copy through a raw pointer and
            // never make a reference into the client-shared memory (Smithay's safety
            // note on `with_buffer_contents_mut`); a client truncating the pool raises
            // SIGBUS, which Smithay catches and reports as an error.
            #[allow(unsafe_code)] // see SAFETY
            unsafe {
                std::ptr::copy_nonoverlapping(source.as_ptr(), ptr.add(offset + y * stride), row);
            }
        }
        Ok(())
    })
    .map_err(|err| format!("accessing the shm buffer: {err:?}"))?
}

impl ImageCaptureSourceHandler for State {}

impl OutputCaptureSourceHandler for State {
    fn output_capture_source_state(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.screencopy.output_source_state
    }

    fn output_source_created(&mut self, source: ImageCaptureSource, output: &Output) {
        source.user_data().insert_if_missing(|| output.downgrade());
    }
}

impl ImageCopyCaptureHandler for State {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.screencopy.copy_state
    }

    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        let output = source_output(source)?;
        self.capture_constraints_for(&output)
    }

    fn new_session(&mut self, session: Session) {
        // Smithay asked `capture_constraints` just before, so both are there; if not,
        // dropping the session stops it.
        let Some(output) = source_output(&session.source()) else {
            return;
        };
        let Some(size) = capture_size(&output) else {
            return;
        };
        debug!(output = output.name(), "screencopy session started");
        self.screencopy.sessions.push(CaptureSession {
            session,
            output: output.downgrade(),
            size,
            target: None,
            waiting: None,
        });
    }

    fn frame(&mut self, session: &SessionRef, frame: Frame) {
        let Some(index) = self
            .screencopy
            .sessions
            .iter()
            .position(|s| s.session == *session)
        else {
            frame.fail(CaptureFailureReason::Stopped);
            return;
        };
        self.screencopy.sessions[index].waiting = Some(frame);
        self.capture(index);
    }

    fn frame_aborted(&mut self, frame: FrameRef) {
        for session in &mut self.screencopy.sessions {
            if session.waiting.as_ref().is_some_and(|f| *f == frame) {
                session.waiting = None;
            }
        }
    }

    fn session_destroyed(&mut self, session: SessionRef) {
        self.screencopy.sessions.retain(|s| s.session != session);
        self.screencopy.copy_state.cleanup();
    }
}
