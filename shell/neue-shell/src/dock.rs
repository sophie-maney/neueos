//! What the dock shows and what a click on it does (task 3.16, ADR 0092): the apps kept in
//! it (`shell.toml`'s `[dock] pinned`), then every other app with windows, each one tile;
//! which tiles run, launch and ask for attention; and which window a click activates.
//!
//! A window belongs to the app whose desktop entry its app id finds
//! ([`AppDatabase::find_by_app_id`]); a window whose app id finds none is its app id's own
//! tile, and one without an app id is a tile of its own. The process side (the surface,
//! icons, launching) is `src/dock_host.rs`.
//!
//! After the apps come the dock's other section (task 3.17c, ADR 0095): a divider, a tile
//! for each minimised window (a click restores it) and the Trash, empty or full. These
//! tiles stay where they are; only the apps can be dragged.
//!
//! Editing (task 3.17b, ADR 0094): an app with a desktop entry can be kept in the dock
//! ([`Dock::pin`]), let go ([`Dock::unpin`]), or dragged to another place, which keeps it
//! there ([`Dock::move_tile`]). The kept apps stay in front of the others, so a tile
//! dropped among those lands after the last kept one. Entries of `pinned` that aren't
//! installed stay where they were, after the kept app before them.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use neue_desktop::{AppDatabase, DesktopEntry, TrashState};
use neue_kit::desktop::{Toplevel, ToplevelId};
use neue_kit::tokens::materials::dock::LAUNCH_TIMEOUT;

use crate::windows::{Windows, app_name};

/// What a tile is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileKind {
    /// An app, kept in the dock or running.
    #[default]
    App,
    /// A minimised window: its tile restores it.
    Minimised(ToplevelId),
    /// The Trash, empty or full.
    Trash(TrashState),
    /// The line between the apps and the rest.
    Divider,
}

/// The key of the divider's tile.
pub const DIVIDER_KEY: &str = "divider";
/// The key of the Trash's tile.
pub const TRASH_KEY: &str = "trash";

/// One tile of the dock.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tile {
    pub kind: TileKind,
    /// Its key: the desktop file ID without `.desktop`, else the windows' app id, else the
    /// window's identifier.
    pub key: String,
    pub name: String,
    /// The theme icon name or path from the desktop entry.
    pub icon: Option<String>,
    /// The desktop file ID to launch, when the app has an entry.
    pub entry: Option<String>,
    /// Its windows, oldest first.
    pub windows: Vec<ToplevelId>,
    pub pinned: bool,
    /// Launched from the dock and no window yet.
    pub launching: bool,
    /// A window of the app asks for attention.
    pub attention: bool,
}

impl Tile {
    pub fn running(&self) -> bool {
        !self.windows.is_empty()
    }
}

/// What a click on a tile asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Click {
    /// Start the app with this desktop file ID (its tile starts bouncing).
    Launch(String),
    /// Activate this window (restoring it if minimised).
    Activate(ToplevelId),
    /// Open the Trash.
    OpenTrash,
    /// Nothing to do (the tile is gone, or it is the divider).
    Nothing,
}

/// The dock's state beyond the window list.
#[derive(Debug, Default)]
pub struct Dock {
    pinned: Vec<String>,
    /// Apps launched from the dock that have no window yet, by key, and when they give up.
    launching: HashMap<String, Instant>,
    /// The window of each app that was active last, by key.
    last_active: HashMap<String, ToplevelId>,
    /// The Trash's state; `None`: there is no Trash to show.
    trash: Option<TrashState>,
}

impl Dock {
    /// Keeping the apps `pinned` (desktop file IDs or app ids, `[dock] pinned`).
    pub fn new(pinned: Vec<String>) -> Self {
        Self {
            pinned,
            ..Self::default()
        }
    }

    /// Shows the Trash as `state` (`None`: no Trash tile). Returns whether that changed.
    pub fn set_trash(&mut self, state: Option<TrashState>) -> bool {
        std::mem::replace(&mut self.trash, state) != state
    }

    /// The Trash's state, if it is shown.
    pub fn trash(&self) -> Option<TrashState> {
        self.trash
    }

