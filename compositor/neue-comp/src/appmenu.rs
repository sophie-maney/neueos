//! The Wayland side of the global menu (task 1.17, ADR 0005 and 0025).
//!
//! Apps tell us where they export their menus on the session bus:
//! * Qt, Chromium, LibreOffice and neue-kit through `org_kde_kwin_appmenu_manager` (v2):
//!   a `com.canonical.dbusmenu` service and object path;
//! * GTK `GtkApplication` apps through `gtk_shell1` (v5): the `org.gtk.Menus` and
//!   `org.gtk.Actions` object paths, in `gtk_surface1.set_dbus_properties`.
//!
//! The compositor does no D-Bus. It checks that the strings are valid bus names and object
//! paths, keeps one address per toplevel surface and shows it in [`WindowInfo::menu`]
//! (`neuectl windows`). The shell reads it from there (`neue-shell-v1`, Phase 3).
//! Every other `gtk_shell1` request is accepted and ignored.

use std::sync::Mutex;

use neue_ipc::MenuAddress;
use neue_protocols::gtk_shell::server::{
    gtk_shell1::{self, GtkShell1},
    gtk_surface1::{self, GtkSurface1},
};
use smithay::reexports::wayland_server::backend::{ClientId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak, backend::GlobalId,
};
use smithay::wayland::compositor::with_states;
use smithay::wayland::{Dispatch2, GlobalDispatch2};
use tracing::debug;
use wayland_protocols_plasma::appmenu::server::{
    org_kde_kwin_appmenu::{self, OrgKdeKwinAppmenu},
    org_kde_kwin_appmenu_manager::{self, OrgKdeKwinAppmenuManager},
};

use crate::state::State;

/// Version of `org_kde_kwin_appmenu_manager` we implement.
const APPMENU_VERSION: u32 = 2;
/// Version of `gtk_shell1` we implement. GTK's XML is at 7, but 6 asserts surface offsets
/// and 7 adds accessibility paths, which we don't do.
const GTK_SHELL_VERSION: u32 = 5;
/// What we tell GTK apps the shell does for them (`gtk_shell1.capabilities`): nothing yet.
/// GTK apps then keep their menu bar inside the window while still exporting the address.
/// Phase 3 turns on `global_app_menu | global_menu_bar` (values 1 and 2, whose bitwise or
/// is the value of `desktop_icons`: the enum is not a proper bit set) when the menu bar
/// shows them.
const GTK_SHELL_CAPABILITIES: u32 = 0;

/// The two globals this module serves.
#[derive(Debug)]
pub struct AppmenuState {
    _appmenu: GlobalId,
    _gtk_shell: GlobalId,
}

impl AppmenuState {
    pub fn new(dh: &DisplayHandle) -> Self {
        Self {
            _appmenu: dh.create_global::<State, OrgKdeKwinAppmenuManager, _>(
                APPMENU_VERSION,
                AppmenuManager,
            ),
            _gtk_shell: dh.create_global::<State, GtkShell1, _>(GTK_SHELL_VERSION, GtkShell),
        }
    }
}

/// Which protocol an address came from. A surface may use both; KDE's wins, as it is the
/// explicit one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Kde,
    Gtk,
}

/// An address and the protocol object that set it.
type Slot = Option<(ObjectId, MenuAddress)>;

/// The menu addresses of a surface, kept in its `data_map`.
#[derive(Debug, Default)]
struct SurfaceMenus(Mutex<[Slot; 2]>);

impl SurfaceMenus {
    fn index(source: Source) -> usize {
        source as usize
    }

    /// Replace `source`'s address. Returns whether the surface's visible address changed.
    fn set(&self, source: Source, owner: &ObjectId, address: Option<MenuAddress>) -> bool {
        let Ok(mut slots) = self.0.lock() else {
            return false;
        };
        let before = Self::visible(&slots);
        slots[Self::index(source)] = address.map(|a| (owner.clone(), a));
        Self::visible(&slots) != before
    }

