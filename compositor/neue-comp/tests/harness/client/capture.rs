//! Screencopy on the test client (tasks 1.16b, 1.16c): `ext-image-capture-source-v1`
//! output and toplevel sources and `ext-image-copy-capture-v1` sessions and frames,
//! captured into shm.

use std::fs::File;
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::time::Duration;

use wayland_client::protocol::{wl_buffer::WlBuffer, wl_shm};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};

use super::{ClientData, TestClient};

/// What the compositor told a capture session.
#[derive(Debug, Default, Clone)]
pub struct SessionEvents {
    /// The last `buffer_size`.
    pub buffer_size: Option<(u32, u32)>,
    /// `shm_format`s since the last `done`.
    pub shm_formats: Vec<wl_shm::Format>,
    /// Number of `done` events.
    pub done: usize,
    pub stopped: bool,
}

/// A damage rectangle: x, y, width, height.
pub type Rect = (i32, i32, i32, i32);

/// What became of a capture frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameState {
    Pending,
    /// `ready`, with the `damage` rectangles (x, y, width, height) and `transform` sent
    /// before it.
    Ready {
        damage: Vec<Rect>,
        transform: u32,
    },
    Failed(u32),
}

/// A capture session the test holds: an index into the client's sessions.
pub struct CaptureSession {
    index: usize,
    pub session: ExtImageCopyCaptureSessionV1,
    _source: ExtImageCaptureSourceV1,
}

/// A frame captured into an shm buffer the test can read. The frame object is destroyed
/// once it is ready or failed, so the session can make the next one.
pub struct CaptureFrame {
    index: usize,
    pub frame: ExtImageCopyCaptureFrameV1,
    pub buffer: WlBuffer,
    file: File,
    pub size: (i32, i32),
    stride: i32,
}

#[derive(Default)]
pub(super) struct CaptureData {
    managers: Option<(
        ExtOutputImageCaptureSourceManagerV1,
        ExtImageCopyCaptureManagerV1,
    )>,
    toplevel_manager: Option<ExtForeignToplevelImageCaptureSourceManagerV1>,
    sessions: Vec<SessionEvents>,
    frames: Vec<FrameState>,
    /// Damage and transform of each frame, collected until `ready`.
    partial: Vec<(Vec<Rect>, u32)>,
}

impl TestClient {
    fn capture_managers(
        &mut self,
    ) -> (
        ExtOutputImageCaptureSourceManagerV1,
        ExtImageCopyCaptureManagerV1,
    ) {
        if let Some(managers) = &self.data.capture.managers {
            return managers.clone();
        }
        let sources: ExtOutputImageCaptureSourceManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding ext_output_image_capture_source_manager_v1");
        let copy: ExtImageCopyCaptureManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding ext_image_copy_capture_manager_v1");
        self.data.capture.managers = Some((sources.clone(), copy.clone()));
        (sources, copy)
    }

    /// Start a capture session of the `n`th `wl_output` (with the cursor if
    /// `paint_cursors`) and wait for the compositor's first answer.
    pub fn capture_output(&mut self, n: usize, paint_cursors: bool) -> CaptureSession {
        let (sources, copy) = self.capture_managers();
        let output = self.output_global(n);
        let source = sources.create_source(&output, &self.qh, ());
        self.start_capture_session(&copy, source, paint_cursors)
    }

    /// Start a capture session of the `n`th toplevel the foreign toplevel list announced
    /// (see [`TestClient::track_foreign_toplevels`]) and wait for the compositor's first
    /// answer.
    pub fn capture_toplevel(&mut self, n: usize) -> CaptureSession {
        let (_, copy) = self.capture_managers();
        let manager = match &self.data.capture.toplevel_manager {
            Some(manager) => manager.clone(),
            None => {
                let manager: ExtForeignToplevelImageCaptureSourceManagerV1 = self
                    ._globals
                    .bind(&self.qh, 1..=1, ())
                    .expect("binding ext_foreign_toplevel_image_capture_source_manager_v1");
                self.data.capture.toplevel_manager = Some(manager.clone());
                manager
            }
        };
        let handle = self.foreign_handle(n);
        let source = manager.create_source(&handle, &self.qh, ());
        self.start_capture_session(&copy, source, false)
    }

    fn start_capture_session(
        &mut self,
        copy: &ExtImageCopyCaptureManagerV1,
        source: ExtImageCaptureSourceV1,
        paint_cursors: bool,
    ) -> CaptureSession {
        let index = self.data.capture.sessions.len();
        self.data.capture.sessions.push(SessionEvents::default());
        let options = if paint_cursors {
            Options::PaintCursors
        } else {
            Options::empty()
        };
        let session = copy.create_session(&source, options, &self.qh, index);
        self.roundtrip();
        CaptureSession {
            index,
            session,
            _source: source,
        }
    }

    /// The events `session` has seen so far.
    pub fn session_events(&self, session: &CaptureSession) -> &SessionEvents {
        &self.data.capture.sessions[session.index]
    }

