//! Screencopy: `ext-image-capture-source-v1` with output and toplevel sources and
//! `ext-image-copy-capture-v1` (tasks 1.16b and 1.16c, ADRs 0022 and 0023). `grim`,
//! `wf-recorder` and the screencast portal capture outputs and windows through these.
//!
//! Smithay runs the protocol (sessions, frames, buffer checks against the constraints);
//! this module draws. Each session has its own [`OffscreenOutput`]: a frame draws the
//! output's elements into it with its own damage tracker, then copies the result into the
//! client's buffer (read back into shm, or blitted into a dmabuf). The damage reported is
//! what changed since the session's previous frame. A frame that would report no damage
//! waits until the output changes (the backends call [`State::capture_waiting_frames`]
//! after drawing an output), as the protocol allows, so a recording client does not spin.
//!
//! Output captures are upright (transform normal) and the size of the output's current
//! mode after its transform. The cursor is drawn when the client asks for it
//! (`paint_cursors`) and the backend draws one itself: the tty backend. A toplevel capture
//! shows one window's surfaces (no title bar, no cursor) on a transparent background, at
//! the window's geometry size and the scale of the output it is on. Only unsandboxed
//! clients see the globals (ADR 0017).

use std::fmt;
use std::time::Duration;

use neue_ipc::{ErrorBody, ErrorCode, WindowId};
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::{Fourcc, Modifier};
use smithay::backend::drm::DrmNode;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{BufferType, buffer_type};
use smithay::desktop::{Space, Window};
use smithay::input::pointer::CursorImageStatus;
use smithay::output::{Output, WeakOutput};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::utils::{
    Buffer as BufferCoords, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelWeakHandle};
use smithay::wayland::image_capture_source::{
    ImageCaptureSource, ImageCaptureSourceHandler, ImageCaptureSourceState,
    OutputCaptureSourceHandler, OutputCaptureSourceState, ToplevelCaptureSourceHandler,
    ToplevelCaptureSourceState,
};
use smithay::wayland::image_copy_capture::{
    BufferConstraints, CaptureFailureReason, DmabufConstraints, Frame, FrameRef,
    ImageCopyCaptureHandler, ImageCopyCaptureState, Session, SessionRef,
};
use smithay::wayland::shm::with_buffer_contents_mut;
use tracing::{debug, warn};

use crate::decorations::Chrome;
use crate::render::{CLEAR_COLOR, OffscreenOutput, ScreenElement, window_content_elements};
use crate::security::is_unrestricted;
use crate::state::{Backend, DndIcon, State};

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

/// What a window capture is cleared to: transparent.
const TRANSPARENT: [f32; 4] = [0.0; 4];

/// The screencopy globals and the open capture sessions.
pub struct Screencopy {
    _source_state: ImageCaptureSourceState,
    output_source_state: OutputCaptureSourceState,
    toplevel_source_state: ToplevelCaptureSourceState,
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
            toplevel_source_state: ToplevelCaptureSourceState::new_with_filter::<State, _>(
                dh,
                is_unrestricted,
            ),
            copy_state: ImageCopyCaptureState::new_with_filter::<State, _>(dh, is_unrestricted),
            sessions: Vec::new(),
        }
    }
}

/// What a capture session shows.
#[derive(Clone)]
enum Subject {
    Output(WeakOutput),
    Toplevel(ForeignToplevelWeakHandle),
}

/// Where a subject is right now: what to draw and how big the capture is.
enum Resolved {
    Output {
        output: Output,
        size: Size<i32, Physical>,
    },
    Window {
        window: Window,
        size: Size<i32, Physical>,
        scale: f64,
    },
}

impl Resolved {
    fn size(&self) -> Size<i32, Physical> {
        match self {
            Self::Output { size, .. } | Self::Window { size, .. } => *size,
        }
    }
}

/// One client's capture of one output or window. Dropping it stops the session.
struct CaptureSession {
    session: Session,
    subject: Subject,
    /// The size last sent in the buffer constraints.
    size: Size<i32, Physical>,
    /// Where this session's frames are drawn; made on the first frame and remade when the
    /// size or scale changes.
    target: Option<OffscreenOutput>,
    /// A frame the client asked for that waits for the output to change.
    waiting: Option<Frame>,
}

/// What an image capture source shows, if it is one of ours.
fn source_subject(source: &ImageCaptureSource) -> Option<Subject> {
    let data = source.user_data();
    data.get::<WeakOutput>()
        .cloned()
        .map(Subject::Output)
        .or_else(|| {
            data.get::<ForeignToplevelWeakHandle>()
                .cloned()
                .map(Subject::Toplevel)
        })
}

