//! Menus over D-Bus as `com.canonical.dbusmenu` (version 3), the interface Qt, Chromium and
//! LibreOffice export their menu bars with and global menus read (ADR 0005, ADR 0047).
//!
//! A [`MenuExport`] serves one tree of [`MenuNode`]s at an object path. Item 0 is the
//! invisible root; its children are the menu bar's titles, theirs the menus' items. Items
//! keep their ids across [`MenuExport::set_items`], and every change bumps the layout
//! revision and emits `LayoutUpdated(revision, 0)`, so clients just fetch the layout again.
//! When a client sends the `clicked` event for an enabled item without children, the
//! export's callback gets its id.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use zbus::blocking::Connection;
use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::signature::Fields;
use zbus::zvariant::{
    DeserializeValue, ObjectPath, OwnedObjectPath, OwnedValue, SerializeValue, Signature, Type,
    Value,
};

use crate::Error;

/// The D-Bus interface name.
pub const INTERFACE: &str = "com.canonical.dbusmenu";

/// Where menus are exported: `MENU_ROOT/<n>`, under the object manager
/// [`crate::connect_session`] serves.
pub const MENU_ROOT: &str = "/org/neueos/MenuBar";

/// The `Version` property: the interface revision we implement.
pub const VERSION: u32 = 3;

/// An item's properties, as `a{sv}`.
pub type Properties = HashMap<String, OwnedValue>;

/// One item in an exported menu tree: a menu-bar title, a menu item, a submenu or a
/// separator. An item with children is a submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuNode {
    /// Unique in the tree and not 0 (the root's). Keep it stable for the same item.
    pub id: i32,
    pub kind: NodeKind,
    /// Plain text; [`MenuNode::properties`] escapes it (DBusMenu reads `_` as a mnemonic
    /// marker).
    pub label: String,
    pub enabled: bool,
    pub toggle: Toggle,
    /// Modifier names (`Control`, `Alt`, `Shift`, `Super`) then the key (`S`, `F5`);
    /// empty for none.
    pub shortcut: Vec<String>,
    pub children: Vec<MenuNode>,
}

/// What a [`MenuNode`] is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A labelled item (or, with children, a submenu).
    #[default]
    Item,
    /// A separator line.
    Separator,
}

/// An item's check mark or radio dot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Toggle {
    #[default]
    None,
    /// `toggle-type` `checkmark`.
    Check(ToggleState),
    /// `toggle-type` `radio`.
    Radio(ToggleState),
}

/// `toggle-state`: 0 off, 1 on, -1 mixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToggleState {
    #[default]
    Off,
    On,
    Mixed,
}

impl ToggleState {
    fn wire(self) -> i32 {
        match self {
            Self::Off => 0,
            Self::On => 1,
            Self::Mixed => -1,
        }
    }
}

impl MenuNode {
    /// An enabled item with `label`.
    pub fn item(id: i32, label: impl Into<String>) -> Self {
        Self {
            id,
            kind: NodeKind::Item,
            label: label.into(),
            enabled: true,
            toggle: Toggle::None,
            shortcut: Vec::new(),
            children: Vec::new(),
        }
    }

    /// A submenu (or menu-bar title) holding `children`.
    pub fn submenu(id: i32, label: impl Into<String>, children: Vec<MenuNode>) -> Self {
        Self {
            children,
            ..Self::item(id, label)
        }
    }

    /// A separator.
    pub fn separator(id: i32) -> Self {
        Self {
            kind: NodeKind::Separator,
            ..Self::item(id, "")
        }
    }

    /// The invisible root (id 0) over `children`.
    fn root(children: Vec<MenuNode>) -> Self {
        Self::submenu(0, "", children)
    }

    /// Whether clicking it chooses it.
    fn clickable(&self) -> bool {
        self.enabled && self.kind == NodeKind::Item && self.children.is_empty()
    }

