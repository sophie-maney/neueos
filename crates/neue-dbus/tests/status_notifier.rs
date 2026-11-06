//! The status-notifier watcher, host and item client against a stand-in tray app (task
//! 3.13a's acceptance), over peer-to-peer connections so no bus daemon is needed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use neue_dbus::status_notifier::watcher::{INTERFACE, NAME, PATH};
use neue_dbus::status_notifier::{
    Host, HostEvent, ItemSignals, Orientation, Status, WatcherEvent, WatcherSignals,
};
use neue_dbus::testing::pair;
use neue_dbus::{MenuExport, MenuNode, Watcher};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedObjectPath;

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
type ToolTip = (String, Pixmaps, String, String);

const WAIT: Duration = Duration::from_secs(5);
const SERVICE: &str = "org.example.Tray";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/StatusNotifierItem/Menu";
const ID: &str = "org.example.Tray/StatusNotifierItem";

struct TrayItem {
    calls: mpsc::Sender<String>,
    status: Arc<Mutex<String>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl TrayItem {
    #[zbus(property)]
    fn id(&self) -> String {
        "example".into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
        "Example".into()
    }
    #[zbus(property)]
    fn category(&self) -> String {
        "Communications".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        self.status.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        "mail-unread".into()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        vec![(1, 1, vec![0xFF, 1, 2, 3])]
    }
    #[zbus(property)]
    fn tool_tip(&self) -> ToolTip {
        (
            String::new(),
            Vec::new(),
            "Example".into(),
            "3 unread".into(),
        )
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).unwrap()
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, x: i32, y: i32) {
        self.calls.send(format!("activate {x} {y}")).unwrap();
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        self.calls.send(format!("secondary {x} {y}")).unwrap();
    }
    fn context_menu(&self, x: i32, y: i32) {
        self.calls.send(format!("context {x} {y}")).unwrap();
    }
    fn scroll(&self, delta: i32, orientation: &str) {
        self.calls
            .send(format!("scroll {delta} {orientation}"))
            .unwrap();
    }

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// Moves a blocking iterator to a thread, so a test that gets nothing fails instead of
/// hanging.
fn events<T: Send + 'static>(iter: impl Iterator<Item = T> + Send + 'static) -> Receiver<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for event in iter {
            if tx.send(event).is_err() {
                return;
            }
        }
    });
    rx
}

struct Fixture {
    watcher: Watcher,
    host: Host,
    client: zbus::blocking::Connection,
    server: zbus::blocking::Connection,
    calls: Receiver<String>,
    status: Arc<Mutex<String>>,
    registered: Receiver<WatcherEvent>,
    changes: Receiver<neue_dbus::status_notifier::ItemChanged>,
    _menu: MenuExport,
}

fn fixture() -> Fixture {
    let (server, client) = pair().unwrap();
    let watcher = Watcher::serve(&server).unwrap();
    let (tx, calls) = mpsc::channel();
    let status = Arc::new(Mutex::new("Active".to_owned()));
    server
        .object_server()
        .at(
            ITEM_PATH,
            TrayItem {
                calls: tx,
                status: Arc::clone(&status),
            },
        )
        .unwrap();
    let menu = MenuExport::new(
        &server,
        MENU_PATH,
        vec![MenuNode::submenu(
            1,
            "Root",
            vec![MenuNode::item(2, "Open"), MenuNode::item(3, "Quit")],
        )],
        |_| {},
    )
    .unwrap();
    // Subscribe before anything registers, as the host's driver must.
    let registered = events(WatcherSignals::new(&client).unwrap());
    let changes = events(ItemSignals::new(&client).unwrap());
    Fixture {
        watcher,
        host: Host::new(&client),
        client,
        server,
        calls,
        status,
        registered,
        changes,
        _menu: menu,
    }
}

impl Fixture {
    fn register(&self, service: &str) -> zbus::Result<()> {
        self.client
            .call_method(
                Some(NAME),
                PATH,
                Some(INTERFACE),
                "RegisterStatusNotifierItem",
                &(service,),
            )
            .map(|_| ())
    }

    /// Registers the example item and shows it on the host.
    fn show(&self) -> HostEvent {
        self.register(SERVICE).unwrap();
        let WatcherEvent::ItemRegistered(address) = self.registered.recv_timeout(WAIT).unwrap()
        else {
            panic!("not a registration");
        };
        assert_eq!(address, ID);
        self.host.registered(&address).expect("the item shows")
    }
}

