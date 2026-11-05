//! The key store over the layered TOML files (ADR 0004 §6, ADR 0074 §2).

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use neue_config::{
    AppearanceConfig, CompositorConfig, ConfigError, ConfigFile, Layers, ShellConfig,
};
use toml::{Table, Value};
use toml_edit::{DocumentMut, Item, TableLike};
use tracing::warn;

use crate::key::{File, Key};

/// Why a get, set or reset failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// The name isn't a key of the schema (for a get: or the key has no value, like an
    /// unset optional picture).
    #[error("no setting `{0}`")]
    UnknownKey(String),
    /// The value has the wrong type or is out of range.
    #[error("`{key}`: {message}")]
    InvalidValue { key: String, message: String },
    /// The user's file doesn't parse; fix it (or remove it) first.
    #[error("{0}")]
    BrokenFile(ConfigError),
    /// Reading or writing the user's file failed.
    #[error("{path}: {message}")]
    Io { path: PathBuf, message: String },
}

/// What a set, reset or reload changed in the effective (merged) settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    /// Keys with a new value, by name.
    pub changed: BTreeMap<String, Value>,
    /// Keys that no longer have a value (an optional key that was reset).
    pub unset: Vec<String>,
    /// The files these keys are in.
    pub files: Vec<File>,
}

impl Changes {
    /// Nothing changed.
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.unset.is_empty()
    }

    /// Whether a key of `file` changed.
    pub fn touches(&self, file: File) -> bool {
        self.files.contains(&file)
    }

    fn extend(&mut self, other: Changes) {
        self.changed.extend(other.changed);
        self.unset.extend(other.unset);
        for file in other.files {
            if !self.files.contains(&file) {
                self.files.push(file);
            }
        }
    }
}

/// Where the user layer lives.
#[derive(Debug)]
enum UserLayer {
    /// In the user's config directory (the last of the layers).
    Files,
    /// In memory, never written: the mock.
    Memory(BTreeMap<File, String>),
}

/// The settings: the effective value of every key, and edits to the user layer.
#[derive(Debug)]
pub struct Store {
    layers: Layers,
    user: UserLayer,
    /// The merged config of each file, serialised.
    effective: BTreeMap<File, Table>,
}

impl Store {
    /// The settings in `layers` (normally [`Layers::standard`]); edits go to the last
    /// directory's files. A file that doesn't load is logged and its defaults are used.
    pub fn new(layers: Layers) -> Self {
        let mut store = Self {
            layers,
            user: UserLayer::Files,
            effective: BTreeMap::new(),
        };
        store.reload();
        store
    }

    /// The mock: built-in defaults and a user layer in memory. Nothing is read or written,
    /// so Preferences can run on macOS without the service.
    pub fn in_memory() -> Self {
        let mut store = Self {
            layers: Layers::none(),
            user: UserLayer::Memory(BTreeMap::new()),
            effective: BTreeMap::new(),
        };
        store.reload();
        store
    }

    /// Re-read every file and report what changed (hand edits). A file that doesn't load
    /// keeps its last good settings; the error is logged.
    pub fn reload(&mut self) -> Changes {
        let mut changes = Changes::default();
        for file in File::ALL {
            let loaded = self
                .user_text(file)
                .map_err(Error::from_io(self.user_path(file)))
                .and_then(|text| {
                    load(file, &self.layers, text.as_deref()).map_err(Error::BrokenFile)
                });
            match loaded {
                Ok((table, unknown)) => {
                    for key in unknown {
                        warn!(file = file.file_name(), key, "ignoring unknown config key");
                    }
                    changes.extend(self.replace(file, table));
                }
                Err(err) => {
                    warn!(%err, "config rejected, keeping the previous one");
                    self.effective.entry(file).or_insert_with(|| defaults(file));
                }
            }
        }
        changes
    }

    /// The effective value of `name`. A table key gives the whole table.
    pub fn get(&self, name: &str) -> Result<Value, Error> {
        let unknown = || Error::UnknownKey(name.to_owned());
        let key = Key::parse(name).ok_or_else(unknown)?;
        lookup(&self.effective[&key.file], &key.path)
            .cloned()
            .ok_or_else(unknown)
    }

