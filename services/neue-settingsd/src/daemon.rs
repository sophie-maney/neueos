//! The D-Bus service (Linux): `org.neueos.Settings1` at `/org/neueos/Settings`, owned as
//! `org.neueos.Settings` on the session bus (ADR 0074 §3).

use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use neue_config::{AppearanceConfig, Layers};
use rustix::fs::inotify::{self, CreateFlags, ReadFlags, WatchFlags};
use tokio::sync::mpsc;
use tracing::{info, warn};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value as Variant};
use zbus::{Connection, connection};

use crate::{Changes, File, Store, dconf, gtk};

/// The well-known name.
pub const BUS_NAME: &str = "org.neueos.Settings";
/// The object path.
pub const PATH: &str = "/org/neueos/Settings";
/// The interface.
pub const INTERFACE: &str = "org.neueos.Settings1";

/// Quiet time after the last file event before the files are re-read (as in neue-comp).
const DEBOUNCE: Duration = Duration::from_millis(100);

/// Errors on the bus: `org.neueos.Settings1.Error.*`.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.neueos.Settings1.Error")]
pub enum SettingsError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// No such key (for `Get`: or it has no value).
    UnknownKey(String),
    /// The value has the wrong type or is out of range.
    InvalidValue(String),
    /// The user's file doesn't parse.
    BrokenFile(String),
    /// Reading or writing a file failed.
    Io(String),
}

impl From<crate::Error> for SettingsError {
    fn from(err: crate::Error) -> Self {
        let message = err.to_string();
        match err {
            crate::Error::UnknownKey(_) => Self::UnknownKey(message),
            crate::Error::InvalidValue { .. } => Self::InvalidValue(message),
            crate::Error::BrokenFile(_) => Self::BrokenFile(message),
            crate::Error::Io { .. } => Self::Io(message),
        }
    }
}

/// What is derived from `appearance.toml` besides the TOML files.
#[derive(Debug, Clone, Default)]
pub struct Outputs {
    /// `$XDG_CONFIG_HOME`, under which GTK's `settings.ini` files are written; `None`: not
    /// written.
    pub gtk_config_home: Option<PathBuf>,
    /// Write the `org.gnome.desktop.*` keys through dconf's writer on the session bus.
    pub dconf: bool,
}

impl Outputs {
    /// Both, under `$XDG_CONFIG_HOME` (or `~/.config`).
    pub fn standard() -> Self {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        Self {
            gtk_config_home: config_home,
            dconf: true,
        }
    }

    /// Write `appearance` out. Failures are logged: the TOML files stay the source of truth
    /// and the next change or start-up tries again.
    pub async fn apply(&self, connection: &Connection, appearance: &AppearanceConfig) {
        if let Some(home) = &self.gtk_config_home
            && let Err(err) = gtk::write(home, appearance)
        {
            warn!(%err, "writing GTK's settings.ini failed");
        }
        if self.dconf {
            let keys = dconf::keys(appearance);
            let entries: Vec<_> = keys.iter().map(|(path, v)| (*path, Some(v))).collect();
            let blob = dconf::changeset(&entries);
            let result = async { DconfWriterProxy::new(connection).await?.change(&blob).await };
            if let Err(err) = result.await {
                warn!(%err, "writing the dconf keys failed");
            }
        }
    }
}

/// dconf's writer (the `dconf` package's D-Bus-activated `dconf-service`).
#[zbus::proxy(
    interface = "ca.desrt.dconf.Writer",
    default_service = "ca.desrt.dconf",
    default_path = "/ca/desrt/dconf/Writer/user",
    gen_blocking = false
)]
trait DconfWriter {
    /// Apply a serialised `a{smv}` changeset; returns the change's tag.
    fn change(&self, blob: &[u8]) -> zbus::Result<String>;
}

/// The `org.neueos.Settings1` object.
#[derive(Debug)]
pub struct Settings {
    store: Arc<Mutex<Store>>,
    outputs: Outputs,
}

