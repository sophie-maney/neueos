//! `org.freedesktop.Notifications`: the notification server (task 3.15a, ADR 0089), owned
//! by the shell. Version 1.2 of the Desktop Notifications spec, with the `ActivationToken`
//! signal.
//!
//! [`Notifications::serve`] puts the object on a connection and hands every request to a
//! callback: a new or replacing [`Notification`] ([`Request::Notify`]) or an app closing
//! one ([`Request::Close`]). The caller shows them and reports back through the handle:
//! [`Notifications::close`] when a banner expires or is dismissed, and
//! [`Notifications::invoke`] when the user picks an action. The server keeps only the ids
//! that are open, so it can answer `Notify` and `CloseNotification` itself; expiry and
//! what is shown are the shell's (`neue_shell::notifications`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::fdo::{self, RequestNameFlags};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

use crate::Error;

/// The well-known name.
pub const NAME: &str = "org.freedesktop.Notifications";
/// The object path.
pub const PATH: &str = "/org/freedesktop/Notifications";
/// The interface.
pub const INTERFACE: &str = "org.freedesktop.Notifications";
/// The spec version we implement.
pub const SPEC_VERSION: &str = "1.2";

/// What `GetCapabilities` answers. `body-markup` waits until banners draw markup; until
/// then apps send plain text.
pub const CAPABILITIES: &[&str] = &["actions", "body", "icon-static"];

/// The action key the spec reserves for a click on the notification itself.
pub const DEFAULT_ACTION: &str = "default";

/// No client can keep more notifications than this open at once.
const LIMIT: usize = 1024;
/// Images larger than this on either side are dropped (they would be scaled down to an
/// icon anyway).
const MAX_IMAGE_SIDE: i32 = 4096;

/// How urgent a notification is (the `urgency` hint).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    /// Shown as an alert and never expires on its own.
    Critical,
}

/// Why a notification closed (`NotificationClosed`'s reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// Its time ran out.
    Expired = 1,
    /// The user dismissed it (or picked an action).
    Dismissed = 2,
    /// The app called `CloseNotification`.
    Closed = 3,
    /// Anything else (the server dropped it).
    Undefined = 4,
}

/// How long a notification asks to stay up (`expire_timeout`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeout {
    /// The server decides (−1, and any other negative value).
    Default,
    /// Until the user dismisses it (0).
    Never,
    /// This long.
    After(Duration),
}

impl Timeout {
    /// From the wire's milliseconds.
    pub fn from_millis(ms: i32) -> Self {
        match ms {
            0 => Self::Never,
            ms if ms < 0 => Self::Default,
            ms => Self::After(Duration::from_millis(ms.unsigned_abs().into())),
        }
    }
}

/// Pixels from `image-data` (or `image_data`, `icon_data`), as straight 8-bit RGBA rows.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// The spec's `(iiibiiay)`: width, height, rowstride, has alpha, bits per sample,
    /// channels, data. Only 8-bit RGB and RGBA are accepted (what every client sends).
    fn from_value(value: &Value<'_>) -> Option<Self> {
        let Value::Structure(s) = value else {
            return None;
        };
        let [w, h, stride, alpha, bits, channels, data] = s.fields() else {
            return None;
        };
        let int = |v: &Value<'_>| i32::try_from(v).ok();
        let (w, h, stride) = (int(w)?, int(h)?, int(stride)?);
        let (bits, channels) = (int(bits)?, int(channels)?);
        let alpha = bool::try_from(alpha).ok()?;
        let Value::Array(data) = data else {
            return None;
        };
        let data = data
            .iter()
            .map(|b| u8::try_from(b).ok())
            .collect::<Option<Vec<u8>>>()?;
        Self::from_raw(w, h, stride, alpha, bits, channels, &data)
    }

    fn from_raw(
        w: i32,
        h: i32,
        stride: i32,
        alpha: bool,
        bits: i32,
        channels: i32,
        data: &[u8],
    ) -> Option<Self> {
        if !(1..=MAX_IMAGE_SIDE).contains(&w) || !(1..=MAX_IMAGE_SIDE).contains(&h) {
            return None;
        }
        if bits != 8 || channels != if alpha { 4 } else { 3 } {
            return None;
        }
        let (w, h) = (w.unsigned_abs() as usize, h.unsigned_abs() as usize);
        let channels = channels.unsigned_abs() as usize;
        let stride = usize::try_from(stride).ok()?;
        let row = w * channels;
        // The last row may stop at its pixels, without the padding.
        if stride < row || data.len() < stride * (h - 1) + row {
            return None;
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let line = &data[y * stride..y * stride + row];
            if alpha {
                rgba.extend_from_slice(line);
            } else {
                for px in line.as_chunks::<3>().0 {
                    rgba.extend_from_slice(&[px[0], px[1], px[2], 0xFF]);
                }
            }
        }
        Some(Self {
            width: u32::try_from(w).ok()?,
            height: u32::try_from(h).ok()?,
            rgba,
        })
    }
}