/// The window id in a toplevel's `ext-foreign-toplevel-list` identifier
/// (`window-<id>`, see `State::announce_toplevel`).
fn handle_window_id(handle: &ForeignToplevelHandle) -> Option<WindowId> {
    let id = handle.identifier().strip_prefix("window-")?.parse().ok()?;
    Some(WindowId(id))
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
    /// Where `subject` is now, or `None` if it cannot be captured: the output has no mode,
    /// the window closed or has no size yet.
    fn resolve(&self, subject: &Subject) -> Option<Resolved> {
        match subject {
            Subject::Output(output) => {
                let output = output.upgrade()?;
                let size = capture_size(&output)?;
                Some(Resolved::Output { output, size })
            }
            Subject::Toplevel(handle) => {
                let handle = handle.upgrade().filter(|h| !h.is_closed())?;
                let window = self.window_by_id(handle_window_id(&handle)?)?;
                // The scale of the output the window is on (the first one if it spans
                // several, any if it is minimised).
                let scale = self
                    .space
                    .outputs_for_element(&window)
                    .first()
                    .or_else(|| self.outputs().next())
                    .map_or(1.0, |output| output.current_scale().fractional_scale());
                let size = window
                    .geometry()
                    .size
                    .to_f64()
                    .to_physical(scale)
                    .to_i32_round();
                (size.w > 0 && size.h > 0).then_some(Resolved::Window {
                    window,
                    size,
                    scale,
                })
            }
        }
    }

    /// Buffer constraints for capturing `subject`, or `None` if it cannot be captured.
    fn capture_constraints_for(&mut self, subject: &Subject) -> Option<BufferConstraints> {
        let size = self.resolve(subject)?.size();
        capture_renderer(&mut self.backend)?;
        Some(BufferConstraints {
            size: (size.w, size.h).into(),
            shm: SHM_FORMATS.to_vec(),
            dma: dmabuf_constraints(&self.backend),
        })
    }

    /// Outputs changed: stop the sessions of outputs that left the layout, and send new
    /// buffer constraints to those whose output changed size (failing a waiting frame,
    /// whose buffer no longer fits). Called from `report_outputs`. Window sessions notice
    /// a new size when they next capture.
    pub(crate) fn refresh_capture_sessions(&mut self) {
        let outputs: Vec<Output> = self.outputs().cloned().collect();
        let mut index = 0;
        while index < self.screencopy.sessions.len() {
            let session = &self.screencopy.sessions[index];
            let (subject, old_size) = (session.subject.clone(), session.size);
            let left_layout = matches!(
                &subject,
                Subject::Output(output) if output.upgrade().is_none_or(|o| !outputs.contains(&o))
            );
            let size = if left_layout {
                None
            } else {
                self.resolve(&subject).map(|resolved| resolved.size())
            };
            let constraints = match size {
                Some(size) if size == old_size => {
                    index += 1;
                    continue;
                }
                Some(_) => self.capture_constraints_for(&subject),
                None => None,
            };
            let (Some(size), Some(constraints)) = (size, constraints) else {
                // Gone, or no mode any more: nothing to capture.
                debug!("screencopy session of a removed output stopped");
                self.screencopy.sessions.swap_remove(index);
                continue;
            };
            let session = &mut self.screencopy.sessions[index];
            session.size = size;
            session.session.update_constraints(constraints);
            if let Some(frame) = session.waiting.take() {
                frame.fail(CaptureFailureReason::BufferConstraints);
            }
            index += 1;
        }
    }

    /// A toplevel went away: stop the sessions that capture it.
    pub(crate) fn stop_toplevel_captures(&mut self, handle: &ForeignToplevelHandle) {
        self.screencopy.sessions.retain(|s| match &s.subject {
            Subject::Toplevel(weak) => weak.upgrade().is_none_or(|h| !h.matches(handle)),
            Subject::Output(_) => true,
        });
    }

    /// Whether a change on `output` may change what `subject` shows.
    fn subject_on_output(&self, subject: &Subject, output: &Output) -> bool {
        match subject {
            Subject::Output(weak) => weak.upgrade().as_ref() == Some(output),
            Subject::Toplevel(handle) => {
                let window = handle
                    .upgrade()
                    .and_then(|handle| self.window_by_id(handle_window_id(&handle)?));
                window.is_none_or(|window| {
                    let outputs = self.space.outputs_for_element(&window);
                    // Minimised (or not on any output): any output's frame will do.
                    outputs.is_empty() || outputs.contains(output)
                })
            }
        }
    }

    /// `output` was drawn: give the frames waiting on it, or on windows on it, another try.
    pub(crate) fn capture_waiting_frames(&mut self, output: &Output) {
        let waiting: Vec<usize> = self
            .screencopy
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.waiting.is_some() && self.subject_on_output(&s.subject, output))
            .map(|(index, _)| index)
            .collect();
        for index in waiting {
            self.capture(index);
        }
    }

    /// Draw session `index`'s subject and, if anything changed since its previous frame,
    /// complete the waiting frame. Otherwise the frame keeps waiting.
    fn capture(&mut self, index: usize) {
        let Some(session) = self.screencopy.sessions.get(index) else {
            return;
        };
        if session.waiting.is_none() {
            return;
        }
        let (subject, session_size) = (session.subject.clone(), session.size);
        let Some(resolved) = self.resolve(&subject) else {
            if let Some(frame) = self.screencopy.sessions[index].waiting.take() {
                frame.fail(CaptureFailureReason::Stopped);
            }
            return;
        };
        if resolved.size() != session_size {
            // The subject changed size since the constraints went out (a window resized):
            // send the new ones, and the frame's buffer no longer fits.
            if let Some(constraints) = self.capture_constraints_for(&subject) {
                let session = &mut self.screencopy.sessions[index];
                session.size = resolved.size();
                session.session.update_constraints(constraints);
                if let Some(frame) = session.waiting.take() {
                    frame.fail(CaptureFailureReason::BufferConstraints);
                }
            }
            return;
        }
        // An output that is not laid out (yet) waits for `refresh_capture_sessions`.
        let output_geo = match &resolved {
            Resolved::Output { output, .. } => match self.space.output_geometry(output) {
                Some(geo) => Some(geo),
                None => return,
            },
            Resolved::Window { .. } => None,
        };

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
        let Some((renderer, cursors)) = capture_renderer(backend) else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };
        let (size, scale, clear) = match &resolved {
            Resolved::Output { output, size } => (
                *size,
                output.current_scale().fractional_scale(),
                CLEAR_COLOR,
            ),
            Resolved::Window { size, scale, .. } => (*size, *scale, TRANSPARENT),
        };
        let mut target = match session.target.take() {
            Some(target) if target.fits(size, scale) => target,
            _ => match OffscreenOutput::with_clear(renderer, size, scale, clear) {
                Ok(target) => target,
                Err(err) => {
                    warn!(err, "screencopy failed");
                    frame.fail(CaptureFailureReason::Unknown);
                    return;
                }
            },
        };

        let elements: Vec<ScreenElement> = match (&resolved, output_geo) {
            (Resolved::Output { output, .. }, Some(output_geo)) => output_capture_elements(
                renderer,
                cursors,
                session.session.draw_cursor(),
                OutputScene {
                    space,
                    chrome,
                    cursor_status,
                    dnd_icon: dnd_icon.as_ref(),
                    pointer_location,
                    time,
                },
                output,
                output_geo,
                scale,
            ),
            (Resolved::Window { window, .. }, _) => {
                window_content_elements(renderer, window, scale)
                    .into_iter()
                    .map(ScreenElement::Output)
                    .collect()
            }
            // Outputs always have a geometry here (checked above).
            (Resolved::Output { .. }, None) => Vec::new(),
        };

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

