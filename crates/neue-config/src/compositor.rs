//! `compositor.toml`: input, keybindings and outputs of `neue-comp` (ADR 0004).
//!
//! Window management, hot corners and Overview settings join this file with their tasks.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use toml::{Table, Value};

use crate::{Chord, ConfigFile};

/// The value that removes a (default) key binding.
pub const UNBOUND: &str = "none";

/// Everything in `compositor.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CompositorConfig {
    /// `[keyboard]`: layouts and key repeat.
    pub keyboard: KeyboardConfig,
    /// `[pointer]`: scrolling.
    pub pointer: PointerConfig,
    /// `[touchpad]`: tapping, clicking and gestures.
    pub touchpad: TouchpadConfig,
    /// `[windows]`: title-bar behaviour.
    pub windows: WindowsConfig,
    /// `[super-as-command]`: Super acting as ⌘ in third-party apps.
    pub super_as_command: SuperAsCommandConfig,
    /// `[keybindings]`: key chord (`"Super+Q"`) to action name. Action names are those of
    /// `neuectl actions`. The value `"none"` removes a binding that a lower layer (or the
    /// built-in defaults) made.
    pub keybindings: BTreeMap<Chord, String>,
    /// `[outputs."<name>"]`: per-output settings, keyed by connector name (`neuectl outputs`).
    pub outputs: BTreeMap<String, OutputConfig>,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            keyboard: KeyboardConfig::default(),
            pointer: PointerConfig::default(),
            touchpad: TouchpadConfig::default(),
            windows: WindowsConfig::default(),
            super_as_command: SuperAsCommandConfig::default(),
            keybindings: BTreeMap::from([
                (
                    Chord {
                        logo: true,
                        ctrl: false,
                        alt: false,
                        shift: false,
                        key: "Q".to_owned(),
                    },
                    "quit".to_owned(),
                ),
                // The menu bar from the keyboard (task 3.7b).
                (
                    Chord {
                        logo: false,
                        ctrl: true,
                        alt: false,
                        shift: false,
                        key: "F2".to_owned(),
                    },
                    "focus-menu-bar".to_owned(),
                ),
            ]),
            outputs: BTreeMap::new(),
        }
    }
}

impl ConfigFile for CompositorConfig {
    const FILE_NAME: &'static str = "compositor.toml";

    /// Rewrites `[keybindings]` keys to their canonical chord spelling.
    fn normalize(layer: &mut Table) {
        let Some(Value::Table(bindings)) = layer.get_mut("keybindings") else {
            return;
        };
        for (key, value) in std::mem::take(bindings) {
            let key = key.parse::<Chord>().map_or(key, |chord| chord.to_string());
            bindings.insert(key, value);
        }
    }
}

/// `[keyboard]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct KeyboardConfig {
    /// xkb layouts, in the order they switch (`["us", "de"]`). Switching needs an xkb
    /// option such as `grp:alt_shift_toggle` in `options`.
    #[serde(deserialize_with = "xkb_names")]
    pub layouts: Vec<String>,
    /// xkb variant for each layout, by position (`["", "nodeadkeys"]`); missing entries
    /// mean the layout's default variant.
    #[serde(deserialize_with = "xkb_variants")]
    pub variants: Vec<String>,
    /// xkb keyboard model (`pc105`); empty for the default.
    #[serde(deserialize_with = "xkb_model")]
    pub model: String,
    /// xkb options (`["ctrl:nocaps", "grp:alt_shift_toggle"]`).
    #[serde(deserialize_with = "xkb_names")]
    pub options: Vec<String>,
    /// Milliseconds a key is held before it repeats (1 to 5000).
    #[serde(deserialize_with = "repeat_delay")]
    pub repeat_delay: u32,
    /// Repeats per second once repeating (0 turns repeat off; at most 200).
    #[serde(deserialize_with = "repeat_rate")]
    pub repeat_rate: u32,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            layouts: vec!["us".to_owned()],
            variants: Vec::new(),
            model: String::new(),
            options: Vec::new(),
            repeat_delay: 300,
            repeat_rate: 30,
        }
    }
}

impl KeyboardConfig {
    /// `layouts` as xkb's comma-separated `layout` string.
    pub fn xkb_layout(&self) -> String {
        self.layouts.join(",")
    }

    /// `variants` as xkb's comma-separated `variant` string.
    pub fn xkb_variant(&self) -> String {
        self.variants.join(",")
    }

    /// `options` as xkb's comma-separated `options` string.
    pub fn xkb_options(&self) -> String {
        self.options.join(",")
    }
}