/// The hints we read; the others (`x`, `y`, sounds, `action-icons`) are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hints {
    pub urgency: Urgency,
    /// `category`, such as `im.received` or `email.arrived`.
    pub category: Option<String>,
    /// `desktop-entry`: the app's desktop entry name without `.desktop`.
    pub desktop_entry: Option<String>,
    /// `image-data` (or the older `image_data`).
    pub image: Option<Image>,
    /// `image-path` (or `image_path`): an icon name, a path or a `file://` URI.
    pub image_path: Option<String>,
    /// `icon_data`, the oldest spelling, below `app_icon` in priority.
    pub icon_data: Option<Image>,
    /// `resident`: an action doesn't close it.
    pub resident: bool,
    /// `transient`: it shouldn't be kept once it's off screen.
    pub transient: bool,
}

impl Hints {
    /// Reads the hints we know from the wire's `a{sv}`, skipping values of the wrong type.
    pub fn parse(hints: &HashMap<String, OwnedValue>) -> Self {
        let get = |key: &str| hints.get(key).map(|v| &**v);
        let string = |key| get(key).and_then(|v| <&str>::try_from(v).ok().map(str::to_owned));
        let image = |key| get(key).and_then(Image::from_value);
        let flag = |key| get(key).is_some_and(|v| bool::try_from(v).unwrap_or(false));
        Self {
            urgency: get("urgency").map_or(Urgency::Normal, urgency),
            category: string("category"),
            desktop_entry: string("desktop-entry").filter(|s| !s.is_empty()),
            image: image("image-data").or_else(|| image("image_data")),
            image_path: string("image-path")
                .or_else(|| string("image_path"))
                .filter(|s| !s.is_empty()),
            icon_data: image("icon_data"),
            resident: flag("resident"),
            transient: flag("transient"),
        }
    }
}

/// The spec says a byte; some clients send another integer type.
fn urgency(value: &Value<'_>) -> Urgency {
    let level = match value {
        Value::U8(n) => i64::from(*n),
        Value::I16(n) => i64::from(*n),
        Value::U16(n) => i64::from(*n),
        Value::I32(n) => i64::from(*n),
        Value::U32(n) => i64::from(*n),
        Value::I64(n) => *n,
        _ => 1,
    };
    match level {
        0 => Urgency::Low,
        2 => Urgency::Critical,
        _ => Urgency::Normal,
    }
}

