//! Notifications in the shell (task 3.15d3, ADR 0091): the shell owns
//! `org.freedesktop.Notifications` and shows what apps send as banners and alerts in a
//! [`NotificationHost`] at the top right of the active output.
//!
//! Two halves:
//! - a worker thread ([`run`]) owns the server and prepares each notification (the icon
//!   from the theme or the pixels the app sent, the plain-text body, `shell.toml`'s
//!   `[notifications]` as it is now) so that the UI thread never reads a file or decodes
//!   an image, and keeps requests in the order they arrived;
//! - [`Notifier`] on the UI thread keeps the open notifications ([`Store`]: replacement,
//!   timeouts, pausing while hovered), the layer surface with the host, and answers the
//!   user's clicks with the app's signals.
//!
//! The surface is opened when the first notification shows and closed once the last panel
//! has slid out. It is opened without an output, so the compositor puts it on the active
//! one, as for the OSD.

use std::collections::BTreeSet;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use neue_config::{Layers, NotificationsConfig, ShellConfig};
use neue_dbus::Notifications;
use neue_dbus::notifications::{
    CloseReason, DEFAULT_ACTION, IconSource, Image, Notification, Request, Timeout, Urgency,
};
use neue_desktop::{AppDatabase, IconLoader, IconLookup, Locale};
use neue_kit::desktop::{
    Anchor, LayerKeyboard, LayerSpec, NotificationContent, NotificationEvent, NotificationHost,
    Picture, ShellLayer,
};
use neue_kit::tokens::materials::notification::{ICON_SIZE, MARGIN, WIDTH};
use neue_kit::{Cx, Handle, Icon, WindowId};
use neue_shell::notifications::{Inserted, Lifetime, Store, lifetime};
use neue_shell::picture;

use crate::tray::{FALLBACK_THEME, ICON_THEME, plain_text};

/// The surface's layer-shell namespace.
const NAMESPACE: &str = "neue-notifications";

/// Notifications kept open at once, shown or not (Do Not Disturb keeps them unseen); the
/// oldest is dropped past this.
const CAPACITY: usize = 64;

/// Banners on screen at once; older ones stay open but are taken off the screen.
const MAX_SHOWN: usize = 5;

/// Icons are loaded for this scale, so they stay sharp on a doubled output.
const ICON_SCALE: u32 = 2;

/// A notification that arrived, ready to show.
#[derive(Debug, Clone)]
pub struct Arrived {
    pub notification: Notification,
    pub content: NotificationContent,
    /// `[notifications]` as it was when the notification arrived (Do Not Disturb is
    /// followed this way, without a watcher).
    pub config: NotificationsConfig,
}

/// What the worker, the host and the timers tell the UI thread.
#[derive(Debug, Clone)]
pub enum Event {
    /// The server owns its name; the UI thread acts through this.
    Server(Arc<Notifications>),
    /// The worker stopped (it logged why).
    Ended,
    /// An app sent a notification, or replaced one.
    Notify(Box<Arrived>),
    /// An app closed notification `id` (it already got its `NotificationClosed`).
    Closed(u32),
    /// The surface exists now: panels waiting for it can go in.
    Opened,
    /// The user did something to a panel.
    Panel(u32, NotificationEvent),
    /// Panel `id` has slid out.
    Gone(u32),
    /// A timeout may have run out.
    Timer,
    /// The compositor answered a token request made for `action` on `id`.
    Token {
        id: u32,
        action: String,
        token: Option<String>,
    },
}

// ---------------------------------------------------------------------------------------
// The worker

/// Owns the server on the session bus and sends the UI thread what it is asked, until the
/// UI thread stops listening (`send` returns false).
pub fn run(send: impl Fn(Event) -> bool) -> Result<(), neue_dbus::Error> {
    let conn = neue_dbus::connect_session()?;
    serve(&conn, true, &Layers::standard(), send)
}

