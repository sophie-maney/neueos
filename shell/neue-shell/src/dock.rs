//! What the dock shows and what a click on it does (task 3.16, ADR 0092): the apps kept in
//! it (`shell.toml`'s `[dock] pinned`), then every other app with windows, each one tile;
//! which tiles run, launch and ask for attention; and which window a click activates.
//!
//! A window belongs to the app whose desktop entry its app id finds
//! ([`AppDatabase::find_by_app_id`]); a window whose app id finds none is its app id's own
//! tile, and one without an app id is a tile of its own. The process side (the surface,
//! icons, launching) is `src/dock_host.rs`.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use neue_desktop::{AppDatabase, DesktopEntry};
use neue_kit::desktop::{Toplevel, ToplevelId};
use neue_kit::tokens::materials::dock::LAUNCH_TIMEOUT;

use crate::windows::{Windows, app_name};

/// One tile of the dock.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tile {
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
    /// Nothing to do (the tile is gone).
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
}

impl Dock {
    /// Keeping the apps `pinned` (desktop file IDs or app ids, `[dock] pinned`).
    pub fn new(pinned: Vec<String>) -> Self {
        Self {
            pinned,
            ..Self::default()
        }
    }

    /// The tiles, left to right: the pinned apps that are installed, then the other apps
    /// with windows in the order their first window opened.
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
        tiles
    }

    /// What a click on tile `key` does: launch the app if it has no window; otherwise
    /// activate its window that was active last, or, if one of its windows is active
    /// already, the next one (so clicks cycle through them).
    pub fn click(&mut self, key: &str, windows: &Windows, apps: &AppDatabase) -> Click {
        let Some(tile) = self.tiles(windows, apps).into_iter().find(|t| t.key == key) else {
            return Click::Nothing;
        };
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
}

/// The tile index each window minimises to: `(window, tile)`.
pub fn minimize_targets(tiles: &[Tile]) -> BTreeMap<ToplevelId, usize> {
    tiles
        .iter()
        .enumerate()
        .flat_map(|(i, tile)| tile.windows.iter().map(move |&w| (w, i)))
        .collect()
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
}
