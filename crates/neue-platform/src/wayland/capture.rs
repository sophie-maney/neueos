//! Window thumbnails for the shell (task 3.17d, ADR 0096): an `ext-image-copy-capture-v1`
//! client with `ext-foreign-toplevel-image-capture-source-v1` sources.
//!
//! - The globals are bound only by the shell (the client `neue_shell_v1` is offered to),
//!   together with `ext-foreign-toplevel-list-v1`, whose handles are what a toplevel source
//!   is made from. A handle is matched to a window of the shell's list by its identifier
//!   (`window-N`, the same string in both lists).
//! - [`ShellRequest::Thumbnail`](crate::desktop::ShellRequest::Thumbnail) starts a
//!   session for the window once its handle is known. Each frame is copied into one shm
//!   buffer kept per session (made again when the session's size or formats change),
//!   scaled down on the CPU and sent to the app as [`ShellEvent::Thumbnail`].
//! - The frame loop: once a frame is ready the next is asked for after [`INTERVAL`]. The
//!   compositor holds a frame until the window changes (ADR 0023 §5), so a window that
//!   doesn't draw (a minimised one, mostly) costs nothing between frames.
//! - A session ends when the app asks (`max_side: None`), when the window closes, or when
//!   the compositor stops it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::reexports::calloop::RegistrationToken;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::shm::raw::RawPool;
use wayland_client::backend::{ObjectData, ObjectId};
use wayland_client::globals::GlobalList;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_shm};
use wayland_client::{Connection, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
};

use super::{AppEvent, NoEvents, State};
use crate::desktop::{ShellEvent, ToplevelId, ToplevelImage};

/// The least time between two frames of a window.
pub(super) const INTERVAL: Duration = Duration::from_millis(500);

/// The shm formats we can read, best first. Little-endian: `Argb8888` is B, G, R, A in
/// memory.
const FORMATS: [wl_shm::Format; 4] = [
    wl_shm::Format::Argb8888,
    wl_shm::Format::Xrgb8888,
    wl_shm::Format::Abgr8888,
    wl_shm::Format::Xbgr8888,
];

/// The driver's window captures.
#[derive(Default)]
pub(super) struct Captures {
    globals: Option<CaptureGlobals>,
    /// Handles of the foreign toplevel list, by identifier, once their first `done` came.
    handles: HashMap<String, ExtForeignToplevelHandleV1>,
    /// Identifiers of handles, as they arrive.
    identifiers: HashMap<ObjectId, Option<String>>,
    watches: HashMap<ToplevelId, Watch>,
}

struct CaptureGlobals {
    /// Kept bound for its handles.
    _list: ExtForeignToplevelListV1,
    sources: ExtForeignToplevelImageCaptureSourceManagerV1,
    copy: ExtImageCopyCaptureManagerV1,
}

/// A window the app wants thumbnails of.
struct Watch {
    identifier: String,
    max_side: u32,
    /// `None` until the window's foreign toplevel handle is known.
    session: Option<Session>,
}

/// A capture session and its frame loop.
struct Session {
    source: ExtImageCaptureSourceV1,
    session: ExtImageCopyCaptureSessionV1,
    /// Constraints since the last `done`.
    pending: Constraints,
    /// Constraints as of the last `done`; `None` before the first.
    constraints: Option<Constraints>,
    buffer: Option<Buffer>,
    /// The frame being captured.
    frame: Option<ExtImageCopyCaptureFrameV1>,
    /// The timer that asks for the next frame.
    timer: Option<RegistrationToken>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Constraints {
    size: Option<(u32, u32)>,
    formats: Vec<wl_shm::Format>,
}

/// The shm buffer frames are copied into.
struct Buffer {
    pool: RawPool,
    wl: WlBuffer,
    size: (u32, u32),
    format: wl_shm::Format,
}

impl Session {
    /// Destroys its objects. The timer is removed by the caller, which has the loop.
    fn destroy(self) -> Option<RegistrationToken> {
        if let Some(frame) = self.frame {
            frame.destroy();
        }
        if let Some(buffer) = self.buffer {
            buffer.wl.destroy();
        }
        self.session.destroy();
        self.source.destroy();
        self.timer
    }
}

impl Captures {
    /// Binds the capture globals and the foreign toplevel list, for the shell only.
    pub(super) fn bind(globals: &GlobalList, qh: &QueueHandle<State>, shell: bool) -> Self {
        if !shell {
            return Self::default();
        }
        let list = globals.bind(qh, 1..=1, ListData).ok();
        let sources = globals.bind(qh, 1..=1, NoEvents).ok();
        let copy = globals.bind(qh, 1..=1, NoEvents).ok();
        let globals = match (list, sources, copy) {
            (Some(list), Some(sources), Some(copy)) => Some(CaptureGlobals {
                _list: list,
                sources,
                copy,
            }),
            _ => {
                tracing::warn!("compositor lacks toplevel capture: no window thumbnails");
                None
            }
        };
        Self {
            globals,
            ..Self::default()
        }
    }

