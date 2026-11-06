//! BlueZ (`org.bluez`) for the Bluetooth menu extra (task 3.11a).
//!
//! [`Bluetooth::state`] reads the first adapter and the devices that belong to it from
//! BlueZ's object manager; [`Bluetooth::set_powered`], [`Bluetooth::connect`] and
//! [`Bluetooth::disconnect`] act on them; [`BluetoothChanges`] says when to read again.
//! Zbus proxies, not `bluer`, which links libdbus (AGENTS.md, ADR 0005).
//!
//! Pairing a new device is not here: it needs a BlueZ agent (a later task). Only devices
//! that are already paired are listed.
//!
//! `connect` blocks until BlueZ is done, which can take many seconds: call it from a
//! thread of its own, on a connection whose method timeout allows for that.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::Error;

/// BlueZ's bus name (on the system bus).
pub const DESTINATION: &str = "org.bluez";
/// The adapter interface.
pub const ADAPTER: &str = "org.bluez.Adapter1";
/// The device interface.
pub const DEVICE: &str = "org.bluez.Device1";

const ROOT: &str = "/";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// An adapter: the machine's Bluetooth radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// Its object path (`/org/bluez/hci0`), which names it in calls.
    pub path: String,
    /// What to call it (`Alias`, which defaults to the host name).
    pub name: String,
    /// The radio is on.
    pub powered: bool,
}

/// A paired device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// Its object path, which names it in calls.
    pub path: String,
    /// Its `Alias`: the name the user sees (BlueZ falls back to the address).
    pub name: String,
    /// The Bluetooth address, `AA:BB:CC:DD:EE:FF`.
    pub address: String,
    /// BlueZ's guess at what it is, as an icon name (`audio-headphones`, `input-mouse`…).
    pub icon: Option<String>,
    pub connected: bool,
}

/// The adapter and its paired devices as of one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub adapter: Adapter,
    /// Paired devices, connected ones first, then by name.
    pub devices: Vec<Device>,
}

/// What `GetManagedObjects` returns: every object, its interfaces and their properties.
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn get<T>(props: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    props.get(name)?.try_clone().ok()?.try_into().ok()
}

impl State {
    /// `None` when there is no adapter. With several, the first by path (`hci0`).
    fn from_objects(objects: &Objects) -> Option<Self> {
        let mut adapters: Vec<_> = objects
            .iter()
            .filter_map(|(path, interfaces)| Some((path, interfaces.get(ADAPTER)?)))
            .collect();
        adapters.sort_by(|a, b| a.0.cmp(b.0));
        let (path, props) = adapters.first()?;
        let adapter = Adapter {
            path: path.to_string(),
            name: get::<String>(props, "Alias")
                .or_else(|| get(props, "Name"))
                .unwrap_or_default(),
            powered: get(props, "Powered").unwrap_or(false),
        };
        let mut devices: Vec<Device> = objects
            .iter()
            .filter_map(|(path, interfaces)| {
                let props = interfaces.get(DEVICE)?;
                if get::<OwnedObjectPath>(props, "Adapter")?.as_str() != adapter.path {
                    return None;
                }
                if !get::<bool>(props, "Paired").unwrap_or(false) {
                    return None;
                }
                let address = get::<String>(props, "Address").unwrap_or_default();
                Some(Device {
                    path: path.to_string(),
                    name: get::<String>(props, "Alias")
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| address.clone()),
                    address,
                    icon: get(props, "Icon"),
                    connected: get(props, "Connected").unwrap_or(false),
                })
            })
            .collect();
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.path.cmp(&b.path))
        });
        Some(Self { adapter, devices })
    }
}

/// A BlueZ client.
#[derive(Debug, Clone)]
pub struct Bluetooth {
    conn: Connection,
}

impl Bluetooth {
    /// A client on `conn`: the system bus, or a peer-to-peer connection in tests.
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Connects to the system bus.
    pub fn system() -> Result<Self, Error> {
        Ok(Self::new(Connection::system()?))
    }

