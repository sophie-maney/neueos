//! GTK's menu protocols, read as the shell's menu bar needs them (task 3.8, ADR 0078):
//! `org.gtk.Menus` (the menu model: items with attributes, sections and submenus linking
//! to other menus) and `org.gtk.Actions` (what the items' `action` names mean: enabled,
//! state, activation).
//!
//! A [`GMenuClient`] follows one window's [`GMenuAddress`]. [`GMenuClient::fetch`] reads
//! the menu bar and the (GTK 3) app menu into the same [`MenuNode`] trees the DBusMenu side
//! produces, so the bar draws both alike; items get sequential ids, and [`GMenus`] says
//! which action each one runs. [`GMenuClient::activate`] runs it. [`GMenuSignals`] reports
//! that some menu or action list changed, and the caller fetches again.
//!
//! The exporter only sends `Changed` for groups a client has `Start`ed, so the client
//! stays subscribed between fetches. A fetch asks for the groups again and only then
//! releases the old subscription, which keeps the count above zero throughout.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::message::Type as MessageType;
use zbus::names::BusName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Signature, Structure, Value};

use crate::menu::{MenuNode, Toggle, ToggleState, unescape_label};
use crate::{Error, MenuChanged};

const MENUS: &str = "org.gtk.Menus";
const ACTIONS: &str = "org.gtk.Actions";

/// Rounds of `Start` for groups that menus link to (submenus are groups of their own), and
/// how many groups one round asks for: bounds on what a huge or hostile app can make the
/// shell read.
const GROUP_ROUNDS: usize = 8;
const GROUP_LIMIT: usize = 64;

/// How deep links between menus are followed, and how many items one menu bar may have.
const DEPTH_LIMIT: usize = 12;
const ITEM_LIMIT: i32 = 5_000;

/// What a menu item is: `a{sv}`.
type Item = HashMap<String, OwnedValue>;
/// `Start`'s answer: `(group, menu, items)` for every menu of the groups asked for.
type Content = Vec<(u32, u32, Vec<Item>)>;
/// `DescribeAll`'s answer: action name to `(enabled, parameter type, state)`.
type Described = HashMap<String, (bool, Signature, Vec<OwnedValue>)>;

/// Where a window's menus are exported (the compositor's `menu_gmenu`, ADR 0025). Each
/// path is optional; the shell only ever asks for ones that were given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GMenuAddress {
    /// The exporting connection.
    pub bus_name: String,
    /// `org.gtk.Menus` of the menu bar.
    pub menubar_path: Option<String>,
    /// `org.gtk.Menus` of the app menu (GTK 3, GNOME style: About, Preferences, Quit).
    pub app_menu_path: Option<String>,
    /// `org.gtk.Actions` of the application (the `app.` actions).
    pub application_path: Option<String>,
    /// `org.gtk.Actions` of the window (the `win.` actions).
    pub window_path: Option<String>,
}

/// Which action group an action belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `app.name`, at the application path.
    App,
    /// `win.name`, at the window path.
    Window,
}

/// What choosing an item does: activate `name` of `scope` with `parameter`.
#[derive(Debug)]
pub struct Activation {
    pub scope: Scope,
    /// The action's name without its `app.`/`win.` prefix.
    pub name: String,
    /// The item's `target`, for items that have one (radio items and the like); toggles
    /// take none.
    pub parameter: Option<OwnedValue>,
}

/// What [`GMenuClient::fetch`] read.
#[derive(Debug, Default)]
pub struct GMenus {
    /// The menu bar's titles with everything under them; empty without a menu bar.
    pub titles: Vec<MenuNode>,
    /// The items of the app menu; empty without one.
    pub app_menu: Vec<MenuNode>,
    /// What each item id (in `titles` and `app_menu`) activates. Separators, submenus and
    /// items without an action have no entry.
    pub activations: HashMap<i32, Activation>,
}

