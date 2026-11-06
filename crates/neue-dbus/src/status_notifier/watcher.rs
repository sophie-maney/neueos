//! `org.kde.StatusNotifierWatcher`: where status-notifier items and hosts register (task
//! 3.13a). The shell owns it, as it owns the appmenu registrar.
//!
//! Items call `RegisterStatusNotifierItem` with their bus name, with a bus name and path
//! (`:1.5/org/ayatana/NotificationItem/foo`), or with just a path (libappindicator, which
//! means the caller's own connection). The watcher keeps each as `bus/path` (the form the
//! signals carry too) and forgets it when its connection leaves the bus.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::fdo::{self, RequestNameFlags};
use zbus::message::{Header, Type as MessageType};
use zbus::names::BusName;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::ObjectPath;

use super::item::DEFAULT_PATH;
use crate::Error;

/// The well-known name.
pub const NAME: &str = "org.kde.StatusNotifierWatcher";
/// The object path.
pub const PATH: &str = "/StatusNotifierWatcher";
/// The interface.
pub const INTERFACE: &str = "org.kde.StatusNotifierWatcher";

/// A client can't make the watcher hold more than this many items or hosts.
const LIMIT: usize = 256;

#[derive(Default)]
struct State {
    /// `bus/path`, in registration order.
    items: Vec<String>,
    /// Host names, in registration order.
    hosts: Vec<String>,
}

type Shared = Arc<Mutex<State>>;

fn state(s: &Shared) -> MutexGuard<'_, State> {
    // Writers push or remove whole entries; a poisoned state is still consistent.
    s.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Splits `bus/path` at the first `/`; `None` without a path.
pub fn split_address(address: &str) -> Option<(&str, &str)> {
    let at = address.find('/')?;
    (at > 0).then(|| address.split_at(at))
}

/// The `bus/path` form of what an item registered, given the caller's unique name.
fn normalize(service: &str, sender: Option<&str>) -> Result<String, fdo::Error> {
    let (bus, path) = if service.starts_with('/') {
        let sender =
            sender.ok_or_else(|| fdo::Error::InvalidArgs("no sender to take as the bus".into()))?;
        (sender, service)
    } else if let Some(split) = split_address(service) {
        split
    } else {
        (service, DEFAULT_PATH)
    };
    if BusName::try_from(bus).is_err() {
        return Err(fdo::Error::InvalidArgs(format!(
            "{bus:?} is not a bus name"
        )));
    }
    if ObjectPath::try_from(path).is_err() {
        return Err(fdo::Error::InvalidArgs(format!(
            "{path:?} is not an object path"
        )));
    }
    Ok(format!("{bus}{path}"))
}

struct Server {
    state: Shared,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Server {
    async fn register_status_notifier_item(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        service: &str,
    ) -> fdo::Result<()> {
        let sender = header.sender().map(ToString::to_string);
        let address = normalize(service, sender.as_deref())?;
        {
            let mut s = state(&self.state);
            if s.items.contains(&address) {
                return Ok(());
            }
            if s.items.len() >= LIMIT {
                return Err(fdo::Error::LimitsExceeded("too many items".into()));
            }
            s.items.push(address.clone());
        }
        Self::status_notifier_item_registered(&emitter, &address).await?;
        self.registered_status_notifier_items_changed(&emitter)
            .await?;
        Ok(())
    }

    async fn register_status_notifier_host(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        service: &str,
    ) -> fdo::Result<()> {
        if BusName::try_from(service).is_err() {
            return Err(fdo::Error::InvalidArgs(format!(
                "{service:?} is not a bus name"
            )));
        }
        let first = {
            let mut s = state(&self.state);
            if s.hosts.iter().any(|h| h == service) {
                return Ok(());
            }
            if s.hosts.len() >= LIMIT {
                return Err(fdo::Error::LimitsExceeded("too many hosts".into()));
            }
            s.hosts.push(service.to_owned());
            s.hosts.len() == 1
        };
        Self::status_notifier_host_registered(&emitter).await?;
        if first {
            self.is_status_notifier_host_registered_changed(&emitter)
                .await?;
        }
        Ok(())
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        state(&self.state).items.clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        !state(&self.state).hosts.is_empty()
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// The watcher object on a connection. Dropping it removes the object; the name, if
/// owned, goes when the connection closes.
#[derive(Debug)]
pub struct Watcher {
    conn: Connection,
    state: Shared,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("items", &self.items)
            .field("hosts", &self.hosts)
            .finish()
    }
}

impl Watcher {
    /// Serves the watcher at [`PATH`] on `conn`, without asking for the name and without
    /// following connections leaving (for peer-to-peer tests). See [`Watcher::own`].
    pub fn serve(conn: &Connection) -> Result<Self, Error> {
        let state = Shared::default();
        let server = Server {
            state: Arc::clone(&state),
        };
        if !conn.object_server().at(PATH, server)? {
            return Err(Error::PathTaken(PATH.to_owned()));
        }
        Ok(Self {
            conn: conn.clone(),
            state,
        })
    }

    /// Serves the watcher, takes [`NAME`] (replacing another owner that allows it, and
    /// letting a later one replace us) and forgets items and hosts whose connection leaves
    /// the bus.
    pub fn own(conn: &Connection) -> Result<Self, Error> {
        let watcher = Self::serve(conn)?;
        // Subscribe before taking the name, so no departure is missed.
        let names = zbus::blocking::fdo::DBusProxy::new(conn)?.receive_name_owner_changed()?;
        conn.request_name_with_flags(
            NAME,
            RequestNameFlags::ReplaceExisting | RequestNameFlags::AllowReplacement,
        )?;
        let weak = Arc::downgrade(&watcher.state);
        let conn = conn.clone();
        std::thread::Builder::new()
            .name("sni-watcher-names".into())
            .spawn(move || follow_departures(&conn, &weak, names))
            .map_err(zbus::Error::from)?;
        Ok(watcher)
    }

    /// The registered items, as `bus/path`.
    pub fn items(&self) -> Vec<String> {
        state(&self.state).items.clone()
    }

    /// Whether a host has registered.
    pub fn has_host(&self) -> bool {
        !state(&self.state).hosts.is_empty()
    }

    /// Forgets every item and host of `bus_name` (it left the bus), and says so.
    pub fn forget(&self, bus_name: &str) -> Result<(), Error> {
        forget(&self.conn, &self.state, bus_name)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Err(err) = self.conn.object_server().remove::<Server, _>(PATH) {
            tracing::debug!(%err, "couldn't remove the status-notifier watcher object");
        }
    }
}

fn forget(conn: &Connection, shared: &Shared, bus_name: &str) -> Result<(), Error> {
    let (gone_items, hosts_before, hosts_after) = {
        let mut s = state(shared);
        let before = s.hosts.len();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut s.items)
            .into_iter()
            .partition(|a| split_address(a).is_some_and(|(bus, _)| bus == bus_name));
        s.items = kept;
        s.hosts.retain(|h| h != bus_name);
        (gone, before, s.hosts.len())
    };
    if gone_items.is_empty() && hosts_before == hosts_after {
        return Ok(());
    }
    let emitter = SignalEmitter::new(conn.inner(), PATH)?;
    for address in &gone_items {
        zbus::block_on(Server::status_notifier_item_unregistered(&emitter, address))?;
    }
    let object = conn.object_server().interface::<_, Server>(PATH)?;
    let server = object.get();
    if !gone_items.is_empty() {
        zbus::block_on(server.registered_status_notifier_items_changed(&emitter))?;
    }
    if hosts_after < hosts_before {
        zbus::block_on(Server::status_notifier_host_unregistered(&emitter))?;
        if hosts_after == 0 {
            zbus::block_on(server.is_status_notifier_host_registered_changed(&emitter))?;
        }
    }
    Ok(())
}

/// Calls [`forget`] for every connection that leaves the bus, until the watcher is gone.
fn follow_departures(
    conn: &Connection,
    shared: &Weak<Mutex<State>>,
    names: zbus::blocking::fdo::NameOwnerChangedIterator,
) {
    for change in names {
        let Some(shared) = shared.upgrade() else {
            return;
        };
        let Ok(args) = change.args() else { continue };
        if args.new_owner().is_some() {
            continue;
        }
        if let Err(err) = forget(conn, &shared, args.name().as_str()) {
            tracing::debug!(%err, "couldn't announce a status-notifier departure");
        }
    }
}

/// Changes on the watcher: what a host follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    /// An item registered, as `bus/path`.
    ItemRegistered(String),
    /// An item left, as `bus/path`.
    ItemUnregistered(String),
}

