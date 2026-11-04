//! A zbus client reads an exported menu and chooses items (task 2.18's acceptance), over a
//! peer-to-peer connection so no bus daemon is needed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use neue_dbus::menu::VERSION;
use neue_dbus::testing::pair;
use neue_dbus::{Layout, MenuExport, MenuNode, Toggle, ToggleState};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

const PATH: &str = "/org/neueos/MenuBar/0";

#[zbus::proxy(
    interface = "com.canonical.dbusmenu",
    default_service = "org.neueos.Test",
    default_path = "/org/neueos/MenuBar/0"
)]
trait DbusMenu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: &[&str],
    ) -> zbus::Result<(u32, Layout)>;

    fn get_group_properties(
        &self,
        ids: &[i32],
        property_names: &[&str],
    ) -> zbus::Result<Vec<(i32, std::collections::HashMap<String, OwnedValue>)>>;

    fn event(&self, id: i32, event_id: &str, data: &Value<'_>, timestamp: u32) -> zbus::Result<()>;

    fn about_to_show(&self, id: i32) -> zbus::Result<bool>;

    #[zbus(property)]
    fn version(&self) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn layout_updated(&self, revision: u32, parent: i32) -> zbus::Result<()>;
}

/// File (Open ⌃O, separator, Recent ▸ [a.txt]), View (Word Wrap ✓, disabled Zoom).
fn menu() -> Vec<MenuNode> {
    let mut open = MenuNode::item(2, "Open");
    open.shortcut = vec!["Control".into(), "O".into()];
    let mut wrap = MenuNode::item(8, "Word Wrap");
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
    ]
}

fn export() -> (MenuExport, Connection, mpsc::Receiver<i32>) {
    let (service, client) = pair().unwrap();
    let (tx, rx) = mpsc::channel();
    let export = MenuExport::new(&service, PATH, menu(), move |id| {
        tx.send(id).unwrap();
    })
    .unwrap();
    (export, client, rx)
}

fn label(layout: &Layout) -> String {
    String::try_from(layout.properties["label"].try_clone().unwrap()).unwrap()
}

#[test]
fn client_reads_the_whole_layout() {
    let (_export, client, _) = export();
    let proxy = DbusMenuProxyBlocking::new(&client).unwrap();
    assert_eq!(proxy.version().unwrap(), VERSION);
    let (revision, root) = proxy.get_layout(0, -1, &[]).unwrap();
    assert_eq!(revision, 1);
    assert_eq!(root.id, 0);
    let titles: Vec<String> = root.children.iter().map(label).collect();
    assert_eq!(titles, ["File", "View"]);
    let file = &root.children[0];
    let ids: Vec<i32> = file.children.iter().map(|c| c.id).collect();
    assert_eq!(ids, [2, 3, 4]);
    assert!(file.children[1].properties.contains_key("type"));
    assert_eq!(label(&file.children[2].children[0]), "a.txt");
    let wrap = &root.children[1].children[0];
    assert_eq!(i32::try_from(&wrap.properties["toggle-state"]).unwrap(), 1);
}

#[test]
fn layout_of_a_submenu_one_level_deep() {
    let (_export, client, _) = export();
    let proxy = DbusMenuProxyBlocking::new(&client).unwrap();
    let (_, file) = proxy.get_layout(1, 1, &["label"]).unwrap();
    assert_eq!(file.id, 1);
    assert_eq!(file.properties.len(), 1);
    // Recent is there, without its items.
    assert!(file.children[2].children.is_empty());
    assert!(proxy.get_layout(42, -1, &[]).is_err());
}

#[test]
fn clicking_an_item_reaches_the_callback() {
    let (_export, client, clicks) = export();
    let proxy = DbusMenuProxyBlocking::new(&client).unwrap();
    assert!(!proxy.about_to_show(1).unwrap());
    proxy.event(2, "clicked", &Value::from(0i32), 0).unwrap();
    assert_eq!(clicks.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    // Disabled items, submenus, separators and other events don't choose anything.
    for (id, event) in [
        (9, "clicked"),
        (4, "clicked"),
        (3, "clicked"),
        (5, "hovered"),
    ] {
        proxy.event(id, event, &Value::from(0i32), 0).unwrap();
    }
    proxy.event(5, "clicked", &Value::from(0i32), 0).unwrap();
    assert_eq!(clicks.recv_timeout(Duration::from_secs(5)).unwrap(), 5);
    assert!(clicks.try_recv().is_err());
    assert!(proxy.event(42, "clicked", &Value::from(0i32), 0).is_err());
}

#[test]
fn group_properties_of_some_or_all_items() {
    let (_export, client, _) = export();
    let proxy = DbusMenuProxyBlocking::new(&client).unwrap();
    let some = proxy.get_group_properties(&[2, 42], &["shortcut"]).unwrap();
    assert_eq!(some.len(), 1);
    assert_eq!(some[0].0, 2);
    let keys: Vec<Vec<String>> = some[0].1["shortcut"]
        .try_clone()
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(keys, [vec!["Control".to_owned(), "O".to_owned()]]);
    let all = proxy.get_group_properties(&[], &[]).unwrap();
    assert_eq!(all.len(), 9);
}

#[test]
fn set_items_bumps_the_revision_and_signals() {
    let (export, client, _) = export();
    let proxy = DbusMenuProxyBlocking::new(&client).unwrap();
    let updates = proxy.receive_layout_updated().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for signal in updates {
            let args = signal.args().unwrap();
            if tx.send((args.revision, args.parent)).is_err() {
                return;
            }
        }
    });
    export
        .set_items(vec![MenuNode::submenu(1, "Edit", vec![])])
        .unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), (2, 0));
    let (revision, root) = proxy.get_layout(0, -1, &[]).unwrap();
    assert_eq!(revision, 2);
    assert_eq!(label(&root.children[0]), "Edit");
}

#[test]
fn dropping_the_export_removes_the_object() {
    let (service, client) = pair().unwrap();
    let export = MenuExport::new(&service, PATH, menu(), |_| {}).unwrap();
    drop(export);
    let proxy = DbusMenuProxyBlocking::new(&client).unwrap();
    let err = proxy.get_layout(0, -1, &[]).unwrap_err();
    assert!(err.to_string().contains("UnknownObject"), "{err}");
}