    /// The adapter and its paired devices; `None` when the machine has no adapter. An
    /// error when BlueZ isn't there.
    pub fn state(&self) -> Result<Option<State>, Error> {
        let reply = self.conn.call_method(
            Some(DESTINATION),
            ROOT,
            Some(OBJECT_MANAGER),
            "GetManagedObjects",
            &(),
        )?;
        let objects: Objects = reply.body().deserialize()?;
        Ok(State::from_objects(&objects))
    }

    /// Turns the adapter at `adapter` (its [`Adapter::path`]) on or off.
    pub fn set_powered(&self, adapter: &str, powered: bool) -> Result<(), Error> {
        let path = object_path(adapter)?;
        self.conn.call_method(
            Some(DESTINATION),
            path,
            Some(PROPERTIES),
            "Set",
            &(ADAPTER, "Powered", Value::from(powered)),
        )?;
        Ok(())
    }

    /// Connects the paired `device` (its [`Device::path`]). Blocks; see the module docs.
    pub fn connect(&self, device: &str) -> Result<(), Error> {
        self.device_call(device, "Connect")
    }

    /// Disconnects `device`.
    pub fn disconnect(&self, device: &str) -> Result<(), Error> {
        self.device_call(device, "Disconnect")
    }

    fn device_call(&self, device: &str, method: &str) -> Result<(), Error> {
        let path = object_path(device)?;
        self.conn
            .call_method(Some(DESTINATION), path, Some(DEVICE), method, &())?;
        Ok(())
    }

    /// Subscribes to changes of BlueZ's objects (adds match rules on a bus).
    pub fn changes(&self) -> Result<BluetoothChanges, Error> {
        BluetoothChanges::new(&self.conn)
    }
}

fn object_path(path: &str) -> Result<ObjectPath<'_>, Error> {
    ObjectPath::try_from(path).map_err(|_| Error::Path(path.to_owned()))
}

/// Says that something of BlueZ's changed (a property of the adapter or of a device, a
/// device added or removed), so the caller reads again with [`Bluetooth::state`].
/// Blocking: iterate it on a thread of its own. It ends when the connection closes.
pub struct BluetoothChanges {
    changes: mpsc::Receiver<()>,
}

impl std::fmt::Debug for BluetoothChanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BluetoothChanges").finish_non_exhaustive()
    }
}

impl BluetoothChanges {
    fn new(conn: &Connection) -> Result<Self, Error> {
        // Properties of any BlueZ object; the object manager's own signals come from the
        // root, outside that namespace, so they are a second subscription.
        let properties = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?
            .path_namespace("/org/bluez")?
            .build();
        let objects = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(OBJECT_MANAGER)?
            .path(ROOT)?
            .build();
        // The caller re-reads on any change, so a burst beyond this loses nothing.
        let properties = MessageIterator::for_match_rule(properties, conn, Some(64))?;
        let objects = MessageIterator::for_match_rule(objects, conn, Some(64))?;
        let (tx, changes) = mpsc::channel();
        forward(properties, tx.clone(), |message| {
            message
                .body()
                .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                .is_ok_and(|(interface, _, _)| interface.starts_with("org.bluez."))
        });
        forward(objects, tx, |message| {
            matches!(
                message.header().member().map(|m| m.as_str()),
                Some("InterfacesAdded" | "InterfacesRemoved")
            )
        });
        Ok(Self { changes })
    }
}