    fn find(&self, id: i32) -> Option<&MenuNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    fn visit<'a>(&'a self, out: &mut Vec<&'a MenuNode>) {
        out.push(self);
        for c in &self.children {
            c.visit(out);
        }
    }

    /// The item's DBusMenu properties named in `names` (all if empty). Properties at
    /// their default value are left out, as the specification allows.
    pub fn properties(&self, names: &[String]) -> Properties {
        let mut props = Properties::new();
        let mut put = |name: &str, value: Value<'static>| {
            if !names.is_empty() && !names.iter().any(|n| n == name) {
                return;
            }
            // Only values holding file descriptors fail to convert; these never do.
            if let Ok(value) = OwnedValue::try_from(value) {
                props.insert(name.to_owned(), value);
            }
        };
        match self.kind {
            NodeKind::Separator => put("type", Value::from("separator")),
            NodeKind::Item => put("label", Value::from(escape_label(&self.label))),
        }
        if !self.enabled {
            put("enabled", Value::from(false));
        }
        let toggle = match self.toggle {
            Toggle::None => None,
            Toggle::Check(state) => Some(("checkmark", state)),
            Toggle::Radio(state) => Some(("radio", state)),
        };
        if let Some((kind, state)) = toggle {
            put("toggle-type", Value::from(kind));
            put("toggle-state", Value::from(state.wire()));
        }
        if !self.shortcut.is_empty() {
            put("shortcut", Value::from(vec![self.shortcut.clone()]));
        }
        if !self.children.is_empty() {
            put("children-display", Value::from("submenu"));
        }
        props
    }

    /// The layout under this item, `depth` levels deep (-1: all).
    fn layout(&self, depth: i32, names: &[String]) -> Layout {
        let children = match depth {
            0 => Vec::new(),
            _ => self
                .children
                .iter()
                .map(|c| c.layout(if depth < 0 { depth } else { depth - 1 }, names))
                .collect(),
        };
        Layout {
            id: self.id,
            properties: self.properties(names),
            children,
        }
    }
}

/// Doubles underscores: a single `_` marks the next letter as the mnemonic.
fn escape_label(label: &str) -> String {
    label.replace('_', "__")
}

/// A subtree as `GetLayout` returns it: `(ia{sv}av)`, each child a variant holding the
/// same structure.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub id: i32,
    pub properties: Properties,
    pub children: Vec<Layout>,
}

impl Type for Layout {
    // `(ia{sv}av)`
    const SIGNATURE: &'static Signature = &Signature::Structure(Fields::Static {
        fields: &[
            <i32 as Type>::SIGNATURE,
            <Properties as Type>::SIGNATURE,
            <Vec<OwnedValue> as Type>::SIGNATURE,
        ],
    });
}

impl Serialize for Layout {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Children<'a>(&'a [Layout]);
        impl Serialize for Children<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
                for child in self.0 {
                    seq.serialize_element(&SerializeValue(child))?;
                }
                seq.end()
            }
        }
        let mut s = serializer.serialize_struct("Layout", 3)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("properties", &self.properties)?;
        s.serialize_field("children", &Children(&self.children))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LayoutVisitor;
        impl<'de> Visitor<'de> for LayoutVisitor {
            type Value = Layout;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a DBusMenu layout (ia{sv}av)")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Layout, A::Error> {
                let missing = |i| de::Error::invalid_length(i, &self);
                let id = seq.next_element()?.ok_or_else(|| missing(0))?;
                let properties = seq.next_element()?.ok_or_else(|| missing(1))?;
                let children: Vec<DeserializeValue<'de, Layout>> =
                    seq.next_element()?.ok_or_else(|| missing(2))?;
                Ok(Layout {
                    id,
                    properties,
                    children: children.into_iter().map(|c| c.0).collect(),
                })
            }
        }
        deserializer.deserialize_struct("Layout", &["id", "properties", "children"], LayoutVisitor)
    }
}

/// The tree and its revision, shared by the export and the D-Bus object.
#[derive(Debug)]
struct Tree {
    revision: u32,
    root: MenuNode,
}

type OnClick = Arc<dyn Fn(i32) + Send + Sync>;

fn lock(tree: &Mutex<Tree>) -> MutexGuard<'_, Tree> {
    // A panic elsewhere can't leave the tree half-written: every writer replaces fields
    // whole.
    tree.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The `com.canonical.dbusmenu` object.
struct Server {
    tree: Arc<Mutex<Tree>>,
    on_click: OnClick,
}

fn no_item(id: i32) -> fdo::Error {
    fdo::Error::InvalidArgs(format!("no menu item {id}"))
}

