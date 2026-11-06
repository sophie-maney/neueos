//! Stands in for NetworkManager on the **system** bus, to check the shell's network menu
//! extra in the VM (task 3.12c). The system bus only lets root own the name, so run it with
//! `sudo` and a policy file for it (`docs/dev-setup.md` §1.6c); it prints each call it gets
//! and runs until killed.
//!
//! ```sh
//! cargo build -p neue-dbus --example mock_network_manager
//! sudo target/debug/examples/mock_network_manager
//! ```
//!
//! It serves a wired device on "Wired connection 1" (connected; `Disconnect` and
//! `ActivateConnection` toggle it) and a Wi-Fi device with "Home" (saved, connected),
//! "Cafe" (open) and "Library" (protected, not saved). Joining one moves the Wi-Fi device
//! to it; the wireless switch is a property, as in the real thing. Stopping it makes the
//! extra disappear.

use anyhow::Result;
use neue_dbus::zbus;
use zbus::blocking::connection::Builder;
use zbus::object_server::{InterfaceRef, SignalEmitter};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue};
use zbus::{ObjectServer, fdo};

const ROOT: &str = "/org/freedesktop/NetworkManager";
const ETH: &str = "/org/freedesktop/NetworkManager/Devices/1";
const WLAN: &str = "/org/freedesktop/NetworkManager/Devices/2";
const ACTIVE_ETH: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
const WIRED_CONN: &str = "/org/freedesktop/NetworkManager/Settings/1";
const HOME_CONN: &str = "/org/freedesktop/NetworkManager/Settings/2";
const HOME_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
const CAFE_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/2";
const LIBRARY_AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/3";
const NONE: &str = "/";

fn path(path: &'static str) -> OwnedObjectPath {
    ObjectPath::from_static_str_unchecked(path).into()
}

/// The access point constant that `path` names.
fn access_point(path: &str) -> fdo::Result<&'static str> {
    [HOME_AP, CAFE_AP, LIBRARY_AP]
        .into_iter()
        .find(|known| *known == path)
        .ok_or_else(|| fdo::Error::InvalidArgs(format!("no access point {path}")))
}

struct Manager {
    wifi_enabled: bool,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        vec![path(ETH), path(WLAN)]
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.wifi_enabled
    }
    #[zbus(property)]
    fn set_wireless_enabled(&mut self, enabled: bool) {
        println!("WirelessEnabled = {enabled}");
        self.wifi_enabled = enabled;
    }

    async fn activate_connection(
        &self,
        connection: OwnedObjectPath,
        device: OwnedObjectPath,
        specific_object: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<OwnedObjectPath> {
        println!("ActivateConnection {connection} {device} {specific_object}");
        activate(server, device.as_str(), specific_object.as_str()).await?;
        Ok(path(ACTIVE_ETH))
    }

    async fn add_and_activate_connection(
        &self,
        connection: std::collections::HashMap<
            String,
            std::collections::HashMap<String, OwnedValue>,
        >,
        device: OwnedObjectPath,
        specific_object: OwnedObjectPath,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<(OwnedObjectPath, OwnedObjectPath)> {
        println!(
            "AddAndActivateConnection {:?} {device} {specific_object}",
            connection.keys().collect::<Vec<_>>()
        );
        activate(server, device.as_str(), specific_object.as_str()).await?;
        Ok((path(HOME_CONN), path(ACTIVE_ETH)))
    }
}

/// Connects `device`, to `access_point` when it is the Wi-Fi one.
async fn activate(server: &ObjectServer, device: &str, specific: &str) -> fdo::Result<()> {
    set_state(server, device, 100).await?;
    if device == WLAN {
        let wireless: InterfaceRef<Wireless> = server.interface(WLAN).await?;
        wireless.get_mut().await.active = access_point(specific)?;
        wireless
            .get()
            .await
            .active_access_point_changed(wireless.signal_emitter())
            .await?;
    }
    Ok(())
}

async fn set_state(server: &ObjectServer, device: &str, state: u32) -> fdo::Result<()> {
    let device: InterfaceRef<Device> = server.interface(device).await?;
    device.get_mut().await.state = state;
    let emitter = device.signal_emitter();
    device.get().await.state_changed(emitter).await?;
    device
        .get()
        .await
        .active_connection_changed(emitter)
        .await?;
    Ok(())
}

struct Device {
    kind: u32,
    interface: &'static str,
    state: u32,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
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
        // Only the wired device has an active connection object in this mock.
        if self.kind == 1 && self.state == 100 {
            path(ACTIVE_ETH)
        } else {
            path(NONE)
        }
    }
    #[zbus(property)]
    fn available_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path(if self.kind == 1 {
            WIRED_CONN
        } else {
            HOME_CONN
        })]
    }

    async fn disconnect(
        &mut self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<()> {
        println!("Disconnect {}", self.interface);
        // Not `set_state`: this call holds the device's lock.
        self.state = 30;
        self.state_changed(&emitter).await?;
        self.active_connection_changed(&emitter).await?;
        if self.kind == 2 {
            let wireless: InterfaceRef<Wireless> = server.interface(WLAN).await?;
            wireless.get_mut().await.active = NONE;
            wireless
                .get()
                .await
                .active_access_point_changed(wireless.signal_emitter())
                .await?;
        }
        Ok(())
    }
}

