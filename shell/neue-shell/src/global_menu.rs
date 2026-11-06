//! The global menu's D-Bus side (tasks 3.7a and 3.8, ADR 0005, ADR 0076, ADR 0078): a
//! worker thread that owns `com.canonical.AppMenu.Registrar`, follows the active window's
//! exported menus (DBusMenu, or GTK's GMenuModel and actions) and
//! passes on what the user does with them, and the conversion of those menus into the
//! bar's titles.
//!
//! The UI thread sends [`Input`]s; the worker sends back [`Menus`] (through the kit's task
//! sender). Every followed menu gets a new generation, and events carry the generation of
//! the menus they came from, so a click on a title that was just replaced never reaches
//! the wrong app.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use neue_dbus::zbus::blocking::Connection;
use neue_dbus::{
    Activation, GMenuAddress, GMenuClient, GMenuSignals, MenuChanged, MenuClient, MenuNode,
    MenuSignals, NodeKind, Registrar,
};
use neue_dbus::{Toggle, ToggleState};
use neue_kit::desktop::{MenuTitle, ToplevelMenu};
use neue_kit::{CheckState, Handle, Menu, MenuItem, Shortcut};
use neue_shell::help;
use neue_shell::search::Entry;

/// How long the shell waits for an app to answer before giving up on a call.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

/// Where a window's menus are exported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// `com.canonical.dbusmenu`: the exporting connection and the object path.
    DbusMenu { service: String, path: String },
    /// GTK's `org.gtk.Menus` and `org.gtk.Actions`.
    GMenu(GMenuAddress),
}

impl Address {
    /// The address of a window's menus, if it exports any.
    pub fn of(menu: &ToplevelMenu) -> Option<Self> {
        match menu {
            ToplevelMenu::DbusMenu { service, path } => Some(Self::DbusMenu {
                service: service.clone(),
                path: path.clone(),
            }),
            ToplevelMenu::GMenu {
                bus_name,
                menubar_path,
                app_menu_path,
                application_path,
                window_path,
            } => Some(Self::GMenu(GMenuAddress {
                bus_name: bus_name.clone(),
                menubar_path: menubar_path.clone(),
                app_menu_path: app_menu_path.clone(),
                application_path: application_path.clone(),
                window_path: window_path.clone(),
            })),
            ToplevelMenu::None => None,
        }
    }
}

/// What happened to an item of the app's menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemEvent {
    Clicked,
    /// A menu-bar title's menu opened (`AboutToShow`, then `opened`).
    Opened,
    Closed,
}

/// A user action on the app's menus: `event` on item `id` of generation `generation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuEvent {
    pub generation: u64,
    pub id: i32,
    pub event: ItemEvent,
}

/// What the worker is told.
#[derive(Debug)]
pub enum Input {
    /// Follow these menus from now on (`None`: the active window exports none).
    Follow(Option<Address>),
    Event(MenuEvent),
    /// A menu on the bus changed (from the signal thread).
    Changed(MenuChanged),
}

/// The app's menus as last read: the menu-bar titles and everything under them, and the
/// items of the app menu (GTK 3's, shown under the app's name). Empty when the window has
/// none or they couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menus {
    pub generation: u64,
    pub titles: Vec<MenuNode>,
    pub app_menu: Vec<MenuNode>,
}

impl Menus {
    /// No menus, as of `generation`.
    pub fn empty(generation: u64) -> Self {
        Self {
            generation,
            titles: Vec::new(),
            app_menu: Vec::new(),
        }
    }
}

/// The menus being followed, by the protocol they use.
enum Source {
    Dbus(MenuClient),
    GMenu {
        client: GMenuClient,
        /// What each item of the last read does.
        activations: HashMap<i32, Activation>,
    },
}

/// The menus being followed.
struct Followed {
    generation: u64,
    source: Option<Source>,
}

impl Followed {
    /// Whether `change` is about the followed menu. Signals always come from a unique name;
    /// an address with a well-known name is matched on the path alone.
    fn is_changed_by(&self, change: &MenuChanged) -> bool {
        match &self.source {
            Some(Source::Dbus(c)) => {
                c.path() == change.path
                    && (c.service() == change.sender || !c.service().starts_with(':'))
            }
            Some(Source::GMenu { client, .. }) => client.is_changed_by(change),
            None => false,
        }
    }