    /// The tiles, left to right: the pinned apps that are installed, then the other apps
    /// with windows in the order their first window opened; then, set apart by a divider,
    /// a tile for each minimised window (oldest first) and the Trash.
    pub fn tiles(&self, windows: &Windows, apps: &AppDatabase) -> Vec<Tile> {
        let mut tiles: Vec<Tile> = Vec::new();
        for wanted in &self.pinned {
            let Some(entry) = apps.by_id(wanted).or_else(|| apps.find_by_app_id(wanted)) else {
                continue;
            };
            let key = entry_key(entry);
            if tiles.iter().any(|t| t.key == key) {
                continue;
            }
            tiles.push(Tile {
                pinned: true,
                ..entry_tile(entry)
            });
        }
        // Window ids grow, so the window list's order is the order they opened in.
        for (id, window) in windows.iter() {
            let (key, entry) = window_key(id, window, apps);
            let i = match tiles.iter().position(|t| t.key == key) {
                Some(i) => i,
                None => {
                    tiles.push(entry.map_or_else(
                        || Tile {
                            key,
                            name: app_name(window),
                            ..Tile::default()
                        },
                        entry_tile,
                    ));
                    tiles.len() - 1
                }
            };
            let tile = &mut tiles[i];
            tile.windows.push(id);
            tile.attention |= window.attention;
        }
        for tile in &mut tiles {
            tile.launching = !tile.running() && self.launching.contains_key(&tile.key);
        }
        let minimised: Vec<Tile> = windows
            .iter()
            .filter(|(_, window)| window.minimized)
            .map(|(id, window)| minimised_tile(id, window, apps))
            .collect();
        if !minimised.is_empty() || self.trash.is_some() {
            tiles.push(Tile {
                kind: TileKind::Divider,
                key: DIVIDER_KEY.to_owned(),
                ..Tile::default()
            });
        }
        tiles.extend(minimised);
        if let Some(state) = self.trash {
            tiles.push(Tile {
                kind: TileKind::Trash(state),
                key: TRASH_KEY.to_owned(),
                name: "Trash".to_owned(),
                icon: Some(
                    match state {
                        TrashState::Empty => "user-trash",
                        TrashState::Full => "user-trash-full",
                    }
                    .to_owned(),
                ),
                ..Tile::default()
            });
        }
        tiles
    }

    /// What a click on tile `key` does: launch the app if it has no window; otherwise
    /// activate its window that was active last, or, if one of its windows is active
    /// already, the next one (so clicks cycle through them).
    pub fn click(&mut self, key: &str, windows: &Windows, apps: &AppDatabase) -> Click {
        let Some(tile) = self.tiles(windows, apps).into_iter().find(|t| t.key == key) else {
            return Click::Nothing;
        };
        match tile.kind {
            TileKind::App => {}
            TileKind::Minimised(window) => return Click::Activate(window),
            TileKind::Trash(_) => return Click::OpenTrash,
            TileKind::Divider => return Click::Nothing,
        }
        if !tile.running() {
            return match tile.entry {
                Some(entry) => Click::Launch(entry),
                None => Click::Nothing,
            };
        }
        let active = windows.active_id();
        if let Some(i) = tile.windows.iter().position(|&w| Some(w) == active) {
            return Click::Activate(tile.windows[(i + 1) % tile.windows.len()]);
        }
        let last = self
            .last_active
            .get(key)
            .filter(|w| tile.windows.contains(w));
        Click::Activate(*last.unwrap_or(&tile.windows[tile.windows.len() - 1]))
    }

    /// Tile `key` was launched at `now`: it bounces until a window appears or
    /// [`LAUNCH_TIMEOUT`] passes.
    pub fn launched(&mut self, key: &str, now: Instant) {
        self.launching.insert(key.to_owned(), now + LAUNCH_TIMEOUT);
    }

    /// Tile `key`'s launch failed: it stops bouncing.
    pub fn cancel_launch(&mut self, key: &str) {
        self.launching.remove(key);
    }

