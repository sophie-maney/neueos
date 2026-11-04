//! neueOS's D-Bus interfaces, on zbus (PLAN.md §3.2, ADR 0005, ADR 0047).
//!
//! - [`menu`]: menus as `com.canonical.dbusmenu`. [`MenuExport`] serves one menu tree
//!   (neue-kit apps' menu bars, task 2.18); [`Layout`] is the wire form a client reads.
//!
//! The StatusNotifierItem, registrar, GMenuModel client and our private services arrive
//! with the shell (Phase 3).
//!
//! Connections are zbus's blocking ones with zbus's own executor thread (`async-io`), so
//! apps without an async runtime can use this crate; method calls are answered on that
//! thread, never on the caller's.

pub mod menu;
pub mod testing;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::fdo::ObjectManager;

/// The zbus this crate uses, for callers that need its types.
pub use zbus;

pub use menu::{Layout, MENU_ROOT, MenuExport, MenuNode, NodeKind, Toggle, ToggleState};

/// Connects to the session bus, ready for [`MenuExport`]s.
///
/// The connection serves an `org.freedesktop.DBus.ObjectManager` at [`MENU_ROOT`], which
/// lists the exported menus. Serving it from the builder also makes zbus start its
/// object server before this returns; started later (on the first export), it could drop
/// a method call that arrives in between.
pub fn connect_session() -> Result<Connection, Error> {
    Ok(with_object_server(Builder::session()?)?.build()?)
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
    #[error("an object is already exported at {0}")]
    PathTaken(String),
}