/// One action as `DescribeAll` gives it.
#[derive(Debug)]
struct ActionInfo {
    enabled: bool,
    parameter: Signature,
    state: Option<OwnedValue>,
}

/// The menus and actions of one window.
#[derive(Debug)]
pub struct GMenuClient {
    conn: Connection,
    address: GMenuAddress,
    /// The groups still subscribed to, per menu object path.
    subscribed: Mutex<Vec<(String, Vec<u32>)>>,
}

impl GMenuClient {
    /// A client for `address`. Nothing is sent yet; the paths are checked.
    pub fn new(conn: &Connection, address: GMenuAddress) -> Result<Self, Error> {
        BusName::try_from(address.bus_name.as_str())
            .map_err(|_| Error::Name(address.bus_name.clone()))?;
        for path in address.paths() {
            ObjectPath::try_from(path).map_err(|_| Error::Path(path.to_owned()))?;
        }
        Ok(Self {
            conn: conn.clone(),
            address,
            subscribed: Mutex::new(Vec::new()),
        })
    }

    /// The address being followed.
    pub fn address(&self) -> &GMenuAddress {
        &self.address
    }

    /// Whether `change` is a signal from this window's menus or actions. Signals always
    /// come from a unique name; an address with a well-known name matches on the path.
    pub fn is_changed_by(&self, change: &MenuChanged) -> bool {
        let bus = &self.address.bus_name;
        self.address.paths().any(|p| p == change.path)
            && (*bus == change.sender || !bus.starts_with(':'))
    }

    /// Reads the menu bar, the app menu and the actions they use.
    pub fn fetch(&self) -> Result<GMenus, Error> {
        let app = self.describe(self.address.application_path.as_deref());
        let window = self.describe(self.address.window_path.as_deref());
        let mut held = Vec::new();
        let read = self.read_both(&mut held);
        // Hold on to what was subscribed even if reading failed, so Drop lets go of it.
        let old = std::mem::replace(
            &mut *self
                .subscribed
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
            held,
        );
        for (path, groups) in old {
            self.end(&path, &groups);
        }
        let (bar, app_menu) = read?;
        let mut builder = Builder {
            actions: Actions {
                app: &app,
                window: &window,
            },
            next_id: 1,
            activations: HashMap::new(),
        };
        let titles = builder.menu(&bar, (0, 0), 0);
        let app_menu = builder.menu(&app_menu, (0, 0), 0);
        Ok(GMenus {
            titles,
            app_menu,
            activations: builder.activations,
        })
    }

    /// Runs `activation`.
    pub fn activate(&self, activation: &Activation) -> Result<(), Error> {
        let path = match activation.scope {
            Scope::App => self.address.application_path.as_deref(),
            Scope::Window => self.address.window_path.as_deref(),
        }
        .ok_or_else(|| Error::Path(format!("(no {:?} actions)", activation.scope)))?;
        let parameter: Vec<&Value<'_>> = activation.parameter.iter().map(|v| &**v).collect();
        let platform_data: HashMap<&str, Value<'_>> = HashMap::new();
        self.proxy(path, ACTIONS)?.call::<_, _, ()>(
            "Activate",
            &(activation.name.as_str(), parameter, platform_data),
        )?;
        Ok(())
    }