    fn session_mut(&mut self, id: ToplevelId) -> Option<&mut Session> {
        self.watches.get_mut(&id)?.session.as_mut()
    }

    /// Starts the session of `id` if its handle is known and it has none.
    fn start(&mut self, id: ToplevelId, qh: &QueueHandle<State>) {
        let Some(globals) = &self.globals else {
            return;
        };
        let Some(watch) = self.watches.get_mut(&id) else {
            return;
        };
        if watch.session.is_some() {
            return;
        }
        let Some(handle) = self.handles.get(&watch.identifier) else {
            return;
        };
        tracing::debug!(
            ?id,
            identifier = watch.identifier,
            "starting a window capture"
        );
        let source = globals.sources.create_source(handle, qh, NoEvents);
        let session = globals
            .copy
            .create_session(&source, Options::empty(), qh, SessionData(id));
        watch.session = Some(Session {
            source,
            session,
            pending: Constraints::default(),
            constraints: None,
            buffer: None,
            frame: None,
            timer: None,
        });
    }
}

impl State {
    /// Handles [`ShellRequest::Thumbnail`](crate::desktop::ShellRequest::Thumbnail).
    pub(super) fn thumbnail_request(&mut self, toplevel: ToplevelId, max_side: Option<u32>) {
        let Some(max_side) = max_side else {
            self.forget_thumbnail(toplevel);
            return;
        };
        let max_side = max_side.max(1);
        if let Some(watch) = self.captures.watches.get_mut(&toplevel) {
            watch.max_side = max_side;
            return;
        }
        if self.captures.globals.is_none() {
            return;
        }
        let Some(identifier) = self.desktop.identifier(toplevel) else {
            tracing::debug!(?toplevel, "thumbnail of a window that has closed");
            return;
        };
        let watch = Watch {
            identifier: identifier.to_owned(),
            max_side,
            session: None,
        };
        self.captures.watches.insert(toplevel, watch);
        self.captures.start(toplevel, &self.qh);
    }

    /// Stops the thumbnails of `toplevel`, if it has any.
    pub(super) fn forget_thumbnail(&mut self, toplevel: ToplevelId) {
        let Some(watch) = self.captures.watches.remove(&toplevel) else {
            return;
        };
        tracing::debug!(?toplevel, "stopping a window capture");
        if let Some(timer) = watch.session.and_then(Session::destroy) {
            self.loop_handle.remove(timer);
        }
    }

    /// Asks for the next frame of `id`, unless one is on its way or there are no
    /// constraints to make a buffer for yet.
    fn capture_frame(&mut self, id: ToplevelId) {
        let qh = self.qh.clone();
        let Some(session) = self.captures.session_mut(id) else {
            return;
        };
        if session.frame.is_some() {
            return;
        }
        let Some(constraints) = &session.constraints else {
            return;
        };
        let format = FORMATS
            .into_iter()
            .find(|f| constraints.formats.contains(f));
        let (Some((w, h)), Some(format)) = (constraints.size, format) else {
            tracing::warn!(?id, ?constraints, "can't capture the window into shm");
            self.forget_thumbnail(id);
            return;
        };
        let fits = |b: &Buffer| b.size == (w, h) && b.format == format;
        if !session.buffer.as_ref().is_some_and(fits) {
            if let Some(old) = session.buffer.take() {
                old.wl.destroy();
            }
            match new_buffer(&self.shm, (w, h), format, &qh) {
                Ok(buffer) => session.buffer = Some(buffer),
                Err(err) => {
                    tracing::warn!(%err, ?id, "can't make a buffer for a window capture");
                    self.forget_thumbnail(id);
                    return;
                }
            }
        }
        let Some(buffer) = &session.buffer else {
            return;
        };
        let frame = session.session.create_frame(&qh, FrameData(id));
        frame.attach_buffer(&buffer.wl);
        // The buffer is read whole after each frame, so all of it is to be filled.
        let (bw, bh) = (clamp_i32(w), clamp_i32(h));
        frame.damage_buffer(0, 0, bw, bh);
        frame.capture();
        session.frame = Some(frame);
    }