/// What an output capture draws besides the output itself: the pointer and the layout.
struct OutputScene<'a> {
    space: &'a Space<Window>,
    chrome: &'a mut Chrome,
    cursor_status: &'a CursorImageStatus,
    dnd_icon: Option<&'a DndIcon>,
    pointer_location: Point<f64, Logical>,
    /// Time since start-up, for animated cursors.
    time: Duration,
}

/// Everything a capture of `output` (laid out at `output_geo`, drawn at `scale`) shows,
/// topmost first: the pointer and drag icon if `draw_cursor` and the backend draws them
/// itself (`cursors` is `Some`), then the output's windows and layers.
fn output_capture_elements(
    renderer: &mut GlesRenderer,
    cursors: Option<&mut Cursors>,
    draw_cursor: bool,
    scene: OutputScene<'_>,
    output: &Output,
    output_geo: Rectangle<i32, Logical>,
    scale: f64,
) -> Vec<ScreenElement> {
    let mut elements: Vec<ScreenElement> = Vec::new();
    #[cfg(feature = "tty")]
    if let Some(cursors) = cursors
        && draw_cursor
    {
        let pointer = crate::tty::PointerLook {
            status: scene.cursor_status,
            dnd_icon: scene.dnd_icon,
            location: scene.pointer_location,
        };
        elements.extend(
            crate::tty::pointer_elements(
                renderer,
                cursors,
                &pointer,
                output_geo,
                Scale::from(scale),
                scene.time,
            )
            .0,
        );
    }
    #[cfg(not(feature = "tty"))]
    let _ = (
        cursors,
        draw_cursor,
        output_geo,
        scene.cursor_status,
        scene.dnd_icon,
        scene.pointer_location,
        scene.time,
    );
    elements.extend(
        crate::render::output_elements(renderer, scene.space, scene.chrome, output)
            .into_iter()
            .map(ScreenElement::Output),
    );
    elements
}