struct Wired;

#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wired")]
impl Wired {
    #[zbus(property)]
    fn carrier(&self) -> bool {
        true
    }
}

struct Wireless {
    active: &'static str,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn access_points(&self) -> Vec<OwnedObjectPath> {
        vec![path(HOME_AP), path(CAFE_AP), path(LIBRARY_AP)]
    }
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        path(self.active)
    }
    fn request_scan(&self, _options: std::collections::HashMap<String, OwnedValue>) {
        println!("RequestScan");
    }
}

struct AccessPoint {
    ssid: &'static str,
    strength: u8,
    secured: bool,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl AccessPoint {
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
        u32::from(self.secured)
    }
    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        if self.secured { 0x188 } else { 0 }
    }
}

struct ActiveConnection;

#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> String {
        "Wired connection 1".to_owned()
    }
}

struct Settings;

#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Settings {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        vec![path(WIRED_CONN), path(HOME_CONN)]
    }
}

struct Connection {
    kind: &'static str,
    ssid: Option<&'static str>,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Connection {
    fn get_settings(
        &self,
    ) -> fdo::Result<std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>>>
    {
        use zbus::zvariant::Value;
        let mut settings = std::collections::HashMap::new();
        let owned = |value: Value<'_>| -> fdo::Result<OwnedValue> {
            value
                .try_into()
                .map_err(|err: zbus::zvariant::Error| fdo::Error::Failed(err.to_string()))
        };
        settings.insert(
            "connection".to_owned(),
            [("type".to_owned(), owned(Value::from(self.kind))?)].into(),
        );
        if let Some(ssid) = self.ssid {
            settings.insert(
                "802-11-wireless".to_owned(),
                [(
                    "ssid".to_owned(),
                    owned(Value::from(ssid.as_bytes().to_vec()))?,
                )]
                .into(),
            );
        }
        Ok(settings)
    }
}

fn main() -> Result<()> {
    let conn = Builder::system()?
        .name("org.freedesktop.NetworkManager")?
        .serve_at(ROOT, Manager { wifi_enabled: true })?
        .serve_at(
            ETH,
            Device {
                kind: 1,
                interface: "enp0s1",
                state: 100,
            },
        )?
        .serve_at(ETH, Wired)?
        .serve_at(ACTIVE_ETH, ActiveConnection)?
        .serve_at(
            WLAN,
            Device {
                kind: 2,
                interface: "wlan0",
                state: 100,
            },
        )?
        .serve_at(WLAN, Wireless { active: HOME_AP })?
        .serve_at(
            HOME_AP,
            AccessPoint {
                ssid: "Home",
                strength: 78,
                secured: true,
            },
        )?
        .serve_at(
            CAFE_AP,
            AccessPoint {
                ssid: "Cafe",
                strength: 55,
                secured: false,
            },
        )?
        .serve_at(
            LIBRARY_AP,
            AccessPoint {
                ssid: "Library",
                strength: 90,
                secured: true,
            },
        )?
        .serve_at("/org/freedesktop/NetworkManager/Settings", Settings)?
        .serve_at(
            WIRED_CONN,
            Connection {
                kind: "802-3-ethernet",
                ssid: None,
            },
        )?
        .serve_at(
            HOME_CONN,
            Connection {
                kind: "802-11-wireless",
                ssid: Some("Home"),
            },
        )?
        .build()?;
    println!(
        "mock NetworkManager on the system bus as {:?}",
        conn.unique_name()
    );
    loop {
        std::thread::park();
    }
}