    /// Follows the window list: remembers each app's active window, and ends launches
    /// that have a window or ran out of time by `now`. Returns whether a launch ended.
    pub fn update(&mut self, windows: &Windows, apps: &AppDatabase, now: Instant) -> bool {
        if let Some((id, window)) = windows.active_id().zip(windows.active()) {
            let (key, _) = window_key(id, window, apps);
            self.last_active.insert(key, id);
        }
        self.last_active
            .retain(|_, id| windows.iter().any(|(w, _)| w == *id));
        let running: Vec<String> = windows
            .iter()
            .map(|(id, w)| window_key(id, w, apps).0)
            .collect();
        let before = self.launching.len();
        self.launching
            .retain(|key, until| now < *until && !running.contains(key));
        self.launching.len() != before
    }

    /// When the next launch gives up, if any is under way.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.launching.values().min().copied()
    }

    /// The apps kept in the dock, as `[dock] pinned` lists them.
    pub fn pinned(&self) -> &[String] {
        &self.pinned
    }

    /// Keeps tile `key`'s app in the dock, after the apps kept already (where it is
    /// shown). Returns whether that changed anything; an app without a desktop entry can't
    /// be kept.
    pub fn pin(&mut self, key: &str, windows: &Windows, apps: &AppDatabase) -> bool {
        let tiles = self.tiles(windows, apps);
        if !tiles.iter().any(|t| t.key == key && t.entry.is_some()) {
            return false;
        }
        let mut kept = kept_keys(&tiles);
        if !kept.iter().any(|k| k == key) {
            kept.push(key.to_owned());
        }
        self.keep(&kept, apps)
    }

    /// Lets tile `key`'s app go from the dock: its tile stays while it has windows.
    /// Returns whether that changed anything.
    pub fn unpin(&mut self, key: &str, windows: &Windows, apps: &AppDatabase) -> bool {
        let mut kept = kept_keys(&self.tiles(windows, apps));
        kept.retain(|k| k != key);
        self.keep(&kept, apps)
    }

    /// Tile `key` was dragged to index `to` of the tiles as shown: its app is kept in the
    /// dock there, among the kept apps (after the last one before `to`). Returns whether
    /// that changed anything; an app without a desktop entry stays where it was.
    pub fn move_tile(
        &mut self,
        key: &str,
        to: usize,
        windows: &Windows,
        apps: &AppDatabase,
    ) -> bool {
        let tiles = self.tiles(windows, apps);
        let Some(from) = tiles.iter().position(|t| t.key == key && t.entry.is_some()) else {
            return false;
        };
        let mut order: Vec<&Tile> = tiles.iter().collect();
        let moved = order.remove(from);
        order.insert(to.min(order.len()), moved);
        let kept: Vec<String> = order
            .into_iter()
            .filter(|t| t.pinned || t.key == key)
            .map(|t| t.key.clone())
            .collect();
        self.keep(&kept, apps)
    }

    /// Makes `kept` (tile keys, in order) the apps kept in the dock. Entries already in
    /// `pinned` keep their spelling, and those that aren't installed stay after the kept
    /// app before them; new ones are written as their key. Returns whether `pinned`
    /// changed.
    fn keep(&mut self, kept: &[String], apps: &AppDatabase) -> bool {
        // Entries not installed before any that is, then each installed one (its key and
        // entries) followed by the ones not installed after it.
        let mut lead = Vec::new();
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for wanted in &self.pinned {
            match pinned_key(wanted, apps) {
                // A second entry for the same app is left out.
                Some(key) if groups.iter().any(|(k, _)| *k == key) => {}
                Some(key) => groups.push((key, vec![wanted.clone()])),
                None => match groups.last_mut() {
                    Some((_, entries)) => entries.push(wanted.clone()),
                    None => lead.push(wanted.clone()),
                },
            }
        }
        let mut pinned = lead;
        for key in kept {
            match groups.iter_mut().find(|(k, _)| k == key) {
                Some((_, entries)) => pinned.append(entries),
                None => pinned.push(key.clone()),
            }
        }
        // Apps let go: the entries not installed after them stay, at the end.
        for (_, entries) in groups {
            pinned.extend(entries.into_iter().skip(1));
        }
        let changed = pinned != self.pinned;
        self.pinned = pinned;
        changed
    }
}

/// The keys of the kept tiles, in order.
fn kept_keys(tiles: &[Tile]) -> Vec<String> {
    tiles
        .iter()
        .filter(|t| t.pinned)
        .map(|t| t.key.clone())
        .collect()
}