#[test]
fn a_registered_item_is_shown_with_what_it_says_about_itself() {
    let f = fixture();
    let HostEvent::Added { id, info } = f.show() else {
        panic!("not Added");
    };
    assert_eq!(id, ID);
    assert_eq!(f.watcher.items(), [ID]);
    assert_eq!(info.id, "example");
    assert_eq!(info.display_name(), "Example");
    assert_eq!(info.status, Status::Active);
    assert_eq!(info.icon.name, "mail-unread");
    assert_eq!(info.icon.pixmaps[0].rgba, [1, 2, 3, 0xFF]);
    assert_eq!(info.tooltip.as_ref().unwrap().text, "3 unread");
    assert_eq!(info.menu.as_deref(), Some(MENU_PATH));
    // The same registration again changes nothing.
    f.register(SERVICE).unwrap();
    assert_eq!(f.watcher.items().len(), 1);
    assert!(f.host.registered(ID).is_none());
}

#[test]
fn a_changed_item_is_read_again_and_only_a_real_change_is_reported() {
    let f = fixture();
    f.show();
    // A signal for nothing that changed.
    let emitter = SignalEmitter::new(f.server.inner(), ITEM_PATH).unwrap();
    zbus::block_on(TrayItem::new_tool_tip(&emitter)).unwrap();
    let change = f.changes.recv_timeout(WAIT).unwrap();
    assert_eq!(change.path, ITEM_PATH);
    assert!(f.host.changed(&change).is_none());
    // Now the status changes.
    *f.status.lock().unwrap() = "NeedsAttention".into();
    zbus::block_on(TrayItem::new_status(&emitter, "NeedsAttention")).unwrap();
    let change = f.changes.recv_timeout(WAIT).unwrap();
    let Some(HostEvent::Changed { id, info }) = f.host.changed(&change) else {
        panic!("not Changed");
    };
    assert_eq!((id.as_str(), info.status), (ID, Status::NeedsAttention));
}

#[test]
fn actions_reach_the_item() {
    let f = fixture();
    f.show();
    f.host.activate(ID, 3, 4).unwrap();
    f.host.secondary_activate(ID, 5, 6).unwrap();
    f.host.context_menu(ID, 7, 8).unwrap();
    f.host.scroll(ID, -1, Orientation::Vertical).unwrap();
    let got: Vec<String> = (0..4)
        .map(|_| f.calls.recv_timeout(WAIT).unwrap())
        .collect();
    assert_eq!(
        got,
        [
            "activate 3 4",
            "secondary 5 6",
            "context 7 8",
            "scroll -1 vertical"
        ]
    );
    assert!(matches!(
        f.host.activate("nobody/Else", 0, 0),
        Err(neue_dbus::Error::UnknownItem(_))
    ));
}

#[test]
fn the_items_menu_is_read_through_dbusmenu() {
    let f = fixture();
    f.show();
    let menu = f.host.menu(ID).unwrap().expect("the item has a menu");
    let tree = menu.fetch().unwrap();
    assert_eq!(tree[0].label, "Root");
    let labels: Vec<_> = tree[0].children.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["Open", "Quit"]);
    menu.event(2, "clicked").unwrap();
}

#[test]
fn a_departing_connection_takes_its_items_away() {
    let f = fixture();
    f.show();
    f.watcher.forget("org.example.Other").unwrap();
    assert_eq!(
        f.watcher.items().len(),
        1,
        "someone else's name changes nothing"
    );
    f.watcher.forget(SERVICE).unwrap();
    assert!(f.watcher.items().is_empty());
    assert_eq!(
        f.registered.recv_timeout(WAIT).unwrap(),
        WatcherEvent::ItemUnregistered(ID.to_owned())
    );
    assert_eq!(
        f.host.unregistered(ID),
        Some(HostEvent::Removed { id: ID.to_owned() })
    );
    assert!(f.host.unregistered(ID).is_none());
}

#[test]
fn a_host_that_starts_late_finds_the_items_already_there() {
    let f = fixture();
    f.show();
    let late = Host::new(&f.client);
    let initial = late.initial().unwrap();
    assert!(matches!(initial.as_slice(), [HostEvent::Added { id, .. }] if id == ID));
}

#[test]
fn hosts_register_and_bad_registrations_are_refused() {
    let f = fixture();
    assert!(!f.watcher.has_host());
    f.client
        .call_method(
            Some(NAME),
            PATH,
            Some(INTERFACE),
            "RegisterStatusNotifierHost",
            &("org.kde.StatusNotifierHost-1",),
        )
        .unwrap();
    assert!(f.watcher.has_host());
    f.watcher.forget("org.kde.StatusNotifierHost-1").unwrap();
    assert!(!f.watcher.has_host());
    // Not a name, and a bare path (there is no sender to take as the bus here).
    assert!(f.register("not a name").is_err());
    assert!(f.register("/just/a/path").is_err());
    assert!(f.watcher.items().is_empty());
}
