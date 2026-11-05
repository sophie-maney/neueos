//! The GMenuModel client against a stand-in for a GTK app's exporter (task 3.8's
//! acceptance), over peer-to-peer connections so no bus daemon is needed.

#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use neue_dbus::testing::pair;
use neue_dbus::{GMenuAddress, GMenuClient, MenuChanged, NodeKind, Scope, Toggle, ToggleState};
use zbus::zvariant::{OwnedValue, Value};

const MENUBAR: &str = "/org/example/App/menus/menubar";
const APP_MENU: &str = "/org/example/App/menus/appmenu";
const APP: &str = "/org/example/App";
const WINDOW: &str = "/org/example/App/window/1";

type Item = HashMap<String, OwnedValue>;
type Content = Vec<(u32, u32, Vec<Item>)>;

fn val<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
    OwnedValue::try_from(v.into()).unwrap()
}

fn item(attrs: &[(&str, OwnedValue)]) -> Item {
    attrs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.try_clone().unwrap()))
        .collect()
}

fn link(group: u32, menu: u32) -> OwnedValue {
    val(Value::from(
        zbus::zvariant::StructureBuilder::new()
            .add_field(group)
            .add_field(menu)
            .build()
            .unwrap(),
    ))
}

/// What a GTK app exports as its menu bar: File (a section of New and Open with an accel,
/// a section with Quit), View (a check item and two radio items, one hidden when
/// disabled). Submenus live in groups of their own, as GTK's exporter does it.
fn menubar_content(group: u32) -> Content {
    let all = vec![
        (
            0,
            0,
            vec![
                item(&[("label", val("_File")), (":submenu", link(1, 0))]),
                item(&[("label", val("_View")), (":submenu", link(2, 0))]),
            ],
        ),
        (
            1,
            0,
            vec![
                item(&[(":section", link(1, 1))]),
                item(&[(":section", link(1, 2))]),
            ],
        ),
        (
            1,
            1,
            vec![
                item(&[("label", val("_New")), ("action", val("app.new"))]),
                item(&[
                    ("label", val("_Open")),
                    ("action", val("win.open")),
                    ("accel", val("<Primary>o")),
                ]),
            ],
        ),
        (
            1,
            2,
            vec![item(&[
                ("label", val("_Quit")),
                ("action", val("app.quit")),
            ])],
        ),
        (
            2,
            0,
            vec![
                item(&[("label", val("Show _Grid")), ("action", val("win.grid"))]),
                item(&[
                    ("label", val("Small")),
                    ("action", val("win.size")),
                    ("target", val("small")),
                ]),
                item(&[
                    ("label", val("Large")),
                    ("action", val("win.size")),
                    ("target", val("large")),
                ]),
                item(&[
                    ("label", val("Secret")),
                    ("action", val("win.secret")),
                    ("hidden-when", val("action-disabled")),
                ]),
            ],
        ),
    ];
    all.into_iter().filter(|(g, ..)| *g == group).collect()
}

fn app_menu_content() -> Content {
    vec![
        (0, 0, vec![item(&[(":section", link(0, 1))])]),
        (
            0,
            1,
            vec![item(&[
                ("label", val("_About")),
                ("action", val("app.about")),
            ])],
        ),
    ]
}

/// What the stand-in saw.
#[derive(Default)]
struct Log {
    started: Vec<(String, Vec<u32>)>,
    ended: Vec<(String, Vec<u32>)>,
    activated: Vec<(String, Vec<String>)>,
}

struct Menus {
    path: &'static str,
    log: Arc<Mutex<Log>>,
}

#[zbus::interface(name = "org.gtk.Menus")]
impl Menus {
    fn start(&self, groups: Vec<u32>) -> Content {
        self.log
            .lock()
            .unwrap()
            .started
            .push((self.path.to_owned(), groups.clone()));
        if self.path == MENUBAR {
            groups.iter().flat_map(|g| menubar_content(*g)).collect()
        } else {
            app_menu_content()
        }
    }

    fn end(&self, groups: Vec<u32>) {
        self.log
            .lock()
            .unwrap()
            .ended
            .push((self.path.to_owned(), groups));
    }
}

struct Actions {
    scope: &'static str,
    log: Arc<Mutex<Log>>,
}

type Described = HashMap<String, (bool, zbus::zvariant::Signature, Vec<OwnedValue>)>;

#[zbus::interface(name = "org.gtk.Actions")]
impl Actions {
    fn describe_all(&self) -> Described {
        let mut all = Described::new();
        let unit = zbus::zvariant::Signature::Unit;
        if self.scope == "app" {
            for name in ["new", "quit", "about"] {
                all.insert(name.into(), (true, unit.clone(), vec![]));
            }
        } else {
            all.insert("open".into(), (true, unit.clone(), vec![]));
            all.insert("grid".into(), (true, unit.clone(), vec![val(true)]));
            all.insert(
                "size".into(),
                (true, "s".try_into().unwrap(), vec![val("large")]),
            );
            all.insert("secret".into(), (false, unit, vec![]));
        }
        all
    }

    fn activate(&self, name: &str, parameter: Vec<OwnedValue>, _platform_data: Item) {
        let parameter = parameter
            .iter()
            .map(|v| <&str>::try_from(&**v).unwrap().to_owned())
            .collect();
        self.log
            .lock()
            .unwrap()
            .activated
            .push((format!("{}.{name}", self.scope), parameter));
    }
}

