//! The menu model (task 2.17a, ADR 0045): what a menu holds, as data plus the messages its
//! items send. Context menus show it now; the menu bar (DBusMenu export, 2.18) and pop-up
//! buttons (2.17b) show the same model.
//!
//! ```no_run
//! # use neue_kit::{CheckState, Menu, MenuItem, Shortcut};
//! # #[derive(Clone)] enum Msg { Copy, Paste, Wrap, Size(u8) }
//! let menu: Menu<Msg> = Menu::new()
//!     .with(MenuItem::new("Copy").shortcut(Shortcut::command("C")).on_choose(Msg::Copy))
//!     .with(MenuItem::new("Paste").shortcut(Shortcut::command("V")).on_choose(Msg::Paste))
//!     .separator()
//!     .with(MenuItem::new("Wrap Lines").checked(true).on_choose(Msg::Wrap))
//!     .with(MenuItem::submenu(
//!         "Text Size",
//!         Menu::new()
//!             .with(MenuItem::new("Small").radio(false).on_choose(Msg::Size(0)))
//!             .with(MenuItem::new("Large").radio(true).on_choose(Msg::Size(1))),
//!     ));
//! ```

use std::cell::Cell;
use std::sync::Arc;

use crate::CheckState;

thread_local! {
    /// Overrides which key the command key is on this thread (goldens draw the same
    /// shortcuts on every host).
    static COMMAND_IS_CTRL: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Makes the command key Ctrl on this thread, whatever the host.
#[cfg(feature = "testing")]
pub(crate) fn command_is_ctrl_here() {
    COMMAND_IS_CTRL.with(|c| c.set(Some(true)));
}

fn command_is_ctrl() -> bool {
    COMMAND_IS_CTRL
        .with(Cell::get)
        .unwrap_or(!cfg!(target_os = "macos"))
}

/// A list of items, separators and submenus.
pub struct Menu<M> {
    items: Vec<MenuItem<M>>,
}

impl<M> Default for Menu<M> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<M> std::fmt::Debug for Menu<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.items).finish()
    }
}

impl<M> Menu<M> {
    /// An empty menu.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `item` at the end.
    #[must_use]
    pub fn with(mut self, item: MenuItem<M>) -> Self {
        self.items.push(item);
        self
    }

    /// Adds a separator line at the end.
    #[must_use]
    pub fn separator(self) -> Self {
        self.with(MenuItem::separator())
    }

    /// The items, in order.
    pub fn items(&self) -> &[MenuItem<M>] {
        &self.items
    }

    /// Whether the menu has no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Splits the menu into what its widgets show and what its items send. Every item in
    /// the tree, separators and submenus included, gets an id: its place in a depth-first
    /// walk, starting at 0. `handlers[id]` makes the message of the item `id`.
    pub(crate) fn into_parts(self) -> (MenuSpec, Vec<Option<ItemHandler<M>>>) {
        let mut handlers = Vec::new();
        let spec = self.flatten(&mut handlers);
        (spec, handlers)
    }

    fn flatten(self, handlers: &mut Vec<Option<ItemHandler<M>>>) -> MenuSpec {
        let mut specs = Vec::with_capacity(self.items.len());
        for item in self.items {
            let id = handlers.len();
            let MenuItem {
                label,
                kind,
                shortcut,
                enabled,
                toggle,
            } = item;
            let kind = match kind {
                Kind::Action(handler) => {
                    handlers.push(handler);
                    SpecKind::Action
                }
                Kind::Separator => {
                    handlers.push(None);
                    SpecKind::Separator
                }
                Kind::Submenu(menu) => {
                    handlers.push(None);
                    SpecKind::Submenu(menu.flatten(handlers))
                }
            };
            specs.push(ItemSpec {
                id,
                label,
                shortcut,
                enabled,
                toggle,
                kind,
            });
        }
        MenuSpec(specs.into())
    }
}

/// Makes the message an item sends.
pub(crate) type ItemHandler<M> = Box<dyn FnMut() -> M>;

/// One row of a [`Menu`]: an item that sends a message, a submenu, or a separator.
pub struct MenuItem<M> {
    label: String,
    kind: Kind<M>,
    shortcut: Option<Shortcut>,
    enabled: bool,
    toggle: MenuToggle,
}

enum Kind<M> {
    Action(Option<ItemHandler<M>>),
    Submenu(Menu<M>),
    Separator,
}

