//! `shell.toml`: settings of `neue-shell` (task 3.2c). The menu bar, dock and the rest join
//! this file with their tasks.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ConfigFile;

/// Everything in `shell.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ShellConfig {
    /// `[desktop]`: the background.
    pub desktop: DesktopConfig,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layers, load};

    #[test]
    fn defaults_show_the_built_in_picture() {
        let config = load::<ShellConfig>(&Layers::none()).unwrap().config;
        assert_eq!(config.desktop.picture, None);
        assert_eq!(config.desktop.fit, PictureFit::Fill);
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
