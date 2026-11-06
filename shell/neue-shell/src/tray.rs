//! The status-icon tray's worker (task 3.13c, ADR 0084): the shell owns the
//! StatusNotifierWatcher, runs the host on a thread of its own and sends the UI thread what
//! each item shows (icons already resolved to pictures) and what its menu holds. The UI
//! thread keeps the [`Host`] to act on items (`Activate`, `ContextMenu`, a menu's `clicked`).

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

use neue_dbus::status_notifier::{
    ItemChanged, ItemIcon, ItemInfo, ItemSignals, Status, ToolTip, WatcherEvent, WatcherSignals,
    best_pixmap,
};
use neue_dbus::{Host, HostEvent, MenuChanged, MenuNode, MenuSignals, Watcher};
use neue_desktop::icon_load::load_file;
use neue_desktop::{IconLoader, IconLookup};
use neue_kit::Icon;
use neue_kit::desktop::Picture;
use neue_kit::tokens::materials::menu_bar::TRAY_ICON_SIZE;
use neue_shell::bar::{TrayItemView, TrayStatus};

/// The theme icon names are looked up in, and the one after it (ADR 0070). Until a setting
/// for the icon theme exists, this is the one neueOS ships.
pub(crate) const ICON_THEME: &str = "neueOS";
pub(crate) const FALLBACK_THEME: &str = "Papirus";

/// Icons are loaded for this scale, so that they stay sharp on a doubled output (the bar
/// fits them to [`TRAY_ICON_SIZE`] whatever their size).
const ICON_SCALE: u32 = 2;

/// The tray's icon size in whole logical pixels.
fn icon_size() -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive pixel size"
    )]
    let size = TRAY_ICON_SIZE.round() as u32;
    size
}

/// What the tray worker tells the UI thread.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    /// An item appeared, or shows something else now.
    Item(Item),
    /// An item's DBusMenu was read (again).
    Menu { id: String, nodes: Vec<MenuNode> },
    /// An item left.
    Removed(String),
}

/// A status item as the UI thread keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub view: TrayItemView,
    /// It exports a DBusMenu (which arrives as an [`Update::Menu`]).
    pub has_menu: bool,
    /// It has no `Activate` of its own: a click opens its menu.
    pub item_is_menu: bool,
}