    /// Drop `source`'s address if `owner` set it (an older, destroyed object must not wipe
    /// the address of a newer one). Returns whether the visible address changed.
    fn clear(&self, source: Source, owner: &ObjectId) -> bool {
        let Ok(mut slots) = self.0.lock() else {
            return false;
        };
        let slot = &mut slots[Self::index(source)];
        if slot.as_ref().is_none_or(|(id, _)| id != owner) {
            return false;
        }
        let before = Self::visible(&slots);
        slots[Self::index(source)] = None;
        Self::visible(&slots) != before
    }

    fn visible(slots: &[Slot; 2]) -> Option<MenuAddress> {
        slots
            .iter()
            .flatten()
            .map(|(_, address)| address.clone())
            .next()
    }

    fn get(&self) -> Option<MenuAddress> {
        let slots = self.0.lock().ok()?;
        Self::visible(&slots)
    }
}

/// The menu address announced for the toplevel `surface`, if any.
pub fn menu_address(surface: &WlSurface) -> Option<MenuAddress> {
    with_states(surface, |states| {
        states.data_map.get::<SurfaceMenus>()?.get()
    })
}

impl State {
    /// Run `f` on `surface`'s menu addresses and report the change to `neuectl` subscribers
    /// if the window's address changed.
    fn update_menus(&mut self, surface: &WlSurface, f: impl FnOnce(&SurfaceMenus) -> bool) {
        let changed = with_states(surface, |states| {
            f(states
                .data_map
                .get_or_insert_threadsafe(SurfaceMenus::default))
        });
        if changed && let Some(window) = self.window_for_surface(surface) {
            self.notify_window_changed(&window);
        }
    }
}

/// Whether `name` is a valid D-Bus bus name: unique (`:1.42`) or well known
/// (`org.kde.kate`).
fn valid_bus_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    let (unique, rest) = match name.strip_prefix(':') {
        Some(rest) => (true, rest),
        None => (false, name),
    };
    let mut elements = 0;
    for element in rest.split('.') {
        let mut chars = element.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        // Only unique names may start an element with a digit.
        if first.is_ascii_digit() && !unique {
            return false;
        }
        if !(first.is_ascii_alphanumeric() || first == '_' || first == '-')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return false;
        }
        elements += 1;
    }
    elements >= 2
}

/// Whether `path` is a valid D-Bus object path.
fn valid_object_path(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    path.strip_prefix('/').is_some_and(|rest| {
        rest.split('/')
            .all(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    })
}

/// `path` if it is a valid object path; `None` for no path and for an invalid one (logged).
fn optional_path(what: &str, path: Option<String>) -> Option<String> {
    let path = path.filter(|p| !p.is_empty())?;
    if valid_object_path(&path) {
        Some(path)
    } else {
        debug!(what, path, "ignoring an invalid object path");
        None
    }
}

/// User data of the `org_kde_kwin_appmenu_manager` global and of bound managers.
#[derive(Debug)]
pub struct AppmenuManager;

/// User data of an `org_kde_kwin_appmenu`: its surface (inert once the surface is gone).
#[derive(Debug)]
pub struct AppmenuObject(Weak<WlSurface>);

impl GlobalDispatch2<OrgKdeKwinAppmenuManager, State> for AppmenuManager {
    fn bind(
        &self,
        _state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<OrgKdeKwinAppmenuManager>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, AppmenuManager);
    }
}

impl Dispatch2<OrgKdeKwinAppmenuManager, State> for AppmenuManager {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _manager: &OrgKdeKwinAppmenuManager,
        request: org_kde_kwin_appmenu_manager::Request,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        if let org_kde_kwin_appmenu_manager::Request::Create { id, surface } = request {
            data_init.init(id, AppmenuObject(surface.downgrade()));
        }
    }
}

impl Dispatch2<OrgKdeKwinAppmenu, State> for AppmenuObject {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        resource: &OrgKdeKwinAppmenu,
        request: org_kde_kwin_appmenu::Request,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let org_kde_kwin_appmenu::Request::SetAddress {
            service_name,
            object_path,
        } = request
        else {
            return;
        };
        let Ok(surface) = self.0.upgrade() else {
            return;
        };
        let address = if valid_bus_name(&service_name) && valid_object_path(&object_path) {
            Some(MenuAddress::DbusMenu {
                service: service_name,
                path: object_path,
            })
        } else {
            debug!(
                service_name,
                object_path, "ignoring an invalid appmenu address"
            );
            None
        };
        let owner = resource.id();
        state.update_menus(&surface, |menus| menus.set(Source::Kde, &owner, address));
    }

    /// The object is gone (`release` or a dead client): its address goes with it.
    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &OrgKdeKwinAppmenu) {
        if let Ok(surface) = self.0.upgrade() {
            let owner = resource.id();
            state.update_menus(&surface, |menus| menus.clear(Source::Kde, &owner));
        }
    }
}

