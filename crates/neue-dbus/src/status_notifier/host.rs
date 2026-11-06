//! The host: follows the items on a watcher and keeps what each one shows (task 3.13a).
//!
//! A [`Host`] runs no threads. The caller (the shell's tray worker) subscribes to
//! [`super::WatcherSignals`] and [`super::ItemSignals`] *first*, then calls [`Host::initial`], then feeds
//! every signal to [`Host::registered`], [`Host::unregistered`] or [`Host::changed`] from
//! whatever thread it iterates them on. Each returns the [`HostEvent`] to show, or `None`
//! when nothing visible changed (an item that says `NewIcon` and still has the same icon).
//! The actions ([`Host::activate`] and the others) can be called from any clone of the host,
//! and block until the item answers.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;
use zbus::fdo::RequestNameFlags;
use zbus::names::BusName;

use super::item::{DEFAULT_PATH, ItemChanged, ItemInfo, Orientation, StatusItem};
use super::watcher::{NAME as WATCHER_NAME, PATH as WATCHER_PATH, split_address};
use crate::{Error, MenuClient};

const WATCHER_INTERFACE: &str = "org.kde.StatusNotifierWatcher";

/// What the tray has to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    /// A new item to show. `id` is its `bus/path`, which names it in the actions.
    Added { id: String, info: ItemInfo },
    /// An item shows something else now.
    Changed { id: String, info: ItemInfo },
    /// An item left.
    Removed { id: String },
}

struct Entry {
    id: String,
    service: String,
    path: String,
    /// The unique name behind `service`, which is what an item's signals come from;
    /// `None` when it couldn't be resolved (a peer-to-peer connection, or it already left).
    owner: Option<String>,
    /// What was last sent; `None` until the item could be read.
    shown: Option<ItemInfo>,
}