/// Plain text from a tooltip's title and text, which may carry the spec's small HTML
/// subset: tags are dropped (`<br>` is a space) and the basic entities decoded.
pub fn plain_text(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            // Not a tag after all.
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + end].trim().to_ascii_lowercase();
        if tag.trim_end_matches('/').trim() == "br" {
            out.push(' ');
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A tooltip as one line of text: its title, then its text.
pub fn tooltip_text(tooltip: Option<&ToolTip>) -> String {
    let Some(tooltip) = tooltip else {
        return String::new();
    };
    let (title, text) = (plain_text(&tooltip.title), plain_text(&tooltip.text));
    match (title.is_empty(), text.is_empty()) {
        (_, true) => title,
        (true, false) => text,
        (false, false) => format!("{title}: {text}"),
    }
}

/// Turns icon names and pixmaps into pictures.
struct Resolver {
    loader: IconLoader,
}

impl Resolver {
    fn new() -> Self {
        Self {
            loader: IconLoader::new(IconLookup::system(ICON_THEME, Some(FALLBACK_THEME))),
        }
    }

    /// The picture for `icon`: its name from the item's own directory (`theme_path`), else
    /// from the icon theme; else its best pixmap. `None` when there is neither.
    fn resolve(&mut self, icon: &ItemIcon, theme_path: Option<&str>) -> Option<Icon> {
        if !icon.name.is_empty() {
            if let Some(found) = theme_path.and_then(|dir| self.in_dir(&icon.name, dir)) {
                return Some(found);
            }
            if let Some(found) = self
                .loader
                .load(&icon.name, icon_size(), f64::from(ICON_SCALE))
            {
                return Some(found);
            }
        }
        pixmap(icon)
    }

    /// `name` in `dir` (`IconThemePath`): loose files there, or a theme laid out under it.
    fn in_dir(&self, name: &str, dir: &str) -> Option<Icon> {
        let dir = std::path::PathBuf::from(dir);
        let lookup = IconLookup::new(
            std::slice::from_ref(&dir),
            "hicolor",
            None,
            std::slice::from_ref(&dir),
        );
        let file = lookup.find(name, icon_size(), ICON_SCALE)?;
        load_file(&file, icon_size() * ICON_SCALE)
            .inspect_err(|error| tracing::debug!(%error, name, "cannot load a tray icon"))
            .ok()
    }

    /// The tray's view of `info`.
    fn view(&mut self, id: &str, info: &ItemInfo) -> TrayItemView {
        let path = info.icon_theme_path.as_deref();
        TrayItemView {
            id: id.to_owned(),
            title: info.display_name().to_owned(),
            status: match info.status {
                Status::Passive => TrayStatus::Passive,
                Status::Active => TrayStatus::Active,
                Status::NeedsAttention => TrayStatus::NeedsAttention,
            },
            icon: self.resolve(&info.icon, path),
            attention_icon: self.resolve(&info.attention_icon, path),
            tooltip: tooltip_text(info.tooltip.as_ref()),
        }
    }

    fn item(&mut self, id: &str, info: &ItemInfo) -> Item {
        Item {
            view: self.view(id, info),
            has_menu: info.menu.is_some(),
            item_is_menu: info.item_is_menu,
        }
    }
}

/// The pixmap of `icon` that fits the tray best, as a picture.
fn pixmap(icon: &ItemIcon) -> Option<Icon> {
    let best = best_pixmap(&icon.pixmaps, icon_size() * ICON_SCALE)?;
    Picture::from_rgba8(best.width, best.height, &best.rgba).map(Icon::raster)
}

/// What the worker's loop waits for.
enum Input {
    Watcher(WatcherEvent),
    Item(ItemChanged),
    /// Some DBusMenu on the bus changed.
    MenuChanged(MenuChanged),
    /// A menu was read for item `id` (by a thread of its own, so that a slow app holds
    /// nothing up).
    Fetched {
        id: String,
        nodes: Option<Vec<MenuNode>>,
    },
}

/// Reads the menus of the items: at most one read per item at a time, and one more after
/// it when the menu changed meanwhile.
struct Menus {
    host: Host,
    tx: mpsc::Sender<Input>,
    /// Item id to its menu's object path (items that have one).
    paths: HashMap<String, String>,
    fetching: HashSet<String>,
    again: HashSet<String>,
}

impl Menus {
    fn request(&mut self, id: &str) {
        if !self.fetching.insert(id.to_owned()) {
            self.again.insert(id.to_owned());
            return;
        }
        let (host, tx, id) = (self.host.clone(), self.tx.clone(), id.to_owned());
        let spawned = std::thread::Builder::new()
            .name("sni-menu".into())
            .spawn(move || {
                let nodes = host
                    .menu(&id)
                    .and_then(|menu| menu.map(|m| m.fetch()).transpose())
                    .inspect_err(|err| tracing::debug!(%err, id, "cannot read a tray menu"))
                    .ok()
                    .flatten();
                // The loop is gone when the shell is.
                tx.send(Input::Fetched { id, nodes }).ok();
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "cannot start a thread to read a tray menu");
        }
    }

    /// The item showed `info`: reads its menu if it is new or moved.
    fn shown(&mut self, id: &str, info: &ItemInfo) {
        match &info.menu {
            Some(path) => {
                if self.paths.get(id) != Some(path) {
                    self.paths.insert(id.to_owned(), path.clone());
                    self.request(id);
                }
            }
            None => {
                self.paths.remove(id);
            }
        }
    }

    fn removed(&mut self, id: &str) {
        self.paths.remove(id);
        self.again.remove(id);
    }

    /// A menu changed: reads again the items whose menu has that path.
    fn changed(&mut self, change: &MenuChanged) {
        let ids: Vec<String> = self
            .paths
            .iter()
            .filter(|(_, path)| **path == change.path)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.request(&id);
        }
    }

    /// A read finished; true if its result is still wanted.
    fn fetched(&mut self, id: &str) -> bool {
        self.fetching.remove(id);
        if self.again.remove(id) && self.paths.contains_key(id) {
            self.request(id);
        }
        self.paths.contains_key(id)
    }
}

/// Turns a blocking iterator of signals into inputs, on a thread of its own that ends with
/// the iterator or the loop.
fn forward<T: Send + 'static>(
    name: &str,
    signals: impl Iterator<Item = T> + Send + 'static,
    tx: mpsc::Sender<Input>,
    wrap: fn(T) -> Input,
) -> Result<(), neue_dbus::Error> {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            for signal in signals {
                if tx.send(wrap(signal)).is_err() {
                    return;
                }
            }
        })
        .map(drop)
        .map_err(|e| neue_dbus::Error::from(neue_dbus::zbus::Error::from(e)))
}