    fn proxy(&self, path: &str, interface: &'static str) -> Result<Proxy<'static>, Error> {
        let service = BusName::try_from(self.address.bus_name.clone())
            .map_err(|_| Error::Name(self.address.bus_name.clone()))?;
        let path: OwnedObjectPath = ObjectPath::try_from(path.to_owned())
            .map_err(|_| Error::Path(path.to_owned()))?
            .into();
        Ok(
            zbus::blocking::proxy::Builder::<Proxy<'static>>::new(&self.conn)
                .destination(service)?
                .path(path)?
                .interface(interface)?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()?,
        )
    }

    /// `DescribeAll` of the actions at `path`; none if there is no path or the call fails
    /// (the items are then shown disabled).
    fn describe(&self, path: Option<&str>) -> HashMap<String, ActionInfo> {
        let Some(path) = path else {
            return HashMap::new();
        };
        let described = self
            .proxy(path, ACTIONS)
            .and_then(|p| Ok(p.call::<_, _, Described>("DescribeAll", &())?));
        match described {
            Ok(all) => all
                .into_iter()
                .map(|(name, (enabled, parameter, mut state))| {
                    let state = (!state.is_empty()).then(|| state.swap_remove(0));
                    (
                        name,
                        ActionInfo {
                            enabled,
                            parameter,
                            state,
                        },
                    )
                })
                .collect(),
            Err(err) => {
                tracing::debug!(%err, path, "can't read the app's actions");
                HashMap::new()
            }
        }
    }

    /// The menu bar's and the app menu's menus.
    fn read_both(&self, held: &mut Vec<(String, Vec<u32>)>) -> Result<(Menus, Menus), Error> {
        let bar = self.read(self.address.menubar_path.as_deref(), held)?;
        let app_menu = self.read(self.address.app_menu_path.as_deref(), held)?;
        Ok((bar, app_menu))
    }

    /// Subscribes to the menu at `path` and to every group it links to, and returns all
    /// its menus. What was subscribed to is added to `held` as it happens.
    fn read(&self, path: Option<&str>, held: &mut Vec<(String, Vec<u32>)>) -> Result<Menus, Error> {
        let Some(path) = path else {
            return Ok(Menus::new());
        };
        let proxy = self.proxy(path, MENUS)?;
        held.push((path.to_owned(), Vec::new()));
        let mut menus = Menus::new();
        let mut asked = HashSet::new();
        let mut wanted = vec![0u32];
        for _ in 0..GROUP_ROUNDS {
            wanted.retain(|g| asked.insert(*g));
            wanted.truncate(GROUP_LIMIT);
            if wanted.is_empty() {
                break;
            }
            let content: Content = proxy.call("Start", &(&wanted,))?;
            if let Some((_, groups)) = held.last_mut() {
                groups.extend(&wanted);
            }
            for (group, menu, items) in content {
                menus.insert((group, menu), items);
            }
            wanted = menus
                .values()
                .flatten()
                .flat_map(|item| [link(item, ":section"), link(item, ":submenu")])
                .flatten()
                .map(|(group, _)| group)
                .filter(|g| !asked.contains(g))
                .collect();
        }
        Ok(menus)
    }

    /// `End` for `groups` of the menu at `path`; the exporter forgets us anyway when we
    /// disconnect, so failures only get logged.
    fn end(&self, path: &str, groups: &[u32]) {
        if groups.is_empty() {
            return;
        }
        let ended = self
            .proxy(path, MENUS)
            .and_then(|p| Ok(p.call::<_, _, ()>("End", &(groups,))?));
        if let Err(err) = ended {
            tracing::debug!(%err, path, "org.gtk.Menus.End failed");
        }
    }
}

impl Drop for GMenuClient {
    fn drop(&mut self) {
        let held = std::mem::take(
            &mut *self
                .subscribed
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for (path, groups) in held {
            self.end(&path, &groups);
        }
    }
}

impl GMenuAddress {
    fn paths(&self) -> impl Iterator<Item = &str> {
        [
            &self.menubar_path,
            &self.app_menu_path,
            &self.application_path,
            &self.window_path,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
    }
}

/// The menus of one object, by `(group, menu)`.
type Menus = HashMap<(u32, u32), Vec<Item>>;

/// `(group, menu)` of a `:section` or `:submenu` link.
fn link(item: &Item, name: &str) -> Option<(u32, u32)> {
    let structure = <&Structure>::try_from(&**item.get(name)?).ok()?;
    match structure.fields() {
        [group, menu] => Some((u32::try_from(group).ok()?, u32::try_from(menu).ok()?)),
        _ => None,
    }
}

fn text<'a>(item: &'a Item, name: &str) -> Option<&'a str> {
    item.get(name).and_then(|v| <&str>::try_from(&**v).ok())
}

/// The two action groups, looked up by full name (`app.quit`).
struct Actions<'a> {
    app: &'a HashMap<String, ActionInfo>,
    window: &'a HashMap<String, ActionInfo>,
}

