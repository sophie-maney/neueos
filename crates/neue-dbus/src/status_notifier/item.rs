//! One status-notifier item: what it shows ([`ItemInfo`]) and how to talk to it
//! ([`StatusItem`]), on `org.kde.StatusNotifierItem` (task 3.13a).
//!
//! Apps differ in what they export (Qt apps give a named icon or pixmaps, libappindicator
//! adds `IconThemePath`, Electron apps often only pixmaps), so [`ItemInfo`] is read
//! leniently: a missing or mistyped property is "none", never an error, and a malformed
//! pixmap is dropped.

use std::collections::HashMap;

use zbus::blocking::Connection;
use zbus::blocking::fdo::PropertiesProxy;
use zbus::message::Type as MessageType;
use zbus::names::{BusName, InterfaceName};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};
use zbus::{MatchRule, blocking::MessageIterator};

use crate::Error;

/// The item interface.
pub const INTERFACE: &str = "org.kde.StatusNotifierItem";
/// Where items that give no path are assumed to be (the spec's default).
pub const DEFAULT_PATH: &str = "/StatusNotifierItem";

/// Pixmaps wider or taller than this are dropped: nobody draws a menu-bar icon that big,
/// and the limit bounds what an app can make the shell hold.
const MAX_PIXMAP_SIDE: u32 = 512;

/// An icon as pixels: straight (not premultiplied) RGBA, row by row, `width * height * 4`
/// bytes. The wire form is ARGB in network byte order; [`Pixmap::from_wire`] converts.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Pixmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixmap({}x{})", self.width, self.height)
    }
}

impl Pixmap {
    /// A pixmap from the wire `(width, height, ARGB bytes)`; `None` when the size is not
    /// positive, is over the limit, or doesn't match the byte count.
    pub fn from_wire(width: i32, height: i32, argb: &[u8]) -> Option<Self> {
        let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
        if w == 0 || h == 0 || w > MAX_PIXMAP_SIDE || h > MAX_PIXMAP_SIDE {
            return None;
        }
        if argb.len() != (w * h * 4) as usize {
            return None;
        }
        let mut rgba = Vec::with_capacity(argb.len());
        for px in argb.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
        }
        Some(Self {
            width: w,
            height: h,
            rgba,
        })
    }
}

/// The pixmap to draw at `size` pixels: the smallest that is at least that big, else the
/// biggest there is.
pub fn best_pixmap(pixmaps: &[Pixmap], size: u32) -> Option<&Pixmap> {
    let side = |p: &Pixmap| p.width.min(p.height);
    pixmaps
        .iter()
        .filter(|p| side(p) >= size)
        .min_by_key(|p| side(p))
        .or_else(|| pixmaps.iter().max_by_key(|p| side(p)))
}

/// What kind of thing an item is (`Category`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
}

/// How much the item wants to be seen (`Status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Nothing to show; the host may hide it.
    Passive,
    #[default]
    Active,
    /// Wants attention: shows its attention icon.
    NeedsAttention,
}

/// An icon by theme name and/or by pixels. The name is empty when the item gives none.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemIcon {
    pub name: String,
    pub pixmaps: Vec<Pixmap>,
}

impl ItemIcon {
    /// Neither a name nor a pixmap.
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.pixmaps.is_empty()
    }
}

/// The tooltip. `title` and `text` are plain strings or the spec's small HTML subset (`<b>`,
/// `<i>`, `<u>`, `<br>`, `<a>`, `<img>`); the caller strips what it doesn't draw.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolTip {
    pub icon: ItemIcon,
    pub title: String,
    pub text: String,
}

/// Everything an item says about itself, as of one read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemInfo {
    /// The app's id for the item (`Id`), stable across runs.
    pub id: String,
    pub title: String,
    pub category: Category,
    pub status: Status,
    pub icon: ItemIcon,
    /// Shown instead of `icon` while the status is [`Status::NeedsAttention`].
    pub attention_icon: ItemIcon,
    /// An extra directory to look for `icon.name` in first (`IconThemePath`, from
    /// libappindicator and Qt); `None` when empty.
    pub icon_theme_path: Option<String>,
    /// `None` when the title and text are both empty.
    pub tooltip: Option<ToolTip>,
    /// The item's DBusMenu, for [`crate::MenuClient`].
    pub menu: Option<String>,
    /// The item has no `Activate` of its own: a click should open its menu.
    pub item_is_menu: bool,
}