impl Server {
    /// Handles one event; false if `id` doesn't exist.
    fn handle_event(&self, id: i32, event: &str) -> bool {
        let click = {
            let tree = lock(&self.tree);
            let Some(node) = tree.root.find(id) else {
                return false;
            };
            event == "clicked" && node.clickable()
        };
        tracing::trace!(id, event, "dbusmenu event");
        if click {
            (self.on_click)(id);
        }
        true
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Server {
    /// The subtree under `parent_id`, `recursion_depth` levels deep (-1: all), with the
    /// properties in `property_names` (all if empty).
    #[zbus(out_args("revision", "layout"))]
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> fdo::Result<(u32, Layout)> {
        let tree = lock(&self.tree);
        let node = tree
            .root
            .find(parent_id)
            .ok_or_else(|| no_item(parent_id))?;
        Ok((tree.revision, node.layout(recursion_depth, &property_names)))
    }

    /// The properties of the items `ids` (every item if empty); unknown ids are skipped.
    #[zbus(out_args("properties"))]
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        let tree = lock(&self.tree);
        let nodes = if ids.is_empty() {
            let mut all = Vec::new();
            tree.root.visit(&mut all);
            all
        } else {
            ids.iter().filter_map(|&id| tree.root.find(id)).collect()
        };
        nodes
            .into_iter()
            .map(|n| (n.id, n.properties(&property_names)))
            .collect()
    }

    /// One property of one item. A property at its default value is an error, as in
    /// libdbusmenu.
    #[zbus(out_args("value"))]
    fn get_property(&self, id: i32, name: String) -> fdo::Result<OwnedValue> {
        let tree = lock(&self.tree);
        let node = tree.root.find(id).ok_or_else(|| no_item(id))?;
        node.properties(std::slice::from_ref(&name))
            .remove(&name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("item {id} has no property {name}")))
    }

    /// `clicked` chooses an item; `hovered`, `opened` and `closed` are accepted and ignored.
    fn event(
        &self,
        id: i32,
        event_id: String,
        data: OwnedValue,
        timestamp: u32,
    ) -> fdo::Result<()> {
        let _ = (data, timestamp);
        if self.handle_event(id, &event_id) {
            Ok(())
        } else {
            Err(no_item(id))
        }
    }

    /// Several events; returns the ids that don't exist (an error if none did).
    #[zbus(out_args("idErrors"))]
    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> fdo::Result<Vec<i32>> {
        let count = events.len();
        let errors: Vec<i32> = events
            .into_iter()
            .filter(|(id, event, ..)| !self.handle_event(*id, event))
            .map(|(id, ..)| id)
            .collect();
        if count > 0 && errors.len() == count {
            return Err(fdo::Error::InvalidArgs(
                "none of the menu items exist".into(),
            ));
        }
        Ok(errors)
    }

    /// The menu under `id` is about to open. The layout is always complete, so no update
    /// is ever needed.
    #[zbus(out_args("needUpdate"))]
    fn about_to_show(&self, id: i32) -> fdo::Result<bool> {
        lock(&self.tree)
            .root
            .find(id)
            .map(|_| false)
            .ok_or_else(|| no_item(id))
    }

    /// [`Self::about_to_show`] for several menus; returns (updates needed, unknown ids).
    #[zbus(out_args("updatesNeeded", "idErrors"))]
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        let tree = lock(&self.tree);
        let errors = ids
            .into_iter()
            .filter(|&id| tree.root.find(id).is_none())
            .collect();
        (Vec::new(), errors)
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        VERSION
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    /// The layout under `parent` changed; its revision is now `revision`.
    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    /// Properties changed without a layout change. Not sent yet: every change is a new
    /// layout. Declared so introspection matches the specification.
    #[zbus(signal)]
    async fn items_properties_updated(
        emitter: &SignalEmitter<'_>,
        updated_props: Vec<(i32, Properties)>,
        removed_props: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;

    /// Asks the menu bar to open the menu of `id` (a key equivalent). Not sent yet.
    #[zbus(signal)]
    async fn item_activation_requested(
        emitter: &SignalEmitter<'_>,
        id: i32,
        timestamp: u32,
    ) -> zbus::Result<()>;
}

/// One exported menu tree. Dropping it removes the object from the bus.
pub struct MenuExport {
    conn: Connection,
    path: OwnedObjectPath,
    tree: Arc<Mutex<Tree>>,
}

impl std::fmt::Debug for MenuExport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuExport")
            .field("service", &self.service())
            .field("path", &self.path.as_str())
            .finish_non_exhaustive()
    }
}

