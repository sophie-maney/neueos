//! The installed applications: every desktop entry under the `applications` directories,
//! and matching a window's app id to its entry.

use std::fs;
use std::path::{Path, PathBuf};

use crate::entry::{DesktopEntry, EntryType};
use crate::keyfile::Locale;

/// Directories nest this deep at most (`applications/kde/foo.desktop` is one level).
const MAX_DEPTH: usize = 4;

/// The `applications` directories of [`crate::all_data_dirs`], in priority order.
pub fn default_application_dirs() -> Vec<PathBuf> {
    crate::all_data_dirs()
        .into_iter()
        .map(|d| d.join("applications"))
        .collect()
}

/// All desktop entries found, one per desktop file ID (the highest-priority directory wins,
/// and a `Hidden` entry there hides the ones below it).
#[derive(Clone, Debug, Default)]
pub struct AppDatabase {
    entries: Vec<DesktopEntry>,
}

impl AppDatabase {
    /// Reads the `applications` directories `dirs`, highest priority first. A directory that
    /// does not exist is skipped; a file that does not parse is skipped with a warning.
    pub fn load(dirs: &[PathBuf], locale: &Locale) -> Self {
        let mut entries: Vec<DesktopEntry> = Vec::new();
        for dir in dirs {
            let mut found = Vec::new();
            collect(dir, dir, 0, &mut found);
            found.sort();
            for (id, path) in found {
                if entries.iter().any(|e| e.id == id) {
                    continue;
                }
                match DesktopEntry::from_file(&id, &path, locale) {
                    Ok(entry) => entries.push(entry),
                    Err(error) => tracing::warn!(%error, "skipping a desktop entry"),
                }
            }
        }
        Self { entries }
    }

    /// Every entry, including hidden and `NoDisplay` ones.
    pub fn all(&self) -> &[DesktopEntry] {
        &self.entries
    }

    /// The entries a menu or the launcher shows in the desktop(s) `current`, by name.
    pub fn listed<'a>(&'a self, current: &'a [&str]) -> impl Iterator<Item = &'a DesktopEntry> {
        let mut shown: Vec<&DesktopEntry> = self
            .entries
            .iter()
            .filter(|e| e.is_listed(current))
            .collect();
        shown.sort_by_cached_key(|e| e.name.to_lowercase());
        shown.into_iter()
    }

    /// The entry with this desktop file ID (`org.gnome.Nautilus.desktop`; the suffix is
    /// optional).
    pub fn by_id(&self, id: &str) -> Option<&DesktopEntry> {
        let id = id.strip_suffix(".desktop").unwrap_or(id);
        self.applications().find(|e| strip(&e.id) == id)
    }

    /// The entry a window with `app_id` belongs to, or `None`. The first rule that matches
    /// wins; each is tried over all entries before the next:
    ///
    /// 1. the desktop file ID is the app id (`org.mozilla.firefox`, also a Flatpak id);
    /// 2. `X-Flatpak` is the app id;
    /// 3. `StartupWMClass` is the app id;
    /// 4. the same three, ignoring case (`Firefox` ↔ `firefox.desktop`);
    /// 5. the app id is the last part of a reverse-DNS file ID (`nautilus` ↔
    ///    `org.gnome.Nautilus.desktop`), or the file ID is the last part of a reverse-DNS
    ///    app id;
    /// 6. the program in `Exec` is the app id;
    /// 7. `Name` is the app id.
    ///
    /// Hidden entries and entries that are not applications never match.
    pub fn find_by_app_id(&self, app_id: &str) -> Option<&DesktopEntry> {
        if app_id.is_empty() {
            return None;
        }
        let lower = app_id.to_lowercase();
        let last = |s: &str| s.rsplit('.').next().unwrap_or(s).to_lowercase();
        type Rule<'a> = Box<dyn Fn(&DesktopEntry) -> bool + 'a>;
        let rules: [Rule<'_>; 11] = [
            Box::new(|e| strip(&e.id) == app_id),
            Box::new(|e| e.flatpak_id.as_deref() == Some(app_id)),
            Box::new(|e| e.startup_wm_class.as_deref() == Some(app_id)),
            Box::new(|e| strip(&e.id).to_lowercase() == lower),
            Box::new(|e| {
                e.flatpak_id
                    .as_deref()
                    .is_some_and(|f| f.to_lowercase() == lower)
            }),
            Box::new(|e| {
                e.startup_wm_class
                    .as_deref()
                    .is_some_and(|c| c.to_lowercase() == lower)
            }),
            Box::new(|e| strip(&e.id).contains('.') && last(strip(&e.id)) == lower),
            Box::new(|e| app_id.contains('.') && strip(&e.id).to_lowercase() == last(app_id)),
            Box::new(|e| e.exec_program().is_some_and(|p| p.to_lowercase() == lower)),
            Box::new(|e| e.name.to_lowercase() == lower),
            // Last resort: the desktop file ID after its vendor prefix (`kde-konsole`).
            Box::new(|e| {
                strip(&e.id).rsplit('-').next().map(str::to_lowercase) == Some(lower.clone())
            }),
        ];
        rules
            .iter()
            .find_map(|rule| self.applications().find(|e| rule(e)))
    }

    fn applications(&self) -> impl Iterator<Item = &DesktopEntry> {
        self.entries
            .iter()
            .filter(|e| e.kind == EntryType::Application && !e.hidden)
    }
}

impl DesktopEntry {
    /// The program `Exec` starts, as its file name: `/usr/bin/env FOO=1 app` is `env`, and
    /// so is `flatpak run org.x.App` `flatpak`; the match against an app id only uses it as
    /// a late guess.
    fn exec_program(&self) -> Option<String> {
        let argv = self.exec_argv(None, &[]).ok()?;
        let program = argv.first()?;
        Path::new(program)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    }
}

fn strip(id: &str) -> &str {
    id.strip_suffix(".desktop").unwrap_or(id)
}

/// Finds `*.desktop` files under `root`: `(desktop file ID, path)`. Symlinks to files are
/// followed (Flatpak and Nix export that way); symlinked directories are not.
fn collect(root: &Path, dir: &Path, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        let Ok(kind) = item.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                collect(root, &path, depth + 1, out);
            }
        } else if path.extension().is_some_and(|e| e == "desktop") && path.is_file() {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let id = relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("-");
            out.push((id, path));
        }
    }
}