    /// Handles `input`; true if the menus must be read again.
    fn handle(&mut self, conn: &Connection, input: Input) -> bool {
        match input {
            Input::Follow(address) => {
                self.generation += 1;
                self.source = address.and_then(|a| {
                    connect(conn, &a)
                        .map_err(|err| tracing::warn!(%err, ?a, "bad menu address"))
                        .ok()
                });
                true
            }
            Input::Changed(change) => self.is_changed_by(&change),
            Input::Event(event) if event.generation == self.generation => {
                let client = match &self.source {
                    Some(Source::Dbus(client)) => client,
                    Some(Source::GMenu {
                        client,
                        activations,
                    }) => {
                        // No AboutToShow here; the app says by itself when menus change.
                        if event.event == ItemEvent::Clicked
                            && let Some(activation) = activations.get(&event.id)
                            && let Err(err) = client.activate(activation)
                        {
                            tracing::debug!(%err, id = event.id, "action failed");
                        }
                        return false;
                    }
                    None => return false,
                };
                let name = match event.event {
                    ItemEvent::Clicked => "clicked",
                    ItemEvent::Opened => "opened",
                    ItemEvent::Closed => "closed",
                };
                let mut again = false;
                if event.event == ItemEvent::Opened {
                    again = client.about_to_show(event.id).unwrap_or_else(|err| {
                        tracing::debug!(%err, "AboutToShow failed");
                        false
                    });
                }
                if let Err(err) = client.event(event.id, name) {
                    tracing::debug!(%err, id = event.id, name, "menu event failed");
                }
                again
            }
            // From menus no longer shown.
            Input::Event(_) => false,
        }
    }

    fn read(&mut self) -> Menus {
        let mut menus = Menus::empty(self.generation);
        match &mut self.source {
            Some(Source::Dbus(client)) => match client.fetch() {
                Ok(titles) => menus.titles = titles,
                Err(err) => {
                    tracing::warn!(%err, service = client.service(), "can't read the app's menus");
                }
            },
            Some(Source::GMenu {
                client,
                activations,
            }) => match client.fetch() {
                Ok(read) => {
                    menus.titles = read.titles;
                    menus.app_menu = read.app_menu;
                    *activations = read.activations;
                }
                Err(err) => {
                    tracing::warn!(%err, address = ?client.address(), "can't read the app's menus");
                    activations.clear();
                }
            },
            None => {}
        }
        menus
    }
}

/// A client for the menus at `address`. Nothing is sent yet.
fn connect(conn: &Connection, address: &Address) -> Result<Source, neue_dbus::Error> {
    Ok(match address {
        Address::DbusMenu { service, path } => Source::Dbus(MenuClient::new(conn, service, path)?),
        Address::GMenu(address) => Source::GMenu {
            client: GMenuClient::new(conn, address.clone())?,
            activations: HashMap::new(),
        },
    })
}

/// Runs the worker until the shell drops its sender: owns the registrar, reads the
/// followed menus and sends them with `send` (false: the shell is gone). `inputs` also
/// gets [`Input::Changed`] from a signal thread through `tx`.
pub fn run(
    inputs: &mpsc::Receiver<Input>,
    tx: mpsc::Sender<Input>,
    send: impl Fn(Menus) -> bool,
) -> Result<(), neue_dbus::Error> {
    let conn = neue_dbus::connect_session_client(CALL_TIMEOUT)?;
    // Qt, Chromium and LibreOffice export only while this name is on the bus.
    let _registrar = Registrar::own(&conn)
        .map_err(|err| tracing::warn!(%err, "can't own the menu registrar"))
        .ok();
    let signals = MenuSignals::new(&conn)?;
    let gtk_signals = GMenuSignals::new(&conn)?;
    let gtk_tx = tx.clone();
    forward("dbusmenu-signals", signals, tx)?;
    forward("gmenu-signals", gtk_signals, gtk_tx)?;
    let mut followed = Followed {
        generation: 0,
        source: None,
    };
    while let Ok(input) = inputs.recv() {
        let mut again = followed.handle(&conn, input);
        // Whatever else is queued first, so a burst of signals is read once.
        while let Ok(input) = inputs.try_recv() {
            again |= followed.handle(&conn, input);
        }
        if again && !send(followed.read()) {
            break;
        }
    }
    Ok(())
}

/// Sends each change of `signals` to the worker, on a thread of its own.
fn forward(
    name: &str,
    signals: impl Iterator<Item = MenuChanged> + Send + 'static,
    tx: mpsc::Sender<Input>,
) -> Result<(), neue_dbus::Error> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            for change in signals {
                if tx.send(Input::Changed(change)).is_err() {
                    return;
                }
            }
        })
        .map_err(|err| neue_dbus::Error::Dbus(err.into()))?;
    Ok(())
}

