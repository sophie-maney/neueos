//! `shell.toml`: settings of `neue-shell` (task 3.2c; notifications, task 3.15a; the dock,
//! tasks 3.16 and 3.17a). The menu bar and the rest join this file with their tasks.

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use crate::ConfigFile;
use crate::compositor::in_range;

/// Everything in `shell.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ShellConfig {
    /// `[desktop]`: the background.
    pub desktop: DesktopConfig,
    /// `[notifications]`: banners and Do Not Disturb.
    pub notifications: NotificationsConfig,
    /// `[dock]`: the apps kept in the dock, its edge, size and magnification.
    pub dock: DockConfig,
}

/// `[dock]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DockConfig {
    /// The apps kept in the dock, left to right: desktop file IDs
    /// (`org.gnome.Nautilus.desktop`, the suffix optional) or the app ids of their windows
    /// (`foot`). Apps that aren't installed are left out. Default: a terminal (`foot`),
    /// until neueOS's own apps arrive.
    pub pinned: Vec<String>,
    /// The screen edge the dock stands on: `"bottom"` (the default), `"left"` or
    /// `"right"`. Read when the shell starts.
    pub edge: DockEdge,
    /// The icons' size in logical pixels, 16 to 128. Default 48.
    #[serde(deserialize_with = "dock_size")]
    pub size: u32,
    /// Whether icons grow under the pointer. Default `false`. Off while motion is reduced.
    pub magnification: bool,
    /// The size icons grow to under the pointer with `magnification` on, in logical
    /// pixels, 16 to 256; no bigger than `size` means no magnification. Default 96.
    #[serde(deserialize_with = "magnified_size")]
    pub magnified_size: u32,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            pinned: vec!["foot".to_owned()],
            edge: DockEdge::Bottom,
            size: 48,
            magnification: false,
            magnified_size: 96,
        }
    }
}

/// The screen edge the dock stands on (`[dock] edge`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockEdge {
    #[default]
    Bottom,
    Left,
    Right,
}

fn dock_size<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    in_range(d, "size", 16, 128)
}

fn magnified_size<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    in_range(d, "magnified-size", 16, 256)
}

impl ConfigFile for ShellConfig {
    const FILE_NAME: &'static str = "shell.toml";
}

/// `[desktop]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DesktopConfig {
    /// A PNG or JPEG file to show on every output instead of the built-in picture. A leading
    /// `~/` is the home directory. Unset (the default), or a file that can't be read or
    /// decoded, shows the built-in picture.
    pub picture: Option<PathBuf>,
    /// How the picture is placed: `"fill"` (the default), `"fit"`, `"stretch"` or
    /// `"center"`.
    pub fit: PictureFit,
}

/// How a desktop picture is placed on an output (`[desktop] fit`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PictureFit {
    /// Scaled to cover the output, keeping its proportions; the overflow is cut off.
    #[default]
    Fill,
    /// Scaled to fit inside the output, keeping its proportions; the rest is black.
    Fit,
    /// Scaled to the output's size, whatever its proportions.
    Stretch,
    /// At its own size, in the middle.
    Center,
}

/// `[notifications]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct NotificationsConfig {
    /// Do Not Disturb: notifications arrive but no banner or alert shows. Off by default;
    /// the notifications extra in the menu bar turns it on and off.
    pub do_not_disturb: bool,
    /// Milliseconds a banner stays up when the app leaves the time to us (1000 to 60000,
    /// default 5000). Critical notifications and those the app asks to keep stay until
    /// dismissed.
    #[serde(deserialize_with = "banner_timeout")]
    pub banner_timeout: u32,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            do_not_disturb: false,
            banner_timeout: 5000,
        }
    }
}

fn banner_timeout<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    in_range(d, "banner-timeout", 1000, 60_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layers, load};

    #[test]
    fn defaults_show_the_built_in_picture() {
        let config = load::<ShellConfig>(&Layers::none()).unwrap().config;
        assert_eq!(config.desktop.picture, None);
        assert_eq!(config.desktop.fit, PictureFit::Fill);
        assert!(!config.notifications.do_not_disturb);
        assert_eq!(config.notifications.banner_timeout, 5000);
    }

    #[test]
    fn notifications_load_and_check_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shell.toml");
        std::fs::write(
            &file,
            "[notifications]\ndo-not-disturb = true\nbanner-timeout = 8000\n",
        )
        .unwrap();
        let layers = Layers::from_dirs([dir.path().to_owned()]);
        let config = load::<ShellConfig>(&layers).unwrap().config;
        assert!(config.notifications.do_not_disturb);
        assert_eq!(config.notifications.banner_timeout, 8000);
        std::fs::write(&file, "[notifications]\nbanner-timeout = 10\n").unwrap();
        assert!(load::<ShellConfig>(&layers).is_err());
    }

    #[test]
    fn the_dock_keeps_a_terminal_unless_told_otherwise() {
        let config = load::<ShellConfig>(&Layers::none()).unwrap().config;
        assert_eq!(config.dock.pinned, ["foot"]);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("shell.toml"),
            "[dock]\npinned = [\"org.gnome.Nautilus.desktop\", \"foot\"]\n",
        )
        .unwrap();
        let config = load::<ShellConfig>(&Layers::from_dirs([dir.path().to_owned()]))
            .unwrap()
            .config;
        assert_eq!(config.dock.pinned, ["org.gnome.Nautilus.desktop", "foot"]);
    }

    #[test]
    fn the_dock_stands_at_the_bottom_unmagnified_unless_told_otherwise() {
        let config = load::<ShellConfig>(&Layers::none()).unwrap().config.dock;
        assert_eq!(config.edge, DockEdge::Bottom);
        assert_eq!(config.size, 48);
        assert!(!config.magnification);
        assert_eq!(config.magnified_size, 96);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shell.toml");
        let layers = Layers::from_dirs([dir.path().to_owned()]);
        std::fs::write(
            &file,
            "[dock]\nedge = \"left\"\nsize = 40\nmagnification = true\nmagnified-size = 128\n",
        )
        .unwrap();
        let config = load::<ShellConfig>(&layers).unwrap().config.dock;
        assert_eq!(config.edge, DockEdge::Left);
        assert_eq!((config.size, config.magnified_size), (40, 128));
        assert!(config.magnification);
        std::fs::write(&file, "[dock]\nsize = 300\n").unwrap();
        assert!(load::<ShellConfig>(&layers).is_err());
        std::fs::write(&file, "[dock]\nedge = \"top\"\n").unwrap();
        assert!(load::<ShellConfig>(&layers).is_err());
    }

    #[test]
    fn a_picture_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("shell.toml"),
            "[desktop]\npicture = \"~/Pictures/a.png\"\nfit = \"center\"\n",
        )
        .unwrap();
        let config = load::<ShellConfig>(&Layers::from_dirs([dir.path().to_owned()]))
            .unwrap()
            .config;
        assert_eq!(config.desktop.picture, Some("~/Pictures/a.png".into()));
        assert_eq!(config.desktop.fit, PictureFit::Center);
    }

    #[test]
    fn an_unknown_fit_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shell.toml"), "[desktop]\nfit = \"tile\"\n").unwrap();
        assert!(load::<ShellConfig>(&Layers::from_dirs([dir.path().to_owned()])).is_err());
    }
}