/// The tile key an entry of `pinned` stands for, if its app is installed.
fn pinned_key(wanted: &str, apps: &AppDatabase) -> Option<String> {
    apps.by_id(wanted)
        .or_else(|| apps.find_by_app_id(wanted))
        .map(entry_key)
}

/// The tile index each window minimises to: `(window, tile)`. A window goes to its app's
/// tile, and once minimised to its own tile, where it is restored from.
pub fn minimize_targets(tiles: &[Tile]) -> BTreeMap<ToplevelId, usize> {
    let mut targets: BTreeMap<ToplevelId, usize> = tiles
        .iter()
        .enumerate()
        .flat_map(|(i, tile)| tile.windows.iter().map(move |&w| (w, i)))
        .collect();
    for (i, tile) in tiles.iter().enumerate() {
        if let TileKind::Minimised(window) = tile.kind {
            targets.insert(window, i);
        }
    }
    targets
}

/// The tile of the minimised `window`: named for its title, with its app's icon.
fn minimised_tile(id: ToplevelId, window: &Toplevel, apps: &AppDatabase) -> Tile {
    let (_, entry) = window_key(id, window, apps);
    let name = if window.title.trim().is_empty() {
        app_name(window)
    } else {
        window.title.clone()
    };
    let key = if window.identifier.is_empty() {
        format!("minimised-{id:?}")
    } else {
        format!("minimised-{}", window.identifier)
    };
    Tile {
        kind: TileKind::Minimised(id),
        key,
        name,
        icon: entry.and_then(|e| e.icon.clone()),
        ..Tile::default()
    }
}

fn entry_key(entry: &DesktopEntry) -> String {
    entry
        .id
        .strip_suffix(".desktop")
        .unwrap_or(&entry.id)
        .to_owned()
}

fn entry_tile(entry: &DesktopEntry) -> Tile {
    Tile {
        key: entry_key(entry),
        name: entry.name.clone(),
        icon: entry.icon.clone(),
        entry: Some(entry.id.clone()),
        ..Tile::default()
    }
}

