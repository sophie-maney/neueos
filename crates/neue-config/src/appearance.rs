//! `appearance.toml`: the look every component shares (ADR 0004, task 3.6): light or dark,
//! accent, contrast, reduced motion, fonts and the cursor. `neue-settingsd` derives GTK's
//! `settings.ini` and the `org.gnome.desktop.*` dconf keys from it (ADR 0074).

use serde::{Deserialize, Deserializer, Serialize};

use crate::ConfigFile;
use crate::compositor::in_range;

/// Everything in `appearance.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppearanceConfig {
    /// `"light"` (the default) or `"dark"`.
    pub mode: Mode,
    /// The accent colour of buttons, selection and focus rings: `"blue"` (the default) or
    /// `"graphite"`.
    pub accent: Accent,
    /// `"standard"` (the default) or `"increased"`: darker edges, separators and secondary
    /// text in light mode, brighter ones in dark mode.
    pub contrast: Contrast,
    /// Replace sliding and zooming animations with fades or nothing. Off by default.
    pub reduced_motion: bool,
    /// `[fonts]`: the interface and monospace fonts.
    pub fonts: FontsConfig,
    /// `[cursor]`: the pointer's XCursor theme and size.
    pub cursor: CursorConfig,
}

impl ConfigFile for AppearanceConfig {
    const FILE_NAME: &'static str = "appearance.toml";
}

impl AppearanceConfig {
    /// The colour variant `mode`, `accent` and `contrast` select.
    pub fn variant(&self) -> neue_theme::Variant {
        neue_theme::Variant {
            appearance: match self.mode {
                Mode::Light => neue_theme::Appearance::Light,
                Mode::Dark => neue_theme::Appearance::Dark,
            },
            accent: match self.accent {
                Accent::Blue => neue_theme::Accent::Blue,
                Accent::Graphite => neue_theme::Accent::Graphite,
            },
            contrast: match self.contrast {
                Contrast::Standard => neue_theme::Contrast::Standard,
                Contrast::Increased => neue_theme::Contrast::Increased,
            },
        }
    }

    /// The toolkit theme: [`Self::variant`] plus `reduced-motion`.
    pub fn theme(&self) -> neue_theme::Theme {
        neue_theme::Theme {
            variant: self.variant(),
            reduced_motion: self.reduced_motion,
        }
    }
}

/// Light or dark (`mode`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

/// The accent colour (`accent`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Accent {
    /// The classic blue gel.
    #[default]
    Blue,
    /// Blue-grey, for a quieter desktop.
    Graphite,
}

/// The contrast level (`contrast`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Contrast {
    #[default]
    Standard,
    Increased,
}

/// `[fonts]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FontsConfig {
    /// Family of the interface font. Default `"DejaVu Sans"` (ADR 0038).
    pub ui: String,
    /// Size of the interface font in points, 6 to 72. Default 10.
    #[serde(deserialize_with = "font_size")]
    pub ui_size: f64,
    /// Family of the monospace font. Default `"DejaVu Sans Mono"`.
    pub monospace: String,
    /// Size of the monospace font in points, 6 to 72. Default 10.
    #[serde(deserialize_with = "font_size")]
    pub monospace_size: f64,
}

impl Default for FontsConfig {
    fn default() -> Self {
        Self {
            ui: "DejaVu Sans".to_owned(),
            ui_size: 10.0,
            monospace: "DejaVu Sans Mono".to_owned(),
            monospace_size: 10.0,
        }
    }
}

/// `[cursor]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CursorConfig {
    /// XCursor theme name. Default `"default"` (ADR 0012).
    pub theme: String,
    /// Nominal size in logical pixels, 8 to 256. Default 24.
    #[serde(deserialize_with = "cursor_size")]
    pub size: u32,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            theme: "default".to_owned(),
            size: 24,
        }
    }
}

fn font_size<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    in_range(d, "font size", 6.0, 72.0)
}

fn cursor_size<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    in_range(d, "cursor size", 8, 256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layers, load};

    fn load_text(text: &str) -> Result<AppearanceConfig, crate::ConfigError> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("appearance.toml"), text).unwrap();
        load::<AppearanceConfig>(&Layers::from_dirs([dir.path().to_owned()])).map(|l| l.config)
    }

    #[test]
    fn defaults_are_light_blue_and_dejavu() {
        let config = load::<AppearanceConfig>(&Layers::none()).unwrap().config;
        assert_eq!(config.mode, Mode::Light);
        assert_eq!(config.accent, Accent::Blue);
        assert_eq!(config.fonts.ui, "DejaVu Sans");
        assert_eq!(config.cursor.size, 24);
    }

    #[test]
    fn the_theme_follows_the_file() {
        let theme = load_text(
            "mode = \"dark\"\naccent = \"graphite\"\ncontrast = \"increased\"\nreduced-motion = true\n",
        )
        .unwrap()
        .theme();
        assert!(theme.variant.is_dark());
        assert_eq!(theme.variant.accent, neue_theme::Accent::Graphite);
        assert_eq!(theme.variant.contrast, neue_theme::Contrast::Increased);
        assert!(theme.reduced_motion);
        assert_eq!(
            AppearanceConfig::default().theme(),
            neue_theme::Theme::default()
        );
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let config = load_text("mode = \"dark\"\n[fonts]\nui-size = 12.5\n").unwrap();
        assert_eq!(config.mode, Mode::Dark);
        assert_eq!(config.fonts.ui_size, 12.5);
        assert_eq!(config.fonts.ui, "DejaVu Sans");
    }

    #[test]
    fn out_of_range_and_unknown_values_are_errors() {
        assert!(load_text("mode = \"dim\"\n").is_err());
        assert!(load_text("[cursor]\nsize = 4\n").is_err());
        assert!(load_text("[fonts]\nui-size = 100.0\n").is_err());
    }
}