impl ItemInfo {
    /// Reads the properties of `org.kde.StatusNotifierItem` (as `GetAll` returns them).
    pub fn from_properties(props: &HashMap<String, OwnedValue>) -> Self {
        let string = |key: &str| {
            props
                .get(key)
                .and_then(|v| <&str>::try_from(&**v).ok())
                .unwrap_or_default()
                .to_owned()
        };
        let pixmaps = |key: &str| -> Vec<Pixmap> {
            props.get(key).and_then(wire_pixmaps).unwrap_or_default()
        };
        let tooltip = props
            .get("ToolTip")
            .and_then(|v| {
                <(String, Vec<(i32, i32, Vec<u8>)>, String, String)>::try_from(v.try_clone().ok()?)
                    .ok()
            })
            .map(|(name, pixmaps, title, text)| ToolTip {
                icon: ItemIcon {
                    name,
                    pixmaps: pixmaps
                        .iter()
                        .filter_map(|(w, h, d)| Pixmap::from_wire(*w, *h, d))
                        .collect(),
                },
                title,
                text,
            })
            .filter(|t| !(t.title.is_empty() && t.text.is_empty()));
        let non_empty = |s: String| (!s.is_empty()).then_some(s);
        Self {
            id: string("Id"),
            title: string("Title"),
            category: match string("Category").as_str() {
                "Communications" => Category::Communications,
                "SystemServices" => Category::SystemServices,
                "Hardware" => Category::Hardware,
                _ => Category::ApplicationStatus,
            },
            status: match string("Status").as_str() {
                "Passive" => Status::Passive,
                "NeedsAttention" => Status::NeedsAttention,
                _ => Status::Active,
            },
            icon: ItemIcon {
                name: string("IconName"),
                pixmaps: pixmaps("IconPixmap"),
            },
            attention_icon: ItemIcon {
                name: string("AttentionIconName"),
                pixmaps: pixmaps("AttentionIconPixmap"),
            },
            icon_theme_path: non_empty(string("IconThemePath")),
            tooltip,
            menu: props
                .get("Menu")
                .and_then(|v| <&ObjectPath>::try_from(&**v).ok())
                // Some apps say "/" for "no menu".
                .map(|p| p.to_string())
                .filter(|p| p != "/"),
            item_is_menu: props
                .get("ItemIsMenu")
                .and_then(|v| bool::try_from(&**v).ok())
                .unwrap_or(false),
        }
    }

    /// The icon to draw now: the attention icon while the item needs attention and has one.
    pub fn current_icon(&self) -> &ItemIcon {
        if self.status == Status::NeedsAttention && !self.attention_icon.is_empty() {
            &self.attention_icon
        } else {
            &self.icon
        }
    }

    /// What to call the item for people: its title, else its id.
    pub fn display_name(&self) -> &str {
        if self.title.is_empty() {
            &self.id
        } else {
            &self.title
        }
    }
}

/// The pixmaps of an `a(iiay)` value; `None` when it has another type.
fn wire_pixmaps(value: &OwnedValue) -> Option<Vec<Pixmap>> {
    let wire = Vec::<(i32, i32, Vec<u8>)>::try_from(value.try_clone().ok()?).ok()?;
    Some(
        wire.iter()
            .filter_map(|(w, h, data)| Pixmap::from_wire(*w, *h, data))
            .collect(),
    )
}

/// Which way a scroll went (`Scroll`'s `orientation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

impl Orientation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
        }
    }
}

/// One item on the bus.
#[derive(Debug, Clone)]
pub struct StatusItem {
    conn: Connection,
    service: BusName<'static>,
    path: OwnedObjectPath,
    proxy: zbus::blocking::Proxy<'static>,
}

impl StatusItem {
    /// The item `service` exports at `path`. Nothing is sent yet.
    pub fn new(conn: &Connection, service: &str, path: &str) -> Result<Self, Error> {
        let service_name =
            BusName::try_from(service.to_owned()).map_err(|_| Error::Name(service.to_owned()))?;
        let object: OwnedObjectPath = ObjectPath::try_from(path.to_owned())
            .map_err(|_| Error::Path(path.to_owned()))?
            .into();
        let proxy = zbus::blocking::proxy::Builder::<zbus::blocking::Proxy<'static>>::new(conn)
            .destination(service_name.clone())?
            .path(object.clone())?
            .interface(INTERFACE)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()?;
        Ok(Self {
            conn: conn.clone(),
            service: service_name,
            path: object,
            proxy,
        })
    }

