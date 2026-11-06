//! The client side of `neue-settingsd` (`org.neueos.Settings1`, ADR 0074), as far as the
//! shell needs it: read and set a boolean key, set a list of strings (the dock's kept
//! apps, task 3.17b), and hear that settings changed. The service
//! is the only writer of the neueOS TOML files (ADR 0004), so the notifications extra's Do
//! Not Disturb toggle (task 3.15d4) goes through it.

use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{MatchRule, message::Type as MessageType};

use crate::Error;

/// The service's bus name.
pub const DESTINATION: &str = "org.neueos.Settings";
/// Its object.
pub const PATH: &str = "/org/neueos/Settings";
/// Its interface.
pub const INTERFACE: &str = "org.neueos.Settings1";

/// A `neue-settingsd` client.
#[derive(Debug, Clone)]
pub struct Settings {
    conn: Connection,
}

impl Settings {
    /// A client on the session bus. The service doesn't have to be running yet: calls fail
    /// until it is.
    pub fn session() -> Result<Self, Error> {
        Ok(Self::new(Connection::session()?))
    }

    /// A client on `conn` (tests pass a peer-to-peer connection).
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// The effective value of the boolean `key` (`shell.notifications.do-not-disturb`).
    /// An error when the service isn't there, the key is unknown or isn't a boolean.
    pub fn get_bool(&self, key: &str) -> Result<bool, Error> {
        let reply =
            self.conn
                .call_method(Some(DESTINATION), PATH, Some(INTERFACE), "Get", &(key,))?;
        let value: OwnedValue = reply.body().deserialize()?;
        Ok(bool::try_from(value).map_err(zbus::Error::from)?)
    }

    /// Sets the boolean `key` in the user's file.
    pub fn set_bool(&self, key: &str, value: bool) -> Result<(), Error> {
        self.conn.call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            "Set",
            &(key, Value::Bool(value)),
        )?;
        Ok(())
    }

    /// Sets the list of strings `key` (`shell.dock.pinned`) in the user's file.
    pub fn set_strings(&self, key: &str, values: &[String]) -> Result<(), Error> {
        self.conn.call_method(
            Some(DESTINATION),
            PATH,
            Some(INTERFACE),
            "Set",
            &(key, Value::from(values.to_vec())),
        )?;
        Ok(())
    }

    /// Subscribes to the service's `Changed` signal (adds a match rule on a bus).
    pub fn changes(&self) -> Result<SettingsChanges, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(INTERFACE)?
            .member("Changed")?
            .path(PATH)?
            .build();
        // The caller reads again on any change, so a burst beyond this loses nothing.
        let messages = MessageIterator::for_match_rule(rule, &self.conn, Some(16))?;
        Ok(SettingsChanges { messages })
    }
}

/// Says that some setting changed, so the caller reads its keys again. Blocking: iterate
/// it on a thread of its own. It ends when the connection closes.
pub struct SettingsChanges {
    messages: MessageIterator,
}

impl std::fmt::Debug for SettingsChanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsChanges").finish_non_exhaustive()
    }
}

impl Iterator for SettingsChanges {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        loop {
            match self.messages.next()? {
                Ok(_) => return Some(()),
                Err(err) => tracing::debug!(%err, "bad message on the settings signal stream"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use zbus::object_server::SignalEmitter;

    use super::*;

    fn wrong(err: zbus::zvariant::Error) -> zbus::fdo::Error {
        zbus::fdo::Error::InvalidArgs(err.to_string())
    }

    /// A stand-in for the service: one boolean key and one list of strings.
    #[derive(Default)]
    struct Fake {
        value: bool,
        pinned: Vec<String>,
    }

    #[zbus::interface(name = "org.neueos.Settings1")]
    impl Fake {
        fn get(&self, key: &str) -> zbus::fdo::Result<OwnedValue> {
            match key {
                "shell.notifications.do-not-disturb" => {
                    Value::Bool(self.value).try_into().map_err(wrong)
                }
                "shell.desktop.fit" => Value::from("fill").try_into().map_err(wrong),
                "shell.dock.pinned" => Value::from(self.pinned.clone()).try_into().map_err(wrong),
                _ => Err(zbus::fdo::Error::InvalidArgs(format!("unknown key {key}"))),
            }
        }

        async fn set(
            &mut self,
            key: &str,
            value: OwnedValue,
            #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        ) -> zbus::fdo::Result<()> {
            match key {
                "shell.notifications.do-not-disturb" => {
                    self.value = bool::try_from(value).map_err(wrong)?;
                }
                "shell.dock.pinned" => {
                    self.pinned = Vec::<String>::try_from(value).map_err(wrong)?;
                }
                _ => return Err(zbus::fdo::Error::InvalidArgs(format!("unknown key {key}"))),
            }
            Self::changed(&emitter, HashMap::new(), Vec::new()).await?;
            Ok(())
        }

        #[zbus(signal)]
        async fn changed(
            emitter: &SignalEmitter<'_>,
            changed: HashMap<String, OwnedValue>,
            unset: Vec<String>,
        ) -> zbus::Result<()>;
    }

    const KEY: &str = "shell.notifications.do-not-disturb";

    #[test]
    fn a_boolean_is_read_set_and_followed() {
        let (server, client) = crate::testing::pair().unwrap();
        server.object_server().at(PATH, Fake::default()).unwrap();
        let settings = Settings::new(client);
        assert!(!settings.get_bool(KEY).unwrap());

        let changes = settings.changes().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for () in changes {
                if tx.send(()).is_err() {
                    return;
                }
            }
        });
        settings.set_bool(KEY, true).unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(settings.get_bool(KEY).unwrap());
    }

    #[test]
    fn a_list_of_strings_is_set() {
        let (server, client) = crate::testing::pair().unwrap();
        server.object_server().at(PATH, Fake::default()).unwrap();
        let settings = Settings::new(client);
        let pinned = vec!["foot".to_owned(), "org.gnome.Nautilus".to_owned()];
        settings.set_strings("shell.dock.pinned", &pinned).unwrap();
        let reply = settings
            .conn
            .call_method(
                Some(DESTINATION),
                PATH,
                Some(INTERFACE),
                "Get",
                &("shell.dock.pinned",),
            )
            .unwrap();
        let value: OwnedValue = reply.body().deserialize().unwrap();
        assert_eq!(Vec::<String>::try_from(value).unwrap(), pinned);
        settings.set_strings("shell.dock.pinned", &[]).unwrap();
    }

    #[test]
    fn a_missing_service_unknown_key_or_other_type_is_an_error() {
        let (_server, client) = crate::testing::pair().unwrap();
        assert!(Settings::new(client).get_bool(KEY).is_err());

        let (server, client) = crate::testing::pair().unwrap();
        server.object_server().at(PATH, Fake::default()).unwrap();
        let settings = Settings::new(client);
        assert!(settings.get_bool("shell.nothing").is_err());
        assert!(settings.get_bool("shell.desktop.fit").is_err());
        assert!(settings.set_bool("shell.nothing", true).is_err());
    }
}