impl Actions<'_> {
    /// The scope and bare name of `full`, and the action's description if it has one.
    fn find<'n>(&self, full: &'n str) -> Option<(Scope, &'n str, Option<&ActionInfo>)> {
        if let Some(name) = full.strip_prefix("app.") {
            Some((Scope::App, name, self.app.get(name)))
        } else {
            let name = full.strip_prefix("win.")?;
            Some((Scope::Window, name, self.window.get(name)))
        }
    }
}

/// Turns menus into [`MenuNode`]s.
struct Builder<'a> {
    actions: Actions<'a>,
    next_id: i32,
    activations: HashMap<i32, Activation>,
}

impl Builder<'_> {
    /// The items of `menu`. Sections are flattened with a separator between them, and
    /// links that don't lead anywhere (or too deep) give nothing.
    fn menu(&mut self, menus: &Menus, menu: (u32, u32), depth: usize) -> Vec<MenuNode> {
        let Some(items) = menus.get(&menu) else {
            return Vec::new();
        };
        if depth > DEPTH_LIMIT {
            return Vec::new();
        }
        // Runs of plain items, and sections, are blocks; separators go between blocks.
        let mut blocks: Vec<Vec<MenuNode>> = Vec::new();
        let mut run = Vec::new();
        for item in items {
            if self.next_id > ITEM_LIMIT {
                break;
            }
            if let Some(section) = link(item, ":section") {
                if !run.is_empty() {
                    blocks.push(std::mem::take(&mut run));
                }
                blocks.push(self.menu(menus, section, depth + 1));
            } else if let Some(node) = self.item(menus, item, depth) {
                run.push(node);
            }
        }
        blocks.push(run);
        let mut out = Vec::new();
        for block in blocks.into_iter().filter(|b| !b.is_empty()) {
            if !out.is_empty() {
                out.push(MenuNode::separator(self.id()));
            }
            out.extend(block);
        }
        out
    }

    fn id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A labelled item, submenu or action; `None` if it is hidden.
    fn item(&mut self, menus: &Menus, item: &Item, depth: usize) -> Option<MenuNode> {
        let label = text(item, "label").map(unescape_label).unwrap_or_default();
        if let Some(submenu) = link(item, ":submenu") {
            let id = self.id();
            let children = self.menu(menus, submenu, depth + 1);
            return Some(MenuNode::submenu(id, label, children));
        }
        let id = self.id();
        let mut node = MenuNode::item(id, label);
        node.shortcut = text(item, "accel").map(accel_keys).unwrap_or_default();
        let Some(full) = text(item, "action") else {
            return Some(node);
        };
        let found = self.actions.find(full);
        let hidden_when = text(item, "hidden-when");
        let usable = found
            .and_then(|(_, _, info)| info)
            .is_some_and(|i| i.enabled);
        let present = found.and_then(|(_, _, info)| info).is_some();
        match hidden_when {
            Some("action-disabled") if !usable => return None,
            Some("action-missing") if !present => return None,
            _ => {}
        }
        node.enabled = usable;
        if let Some((scope, name, info)) = found {
            let target = item.get("target").and_then(|v| v.try_clone().ok());
            node.toggle = match (info, &target) {
                (Some(info), Some(target)) => match &info.state {
                    Some(state) => Toggle::Radio(on(**state == **target)),
                    None => Toggle::None,
                },
                (Some(info), None) => match info.state.as_deref().map(<bool>::try_from) {
                    Some(Ok(state)) => Toggle::Check(on(state)),
                    _ => Toggle::None,
                },
                (None, _) => Toggle::None,
            };
            // A parameterless action takes none, whatever the item says.
            let takes_none = info.is_some_and(|i| matches!(i.parameter, Signature::Unit));
            self.activations.insert(
                id,
                Activation {
                    scope,
                    name: name.to_owned(),
                    parameter: if takes_none { None } else { target },
                },
            );
        }
        Some(node)
    }
}

