//! neueOS configuration (ADR 0004, ADR 0009): one serde struct per TOML file, layered
//! loading and validation. Portable: no I/O besides reading files, no logging (problems come
//! back as data so the caller decides how to report them).
//!
//! Layers, lowest to highest priority: the schema defaults, `/usr/share/neueos/`,
//! `/etc/neueos/`, `$XDG_CONFIG_HOME/neueos/`. Tables merge key by key, everything else
//! (scalars and arrays) is replaced. A missing file at any layer is fine.
//!
//! A layer that does not parse, or has a value of the wrong type or out of range, makes
//! [`load`] fail with a [`ConfigError`] naming the file, line and column; callers keep the
//! last good config (or use defaults at start-up). Keys the schema does not know are not
//! errors: they come back as [`Loaded::unknown_keys`].
//!
//! Limits of unknown-key detection: it compares the input with the re-serialised config, so
//! a key that is only accepted through `#[serde(alias)]` is reported as unknown. Add an
//! alias-aware check here before using the first alias.

mod chord;
pub mod compositor;

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use toml::{Table, Value};

pub use chord::{Chord, ChordError};
pub use compositor::{CompositorConfig, TitleBarDoubleClick, WindowsConfig};

/// Directory name inside each config root.
const DIR_NAME: &str = "neueos";

/// A config file with a schema. Implemented by one struct per file.
pub trait ConfigFile: Serialize + DeserializeOwned + Default {
    /// File name inside the config directories, e.g. `compositor.toml`.
    const FILE_NAME: &'static str;

    /// Rewrite a parsed layer into canonical form before layers are merged, so that two
    /// spellings of one thing (`"super+q"` and `"Super+Q"`) land on one key. Must leave
    /// anything it does not understand alone; deserialisation reports it.
    fn normalize(_layer: &mut Table) {}
}

/// Where config files are looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layers {
    /// Directories, lowest priority first. The last one is the user's.
    dirs: Vec<PathBuf>,
}

impl Layers {
    /// The standard layers. The user layer is `$XDG_CONFIG_HOME/neueos`, or
    /// `$HOME/.config/neueos`; without either only the system layers exist.
    pub fn standard() -> Self {
        Self::from_env(
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            std::env::var_os("HOME").map(PathBuf::from),
        )
    }

    fn from_env(xdg_config_home: Option<PathBuf>, home: Option<PathBuf>) -> Self {
        let mut dirs = vec![
            PathBuf::from("/usr/share").join(DIR_NAME),
            PathBuf::from("/etc").join(DIR_NAME),
        ];
        let user = xdg_config_home
            .filter(|p| p.is_absolute())
            .or_else(|| home.filter(|p| p.is_absolute()).map(|h| h.join(".config")));
        dirs.extend(user.map(|p| p.join(DIR_NAME)));
        Self { dirs }
    }

    /// Exactly these directories, lowest priority first (tests, and `--config-dir`).
    pub fn from_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            dirs: dirs.into_iter().collect(),
        }
    }

    /// No directories: only the schema defaults.
    pub fn none() -> Self {
        Self { dirs: Vec::new() }
    }

    /// The directories, lowest priority first.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// The highest-priority directory, where edits are made.
    pub fn user_dir(&self) -> Option<&Path> {
        self.dirs.last().map(PathBuf::as_path)
    }
}

/// A config that loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded<T> {
    pub config: T,
    /// Dotted paths of keys the schema does not know, e.g. `keyboard.layoutz`. Newer
    /// configs on older builds land here; log them as warnings.
    pub unknown_keys: Vec<String>,
}