/// The watcher's item signals, from any watcher on `conn`. Blocking: iterate it on a
/// thread of its own. It ends when the connection closes.
pub struct WatcherSignals {
    messages: MessageIterator,
}

impl std::fmt::Debug for WatcherSignals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatcherSignals").finish_non_exhaustive()
    }
}

impl WatcherSignals {
    /// Subscribes to `org.kde.StatusNotifierWatcher` signals (adds a match rule on a bus).
    pub fn new(conn: &Connection) -> Result<Self, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(INTERFACE)?
            .path(PATH)?
            .build();
        let messages = MessageIterator::for_match_rule(rule, conn, Some(64))?;
        Ok(Self { messages })
    }
}

impl Iterator for WatcherSignals {
    type Item = WatcherEvent;

    fn next(&mut self) -> Option<WatcherEvent> {
        loop {
            let message = match self.messages.next()? {
                Ok(message) => message,
                Err(err) => {
                    tracing::debug!(%err, "bad message on the watcher signal stream");
                    continue;
                }
            };
            let member = message.header().member().map(|m| m.as_str().to_owned());
            let make: fn(String) -> WatcherEvent = match member.as_deref() {
                Some("StatusNotifierItemRegistered") => WatcherEvent::ItemRegistered,
                Some("StatusNotifierItemUnregistered") => WatcherEvent::ItemUnregistered,
                _ => continue,
            };
            match message.body().deserialize::<String>() {
                Ok(address) => return Some(make(address)),
                Err(err) => tracing::debug!(%err, "bad watcher signal body"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_take_three_forms() {
        let n = |s, sender| normalize(s, sender).unwrap_or_else(|e| format!("error: {e}"));
        assert_eq!(n("org.kde.Foo", None), "org.kde.Foo/StatusNotifierItem");
        assert_eq!(
            n(":1.5/org/ayatana/Item/foo", None),
            ":1.5/org/ayatana/Item/foo"
        );
        assert_eq!(
            n("/org/ayatana/Item/foo", Some(":1.9")),
            ":1.9/org/ayatana/Item/foo"
        );
    }

    #[test]
    fn bad_registrations_are_refused() {
        assert!(normalize("/path/only", None).is_err(), "no sender");
        assert!(normalize("not a name", None).is_err());
        assert!(normalize(":1.5/bad//path", None).is_err());
        assert!(normalize("", Some(":1.1")).is_err());
    }

    #[test]
    fn addresses_split_at_the_first_slash() {
        assert_eq!(split_address(":1.5/a/b"), Some((":1.5", "/a/b")));
        assert_eq!(split_address(":1.5"), None);
        assert_eq!(split_address("/a"), None);
    }
}
