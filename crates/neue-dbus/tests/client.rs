//! The shell's DBusMenu client and the registrar against our own export (task 3.7a's
//! acceptance), over peer-to-peer connections so no bus daemon is needed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use neue_dbus::registrar::{self, Registrar};
use neue_dbus::testing::pair;
use neue_dbus::{MenuClient, MenuExport, MenuIcon, MenuNode, MenuSignals, Toggle, ToggleState};
use zbus::zvariant::OwnedObjectPath;

const PATH: &str = "/org/neueos/MenuBar/3";
const WAIT: Duration = Duration::from_secs(5);

/// File (Open ⌃O with an icon, separator, Recent ▸ [a.txt]), View (Word Wrap ✓, disabled
/// Zoom), and an empty Help.
fn menu() -> Vec<MenuNode> {
    let mut open = MenuNode::item(2, "Open");
    open.shortcut = vec!["Control".into(), "O".into()];
    open.icon = Some(MenuIcon::Name("document-open".into()));
    let mut wrap = MenuNode::item(8, "Word_Wrap");
    wrap.toggle = Toggle::Check(ToggleState::On);
    let mut zoom = MenuNode::item(9, "Zoom");
    zoom.enabled = false;
    vec![
        MenuNode::submenu(
            1,
            "File",
            vec![
                open,
                MenuNode::separator(3),
                MenuNode::submenu(4, "Recent", vec![MenuNode::item(5, "a.txt")]),
            ],
        ),
        MenuNode::submenu(7, "View", vec![wrap, zoom]),
        MenuNode::item(10, "Help"),
    ]
}

#[test]
fn the_client_reads_the_tree_an_app_exports() {
    let (service, client) = pair().unwrap();
    let _export = MenuExport::new(&service, PATH, menu(), |_| {}).unwrap();
    let menus = MenuClient::new(&client, ":1.7", PATH).unwrap();
    assert_eq!((menus.service(), menus.path()), (":1.7", PATH));
    // Exactly what was exported, labels and all (an empty Help asked to fill itself).
    assert_eq!(menus.fetch().unwrap(), menu());
}

#[test]
fn events_reach_the_app() {
    let (service, client) = pair().unwrap();
    let (tx, clicks) = mpsc::channel();
    let _export = MenuExport::new(&service, PATH, menu(), move |id| {
        tx.send(id).unwrap();
    })
    .unwrap();
    let menus = MenuClient::new(&client, ":1.7", PATH).unwrap();
    assert!(!menus.about_to_show(1).unwrap());
    menus.event(1, "opened").unwrap();
    menus.event(2, "clicked").unwrap();
    menus.event(1, "closed").unwrap();
    assert_eq!(clicks.recv_timeout(WAIT).unwrap(), 2);
    // A disabled item does nothing; an unknown one is an error.
    menus.event(9, "clicked").unwrap();
    assert!(menus.event(42, "clicked").is_err());
    assert!(clicks.try_recv().is_err());
}

#[test]
fn changes_are_signalled_and_read_again() {
    let (service, client) = pair().unwrap();
    let export = MenuExport::new(&service, PATH, menu(), |_| {}).unwrap();
    let menus = MenuClient::new(&client, ":1.7", PATH).unwrap();
    let signals = MenuSignals::new(&client).unwrap();
    let (tx, changes) = mpsc::channel();
    thread::spawn(move || {
        for change in signals {
            if tx.send(change).is_err() {
                return;
            }
        }
    });
    export
        .set_items(vec![MenuNode::submenu(
            1,
            "Edit",
            vec![MenuNode::item(2, "Undo")],
        )])
        .unwrap();
    let change = changes.recv_timeout(WAIT).unwrap();
    assert_eq!(change.path, PATH);
    let tree = menus.fetch().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].label, "Edit");
    assert_eq!(tree[0].children[0].label, "Undo");
}

#[test]
fn a_gone_menu_is_an_error() {
    let (_service, client) = pair().unwrap();
    let menus = MenuClient::new(&client, ":1.7", PATH).unwrap();
    assert!(menus.fetch().is_err());
    assert!(MenuClient::new(&client, "not a name", PATH).is_err());
    assert!(MenuClient::new(&client, ":1.7", "no/path").is_err());
}

#[zbus::proxy(
    interface = "com.canonical.AppMenu.Registrar",
    default_service = "com.canonical.AppMenu.Registrar",
    default_path = "/com/canonical/AppMenu/Registrar"
)]
trait AppMenuRegistrar {
    fn register_window(
        &self,
        window_id: u32,
        menu_object_path: &OwnedObjectPath,
    ) -> zbus::Result<()>;
    fn unregister_window(&self, window_id: u32) -> zbus::Result<()>;
    fn get_menu_for_window(&self, window_id: u32) -> zbus::Result<(String, OwnedObjectPath)>;
    fn get_menus(&self) -> zbus::Result<Vec<(u32, String, OwnedObjectPath)>>;
}

#[test]
fn the_registrar_keeps_registrations() {
    let (service, client) = pair().unwrap();
    let registrar = Registrar::serve(&service).unwrap();
    assert!(
        Registrar::serve(&service).is_err(),
        "one registrar per connection"
    );
    let proxy = AppMenuRegistrarProxyBlocking::new(&client).unwrap();
    let path = OwnedObjectPath::try_from(PATH).unwrap();
    proxy.register_window(0x40_0001, &path).unwrap();
    assert_eq!(registrar.len(), 1);
    // No bus, so no sender name.
    let (sender, got) = proxy.get_menu_for_window(0x40_0001).unwrap();
    assert_eq!((sender.as_str(), got.as_str()), ("", PATH));
    assert_eq!(proxy.get_menus().unwrap().len(), 1);
    proxy.unregister_window(0x40_0001).unwrap();
    assert!(registrar.is_empty());
    assert!(proxy.get_menu_for_window(0x40_0001).is_err());
    assert_eq!(registrar::NAME, "com.canonical.AppMenu.Registrar");
}
