//! Stands in for UPower and BlueZ on the **system** bus, to check the shell's battery and
//! Bluetooth menu extras in the VM (task 3.11c). The system bus only lets root own these
//! names, so run it with `sudo` and a policy file for it (`docs/dev-setup.md` §1.6b); it prints each call it gets and runs until killed.
//!
//! ```sh
//! cargo build -p neue-dbus --example mock_power_bluetooth
//! sudo target/debug/examples/mock_power_bluetooth [--battery PERCENT] [--charging]
//! ```
//!
//! It serves a battery at 64 % (discharging, 3 h 12 min left) and an adapter (powered)
//! with the paired devices "Earbuds" (connected), "Keyboard" and "Broken" (its `Connect`
//! fails). Stopping it makes both extras disappear.

use anyhow::Result;
use neue_dbus::zbus;
use zbus::blocking::connection::Builder;
use zbus::fdo::ObjectManager;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

const HCI0: &str = "/org/bluez/hci0";

struct Battery {
    percentage: f64,
    charging: bool,
}

#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Battery {
    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.percentage
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        if self.charging { 1 } else { 2 }
    }
    #[zbus(property)]
    fn time_to_empty(&self) -> i64 {
        if self.charging { 0 } else { 11_520 }
    }
    #[zbus(property)]
    fn time_to_full(&self) -> i64 {
        if self.charging { 2_700 } else { 0 }
    }
}

struct Adapter {
    powered: bool,
}

#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn alias(&self) -> String {
        "neue-mock".to_owned()
    }
    #[zbus(property)]
    fn powered(&self) -> bool {
        self.powered
    }
    #[zbus(property)]
    fn set_powered(&mut self, powered: bool) {
        println!("Powered = {powered}");
        self.powered = powered;
    }
}

struct Device {
    alias: &'static str,
    address: &'static str,
    connected: bool,
}

#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        ObjectPath::from_static_str_unchecked(HCI0).into()
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
        "input-keyboard".to_owned()
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.connected
    }

    async fn connect(
        &mut self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        println!("Connect {}", self.alias);
        if self.alias == "Broken" {
            return Err(zbus::fdo::Error::Failed("page timeout".to_owned()));
        }
        self.connected = true;
        self.connected_changed(&emitter).await?;
        Ok(())
    }

    async fn disconnect(
        &mut self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        println!("Disconnect {}", self.alias);
        self.connected = false;
        self.connected_changed(&emitter).await?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut percentage = 64.0;
    let mut charging = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--battery" => percentage = args.next().unwrap_or_default().parse()?,
            "--charging" => charging = true,
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    let conn = Builder::system()?
        .name("org.freedesktop.UPower")?
        .name("org.bluez")?
        .serve_at(
            "/org/freedesktop/UPower/devices/DisplayDevice",
            Battery {
                percentage,
                charging,
            },
        )?
        .serve_at("/", ObjectManager)?
        .serve_at(HCI0, Adapter { powered: true })?
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01",
            Device {
                alias: "Earbuds",
                address: "AA:BB:CC:DD:EE:01",
                connected: true,
            },
        )?
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_02",
            Device {
                alias: "Keyboard",
                address: "AA:BB:CC:DD:EE:02",
                connected: false,
            },
        )?
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_03",
            Device {
                alias: "Broken",
                address: "AA:BB:CC:DD:EE:03",
                connected: false,
            },
        )?
        .build()?;
    println!(
        "mock UPower and BlueZ on the system bus as {:?}",
        conn.unique_name()
    );
    loop {
        std::thread::park();
    }
}