impl MenuExport {
    /// Exports `items` (the menu-bar titles) at `path` on `conn`, a connection made by
    /// [`crate::connect_session`] (or one whose object server already runs). `on_click` runs on
    /// zbus's executor thread with the id of each item a client chooses; keep it short
    /// (send the id to your own thread).
    pub fn new(
        conn: &Connection,
        path: &str,
        items: Vec<MenuNode>,
        on_click: impl Fn(i32) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let path: OwnedObjectPath = ObjectPath::try_from(path)
            .map_err(|_| Error::Path(path.to_owned()))?
            .into();
        let tree = Arc::new(Mutex::new(Tree {
            revision: 1,
            root: MenuNode::root(items),
        }));
        let server = Server {
            tree: Arc::clone(&tree),
            on_click: Arc::new(on_click),
        };
        if !conn.object_server().at(&path, server)? {
            return Err(Error::PathTaken(path.to_string()));
        }
        Ok(Self {
            conn: conn.clone(),
            path,
            tree,
        })
    }

    /// Replaces the menu-bar titles and everything under them, and tells clients.
    pub fn set_items(&self, items: Vec<MenuNode>) -> Result<(), Error> {
        let revision = {
            let mut tree = lock(&self.tree);
            tree.root = MenuNode::root(items);
            tree.revision = tree.revision.wrapping_add(1);
            tree.revision
        };
        let emitter = SignalEmitter::new(self.conn.inner(), self.path.as_ref())?;
        zbus::block_on(Server::layout_updated(&emitter, revision, 0))?;
        Ok(())
    }

    /// The connection's unique bus name, for the appmenu address. `None` on a
    /// peer-to-peer connection.
    pub fn service(&self) -> Option<String> {
        self.conn.unique_name().map(ToString::to_string)
    }

    /// The object path.
    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    /// The current layout revision.
    pub fn revision(&self) -> u32 {
        lock(&self.tree).revision
    }
}

impl Drop for MenuExport {
    fn drop(&mut self) {
        if let Err(err) = self.conn.object_server().remove::<Server, _>(&self.path) {
            tracing::debug!(%err, path = %self.path, "couldn't remove the menu object");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn default_properties_are_left_out() {
        let props = MenuNode::item(1, "Open").properties(&[]);
        assert_eq!(props.len(), 1);
        assert_eq!(
            props["label"],
            OwnedValue::from(zbus::zvariant::Str::from("Open"))
        );
    }

    #[test]
    fn properties_cover_every_item_feature() {
        let mut node = MenuNode::item(1, "Word_Wrap");
        node.enabled = false;
        node.toggle = Toggle::Check(ToggleState::Mixed);
        node.shortcut = names(&["Control", "Shift", "W"]);
        let props = node.properties(&[]);
        let s = |k: &str| String::try_from(props[k].try_clone().unwrap()).unwrap();
        assert_eq!(s("label"), "Word__Wrap");
        assert_eq!(s("toggle-type"), "checkmark");
        assert_eq!(i32::try_from(&props["toggle-state"]).unwrap(), -1);
        assert!(!bool::try_from(&props["enabled"]).unwrap());
        let keys: Vec<Vec<String>> = props["shortcut"].try_clone().unwrap().try_into().unwrap();
        assert_eq!(keys, [names(&["Control", "Shift", "W"])]);

        let sep = MenuNode::separator(2).properties(&[]);
        assert_eq!(sep.len(), 1);
        assert!(sep.contains_key("type"));

        let sub = MenuNode::submenu(3, "More", vec![MenuNode::item(4, "A")]).properties(&[]);
        assert!(sub.contains_key("children-display"));
    }

    #[test]
    fn property_names_filter() {
        let mut node = MenuNode::item(1, "Open");
        node.enabled = false;
        let props = node.properties(&names(&["enabled"]));
        assert_eq!(props.keys().collect::<Vec<_>>(), ["enabled"]);
    }

    #[test]
    fn layout_depth_limits_children() {
        let root = MenuNode::root(vec![MenuNode::submenu(
            1,
            "File",
            vec![MenuNode::item(2, "Open")],
        )]);
        assert!(root.layout(0, &[]).children.is_empty());
        let one = root.layout(1, &[]);
        assert_eq!(one.children.len(), 1);
        assert!(one.children[0].children.is_empty());
        assert_eq!(root.layout(-1, &[]).children[0].children[0].id, 2);
    }
}