/// [`run`] on `conn`; `own` takes the well-known name (not in peer-to-peer tests).
fn serve(
    conn: &neue_dbus::zbus::blocking::Connection,
    own: bool,
    layers: &Layers,
    send: impl Fn(Event) -> bool,
) -> Result<(), neue_dbus::Error> {
    let (tx, rx) = mpsc::channel();
    let callback = move |request| {
        // The receiver is gone when the worker is.
        let _ = tx.send(request);
    };
    let server = if own {
        Notifications::own(conn, callback)?
    } else {
        Notifications::serve(conn, callback)?
    };
    if !send(Event::Server(Arc::new(server))) {
        return Ok(());
    }
    let mut preparer = Preparer::new();
    // Ends when the UI thread lets go of the server (the callback's sender with it).
    while let Ok(request) = rx.recv() {
        let alive = match request {
            Request::Notify(notification) => {
                let content = preparer.content(&notification);
                send(Event::Notify(Box::new(Arrived {
                    notification: *notification,
                    content,
                    config: config(layers),
                })))
            }
            Request::Close(id) => send(Event::Closed(id)),
        };
        if !alive {
            break;
        }
    }
    Ok(())
}

/// `shell.toml`'s `[notifications]`; the defaults when the file doesn't load.
fn config(layers: &Layers) -> NotificationsConfig {
    match neue_config::load::<ShellConfig>(layers) {
        Ok(loaded) => loaded.config.notifications,
        Err(error) => {
            tracing::warn!(%error, "shell.toml is invalid: using the default notification settings");
            NotificationsConfig::default()
        }
    }
}

/// Turns what an app sent into what the panel shows.
struct Preparer {
    loader: IconLoader,
    /// Read when a notification names a `desktop-entry` and sends no icon of its own.
    apps: Option<AppDatabase>,
}

impl Preparer {
    fn new() -> Self {
        Self {
            loader: IconLoader::new(IconLookup::system(ICON_THEME, Some(FALLBACK_THEME))),
            apps: None,
        }
    }

    fn content(&mut self, n: &Notification) -> NotificationContent {
        NotificationContent {
            title: n.summary.clone(),
            // Some apps send markup although we don't announce `body-markup`.
            body: plain_text(&n.body),
            app_icon: self.app_icon(n),
            image: self.image(n),
            actions: n.buttons().map(|(_, label)| label.clone()).collect(),
        }
    }

    /// The big picture: `image-data`, or `image-path` when it is a file.
    fn image(&self, n: &Notification) -> Option<Picture> {
        if let Some(pixels) = n.hints.image.as_ref().and_then(pixels) {
            return Some(pixels);
        }
        let source = n.hints.image_path.as_deref().and_then(IconSource::parse)?;
        match source {
            IconSource::File(_) => file(&source),
            _ => None,
        }
    }

    /// The app's icon: `app_icon`, `image-path` when it names a theme icon, the desktop
    /// entry's icon, then the deprecated `icon_data`.
    fn app_icon(&mut self, n: &Notification) -> Option<Icon> {
        for named in [
            n.app_icon.as_str(),
            n.hints.image_path.as_deref().unwrap_or(""),
        ] {
            if let Some(icon) = self.named(named) {
                return Some(icon);
            }
        }
        if let Some(entry) = n.hints.desktop_entry.as_deref() {
            let apps = self.apps.get_or_insert_with(|| {
                AppDatabase::load(
                    &neue_desktop::apps::default_application_dirs(),
                    &Locale::from_env(),
                )
            });
            let icon = apps.by_id(entry).and_then(|e| e.icon.clone());
            if let Some(icon) = icon.and_then(|name| self.named(&name)) {
                return Some(icon);
            }
        }
        n.hints
            .icon_data
            .as_ref()
            .and_then(pixels)
            .map(Icon::raster)
    }

    /// An icon from a theme name, a path or a `file://` URI (PNG or JPEG).
    fn named(&mut self, text: &str) -> Option<Icon> {
        match IconSource::parse(text)? {
            IconSource::Name(name) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the panel's 40 px icon"
                )]
                let size = ICON_SIZE.round() as u32;
                self.loader.load(name, size, f64::from(ICON_SCALE))
            }
            source @ IconSource::File(_) => file(&source).map(Icon::raster),
            IconSource::Pixels(_) => None,
        }
    }
}