    /// Reads every property.
    pub fn info(&self) -> Result<ItemInfo, Error> {
        let props = PropertiesProxy::builder(&self.conn)
            .destination(self.service.clone())?
            .path(self.path.clone())?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()?;
        let all = props
            .get_all(InterfaceName::from_static_str_unchecked(INTERFACE))
            .map_err(zbus::Error::from)?;
        Ok(ItemInfo::from_properties(&all))
    }

    /// Primary action (a left click); `x` and `y` are the screen position of the click.
    pub fn activate(&self, x: i32, y: i32) -> Result<(), Error> {
        Ok(self.proxy.call("Activate", &(x, y))?)
    }

    /// Secondary action (a middle click).
    pub fn secondary_activate(&self, x: i32, y: i32) -> Result<(), Error> {
        Ok(self.proxy.call("SecondaryActivate", &(x, y))?)
    }

    /// Asks the item to show its own context menu, for items without a DBusMenu.
    pub fn context_menu(&self, x: i32, y: i32) -> Result<(), Error> {
        Ok(self.proxy.call("ContextMenu", &(x, y))?)
    }

    /// A scroll over the icon by `delta` steps.
    pub fn scroll(&self, delta: i32, orientation: Orientation) -> Result<(), Error> {
        Ok(self.proxy.call("Scroll", &(delta, orientation.as_str()))?)
    }

    /// The item's bus name, as given.
    pub fn service(&self) -> &str {
        self.service.as_str()
    }

    /// The item's object path.
    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

/// An item changed (`NewIcon`, `NewToolTip`, `NewStatus`, ...): read it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemChanged {
    /// The sender's unique name (`:1.42`); empty on a peer-to-peer connection.
    pub sender: String,
    pub path: String,
}

/// Every item change signal on `conn`, from any app. Blocking: iterate it on a thread of
/// its own. It ends when the connection closes.
pub struct ItemSignals {
    messages: MessageIterator,
}

impl std::fmt::Debug for ItemSignals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemSignals").finish_non_exhaustive()
    }
}

impl ItemSignals {
    /// Subscribes to `org.kde.StatusNotifierItem` signals (adds a match rule on a bus).
    pub fn new(conn: &Connection) -> Result<Self, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(INTERFACE)?
            .build();
        // The caller reads the item again, so a burst beyond this loses nothing.
        let messages = MessageIterator::for_match_rule(rule, conn, Some(64))?;
        Ok(Self { messages })
    }
}

impl Iterator for ItemSignals {
    type Item = ItemChanged;