/// Where a notification's icon comes from, in the spec's order of priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSource<'a> {
    Pixels(&'a Image),
    /// A file (from a path or a `file://` URI).
    File(&'a str),
    /// An icon name to look up in the theme.
    Name(&'a str),
}

impl<'a> IconSource<'a> {
    /// What `image-path`, `app_icon` or a desktop entry's `Icon` names: a `file://` URI, an
    /// absolute path or an icon name; `None` when empty.
    pub fn parse(s: &'a str) -> Option<Self> {
        if s.is_empty() {
            None
        } else if let Some(path) = s.strip_prefix("file://") {
            Some(Self::File(path))
        } else if s.starts_with('/') {
            Some(Self::File(s))
        } else {
            Some(Self::Name(s))
        }
    }

    /// The file's path, for [`IconSource::File`]. Percent escapes in a URI are decoded.
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            Self::File(s) => Some(PathBuf::from(percent_decode(s))),
            _ => None,
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            // Two hex digits make at most 0xFF.
            out.push(u8::try_from(hi * 16 + lo).unwrap_or(b'?'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One notification as an app sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Its id: new, or the one it replaces.
    pub id: u32,
    /// Whether it replaces an open notification with the same id.
    pub replaces: bool,
    pub app_name: String,
    /// An icon name, a path or a `file://` URI; may be empty.
    pub app_icon: String,
    pub summary: String,
    /// Plain text (we don't announce `body-markup`), though some apps send markup anyway.
    pub body: String,
    /// `(key, label)` pairs in the app's order, [`DEFAULT_ACTION`] among them.
    pub actions: Vec<(String, String)>,
    pub hints: Hints,
    pub timeout: Timeout,
}

impl Notification {
    /// The icon to show, in the spec's order: `image-data`, `image-path`, `app_icon`,
    /// `icon_data`.
    pub fn icon(&self) -> Option<IconSource<'_>> {
        self.hints
            .image
            .as_ref()
            .map(IconSource::Pixels)
            .or_else(|| self.hints.image_path.as_deref().and_then(IconSource::parse))
            .or_else(|| IconSource::parse(&self.app_icon))
            .or_else(|| self.hints.icon_data.as_ref().map(IconSource::Pixels))
    }

    /// Whether a click on the notification invokes [`DEFAULT_ACTION`].
    pub fn has_default_action(&self) -> bool {
        self.actions.iter().any(|(key, _)| key == DEFAULT_ACTION)
    }

    /// The actions shown as buttons: all but [`DEFAULT_ACTION`].
    pub fn buttons(&self) -> impl Iterator<Item = &(String, String)> {
        self.actions.iter().filter(|(key, _)| key != DEFAULT_ACTION)
    }
}

/// `actions`' flat list as `(key, label)` pairs; an odd last key is dropped. A label left
/// empty takes its key.
fn pairs(flat: Vec<String>) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(flat.len() / 2);
    let mut it = flat.into_iter();
    while let (Some(key), Some(label)) = (it.next(), it.next()) {
        let label = if label.is_empty() { key.clone() } else { label };
        out.push((key, label));
    }
    out
}

/// What the server hands to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Show this notification (or update the open one with its id).
    Notify(Box<Notification>),
    /// The app closed this notification; `NotificationClosed` is already sent.
    Close(u32),
}

type Callback = Box<dyn Fn(Request) + Send + Sync>;

#[derive(Debug)]
struct State {
    /// The last id handed out.
    last: u32,
    open: HashSet<u32>,
}

type Shared = Arc<Mutex<State>>;

fn state(s: &Shared) -> MutexGuard<'_, State> {
    // Writers insert or remove whole ids; a poisoned state is still consistent.
    s.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    /// The id for a `Notify` with `replaces_id`, and whether it replaces an open one.
    fn assign(&mut self, replaces_id: u32) -> fdo::Result<(u32, bool)> {
        if replaces_id != 0 && self.open.contains(&replaces_id) {
            return Ok((replaces_id, true));
        }
        if self.open.len() >= LIMIT {
            return Err(fdo::Error::LimitsExceeded(
                "too many open notifications".into(),
            ));
        }
        // The spec returns `replaces_id` even when it is no longer open.
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            loop {
                self.last = self.last.wrapping_add(1);
                if self.last != 0 && !self.open.contains(&self.last) {
                    break self.last;
                }
            }
        };
        self.open.insert(id);
        Ok((id, false))
    }
}

struct Server {
    state: Shared,
    callback: Callback,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    #[allow(clippy::too_many_arguments)] // the spec's method
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> fdo::Result<u32> {
        let (id, replaces) = state(&self.state).assign(replaces_id)?;
        (self.callback)(Request::Notify(Box::new(Notification {
            id,
            replaces,
            app_name,
            app_icon,
            summary,
            body,
            actions: pairs(actions),
            hints: Hints::parse(&hints),
            timeout: Timeout::from_millis(expire_timeout),
        })));
        Ok(id)
    }

