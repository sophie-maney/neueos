//! The global-menu protocol side (task 1.17, ADR 0025): `org_kde_kwin_appmenu` and
//! `gtk_shell1` addresses show up in `neuectl windows` and in `window_changed` events.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::Duration;

use harness::{Server, TestClient, Toplevel};
use neue_ipc::{Event, MenuAddress, Subscription, Topic, WindowInfo};

const APP: &str = "org.neueos.Test";

fn toplevel(client: &mut TestClient) -> Toplevel {
    client.create_toplevel("Menus", APP, (300, 200))
}

/// The menu of the only window, as `neuectl windows` reports it.
fn menu(server: &Server) -> Option<MenuAddress> {
    let windows: Vec<WindowInfo> = server.ipc().windows().unwrap();
    assert_eq!(windows.len(), 1);
    windows.into_iter().next().unwrap().menu
}

fn dbus(service: &str, path: &str) -> MenuAddress {
    MenuAddress::DbusMenu {
        service: service.into(),
        path: path.into(),
    }
}

fn set_gtk(
    gtk: &neue_protocols::gtk_shell::client::gtk_surface1::GtkSurface1,
    bus_name: Option<&str>,
    menubar: Option<&str>,
) {
    gtk.set_dbus_properties(
        Some(APP.into()),
        None,
        menubar.map(Into::into),
        Some("/org/neueos/Test/window/1".into()),
        Some("/org/neueos/Test".into()),
        bus_name.map(Into::into),
    );
}

fn subscribe(server: &Server) -> Subscription {
    let events = server.ipc().subscribe(vec![Topic::Windows]).unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    events
}

#[test]
fn globals_are_advertised_at_the_versions_we_implement() {
    let server = Server::start();
    let mut client = server.connect();
    assert!(client.has_global("org_kde_kwin_appmenu_manager"));
    assert!(client.has_global("gtk_shell1"));
    assert_eq!(client.gtk_shell_version(), 5);
    // The shell shows app menu and menu bar (bits 0 and 1), so GTK hides its own.
    assert_eq!(client.gtk_shell_capabilities(), Some(0b11));
}

#[test]
fn kde_address_shows_in_windows_and_goes_with_the_object() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    assert_eq!(menu(&server), None);

    let appmenu = client.kde_appmenu(&window.surface);
    appmenu.set_address(":1.42".into(), "/MenuBar".into());
    client.roundtrip();
    assert_eq!(menu(&server), Some(dbus(":1.42", "/MenuBar")));

    // The address can move (Qt re-registers) ...
    appmenu.set_address("org.kde.kate".into(), "/MenuBar/2".into());
    client.roundtrip();
    assert_eq!(menu(&server), Some(dbus("org.kde.kate", "/MenuBar/2")));

    // ... and release removes it.
    appmenu.release();
    client.roundtrip();
    assert_eq!(menu(&server), None);
}

#[test]
fn invalid_kde_addresses_are_dropped() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let appmenu = client.kde_appmenu(&window.surface);

    appmenu.set_address(":1.42".into(), "/MenuBar".into());
    client.roundtrip();
    assert!(menu(&server).is_some());
    // A bad address replaces the old one with nothing rather than keeping a stale one.
    for (service, path) in [
        ("not a name", "/MenuBar"),
        ("kate", "/MenuBar"),
        (":1.42", "MenuBar"),
        (":1.42", "/Menu-Bar/"),
        ("", ""),
    ] {
        appmenu.set_address(":1.42".into(), "/MenuBar".into());
        appmenu.set_address(service.into(), path.into());
        client.roundtrip();
        assert_eq!(menu(&server), None, "{service:?} {path:?}");
    }
}

#[test]
fn an_older_object_does_not_wipe_a_newer_address() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let first = client.kde_appmenu(&window.surface);
    let second = client.kde_appmenu(&window.surface);
    first.set_address(":1.1".into(), "/A".into());
    second.set_address(":1.2".into(), "/B".into());
    client.roundtrip();
    assert_eq!(menu(&server), Some(dbus(":1.2", "/B")));
    first.release();
    client.roundtrip();
    assert_eq!(menu(&server), Some(dbus(":1.2", "/B")));
    second.release();
    client.roundtrip();
    assert_eq!(menu(&server), None);
}