/// The bar's titles for `titles` of generation `generation`: `message(event)` for each
/// item chosen and each title opened or closed. Separators and items without a menu at the
/// top level are left out (the bar has no room for buttons). `super_as_ctrl`: the app gets
/// Super as Control, so its Control shortcuts show as ⌘ ([`shortcut`]).
///
/// The app's Help menu, if it has one, is where menu search goes: `help` is pointed at
/// its title (and left unbound when there is none), and with `search` it gets the item
/// sending that message at the top.
pub fn bar_titles<M: Clone + 'static>(
    titles: &[MenuNode],
    generation: u64,
    super_as_ctrl: bool,
    search: Option<&M>,
    help: &mut Handle<MenuTitle<M>>,
    message: impl Fn(MenuEvent) -> M + Clone + 'static,
) -> Vec<MenuTitle<M>> {
    let event = |id, event| MenuEvent {
        generation,
        id,
        event,
    };
    let mut built = Vec::new();
    for t in titles
        .iter()
        .filter(|t| t.kind == NodeKind::Item && !t.children.is_empty())
    {
        let is_help = !help.is_bound() && help::is_help(&t.label);
        let first = match search {
            Some(open) if is_help => Menu::new()
                .with(help::search_item(open.clone()))
                .separator(),
            _ => Menu::new(),
        };
        let mut title = MenuTitle::new(
            t.label.clone(),
            menu_from(first, &t.children, generation, super_as_ctrl, &message),
        )
        .on_open(message(event(t.id, ItemEvent::Opened)))
        .on_close(message(event(t.id, ItemEvent::Closed)));
        if is_help {
            title = title.bind(help);
        }
        built.push(title);
    }
    built
}

/// What menu search finds in the app's menus: every enabled item, by its path from the
/// menu title (the app menu is under `app_name`). Submenus that were never opened may be
/// empty, and so are not searched. Choosing one is [`ItemEvent::Clicked`].
pub fn entries(menus: &Menus, app_name: &str, super_as_ctrl: bool) -> Vec<Entry<MenuEvent>> {
    let mut found = Vec::new();
    let mut walk = |path: &mut Vec<String>, nodes: &[MenuNode]| {
        collect(&mut found, path, nodes, menus.generation, super_as_ctrl);
    };
    walk(&mut vec![app_name.to_owned()], &menus.app_menu);
    for title in &menus.titles {
        walk(&mut vec![title.label.clone()], &title.children);
    }
    found
}

fn collect(
    found: &mut Vec<Entry<MenuEvent>>,
    path: &mut Vec<String>,
    nodes: &[MenuNode],
    generation: u64,
    super_as_ctrl: bool,
) {
    for node in nodes.iter().filter(|n| n.kind == NodeKind::Item) {
        if !node.children.is_empty() {
            path.push(node.label.clone());
            collect(found, path, &node.children, generation, super_as_ctrl);
            path.pop();
        } else if node.enabled && !node.label.trim().is_empty() {
            let mut full = path.clone();
            full.push(node.label.clone());
            found.push(Entry {
                path: full,
                shortcut: shortcut(&node.shortcut, super_as_ctrl),
                target: MenuEvent {
                    generation,
                    id: node.id,
                    event: ItemEvent::Clicked,
                },
            });
        }
    }
}

/// The kit menu under the app's name for the app menu `items`: each item chosen sends
/// `message(event)`.
pub fn app_menu<M: 'static>(
    items: &[MenuNode],
    generation: u64,
    super_as_ctrl: bool,
    message: impl Fn(MenuEvent) -> M + Clone + 'static,
) -> Menu<M> {
    menu(items, generation, super_as_ctrl, &message)
}

/// A kit menu for `items`.
fn menu<M: 'static>(
    items: &[MenuNode],
    generation: u64,
    super_as_ctrl: bool,
    message: &(impl Fn(MenuEvent) -> M + Clone + 'static),
) -> Menu<M> {
    menu_from(Menu::new(), items, generation, super_as_ctrl, message)
}