    /// Closing an id that isn't open does nothing: apps close stale ids routinely, and the
    /// spec's "empty error" only makes noise in their logs.
    async fn close_notification(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> fdo::Result<()> {
        if !state(&self.state).open.remove(&id) {
            return Ok(());
        }
        Self::notification_closed(&emitter, id, CloseReason::Closed as u32).await?;
        (self.callback)(Request::Close(id));
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        CAPABILITIES.to_vec()
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        (
            "neue-shell",
            "neueOS",
            env!("CARGO_PKG_VERSION"),
            SPEC_VERSION,
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activation_token(
        emitter: &SignalEmitter<'_>,
        id: u32,
        activation_token: &str,
    ) -> zbus::Result<()>;
}

/// The notification server on a connection. Dropping it removes the object; the name, if
/// owned, goes when the connection closes.
#[derive(Debug)]
pub struct Notifications {
    conn: Connection,
    state: Shared,
}

impl Notifications {
    /// Serves the server at [`PATH`] on `conn` without asking for the name (for
    /// peer-to-peer tests). See [`Notifications::own`].
    ///
    /// `callback` runs on zbus's executor thread, before the app gets its reply: hand the
    /// request on (to a channel or an event loop) and return; don't block.
    pub fn serve(
        conn: &Connection,
        callback: impl Fn(Request) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let state = Shared::new(Mutex::new(State {
            last: 0,
            open: HashSet::new(),
        }));
        let server = Server {
            state: Arc::clone(&state),
            callback: Box::new(callback),
        };
        if !conn.object_server().at(PATH, server)? {
            return Err(Error::PathTaken(PATH.to_owned()));
        }
        Ok(Self {
            conn: conn.clone(),
            state,
        })
    }

    /// Serves the server and takes [`NAME`], replacing another owner that allows it (a
    /// shell that is restarting) and letting a later one replace us.
    pub fn own(
        conn: &Connection,
        callback: impl Fn(Request) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let server = Self::serve(conn, callback)?;
        conn.request_name_with_flags(
            NAME,
            RequestNameFlags::ReplaceExisting | RequestNameFlags::AllowReplacement,
        )?;
        Ok(server)
    }

    /// Whether `id` is open.
    pub fn is_open(&self, id: u32) -> bool {
        state(&self.state).open.contains(&id)
    }

    /// Closes `id` for `reason` and tells the app (`NotificationClosed`). `false` (and no
    /// signal) when it wasn't open.
    pub fn close(&self, id: u32, reason: CloseReason) -> Result<bool, Error> {
        if !state(&self.state).open.remove(&id) {
            return Ok(false);
        }
        self.emit("NotificationClosed", &(id, reason as u32))?;
        Ok(true)
    }

    /// The user picked `action` on `id` ([`DEFAULT_ACTION`] for a click on it): sends
    /// `ActivationToken` first when there is a `token` (for the app to raise its window
    /// with), then `ActionInvoked`. The notification stays open; close it unless it is
    /// [`Hints::resident`]. `false` (and no signal) when `id` wasn't open.
    pub fn invoke(&self, id: u32, action: &str, token: Option<&str>) -> Result<bool, Error> {
        if !self.is_open(id) {
            return Ok(false);
        }
        if let Some(token) = token {
            self.emit("ActivationToken", &(id, token))?;
        }
        self.emit("ActionInvoked", &(id, action))?;
        Ok(true)
    }

    fn emit<B>(&self, member: &str, body: &B) -> Result<(), Error>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.conn
            .emit_signal(None::<()>, PATH, INTERFACE, member, body)?;
        Ok(())
    }
}

impl Drop for Notifications {
    fn drop(&mut self) {
        if let Err(err) = self.conn.object_server().remove::<Server, _>(PATH) {
            tracing::debug!(%err, "couldn't remove the notifications object");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> State {
        State {
            last: 0,
            open: HashSet::new(),
        }
    }

    #[test]
    fn ids_count_up_and_replacements_keep_theirs() {
        let mut s = fresh();
        assert_eq!(s.assign(0).unwrap(), (1, false));
        assert_eq!(s.assign(0).unwrap(), (2, false));
        assert_eq!(s.assign(1).unwrap(), (1, true));
        // A closed id comes back as itself, as a new notification.
        s.open.remove(&2);
        assert_eq!(s.assign(2).unwrap(), (2, false));
        // A client-chosen id isn't handed out again while it's open.
        assert_eq!(s.assign(3).unwrap(), (3, false));
        assert_eq!(s.assign(0).unwrap(), (4, false));
    }

    #[test]
    fn ids_wrap_past_zero_and_skip_open_ones() {
        let mut s = fresh();
        s.last = u32::MAX - 1;
        s.open.insert(u32::MAX);
        s.open.insert(1);
        assert_eq!(s.assign(0).unwrap(), (2, false));
    }

    #[test]
    fn too_many_open_is_refused() {
        let mut s = fresh();
        for _ in 0..LIMIT {
            s.assign(0).unwrap();
        }
        assert!(s.assign(0).is_err());
        assert_eq!(s.assign(5).unwrap(), (5, true), "replacing still works");
    }

    #[test]
    fn actions_pair_up() {
        let flat = ["default", "", "reply", "Reply", "odd"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            pairs(flat),
            [
                ("default".into(), "default".into()),
                ("reply".into(), "Reply".into())
            ]
        );
    }

    #[test]
    fn timeouts_read_the_spec_values() {
        assert_eq!(Timeout::from_millis(-1), Timeout::Default);
        assert_eq!(Timeout::from_millis(-7), Timeout::Default);
        assert_eq!(Timeout::from_millis(0), Timeout::Never);
        assert_eq!(
            Timeout::from_millis(2500),
            Timeout::After(Duration::from_millis(2500))
        );
    }

    #[test]
    fn rgb_images_gain_alpha_and_lose_padding() {
        // 2x2 RGB with a stride of 8 (2 bytes of padding); the last row is unpadded.
        let data = [1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12];
        let image = Image::from_raw(2, 2, 8, false, 8, 3, &data).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(
            image.rgba,
            [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
    }

    #[test]
    fn bad_images_are_dropped() {
        let data = [0u8; 64];
        assert!(
            Image::from_raw(0, 2, 8, true, 8, 4, &data).is_none(),
            "empty"
        );
        assert!(
            Image::from_raw(2, 2, 8, true, 16, 4, &data).is_none(),
            "16 bit"
        );
        assert!(
            Image::from_raw(2, 2, 8, true, 8, 3, &data).is_none(),
            "alpha mismatch"
        );
        assert!(
            Image::from_raw(2, 2, 4, true, 8, 4, &data).is_none(),
            "short stride"
        );
        assert!(
            Image::from_raw(4, 4, 16, true, 8, 4, &data[..60]).is_none(),
            "short data"
        );
        assert!(Image::from_raw(4, 4, 16, true, 8, 4, &data).is_some());
    }

    #[test]
    fn hints_read_the_known_keys() {
        let image = Value::from((1i32, 1i32, 4i32, true, 8i32, 4i32, vec![9u8, 8, 7, 6]));
        let mut h: HashMap<String, OwnedValue> = HashMap::new();
        let mut put = |k: &str, v: Value<'_>| {
            h.insert(k.into(), v.try_into_owned().unwrap());
        };
        put("urgency", Value::U8(2));
        put("category", Value::from("im.received"));
        put("desktop-entry", Value::from("org.example.Chat"));
        put("image_data", image);
        put("image-path", Value::from("file:///tmp/a%20b.png"));
        put("resident", Value::Bool(true));
        put("transient", Value::from("not a bool"));
        let hints = Hints::parse(&h);
        assert_eq!(hints.urgency, Urgency::Critical);
        assert_eq!(hints.category.as_deref(), Some("im.received"));
        assert_eq!(hints.desktop_entry.as_deref(), Some("org.example.Chat"));
        assert_eq!(hints.image.as_ref().unwrap().rgba, [9, 8, 7, 6]);
        assert!(hints.resident);
        assert!(!hints.transient, "a hint of the wrong type is ignored");
        assert_eq!(Hints::parse(&HashMap::new()).urgency, Urgency::Normal);
        let mut h = HashMap::new();
        h.insert("urgency".into(), OwnedValue::from(0i32));
        assert_eq!(Hints::parse(&h).urgency, Urgency::Low, "an int32 urgency");
    }

    #[test]
    fn icons_follow_the_spec_priority() {
        let image = Image::from_raw(1, 1, 4, true, 8, 4, &[0; 4]).unwrap();
        let mut n = Notification {
            id: 1,
            replaces: false,
            app_name: "Chat".into(),
            app_icon: "chat".into(),
            summary: String::new(),
            body: String::new(),
            actions: Vec::new(),
            hints: Hints {
                icon_data: Some(image.clone()),
                ..Hints::default()
            },
            timeout: Timeout::Default,
        };
        assert_eq!(n.icon(), Some(IconSource::Name("chat")));
        n.hints.image_path = Some("file:///tmp/a%20b.png".into());
        let icon = n.icon().unwrap();
        assert_eq!(icon, IconSource::File("/tmp/a%20b.png"));
        assert_eq!(icon.path(), Some(PathBuf::from("/tmp/a b.png")));
        n.hints.image = Some(image.clone());
        assert_eq!(n.icon(), Some(IconSource::Pixels(&image)));
        n.hints = Hints {
            icon_data: Some(image.clone()),
            ..Hints::default()
        };
        n.app_icon.clear();
        assert_eq!(n.icon(), Some(IconSource::Pixels(&image)), "icon_data last");
    }
}
