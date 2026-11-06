//! NetworkManager (`org.freedesktop.NetworkManager`) for the network menu extra (task 3.12a).
//!
//! [`Network::state`] reads the wired and Wi-Fi devices and the Wi-Fi access points as one
//! [`State`]; [`Network::set_wifi_enabled`], [`Network::request_scan`], [`Network::join`],
//! [`Network::connect`] and [`Network::disconnect`] act on them; [`NetworkChanges`] says
//! when to read again. Zbus proxies, not `networkmanager` or similar crates (D-Bus stays
//! on zbus, ADR 0005).
//!
//! NetworkManager has no object manager, so a read is a handful of `GetAll` calls per
//! device and access point. A device or access point that disappears during a read is
//! skipped, not an error.
//!
//! Joining a network that isn't saved yet works for open ones only. A protected one needs
//! a secret agent to ask for the password (a later task): [`Network::join`] fails with
//! [`Error::SecretRequired`] and [`AccessPoint::joinable`] is false, so a menu can grey it
//! out. Use `nmtui` until then.

use std::collections::HashMap;

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::Error;

/// NetworkManager's bus name (on the system bus).
pub const DESTINATION: &str = "org.freedesktop.NetworkManager";
/// The manager object's path; everything else is below it.
pub const ROOT: &str = "/org/freedesktop/NetworkManager";
/// The manager interface.
pub const MANAGER: &str = "org.freedesktop.NetworkManager";
/// A network device.
pub const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
/// The extra interface of an Ethernet device.
pub const WIRED: &str = "org.freedesktop.NetworkManager.Device.Wired";
/// The extra interface of a Wi-Fi device.
pub const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
/// A Wi-Fi access point.
pub const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
/// An active connection.
pub const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
/// The saved connections' service, and its path.
pub const SETTINGS: &str = "org.freedesktop.NetworkManager.Settings";
pub const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
/// One saved connection.
pub const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// Object path that stands for "none" in NetworkManager's properties and arguments.
const NONE: &str = "/";

/// `NMDeviceType`.
const TYPE_ETHERNET: u32 = 1;
const TYPE_WIFI: u32 = 2;
/// `NMDeviceState`: managed by something else, and fully connected.
const STATE_UNMANAGED: u32 = 10;
const STATE_ACTIVATED: u32 = 100;
/// Preparing, configuring, waiting for secrets, getting an address, checking it: still
/// on the way to connected.
const STATE_CONNECTING: std::ops::RangeInclusive<u32> = 40..=90;
/// `NM80211ApFlags::PRIVACY`.
const AP_PRIVACY: u32 = 1;

/// A wired (Ethernet) device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wired {
    /// Its object path, which names it in calls.
    pub device: String,
    /// The connection it is on, or its interface (`enp0s1`) when it is on none.
    pub name: String,
    /// A cable is plugged in.
    pub carrier: bool,
    pub connected: bool,
    /// Connecting, not there yet.
    pub connecting: bool,
}

/// A Wi-Fi network in range. Several access points of one network (a mesh) are one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    /// The object path of the entry's access point (the active one, else the strongest),
    /// which names it in [`Network::join`].
    pub path: String,
    pub ssid: String,
    /// Signal strength, 0 to 100.
    pub strength: u8,
    /// Needs a password (the lock in the list).
    pub secured: bool,
    /// A saved connection for this network exists, so joining needs no password.
    pub saved: bool,
    /// The Wi-Fi device is on this network.
    pub active: bool,
}

impl AccessPoint {
    /// [`Network::join`] can work: the network is open or already saved.
    pub fn joinable(&self) -> bool {
        self.saved || !self.secured
    }
}

/// The Wi-Fi device (the first, by interface name) and the networks it sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wifi {
    /// Its object path, which names it in calls.
    pub device: String,
    pub connected: bool,
    /// Connecting, not there yet.
    pub connecting: bool,
    /// Connected one first, then by signal, strongest first.
    pub access_points: Vec<AccessPoint>,
}

/// The network devices as of one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// The Wi-Fi switch (NetworkManager's `WirelessEnabled`).
    pub wifi_enabled: bool,
    /// Managed Ethernet devices, by interface name.
    pub wired: Vec<Wired>,
    /// `None` without a Wi-Fi device.
    pub wifi: Option<Wifi>,
}

type Props = HashMap<String, OwnedValue>;

fn get<T>(props: &Props, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    props.get(name)?.try_clone().ok()?.try_into().ok()
}

fn is_none(path: &OwnedObjectPath) -> bool {
    path.as_str() == NONE
}

/// A device or access point that went away while it was being read.
type Gone = zbus::Error;

/// A saved Wi-Fi connection.
struct Saved {
    path: String,
    ssid: Vec<u8>,
}