impl<M> std::fmt::Debug for MenuItem<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("MenuItem");
        match &self.kind {
            Kind::Action(handler) => d
                .field("label", &self.label)
                .field("on_choose", &handler.is_some()),
            Kind::Submenu(menu) => d.field("label", &self.label).field("submenu", menu),
            Kind::Separator => return f.write_str("Separator"),
        };
        d.field("shortcut", &self.shortcut)
            .field("enabled", &self.enabled)
            .field("toggle", &self.toggle)
            .finish()
    }
}

impl<M> MenuItem<M> {
    /// An item titled `label`. It sends nothing until [`on_choose`](Self::on_choose).
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: Kind::Action(None),
            shortcut: None,
            enabled: true,
            toggle: MenuToggle::None,
        }
    }

    /// An item titled `label` that opens `menu` beside it.
    pub fn submenu(label: impl Into<String>, menu: Menu<M>) -> Self {
        Self {
            kind: Kind::Submenu(menu),
            ..Self::new(label)
        }
    }

    /// A separator line between groups of items.
    pub fn separator() -> Self {
        Self {
            kind: Kind::Separator,
            ..Self::new("")
        }
    }

    /// Sends `message` when the item is chosen.
    #[must_use]
    pub fn on_choose(self, message: M) -> Self
    where
        M: Clone + 'static,
    {
        self.on_choose_with(move || message.clone())
    }

    /// Sends the message `make` returns when the item is chosen.
    #[must_use]
    pub fn on_choose_with(mut self, make: impl FnMut() -> M + 'static) -> Self {
        if let Kind::Action(handler) = &mut self.kind {
            *handler = Some(Box::new(make));
        }
        self
    }

    /// Shows `shortcut` at the item's right. It only labels the item: the key does what
    /// the app binds it to.
    #[must_use]
    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Greys the item out; it can't be chosen.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.enabled = !disabled;
        self
    }

    /// A check mark before the label while `checked`.
    #[must_use]
    pub fn checked(self, checked: bool) -> Self {
        self.state(if checked {
            CheckState::On
        } else {
            CheckState::Off
        })
    }

    /// A check mark (on), a dash (mixed) or nothing (off) before the label.
    #[must_use]
    pub fn state(mut self, state: CheckState) -> Self {
        self.toggle = MenuToggle::Check(state);
        self
    }

    /// One item of a radio group: a dot before the label while `selected`. The app keeps
    /// one item of the group selected.
    #[must_use]
    pub fn radio(mut self, selected: bool) -> Self {
        self.toggle = MenuToggle::Radio(selected);
        self
    }

    /// The label (empty for separators).
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether this is a separator line.
    pub fn is_separator(&self) -> bool {
        matches!(self.kind, Kind::Separator)
    }

    /// The menu this item opens, if it is a submenu.
    pub fn child_menu(&self) -> Option<&Menu<M>> {
        match &self.kind {
            Kind::Submenu(menu) => Some(menu),
            _ => None,
        }
    }

    /// The shortcut shown at the right.
    pub fn shortcut_keys(&self) -> Option<&Shortcut> {
        self.shortcut.as_ref()
    }

    /// Whether the item can be chosen.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Its check mark or radio dot.
    pub fn toggle(&self) -> MenuToggle {
        self.toggle
    }
}

/// An item's check mark or radio dot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MenuToggle {
    /// Neither: a plain item.
    #[default]
    None,
    /// A checkbox item: a check mark when on, a dash when mixed.
    Check(CheckState),
    /// One item of a radio group: a dot when selected.
    Radio(bool),
}

/// The keys shown beside a menu item: modifiers (as the glyphs ⌃ ⌥ ⇧ ⌘) and a key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Shortcut {
    key: String,
    command: bool,
    shift: bool,
    alt: bool,
    ctrl: bool,
}