    /// Every key with a value, by name; tables are flattened to their leaves.
    pub fn all(&self) -> BTreeMap<String, Value> {
        let mut all = BTreeMap::new();
        for (file, table) in &self.effective {
            flatten(*file, table, &mut Vec::new(), &mut all);
        }
        all
    }

    /// The effective `appearance.toml`.
    pub fn appearance(&self) -> AppearanceConfig {
        Value::Table(self.effective[&File::Appearance].clone())
            .try_into()
            .unwrap_or_default()
    }

    /// Set `name` to `value` in the user's file, after checking it against the schema.
    pub fn set(&mut self, name: &str, value: Value) -> Result<Changes, Error> {
        let key = Key::parse(name).ok_or_else(|| Error::UnknownKey(name.to_owned()))?;
        // An integer for a float key (`ui-size = 12`) is what the caller means.
        let value = match (lookup(&self.effective[&key.file], &key.path), value) {
            (Some(Value::Float(_)), Value::Integer(i)) => Value::Float(i as f64),
            (_, value) => value,
        };
        let edit_value = to_edit(&value);
        self.edit(&key, |doc| {
            let (last, parents) = key.path.split_last().ok_or("empty key")?;
            let mut table: &mut dyn TableLike = doc.as_table_mut();
            for segment in parents {
                let item = table.entry(segment).or_insert_with(|| {
                    let mut t = toml_edit::Table::new();
                    t.set_implicit(true);
                    Item::Table(t)
                });
                table = item
                    .as_table_like_mut()
                    .ok_or("a parent of this key is not a table")?;
            }
            match table.get_mut(last) {
                // Keep the comments and spacing around a value that is replaced.
                Some(Item::Value(old)) => {
                    let decor = old.decor().clone();
                    *old = edit_value;
                    *old.decor_mut() = decor;
                }
                _ => {
                    table.insert(last, Item::Value(edit_value));
                }
            }
            Ok(())
        })
    }

    /// Remove `name` from the user's file, so a lower layer's value (or the default)
    /// applies again. A key the user's file doesn't set is not an error.
    pub fn reset(&mut self, name: &str) -> Result<Changes, Error> {
        let key = Key::parse(name).ok_or_else(|| Error::UnknownKey(name.to_owned()))?;
        // Optional keys without a value, and map entries, are absent from the effective
        // config: for those the table that would hold them must exist.
        let effective = &self.effective[&key.file];
        let known = lookup(effective, &key.path).is_some()
            || key.path.split_last().is_some_and(|(_, parents)| {
                !parents.is_empty() && lookup(effective, parents).is_some_and(Value::is_table)
            });
        if !known {
            return Err(Error::UnknownKey(name.to_owned()));
        }
        self.edit(&key, |doc| {
            let (last, parents) = key.path.split_last().ok_or("empty key")?;
            let mut table: &mut dyn TableLike = doc.as_table_mut();
            for segment in parents {
                match table.get_mut(segment).and_then(Item::as_table_like_mut) {
                    Some(inner) => table = inner,
                    None => return Ok(()),
                }
            }
            table.remove(last);
            Ok(())
        })
    }

    /// Apply `change` to the user's file of `key`, check the result, store it and report
    /// what changed.
    fn edit(
        &mut self,
        key: &Key,
        change: impl FnOnce(&mut DocumentMut) -> Result<(), &'static str>,
    ) -> Result<Changes, Error> {
        let file = key.file;
        let path = self.user_path(file);
        let text = self.user_text(file).map_err(Error::from_io(path.clone()))?;
        let text = text.unwrap_or_default();
        let mut doc: DocumentMut = text.parse().map_err(|err: toml_edit::TomlError| {
            Error::BrokenFile(ConfigError::in_file(&path, err.message().trim_end()))
        })?;
        let invalid = |message: String| Error::InvalidValue {
            key: key.to_string(),
            message,
        };
        change(&mut doc).map_err(|m| invalid(m.to_owned()))?;
        let new_text = doc.to_string();
        let (table, unknown) =
            load(file, &self.layers, Some(&new_text)).map_err(|err| invalid(err.message))?;
        let plain = key.plain_path();
        if unknown
            .iter()
            .any(|u| plain == *u || plain.starts_with(&format!("{u}.")))
        {
            return Err(Error::UnknownKey(key.to_string()));
        }
        if new_text != text {
            self.write_user(file, new_text)?;
        }
        Ok(self.replace(file, table))
    }