/// Owns the watcher on the session bus and hosts its items until `send` says the shell is
/// gone. `host` is given the [`Host`] once, for the UI thread's actions.
pub fn run(
    host: impl FnOnce(Host) -> bool,
    send: impl Fn(Update) -> bool,
) -> Result<(), neue_dbus::Error> {
    let conn = neue_dbus::connect_session()?;
    // Held for as long as the loop runs: dropping it gives the name up.
    let _watcher = Watcher::own(&conn)?;
    let sni = Host::new(&conn);
    let (tx, rx) = mpsc::channel();
    // Subscribe before reading what is there, so that nothing registering meanwhile is
    // missed (ADR 0084).
    forward(
        "sni-watcher",
        WatcherSignals::new(&conn)?,
        tx.clone(),
        Input::Watcher,
    )?;
    forward(
        "sni-items",
        ItemSignals::new(&conn)?,
        tx.clone(),
        Input::Item,
    )?;
    forward(
        "sni-menus",
        MenuSignals::new(&conn)?,
        tx.clone(),
        Input::MenuChanged,
    )?;
    sni.announce()?;
    if !host(sni.clone()) {
        return Ok(());
    }
    let mut resolver = Resolver::new();
    let mut menus = Menus {
        host: sni.clone(),
        tx,
        paths: HashMap::new(),
        fetching: HashSet::new(),
        again: HashSet::new(),
    };
    let mut apply = |event: HostEvent, menus: &mut Menus| -> bool {
        match event {
            HostEvent::Added { id, info } | HostEvent::Changed { id, info } => {
                menus.shown(&id, &info);
                send(Update::Item(resolver.item(&id, &info)))
            }
            HostEvent::Removed { id } => {
                menus.removed(&id);
                send(Update::Removed(id))
            }
        }
    };
    for event in sni.initial()? {
        if !apply(event, &mut menus) {
            return Ok(());
        }
    }
    for input in rx {
        let event = match input {
            Input::Watcher(WatcherEvent::ItemRegistered(address)) => sni.registered(&address),
            Input::Watcher(WatcherEvent::ItemUnregistered(address)) => sni.unregistered(&address),
            Input::Item(change) => sni.changed(&change),
            Input::MenuChanged(change) => {
                menus.changed(&change);
                None
            }
            Input::Fetched { id, nodes } => {
                if menus.fetched(&id)
                    && let Some(nodes) = nodes
                    && !send(Update::Menu { id, nodes })
                {
                    return Ok(());
                }
                None
            }
        };
        if let Some(event) = event
            && !apply(event, &mut menus)
        {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use neue_dbus::status_notifier::Pixmap;

    use super::*;

    #[test]
    fn plain_text_drops_tags_and_decodes_entities() {
        assert_eq!(plain_text("Plain"), "Plain");
        assert_eq!(plain_text("<b>Chat</b>: 2 <i>unread</i>"), "Chat: 2 unread");
        assert_eq!(plain_text("one<br/>two<br>three"), "one two three");
        assert_eq!(plain_text("Tom &amp; Jerry &lt;3"), "Tom & Jerry <3");
        assert_eq!(
            plain_text("a <img src=\"x.png\"> b"),
            "a b",
            "an image has no text"
        );
        // A lone `<` is text.
        assert_eq!(plain_text("1 < 2"), "1 < 2");
    }

    #[test]
    fn a_tooltip_is_its_title_then_its_text() {
        let tip = |title: &str, text: &str| ToolTip {
            icon: ItemIcon::default(),
            title: title.to_owned(),
            text: text.to_owned(),
        };
        assert_eq!(tooltip_text(None), "");
        assert_eq!(tooltip_text(Some(&tip("Chat", ""))), "Chat");
        assert_eq!(tooltip_text(Some(&tip("", "2 unread"))), "2 unread");
        assert_eq!(
            tooltip_text(Some(&tip("Chat", "<b>2</b> unread"))),
            "Chat: 2 unread"
        );
    }

    fn info(name: &str) -> ItemInfo {
        ItemInfo {
            id: "app".to_owned(),
            title: String::new(),
            icon: ItemIcon {
                name: name.to_owned(),
                pixmaps: Vec::new(),
            },
            ..ItemInfo::default()
        }
    }

    #[test]
    fn an_item_view_maps_the_status_and_names_the_item() {
        let mut resolver = Resolver::new();
        let mut item = info("");
        item.status = Status::NeedsAttention;
        item.tooltip = Some(ToolTip {
            icon: ItemIcon::default(),
            title: "Mail".to_owned(),
            text: String::new(),
        });
        let view = resolver.view(":1.5/Item", &item);
        assert_eq!(view.id, ":1.5/Item");
        assert_eq!(view.title, "app", "the id names an item without a title");
        assert_eq!(view.status, TrayStatus::NeedsAttention);
        assert_eq!(view.tooltip, "Mail");
        assert!(view.icon.is_none(), "no name and no pixmap: a placeholder");
        item.title = "Mail".to_owned();
        assert_eq!(resolver.view(":1.5/Item", &item).title, "Mail");
    }

    #[test]
    fn a_pixmap_is_used_when_the_name_is_not_in_any_theme() {
        let mut resolver = Resolver::new();
        let mut item = info("no-such-icon-anywhere-3-13c");
        assert!(resolver.view("a/b", &item).icon.is_none());
        item.icon.pixmaps = vec![Pixmap {
            width: 2,
            height: 2,
            rgba: vec![255; 16],
        }];
        assert!(resolver.view("a/b", &item).icon.is_some());
    }

    #[test]
    fn a_name_is_found_in_the_items_own_directory() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        // A 1x1 PNG, as libappindicator apps drop loose icons in their `IconThemePath`.
        let png = {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("a header");
            writer
                .write_image_data(&[10, 20, 30, 255])
                .expect("a pixel");
            writer.finish().expect("a PNG");
            bytes
        };
        std::fs::write(dir.path().join("only-here-3-13c.png"), png).expect("the file");
        let mut item = info("only-here-3-13c");
        let mut resolver = Resolver::new();
        assert!(resolver.view("a/b", &item).icon.is_none());
        item.icon_theme_path = Some(dir.path().to_string_lossy().into_owned());
        assert!(resolver.view("a/b", &item).icon.is_some());
    }

    #[test]
    fn menus_are_read_one_at_a_time_and_again_when_they_changed_meanwhile() {
        let conn = neue_dbus::testing::pair().expect("a peer pair").0;
        let (tx, rx) = mpsc::channel();
        let mut menus = Menus {
            host: Host::new(&conn),
            tx,
            paths: HashMap::from([("a/b".to_owned(), "/Menu".to_owned())]),
            fetching: HashSet::new(),
            again: HashSet::new(),
        };
        menus.request("a/b");
        // A second request while the first is out waits for it.
        menus.request("a/b");
        assert!(menus.again.contains("a/b"));
        // The first answers (the host doesn't know the item, so with no menu); the one
        // that waited goes out and its answer comes too.
        let Input::Fetched { id, nodes } = rx.recv().expect("an answer") else {
            panic!("a fetch result");
        };
        assert_eq!((id.as_str(), nodes), ("a/b", None));
        assert!(menus.fetched("a/b"));
        assert!(menus.fetching.contains("a/b") && menus.again.is_empty());
        assert!(matches!(rx.recv().expect("another"), Input::Fetched { .. }));
        // An item that left meanwhile isn't wanted.
        menus.removed("a/b");
        assert!(!menus.fetched("a/b"));
    }

    /// Run by hand against the mock tray app on a private session bus (docs/dev-setup.md
    /// §1.6d): the real worker owns the watcher, shows the item with its icon, tooltip
    /// and menu, and its actions reach the app.
    #[test]
    #[ignore = "needs a session bus with mock_sni_item on it (docs/dev-setup.md §1.6d)"]
    fn the_worker_shows_the_mock_item_and_acts_on_it() {
        use std::time::{Duration, Instant};

        let (updates, rx) = mpsc::channel();
        let (given, host) = mpsc::channel();
        std::thread::spawn(move || {
            run(|h| given.send(h).is_ok(), |u| updates.send(u).is_ok()).expect("the worker");
        });
        let host = host
            .recv_timeout(Duration::from_secs(10))
            .expect("the host");
        let wait = |want: &dyn Fn(&Update) -> bool| -> Update {
            let until = Instant::now() + Duration::from_secs(10);
            loop {
                let left = until.saturating_duration_since(Instant::now());
                let update = rx.recv_timeout(left).expect("an update in time");
                if want(&update) {
                    return update;
                }
            }
        };
        let Update::Item(item) = wait(&|u| matches!(u, Update::Item(_))) else {
            unreachable!("waited for an item");
        };
        let id = item.view.id.clone();
        assert_eq!(item.view.title, "Mock Tray");
        assert!(item.has_menu && !item.item_is_menu);
        assert!(item.view.icon.is_some(), "the pixmap became a picture");
        assert!(!item.view.tooltip.is_empty());
        println!("item {item:?}");

        let Update::Menu { nodes, .. } = wait(&|u| matches!(u, Update::Menu { .. })) else {
            unreachable!("waited for a menu");
        };
        let labels: Vec<_> = nodes.iter().map(|n| n.label.as_str()).collect();
        println!("menu {labels:?}");
        assert!(labels.contains(&"Open"));

        host.activate(&id, 0, 0).expect("Activate");
        host.context_menu(&id, 0, 0).expect("ContextMenu");
        let attention = nodes
            .iter()
            .find(|n| n.label.contains("Attention"))
            .expect("the attention item");
        host.menu(&id)
            .expect("a menu client")
            .expect("a menu")
            .event(attention.id, "clicked")
            .expect("clicked");
        let Update::Item(item) =
            wait(&|u| matches!(u, Update::Item(i) if i.view.status == TrayStatus::NeedsAttention))
        else {
            unreachable!("waited for an item");
        };
        assert!(item.view.attention_icon.is_some());
    }
}
