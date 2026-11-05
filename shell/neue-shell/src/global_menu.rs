//! The global menu's DBusMenu side (task 3.7a, ADR 0005, ADR 0076): a worker thread that
//! owns `com.canonical.AppMenu.Registrar`, follows the active window's exported menus and
//! passes on what the user does with them, and the conversion of those menus into the
//! bar's titles.
//!
//! The UI thread sends [`Input`]s; the worker sends back [`Menus`] (through the kit's task
//! sender). Every followed menu gets a new generation, and events carry the generation of
//! the menus they came from, so a click on a title that was just replaced never reaches
//! the wrong app.

use std::sync::mpsc;
use std::time::Duration;

use neue_dbus::zbus::blocking::Connection;
use neue_dbus::{MenuChanged, MenuClient, MenuNode, MenuSignals, NodeKind, Registrar};
use neue_dbus::{Toggle, ToggleState};
use neue_kit::desktop::{MenuTitle, ToplevelMenu};
use neue_kit::{CheckState, Menu, MenuItem, Shortcut};

/// How long the shell waits for an app to answer before giving up on a call.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

/// A DBusMenu address: the exporting connection and the object path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub service: String,
    pub path: String,
}

impl Address {
    /// The DBusMenu address of a window's menus, if they are one.
    pub fn of(menu: &ToplevelMenu) -> Option<Self> {
        match menu {
            ToplevelMenu::DbusMenu { service, path } => Some(Self {
                service: service.clone(),
                path: path.clone(),
            }),
            // GMenuModel menus are task 3.8.
            ToplevelMenu::GMenu { .. } | ToplevelMenu::None => None,
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
    /// Follow these menus from now on (`None`: the active window has no DBusMenu).
    Follow(Option<Address>),
    Event(MenuEvent),
    /// A menu on the bus changed (from the signal thread).
    Changed(MenuChanged),
}

/// The app's menus as last read: the menu-bar titles and everything under them. Empty when
/// the window has none or they couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menus {
    pub generation: u64,
    pub titles: Vec<MenuNode>,
}

/// The menus being followed.
struct Followed {
    generation: u64,
    client: Option<MenuClient>,
}

impl Followed {
    /// Whether `change` is about the followed menu. Signals always come from a unique name;
    /// an address with a well-known name is matched on the path alone.
    fn is_changed_by(&self, change: &MenuChanged) -> bool {
        self.client.as_ref().is_some_and(|c| {
            c.path() == change.path
                && (c.service() == change.sender || !c.service().starts_with(':'))
        })
    }

    /// Handles `input`; true if the menus must be read again.
    fn handle(&mut self, conn: &Connection, input: Input) -> bool {
        match input {
            Input::Follow(address) => {
                self.generation += 1;
                self.client = address.and_then(|a| {
                    MenuClient::new(conn, &a.service, &a.path)
                        .map_err(|err| tracing::warn!(%err, ?a, "bad menu address"))
                        .ok()
                });
                true
            }
            Input::Changed(change) => self.is_changed_by(&change),
            Input::Event(event) if event.generation == self.generation => {
                let Some(client) = &self.client else {
                    return false;
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

    fn read(&self) -> Menus {
        let titles = self.client.as_ref().map_or_else(Vec::new, |client| {
            client.fetch().unwrap_or_else(|err| {
                tracing::warn!(%err, service = client.service(), "can't read the app's menus");
                Vec::new()
            })
        });
        Menus {
            generation: self.generation,
            titles,
        }
    }
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
    std::thread::Builder::new()
        .name("dbusmenu-signals".into())
        .spawn(move || {
            for change in signals {
                if tx.send(Input::Changed(change)).is_err() {
                    return;
                }
            }
        })
        .map_err(|err| neue_dbus::Error::Dbus(err.into()))?;
    let mut followed = Followed {
        generation: 0,
        client: None,
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

/// The bar's titles for `titles` of generation `generation`: `message(event)` for each
/// item chosen and each title opened or closed. Separators and items without a menu at the
/// top level are left out (the bar has no room for buttons).
pub fn bar_titles<M: Clone + 'static>(
    titles: &[MenuNode],
    generation: u64,
    message: impl Fn(MenuEvent) -> M + Clone + 'static,
) -> Vec<MenuTitle<M>> {
    let event = |id, event| MenuEvent {
        generation,
        id,
        event,
    };
    titles
        .iter()
        .filter(|t| t.kind == NodeKind::Item && !t.children.is_empty())
        .map(|t| {
            MenuTitle::new(t.label.clone(), menu(&t.children, generation, &message))
                .on_open(message(event(t.id, ItemEvent::Opened)))
                .on_close(message(event(t.id, ItemEvent::Closed)))
        })
        .collect()
}

/// A kit menu for `items`.
fn menu<M: 'static>(
    items: &[MenuNode],
    generation: u64,
    message: &(impl Fn(MenuEvent) -> M + Clone + 'static),
) -> Menu<M> {
    items.iter().fold(Menu::new(), |built, node| {
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
                menu(&node.children, generation, message),
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
        let item = match shortcut(&node.shortcut) {
            Some(keys) => item.shortcut(keys),
            None => item,
        };
        // Icons are read but not drawn: the bar's menus show none (ADR 0076).
        built.with(item.disabled(!node.enabled))
    })
}

/// A DBusMenu shortcut (`["Control", "Shift", "S"]`) as the kit shows it.
fn shortcut(keys: &[String]) -> Option<Shortcut> {
    let (key, modifiers) = keys.split_last()?;
    let mut shortcut = if modifiers.iter().any(|m| m == "Super") {
        Shortcut::command(key.clone())
    } else {
        Shortcut::new(key.clone())
    };
    for modifier in modifiers {
        shortcut = match modifier.as_str() {
            "Control" => shortcut.ctrl(),
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
        let titles = bar_titles(&tree(), 3, |e| e);
        assert_eq!(titles.len(), 2);
        let file = menu(&tree()[0].children, 3, &|e| e);
        let save = &file.items()[0];
        assert_eq!(save.label(), "Save");
        assert_eq!(
            save.shortcut_keys(),
            Some(&Shortcut::new("S").ctrl().shift())
        );
        assert!(file.items()[1].is_separator());
        let view = menu(&tree()[1].children, 3, &|e| e);
        assert_eq!(
            view.items()[0].toggle(),
            neue_kit::MenuToggle::Check(CheckState::On)
        );
        assert!(!view.items()[1].is_enabled());
    }

    #[test]
    fn shortcuts_map_modifiers() {
        assert_eq!(shortcut(&[]), None);
        assert_eq!(shortcut(&["F5".into()]), Some(Shortcut::new("F5")));
        assert_eq!(
            shortcut(&["Super".into(), "Alt".into(), "Q".into()]),
            Some(Shortcut::command("Q").alt())
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
            Some(Address {
                service: ":1.4".into(),
                path: "/MenuBar/1".into()
            })
        );
        assert_eq!(Address::of(&ToplevelMenu::None), None);
    }

    #[test]
    fn changes_match_the_followed_menu() {
        let (_service, client) = neue_dbus::testing::pair().unwrap();
        let followed = |service: &str| Followed {
            generation: 1,
            client: Some(MenuClient::new(&client, service, "/m").unwrap()),
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