/// `[super-as-command]` (PLAN.md Q1, ADR 0079): Super is the command key (⌘). neue-kit
/// apps take Super themselves; other apps get Super+key chords as Ctrl+key, so Super+C
/// copies and Super+T opens a tab in them too. Compositor key bindings come first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SuperAsCommandConfig {
    /// Turn Super into Ctrl for apps outside the exceptions. Off: every app gets Super as
    /// it is.
    pub enabled: bool,
    /// App ids (compared ignoring case) that get Super as it is: terminals by default,
    /// where Ctrl+C interrupts and copying is Ctrl+Shift+C. An app's id is the `app-id`
    /// column of `neuectl windows`. The list replaces the default one.
    pub exceptions: Vec<String>,
}

impl Default for SuperAsCommandConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            exceptions: [
                "foot",
                "footclient",
                "Alacritty",
                "kitty",
                "org.wezfurlong.wezterm",
                "com.mitchellh.ghostty",
                "org.gnome.Console",
                "org.gnome.Ptyxis",
                "org.gnome.Terminal",
                "org.kde.konsole",
                "xfce4-terminal",
                "XTerm",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

impl SuperAsCommandConfig {
    /// Whether Super+key reaches the app `app_id` (empty when unset) as Ctrl+key.
    pub fn remaps(&self, app_id: &str) -> bool {
        self.enabled
            && !self
                .exceptions
                .iter()
                .any(|exempt| exempt.eq_ignore_ascii_case(app_id))
    }
}

/// `[pointer]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PointerConfig {
    /// Scroll the content the way the fingers move (reverses the scroll direction).
    pub natural_scroll: bool,
    /// Multiplier for scroll distance (0.1 to 10).
    #[serde(deserialize_with = "scroll_factor")]
    pub scroll_factor: f64,
}

impl Default for PointerConfig {
    fn default() -> Self {
        Self {
            natural_scroll: false,
            scroll_factor: 1.0,
        }
    }
}

/// `[touchpad]`. Applied to every touchpad libinput reports; the other settings of a
/// device (acceleration, scroll method) stay at libinput's defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TouchpadConfig {
    /// A light tap clicks (one finger left, two right, three middle).
    pub tap_to_click: bool,
    /// Tap, then touch again and move to drag.
    pub tap_and_drag: bool,
    /// Keep dragging for a moment after the finger lifts, until the next tap.
    pub drag_lock: bool,
    /// Ignore the touchpad while typing.
    pub disable_while_typing: bool,
    /// How a physical click is told left from right: `"clickfinger"` (the number of
    /// fingers on the pad; the default) or `"button-areas"` (the bottom corners).
    pub click_method: ClickMethod,
    /// Send multi-finger swipe, pinch and hold gestures to the focused app. Off drops them
    /// (the compositor does not use them yet).
    pub gestures: bool,
}

impl Default for TouchpadConfig {
    fn default() -> Self {
        Self {
            tap_to_click: true,
            tap_and_drag: true,
            drag_lock: false,
            disable_while_typing: true,
            click_method: ClickMethod::default(),
            gestures: true,
        }
    }
}

/// How a touchpad click is classified (`[touchpad] click-method`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClickMethod {
    /// By the number of fingers resting on the pad.
    #[default]
    Clickfinger,
    /// By where on the pad the finger is.
    ButtonAreas,
}

/// `[windows]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WindowsConfig {
    /// What a double click on a server-side title bar does: `"zoom"` (the default),
    /// `"minimize"` or `"none"`.
    pub title_bar_double_click: TitleBarDoubleClick,
}

/// The action of a double click on a title bar (`[windows] title-bar-double-click`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleBarDoubleClick {
    /// Zoom the window, or restore it if it is zoomed.
    #[default]
    Zoom,
    /// Minimise the window.
    Minimize,
    /// Do nothing.
    None,
}

/// `[outputs."<name>"]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct OutputConfig {
    /// Scale factor (0.5 to 4). Unset: the backend's choice (1).
    #[serde(deserialize_with = "output_scale")]
    pub scale: Option<f64>,
    /// Top-left corner in the global layout, in logical pixels (`[1920, 0]`), each
    /// coordinate between -32768 and 32767. Unset: placed to the right of the outputs that
    /// are already laid out, top edges aligned.
    #[serde(deserialize_with = "output_position")]
    pub position: Option<[i32; 2]>,
    /// Variable refresh rate (FreeSync, VESA Adaptive-Sync): `off` (default), `on`, or
    /// `fullscreen` (only while a full-screen window covers the output). Ignored, with a
    /// warning in the log, on displays or drivers that cannot do it.
    pub adaptive_sync: AdaptiveSync,
}

/// `[outputs."<name>"] adaptive-sync`: when an output runs at a variable refresh rate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdaptiveSync {
    /// Always the mode's fixed refresh rate.
    #[default]
    Off,
    /// Always variable, where the display supports it.
    On,
    /// Variable while a full-screen window (a game, a video) covers the output, fixed
    /// otherwise. Some displays flicker at low frame rates; this keeps the desktop clear of
    /// that.
    Fullscreen,
}