#[test]
fn gtk_properties_show_in_windows() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let gtk = client.gtk_surface(&window.surface);

    set_gtk(&gtk, Some(":1.7"), Some("/org/neueos/Test/menus/menubar"));
    client.roundtrip();
    assert_eq!(
        menu(&server),
        Some(MenuAddress::Gmenu {
            bus_name: ":1.7".into(),
            menubar_path: Some("/org/neueos/Test/menus/menubar".into()),
            app_menu_path: None,
            application_path: Some("/org/neueos/Test".into()),
            window_path: Some("/org/neueos/Test/window/1".into()),
        })
    );

    gtk.release();
    client.roundtrip();
    assert_eq!(menu(&server), None);
}

#[test]
fn gtk_without_a_usable_bus_name_or_paths_has_no_address() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let gtk = client.gtk_surface(&window.surface);

    set_gtk(&gtk, None, Some("/menubar"));
    client.roundtrip();
    assert_eq!(menu(&server), None);

    set_gtk(&gtk, Some("bad name"), Some("/menubar"));
    client.roundtrip();
    assert_eq!(menu(&server), None);

    // An invalid path is dropped and the rest kept.
    set_gtk(&gtk, Some(":1.7"), Some("menubar"));
    client.roundtrip();
    let Some(MenuAddress::Gmenu { menubar_path, .. }) = menu(&server) else {
        panic!("expected a gmenu address");
    };
    assert_eq!(menubar_path, None);

    // No paths at all: nothing to show.
    gtk.set_dbus_properties(None, None, None, None, None, Some(":1.7".into()));
    client.roundtrip();
    assert_eq!(menu(&server), None);
}

#[test]
fn other_gtk_shell_requests_are_accepted() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let gtk = client.gtk_surface(&window.surface);
    let shell = client.gtk_shell();

    shell.set_startup_id(Some("id".into()));
    shell.set_startup_id(None);
    shell.system_bell(Some(&gtk));
    shell.system_bell(None);
    shell.notify_launch("id".into());
    gtk.set_modal();
    gtk.unset_modal();
    gtk.present(0);
    gtk.request_focus(None);
    // A protocol error would have disconnected us.
    client.roundtrip();
    assert_eq!(menu(&server), None);
}

#[test]
fn kde_wins_over_gtk_and_gtk_is_the_fallback() {
    let server = Server::start();
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let gtk = client.gtk_surface(&window.surface);
    let appmenu = client.kde_appmenu(&window.surface);

    set_gtk(&gtk, Some(":1.7"), Some("/menubar"));
    appmenu.set_address(":1.42".into(), "/MenuBar".into());
    client.roundtrip();
    assert_eq!(menu(&server), Some(dbus(":1.42", "/MenuBar")));

    appmenu.release();
    client.roundtrip();
    assert!(matches!(menu(&server), Some(MenuAddress::Gmenu { .. })));
}

#[test]
fn address_changes_are_announced_once_each() {
    let server = Server::start();
    let mut events = subscribe(&server);
    let mut client = server.connect();
    let window = toplevel(&mut client);
    let appmenu = client.kde_appmenu(&window.surface);

    appmenu.set_address(":1.42".into(), "/MenuBar".into());
    // The same address again is not a change.
    appmenu.set_address(":1.42".into(), "/MenuBar".into());
    appmenu.release();
    client.roundtrip();

    // Closing the window ends the stream of changes we care about: nothing else may sit
    // between the two changes and the close.
    client.destroy_toplevel(window);
    let mut seen = Vec::new();
    loop {
        match events.next_event().unwrap().expect("event stream ended") {
            Event::WindowChanged(info) => seen.push(info.menu),
            Event::WindowClosed { .. } => break,
            _ => {}
        }
    }
    assert_eq!(seen, vec![Some(dbus(":1.42", "/MenuBar")), None]);
}