impl Shortcut {
    /// `key` alone (`"F5"`, `"Delete"`). Single letters are shown upper-case.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ..Self::default()
        }
    }

    /// The command key and `key`. The command key is Cmd on macOS hosts and Ctrl
    /// elsewhere, as in text fields (ADR 0042).
    pub fn command(key: impl Into<String>) -> Self {
        Self {
            command: true,
            ..Self::new(key)
        }
    }

    /// Adds Shift.
    #[must_use]
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Adds Alt (Option).
    #[must_use]
    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Adds Ctrl (Control on macOS, where it isn't the command key).
    #[must_use]
    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// The key without modifiers.
    pub fn key(&self) -> &str {
        &self.key
    }

    fn key_shown(&self) -> String {
        let mut chars = self.key.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => c.to_uppercase().collect(),
            _ => self.key.clone(),
        }
    }

    /// Ctrl, Alt, Shift and the command key, in display order, as (glyph, name).
    fn modifiers(&self) -> Vec<(&'static str, &'static str)> {
        let ctrl = self.ctrl || (self.command && command_is_ctrl());
        let mut mods = Vec::new();
        if ctrl {
            mods.push(("⌃", "Ctrl"));
        }
        if self.alt {
            mods.push(("⌥", "Alt"));
        }
        if self.shift {
            mods.push(("⇧", "Shift"));
        }
        if self.command && !command_is_ctrl() {
            mods.push(("⌘", "Cmd"));
        }
        mods
    }

    /// What the menu shows: modifier glyphs, then the key (`⌃⇧S`).
    pub fn label(&self) -> String {
        let mut s: String = self.modifiers().into_iter().map(|(g, _)| g).collect();
        s.push_str(&self.key_shown());
        s
    }

    /// What assistive technologies read: names joined by `+` (`Ctrl+Shift+S`).
    pub fn spoken(&self) -> String {
        let mut parts: Vec<String> = self
            .modifiers()
            .into_iter()
            .map(|(_, n)| n.to_owned())
            .collect();
        parts.push(self.key_shown());
        parts.join("+")
    }
}

/// What a menu's widgets show: the items without their messages. Cheap to clone.
#[derive(Clone, Debug, Default)]
pub(crate) struct MenuSpec(pub(crate) Arc<[ItemSpec]>);

/// One row of a [`MenuSpec`].
#[derive(Clone, Debug)]
pub(crate) struct ItemSpec {
    /// The item's place in the whole menu tree (see [`Menu::into_parts`]).
    pub(crate) id: usize,
    pub(crate) label: String,
    pub(crate) shortcut: Option<Shortcut>,
    pub(crate) enabled: bool,
    pub(crate) toggle: MenuToggle,
    pub(crate) kind: SpecKind,
}

#[derive(Clone, Debug)]
pub(crate) enum SpecKind {
    Action,
    Submenu(MenuSpec),
    Separator,
}

impl ItemSpec {
    /// Whether the keyboard and the pointer can highlight it.
    pub(crate) fn selectable(&self) -> bool {
        self.enabled && !matches!(self.kind, SpecKind::Separator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_walk_the_tree_depth_first() {
        let menu: Menu<u8> = Menu::new()
            .with(MenuItem::new("A").on_choose(1))
            .separator()
            .with(MenuItem::submenu(
                "B",
                Menu::new().with(MenuItem::new("B1").on_choose(2)),
            ))
            .with(MenuItem::new("C").on_choose(3));
        let (spec, mut handlers) = menu.into_parts();
        let ids: Vec<usize> = spec.0.iter().map(|i| i.id).collect();
        assert_eq!(ids, [0, 1, 2, 4]);
        let SpecKind::Submenu(sub) = &spec.0[2].kind else {
            panic!("a submenu");
        };
        assert_eq!(sub.0[0].id, 3);
        let sent: Vec<Option<u8>> = handlers
            .iter_mut()
            .map(|h| h.as_mut().map(|make| make()))
            .collect();
        assert_eq!(sent, [Some(1), None, None, Some(2), Some(3)]);
    }

    #[test]
    fn shortcuts_show_glyphs_and_read_names() {
        let s = Shortcut::command("s").shift();
        if cfg!(target_os = "macos") {
            assert_eq!(s.label(), "⇧⌘S");
            assert_eq!(s.spoken(), "Shift+Cmd+S");
        } else {
            assert_eq!(s.label(), "⌃⇧S");
            assert_eq!(s.spoken(), "Ctrl+Shift+S");
        }
        assert_eq!(Shortcut::new("F5").alt().label(), "⌥F5");
    }

    #[test]
    fn separators_and_disabled_items_are_not_selectable() {
        let (spec, _) = Menu::<()>::new()
            .with(MenuItem::new("A"))
            .separator()
            .with(MenuItem::new("B").disabled(true))
            .into_parts();
        let selectable: Vec<bool> = spec.0.iter().map(ItemSpec::selectable).collect();
        assert_eq!(selectable, [true, false, false]);
    }
}
