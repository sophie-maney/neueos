//! The StatusNotifierItem tray protocol (task 3.13a, ADR 0084): the status icons apps put
//! in the menu bar's right side (`nm-applet --indicator`, Discord, Steam, Syncthing).
//!
//! - [`watcher`]: `org.kde.StatusNotifierWatcher`, served by the shell.
//! - [`item`]: reading one item ([`ItemInfo`]) and acting on it ([`StatusItem`]).
//! - [`host`]: [`Host`] follows the watcher's items and says what to show.
//!
//! Menus are the items' DBusMenu, read with [`crate::MenuClient`] ([`Host::menu`]).

pub mod host;
pub mod item;
pub mod watcher;

pub use host::{Host, HostEvent};
pub use item::{
    Category, ItemChanged, ItemIcon, ItemInfo, ItemSignals, Orientation, Pixmap, Status,
    StatusItem, ToolTip, best_pixmap,
};
pub use watcher::{Watcher, WatcherEvent, WatcherSignals};