/// Sends `()` on `tx` for each message of `messages` that `keep` accepts, on a thread of
/// its own that ends with the stream or the receiver.
fn forward(
    messages: MessageIterator,
    tx: mpsc::Sender<()>,
    keep: impl Fn(&zbus::Message) -> bool + Send + 'static,
) {
    thread::spawn(move || {
        for message in messages {
            match message {
                Ok(message) if keep(&message) => {
                    if tx.send(()).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(err) => tracing::debug!(%err, "bad message on the BlueZ signal stream"),
            }
        }
    });
}

impl Iterator for BluetoothChanges {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        self.changes.recv().ok()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use zbus::fdo::ObjectManager;

    use super::*;

    const HCI0: &str = "/org/bluez/hci0";
    const BUDS: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01";
    const MOUSE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_02";
    const WAIT: Duration = Duration::from_secs(5);

    struct FakeAdapter {
        powered: bool,
    }

    #[zbus::interface(name = "org.bluez.Adapter1")]
    impl FakeAdapter {
        #[zbus(property)]
        fn alias(&self) -> String {
            "neue".to_owned()
        }
        #[zbus(property)]
        fn powered(&self) -> bool {
            self.powered
        }
        #[zbus(property)]
        fn set_powered(&mut self, powered: bool) {
            self.powered = powered;
        }
    }

    struct FakeDevice {
        adapter: &'static str,
        alias: &'static str,
        address: &'static str,
        paired: bool,
        connected: bool,
        /// `Connect` and `Disconnect` calls, in order.
        log: Arc<Mutex<Vec<String>>>,
    }

    #[zbus::interface(name = "org.bluez.Device1")]
    impl FakeDevice {
        #[zbus(property)]
        fn adapter(&self) -> OwnedObjectPath {
            ObjectPath::try_from(self.adapter).unwrap().into()
        }
        #[zbus(property)]
        fn alias(&self) -> String {
            self.alias.to_owned()
        }
        #[zbus(property)]
        fn address(&self) -> String {
            self.address.to_owned()
        }
        #[zbus(property)]
        fn icon(&self) -> String {
            "audio-headphones".to_owned()
        }
        #[zbus(property)]
        fn paired(&self) -> bool {
            self.paired
        }
        #[zbus(property)]
        fn connected(&self) -> bool {
            self.connected
        }
        fn connect(&mut self) -> zbus::fdo::Result<()> {
            self.log.lock().unwrap().push("Connect".to_owned());
            if self.alias == "Broken" {
                return Err(zbus::fdo::Error::Failed("page timeout".to_owned()));
            }
            self.connected = true;
            Ok(())
        }
        fn disconnect(&mut self) {
            self.log.lock().unwrap().push("Disconnect".to_owned());
            self.connected = false;
        }
    }

    fn device(
        alias: &'static str,
        address: &'static str,
        paired: bool,
        connected: bool,
        log: &Arc<Mutex<Vec<String>>>,
    ) -> FakeDevice {
        FakeDevice {
            adapter: HCI0,
            alias,
            address,
            paired,
            connected,
            log: log.clone(),
        }
    }

    /// A BlueZ with an adapter (powered), connected earbuds, a paired mouse and a device
    /// that is only seen, not paired.
    fn setup(powered: bool) -> (Connection, Bluetooth, Arc<Mutex<Vec<String>>>) {
        let (server, client) = crate::testing::pair().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let objects = server.object_server().clone();
        objects.at(ROOT, ObjectManager).unwrap();
        objects.at(HCI0, FakeAdapter { powered }).unwrap();
        objects
            .at(
                BUDS,
                device("Earbuds", "AA:BB:CC:DD:EE:01", true, true, &log),
            )
            .unwrap();
        objects
            .at(
                MOUSE,
                device("mouse", "AA:BB:CC:DD:EE:02", true, false, &log),
            )
            .unwrap();
        objects
            .at(
                "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_03",
                device("Stranger", "AA:BB:CC:DD:EE:03", false, false, &log),
            )
            .unwrap();
        (server, Bluetooth::new(client), log)
    }

    #[test]
    fn reads_the_adapter_and_paired_devices() {
        let (_server, bluetooth, _log) = setup(true);
        let state = bluetooth.state().unwrap().unwrap();
        assert_eq!(
            state.adapter,
            Adapter {
                path: HCI0.to_owned(),
                name: "neue".to_owned(),
                powered: true
            }
        );
        // Connected first, then by name; the unpaired stranger isn't listed.
        let names: Vec<_> = state.devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Earbuds", "mouse"]);
        assert!(state.devices[0].connected);
        assert!(!state.devices[1].connected);
        assert_eq!(state.devices[0].address, "AA:BB:CC:DD:EE:01");
        assert_eq!(state.devices[0].icon.as_deref(), Some("audio-headphones"));
        assert_eq!(state.devices[0].path, BUDS);
    }

    #[test]
    fn no_adapter_is_none() {
        let (server, client) = crate::testing::pair().unwrap();
        server.object_server().at(ROOT, ObjectManager).unwrap();
        assert_eq!(Bluetooth::new(client).state().unwrap(), None);
    }

    #[test]
    fn a_missing_bluez_is_an_error() {
        let (_server, client) = crate::testing::pair().unwrap();
        assert!(Bluetooth::new(client).state().is_err());
    }

    #[test]
    fn the_adapter_powers_on_and_off() {
        let (_server, bluetooth, _log) = setup(false);
        assert!(!bluetooth.state().unwrap().unwrap().adapter.powered);
        bluetooth.set_powered(HCI0, true).unwrap();
        assert!(bluetooth.state().unwrap().unwrap().adapter.powered);
        bluetooth.set_powered(HCI0, false).unwrap();
        assert!(!bluetooth.state().unwrap().unwrap().adapter.powered);
    }

    #[test]
    fn devices_connect_and_disconnect() {
        let (_server, bluetooth, log) = setup(true);
        bluetooth.connect(MOUSE).unwrap();
        bluetooth.disconnect(BUDS).unwrap();
        assert_eq!(*log.lock().unwrap(), ["Connect", "Disconnect"]);
        let state = bluetooth.state().unwrap().unwrap();
        let connected = |name: &str| {
            state
                .devices
                .iter()
                .find(|d| d.name == name)
                .unwrap()
                .connected
        };
        assert!(connected("mouse"));
        assert!(!connected("Earbuds"));
    }

    #[test]
    fn a_failed_connect_and_bad_paths_are_errors() {
        let (server, bluetooth, log) = setup(true);
        server
            .object_server()
            .at(
                "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_04",
                device("Broken", "AA:BB:CC:DD:EE:04", true, false, &log),
            )
            .unwrap();
        assert!(
            bluetooth
                .connect("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_04")
                .is_err()
        );
        assert!(bluetooth.connect("/org/bluez/hci0/dev_gone").is_err());
        assert!(matches!(bluetooth.connect("no/path"), Err(Error::Path(_))));
        assert!(matches!(
            bluetooth.set_powered("", true),
            Err(Error::Path(_))
        ));
    }

    /// Changes, forwarded from a thread so the test can wait on them.
    fn watch(bluetooth: &Bluetooth) -> mpsc::Receiver<()> {
        let changes = bluetooth.changes().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for () in changes {
                if tx.send(()).is_err() {
                    return;
                }
            }
        });
        rx
    }

    #[test]
    fn a_changed_property_is_signalled() {
        let (server, bluetooth, _log) = setup(true);
        let changes = watch(&bluetooth);
        let iface = server
            .object_server()
            .interface::<_, FakeDevice>(MOUSE)
            .unwrap();
        iface.get_mut().connected = true;
        zbus::block_on(iface.get().connected_changed(iface.signal_emitter())).unwrap();
        changes.recv_timeout(WAIT).unwrap();
        assert!(bluetooth.state().unwrap().unwrap().devices[1].connected);
    }

    #[test]
    fn a_device_appearing_and_disappearing_is_signalled() {
        let (server, bluetooth, log) = setup(true);
        let changes = watch(&bluetooth);
        let new = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_05";
        server
            .object_server()
            .at(
                new,
                device("Keyboard", "AA:BB:CC:DD:EE:05", true, false, &log),
            )
            .unwrap();
        changes.recv_timeout(WAIT).unwrap();
        assert_eq!(bluetooth.state().unwrap().unwrap().devices.len(), 3);
        server.object_server().remove::<FakeDevice, _>(new).unwrap();
        changes.recv_timeout(WAIT).unwrap();
        assert_eq!(bluetooth.state().unwrap().unwrap().devices.len(), 2);
    }
}