/// `first` followed by a kit menu's items for `items`.
fn menu_from<M: 'static>(
    first: Menu<M>,
    items: &[MenuNode],
    generation: u64,
    super_as_ctrl: bool,
    message: &(impl Fn(MenuEvent) -> M + Clone + 'static),
) -> Menu<M> {
    items.iter().fold(first, |built, node| {
        if node.kind == NodeKind::Separator {
            return built.separator();
        }
        let item = if node.children.is_empty() {
            let message = message.clone();
            let (id, generation) = (node.id, generation);
            MenuItem::new(node.label.clone()).on_choose_with(move || {
                message(MenuEvent {
                    generation,
                    id,
                    event: ItemEvent::Clicked,
                })
            })
        } else {
            MenuItem::submenu(
                node.label.clone(),
                menu(&node.children, generation, super_as_ctrl, message),
            )
        };
        let item = match node.toggle {
            Toggle::None => item,
            Toggle::Check(state) => item.state(match state {
                ToggleState::Off => CheckState::Off,
                ToggleState::On => CheckState::On,
                ToggleState::Mixed => CheckState::Mixed,
            }),
            Toggle::Radio(state) => item.radio(state == ToggleState::On),
        };
        let item = match shortcut(&node.shortcut, super_as_ctrl) {
            Some(keys) => item.shortcut(keys),
            None => item,
        };
        // Icons are read but not drawn: the bar's menus show none (ADR 0076).
        built.with(item.disabled(!node.enabled))
    })
}

