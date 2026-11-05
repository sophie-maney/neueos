//! neueOS's D-Bus interfaces, on zbus (PLAN.md §3.2, ADR 0005, ADR 0047).
//!
//! - [`menu`]: menus as `com.canonical.dbusmenu`. [`MenuExport`] serves one menu tree
//!   (neue-kit apps' menu bars, task 2.18); [`Layout`] is the wire form a client reads.
//! - [`menu_client`]: the shell's side, reading an app's menus (task 3.7a).
//! - [`gmenu_client`]: the same for GTK's `org.gtk.Menus` and `org.gtk.Actions` (task 3.8).
//! - [`registrar`]: `com.canonical.AppMenu.Registrar`, owned by the shell (task 3.7a).
//! - [`login`]: the logind calls behind the logo menu.
//!
//! The StatusNotifierItem host and our private services arrive
//! with later Phase 3 tasks.
//!
//! Connections are zbus's blocking ones with zbus's own executor thread (`async-io`), so
//! apps without an async runtime can use this crate; method calls are answered on that
//! thread, never on the caller's.

pub mod gmenu_client;
pub mod login;
pub mod menu;
pub mod menu_client;
pub mod registrar;
pub mod testing;

use std::time::Duration;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::fdo::ObjectManager;

/// The zbus this crate uses, for callers that need its types.
pub use zbus;

pub use gmenu_client::{Activation, GMenuAddress, GMenuClient, GMenuSignals, GMenus, Scope};
pub use menu::{Layout, MENU_ROOT, MenuExport, MenuIcon, MenuNode, NodeKind, Toggle, ToggleState};
pub use menu_client::{MenuChanged, MenuClient, MenuSignals};
pub use registrar::Registrar;

/// Connects to the session bus, ready for [`MenuExport`]s.
///
/// The connection serves an `org.freedesktop.DBus.ObjectManager` at [`MENU_ROOT`], which
/// lists the exported menus. Serving it from the builder also makes zbus start its
/// object server before this returns; started later (on the first export), it could drop
/// a method call that arrives in between.
pub fn connect_session() -> Result<Connection, Error> {
    Ok(with_object_server(Builder::session()?)?.build()?)
}

/// Connects to the session bus as a client of other apps' objects (the shell): every
/// method call fails after `timeout` instead of waiting for a hung app forever.
pub fn connect_session_client(timeout: Duration) -> Result<Connection, Error> {
    Ok(Builder::session()?.method_timeout(timeout).build()?)
}

/// Adds the [`MENU_ROOT`] object manager to `builder` (see [`connect_session`]).
fn with_object_server(builder: Builder<'_>) -> zbus::Result<Builder<'_>> {
    builder.serve_at(MENU_ROOT, ObjectManager)
}

/// Errors from this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Dbus(#[from] zbus::Error),
    #[error("{0:?} is not a valid D-Bus object path")]
    Path(String),
    #[error("{0:?} is not a valid D-Bus bus name")]
    Name(String),
    #[error("an object is already exported at {0}")]
    PathTaken(String),
}