/// The picture of `image`'s pixels, if they are a valid picture.
fn pixels(image: &Image) -> Option<Picture> {
    Picture::from_rgba8(image.width, image.height, &image.rgba)
}

/// The picture in the file `source` names; `None` (logged) if it can't be read or isn't a
/// PNG or JPEG.
fn file(source: &IconSource<'_>) -> Option<Picture> {
    let path = source.path()?;
    picture::load(&path)
        .inspect_err(|error| tracing::debug!(%error, path = %path.display(), "cannot load a notification image"))
        .ok()
}

// ---------------------------------------------------------------------------------------
// The UI thread

/// The layer surface with the host in it.
struct Surface<M: 'static> {
    window: WindowId,
    host: Handle<NotificationHost<M>>,
    /// The window exists: panels can be added to the host.
    ready: bool,
}

/// The notifications as the UI thread keeps them.
pub struct Notifier<M: 'static> {
    wrap: fn(Event) -> M,
    /// To tell apps what happened; `None` until the worker has it, and after it stopped.
    server: Option<Arc<Notifications>>,
    /// Every open notification (shown or not), with its clock.
    store: Store<Notification>,
    surface: Option<Surface<M>>,
    /// Notifications on screen, oldest first.
    shown: Vec<u32>,
    /// Panels in the host (sliding out ones too), until `Gone`.
    in_host: BTreeSet<u32>,
    /// Waiting for the surface to exist.
    pending: Vec<(u32, NotificationContent)>,
    /// When the sleeping timer thread wakes up, if one is running.
    armed: Option<Instant>,
}