/// A DBusMenu shortcut (`["Control", "Shift", "S"]`) as the kit shows it. With
/// `super_as_ctrl` (the compositor sends the app Control for Super, ADR 0079) Control is
/// the key the user types as ⌘, so it shows as ⌘ (ADR 0081); a Super in the same
/// shortcut is the same key and shows once.
fn shortcut(keys: &[String], super_as_ctrl: bool) -> Option<Shortcut> {
    let (key, modifiers) = keys.split_last()?;
    let command = |m: &String| m == "Super" || (super_as_ctrl && m == "Control");
    let mut shortcut = if modifiers.iter().any(command) {
        Shortcut::command(key.clone())
    } else {
        Shortcut::new(key.clone())
    };
    for modifier in modifiers {
        shortcut = match modifier.as_str() {
            "Control" if !super_as_ctrl => shortcut.ctrl(),
            "Alt" => shortcut.alt(),
            "Shift" => shortcut.shift(),
            _ => shortcut,
        };
    }
    Some(shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<MenuNode> {
        let mut save = MenuNode::item(2, "Save");
        save.shortcut = vec!["Control".into(), "Shift".into(), "S".into()];
        let mut wrap = MenuNode::item(5, "Wrap");
        wrap.toggle = Toggle::Check(ToggleState::On);
        let mut gone = MenuNode::item(6, "Gone");
        gone.enabled = false;
        vec![
            MenuNode::submenu(1, "File", vec![save, MenuNode::separator(3)]),
            MenuNode::submenu(4, "View", vec![wrap, gone]),
            // No menu: not a title.
            MenuNode::item(7, "Help"),
            MenuNode::separator(8),
        ]
    }

    #[test]
    fn titles_are_the_top_level_menus() {
        let titles = bar_titles(&tree(), 3, false, None, &mut Handle::default(), |e| e);
        assert_eq!(titles.len(), 2);
        let file = menu(&tree()[0].children, 3, false, &|e| e);
        let save = &file.items()[0];
        assert_eq!(save.label(), "Save");
        assert_eq!(
            save.shortcut_keys(),
            Some(&Shortcut::new("S").ctrl().shift())
        );
        assert!(file.items()[1].is_separator());
        let view = menu(&tree()[1].children, 3, false, &|e| e);
        assert_eq!(
            view.items()[0].toggle(),
            neue_kit::MenuToggle::Check(CheckState::On)
        );
        assert!(!view.items()[1].is_enabled());
    }

    #[test]
    fn an_apps_help_menu_gets_the_search_item_and_the_handle() {
        let mut tree = tree();
        tree.push(MenuNode::submenu(
            9,
            "Help",
            vec![MenuNode::item(10, "Manual")],
        ));
        let mut handle = Handle::default();
        let titles = bar_titles(&tree, 3, false, Some(&0u8), &mut handle, |_| 1u8);
        assert_eq!(titles.len(), 3);
        assert!(handle.is_bound());
        // Without one the handle stays unbound.
        let mut none = Handle::default();
        bar_titles(&self::tree(), 3, false, Some(&0u8), &mut none, |_| 1u8);
        assert!(!none.is_bound());
    }

    #[test]
    fn search_finds_enabled_items_by_path() {
        let mut nested = MenuNode::submenu(
            20,
            "Recent",
            vec![MenuNode::item(21, "One"), MenuNode::separator(22)],
        );
        nested.enabled = true;
        let menus = Menus {
            generation: 4,
            titles: vec![
                MenuNode::submenu(1, "File", vec![MenuNode::item(2, "Save"), nested]),
                MenuNode::submenu(4, "View", {
                    let mut gone = MenuNode::item(6, "Gone");
                    gone.enabled = false;
                    vec![gone]
                }),
            ],
            app_menu: vec![MenuNode::item(30, "Preferences")],
        };
        let found = entries(&menus, "Gallery", false);
        let paths: Vec<String> = found.iter().map(|e| e.path.join("/")).collect();
        assert_eq!(
            paths,
            ["Gallery/Preferences", "File/Save", "File/Recent/One"]
        );
        assert_eq!(
            found[1].target,
            MenuEvent {
                generation: 4,
                id: 2,
                event: ItemEvent::Clicked
            }
        );
    }

    #[test]
    fn shortcuts_map_modifiers() {
        assert_eq!(shortcut(&[], false), None);
        assert_eq!(shortcut(&["F5".into()], false), Some(Shortcut::new("F5")));
        assert_eq!(
            shortcut(&["Super".into(), "Alt".into(), "Q".into()], false),
            Some(Shortcut::command("Q").alt())
        );
        assert_eq!(
            shortcut(&["Control".into(), "Shift".into(), "S".into()], false),
            Some(Shortcut::new("S").ctrl().shift())
        );
    }

    #[test]
    fn control_shows_as_command_where_super_is_sent_as_control() {
        let keys = |names: &[&str]| names.iter().map(|n| (*n).into()).collect::<Vec<String>>();
        assert_eq!(
            shortcut(&keys(&["Control", "C"]), true),
            Some(Shortcut::command("C"))
        );
        assert_eq!(
            shortcut(&keys(&["Control", "Shift", "S"]), true),
            Some(Shortcut::command("S").shift())
        );
        // Super and Control are the same key to such an app: one ⌘.
        assert_eq!(
            shortcut(&keys(&["Super", "Control", "Q"]), true),
            Some(Shortcut::command("Q"))
        );
        // Others are untouched.
        assert_eq!(
            shortcut(&keys(&["Alt", "F4"]), true),
            Some(Shortcut::new("F4").alt())
        );
        assert_eq!(shortcut(&keys(&["F5"]), true), Some(Shortcut::new("F5")));
    }

    #[test]
    fn the_menu_shows_command_for_remapped_apps() {
        let save = menu(&tree()[0].children, 3, true, &|e| e);
        assert_eq!(
            save.items()[0].shortcut_keys(),
            Some(&Shortcut::command("S").shift())
        );
    }

    #[test]
    fn addresses_come_from_dbusmenu_windows_only() {
        let dbus = ToplevelMenu::DbusMenu {
            service: ":1.4".into(),
            path: "/MenuBar/1".into(),
        };
        assert_eq!(
            Address::of(&dbus),
            Some(Address::DbusMenu {
                service: ":1.4".into(),
                path: "/MenuBar/1".into()
            })
        );
        assert_eq!(Address::of(&ToplevelMenu::None), None);
    }

    #[test]
    fn addresses_come_from_gmenu_windows_too() {
        let gmenu = ToplevelMenu::GMenu {
            bus_name: ":1.5".into(),
            menubar_path: Some("/app/menubar".into()),
            app_menu_path: None,
            application_path: Some("/app".into()),
            window_path: Some("/app/window/1".into()),
        };
        assert_eq!(
            Address::of(&gmenu),
            Some(Address::GMenu(GMenuAddress {
                bus_name: ":1.5".into(),
                menubar_path: Some("/app/menubar".into()),
                app_menu_path: None,
                application_path: Some("/app".into()),
                window_path: Some("/app/window/1".into()),
            }))
        );
    }

    #[test]
    fn changes_match_the_followed_menu() {
        let (_service, client) = neue_dbus::testing::pair().unwrap();
        let followed = |service: &str| Followed {
            generation: 1,
            source: Some(Source::Dbus(
                MenuClient::new(&client, service, "/m").unwrap(),
            )),
        };
        let change = |sender: &str, path: &str| MenuChanged {
            sender: sender.into(),
            path: path.into(),
        };
        assert!(followed(":1.4").is_changed_by(&change(":1.4", "/m")));
        assert!(!followed(":1.4").is_changed_by(&change(":1.5", "/m")));
        assert!(!followed(":1.4").is_changed_by(&change(":1.4", "/n")));
        assert!(followed("org.app").is_changed_by(&change(":1.5", "/m")));
    }
}