fn xkb_names<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    if let Some(bad) = names
        .iter()
        .find(|n| n.is_empty() || n.contains([',', ' ']))
    {
        return Err(serde::de::Error::custom(format!(
            "`{bad}` is not a valid xkb name (not empty, no commas or spaces)"
        )));
    }
    Ok(names)
}

fn xkb_model<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let model = String::deserialize(d)?;
    if model.contains([',', ' ']) {
        return Err(serde::de::Error::custom(format!(
            "`{model}` is not a valid xkb model (no commas or spaces)"
        )));
    }
    Ok(model)
}

/// Like [`xkb_names`], but empty entries are allowed (they mean "default").
fn xkb_variants<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    if let Some(bad) = names.iter().find(|n| n.contains([',', ' '])) {
        return Err(serde::de::Error::custom(format!(
            "`{bad}` is not a valid xkb name (no commas or spaces)"
        )));
    }
    Ok(names)
}

pub(crate) fn in_range<'de, D, T>(d: D, what: &str, min: T, max: T) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + PartialOrd + std::fmt::Display,
{
    let value = T::deserialize(d)?;
    if value < min || value > max || value.partial_cmp(&value).is_none() {
        return Err(serde::de::Error::custom(format!(
            "{what} must be between {min} and {max}, not {value}"
        )));
    }
    Ok(value)
}

fn repeat_delay<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    in_range(d, "repeat-delay", 1, 5000)
}

fn repeat_rate<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    in_range(d, "repeat-rate", 0, 200)
}

fn scroll_factor<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    in_range(d, "scroll-factor", 0.1, 10.0)
}

fn output_scale<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    let scale = Option::<f64>::deserialize(d)?;
    match scale {
        Some(s) if !(0.5..=4.0).contains(&s) => Err(serde::de::Error::custom(format!(
            "scale must be between 0.5 and 4, not {s}"
        ))),
        scale => Ok(scale),
    }
}