/// A NetworkManager client.
#[derive(Debug, Clone)]
pub struct Network {
    conn: Connection,
}

impl Network {
    /// A client on `conn`: the system bus, or a peer-to-peer connection in tests.
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Connects to the system bus.
    pub fn system() -> Result<Self, Error> {
        Ok(Self::new(Connection::system()?))
    }

    /// The wired and Wi-Fi devices. An error when NetworkManager isn't there.
    pub fn state(&self) -> Result<State, Error> {
        let manager = self.props(ROOT, MANAGER)?;
        let devices: Vec<OwnedObjectPath> = get(&manager, "Devices").unwrap_or_default();
        let mut wired = Vec::new();
        let mut wifis = Vec::new();
        for path in &devices {
            let read = self.read_device(path.as_str());
            match read {
                Ok(Some(Found::Wired(device, interface))) => wired.push((interface, device)),
                Ok(Some(Found::Wifi(device, interface, access_points))) => {
                    wifis.push((interface, device, access_points));
                }
                Ok(None) => {}
                Err(err) => tracing::debug!(%err, device = path.as_str(), "skipping a device"),
            }
        }
        wired.sort_by(|a, b| a.0.cmp(&b.0));
        wifis.sort_by(|a, b| a.0.cmp(&b.0));
        let wifi = match wifis.into_iter().next() {
            Some((_, device, raw)) => Some(self.wifi(device, raw)?),
            None => None,
        };
        Ok(State {
            wifi_enabled: get(&manager, "WirelessEnabled").unwrap_or(false),
            wired: wired.into_iter().map(|(_, device)| device).collect(),
            wifi,
        })
    }

    /// Turns Wi-Fi on or off (the switch; Wi-Fi off disconnects it).
    pub fn set_wifi_enabled(&self, enabled: bool) -> Result<(), Error> {
        self.conn.call_method(
            Some(DESTINATION),
            ROOT,
            Some(PROPERTIES),
            "Set",
            &(MANAGER, "WirelessEnabled", Value::from(enabled)),
        )?;
        Ok(())
    }

    /// Asks the Wi-Fi `device` (its [`Wifi::device`]) to look for networks again. The new
    /// list arrives as changes. NetworkManager refuses scans that come too soon after
    /// one: that is an error to log, not to show.
    pub fn request_scan(&self, device: &str) -> Result<(), Error> {
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        self.conn.call_method(
            Some(DESTINATION),
            object_path(device)?,
            Some(WIRELESS),
            "RequestScan",
            &(options,),
        )?;
        Ok(())
    }

    /// Joins the network of `access_point` (its [`AccessPoint::path`]) on the Wi-Fi
    /// `device`: with its saved connection, or, for an open network, a new one that
    /// NetworkManager saves. Returns once NetworkManager has started connecting; the
    /// device's state follows as changes. A protected network that isn't saved is
    /// [`Error::SecretRequired`].
    pub fn join(&self, device: &str, access_point: &str) -> Result<(), Error> {
        let device_path = object_path(device)?;
        let access_point_path = object_path(access_point)?;
        let ap = self.props(access_point, ACCESS_POINT)?;
        let ssid: Vec<u8> = get(&ap, "Ssid").unwrap_or_default();
        let name = String::from_utf8_lossy(&ssid).into_owned();
        let available: Vec<OwnedObjectPath> = self
            .props(device, DEVICE)
            .ok()
            .and_then(|props| get(&props, "AvailableConnections"))
            .unwrap_or_default();
        let mut saved: Vec<Saved> = self
            .saved_wifi()?
            .into_iter()
            .filter(|connection| connection.ssid == ssid)
            .collect();
        // One NetworkManager says fits this device first.
        saved.sort_by_key(|connection| {
            !available
                .iter()
                .any(|path| path.as_str() == connection.path)
        });
        if let Some(connection) = saved.first() {
            self.conn.call_method(
                Some(DESTINATION),
                ROOT,
                Some(MANAGER),
                "ActivateConnection",
                &(
                    object_path(&connection.path)?,
                    device_path,
                    access_point_path,
                ),
            )?;
            return Ok(());
        }
        if is_secured(&ap) {
            return Err(Error::SecretRequired(name));
        }
        let mut connection: HashMap<&str, HashMap<&str, Value<'_>>> = HashMap::new();
        connection.insert(
            "connection",
            HashMap::from([
                ("type", Value::from("802-11-wireless")),
                ("id", Value::from(name.as_str())),
            ]),
        );
        connection.insert(
            "802-11-wireless",
            HashMap::from([("ssid", Value::from(ssid))]),
        );
        self.conn.call_method(
            Some(DESTINATION),
            ROOT,
            Some(MANAGER),
            "AddAndActivateConnection",
            &(connection, device_path, access_point_path),
        )?;
        Ok(())
    }