    /// Make `table` the effective config of `file`; the keys that differ.
    fn replace(&mut self, file: File, table: Table) -> Changes {
        let mut old = BTreeMap::new();
        if let Some(previous) = self.effective.get(&file) {
            flatten(file, previous, &mut Vec::new(), &mut old);
        }
        let mut new = BTreeMap::new();
        flatten(file, &table, &mut Vec::new(), &mut new);
        self.effective.insert(file, table);
        let unset: Vec<String> = old
            .keys()
            .filter(|k| !new.contains_key(*k))
            .cloned()
            .collect();
        let changed: BTreeMap<String, Value> = new
            .into_iter()
            .filter(|(k, v)| old.get(k) != Some(v))
            .collect();
        let files = if changed.is_empty() && unset.is_empty() {
            Vec::new()
        } else {
            vec![file]
        };
        Changes {
            changed,
            unset,
            files,
        }
    }

    fn user_path(&self, file: File) -> PathBuf {
        self.layers.user_dir().map_or_else(
            || PathBuf::from(file.file_name()),
            |d| d.join(file.file_name()),
        )
    }

    /// The user layer's text; `None` when there is no file.
    fn user_text(&self, file: File) -> std::io::Result<Option<String>> {
        match &self.user {
            UserLayer::Memory(files) => Ok(files.get(&file).cloned()),
            UserLayer::Files if self.layers.user_dir().is_none() => Ok(None),
            UserLayer::Files => match std::fs::read_to_string(self.user_path(file)) {
                Ok(text) => Ok(Some(text)),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(err) => Err(err),
            },
        }
    }

    fn write_user(&mut self, file: File, text: String) -> Result<(), Error> {
        match &mut self.user {
            UserLayer::Memory(files) => {
                files.insert(file, text);
                Ok(())
            }
            UserLayer::Files => {
                let path = self.user_path(file);
                write_atomically(&path, text.as_bytes()).map_err(Error::from_io(path))
            }
        }
    }
}

impl Error {
    fn from_io(path: PathBuf) -> impl FnOnce(std::io::Error) -> Error {
        move |err| Error::Io {
            path,
            message: err.to_string(),
        }
    }
}

/// Write `bytes` to a temporary file next to `path` and rename it into place, so readers
/// never see half a file (ADR 0004 §6).
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|err| err.error)?;
    Ok(())
}

/// Load `file` with `user` as its user layer: the merged config, serialised, and the
/// unknown keys.
fn load(
    file: File,
    layers: &Layers,
    user: Option<&str>,
) -> Result<(Table, Vec<String>), ConfigError> {
    fn one<T: ConfigFile>(
        layers: &Layers,
        user: Option<&str>,
    ) -> Result<(Table, Vec<String>), ConfigError> {
        let loaded = neue_config::load_with_user::<T>(layers, user)?;
        let table = Table::try_from(&loaded.config)
            .map_err(|err| ConfigError::in_file(T::FILE_NAME, err.to_string()))?;
        Ok((table, loaded.unknown_keys))
    }
    match file {
        File::Appearance => one::<AppearanceConfig>(layers, user),
        File::Compositor => one::<CompositorConfig>(layers, user),
        File::Shell => one::<ShellConfig>(layers, user),
    }
}

fn defaults(file: File) -> Table {
    load(file, &Layers::none(), None).map_or_else(|_| Table::new(), |(table, _)| table)
}

