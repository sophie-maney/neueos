//! Setting names: the file's name without `.toml`, then the key's dotted path in that file,
//! in TOML's dotted-key syntax: `appearance.mode`, `compositor.pointer.natural-scroll`,
//! `compositor.keybindings."Super+Q"`.

use std::fmt;

use neue_config::{AppearanceConfig, CompositorConfig, ConfigFile, ShellConfig};

/// A config file served as keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum File {
    Appearance,
    Compositor,
    Shell,
}

impl File {
    /// Every served file.
    pub const ALL: [File; 3] = [File::Appearance, File::Compositor, File::Shell];

    /// The key prefix: the file name without `.toml`.
    pub fn prefix(self) -> &'static str {
        match self {
            File::Appearance => "appearance",
            File::Compositor => "compositor",
            File::Shell => "shell",
        }
    }

    /// The file name, e.g. `appearance.toml`.
    pub fn file_name(self) -> &'static str {
        match self {
            File::Appearance => AppearanceConfig::FILE_NAME,
            File::Compositor => CompositorConfig::FILE_NAME,
            File::Shell => ShellConfig::FILE_NAME,
        }
    }

    fn from_prefix(prefix: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.prefix() == prefix)
    }
}

/// A parsed setting name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub file: File,
    /// The path inside the file; never empty.
    pub path: Vec<String>,
}

impl Key {
    /// Parse `name`. `None` if it isn't a dotted key, names no served file, or names the
    /// whole file.
    pub fn parse(name: &str) -> Option<Self> {
        let mut parts = toml_edit::Key::parse(name)
            .ok()?
            .into_iter()
            .map(|k| k.get().to_owned());
        let file = File::from_prefix(&parts.next()?)?;
        let path: Vec<String> = parts.collect();
        (!path.is_empty()).then_some(Self { file, path })
    }

    /// The path as `neue_config` reports unknown keys: segments joined by dots, unquoted.
    pub(crate) fn plain_path(&self) -> String {
        self.path.join(".")
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file.prefix())?;
        for segment in &self.path {
            write!(
                f,
                ".{}",
                toml_edit::Key::new(segment.as_str()).display_repr()
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_and_print_back() {
        let key = Key::parse("appearance.mode").unwrap();
        assert_eq!(
            (key.file, key.path.as_slice()),
            (File::Appearance, &["mode".to_owned()][..])
        );
        let chord = Key::parse("compositor.keybindings.\"Super+Q\"").unwrap();
        assert_eq!(chord.path, ["keybindings", "Super+Q"]);
        assert_eq!(chord.to_string(), "compositor.keybindings.\"Super+Q\"");
        assert_eq!(
            Key::parse("compositor . pointer.natural-scroll")
                .unwrap()
                .to_string(),
            "compositor.pointer.natural-scroll"
        );
    }

    #[test]
    fn bad_names_are_rejected() {
        for name in [
            "",
            "appearance",
            "nope.mode",
            "appearance.",
            "appearance..mode",
        ] {
            assert_eq!(Key::parse(name), None, "{name:?}");
        }
    }
}
