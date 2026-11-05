//! freedesktop specs for neueOS (task 3.3, ADR 0070): the parts of the desktop that the dock,
//! launcher, search and the apps share. Portable: it only reads files and starts processes,
//! so it builds and tests on macOS as well as Linux.
//!
//! - [`entry`]: desktop entries (`.desktop` files): parsing, localisation, actions,
//!   `NoDisplay`, `OnlyShowIn`, and the `Exec` field codes.
//! - [`apps`]: the installed applications, and matching a window's app id to its entry.
//! - [`icon_theme`]: icon theme lookup by name, size and scale (`index.theme`, inheritance,
//!   Papirus then hicolor as fallbacks).
//! - [`icon_load`]: loading the file found into a [`neue_paint::icon::Icon`] (PNG as it is,
//!   SVG rasterised at the size it will be drawn).
//! - [`launch`]: starting an entry or one of its actions, with an `XDG_ACTIVATION_TOKEN`.
//! - [`trash`]: the state of the trash (empty or full) and emptying it.
//!
//! Nothing here logs except `tracing` warnings for files that cannot be used; problems come
//! back as errors or as `None`.

pub mod apps;
pub mod entry;
pub mod icon_load;
pub mod icon_theme;
mod keyfile;
pub mod launch;
pub mod trash;

pub use apps::AppDatabase;
pub use entry::{Action, DesktopEntry, EntryError, EntryType};
pub use icon_load::{IconLoadError, IconLoader};
pub use icon_theme::{IconFile, IconFormat, IconLookup};
pub use keyfile::Locale;
pub use launch::{LaunchError, LaunchOptions};
pub use trash::{Trash, TrashState};

use std::env;
use std::path::PathBuf;

/// `$XDG_DATA_HOME`, or `~/.local/share` (the spec's default).
pub fn data_home() -> Option<PathBuf> {
    absolute_env("XDG_DATA_HOME").or_else(|| env::home_dir().map(|h| h.join(".local/share")))
}

/// `$XDG_DATA_DIRS`, or `/usr/local/share:/usr/share` (the spec's default). Relative
/// entries are ignored, as the spec says.
pub fn data_dirs() -> Vec<PathBuf> {
    let value = env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty());
    let value = value.as_deref().unwrap_or("/usr/local/share:/usr/share");
    value
        .split(':')
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .collect()
}

/// The data directories in priority order: the user's, then the system's, then the Flatpak
/// exports (which `XDG_DATA_DIRS` has when Flatpak's profile script ran, but a bare session
/// may not).
pub fn all_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(data_home());
    dirs.extend(data_dirs());
    if let Some(home) = data_home() {
        dirs.push(home.join("flatpak/exports/share"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    let mut seen = Vec::new();
    dirs.retain(|d| {
        let new = !seen.contains(d);
        seen.push(d.clone());
        new
    });
    dirs
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    let value = env::var_os(name)?;
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}