fn on(state: bool) -> ToggleState {
    if state {
        ToggleState::On
    } else {
        ToggleState::Off
    }
}

/// GTK's accelerator (`<Primary><Shift>s`) as DBusMenu shortcut keys (`Control`, `Shift`,
/// `S`). Empty if there's no key.
fn accel_keys(accel: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = accel.trim();
    while let Some(after) = rest.strip_prefix('<') {
        let Some((name, tail)) = after.split_once('>') else {
            return Vec::new();
        };
        let modifier = match name.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" => "Control",
            "alt" | "mod1" => "Alt",
            "shift" => "Shift",
            "super" | "meta" | "mod4" => "Super",
            _ => return Vec::new(),
        };
        if !keys.iter().any(|k| k == modifier) {
            keys.push(modifier.to_owned());
        }
        rest = tail;
    }
    let key = match rest {
        "" => return Vec::new(),
        "plus" => "+".to_owned(),
        "minus" => "-".to_owned(),
        "comma" => ",".to_owned(),
        "period" => ".".to_owned(),
        "slash" => "/".to_owned(),
        "space" => "Space".to_owned(),
        key if key.chars().count() == 1 => key.to_uppercase(),
        key => key.to_owned(),
    };
    keys.push(key);
    keys
}

/// Every `Changed` signal of GTK's menus and actions on `conn`, from any app. Blocking:
/// iterate it on a thread of its own. It ends when the connection closes.
pub struct GMenuSignals {
    messages: MessageIterator,
}

impl std::fmt::Debug for GMenuSignals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GMenuSignals").finish_non_exhaustive()
    }
}

impl GMenuSignals {
    /// Subscribes to `Changed` signals (a match rule on the member: the rule can't name
    /// two interfaces; others' `Changed` signals are skipped by [`Iterator::next`]).
    pub fn new(conn: &Connection) -> Result<Self, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .member("Changed")?
            .build();
        let messages = MessageIterator::for_match_rule(rule, conn, Some(64))?;
        Ok(Self { messages })
    }
}

impl Iterator for GMenuSignals {
    type Item = MenuChanged;

    fn next(&mut self) -> Option<MenuChanged> {
        loop {
            let message = match self.messages.next()? {
                Ok(message) => message,
                Err(err) => {
                    tracing::debug!(%err, "bad message on the GMenu signal stream");
                    continue;
                }
            };
            let header = message.header();
            let interface = header.interface().map(|i| i.as_str());
            if !matches!(interface, Some(MENUS | ACTIONS)) {
                continue;
            }
            let Some(path) = header.path() else {
                continue;
            };
            return Some(MenuChanged {
                sender: header.sender().map(ToString::to_string).unwrap_or_default(),
                path: path.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerators_become_shortcut_keys() {
        assert_eq!(accel_keys("<Primary>q"), ["Control", "Q"]);
        assert_eq!(accel_keys("<Shift><Control>s"), ["Shift", "Control", "S"]);
        assert_eq!(accel_keys("<Alt>Left"), ["Alt", "Left"]);
        assert_eq!(accel_keys("F1"), ["F1"]);
        assert_eq!(accel_keys("<Primary>plus"), ["Control", "+"]);
        assert!(accel_keys("").is_empty());
        assert!(accel_keys("<Primary>").is_empty());
        assert!(accel_keys("<Bogus>x").is_empty());
        assert!(accel_keys("<Primary").is_empty());
    }
}