    /// Ask for a frame of `session` into a new ARGB8888 shm buffer of the size the
    /// session announced, with the whole buffer damaged. Does not wait for it.
    pub fn capture_frame(&mut self, session: &CaptureSession) -> CaptureFrame {
        let (w, h) = self
            .session_events(session)
            .buffer_size
            .expect("no buffer_size");
        let size = (w as i32, h as i32);
        self.capture_frame_into(session, size, wl_shm::Format::Argb8888)
    }

    /// Ask for a frame of `session` into a new shm buffer of `size` and `format`, with
    /// the whole buffer damaged. Does not wait for it.
    pub fn capture_frame_into(
        &mut self,
        session: &CaptureSession,
        (w, h): (i32, i32),
        format: wl_shm::Format,
    ) -> CaptureFrame {
        let stride = w * 4;
        let len = stride * h;
        let fd = rustix::fs::memfd_create("neue-comp-capture", rustix::fs::MemfdFlags::CLOEXEC)
            .expect("memfd_create");
        let file = File::from(fd);
        file.set_len(len as u64).expect("sizing the shm file");
        let pool = self.shm.create_pool(file.as_fd(), len, &self.qh, ());
        let buffer = pool.create_buffer(0, w, h, stride, format, &self.qh, ());
        pool.destroy();

        let index = self.data.capture.frames.len();
        self.data.capture.frames.push(FrameState::Pending);
        self.data.capture.partial.push((Vec::new(), 0));
        let frame = session.session.create_frame(&self.qh, index);
        frame.attach_buffer(&buffer);
        frame.damage_buffer(0, 0, w, h);
        frame.capture();
        self.roundtrip();
        CaptureFrame {
            index,
            frame,
            buffer,
            file,
            size: (w, h),
            stride,
        }
    }

    /// What became of `frame` so far.
    pub fn frame_state(&self, frame: &CaptureFrame) -> FrameState {
        self.data.capture.frames[frame.index].clone()
    }

    /// Wait (up to two seconds) until `frame` is ready or failed.
    pub fn wait_for_frame(&mut self, frame: &CaptureFrame) -> FrameState {
        for _ in 0..200 {
            self.roundtrip();
            if self.frame_state(frame) != FrameState::Pending {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.frame_state(frame)
    }
}

impl CaptureFrame {
    /// The captured pixel at (`x`, `y`) as `0xAARRGGBB`.
    pub fn pixel(&self, x: i32, y: i32) -> u32 {
        let mut bytes = [0u8; 4];
        let at = (y * self.stride + x * 4) as u64;
        self.file
            .read_exact_at(&mut bytes, at)
            .expect("reading the shm buffer");
        u32::from_le_bytes(bytes)
    }

    /// The whole capture, rows packed (as `OutputPixels::data`).
    pub fn data(&self) -> Vec<u8> {
        let (w, h) = self.size;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for (y, row) in data.chunks_exact_mut((w * 4) as usize).enumerate() {
            let at = y as u64 * self.stride as u64;
            self.file
                .read_exact_at(row, at)
                .expect("reading the shm buffer");
        }
        data
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, usize> for ClientData {
    fn event(
        state: &mut Self,
        _: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let session = &mut state.capture.sessions[*index];
        match event {
            ext_image_copy_capture_session_v1::Event::BufferSize { width, height } => {
                session.buffer_size = Some((width, height));
                session.shm_formats.clear();
            }
            ext_image_copy_capture_session_v1::Event::ShmFormat {
                format: WEnum::Value(format),
            } => session.shm_formats.push(format),
            ext_image_copy_capture_session_v1::Event::Done => session.done += 1,
            ext_image_copy_capture_session_v1::Event::Stopped => session.stopped = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, usize> for ClientData {
    fn event(
        state: &mut Self,
        frame: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let capture = &mut state.capture;
        match event {
            ext_image_copy_capture_frame_v1::Event::Transform { transform } => {
                capture.partial[*index].1 = transform.into();
            }
            ext_image_copy_capture_frame_v1::Event::Damage {
                x,
                y,
                width,
                height,
            } => capture.partial[*index].0.push((x, y, width, height)),
            ext_image_copy_capture_frame_v1::Event::Ready => {
                let (damage, transform) = std::mem::take(&mut capture.partial[*index]);
                capture.frames[*index] = FrameState::Ready { damage, transform };
                // One frame per session at a time: make room for the next.
                frame.destroy();
            }
            ext_image_copy_capture_frame_v1::Event::Failed { reason } => {
                capture.frames[*index] = FrameState::Failed(reason.into());
                frame.destroy();
            }
            _ => {}
        }
    }
}

delegate_noop!(ClientData: ignore ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(ClientData: ignore ExtForeignToplevelImageCaptureSourceManagerV1);
delegate_noop!(ClientData: ignore ExtImageCaptureSourceV1);
delegate_noop!(ClientData: ignore ExtImageCopyCaptureManagerV1);