fn address() -> GMenuAddress {
    GMenuAddress {
        bus_name: ":1.9".into(),
        menubar_path: Some(MENUBAR.into()),
        app_menu_path: Some(APP_MENU.into()),
        application_path: Some(APP.into()),
        window_path: Some(WINDOW.into()),
    }
}

fn serve() -> (zbus::blocking::Connection, GMenuClient, Arc<Mutex<Log>>) {
    let (service, client) = pair().unwrap();
    let log = Arc::new(Mutex::new(Log::default()));
    {
        let server = service.object_server();
        for path in [MENUBAR, APP_MENU] {
            let menus = Menus {
                path,
                log: Arc::clone(&log),
            };
            assert!(server.at(path, menus).unwrap());
        }
        for (path, scope) in [(APP, "app"), (WINDOW, "win")] {
            let actions = Actions {
                scope,
                log: Arc::clone(&log),
            };
            assert!(server.at(path, actions).unwrap());
        }
    }
    let client = GMenuClient::new(&client, address()).unwrap();
    (service, client, log)
}

fn labels(nodes: &[neue_dbus::MenuNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.label.as_str()).collect()
}

#[test]
fn the_menu_bar_is_read_with_sections_and_submenus() {
    let (_service, client, log) = serve();
    let menus = client.fetch().unwrap();
    assert_eq!(labels(&menus.titles), ["File", "View"]);
    let file = &menus.titles[0].children;
    // New, Open | Quit: sections flattened with a separator, mnemonics removed.
    assert_eq!(labels(file), ["New", "Open", "", "Quit"]);
    assert_eq!(file[2].kind, NodeKind::Separator);
    assert_eq!(file[1].shortcut, ["Control", "O"]);
    // Subscribed to group 0, then to the groups its submenus live in.
    let started = log.lock().unwrap().started.clone();
    assert_eq!(started[0], (MENUBAR.to_owned(), vec![0]));
    let mut later: Vec<u32> = started
        .iter()
        .filter(|(p, _)| p == MENUBAR)
        .skip(1)
        .flat_map(|(_, g)| g.clone())
        .collect();
    later.sort_unstable();
    assert_eq!(later, [1, 2]);
}

#[test]
fn the_app_menu_is_read_apart() {
    let (_service, client, _log) = serve();
    let menus = client.fetch().unwrap();
    assert_eq!(labels(&menus.app_menu), ["About"]);
}

#[test]
fn actions_give_state_and_enabled() {
    let (_service, client, _log) = serve();
    let menus = client.fetch().unwrap();
    let view = &menus.titles[1].children;
    // "Secret" is disabled and says hidden-when action-disabled.
    assert_eq!(labels(view), ["Show Grid", "Small", "Large"]);
    assert_eq!(view[0].toggle, Toggle::Check(ToggleState::On));
    assert_eq!(view[1].toggle, Toggle::Radio(ToggleState::Off));
    assert_eq!(view[2].toggle, Toggle::Radio(ToggleState::On));
    assert!(view.iter().all(|n| n.enabled));
}

#[test]
fn choosing_an_item_activates_its_action() {
    let (_service, client, log) = serve();
    let menus = client.fetch().unwrap();
    let quit = &menus.titles[0].children[3];
    let open = &menus.titles[0].children[1];
    let small = &menus.titles[1].children[1];
    let grid = &menus.titles[1].children[0];
    for node in [quit, open, small, grid] {
        client.activate(&menus.activations[&node.id]).unwrap();
    }
    assert_eq!(menus.activations[&quit.id].scope, Scope::App);
    assert_eq!(menus.activations[&open.id].scope, Scope::Window);
    assert_eq!(
        log.lock().unwrap().activated,
        [
            ("app.quit".to_owned(), vec![]),
            ("win.open".to_owned(), vec![]),
            // Radio items pass their target; toggles pass nothing.
            ("win.size".to_owned(), vec!["small".to_owned()]),
            ("win.grid".to_owned(), vec![]),
        ]
    );
    // Separators and submenu titles don't act.
    assert!(!menus.activations.contains_key(&menus.titles[0].id));
}

#[test]
fn fetching_again_keeps_the_subscription() {
    let (_service, client, log) = serve();
    client.fetch().unwrap();
    client.fetch().unwrap();
    {
        let log = log.lock().unwrap();
        // The second read asked again (3 groups of the menu bar, 1 of the app menu) and
        // only then let go of the first read's.
        let count =
            |calls: &[(String, Vec<u32>)]| calls.iter().map(|(_, g)| g.len()).sum::<usize>();
        assert_eq!(count(&log.started), 2 * 4);
        assert_eq!(count(&log.ended), 4);
    }
    drop(client);
    assert_eq!(
        log.lock()
            .unwrap()
            .ended
            .iter()
            .map(|(_, g)| g.len())
            .sum::<usize>(),
        2 * 4
    );
}

#[test]
fn changes_match_this_windows_paths() {
    let (_service, client, _log) = serve();
    let change = |sender: &str, path: &str| MenuChanged {
        sender: sender.into(),
        path: path.into(),
    };
    assert!(client.is_changed_by(&change(":1.9", MENUBAR)));
    assert!(client.is_changed_by(&change(":1.9", WINDOW)));
    assert!(!client.is_changed_by(&change(":1.8", MENUBAR)));
    assert!(!client.is_changed_by(&change(":1.9", "/other")));
}

#[test]
fn a_dead_app_is_an_error_not_a_hang() {
    let (service, client, _log) = serve();
    drop(service);
    assert!(client.fetch().is_err());
}