    /// Asks for the next frame of `id` after [`INTERVAL`].
    fn schedule_frame(&mut self, id: ToplevelId) {
        let timer = self.loop_handle.insert_source(
            Timer::from_duration(INTERVAL),
            move |_, (), state: &mut State| {
                if let Some(session) = state.captures.session_mut(id) {
                    session.timer = None;
                }
                state.capture_frame(id);
                TimeoutAction::Drop
            },
        );
        match timer {
            Ok(token) => {
                if let Some(session) = self.captures.session_mut(id) {
                    session.timer = Some(token);
                } else {
                    self.loop_handle.remove(token);
                }
            }
            Err(err) => tracing::warn!(%err, "can't time the next window capture"),
        }
    }

    /// The frame of `id` is in its buffer: tell the app, and ask for the next later.
    fn frame_ready(&mut self, id: ToplevelId) {
        let Some(watch) = self.captures.watches.get_mut(&id) else {
            return;
        };
        let max_side = watch.max_side;
        let Some(session) = watch.session.as_mut() else {
            return;
        };
        if let Some(frame) = session.frame.take() {
            frame.destroy();
        }
        let image = session.buffer.as_mut().and_then(|b| {
            let (format, size) = (b.format, b.size);
            thumbnail(b.pool.mmap(), size, format, max_side)
        });
        if let Some(image) = image {
            self.events
                .push(AppEvent::Shell(ShellEvent::Thumbnail(id, image)));
        }
        self.schedule_frame(id);
    }
}

/// An shm buffer of `size` in `format`, with its pool.
fn new_buffer(
    shm: &smithay_client_toolkit::shm::Shm,
    (w, h): (u32, u32),
    format: wl_shm::Format,
    qh: &QueueHandle<State>,
) -> Result<Buffer, smithay_client_toolkit::shm::CreatePoolError> {
    let stride = w.saturating_mul(4);
    let len = usize::try_from(u64::from(stride) * u64::from(h)).unwrap_or(usize::MAX);
    let mut pool = RawPool::new(len.max(4), shm)?;
    let wl = pool.create_buffer(
        0,
        clamp_i32(w),
        clamp_i32(h),
        clamp_i32(stride),
        format,
        NoEvents,
        qh,
    );
    Ok(Buffer {
        pool,
        wl,
        size: (w, h),
        format,
    })
}

fn clamp_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// The size of a `(w, h)` image scaled down to fit in `max_side` square, keeping its
/// proportions; never larger than it is, never a side below 1.
fn fitted((w, h): (u32, u32), max_side: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_side {
        return (w, h);
    }
    let scale = |v: u32| {
        let scaled = (u64::from(v) * u64::from(max_side) + u64::from(long) / 2) / u64::from(long);
        u32::try_from(scaled).unwrap_or(max_side).max(1)
    };
    (scale(w), scale(h))
}

/// The packed shm image `data` (`size`, rows `size.0 × 4` bytes apart, in `format`) as
/// premultiplied RGBA8, box-filtered down to fit in `max_side`. `None` for an empty image,
/// a short buffer or a format we don't read.
fn thumbnail(
    data: &[u8],
    (w, h): (u32, u32),
    format: wl_shm::Format,
    max_side: u32,
) -> Option<ToplevelImage> {
    // Byte offsets of red, green, blue and alpha (`None`: opaque).
    let (r, g, b, a) = match format {
        wl_shm::Format::Argb8888 => (2, 1, 0, Some(3)),
        wl_shm::Format::Xrgb8888 => (2, 1, 0, None),
        wl_shm::Format::Abgr8888 => (0, 1, 2, Some(3)),
        wl_shm::Format::Xbgr8888 => (0, 1, 2, None),
        _ => return None,
    };
    let (sw, sh) = (usize::try_from(w).ok()?, usize::try_from(h).ok()?);
    let stride = sw.checked_mul(4)?;
    if sw == 0 || sh == 0 || data.len() < stride.checked_mul(sh)? {
        return None;
    }
    let (dw, dh) = fitted((w, h), max_side);
    let (dwu, dhu) = (usize::try_from(dw).ok()?, usize::try_from(dh).ok()?);
    let mut out = vec![0_u8; dwu * dhu * 4];
    // The source pixels of destination column or row `d`: [d·n/m, (d+1)·n/m), at least one.
    let span = |d: usize, n: usize, m: usize| {
        let start = d * n / m;
        (start, ((d + 1) * n / m).max(start + 1))
    };
    for dy in 0..dhu {
        let (y0, y1) = span(dy, sh, dhu);
        for dx in 0..dwu {
            let (x0, x1) = span(dx, sw, dwu);
            let mut sum = [0_u32; 4];
            for row in data[y0 * stride..y1 * stride].chunks_exact(stride) {
                for px in row[x0 * 4..x1 * 4].as_chunks::<4>().0 {
                    sum[0] += u32::from(px[r]);
                    sum[1] += u32::from(px[g]);
                    sum[2] += u32::from(px[b]);
                    sum[3] += a.map_or(255, |a| u32::from(px[a]));
                }
            }
            // At most `max_side`² source pixels per box in practice; far below u32::MAX/255
            // for any real window.
            let n = u32::try_from((y1 - y0) * (x1 - x0)).unwrap_or(u32::MAX);
            let at = (dy * dwu + dx) * 4;
            for (c, s) in out[at..at + 4].iter_mut().zip(sum) {
                // An average of bytes is a byte.
                *c = u8::try_from((s + n / 2) / n).unwrap_or(u8::MAX);
            }
        }
    }
    Some(ToplevelImage {
        width: dw,
        height: dh,
        pixels: Arc::from(out),
    })
}

/// User data for `ext_foreign_toplevel_list_v1`.
struct ListData;

impl Dispatch2<ExtForeignToplevelListV1, State> for ListData {
    fn event(
        &self,
        state: &mut State,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.captures.identifiers.insert(toplevel.id(), None);
        }
    }