    fn next(&mut self) -> Option<ItemChanged> {
        loop {
            let message = match self.messages.next()? {
                Ok(message) => message,
                Err(err) => {
                    tracing::debug!(%err, "bad message on the status-item signal stream");
                    continue;
                }
            };
            let header = message.header();
            let member = header.member().map(|m| m.as_str());
            if !matches!(
                member,
                Some(
                    "NewTitle"
                        | "NewIcon"
                        | "NewAttentionIcon"
                        | "NewOverlayIcon"
                        | "NewToolTip"
                        | "NewStatus"
                )
            ) {
                continue;
            }
            let Some(path) = header.path() else {
                continue;
            };
            return Some(ItemChanged {
                sender: header.sender().map(ToString::to_string).unwrap_or_default(),
                path: path.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use zbus::zvariant::Value;

    use super::*;

    fn val<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
        OwnedValue::try_from(v.into()).unwrap()
    }

    #[test]
    fn wire_pixels_become_straight_rgba() {
        // One ARGB pixel: alpha 0x80, red 1, green 2, blue 3.
        let p = Pixmap::from_wire(1, 1, &[0x80, 1, 2, 3]).unwrap();
        assert_eq!((p.width, p.height, p.rgba), (1, 1, vec![1, 2, 3, 0x80]));
    }

    #[test]
    fn malformed_pixmaps_are_dropped() {
        assert!(Pixmap::from_wire(0, 1, &[]).is_none());
        assert!(Pixmap::from_wire(-1, 1, &[0; 4]).is_none());
        assert!(Pixmap::from_wire(2, 2, &[0; 12]).is_none(), "short data");
        assert!(Pixmap::from_wire(1, 1, &[0; 8]).is_none(), "long data");
        assert!(Pixmap::from_wire(513, 1, &vec![0; 513 * 4]).is_none());
    }

    #[test]
    fn the_best_pixmap_is_the_smallest_big_enough_else_the_biggest() {
        let sq = |n: u32| Pixmap {
            width: n,
            height: n,
            rgba: Vec::new(),
        };
        let all = [sq(32), sq(16), sq(64)];
        assert_eq!(best_pixmap(&all, 20).unwrap().width, 32);
        assert_eq!(best_pixmap(&all, 16).unwrap().width, 16);
        assert_eq!(best_pixmap(&all, 100).unwrap().width, 64);
        assert!(best_pixmap(&[], 16).is_none());
    }

    #[test]
    fn properties_are_read_leniently() {
        let mut props = HashMap::new();
        props.insert("Id".to_owned(), val("nm-applet"));
        props.insert("Title".to_owned(), val("Network"));
        props.insert("Category".to_owned(), val("Hardware"));
        props.insert("Status".to_owned(), val("NeedsAttention"));
        props.insert("IconName".to_owned(), val("network-wired"));
        props.insert("AttentionIconName".to_owned(), val("network-error"));
        props.insert("IconThemePath".to_owned(), val(""));
        props.insert(
            "Menu".to_owned(),
            val(ObjectPath::try_from("/org/ayatana/NotificationItem/nm/Menu").unwrap()),
        );
        // A property with the wrong type is none, not an error.
        props.insert("ItemIsMenu".to_owned(), val("yes"));
        props.insert("IconPixmap".to_owned(), val(7u32));
        let info = ItemInfo::from_properties(&props);
        assert_eq!(info.display_name(), "Network");
        assert_eq!(info.category, Category::Hardware);
        assert_eq!(info.status, Status::NeedsAttention);
        assert_eq!(info.current_icon().name, "network-error");
        assert_eq!(info.icon_theme_path, None);
        assert_eq!(
            info.menu.as_deref(),
            Some("/org/ayatana/NotificationItem/nm/Menu")
        );
        assert!(!info.item_is_menu);
        assert!(info.icon.pixmaps.is_empty());
        assert!(info.tooltip.is_none());
    }

    #[test]
    fn an_empty_item_defaults_to_active_and_the_normal_icon() {
        let info = ItemInfo::from_properties(&HashMap::new());
        assert_eq!(info.status, Status::Active);
        assert_eq!(info.category, Category::ApplicationStatus);
        assert!(info.current_icon().is_empty());
        // NeedsAttention without an attention icon keeps the normal one.
        let info = ItemInfo {
            status: Status::NeedsAttention,
            icon: ItemIcon {
                name: "a".into(),
                pixmaps: Vec::new(),
            },
            ..ItemInfo::default()
        };
        assert_eq!(info.current_icon().name, "a");
    }

    #[test]
    fn pixmaps_and_tooltips_come_off_the_wire() {
        let mut props = HashMap::new();
        let good = (1i32, 1i32, vec![0xFFu8, 10, 20, 30]);
        let bad = (2i32, 2i32, vec![0u8; 3]);
        props.insert(
            "IconPixmap".to_owned(),
            val(vec![good.clone(), bad.clone()]),
        );
        props.insert(
            "ToolTip".to_owned(),
            val(Value::from(
                zbus::zvariant::StructureBuilder::new()
                    .add_field("tip-icon".to_owned())
                    .add_field(vec![good])
                    .add_field("Title".to_owned())
                    .add_field("Body".to_owned())
                    .build()
                    .unwrap(),
            )),
        );
        let info = ItemInfo::from_properties(&props);
        assert_eq!(info.icon.pixmaps.len(), 1);
        assert_eq!(info.icon.pixmaps[0].rgba, [10, 20, 30, 0xFF]);
        let tip = info.tooltip.unwrap();
        assert_eq!((tip.title.as_str(), tip.text.as_str()), ("Title", "Body"));
        assert_eq!(tip.icon.name, "tip-icon");
        assert_eq!(tip.icon.pixmaps.len(), 1);
    }
}