/// The key of `window`'s tile, and its app's entry if it has one.
fn window_key<'a>(
    id: ToplevelId,
    window: &Toplevel,
    apps: &'a AppDatabase,
) -> (String, Option<&'a DesktopEntry>) {
    if let Some(entry) = apps.find_by_app_id(&window.app_id) {
        return (entry_key(entry), Some(entry));
    }
    let key = if window.app_id.is_empty() {
        if window.identifier.is_empty() {
            format!("{id:?}")
        } else {
            window.identifier.clone()
        }
    } else {
        window.app_id.clone()
    };
    (key, None)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use std::time::Duration;

    use neue_desktop::Locale;
    use neue_kit::desktop::ShellEvent;

    use super::*;

    fn apps() -> (tempfile::TempDir, AppDatabase) {
        let dir = tempfile::tempdir().unwrap();
        let entry = |file: &str, name: &str, extra: &str| {
            std::fs::write(
                dir.path().join(file),
                format!("[Desktop Entry]\nType=Application\nName={name}\nExec={name}\nIcon={name}-icon\n{extra}"),
            )
            .unwrap();
        };
        entry(
            "org.codeberg.dnkl.foot.desktop",
            "Foot",
            "StartupWMClass=foot\n",
        );
        entry("org.gnome.Nautilus.desktop", "Files", "");
        let db = AppDatabase::load(&[dir.path().to_owned()], &Locale::none());
        (dir, db)
    }

    fn window(app_id: &str, n: u64) -> Toplevel {
        Toplevel {
            identifier: format!("window-{n}"),
            app_id: app_id.to_owned(),
            ..Toplevel::default()
        }
    }

    fn open(windows: &mut Windows, n: u64, w: Toplevel) -> ToplevelId {
        let id = ToplevelId::from_raw(n);
        windows.apply(ShellEvent::Toplevel(id, w));
        id
    }

    #[test]
    fn pinned_apps_come_first_then_running_ones() {
        let (_dir, apps) = apps();
        let dock = Dock::new(vec![
            "org.gnome.Nautilus.desktop".into(),
            "foot".into(),
            "not-installed".into(),
        ]);
        let mut windows = Windows::default();
        let a = open(&mut windows, 1, window("org.example.Chat", 1));
        let b = open(&mut windows, 2, window("foot", 2));
        let c = open(&mut windows, 3, window("", 3));
        let tiles = dock.tiles(&windows, &apps);
        let keys: Vec<_> = tiles.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "org.gnome.Nautilus",
                "org.codeberg.dnkl.foot",
                "org.example.Chat",
                "window-3"
            ]
        );
        assert!(!tiles[0].running() && tiles[0].pinned);
        assert_eq!(tiles[1].windows, [b]);
        assert_eq!(tiles[1].icon.as_deref(), Some("Foot-icon"));
        assert_eq!(tiles[2].windows, [a]);
        assert_eq!(tiles[2].name, "Chat");
        assert!(!tiles[2].pinned && tiles[2].entry.is_none());
        assert_eq!(tiles[3].windows, [c]);
        assert_eq!(
            minimize_targets(&tiles),
            BTreeMap::from([(a, 2), (b, 1), (c, 3)])
        );
    }

    #[test]
    fn a_launch_bounces_until_a_window_opens_or_time_runs_out() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["foot".into(), "org.gnome.Nautilus".into()]);
        let mut windows = Windows::default();
        let now = Instant::now();
        assert_eq!(
            dock.click("org.codeberg.dnkl.foot", &windows, &apps),
            Click::Launch("org.codeberg.dnkl.foot.desktop".into())
        );
        dock.launched("org.codeberg.dnkl.foot", now);
        dock.launched("org.gnome.Nautilus", now);
        assert!(dock.tiles(&windows, &apps).iter().all(|t| t.launching));
        assert_eq!(dock.next_deadline(), Some(now + LAUNCH_TIMEOUT));

        open(&mut windows, 1, window("foot", 1));
        assert!(dock.update(&windows, &apps, now + Duration::from_secs(1)));
        let tiles = dock.tiles(&windows, &apps);
        assert!(!tiles[0].launching && tiles[0].running());
        assert!(tiles[1].launching);
        assert!(!dock.update(&windows, &apps, now + Duration::from_secs(2)));
        assert!(dock.update(&windows, &apps, now + LAUNCH_TIMEOUT));
        assert!(!dock.tiles(&windows, &apps)[1].launching);
        assert_eq!(dock.next_deadline(), None);
    }

    #[test]
    fn clicks_activate_the_last_active_window_then_cycle() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(Vec::new());
        let mut windows = Windows::default();
        let a = open(&mut windows, 1, window("foot", 1));
        let b = open(&mut windows, 2, window("foot", 2));
        let c = open(&mut windows, 3, window("foot", 3));
        let other = open(&mut windows, 4, window("org.gnome.Nautilus", 4));
        let key = "org.codeberg.dnkl.foot";
        let now = Instant::now();
        // Never active: the newest.
        assert_eq!(dock.click(key, &windows, &apps), Click::Activate(c));
        windows.apply(ShellEvent::Active(Some(a)));
        dock.update(&windows, &apps, now);
        windows.apply(ShellEvent::Active(Some(other)));
        dock.update(&windows, &apps, now);
        // Another app is active: the one active last.
        assert_eq!(dock.click(key, &windows, &apps), Click::Activate(a));
        // Its own window is active: the next, wrapping round.
        windows.apply(ShellEvent::Active(Some(a)));
        assert_eq!(dock.click(key, &windows, &apps), Click::Activate(b));
        windows.apply(ShellEvent::Active(Some(c)));
        assert_eq!(dock.click(key, &windows, &apps), Click::Activate(a));
        assert_eq!(dock.click("gone", &windows, &apps), Click::Nothing);
    }

    fn keys(dock: &Dock, windows: &Windows, apps: &AppDatabase) -> Vec<String> {
        dock.tiles(windows, apps)
            .into_iter()
            .map(|t| t.key)
            .collect()
    }

    const FOOT: &str = "org.codeberg.dnkl.foot";
    const FILES: &str = "org.gnome.Nautilus";

    #[test]
    fn a_dragged_tile_is_kept_where_it_lands() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec![
            "gone-before".into(),
            "foot".into(),
            "gone-after".into(),
            "org.gnome.Nautilus.desktop".into(),
        ]);
        let windows = Windows::default();
        assert_eq!(keys(&dock, &windows, &apps), [FOOT, FILES]);
        assert!(dock.move_tile(FOOT, 1, &windows, &apps));
        assert_eq!(keys(&dock, &windows, &apps), [FILES, FOOT]);
        // Spellings kept; what isn't installed stays after the app it followed.
        assert_eq!(
            dock.pinned(),
            [
                "gone-before",
                "org.gnome.Nautilus.desktop",
                "foot",
                "gone-after"
            ]
        );
        // Dropped where it is: nothing changes.
        assert!(!dock.move_tile(FOOT, 1, &windows, &apps));
        assert!(!dock.move_tile("nothing", 0, &windows, &apps));
    }

    #[test]
    fn a_running_app_dragged_into_the_dock_is_kept_there() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["org.gnome.Nautilus".into()]);
        let mut windows = Windows::default();
        open(&mut windows, 1, window("foot", 1));
        open(&mut windows, 2, window("org.example.Chat", 2));
        assert_eq!(
            keys(&dock, &windows, &apps),
            [FILES, FOOT, "org.example.Chat"]
        );
        // In front of Files.
        assert!(dock.move_tile(FOOT, 0, &windows, &apps));
        assert_eq!(dock.pinned(), [FOOT, "org.gnome.Nautilus"]);
        let tiles = dock.tiles(&windows, &apps);
        assert!(tiles[0].pinned && tiles[0].key == FOOT);
        // A kept app dropped after a running one lands after the last kept one.
        assert!(dock.move_tile(FOOT, 2, &windows, &apps));
        assert_eq!(
            keys(&dock, &windows, &apps),
            [FILES, FOOT, "org.example.Chat"]
        );
        assert_eq!(dock.pinned(), ["org.gnome.Nautilus", FOOT]);
        // An app without a desktop entry can't be kept.
        assert!(!dock.move_tile("org.example.Chat", 0, &windows, &apps));
        assert!(!dock.pin("org.example.Chat", &windows, &apps));
    }

    #[test]
    fn keep_in_dock_pins_and_unpins() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["missing".into(), "org.gnome.Nautilus".into()]);
        let mut windows = Windows::default();
        open(&mut windows, 1, window("foot", 1));
        assert!(dock.pin(FOOT, &windows, &apps));
        assert!(!dock.pin(FOOT, &windows, &apps));
        assert_eq!(dock.pinned(), ["missing", "org.gnome.Nautilus", FOOT]);
        // Let go while running: the tile stays, no longer kept, after the kept ones.
        assert!(dock.unpin(FILES, &windows, &apps));
        assert_eq!(dock.pinned(), ["missing", FOOT]);
        assert!(dock.unpin(FOOT, &windows, &apps));
        assert_eq!(dock.pinned(), ["missing"]);
        let tiles = dock.tiles(&windows, &apps);
        assert_eq!(tiles.len(), 1);
        assert!(!tiles[0].pinned && tiles[0].running());
        assert!(!dock.unpin(FOOT, &windows, &apps));
    }

    #[test]
    fn a_tile_dragged_off_and_quit_leaves() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["foot".into(), "org.gnome.Nautilus".into()]);
        let mut windows = Windows::default();
        let id = open(&mut windows, 1, window("foot", 1));
        // Removing a running app lets it go; its tile goes when its window does.
        assert!(dock.unpin(FOOT, &windows, &apps));
        assert_eq!(keys(&dock, &windows, &apps), [FILES, FOOT]);
        windows.apply(ShellEvent::Closed(id));
        assert_eq!(keys(&dock, &windows, &apps), [FILES]);
    }

    #[test]
    fn attention_marks_the_tile() {
        let (_dir, apps) = apps();
        let dock = Dock::new(Vec::new());
        let mut windows = Windows::default();
        open(&mut windows, 1, window("foot", 1));
        open(
            &mut windows,
            2,
            Toplevel {
                attention: true,
                ..window("foot", 2)
            },
        );
        assert!(dock.tiles(&windows, &apps)[0].attention);
    }

    fn minimised(app_id: &str, n: u64, title: &str) -> Toplevel {
        Toplevel {
            minimized: true,
            title: title.to_owned(),
            ..window(app_id, n)
        }
    }

    #[test]
    fn minimised_windows_and_the_trash_follow_a_divider() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["foot".into()]);
        let mut windows = Windows::default();
        // Nothing but the apps.
        let tiles = dock.tiles(&windows, &apps);
        assert_eq!(tiles.len(), 1);
        let a = open(&mut windows, 1, window("foot", 1));
        let b = open(&mut windows, 2, minimised("foot", 2, "notes"));
        let c = open(&mut windows, 3, minimised("org.example.Chat", 3, " "));
        assert!(dock.set_trash(Some(TrashState::Full)));
        assert!(!dock.set_trash(Some(TrashState::Full)));
        let tiles = dock.tiles(&windows, &apps);
        let keys: Vec<_> = tiles.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "org.codeberg.dnkl.foot",
                // The chat's window has no entry: its own app tile, before the divider.
                "org.example.Chat",
                DIVIDER_KEY,
                "minimised-window-2",
                "minimised-window-3",
                TRASH_KEY
            ]
        );
        assert_eq!(tiles[2].kind, TileKind::Divider);
        assert_eq!(tiles[3].kind, TileKind::Minimised(b));
        assert_eq!(tiles[3].name, "notes");
        assert_eq!(tiles[3].icon.as_deref(), Some("Foot-icon"));
        // A blank title falls back to the app's name.
        assert_eq!(tiles[4].name, "Chat");
        assert_eq!(tiles[5].kind, TileKind::Trash(TrashState::Full));
        assert_eq!(tiles[5].icon.as_deref(), Some("user-trash-full"));
        // The app is still running with its minimised window.
        assert_eq!(tiles[0].windows, [a, b]);
        // Minimised windows go to their own tiles, the others to their apps'.
        assert_eq!(
            minimize_targets(&tiles),
            BTreeMap::from([(a, 0), (b, 3), (c, 4)])
        );
    }

    #[test]
    fn the_trash_alone_is_set_apart_and_goes_with_no_trash() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["foot".into()]);
        let windows = Windows::default();
        dock.set_trash(Some(TrashState::Empty));
        let kinds: Vec<_> = dock.tiles(&windows, &apps).iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            [
                TileKind::App,
                TileKind::Divider,
                TileKind::Trash(TrashState::Empty)
            ]
        );
        assert!(dock.set_trash(None));
        assert_eq!(dock.tiles(&windows, &apps).len(), 1);
        assert_eq!(dock.trash(), None);
    }

    #[test]
    fn clicks_restore_minimised_windows_and_open_the_trash() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(Vec::new());
        let mut windows = Windows::default();
        let b = open(&mut windows, 2, minimised("foot", 2, "notes"));
        dock.set_trash(Some(TrashState::Empty));
        assert_eq!(
            dock.click("minimised-window-2", &windows, &apps),
            Click::Activate(b)
        );
        assert_eq!(dock.click(TRASH_KEY, &windows, &apps), Click::OpenTrash);
        assert_eq!(dock.click(DIVIDER_KEY, &windows, &apps), Click::Nothing);
    }

    #[test]
    fn editing_the_apps_ignores_the_other_section() {
        let (_dir, apps) = apps();
        let mut dock = Dock::new(vec!["foot".into(), "org.gnome.Nautilus".into()]);
        let mut windows = Windows::default();
        open(&mut windows, 1, minimised("foot", 1, "notes"));
        dock.set_trash(Some(TrashState::Full));
        // Dropped on the Trash, the app is kept after the last kept app.
        assert!(dock.move_tile("org.codeberg.dnkl.foot", 5, &windows, &apps));
        assert_eq!(dock.pinned(), ["org.gnome.Nautilus", "foot"]);
        // The Trash, the divider and minimised windows can't be kept.
        assert!(!dock.pin(TRASH_KEY, &windows, &apps));
        assert!(!dock.move_tile(TRASH_KEY, 0, &windows, &apps));
        assert!(!dock.move_tile("minimised-window-1", 0, &windows, &apps));
        assert_eq!(dock.pinned(), ["org.gnome.Nautilus", "foot"]);
    }
}