    /// Connects the wired `device` (its [`Wired::device`]) with the first saved connection
    /// that fits it. [`Error::NoConnection`] when there is none.
    pub fn connect(&self, device: &str) -> Result<(), Error> {
        let device_path = object_path(device)?;
        let props = self.props(device, DEVICE)?;
        let available: Vec<OwnedObjectPath> =
            get(&props, "AvailableConnections").unwrap_or_default();
        let Some(connection) = available.into_iter().next() else {
            return Err(Error::NoConnection(
                get(&props, "Interface").unwrap_or_else(|| device.to_owned()),
            ));
        };
        self.conn.call_method(
            Some(DESTINATION),
            ROOT,
            Some(MANAGER),
            "ActivateConnection",
            &(connection, device_path, object_path(NONE)?),
        )?;
        Ok(())
    }

    /// Disconnects `device`, wired or Wi-Fi. NetworkManager then doesn't connect it again
    /// by itself until the user does.
    pub fn disconnect(&self, device: &str) -> Result<(), Error> {
        self.conn.call_method(
            Some(DESTINATION),
            object_path(device)?,
            Some(DEVICE),
            "Disconnect",
            &(),
        )?;
        Ok(())
    }

    /// Subscribes to changes of NetworkManager's objects (adds a match rule on a bus).
    pub fn changes(&self) -> Result<NetworkChanges, Error> {
        NetworkChanges::new(&self.conn)
    }

    fn props(&self, path: &str, interface: &str) -> Result<Props, Error> {
        let reply = self.conn.call_method(
            Some(DESTINATION),
            object_path(path)?,
            Some(PROPERTIES),
            "GetAll",
            &(interface,),
        )?;
        Ok(reply.body().deserialize()?)
    }

    /// One device, unless it is of another kind or unmanaged.
    fn read_device(&self, path: &str) -> Result<Option<Found>, Gone> {
        let props = self.props(path, DEVICE).map_err(gone)?;
        let state: u32 = get(&props, "State").unwrap_or(0);
        if state == STATE_UNMANAGED {
            return Ok(None);
        }
        let interface: String = get(&props, "Interface").unwrap_or_default();
        let connected = state == STATE_ACTIVATED;
        let connecting = STATE_CONNECTING.contains(&state);
        match get::<u32>(&props, "DeviceType") {
            Some(TYPE_ETHERNET) => {
                let wired = self.props(path, WIRED).map_err(gone)?;
                let name = get::<OwnedObjectPath>(&props, "ActiveConnection")
                    .filter(|active| !is_none(active))
                    .and_then(|active| self.props(active.as_str(), ACTIVE).ok())
                    .and_then(|active| get::<String>(&active, "Id"))
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| interface.clone());
                Ok(Some(Found::Wired(
                    Wired {
                        device: path.to_owned(),
                        name,
                        carrier: get(&wired, "Carrier").unwrap_or(false),
                        connected,
                        connecting,
                    },
                    interface,
                )))
            }
            Some(TYPE_WIFI) => {
                let wireless = self.props(path, WIRELESS).map_err(gone)?;
                let active: Option<String> = get::<OwnedObjectPath>(&wireless, "ActiveAccessPoint")
                    .filter(|active| !is_none(active))
                    .map(|active| active.to_string());
                let paths: Vec<OwnedObjectPath> =
                    get(&wireless, "AccessPoints").unwrap_or_default();
                let mut access_points = Vec::new();
                for ap in &paths {
                    match self.props(ap.as_str(), ACCESS_POINT) {
                        Ok(props) => {
                            let is_active = active.as_deref() == Some(ap.as_str());
                            access_points.push(RawAccessPoint::read(
                                ap.as_str(),
                                &props,
                                is_active,
                            ));
                        }
                        Err(err) => {
                            tracing::debug!(%err, ap = ap.as_str(), "skipping an access point");
                        }
                    }
                }
                Ok(Some(Found::Wifi(
                    Wifi {
                        device: path.to_owned(),
                        connected,
                        connecting,
                        access_points: Vec::new(),
                    },
                    interface,
                    access_points,
                )))
            }
            _ => Ok(None),
        }
    }

    /// Fills in the networks of the Wi-Fi device: merged by name and marked with the
    /// saved connections.
    fn wifi(&self, mut wifi: Wifi, raw: Vec<RawAccessPoint>) -> Result<Wifi, Error> {
        let saved: Vec<Vec<u8>> = if raw.is_empty() {
            Vec::new()
        } else {
            self.saved_wifi()?
                .into_iter()
                .map(|connection| connection.ssid)
                .collect()
        };
        let mut merged: HashMap<Vec<u8>, AccessPoint> = HashMap::new();
        for ap in raw {
            // Hidden networks have no name to list.
            if ap.ssid.is_empty() {
                continue;
            }
            let entry = AccessPoint {
                path: ap.path,
                ssid: String::from_utf8_lossy(&ap.ssid).into_owned(),
                strength: ap.strength,
                secured: ap.secured,
                saved: saved.contains(&ap.ssid),
                active: ap.active,
            };
            match merged.get(&ap.ssid) {
                // The active access point stands for its network, else the strongest.
                Some(known) if (known.active, known.strength) >= (entry.active, entry.strength) => {
                }
                _ => {
                    merged.insert(ap.ssid, entry);
                }
            }
        }
        let mut access_points: Vec<AccessPoint> = merged.into_values().collect();
        access_points.sort_by(|a, b| {
            b.active
                .cmp(&a.active)
                .then_with(|| b.strength.cmp(&a.strength))
                .then_with(|| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
                .then_with(|| a.path.cmp(&b.path))
        });
        wifi.access_points = access_points;
        Ok(wifi)
    }

    /// The saved Wi-Fi connections. Ones that can't be read (another user's, say) are
    /// left out.
    fn saved_wifi(&self) -> Result<Vec<Saved>, Error> {
        let reply = self.conn.call_method(
            Some(DESTINATION),
            SETTINGS_PATH,
            Some(SETTINGS),
            "ListConnections",
            &(),
        )?;
        let paths: Vec<OwnedObjectPath> = reply.body().deserialize()?;
        let mut saved = Vec::new();
        for path in paths {
            let settings: Result<HashMap<String, Props>, Error> = (|| {
                let reply = self.conn.call_method(
                    Some(DESTINATION),
                    object_path(path.as_str())?,
                    Some(CONNECTION),
                    "GetSettings",
                    &(),
                )?;
                Ok(reply.body().deserialize()?)
            })();
            let Ok(settings) = settings else {
                tracing::debug!(path = path.as_str(), "can't read a saved connection");
                continue;
            };
            let is_wifi = settings
                .get("connection")
                .and_then(|connection| get::<String>(connection, "type"))
                .is_some_and(|kind| kind == "802-11-wireless");
            let ssid = settings
                .get("802-11-wireless")
                .and_then(|wireless| get::<Vec<u8>>(wireless, "ssid"));
            if let (true, Some(ssid)) = (is_wifi, ssid) {
                saved.push(Saved {
                    path: path.to_string(),
                    ssid,
                });
            }
        }
        Ok(saved)
    }
}