/// A one-off capture of an output: ARGB8888 pixels, rows packed.
pub(crate) struct OutputShot {
    pub output: String,
    pub size: Size<i32, Physical>,
    pub data: Vec<u8>,
}

impl State {
    /// Draw the output called `name` (the first one if `None`) into a fresh buffer and read
    /// it back, for `neuectl screenshot` (ADR 0024). Blocks like a screencopy frame does.
    pub(crate) fn capture_output_now(
        &mut self,
        name: Option<&str>,
        cursor: bool,
    ) -> Result<OutputShot, ErrorBody> {
        let output = match name {
            Some(name) => self.outputs().find(|o| o.name() == name),
            None => self.outputs().next(),
        }
        .cloned()
        .ok_or_else(|| match name {
            Some(name) => ErrorBody::new(ErrorCode::UnknownOutput, format!("no output {name}")),
            None => ErrorBody::new(ErrorCode::UnknownOutput, "there is no output"),
        })?;
        let failed = |message: String| ErrorBody::new(ErrorCode::CaptureFailed, message);
        let size = capture_size(&output).ok_or_else(|| failed("the output has no mode".into()))?;
        let output_geo = self
            .space
            .output_geometry(&output)
            .ok_or_else(|| failed("the output is not part of the layout".into()))?;
        let scale = output.current_scale().fractional_scale();
        let time = self.start_time.elapsed();
        let pointer_location = self.pointer.current_location();
        let State {
            backend,
            space,
            chrome,
            cursor_status,
            dnd_icon,
            ..
        } = self;
        let (renderer, cursors) = capture_renderer(backend)
            .ok_or_else(|| failed("this backend has no renderer to capture with".into()))?;
        let mut target = OffscreenOutput::new(renderer, size, scale).map_err(failed)?;
        let elements = output_capture_elements(
            renderer,
            cursors,
            cursor,
            OutputScene {
                space,
                chrome,
                cursor_status,
                dnd_icon: dnd_icon.as_ref(),
                pointer_location,
                time,
            },
            &output,
            output_geo,
            scale,
        );
        target.render(renderer, &elements).map_err(failed)?;
        let data = target.read_back(renderer).map_err(failed)?;
        Ok(OutputShot {
            output: output.name(),
            size,
            data,
        })
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
            debug!("screencopy frame into a shm buffer");
            let pixels = target.read_back(renderer)?;
            copy_into_shm(&pixels, target.size(), buffer)
        }
        Some(BufferType::Dma) => {
            debug!("screencopy frame into a dmabuf");
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

impl ToplevelCaptureSourceHandler for State {
    fn toplevel_capture_source_state(&mut self) -> &mut ToplevelCaptureSourceState {
        &mut self.screencopy.toplevel_source_state
    }

    fn toplevel_source_created(
        &mut self,
        source: ImageCaptureSource,
        toplevel: ForeignToplevelHandle,
    ) {
        source
            .user_data()
            .insert_if_missing(|| toplevel.downgrade());
    }
}

impl ImageCopyCaptureHandler for State {
    fn image_copy_capture_state(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.screencopy.copy_state
    }

    fn capture_constraints(&mut self, source: &ImageCaptureSource) -> Option<BufferConstraints> {
        let subject = source_subject(source)?;
        self.capture_constraints_for(&subject)
    }

    fn new_session(&mut self, session: Session) {
        // Smithay asked `capture_constraints` just before, so both are there; if not,
        // dropping the session stops it.
        let Some(subject) = source_subject(&session.source()) else {
            return;
        };
        let Some(resolved) = self.resolve(&subject) else {
            return;
        };
        debug!("screencopy session started");
        self.screencopy.sessions.push(CaptureSession {
            session,
            subject,
            size: resolved.size(),
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