/// Why a config file was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct ConfigError {
    /// The offending file.
    pub file: PathBuf,
    /// 1-based position of the problem, when the parser knows it.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl ConfigError {
    /// An error in `file` without a position, e.g. a value that is well-formed but that
    /// the program cannot use (an xkb layout that does not exist).
    pub fn in_file(file: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            line: None,
            column: None,
            message: message.into(),
        }
    }

    fn at(file: &Path, text: &str, err: &toml::de::Error) -> Self {
        let (line, column) = err
            .span()
            .map(|span| line_and_column(text, span.start))
            .unzip();
        Self {
            file: file.to_owned(),
            line,
            column,
            message: err.message().trim_end().to_owned(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())?;
        if let (Some(line), Some(column)) = (self.line, self.column) {
            write!(f, ":{line}:{column}")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// 1-based line and column (in characters) of byte offset `offset` in `text`.
fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// Load `T` from every layer and merge them.
pub fn load<T: ConfigFile>(layers: &Layers) -> Result<Loaded<T>, ConfigError> {
    // The defaults are the lowest layer, so tables with default entries (keybindings) merge
    // with the files instead of being replaced by them.
    let mut merged = Table::try_from(T::default()).map_err(|err| {
        ConfigError::in_file(T::FILE_NAME, format!("encoding the defaults: {err}"))
    })?;
    let mut input = Table::new();
    for dir in layers.dirs() {
        let path = dir.join(T::FILE_NAME);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(ConfigError::in_file(path, err.to_string())),
        };
        let mut layer = parse_layer::<T>(&path, &text)?;
        T::normalize(&mut layer);
        merge(&mut merged, layer.clone());
        merge(&mut input, layer);
    }
    let config: T = Value::Table(merged)
        .try_into()
        .map_err(|err: toml::de::Error| {
            // Every layer validated on its own, so this is a combination no layer shows.
            let file = layers
                .user_dir()
                .map_or_else(|| PathBuf::from(T::FILE_NAME), |d| d.join(T::FILE_NAME));
            ConfigError::in_file(file, err.message().trim_end())
        })?;
    let unknown_keys = unknown_keys(&input, &config);
    Ok(Loaded {
        config,
        unknown_keys,
    })
}

/// Parse one layer, checking syntax and, on its own, the schema (every field has a default,
/// so a partial layer deserialises), which gives errors a position in this file.
fn parse_layer<T: ConfigFile>(path: &Path, text: &str) -> Result<Table, ConfigError> {
    let mut table: Table = text.parse().map_err(|e| ConfigError::at(path, text, &e))?;
    // Validate the normalised layer, not the raw text, so spelling variants are checked the
    // way they will be used. Positions come from the raw text when this fails.
    let raw_error = toml::from_str::<T>(text).err();
    T::normalize(&mut table);
    if let Err(err) = Value::Table(table.clone()).try_into::<T>() {
        return Err(match raw_error {
            Some(raw) => ConfigError::at(path, text, &raw),
            None => ConfigError::in_file(path, err.message().trim_end()),
        });
    }
    // `normalize` runs again in `load`; it must be idempotent.
    Ok(table)
}

/// Merge `top` into `base`: tables key by key, anything else replaced.
fn merge(base: &mut Table, top: Table) {
    for (key, value) in top {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(base)), Value::Table(top)) => merge(base, top),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Dotted paths in `input` that the serialised `config` does not have.
fn unknown_keys<T: Serialize>(input: &Table, config: &T) -> Vec<String> {
    let Ok(known) = Table::try_from(config) else {
        return Vec::new();
    };
    let mut unknown = Vec::new();
    collect_unknown(input, &known, "", &mut unknown);
    unknown
}

fn collect_unknown(input: &Table, known: &Table, prefix: &str, out: &mut Vec<String>) {
    for (key, value) in input {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match known.get(key) {
            None => out.push(path),
            Some(Value::Table(known)) => {
                if let Value::Table(input) = value {
                    collect_unknown(input, known, &path, out);
                }
            }
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Sample {
        name: String,
        count: u8,
        list: Vec<String>,
        inner: Inner,
    }

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(default, rename_all = "kebab-case")]
    struct Inner {
        a: u8,
        b: u8,
    }

    impl ConfigFile for Sample {
        const FILE_NAME: &'static str = "sample.toml";
    }

    fn layers(files: &[&str]) -> (tempfile::TempDir, Layers) {
        let root = tempfile::tempdir().unwrap();
        let mut dirs = Vec::new();
        for (i, content) in files.iter().enumerate() {
            let dir = root.path().join(i.to_string());
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("sample.toml"), content).unwrap();
            dirs.push(dir);
        }
        (root, Layers::from_dirs(dirs))
    }

    #[test]
    fn missing_files_give_defaults() {
        let loaded =
            load::<Sample>(&Layers::from_dirs([PathBuf::from("/nonexistent/neueos")])).unwrap();
        assert_eq!(loaded.config, Sample::default());
        assert!(loaded.unknown_keys.is_empty());
        assert_eq!(
            load::<Sample>(&Layers::none()).unwrap().config,
            Sample::default()
        );
    }

    #[test]
    fn tables_merge_and_other_values_replace() {
        let (_root, layers) = layers(&[
            "name = \"system\"\ncount = 1\nlist = [\"a\", \"b\"]\n[inner]\na = 1\nb = 2\n",
            "count = 5\nlist = [\"c\"]\n[inner]\nb = 9\n",
        ]);
        let config = load::<Sample>(&layers).unwrap().config;
        assert_eq!(config.name, "system");
        assert_eq!(config.count, 5);
        assert_eq!(config.list, ["c"]);
        assert_eq!(config.inner, Inner { a: 1, b: 9 });
    }

    #[test]
    fn unknown_keys_are_reported_not_rejected() {
        let (_root, layers) =
            layers(&["count = 2\nfuture = 1\n[inner]\na = 3\nz = 4\n[extra]\nk = 1\n"]);
        let loaded = load::<Sample>(&layers).unwrap();
        assert_eq!(loaded.config.count, 2);
        assert_eq!(loaded.unknown_keys, ["extra", "future", "inner.z"]);
    }

    #[test]
    fn syntax_errors_carry_file_line_and_column() {
        let (_root, layers) = layers(&["count = 1\nname = \n"]);
        let err = load::<Sample>(&layers).unwrap_err();
        assert!(err.file.ends_with("0/sample.toml"));
        assert_eq!(err.line, Some(2));
        assert!(err.column.is_some());
        assert!(err.to_string().contains("sample.toml:2:"));
    }

    #[test]
    fn wrong_type_and_out_of_range_are_rejected_with_a_position() {
        let (_root, wrong) = layers(&["name = \"x\"\ncount = \"many\"\n"]);
        let err = load::<Sample>(&wrong).unwrap_err();
        assert_eq!(err.line, Some(2));
        let (_root, range) = layers(&["\n\ncount = 300\n"]);
        let err = load::<Sample>(&range).unwrap_err();
        assert_eq!(err.line, Some(3));
    }

    #[test]
    fn a_bad_layer_rejects_the_whole_load() {
        let (_root, layers) = layers(&["count = 1\n", "count = [\n"]);
        assert!(load::<Sample>(&layers).is_err());
    }

    #[test]
    fn standard_layers_follow_xdg() {
        let layers = Layers::from_env(Some("/xdg".into()), Some("/home/u".into()));
        assert_eq!(layers.user_dir(), Some(Path::new("/xdg/neueos")));
        assert_eq!(layers.dirs().len(), 3);
        let layers = Layers::from_env(None, Some("/home/u".into()));
        assert_eq!(layers.user_dir(), Some(Path::new("/home/u/.config/neueos")));
        // A relative XDG_CONFIG_HOME is ignored, per the XDG spec.
        let layers = Layers::from_env(Some("rel".into()), Some("/home/u".into()));
        assert_eq!(layers.user_dir(), Some(Path::new("/home/u/.config/neueos")));
        let layers = Layers::from_env(None, None);
        assert_eq!(layers.dirs().len(), 2);
    }

    #[test]
    fn line_and_column_count_characters() {
        assert_eq!(line_and_column("ab\ncé=", 6), (2, 3));
        assert_eq!(line_and_column("x", 0), (1, 1));
    }
}
