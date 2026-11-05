//! `com.canonical.AppMenu.Registrar`, the name whose presence makes Qt, Chromium and
//! LibreOffice export their menus (ADR 0005, task 3.7a).
//!
//! On Wayland those apps announce the menu's address through the KDE appmenu protocol
//! (the compositor passes it on to the shell), so the registrar's job is mostly to exist.
//! It still implements the interface: X11-style window ids registered with
//! `RegisterWindow` are kept, with the caller's bus name, until they're unregistered.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use zbus::blocking::Connection;
use zbus::fdo;
use zbus::fdo::RequestNameFlags;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use crate::Error;

/// The well-known name.
pub const NAME: &str = "com.canonical.AppMenu.Registrar";
/// The object path.
pub const PATH: &str = "/com/canonical/AppMenu/Registrar";

/// A registered window: the exporting connection and the menu's path.
type Entry = (String, OwnedObjectPath);

type Windows = Arc<Mutex<BTreeMap<u32, Entry>>>;

struct Server {
    windows: Windows,
}

fn windows(w: &Windows) -> std::sync::MutexGuard<'_, BTreeMap<u32, Entry>> {
    // Writers insert or remove whole entries; a poisoned map is still consistent.
    w.lock().unwrap_or_else(PoisonError::into_inner)
}

#[zbus::interface(name = "com.canonical.AppMenu.Registrar")]
impl Server {
    /// The caller exports the menu of X11 window `window_id` at `menu_object_path`.
    async fn register_window(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        window_id: u32,
        menu_object_path: ObjectPath<'_>,
    ) -> fdo::Result<()> {
        let service = header.sender().map(ToString::to_string).unwrap_or_default();
        let path = OwnedObjectPath::from(menu_object_path);
        windows(&self.windows).insert(window_id, (service.clone(), path.clone()));
        Self::window_registered(&emitter, window_id, &service, path.as_ref()).await?;
        Ok(())
    }

    async fn unregister_window(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        window_id: u32,
    ) -> fdo::Result<()> {
        if windows(&self.windows).remove(&window_id).is_some() {
            Self::window_unregistered(&emitter, window_id).await?;
        }
        Ok(())
    }

    /// The service and path registered for `window_id`.
    #[zbus(out_args("service", "menuObjectPath"))]
    fn get_menu_for_window(&self, window_id: u32) -> fdo::Result<(String, OwnedObjectPath)> {
        windows(&self.windows)
            .get(&window_id)
            .cloned()
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no menu for window {window_id}")))
    }

    /// Every registration: (window id, service, path).
    #[zbus(out_args("menus"))]
    fn get_menus(&self) -> Vec<(u32, String, OwnedObjectPath)> {
        windows(&self.windows)
            .iter()
            .map(|(id, (service, path))| (*id, service.clone(), path.clone()))
            .collect()
    }

    #[zbus(signal)]
    async fn window_registered(
        emitter: &SignalEmitter<'_>,
        window_id: u32,
        service: &str,
        path: ObjectPath<'_>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn window_unregistered(emitter: &SignalEmitter<'_>, window_id: u32) -> zbus::Result<()>;
}

/// The registrar object on a connection. Dropping it removes the object; the name, if
/// owned, goes when the connection closes.
#[derive(Debug)]
pub struct Registrar {
    conn: Connection,
    windows: Windows,
}

impl Registrar {
    /// Serves the registrar at [`PATH`] on `conn`, without asking for the name (for
    /// peer-to-peer tests). See [`Registrar::own`].
    pub fn serve(conn: &Connection) -> Result<Self, Error> {
        let windows = Windows::default();
        let server = Server {
            windows: Arc::clone(&windows),
        };
        if !conn.object_server().at(PATH, server)? {
            return Err(Error::PathTaken(PATH.to_owned()));
        }
        Ok(Self {
            conn: conn.clone(),
            windows,
        })
    }

    /// Serves the registrar and takes [`NAME`] on the bus, replacing another owner that
    /// allows it and letting a later one replace us (a restarted shell takes it back).
    pub fn own(conn: &Connection) -> Result<Self, Error> {
        let registrar = Self::serve(conn)?;
        conn.request_name_with_flags(
            NAME,
            RequestNameFlags::ReplaceExisting | RequestNameFlags::AllowReplacement,
        )?;
        Ok(registrar)
    }

    /// How many windows are registered.
    pub fn len(&self) -> usize {
        windows(&self.windows).len()
    }

    /// Whether no window is registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for Registrar {
    fn drop(&mut self) {
        if let Err(err) = self.conn.object_server().remove::<Server, _>(PATH) {
            tracing::debug!(%err, "couldn't remove the registrar object");
        }
    }
}