impl<M: Send + 'static> Notifier<M> {
    /// `wrap` turns an [`Event`] into the app's message.
    pub fn new(wrap: fn(Event) -> M) -> Self {
        Self {
            wrap,
            server: None,
            store: Store::new(CAPACITY),
            surface: None,
            shown: Vec::new(),
            in_host: BTreeSet::new(),
            pending: Vec::new(),
            armed: None,
        }
    }

    /// Starts the worker on a thread of its own.
    pub fn start(&self, cx: &mut Cx<'_, M>) {
        let wrap = self.wrap;
        cx.spawn(move |sender| {
            let result = run(|event| sender.send(wrap(event)));
            if let Err(err) = result {
                tracing::warn!(%err, "no notifications: the session bus is unavailable or another server owns the name");
            }
            wrap(Event::Ended)
        });
    }

    pub fn event(&mut self, cx: &mut Cx<'_, M>, event: Event) {
        match event {
            Event::Server(server) => self.server = Some(server),
            Event::Ended => self.server = None,
            Event::Notify(arrived) => self.notify(cx, *arrived),
            Event::Closed(id) => {
                self.store.remove(id);
                self.hide(cx, id);
            }
            Event::Opened => self.opened(cx),
            Event::Panel(id, event) => self.panel(cx, id, event),
            Event::Gone(id) => {
                self.in_host.remove(&id);
                self.settle(cx);
            }
            Event::Timer => self.expire(cx),
            Event::Token { id, action, token } => self.invoke(cx, id, &action, token.as_deref()),
        }
    }

    fn notify(&mut self, cx: &mut Cx<'_, M>, arrived: Arrived) {
        let Arrived {
            notification,
            content,
            config,
        } = arrived;
        let id = notification.id;
        let requested = match notification.timeout {
            Timeout::Default => None,
            Timeout::Never => Some(Lifetime::Never),
            Timeout::After(time) => Some(Lifetime::After(time)),
        };
        let critical = notification.hints.urgency == Urgency::Critical;
        let default = Duration::from_millis(config.banner_timeout.into());
        let life = lifetime(requested, critical, default);
        if let Inserted::Evicted(old, _) = self.store.insert(id, notification, life, Instant::now())
        {
            self.hide(cx, old);
            self.close(old, CloseReason::Undefined);
        }
        if config.do_not_disturb {
            // Counted, not shown (and taken off the screen if it was).
            self.hide(cx, id);
        } else {
            self.show(cx, id, content);
        }
        self.arm(cx);
    }

    // The screen --------------------------------------------------------------------------

    fn show(&mut self, cx: &mut Cx<'_, M>, id: u32, content: NotificationContent) {
        self.forget_lost_surface(cx);
        if !self.shown.contains(&id) {
            self.shown.push(id);
        }
        match &self.surface {
            Some(Surface {
                ready: true, host, ..
            }) => {
                host.show(cx, id, content);
                self.in_host.insert(id);
            }
            Some(_) => self.wait_for_surface(id, content),
            None => {
                self.open_surface(cx);
                self.wait_for_surface(id, content);
            }
        }
        while self.shown.len() > MAX_SHOWN {
            let oldest = self.shown.remove(0);
            self.take_off(cx, oldest);
        }
    }

    /// The compositor closes a layer surface whose output went away. The panels on it are
    /// gone with it (the notifications stay open); the next one opens a new surface, on the
    /// active output.
    fn forget_lost_surface(&mut self, cx: &Cx<'_, M>) {
        let lost = self
            .surface
            .as_ref()
            .is_some_and(|s| s.ready && !cx.windows().contains(&s.window));
        if lost {
            tracing::info!("the notification surface was closed by the compositor");
            self.surface = None;
            self.in_host.clear();
            self.shown.clear();
            self.pending.clear();
        }
    }

    fn wait_for_surface(&mut self, id: u32, content: NotificationContent) {
        match self.pending.iter_mut().find(|(p, _)| *p == id) {
            Some(slot) => slot.1 = content,
            None => self.pending.push((id, content)),
        }
    }

    /// Takes `id` off the screen if it is on it; it stays open.
    fn hide(&mut self, cx: &mut Cx<'_, M>, id: u32) {
        if let Some(i) = self.shown.iter().position(|&s| s == id) {
            self.shown.remove(i);
            self.take_off(cx, id);
        }
    }

    /// Slides `id`'s panel out (or forgets that it was waiting for the surface).
    fn take_off(&mut self, cx: &mut Cx<'_, M>, id: u32) {
        self.pending.retain(|(p, _)| *p != id);
        if self.in_host.contains(&id)
            && let Some(Surface {
                ready: true, host, ..
            }) = &self.surface
        {
            host.dismiss(cx, id);
        }
        self.settle(cx);
    }

    /// Opens the surface, with an empty host: the first panel is added once the window
    /// exists, so that it slides in.
    fn open_surface(&mut self, cx: &mut Cx<'_, M>) {
        let wrap = self.wrap;
        let mut host = Handle::default();
        let root = NotificationHost::new()
            .on_event(move |id, event| Some(wrap(Event::Panel(id, event))))
            .on_gone(move |id| Some(wrap(Event::Gone(id))))
            .bind(&mut host);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the host's fixed width in whole pixels"
        )]
        let width = (WIDTH + 2.0 * MARGIN).ceil() as u32;
        let layer = LayerSpec {
            layer: ShellLayer::Overlay,
            // A banner never takes the keyboard from the app being used.
            keyboard: LayerKeyboard::None,
            // The compositor puts it on the active output.
            output: None,
            ..LayerSpec::new(NAMESPACE, Anchor::TOP_RIGHT)
        };
        let window = neue_kit::Window::new("Notifications", (width, 1), root)
            .layer(layer)
            .fit_content();
        let window = cx.open_window(window);
        self.surface = Some(Surface {
            window,
            host,
            ready: false,
        });
        // Delivered after the driver has created the window.
        cx.spawn(move |_| wrap(Event::Opened));
    }

    fn opened(&mut self, cx: &mut Cx<'_, M>) {
        let Some(surface) = &mut self.surface else {
            return;
        };
        surface.ready = true;
        for (id, content) in std::mem::take(&mut self.pending) {
            surface.host.show(cx, id, content);
            self.in_host.insert(id);
        }
        self.settle(cx);
    }

    /// Closes the surface once nothing is on it or waiting for it.
    fn settle(&mut self, cx: &mut Cx<'_, M>) {
        if self.in_host.is_empty()
            && self.pending.is_empty()
            && self.surface.as_ref().is_some_and(|s| s.ready)
            && let Some(surface) = self.surface.take()
        {
            cx.close_window(surface.window);
        }
    }

    // The user ----------------------------------------------------------------------------

    fn panel(&mut self, cx: &mut Cx<'_, M>, id: u32, event: NotificationEvent) {
        let now = Instant::now();
        match event {
            NotificationEvent::Hovered(true) => {
                self.store.pause(id, now);
            }
            NotificationEvent::Hovered(false) => {
                self.store.resume(id, now);
                self.arm(cx);
            }
            NotificationEvent::Closed => self.dismiss(cx, id),
            NotificationEvent::Activated => {
                // A click on a notification without a default action only dismisses it.
                if self
                    .store
                    .get(id)
                    .is_some_and(Notification::has_default_action)
                {
                    self.ask_token(cx, id, DEFAULT_ACTION.to_owned());
                } else {
                    self.dismiss(cx, id);
                }
            }
            NotificationEvent::Action(i) => {
                let key = self
                    .store
                    .get(id)
                    .and_then(|n| n.buttons().nth(i))
                    .map(|(key, _)| key.clone());
                if let Some(key) = key {
                    self.ask_token(cx, id, key);
                }
            }
        }
    }

    /// Asks for an activation token for the app to raise its window with; the answer
    /// (with or without a token) invokes the action.
    fn ask_token(&self, cx: &mut Cx<'_, M>, id: u32, action: String) {
        let wrap = self.wrap;
        let Some(surface) = &self.surface else {
            return;
        };
        cx.activation_token(surface.window, move |token| {
            wrap(Event::Token { id, action, token })
        });
    }

    fn invoke(&mut self, cx: &mut Cx<'_, M>, id: u32, action: &str, token: Option<&str>) {
        // Closed by its app or expired while the token was on its way.
        let Some(resident) = self.store.get(id).map(|n| n.hints.resident) else {
            return;
        };
        if let Some(server) = &self.server
            && let Err(error) = server.invoke(id, action, token)
        {
            tracing::warn!(%error, id, action, "couldn't send ActionInvoked");
        }
        if !resident {
            self.dismiss(cx, id);
        }
    }

    /// The user closed `id` (or picked an action on it).
    fn dismiss(&mut self, cx: &mut Cx<'_, M>, id: u32) {
        self.store.remove(id);
        self.hide(cx, id);
        self.close(id, CloseReason::Dismissed);
    }

    fn close(&self, id: u32, reason: CloseReason) {
        if let Some(server) = &self.server
            && let Err(error) = server.close(id, reason)
        {
            tracing::warn!(%error, id, "couldn't send NotificationClosed");
        }
    }

    // The clocks --------------------------------------------------------------------------

    /// Makes sure a timer thread will wake the loop at the next deadline.
    fn arm(&mut self, cx: &mut Cx<'_, M>) {
        let Some(deadline) = self.store.next_deadline() else {
            return;
        };
        if self.armed.is_some_and(|armed| armed <= deadline) {
            return;
        }
        self.armed = Some(deadline);
        let wrap = self.wrap;
        cx.spawn(move |_| {
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            wrap(Event::Timer)
        });
    }

    fn expire(&mut self, cx: &mut Cx<'_, M>) {
        self.armed = None;
        for (id, _) in self.store.expire(Instant::now()) {
            self.hide(cx, id);
            self.close(id, CloseReason::Expired);
        }
        self.arm(cx);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // test code

    use std::collections::HashMap;
    use std::time::Duration;

    use neue_dbus::testing::pair;
    use neue_dbus::zbus::zvariant::Value;

    use super::*;

    fn note(summary: &str) -> Notification {
        Notification {
            id: 1,
            replaces: false,
            app_name: "Chat".to_owned(),
            app_icon: String::new(),
            summary: summary.to_owned(),
            body: String::new(),
            actions: Vec::new(),
            hints: neue_dbus::notifications::Hints::default(),
            timeout: Timeout::Default,
        }
    }

    #[test]
    fn the_panel_shows_plain_text_and_the_buttons_but_not_the_default_action() {
        let mut preparer = Preparer::new();
        let mut n = note("Mia");
        n.body = "<b>Lunch</b> &amp; coffee?<br/>12:30".to_owned();
        n.actions = vec![
            (DEFAULT_ACTION.to_owned(), "Open".to_owned()),
            ("reply".to_owned(), "Reply".to_owned()),
        ];
        let content = preparer.content(&n);
        assert_eq!(content.title, "Mia");
        assert_eq!(content.body, "Lunch & coffee? 12:30");
        assert_eq!(content.actions, ["Reply"]);
    }

    #[test]
    fn pixels_become_the_image_and_a_file_becomes_the_icon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("icon.png");
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255; 16])
                .unwrap();
        }
        std::fs::write(&path, &png).unwrap();

        let mut preparer = Preparer::new();
        let mut n = note("Photo");
        n.hints.image = Some(Image {
            width: 1,
            height: 1,
            rgba: vec![255, 0, 0, 255],
        });
        n.app_icon = format!("file://{}", path.display());
        let content = preparer.content(&n);
        assert!(content.image.is_some(), "image-data");
        assert!(content.app_icon.is_some(), "app_icon as a file URI");

        // A picture that doesn't add up, and a file that isn't there, show nothing.
        let mut n = note("Broken");
        n.hints.image = Some(Image {
            width: 3,
            height: 3,
            rgba: vec![0; 4],
        });
        n.app_icon = "/no/such/icon.png".to_owned();
        let content = preparer.content(&n);
        assert!(content.image.is_none());
        assert!(content.app_icon.is_none());

        // image-path as a file is the image; as a name it is looked up as an icon.
        let mut n = note("Path");
        n.hints.image_path = Some(path.display().to_string());
        assert!(preparer.content(&n).image.is_some());
    }

    #[test]
    fn the_worker_hands_over_notifications_and_closes_in_order() {
        let (service, client) = pair().unwrap();
        let (tx, rx) = mpsc::channel();
        let config_dir = tempfile::tempdir().unwrap();
        let layers = Layers::from_dirs([config_dir.path().to_owned()]);
        let worker = std::thread::spawn(move || {
            serve(&service, false, &layers, |event| tx.send(event).is_ok()).unwrap();
        });
        let Event::Server(server) = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!("the server comes first");
        };
        let notify = |summary: &str| -> u32 {
            let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
            hints.insert("urgency", Value::U8(2));
            client
                .call_method(
                    None::<()>,
                    neue_dbus::notifications::PATH,
                    Some(neue_dbus::notifications::INTERFACE),
                    "Notify",
                    &(
                        "Chat",
                        0_u32,
                        "",
                        summary,
                        "<i>hi</i>",
                        vec!["reply", "Reply"],
                        hints,
                        -1_i32,
                    ),
                )
                .unwrap()
                .body()
                .deserialize()
                .unwrap()
        };
        let (a, b) = (notify("first"), notify("second"));
        for (id, summary) in [(a, "first"), (b, "second")] {
            let Event::Notify(arrived) = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
                panic!("a notification");
            };
            assert_eq!(arrived.notification.id, id);
            assert_eq!(arrived.content.title, summary);
            assert_eq!(arrived.content.body, "hi");
            assert_eq!(arrived.content.actions, ["Reply"]);
            assert_eq!(arrived.notification.hints.urgency, Urgency::Critical);
            assert!(!arrived.config.do_not_disturb);
        }
        client
            .call_method(
                None::<()>,
                neue_dbus::notifications::PATH,
                Some(neue_dbus::notifications::INTERFACE),
                "CloseNotification",
                &a,
            )
            .unwrap();
        let Event::Closed(id) = rx.recv_timeout(Duration::from_secs(5)).unwrap() else {
            panic!("a close");
        };
        assert_eq!(id, a);
        // The UI thread letting go of the server ends the worker.
        drop(server);
        drop(rx);
        drop(client);
        worker.join().unwrap();
    }
}