impl Settings {
    pub fn new(store: Arc<Mutex<Store>>, outputs: Outputs) -> Self {
        Self { store, outputs }
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    /// Derive the outputs if appearance changed, then send `Changed`.
    async fn publish(
        &self,
        connection: &Connection,
        emitter: &SignalEmitter<'_>,
        changes: Changes,
    ) -> zbus::Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        if changes.touches(File::Appearance) {
            let appearance = self.store().appearance();
            self.outputs.apply(connection, &appearance).await;
        }
        let changed = changes
            .changed
            .iter()
            .map(|(k, v)| Ok((k.clone(), to_variant(v)?)))
            .collect::<zbus::Result<HashMap<_, _>>>()?;
        Self::changed(emitter, changed, changes.unset).await
    }
}

#[zbus::interface(name = "org.neueos.Settings1")]
impl Settings {
    /// The effective value of `key` (`appearance.mode`); a table gives `a{sv}`.
    async fn get(&self, key: &str) -> Result<OwnedValue, SettingsError> {
        let value = self.store().get(key)?;
        Ok(to_variant(&value)?)
    }

    /// Every key with a value.
    async fn get_all(&self) -> Result<HashMap<String, OwnedValue>, SettingsError> {
        let all = self.store().all();
        Ok(all
            .iter()
            .map(|(k, v)| Ok((k.clone(), to_variant(v)?)))
            .collect::<zbus::Result<_>>()?)
    }

    /// Set `key` in the user's file.
    async fn set(
        &self,
        key: &str,
        value: OwnedValue,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), SettingsError> {
        let value = from_variant(&value)
            .map_err(|message| SettingsError::InvalidValue(format!("`{key}`: {message}")))?;
        let changes = self.store().set(key, value)?;
        Ok(self.publish(connection, &emitter, changes).await?)
    }