    fn event_created_child(opcode: u16, qh: &QueueHandle<State>) -> Arc<dyn ObjectData> {
        assert_eq!(
            opcode,
            ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE,
            "ext_foreign_toplevel_list_v1 creates objects only in its toplevel event"
        );
        qh.make_data::<ExtForeignToplevelHandleV1, _>(HandleData)
    }
}

/// User data for `ext_foreign_toplevel_handle_v1`.
struct HandleData;

impl Dispatch2<ExtForeignToplevelHandleV1, State> for HandleData {
    fn event(
        &self,
        state: &mut State,
        proxy: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &Connection,
        qh: &QueueHandle<State>,
    ) {
        use ext_foreign_toplevel_handle_v1::Event;
        let captures = &mut state.captures;
        match event {
            Event::Identifier { identifier } => {
                captures.identifiers.insert(proxy.id(), Some(identifier));
            }
            Event::Done => {
                let Some(Some(identifier)) = captures.identifiers.get(&proxy.id()) else {
                    return;
                };
                if captures.handles.contains_key(identifier) {
                    return;
                }
                let identifier = identifier.clone();
                captures.handles.insert(identifier.clone(), proxy.clone());
                // A window asked for before its handle arrived.
                let waiting: Vec<ToplevelId> = captures
                    .watches
                    .iter()
                    .filter(|(_, w)| w.identifier == identifier)
                    .map(|(id, _)| *id)
                    .collect();
                for id in waiting {
                    captures.start(id, qh);
                }
            }
            Event::Closed => {
                if let Some(Some(identifier)) = captures.identifiers.remove(&proxy.id()) {
                    captures.handles.remove(&identifier);
                }
                proxy.destroy();
            }
            _ => {}
        }
    }
}

/// User data for a capture session: the window it captures.
struct SessionData(ToplevelId);