fn lookup<'a>(table: &'a Table, path: &[String]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let value = table.get(first)?;
    match (rest.is_empty(), value) {
        (true, value) => Some(value),
        (false, Value::Table(inner)) => lookup(inner, rest),
        (false, _) => None,
    }
}

fn flatten(file: File, table: &Table, path: &mut Vec<String>, out: &mut BTreeMap<String, Value>) {
    for (name, value) in table {
        path.push(name.clone());
        match value {
            Value::Table(inner) => flatten(file, inner, path, out),
            value => {
                let key = Key {
                    file,
                    path: path.clone(),
                };
                out.insert(key.to_string(), value.clone());
            }
        }
        path.pop();
    }
}

/// A `toml` value as a `toml_edit` one (tables become inline tables).
fn to_edit(value: &Value) -> toml_edit::Value {
    match value {
        Value::String(s) => s.as_str().into(),
        Value::Integer(i) => (*i).into(),
        Value::Float(f) => (*f).into(),
        Value::Boolean(b) => (*b).into(),
        Value::Datetime(d) => d.to_string().into(),
        Value::Array(items) => {
            toml_edit::Value::Array(items.iter().map(to_edit).collect::<toml_edit::Array>())
        }
        Value::Table(table) => {
            let mut inline = toml_edit::InlineTable::new();
            for (k, v) in table {
                inline.insert(k, to_edit(v));
            }
            toml_edit::Value::InlineTable(inline)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(user: &str) -> (tempfile::TempDir, Store) {
        let root = tempfile::tempdir().unwrap();
        let system = root.path().join("system");
        let user_dir = root.path().join("user");
        std::fs::create_dir_all(&system).unwrap();
        std::fs::create_dir_all(&user_dir).unwrap();
        std::fs::write(system.join("appearance.toml"), "accent = \"graphite\"\n").unwrap();
        std::fs::write(user_dir.join("appearance.toml"), user).unwrap();
        let store = Store::new(Layers::from_dirs([system, user_dir]));
        (root, store)
    }

    fn user_file(root: &tempfile::TempDir, name: &str) -> String {
        std::fs::read_to_string(root.path().join("user").join(name)).unwrap_or_default()
    }

    #[test]
    fn get_gives_the_merged_value() {
        let (_root, store) = files("mode = \"dark\"\n");
        assert_eq!(store.get("appearance.mode").unwrap(), Value::from("dark"));
        assert_eq!(
            store.get("appearance.accent").unwrap(),
            Value::from("graphite")
        );
        assert_eq!(
            store.get("appearance.cursor.size").unwrap(),
            Value::Integer(24)
        );
        assert!(store.get("appearance.cursor").unwrap().is_table());
        assert_eq!(
            store.get("compositor.keybindings.\"Super+Q\"").unwrap(),
            Value::from("quit")
        );
        assert!(matches!(
            store.get("appearance.nope"),
            Err(Error::UnknownKey(_))
        ));
        // An optional key without a value.
        assert!(matches!(
            store.get("shell.desktop.picture"),
            Err(Error::UnknownKey(_))
        ));
    }

    #[test]
    fn set_edits_the_user_file_and_keeps_comments() {
        let (root, mut store) = files("# mine\nmode = \"light\" # keep\n");
        let changes = store.set("appearance.mode", Value::from("dark")).unwrap();
        assert_eq!(changes.changed["appearance.mode"], Value::from("dark"));
        assert!(changes.touches(File::Appearance));
        let text = user_file(&root, "appearance.toml");
        assert!(text.starts_with("# mine\nmode = \"dark\""), "{text}");
        assert!(text.contains("# keep"), "{text}");
        assert_eq!(store.appearance().mode, neue_config::appearance::Mode::Dark);

        store
            .set("compositor.pointer.natural-scroll", Value::Boolean(true))
            .unwrap();
        let text = user_file(&root, "compositor.toml");
        assert_eq!(text, "[pointer]\nnatural-scroll = true\n");

        // Setting the same value again changes nothing.
        assert!(
            store
                .set("appearance.mode", Value::from("dark"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn set_checks_the_schema() {
        let (root, mut store) = files("");
        let invalid = |r: Result<Changes, Error>| matches!(r, Err(Error::InvalidValue { .. }));
        assert!(invalid(store.set("appearance.mode", Value::from("dim"))));
        assert!(invalid(
            store.set("appearance.cursor.size", Value::Integer(2))
        ));
        assert!(invalid(
            store.set("appearance.reduced-motion", Value::from("yes"))
        ));
        assert!(invalid(
            store.set("appearance.cursor.size.x", Value::Integer(2))
        ));
        assert!(matches!(
            store.set("appearance.moed", Value::from("dark")),
            Err(Error::UnknownKey(_))
        ));
        assert!(matches!(
            store.set("appearance.fonts.colour", Value::from("red")),
            Err(Error::UnknownKey(_))
        ));
        // Nothing was written.
        assert_eq!(user_file(&root, "appearance.toml"), "");
        // An integer for a float key works.
        store
            .set("appearance.fonts.ui-size", Value::Integer(12))
            .unwrap();
        assert_eq!(
            store.get("appearance.fonts.ui-size").unwrap(),
            Value::Float(12.0)
        );
    }

    #[test]
    fn map_keys_and_optional_keys() {
        let (_root, mut store) = files("");
        let changes = store
            .set("compositor.keybindings.\"Super+W\"", Value::from("close"))
            .unwrap();
        assert!(
            changes
                .changed
                .contains_key("compositor.keybindings.\"Super+W\"")
        );
        let changes = store
            .set("shell.desktop.picture", Value::from("/a.png"))
            .unwrap();
        assert_eq!(
            changes.changed["shell.desktop.picture"],
            Value::from("/a.png")
        );
        let changes = store.reset("shell.desktop.picture").unwrap();
        assert_eq!(changes.unset, ["shell.desktop.picture"]);
        assert!(
            store
                .reset("compositor.keybindings.\"Super+Z\"")
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            store.reset("shell.nope.x"),
            Err(Error::UnknownKey(_))
        ));
    }

    #[test]
    fn reset_falls_back_to_the_lower_layer() {
        let (_root, mut store) = files("accent = \"blue\"\n");
        assert_eq!(store.get("appearance.accent").unwrap(), Value::from("blue"));
        let changes = store.reset("appearance.accent").unwrap();
        assert_eq!(
            changes.changed["appearance.accent"],
            Value::from("graphite")
        );
    }

    #[test]
    fn a_broken_user_file_is_reported_not_overwritten() {
        let (root, mut store) = files("mode = \n");
        // Loading kept the defaults.
        assert_eq!(store.get("appearance.mode").unwrap(), Value::from("light"));
        assert!(matches!(
            store.set("appearance.mode", Value::from("dark")),
            Err(Error::BrokenFile(_))
        ));
        assert_eq!(user_file(&root, "appearance.toml"), "mode = \n");
    }

    #[test]
    fn reload_reports_hand_edits() {
        let (root, mut store) = files("");
        assert!(store.reload().is_empty());
        std::fs::write(
            root.path().join("user/appearance.toml"),
            "contrast = \"increased\"\n",
        )
        .unwrap();
        let changes = store.reload();
        assert_eq!(
            changes.changed.keys().collect::<Vec<_>>(),
            ["appearance.contrast"]
        );
        // A broken edit keeps the last good settings.
        std::fs::write(root.path().join("user/appearance.toml"), "contrast = [\n").unwrap();
        assert!(store.reload().is_empty());
        assert_eq!(
            store.get("appearance.contrast").unwrap(),
            Value::from("increased")
        );
    }

    #[test]
    fn the_mock_keeps_edits_in_memory() {
        let mut store = Store::in_memory();
        assert_eq!(store.get("appearance.mode").unwrap(), Value::from("light"));
        store.set("appearance.mode", Value::from("dark")).unwrap();
        assert_eq!(store.get("appearance.mode").unwrap(), Value::from("dark"));
        assert!(store.reload().is_empty());
        assert!(store.all().contains_key("compositor.keyboard.layouts"));
    }
}