/// User data of the `gtk_shell1` global and of bound shells.
#[derive(Debug)]
pub struct GtkShell;

/// User data of a `gtk_surface1`: its surface (inert once the surface is gone).
#[derive(Debug)]
pub struct GtkSurface(Weak<WlSurface>);

impl GlobalDispatch2<GtkShell1, State> for GtkShell {
    fn bind(
        &self,
        _state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<GtkShell1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        let shell = data_init.init(resource, GtkShell);
        shell.capabilities(GTK_SHELL_CAPABILITIES);
    }
}

impl Dispatch2<GtkShell1, State> for GtkShell {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _shell: &GtkShell1,
        request: gtk_shell1::Request,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        if let gtk_shell1::Request::GetGtkSurface {
            gtk_surface,
            surface,
        } = request
        {
            data_init.init(gtk_surface, GtkSurface(surface.downgrade()));
        }
        // `set_startup_id`, `system_bell` and `notify_launch` are accepted and ignored.
    }
}

impl Dispatch2<GtkSurface1, State> for GtkSurface {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        resource: &GtkSurface1,
        request: gtk_surface1::Request,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let gtk_surface1::Request::SetDbusProperties {
            application_id: _,
            app_menu_path,
            menubar_path,
            window_object_path,
            application_object_path,
            unique_bus_name,
        } = request
        else {
            // Modal, present, focus and title-bar gestures: ignored (ADR 0025).
            return;
        };
        let Ok(surface) = self.0.upgrade() else {
            return;
        };
        // Without a bus name there is nothing to connect to. Paths the app left out, or
        // that are invalid, are absent; an app with none at all has no address.
        let address = unique_bus_name
            .filter(|name| valid_bus_name(name))
            .map(|bus_name| MenuAddress::Gmenu {
                bus_name,
                menubar_path: optional_path("menubar", menubar_path),
                app_menu_path: optional_path("app menu", app_menu_path),
                application_path: optional_path("application", application_object_path),
                window_path: optional_path("window", window_object_path),
            })
            .filter(|address| {
                !matches!(
                    address,
                    MenuAddress::Gmenu {
                        menubar_path: None,
                        app_menu_path: None,
                        application_path: None,
                        window_path: None,
                        ..
                    }
                )
            });
        let owner = resource.id();
        state.update_menus(&surface, |menus| menus.set(Source::Gtk, &owner, address));
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &GtkSurface1) {
        if let Ok(surface) = self.0.upgrade() {
            let owner = resource.id();
            state.update_menus(&surface, |menus| menus.clear(Source::Gtk, &owner));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_names() {
        for good in [
            ":1.42",
            ":1.0",
            "org.kde.kate",
            "org.gnome.TextEditor",
            "a.b_c-d",
        ] {
            assert!(valid_bus_name(good), "{good}");
        }
        for bad in [
            "",
            ":",
            "kate",
            ":1",
            "org..kate",
            ".org.kate",
            "org.kate.",
            "org.1kate",
            "org.ka te",
            "org.käte",
            "org/kate",
        ] {
            assert!(!valid_bus_name(bad), "{bad}");
        }
        assert!(!valid_bus_name(&format!("org.{}", "a".repeat(260))));
    }

    #[test]
    fn object_paths() {
        for good in ["/", "/MenuBar", "/org/gnome/Edit/menus/menubar", "/a_1/B2"] {
            assert!(valid_object_path(good), "{good}");
        }
        for bad in ["", "MenuBar", "//", "/a/", "/a//b", "/a-b", "/a b", "/ä"] {
            assert!(!valid_object_path(bad), "{bad}");
        }
    }
}