impl Dispatch2<ExtImageCopyCaptureSessionV1, State> for SessionData {
    fn event(
        &self,
        state: &mut State,
        proxy: &ExtImageCopyCaptureSessionV1,
        event: ext_image_copy_capture_session_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use ext_image_copy_capture_session_v1::Event;
        let id = self.0;
        let Some(session) = state
            .captures
            .session_mut(id)
            .filter(|s| s.session == *proxy)
        else {
            return;
        };
        match event {
            Event::BufferSize { width, height } => session.pending.size = Some((width, height)),
            Event::ShmFormat {
                format: WEnum::Value(format),
            } => session.pending.formats.push(format),
            Event::Done => {
                session.constraints = Some(std::mem::take(&mut session.pending));
                if session.timer.is_none() {
                    state.capture_frame(id);
                }
            }
            Event::Stopped => {
                tracing::debug!(?id, "the compositor stopped a window capture");
                state.forget_thumbnail(id);
            }
            _ => {}
        }
    }
}

/// User data for a capture frame: the window it captures.
struct FrameData(ToplevelId);

impl Dispatch2<ExtImageCopyCaptureFrameV1, State> for FrameData {
    fn event(
        &self,
        state: &mut State,
        proxy: &ExtImageCopyCaptureFrameV1,
        event: ext_image_copy_capture_frame_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use ext_image_copy_capture_frame_v1::{Event, FailureReason};
        let id = self.0;
        let current = state
            .captures
            .session_mut(id)
            .is_some_and(|s| s.frame.as_ref() == Some(proxy));
        if !current {
            return;
        }
        match event {
            Event::Ready => state.frame_ready(id),
            Event::Failed { reason } => {
                if let Some(frame) = state.captures.session_mut(id).and_then(|s| s.frame.take()) {
                    frame.destroy();
                }
                if reason == WEnum::Value(FailureReason::Stopped) {
                    state.forget_thumbnail(id);
                } else {
                    // New constraints come with `done`; the next frame uses them.
                    tracing::debug!(?id, ?reason, "a window capture frame failed");
                    state.schedule_frame(id);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_keeps_proportions_and_never_grows() {
        assert_eq!(fitted((800, 600), 200), (200, 150));
        assert_eq!(fitted((600, 800), 200), (150, 200));
        assert_eq!(fitted((100, 50), 200), (100, 50));
        assert_eq!(fitted((4000, 1), 100), (100, 1));
    }

    #[test]
    fn thumbnails_swizzle_and_average() {
        // 2×2 ARGB8888 (B, G, R, A bytes): red, green, blue, transparent.
        let data = [
            0, 0, 255, 255, 0, 255, 0, 255, //
            255, 0, 0, 255, 0, 0, 0, 0,
        ];
        let full = thumbnail(&data, (2, 2), wl_shm::Format::Argb8888, 8).unwrap();
        assert_eq!((full.width, full.height), (2, 2));
        assert_eq!(&full.pixels[..8], &[255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(&full.pixels[12..], &[0, 0, 0, 0]);
        // One pixel: the average, premultiplied as the buffer is.
        let one = thumbnail(&data, (2, 2), wl_shm::Format::Argb8888, 1).unwrap();
        assert_eq!((one.width, one.height), (1, 1));
        assert_eq!(&one.pixels[..], &[64, 64, 64, 191]);
    }

    #[test]
    fn x_formats_are_opaque() {
        let data = [10, 20, 30, 0];
        let xrgb = thumbnail(&data, (1, 1), wl_shm::Format::Xrgb8888, 4).unwrap();
        assert_eq!(&xrgb.pixels[..], &[30, 20, 10, 255]);
        let xbgr = thumbnail(&data, (1, 1), wl_shm::Format::Xbgr8888, 4).unwrap();
        assert_eq!(&xbgr.pixels[..], &[10, 20, 30, 255]);
    }

    #[test]
    fn short_buffers_and_odd_formats_give_nothing() {
        assert!(thumbnail(&[0; 12], (2, 2), wl_shm::Format::Argb8888, 4).is_none());
        assert!(thumbnail(&[0; 16], (2, 2), wl_shm::Format::Rgb565, 4).is_none());
        assert!(thumbnail(&[], (0, 0), wl_shm::Format::Argb8888, 4).is_none());
    }
}