/// What `read_device` found, with the interface name the devices are sorted by.
enum Found {
    Wired(Wired, String),
    Wifi(Wifi, String, Vec<RawAccessPoint>),
}

/// An access point as read, before they are merged by name.
struct RawAccessPoint {
    path: String,
    ssid: Vec<u8>,
    strength: u8,
    secured: bool,
    active: bool,
}

impl RawAccessPoint {
    fn read(path: &str, props: &Props, active: bool) -> Self {
        Self {
            path: path.to_owned(),
            ssid: get(props, "Ssid").unwrap_or_default(),
            strength: get::<u8>(props, "Strength").unwrap_or(0).min(100),
            secured: is_secured(props),
            active,
        }
    }
}

/// An access point wants a password: the privacy flag, or any WPA or RSN (WPA2, WPA3)
/// flags.
fn is_secured(props: &Props) -> bool {
    get::<u32>(props, "Flags").unwrap_or(0) & AP_PRIVACY != 0
        || get::<u32>(props, "WpaFlags").unwrap_or(0) != 0
        || get::<u32>(props, "RsnFlags").unwrap_or(0) != 0
}

fn gone(err: Error) -> Gone {
    match err {
        Error::Dbus(err) => err,
        other => zbus::Error::Failure(other.to_string()),
    }
}

fn object_path(path: &str) -> Result<ObjectPath<'_>, Error> {
    ObjectPath::try_from(path).map_err(|_| Error::Path(path.to_owned()))
}

/// Says that something of NetworkManager's changed (a property of a device, an access
/// point or an active connection; a device, access point or saved connection added or
/// removed), so the caller reads again with [`Network::state`]. Blocking: iterate it on a
/// thread of its own. It ends when the connection closes.
///
/// Changes come in bursts (every scan touches the access points), so a caller should
/// wait a moment for the burst to end before it reads. Traffic counters and the
/// last-seen and last-scan times, which change all the time and mean nothing, don't count.
pub struct NetworkChanges {
    messages: MessageIterator,
}

impl std::fmt::Debug for NetworkChanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkChanges").finish_non_exhaustive()
    }
}

/// Properties that change without the picture changing.
const NOISE: [&str; 2] = ["LastSeen", "LastScan"];

impl NetworkChanges {
    fn new(conn: &Connection) -> Result<Self, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .path_namespace(ROOT)?
            .build();
        // The caller re-reads on any change, so a burst beyond this loses nothing.
        let messages = MessageIterator::for_match_rule(rule, conn, Some(64))?;
        Ok(Self { messages })
    }
}