fn output_position<'de, D: Deserializer<'de>>(d: D) -> Result<Option<[i32; 2]>, D::Error> {
    let position = Option::<[i32; 2]>::deserialize(d)?;
    match position {
        Some(p) if p.iter().any(|c| !(-32768..=32767).contains(c)) => {
            Err(serde::de::Error::custom(format!(
                "position must be between -32768 and 32767, not [{}, {}]",
                p[0], p[1]
            )))
        }
        position => Ok(position),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layers, load};

    fn load_text(text: &str) -> Result<crate::Loaded<CompositorConfig>, crate::ConfigError> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("compositor.toml"), text).unwrap();
        load(&Layers::from_dirs([dir.path().to_owned()]))
    }

    #[test]
    fn defaults_have_super_q() {
        let config = load::<CompositorConfig>(&Layers::none()).unwrap().config;
        assert_eq!(config, CompositorConfig::default());
        assert_eq!(
            config.keybindings[&"Super+Q".parse::<Chord>().unwrap()],
            "quit"
        );
        assert_eq!(config.keyboard.xkb_layout(), "us");
    }

    #[test]
    fn touchpad_settings_load() {
        let loaded = load_text(
            "[touchpad]\ntap-to-click = false\ndrag-lock = true\nclick-method = \"button-areas\"\ngestures = false\n",
        )
        .unwrap();
        let t = loaded.config.touchpad;
        assert!(!t.tap_to_click && t.drag_lock && !t.gestures);
        assert!(t.tap_and_drag && t.disable_while_typing);
        assert_eq!(t.click_method, ClickMethod::ButtonAreas);
        assert!(load_text("[touchpad]\nclick-method = \"nope\"\n").is_err());
    }

    #[test]
    fn keyboard_and_pointer_settings_load() {
        let loaded = load_text(
            r#"
[keyboard]
layouts = ["us", "de"]
variants = ["", "nodeadkeys"]
options = ["ctrl:nocaps", "grp:alt_shift_toggle"]
repeat-delay = 200
repeat-rate = 40

[pointer]
natural-scroll = true
scroll-factor = 2.5

[outputs."HDMI-A-1"]
scale = 1.5
position = [-1280, 0]
adaptive-sync = "fullscreen"
"#,
        )
        .unwrap();
        let c = loaded.config;
        assert_eq!(c.keyboard.xkb_layout(), "us,de");
        assert_eq!(c.keyboard.xkb_variant(), ",nodeadkeys");
        assert_eq!(c.keyboard.xkb_options(), "ctrl:nocaps,grp:alt_shift_toggle");
        assert_eq!((c.keyboard.repeat_delay, c.keyboard.repeat_rate), (200, 40));
        assert!(c.pointer.natural_scroll);
        assert_eq!(c.pointer.scroll_factor, 2.5);
        assert_eq!(c.touchpad, TouchpadConfig::default());
        assert_eq!(c.outputs["HDMI-A-1"].scale, Some(1.5));
        assert_eq!(c.outputs["HDMI-A-1"].position, Some([-1280, 0]));
        assert_eq!(
            c.outputs["HDMI-A-1"].adaptive_sync,
            AdaptiveSync::Fullscreen
        );
        assert_eq!(c.windows.title_bar_double_click, TitleBarDoubleClick::Zoom);
        assert!(loaded.unknown_keys.is_empty(), "{:?}", loaded.unknown_keys);
    }

    #[test]
    fn super_as_command_exempts_terminals_and_can_be_changed() {
        let defaults = SuperAsCommandConfig::default();
        assert!(defaults.remaps("firefox"));
        assert!(defaults.remaps(""));
        assert!(!defaults.remaps("foot"));
        assert!(!defaults.remaps("xterm"), "case is ignored");

        let c = load_text("[super-as-command]\nexceptions = [\"org.gnome.TextEditor\"]\n")
            .unwrap()
            .config
            .super_as_command;
        assert!(c.remaps("foot"), "the list replaces the default one");
        assert!(!c.remaps("org.gnome.TextEditor"));

        let off = load_text("[super-as-command]\nenabled = false\n")
            .unwrap()
            .config
            .super_as_command;
        assert!(!off.remaps("firefox"));
    }

    #[test]
    fn title_bar_double_click_loads_and_rejects_typos() {
        let c = load_text("[windows]\ntitle-bar-double-click = \"minimize\"\n")
            .unwrap()
            .config;
        assert_eq!(
            c.windows.title_bar_double_click,
            TitleBarDoubleClick::Minimize
        );
        assert!(load_text("[windows]\ntitle-bar-double-click = \"shade\"\n").is_err());
    }

    #[test]
    fn keybindings_merge_with_the_defaults_and_can_unbind() {
        let c = load_text(
            "[keybindings]\n\"super+shift+w\" = \"close-window\"\n\"Super+Q\" = \"none\"\n",
        )
        .unwrap()
        .config;
        let q = "Super+Q".parse::<Chord>().unwrap();
        let w = "Super+Shift+W".parse::<Chord>().unwrap();
        assert_eq!(c.keybindings[&q], UNBOUND);
        assert_eq!(c.keybindings[&w], "close-window");
    }

    #[test]
    fn two_spellings_across_layers_are_one_binding() {
        let root = tempfile::tempdir().unwrap();
        let (a, b) = (root.path().join("a"), root.path().join("b"));
        for (dir, text) in [
            (&a, "[keybindings]\n\"Super+Q\" = \"close-window\"\n"),
            (&b, "[keybindings]\n\"logo+q\" = \"quit\"\n"),
        ] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("compositor.toml"), text).unwrap();
        }
        let c = load::<CompositorConfig>(&Layers::from_dirs([a, b]))
            .unwrap()
            .config;
        let q = "Super+Q".parse::<Chord>().unwrap();
        // The other default (Ctrl+F2) plus the one merged Super+Q.
        assert_eq!(c.keybindings.len(), 2);
        assert_eq!(c.keybindings[&q], "quit");
    }

    #[test]
    fn bad_values_are_rejected_with_a_line() {
        for (text, line) in [
            ("[keyboard]\nrepeat-delay = 0\n", 2),
            ("\n[keyboard]\nrepeat-rate = 500\n", 3),
            ("[keyboard]\nlayouts = [\"us,de\"]\n", 2),
            ("[pointer]\nscroll-factor = 0.0\n", 2),
            ("[pointer]\nscroll-factor = nan\n", 2),
            ("[outputs.X]\nscale = 9\n", 2),
            ("[outputs.X]\nposition = [0, 40000]\n", 2),
            ("[outputs.X]\nposition = [0]\n", 2),
            ("[outputs.X]\nadaptive-sync = \"auto\"\n", 2),
            ("[outputs.X]\nadaptive-sync = true\n", 2),
            ("[keybindings]\n\"Hyper+Q\" = \"quit\"\n", 2),
            ("[keybindings]\n\"Super+Q\" = 5\n", 2),
        ] {
            let err = load_text(text).unwrap_err();
            assert_eq!(err.line, Some(line), "{text:?}: {err}");
        }
    }

    #[test]
    fn unknown_keys_warn() {
        let loaded = load_text("[keyboard]\nlayoutz = [\"us\"]\n[futuristic]\nx = 1\n").unwrap();
        assert_eq!(loaded.unknown_keys, ["futuristic", "keyboard.layoutz"]);
    }
}