/// Follows a watcher's items; see the [module](self) for how it is driven.
#[derive(Clone)]
pub struct Host {
    conn: Connection,
    entries: Arc<Mutex<Vec<Entry>>>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

fn lock(entries: &Mutex<Vec<Entry>>) -> MutexGuard<'_, Vec<Entry>> {
    // Writers change whole entries; a poisoned list is still consistent.
    entries.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Host {
    /// A host on `conn`, with no items yet.
    pub fn new(conn: &Connection) -> Self {
        Self {
            conn: conn.clone(),
            entries: Arc::default(),
        }
    }

    /// Takes a host name on the bus and registers it with the watcher, so that items see
    /// `IsStatusNotifierHostRegistered` and show themselves instead of falling back to a
    /// window. Returns the name taken.
    pub fn announce(&self) -> Result<String, Error> {
        let name = format!("org.kde.StatusNotifierHost-{}-1", std::process::id());
        self.conn
            .request_name_with_flags(name.as_str(), RequestNameFlags::DoNotQueue.into())?;
        self.conn.call_method(
            Some(WATCHER_NAME),
            WATCHER_PATH,
            Some(WATCHER_INTERFACE),
            "RegisterStatusNotifierHost",
            &(name.as_str(),),
        )?;
        Ok(name)
    }

    /// The items already registered, as events. Subscribe to the signals before calling
    /// this, so that an item registering meanwhile is not missed (it is then reported
    /// twice, which [`Host::registered`] ignores).
    pub fn initial(&self) -> Result<Vec<HostEvent>, Error> {
        let proxy = zbus::blocking::fdo::PropertiesProxy::builder(&self.conn)
            .destination(WATCHER_NAME)?
            .path(WATCHER_PATH)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()?;
        let value = proxy
            .get(
                zbus::names::InterfaceName::from_static_str_unchecked(WATCHER_INTERFACE),
                "RegisteredStatusNotifierItems",
            )
            .map_err(zbus::Error::from)?;
        let addresses = Vec::<String>::try_from(value).map_err(zbus::Error::from)?;
        Ok(addresses
            .iter()
            .filter_map(|a| self.registered(a))
            .collect())
    }

    /// An item registered (`StatusNotifierItemRegistered`).
    pub fn registered(&self, address: &str) -> Option<HostEvent> {
        let (service, path) = match split_address(address) {
            Some((service, path)) => (service, path),
            None => (address, DEFAULT_PATH),
        };
        if lock(&self.entries).iter().any(|e| e.id == address) {
            return None;
        }
        let owner = self.owner_of(service);
        lock(&self.entries).push(Entry {
            id: address.to_owned(),
            service: service.to_owned(),
            path: path.to_owned(),
            owner,
            shown: None,
        });
        self.refresh(address)
    }

    /// An item left (`StatusNotifierItemUnregistered`).
    pub fn unregistered(&self, address: &str) -> Option<HostEvent> {
        let mut entries = lock(&self.entries);
        let at = entries.iter().position(|e| e.id == address)?;
        let entry = entries.remove(at);
        entry.shown.map(|_| HostEvent::Removed { id: entry.id })
    }

    /// An item said it changed (`NewIcon`, `NewToolTip`, `NewStatus`, ...).
    pub fn changed(&self, change: &ItemChanged) -> Option<HostEvent> {
        let id = lock(&self.entries)
            .iter()
            .find(|e| {
                e.path == change.path
                    && e.owner
                        .as_deref()
                        .is_none_or(|o| change.sender.is_empty() || o == change.sender)
            })
            .map(|e| e.id.clone())?;
        self.refresh(&id)
    }

    /// Reads item `id` and returns what changed since it was last shown.
    fn refresh(&self, id: &str) -> Option<HostEvent> {
        let item = self.item(id).ok()?;
        let info = match item.info() {
            Ok(info) => info,
            Err(err) => {
                // Registered before it exports its properties, or already gone: it is
                // shown at its first change signal, and dropped at its unregistration.
                tracing::debug!(%err, id, "couldn't read a status-notifier item");
                return None;
            }
        };
        let mut entries = lock(&self.entries);
        let entry = entries.iter_mut().find(|e| e.id == id)?;
        match &entry.shown {
            Some(shown) if *shown == info => None,
            Some(_) => {
                entry.shown = Some(info.clone());
                Some(HostEvent::Changed {
                    id: id.to_owned(),
                    info,
                })
            }
            None => {
                entry.shown = Some(info.clone());
                Some(HostEvent::Added {
                    id: id.to_owned(),
                    info,
                })
            }
        }
    }

    /// Primary action on item `id` (a left click at screen position `x`, `y`).
    pub fn activate(&self, id: &str, x: i32, y: i32) -> Result<(), Error> {
        self.item(id)?.activate(x, y)
    }

    /// Secondary action (a middle click).
    pub fn secondary_activate(&self, id: &str, x: i32, y: i32) -> Result<(), Error> {
        self.item(id)?.secondary_activate(x, y)
    }

    /// Asks the item to show its own context menu (for items without a DBusMenu).
    pub fn context_menu(&self, id: &str, x: i32, y: i32) -> Result<(), Error> {
        self.item(id)?.context_menu(x, y)
    }

    /// A scroll over the item.
    pub fn scroll(&self, id: &str, delta: i32, orientation: Orientation) -> Result<(), Error> {
        self.item(id)?.scroll(delta, orientation)
    }

    /// The item's DBusMenu, when it has one (as last read).
    pub fn menu(&self, id: &str) -> Result<Option<MenuClient>, Error> {
        let (service, path) = {
            let entries = lock(&self.entries);
            let entry = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| Error::UnknownItem(id.to_owned()))?;
            let Some(path) = entry.shown.as_ref().and_then(|i| i.menu.clone()) else {
                return Ok(None);
            };
            (entry.service.clone(), path)
        };
        MenuClient::new(&self.conn, &service, &path).map(Some)
    }

    fn item(&self, id: &str) -> Result<StatusItem, Error> {
        let (service, path) = {
            let entries = lock(&self.entries);
            let entry = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| Error::UnknownItem(id.to_owned()))?;
            (entry.service.clone(), entry.path.clone())
        };
        StatusItem::new(&self.conn, &service, &path)
    }

    fn owner_of(&self, service: &str) -> Option<String> {
        if service.starts_with(':') {
            return Some(service.to_owned());
        }
        let name = BusName::try_from(service).ok()?;
        let owner = DBusProxy::new(&self.conn).ok()?.get_name_owner(name).ok()?;
        Some(owner.to_string())
    }
}
