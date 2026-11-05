//! The other side of [`crate::menu`]: reading an app's `com.canonical.dbusmenu` menus, as
//! the shell's menu bar does (task 3.7a, ADR 0005, ADR 0076).
//!
//! A [`MenuClient`] talks to one exported menu. [`MenuClient::fetch`] reads the whole tree
//! and first asks the app to fill its menus (`AboutToShow`), since some apps (LibreOffice)
//! leave them empty until then. [`MenuClient::event`] sends `clicked`, `opened` and
//! `closed`. [`MenuSignals`] reports, for every menu on the bus at once, that a layout or
//! an item's properties changed, so the caller fetches that menu again.

use std::time::{SystemTime, UNIX_EPOCH};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::names::BusName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

use crate::Error;
use crate::menu::{INTERFACE, Layout, MenuNode};

/// How many rounds of `AboutToShow` [`MenuClient::fetch`] sends for submenus that are
/// still empty: the menu-bar titles, then submenus inside them, and so on.
const FILL_ROUNDS: usize = 3;

/// At most this many submenus are asked to fill themselves per round, so an app with a
/// huge lazy tree can't keep the shell busy.
const FILL_LIMIT: usize = 64;

/// One app's exported menu.
#[derive(Debug, Clone)]
pub struct MenuClient {
    proxy: zbus::blocking::Proxy<'static>,
}

impl MenuClient {
    /// The menu `service` exports at `path` (an appmenu address). Nothing is sent yet.
    pub fn new(conn: &Connection, service: &str, path: &str) -> Result<Self, Error> {
        let service =
            BusName::try_from(service.to_owned()).map_err(|_| Error::Name(service.to_owned()))?;
        let path: OwnedObjectPath = ObjectPath::try_from(path.to_owned())
            .map_err(|_| Error::Path(path.to_owned()))?
            .into();
        let proxy = zbus::blocking::proxy::Builder::<zbus::blocking::Proxy<'static>>::new(conn)
            .destination(service)?
            .path(path)?
            .interface(INTERFACE)?
            // No property cache: it would subscribe to PropertiesChanged per menu.
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()?;
        Ok(Self { proxy })
    }

    /// The whole tree: the menu-bar titles with everything under them, hidden items left
    /// out. Empty submenus are asked to fill themselves first (`AboutToShow`, up to
    /// [`FILL_ROUNDS`] levels deep), and the tree is read again when one did.
    pub fn fetch(&self) -> Result<Vec<MenuNode>, Error> {
        let mut root = self.layout()?;
        let mut asked = std::collections::HashSet::new();
        for _ in 0..FILL_ROUNDS {
            let mut empty = Vec::new();
            empty_submenus(&root, true, &mut empty);
            empty.retain(|id| asked.insert(*id));
            empty.truncate(FILL_LIMIT);
            if empty.is_empty() {
                break;
            }
            if self.about_to_show_all(&empty) {
                root = self.layout()?;
            }
        }
        Ok(root
            .children
            .iter()
            .filter_map(MenuNode::from_layout)
            .collect())
    }

    /// `GetLayout(0, -1, [])`.
    fn layout(&self) -> Result<Layout, Error> {
        let (_revision, root): (u32, Layout) = self
            .proxy
            .call("GetLayout", &(0i32, -1i32, Vec::<&str>::new()))?;
        Ok(root)
    }

    /// Sends `AboutToShow` for `ids` (as one `AboutToShowGroup` when the app has it);
    /// true if any of them changed or might have. Failures are logged, not returned: a
    /// menu that can't be filled is shown as it is.
    fn about_to_show_all(&self, ids: &[i32]) -> bool {
        match self
            .proxy
            .call::<_, _, (Vec<i32>, Vec<i32>)>("AboutToShowGroup", &(ids,))
        {
            Ok((updates, _errors)) => !updates.is_empty(),
            // Old exporters only have AboutToShow.
            Err(_) => {
                let mut changed = false;
                for &id in ids {
                    changed |= self.about_to_show(id).unwrap_or_else(|err| {
                        tracing::debug!(%err, id, "AboutToShow failed");
                        false
                    });
                }
                changed
            }
        }
    }

    /// The menu under item `id` is about to open; true if the app says the tree must be
    /// read again.
    pub fn about_to_show(&self, id: i32) -> Result<bool, Error> {
        Ok(self.proxy.call("AboutToShow", &(id,))?)
    }

    /// Sends `event` (`clicked`, `opened`, `closed`, `hovered`) for item `id`.
    pub fn event(&self, id: i32, event: &str) -> Result<(), Error> {
        self.proxy
            .call::<_, _, ()>("Event", &(id, event, Value::from(0i32), timestamp()))?;
        Ok(())
    }

    /// The exporting connection's name, as given.
    pub fn service(&self) -> &str {
        self.proxy.inner().destination().as_str()
    }

    /// The menu's object path.
    pub fn path(&self) -> &str {
        self.proxy.inner().path().as_str()
    }
}

/// Ids of submenus whose children aren't there: items saying `children-display` is
/// `submenu` with none, and (`top`) every menu-bar title without children.
fn empty_submenus(layout: &Layout, top: bool, out: &mut Vec<i32>) {
    for child in &layout.children {
        let says_submenu = child
            .properties
            .get("children-display")
            .and_then(|v| <&str>::try_from(&**v).ok())
            == Some("submenu");
        if child.children.is_empty() {
            if top || says_submenu {
                out.push(child.id);
            }
        } else {
            empty_submenus(child, false, out);
        }
    }
}

/// Seconds-resolution X-style timestamp for events (apps only compare them).
fn timestamp() -> u32 {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    // Wraps like an X server time.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a wrapping millisecond clock"
    )]
    let ms = ms as u32;
    ms
}

/// A menu changed: its layout (`LayoutUpdated`) or some items' properties
/// (`ItemsPropertiesUpdated`). Read it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuChanged {
    /// The sender's unique name (`:1.42`); empty on a peer-to-peer connection.
    pub sender: String,
    pub path: String,
}

/// Every DBusMenu change signal on `conn`, from any app. Blocking: iterate it on a
/// thread of its own. It ends when the connection closes.
pub struct MenuSignals {
    messages: MessageIterator,
}

impl std::fmt::Debug for MenuSignals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuSignals").finish_non_exhaustive()
    }
}

impl MenuSignals {
    /// Subscribes to `com.canonical.dbusmenu` signals (adds a match rule on a bus).
    pub fn new(conn: &Connection) -> Result<Self, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(INTERFACE)?
            .build();
        // Signals are coalesced by the caller; a burst beyond this drops the oldest.
        let messages = MessageIterator::for_match_rule(rule, conn, Some(64))?;
        Ok(Self { messages })
    }
}

impl Iterator for MenuSignals {
    type Item = MenuChanged;

    fn next(&mut self) -> Option<MenuChanged> {
        loop {
            let message = match self.messages.next()? {
                Ok(message) => message,
                Err(err) => {
                    tracing::debug!(%err, "bad message on the dbusmenu signal stream");
                    continue;
                }
            };
            let header = message.header();
            let member = header.member().map(|m| m.as_str());
            if !matches!(member, Some("LayoutUpdated" | "ItemsPropertiesUpdated")) {
                continue;
            }
            let Some(path) = header.path() else {
                continue;
            };
            return Some(MenuChanged {
                sender: header.sender().map(ToString::to_string).unwrap_or_default(),
                path: path.to_string(),
            });
        }
    }
}
