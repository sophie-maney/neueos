//! Menu bars exported as DBusMenu (task 2.18): a zbus client on a peer-to-peer connection
//! reads the window's menu bar and chooses items, which reach `update` as messages.

use std::time::{Duration, Instant};

use neue_dbus::Layout;
use neue_dbus::menu::INTERFACE;
use neue_dbus::zbus::blocking::Connection;
use neue_dbus::zbus::zvariant::Value;

use super::*;
use crate::{Label, Menu, MenuItem, Shortcut};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    New,
    Wrap,
    Big,
}

#[derive(Default)]
struct Editor {
    window: Option<WindowId>,
    got: Vec<Msg>,
}

fn file_view() -> Menu<Msg> {
    Menu::new()
        .with(MenuItem::submenu(
            "File",
            Menu::new().with(
                MenuItem::new("New_File")
                    .shortcut(Shortcut::command("n").shift())
                    .on_choose(Msg::New),
            ),
        ))
        .with(MenuItem::submenu(
            "View",
            Menu::new()
                .with(MenuItem::new("Wrap").checked(true).on_choose(Msg::Wrap))
                .separator()
                .with(MenuItem::new("Big").disabled(true).on_choose(Msg::Big)),
        ))
}

impl Application for Editor {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let window = Window::new("Editor", (200, 100), Label::new("text")).menu_bar(file_view());
        self.window = Some(cx.open_window(window));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        // Choosing New adds an Edit menu.
        if message == Msg::New
            && let Some(window) = self.window
        {
            cx.set_menu_bar(
                window,
                file_view().with(MenuItem::submenu("Edit", Menu::new())),
            );
        }
        self.got.push(message);
    }
}

const PATH: &str = "/org/neueos/MenuBar/0";

fn get_layout(client: &Connection) -> neue_dbus::zbus::Result<(u32, Layout)> {
    let reply = client.call_method(
        None::<&str>,
        PATH,
        Some(INTERFACE),
        "GetLayout",
        &(0i32, -1i32, Vec::<String>::new()),
    )?;
    reply.body().deserialize()
}

fn click(client: &Connection, id: i32) {
    client
        .call_method(
            None::<&str>,
            PATH,
            Some(INTERFACE),
            "Event",
            &(id, "clicked", Value::from(0i32), 0u32),
        )
        .unwrap();
}

fn label(layout: &Layout) -> String {
    String::try_from(layout.properties["label"].try_clone().unwrap()).unwrap()
}

/// Drains the runner until the app has `n` messages, or panics after a few seconds.
fn wait_for(runner: &mut Runner<Editor>, host: &mut FakeHost, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while runner.app().got.len() < n {
        assert!(Instant::now() < deadline, "only {:?}", runner.app().got);
        std::thread::sleep(Duration::from_millis(5));
        runner.drain(host);
    }
}

fn start_editor() -> (Runner<Editor>, FakeHost, Connection) {
    let (service, client) = neue_dbus::testing::pair().unwrap();
    let mut runner = Runner::new(Editor::default(), Settings::default());
    runner.dbus = Some(service);
    let mut host = FakeHost::default();
    runner.start(&mut host, Arc::new(|| {}));
    (runner, host, client)
}

#[test]
fn the_menu_bar_is_exported_with_its_items() {
    let (_runner, host, client) = start_editor();
    // Peer to peer there's no bus name, so no address for the compositor.
    assert!(host.menu_addresses.is_empty());
    let (revision, root) = get_layout(&client).unwrap();
    assert_eq!(revision, 1);
    let titles: Vec<String> = root.children.iter().map(label).collect();
    assert_eq!(titles, ["File", "View"]);
    // Ids are the model's depth-first ids plus one.
    let new = &root.children[0].children[0];
    assert_eq!(new.id, 2);
    assert_eq!(label(new), "New__File");
    let keys: Vec<Vec<String>> = new.properties["shortcut"]
        .try_clone()
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(keys, [["Shift", "Super", "N"]]);
    let view: Vec<i32> = root.children[1].children.iter().map(|c| c.id).collect();
    assert_eq!(view, [4, 5, 6]);
    let wrap = &root.children[1].children[0].properties;
    assert_eq!(i32::try_from(&wrap["toggle-state"]).unwrap(), 1);
    assert!(
        root.children[1].children[2]
            .properties
            .contains_key("enabled")
    );
}

#[test]
fn choosing_items_sends_their_messages_and_updates_reach_clients() {
    let (mut runner, mut host, client) = start_editor();
    click(&client, 4);
    wait_for(&mut runner, &mut host, 1);
    // Disabled items, separators and submenus send nothing; New does.
    click(&client, 6);
    click(&client, 5);
    click(&client, 3);
    click(&client, 2);
    wait_for(&mut runner, &mut host, 2);
    assert_eq!(runner.app().got, [Msg::Wrap, Msg::New]);
    // New replaced the menu bar.
    let (revision, root) = get_layout(&client).unwrap();
    assert_eq!(revision, 2);
    assert_eq!(label(&root.children[2]), "Edit");
}

#[test]
fn closing_the_window_removes_its_menu_bar() {
    let (mut runner, mut host, client) = start_editor();
    let window = runner.app().window.unwrap();
    runner.close_requested(&mut host, window);
    let err = get_layout(&client).unwrap_err();
    assert!(err.to_string().contains("UnknownObject"), "{err}");
}