/// A signal of NetworkManager's that the menu shows something of.
fn is_change(message: &zbus::Message) -> bool {
    match message.header().member().map(|member| member.as_str()) {
        Some(
            "DeviceAdded" | "DeviceRemoved" | "AccessPointAdded" | "AccessPointRemoved"
            | "NewConnection" | "ConnectionRemoved",
        ) => true,
        Some("PropertiesChanged") => message
            .body()
            .deserialize::<(String, Props, Vec<String>)>()
            .is_ok_and(|(interface, changed, invalidated)| {
                interface.starts_with(MANAGER)
                    && !interface.ends_with(".Statistics")
                    && (!invalidated.is_empty()
                        || changed.keys().any(|key| !NOISE.contains(&key.as_str())))
            }),
        _ => false,
    }
}

impl Iterator for NetworkChanges {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        loop {
            match self.messages.next()? {
                Ok(message) if is_change(&message) => return Some(()),
                Ok(_) => {}
                Err(err) => {
                    tracing::debug!(%err, "bad message on the NetworkManager signal stream")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use zbus::object_server::SignalEmitter;

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);
    const ETH: &str = "/org/freedesktop/NetworkManager/Devices/1";
    const WLAN: &str = "/org/freedesktop/NetworkManager/Devices/2";
    const LO: &str = "/org/freedesktop/NetworkManager/Devices/3";
    const ACTIVE_ETH: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
    const WIRED_CONN: &str = "/org/freedesktop/NetworkManager/Settings/1";
    const HOME_CONN: &str = "/org/freedesktop/NetworkManager/Settings/2";
    const VPN_CONN: &str = "/org/freedesktop/NetworkManager/Settings/3";
    const HOME_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
    const HOME_AP_WEAK: &str = "/org/freedesktop/NetworkManager/AccessPoint/2";
    const CAFE_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/3";
    const LIBRARY_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/4";
    const HIDDEN_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/5";

    /// What the mock was asked to do, in order: `Method(arg, …)`.
    type Log = Arc<Mutex<Vec<String>>>;

    fn path(path: &str) -> OwnedObjectPath {
        ObjectPath::try_from(path.to_owned()).unwrap().into()
    }

    struct FakeManager {
        wifi_enabled: bool,
        devices: Vec<&'static str>,
        log: Log,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager")]
    impl FakeManager {
        #[zbus(property)]
        fn devices(&self) -> Vec<OwnedObjectPath> {
            self.devices.iter().map(|device| path(device)).collect()
        }
        #[zbus(property)]
        fn wireless_enabled(&self) -> bool {
            self.wifi_enabled
        }
        #[zbus(property)]
        fn set_wireless_enabled(&mut self, enabled: bool) {
            self.wifi_enabled = enabled;
        }
        fn activate_connection(
            &self,
            connection: OwnedObjectPath,
            device: OwnedObjectPath,
            specific_object: OwnedObjectPath,
        ) -> OwnedObjectPath {
            self.log.lock().unwrap().push(format!(
                "Activate({connection}, {device}, {specific_object})"
            ));
            path(ACTIVE_ETH)
        }
        fn add_and_activate_connection(
            &self,
            connection: HashMap<String, HashMap<String, OwnedValue>>,
            device: OwnedObjectPath,
            specific_object: OwnedObjectPath,
        ) -> (OwnedObjectPath, OwnedObjectPath) {
            let ssid: Vec<u8> = get(&connection["802-11-wireless"], "ssid").unwrap();
            let id: String = get(&connection["connection"], "id").unwrap();
            let kind: String = get(&connection["connection"], "type").unwrap();
            self.log.lock().unwrap().push(format!(
                "AddAndActivate({kind}, {id}, {}, {device}, {specific_object})",
                String::from_utf8(ssid).unwrap()
            ));
            (path(HOME_CONN), path(ACTIVE_ETH))
        }
    }

    struct FakeDevice {
        kind: u32,
        interface: &'static str,
        state: u32,
        active: &'static str,
        available: Vec<&'static str>,
        log: Log,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
    impl FakeDevice {
        #[zbus(property)]
        fn device_type(&self) -> u32 {
            self.kind
        }
        #[zbus(property)]
        fn interface(&self) -> String {
            self.interface.to_owned()
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            self.state
        }
        #[zbus(property)]
        fn active_connection(&self) -> OwnedObjectPath {
            path(self.active)
        }
        #[zbus(property)]
        fn available_connections(&self) -> Vec<OwnedObjectPath> {
            self.available.iter().map(|c| path(c)).collect()
        }
        fn disconnect(&mut self) {
            self.log
                .lock()
                .unwrap()
                .push(format!("Disconnect({})", self.interface));
            self.state = 30;
            self.active = NONE;
        }
    }

    struct FakeWired {
        carrier: bool,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wired")]
    impl FakeWired {
        #[zbus(property)]
        fn carrier(&self) -> bool {
            self.carrier
        }
    }

    struct FakeWireless {
        access_points: Vec<&'static str>,
        active: &'static str,
        log: Log,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
    impl FakeWireless {
        #[zbus(property)]
        fn access_points(&self) -> Vec<OwnedObjectPath> {
            self.access_points.iter().map(|ap| path(ap)).collect()
        }
        #[zbus(property)]
        fn active_access_point(&self) -> OwnedObjectPath {
            path(self.active)
        }
        #[zbus(property)]
        fn last_scan(&self) -> i64 {
            0
        }
        fn request_scan(&self, options: HashMap<String, OwnedValue>) {
            assert!(options.is_empty());
            self.log.lock().unwrap().push("RequestScan".to_owned());
        }
    }

    struct FakeAccessPoint {
        ssid: &'static str,
        strength: u8,
        flags: u32,
        rsn_flags: u32,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
    impl FakeAccessPoint {
        #[zbus(property)]
        fn ssid(&self) -> Vec<u8> {
            self.ssid.as_bytes().to_vec()
        }
        #[zbus(property)]
        fn strength(&self) -> u8 {
            self.strength
        }
        #[zbus(property)]
        fn flags(&self) -> u32 {
            self.flags
        }
        #[zbus(property)]
        fn wpa_flags(&self) -> u32 {
            0
        }
        #[zbus(property)]
        fn rsn_flags(&self) -> u32 {
            self.rsn_flags
        }
        #[zbus(property)]
        fn last_seen(&self) -> i32 {
            0
        }
    }

    struct FakeActive;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
    impl FakeActive {
        #[zbus(property)]
        fn id(&self) -> String {
            "Wired connection 1".to_owned()
        }
    }

    struct FakeSettings;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
    impl FakeSettings {
        fn list_connections(&self) -> Vec<OwnedObjectPath> {
            [WIRED_CONN, HOME_CONN, VPN_CONN]
                .iter()
                .map(|c| path(c))
                .collect()
        }
    }

    struct FakeConnection {
        kind: &'static str,
        ssid: Option<&'static str>,
    }

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
    impl FakeConnection {
        fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
            let mut settings = HashMap::new();
            settings.insert(
                "connection".to_owned(),
                HashMap::from([(
                    "type".to_owned(),
                    Value::from(self.kind).try_into().unwrap(),
                )]),
            );
            if let Some(ssid) = self.ssid {
                settings.insert(
                    "802-11-wireless".to_owned(),
                    HashMap::from([(
                        "ssid".to_owned(),
                        Value::from(ssid.as_bytes().to_vec()).try_into().unwrap(),
                    )]),
                );
            }
            settings
        }
    }

    fn ap(ssid: &'static str, strength: u8, secured: bool) -> FakeAccessPoint {
        FakeAccessPoint {
            ssid,
            strength,
            flags: u32::from(secured),
            rsn_flags: if secured { 0x188 } else { 0 },
        }
    }

    struct Mock {
        server: Connection,
        network: Network,
        log: Log,
    }

    /// A NetworkManager with a wired device on "Wired connection 1", a Wi-Fi device on
    /// "Home" (two access points of it, the active one weaker), "Cafe" (open, in range),
    /// "Library" (protected, never joined), a hidden network, and a loopback it doesn't
    /// manage. Saved: the wired connection, "Home" and a VPN.
    fn setup() -> Mock {
        let (server, client) = crate::testing::pair().unwrap();
        let log = Log::default();
        let objects = server.object_server().clone();
        objects
            .at(
                ROOT,
                FakeManager {
                    wifi_enabled: true,
                    devices: vec![ETH, WLAN, LO],
                    log: log.clone(),
                },
            )
            .unwrap();
        let device = |kind, interface, state, active, available: Vec<&'static str>| FakeDevice {
            kind,
            interface,
            state,
            active,
            available,
            log: log.clone(),
        };
        objects
            .at(ETH, device(1, "enp0s1", 100, ACTIVE_ETH, vec![WIRED_CONN]))
            .unwrap();
        objects.at(ETH, FakeWired { carrier: true }).unwrap();
        objects.at(ACTIVE_ETH, FakeActive).unwrap();
        objects
            .at(WLAN, device(2, "wlan0", 100, NONE, vec![HOME_CONN]))
            .unwrap();
        objects
            .at(
                WLAN,
                FakeWireless {
                    access_points: vec![HOME_AP_WEAK, CAFE_AP, HOME_AP, LIBRARY_AP, HIDDEN_AP],
                    active: HOME_AP_WEAK,
                    log: log.clone(),
                },
            )
            .unwrap();
        objects.at(HOME_AP, ap("Home", 80, true)).unwrap();
        objects.at(HOME_AP_WEAK, ap("Home", 40, true)).unwrap();
        objects.at(CAFE_AP, ap("Cafe", 60, false)).unwrap();
        objects.at(LIBRARY_AP, ap("library", 90, true)).unwrap();
        objects.at(HIDDEN_AP, ap("", 70, true)).unwrap();
        objects
            .at(LO, device(14, "lo", STATE_UNMANAGED, NONE, vec![]))
            .unwrap();
        objects.at(SETTINGS_PATH, FakeSettings).unwrap();
        objects
            .at(
                WIRED_CONN,
                FakeConnection {
                    kind: "802-3-ethernet",
                    ssid: None,
                },
            )
            .unwrap();
        objects
            .at(
                HOME_CONN,
                FakeConnection {
                    kind: "802-11-wireless",
                    ssid: Some("Home"),
                },
            )
            .unwrap();
        objects
            .at(
                VPN_CONN,
                FakeConnection {
                    kind: "vpn",
                    ssid: None,
                },
            )
            .unwrap();
        Mock {
            server,
            network: Network::new(client),
            log,
        }
    }

    impl Mock {
        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    #[test]
    fn reads_wired_and_wifi() {
        let mock = setup();
        let state = mock.network.state().unwrap();
        assert!(state.wifi_enabled);
        assert_eq!(
            state.wired,
            [Wired {
                device: ETH.to_owned(),
                name: "Wired connection 1".to_owned(),
                carrier: true,
                connected: true,
                connecting: false,
            }]
        );
        let wifi = state.wifi.unwrap();
        assert_eq!(wifi.device, WLAN);
        assert!(wifi.connected);
        assert!(!wifi.connecting);
        // The active network first even though it's weaker, then by signal; the two
        // access points of Home are one entry (the active one); the hidden one is left out.
        let summary: Vec<_> = wifi
            .access_points
            .iter()
            .map(|ap| {
                (
                    ap.ssid.as_str(),
                    ap.strength,
                    ap.secured,
                    ap.saved,
                    ap.active,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("Home", 40, true, true, true),
                ("library", 90, true, false, false),
                ("Cafe", 60, false, false, false),
            ]
        );
        assert_eq!(wifi.access_points[0].path, HOME_AP_WEAK);
        let joinable: Vec<_> = wifi
            .access_points
            .iter()
            .map(AccessPoint::joinable)
            .collect();
        assert_eq!(joinable, [true, false, true]);
    }

    #[test]
    fn no_devices_is_empty_and_no_manager_is_an_error() {
        let (server, client) = crate::testing::pair().unwrap();
        server
            .object_server()
            .at(
                ROOT,
                FakeManager {
                    wifi_enabled: false,
                    devices: vec![],
                    log: Log::default(),
                },
            )
            .unwrap();
        let state = Network::new(client).state().unwrap();
        assert_eq!(
            state,
            State {
                wifi_enabled: false,
                wired: vec![],
                wifi: None
            }
        );
        let (_server, client) = crate::testing::pair().unwrap();
        assert!(Network::new(client).state().is_err());
    }

    #[test]
    fn wired_without_a_cable_or_a_connection() {
        let mock = setup();
        let objects = mock.server.object_server();
        let eth = objects.interface::<_, FakeDevice>(ETH).unwrap();
        {
            let mut eth = eth.get_mut();
            eth.state = 20;
            eth.active = NONE;
        }
        objects
            .interface::<_, FakeWired>(ETH)
            .unwrap()
            .get_mut()
            .carrier = false;
        let wired = mock.network.state().unwrap().wired;
        assert_eq!(wired[0].name, "enp0s1");
        assert!(!wired[0].carrier && !wired[0].connected && !wired[0].connecting);
        eth.get_mut().state = 50;
        assert!(mock.network.state().unwrap().wired[0].connecting);
    }

    #[test]
    fn wifi_switch() {
        let mock = setup();
        mock.network.set_wifi_enabled(false).unwrap();
        assert!(!mock.network.state().unwrap().wifi_enabled);
        mock.network.set_wifi_enabled(true).unwrap();
        assert!(mock.network.state().unwrap().wifi_enabled);
    }

    #[test]
    fn scan_is_requested() {
        let mock = setup();
        mock.network.request_scan(WLAN).unwrap();
        assert_eq!(mock.log(), ["RequestScan"]);
    }

    #[test]
    fn joins_a_saved_network() {
        let mock = setup();
        mock.network.join(WLAN, HOME_AP).unwrap();
        assert_eq!(
            mock.log(),
            [format!("Activate({HOME_CONN}, {WLAN}, {HOME_AP})")]
        );
    }

    #[test]
    fn joins_an_open_network_with_a_new_connection() {
        let mock = setup();
        mock.network.join(WLAN, CAFE_AP).unwrap();
        assert_eq!(
            mock.log(),
            [format!(
                "AddAndActivate(802-11-wireless, Cafe, Cafe, {WLAN}, {CAFE_AP})"
            )]
        );
    }

    #[test]
    fn a_protected_unsaved_network_needs_a_secret() {
        let mock = setup();
        let err = mock.network.join(WLAN, LIBRARY_AP).unwrap_err();
        assert!(matches!(&err, Error::SecretRequired(name) if name == "library"));
        assert!(mock.log().is_empty());
        assert!(mock.network.join(WLAN, "/gone").is_err());
        assert!(matches!(
            mock.network.join("no/path", CAFE_AP),
            Err(Error::Path(_))
        ));
    }

    #[test]
    fn wired_connects_and_disconnects() {
        let mock = setup();
        mock.network.disconnect(ETH).unwrap();
        let wired = mock.network.state().unwrap().wired;
        assert!(!wired[0].connected);
        mock.network.connect(ETH).unwrap();
        assert_eq!(
            mock.log(),
            [
                "Disconnect(enp0s1)".to_owned(),
                format!("Activate({WIRED_CONN}, {ETH}, /)")
            ]
        );
    }

    #[test]
    fn wired_without_a_saved_connection_cannot_connect() {
        let mock = setup();
        mock.server
            .object_server()
            .interface::<_, FakeDevice>(ETH)
            .unwrap()
            .get_mut()
            .available
            .clear();
        let err = mock.network.connect(ETH).unwrap_err();
        assert!(matches!(&err, Error::NoConnection(name) if name == "enp0s1"));
    }

    /// Changes, forwarded from a thread so the test can wait on them.
    fn watch(network: &Network) -> mpsc::Receiver<()> {
        let changes = network.changes().unwrap();
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
        let mock = setup();
        let changes = watch(&mock.network);
        let iface = mock
            .server
            .object_server()
            .interface::<_, FakeDevice>(ETH)
            .unwrap();
        iface.get_mut().state = 20;
        zbus::block_on(iface.get().state_changed(iface.signal_emitter())).unwrap();
        changes.recv_timeout(WAIT).unwrap();
        assert!(!mock.network.state().unwrap().wired[0].connected);
    }

    #[test]
    fn noise_is_not_signalled() {
        let mock = setup();
        let changes = watch(&mock.network);
        let iface = mock
            .server
            .object_server()
            .interface::<_, FakeWireless>(WLAN)
            .unwrap();
        zbus::block_on(iface.get().last_scan_changed(iface.signal_emitter())).unwrap();
        let ap = mock
            .server
            .object_server()
            .interface::<_, FakeAccessPoint>(CAFE_AP)
            .unwrap();
        zbus::block_on(ap.get().last_seen_changed(ap.signal_emitter())).unwrap();
        // A real change after them: the first signal to arrive is that one.
        zbus::block_on(ap.get().strength_changed(ap.signal_emitter())).unwrap();
        changes.recv_timeout(WAIT).unwrap();
        assert!(changes.recv_timeout(Duration::from_millis(300)).is_err());
    }

    #[test]
    fn signals_for_added_and_removed_objects() {
        let mock = setup();
        let changes = watch(&mock.network);
        // An access point appearing, on the manager's own object here: the mock sends the
        // signal NetworkManager sends from the Wi-Fi device.
        let emitter = SignalEmitter::new(mock.server.inner(), WLAN).unwrap();
        let signal = |name: &'static str, object: &'static str| {
            zbus::block_on(emitter.emit(WIRELESS, name, &(path(object),))).unwrap();
        };
        signal(
            "AccessPointAdded",
            "/org/freedesktop/NetworkManager/AccessPoint/9",
        );
        changes.recv_timeout(WAIT).unwrap();
        signal(
            "AccessPointRemoved",
            "/org/freedesktop/NetworkManager/AccessPoint/9",
        );
        changes.recv_timeout(WAIT).unwrap();
        let settings = SignalEmitter::new(mock.server.inner(), SETTINGS_PATH).unwrap();
        zbus::block_on(settings.emit(SETTINGS, "NewConnection", &(path(VPN_CONN),))).unwrap();
        changes.recv_timeout(WAIT).unwrap();
    }

    #[test]
    fn a_vanished_access_point_is_skipped() {
        let mock = setup();
        mock.server
            .object_server()
            .remove::<FakeAccessPoint, _>(CAFE_AP)
            .unwrap();
        let wifi = mock.network.state().unwrap().wifi.unwrap();
        assert!(wifi.access_points.iter().all(|ap| ap.ssid != "Cafe"));
        assert_eq!(wifi.access_points.len(), 2);
    }
}