    /// Remove `key` from the user's file, so the system value or default applies.
    async fn reset(
        &self,
        key: &str,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<(), SettingsError> {
        let changes = self.store().reset(key)?;
        Ok(self.publish(connection, &emitter, changes).await?)
    }

    /// Keys whose effective value changed (by a call or by editing a file), and keys that
    /// no longer have one.
    #[zbus(signal)]
    async fn changed(
        emitter: &SignalEmitter<'_>,
        changed: HashMap<String, OwnedValue>,
        unset: Vec<String>,
    ) -> zbus::Result<()>;
}

/// Serve the settings in `layers` on the session bus until the process is stopped.
pub async fn run(layers: Layers, outputs: Outputs) -> zbus::Result<()> {
    let store = Arc::new(Mutex::new(Store::new(layers.clone())));
    let settings = Settings::new(Arc::clone(&store), outputs.clone());
    let connection = connection::Builder::session()?
        .serve_at(PATH, settings)?
        .name(BUS_NAME)?
        .build()
        .await?;
    // The TOML files are the source of truth: bring GTK and dconf in line with them, in
    // case they were edited while the service was down.
    let appearance = lock(&store).appearance();
    outputs.apply(&connection, &appearance).await;
    info!("serving {BUS_NAME}");

    let (tx, mut rx) = mpsc::unbounded_channel();
    if let Err(err) = watch(&layers, tx) {
        warn!(%err, "not watching the config files: hand edits are seen after a restart");
    }
    let iface = connection
        .object_server()
        .interface::<_, Settings>(PATH)
        .await?;
    while rx.recv().await.is_some() {
        // Editors write in several steps: wait for a quiet moment.
        while let Ok(Some(())) = tokio::time::timeout(DEBOUNCE, rx.recv()).await {}
        let changes = lock(&store).reload();
        let settings = iface.get().await;
        if let Err(err) = settings
            .publish(&connection, iface.signal_emitter(), changes)
            .await
        {
            warn!(%err, "sending Changed failed");
        }
    }
    // The watcher stopped; keep serving calls.
    std::future::pending().await
}

fn lock(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    // A panic while holding the lock can't leave the store half-edited (edits replace the
    // effective table in one assignment), so a poisoned lock is still usable.
    store.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Watch every layer directory for our files, on a thread with blocking reads; each
/// relevant event sends `()`.
fn watch(layers: &Layers, tx: mpsc::UnboundedSender<()>) -> std::io::Result<()> {
    let fd = inotify::init(CreateFlags::CLOEXEC)?;
    if let Some(user) = layers.user_dir() {
        // So a first file there is noticed.
        std::fs::create_dir_all(user)?;
    }
    let flags = WatchFlags::CLOSE_WRITE
        | WatchFlags::MOVED_TO
        | WatchFlags::MOVED_FROM
        | WatchFlags::CREATE
        | WatchFlags::DELETE;
    for dir in layers.dirs().iter().filter(|d| d.is_dir()) {
        if let Err(err) = inotify::add_watch(&fd, dir, flags) {
            warn!(dir = %dir.display(), %err, "not watching a config directory");
        }
    }
    std::thread::Builder::new()
        .name("config-watch".into())
        .spawn(move || {
            let mut buffer = [MaybeUninit::<u8>::uninit(); 4096];
            let mut reader = inotify::Reader::new(&fd, &mut buffer);
            loop {
                match reader.next() {
                    Ok(event) => {
                        let ours = event.events().contains(ReadFlags::QUEUE_OVERFLOW)
                            || event.file_name().is_some_and(|name| {
                                File::ALL
                                    .iter()
                                    .any(|f| name.to_bytes() == f.file_name().as_bytes())
                            });
                        if ours && tx.send(()).is_err() {
                            return;
                        }
                    }
                    Err(rustix::io::Errno::INTR) => {}
                    Err(err) => {
                        warn!(%err, "reading config file events failed");
                        return;
                    }
                }
            }
        })?;
    Ok(())
}

/// A TOML value on the bus: `b`, `x`, `d`, `s`, arrays as `av`, tables as `a{sv}`.
fn to_variant(value: &toml::Value) -> zbus::Result<OwnedValue> {
    fn plain(value: &toml::Value) -> Variant<'static> {
        match value {
            toml::Value::String(s) => Variant::from(s.clone()),
            toml::Value::Integer(i) => Variant::from(*i),
            toml::Value::Float(f) => Variant::from(*f),
            toml::Value::Boolean(b) => Variant::from(*b),
            toml::Value::Datetime(d) => Variant::from(d.to_string()),
            toml::Value::Array(items) => Variant::from(items.iter().map(plain).collect::<Vec<_>>()),
            toml::Value::Table(table) => Variant::from(
                table
                    .iter()
                    .map(|(k, v)| (k.clone(), plain(v)))
                    .collect::<HashMap<_, _>>(),
            ),
        }
    }
    Ok(OwnedValue::try_from(plain(value))?)
}

/// A value from the bus as TOML. Any integer type works; strings, object paths and
/// signatures become strings; dictionaries need string keys.
fn from_variant(value: &Variant<'_>) -> Result<toml::Value, String> {
    use toml::Value as T;
    Ok(match value {
        Variant::Bool(b) => T::Boolean(*b),
        Variant::U8(n) => T::Integer((*n).into()),
        Variant::I16(n) => T::Integer((*n).into()),
        Variant::U16(n) => T::Integer((*n).into()),
        Variant::I32(n) => T::Integer((*n).into()),
        Variant::U32(n) => T::Integer((*n).into()),
        Variant::I64(n) => T::Integer(*n),
        Variant::U64(n) => T::Integer(i64::try_from(*n).map_err(|_| "integer too large")?),
        Variant::F64(f) => T::Float(*f),
        Variant::Str(s) => T::String(s.to_string()),
        Variant::ObjectPath(p) => T::String(p.to_string()),
        Variant::Signature(s) => T::String(s.to_string()),
        Variant::Value(inner) => from_variant(inner)?,
        Variant::Array(items) => T::Array(
            items
                .iter()
                .map(from_variant)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Variant::Dict(dict) => {
            let mut table = toml::Table::new();
            for (k, v) in dict.iter() {
                let Variant::Str(k) = k else {
                    return Err("dictionary keys must be strings".into());
                };
                table.insert(k.to_string(), from_variant(v)?);
            }
            T::Table(table)
        }
        other => {
            return Err(format!(
                "values of type `{}` aren't settings",
                other.value_signature()
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use futures_util::StreamExt;
    use zbus::message::Type as MessageType;

    use super::*;

    #[test]
    fn values_round_trip() {
        let value: toml::Value = toml::from_str::<toml::Table>(
            "a = 1\nb = 2.5\nc = \"x\"\nd = [\"us\", \"de\"]\n[e]\nf = true\n",
        )
        .unwrap()
        .into();
        let variant = to_variant(&value).unwrap();
        assert_eq!(variant.value_signature().to_string(), "a{sv}");
        let list = to_variant(&toml::Value::from(vec!["us"])).unwrap();
        assert_eq!(list.value_signature().to_string(), "av");
        assert_eq!(from_variant(&variant).unwrap(), value);
        assert!(from_variant(&Variant::from((1u8, 2u8))).is_err());
    }

    async fn pair(store: Store, outputs: Outputs) -> (Connection, Connection) {
        let (a, b) = UnixStream::pair().unwrap();
        let settings = Settings::new(Arc::new(Mutex::new(store)), outputs);
        let server = connection::Builder::async_io_unix_stream(a)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at(PATH, settings)
            .unwrap()
            .build();
        let client = connection::Builder::async_io_unix_stream(b).p2p().build();
        let (server, client) = tokio::join!(server, client);
        (server.unwrap(), client.unwrap())
    }

    async fn call<B>(client: &Connection, method: &str, body: &B) -> zbus::Result<zbus::Message>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        client
            .call_method(None::<&str>, PATH, Some(INTERFACE), method, body)
            .await
    }

    #[tokio::test]
    async fn set_get_reset_and_changed_over_the_bus() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::new(Layers::from_dirs([root.path().join("neueos")]));
        let outputs = Outputs {
            gtk_config_home: Some(root.path().to_owned()),
            dconf: false,
        };
        let (_server, client) = pair(store, outputs).await;
        let mut stream = zbus::MessageStream::from(&client);

        let reply = call(&client, "Get", &("appearance.mode",)).await.unwrap();
        let mode: OwnedValue = reply.body().deserialize().unwrap();
        assert_eq!(from_variant(&mode).unwrap(), toml::Value::from("light"));

        call(&client, "Set", &("appearance.mode", Variant::from("dark")))
            .await
            .unwrap();
        let signal = loop {
            let message = stream.next().await.unwrap().unwrap();
            if message.message_type() == MessageType::Signal {
                break message;
            }
        };
        assert_eq!(signal.header().member().unwrap().as_str(), "Changed");
        let (changed, unset): (HashMap<String, OwnedValue>, Vec<String>) =
            signal.body().deserialize().unwrap();
        assert_eq!(
            from_variant(&changed["appearance.mode"]).unwrap(),
            toml::Value::from("dark")
        );
        assert!(unset.is_empty());
        let toml = std::fs::read_to_string(root.path().join("neueos/appearance.toml")).unwrap();
        assert_eq!(toml, "mode = \"dark\"\n");
        let ini = std::fs::read_to_string(root.path().join("gtk-4.0/settings.ini")).unwrap();
        assert!(
            ini.contains("gtk-application-prefer-dark-theme=true"),
            "{ini}"
        );

        let err = call(&client, "Set", &("appearance.mode", Variant::from(3i32)))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, zbus::Error::MethodError(name, ..)
                if name.as_str() == "org.neueos.Settings1.Error.InvalidValue"),
            "{err}"
        );
        let err = call(&client, "Get", &("appearance.nope",))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, zbus::Error::MethodError(name, ..)
                if name.as_str() == "org.neueos.Settings1.Error.UnknownKey"),
            "{err}"
        );

        call(&client, "Reset", &("appearance.mode",)).await.unwrap();
        let all = call(&client, "GetAll", &()).await.unwrap();
        let all: HashMap<String, OwnedValue> = all.body().deserialize().unwrap();
        assert_eq!(
            from_variant(&all["appearance.mode"]).unwrap(),
            toml::Value::from("light")
        );
    }
}
