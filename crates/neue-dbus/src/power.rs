//! UPower's battery (`org.freedesktop.UPower`) for the battery menu extra (task 3.11a).
//!
//! Only the *display device* is used: UPower's own summary of every battery (and the line
//! power) as one, which is what a menu bar wants. [`Power::battery`] reads it; [`PowerChanges`]
//! says when to read it again. Zbus proxies, not `upower-dbus` or similar crates (D-Bus
//! stays on zbus, ADR 0005).

use std::collections::HashMap;
use std::time::Duration;

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MessageType;
use zbus::zvariant::OwnedValue;

use crate::Error;

/// UPower's bus name (on the system bus).
pub const DESTINATION: &str = "org.freedesktop.UPower";
/// The display device: one summary of all batteries.
pub const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
/// The device interface.
pub const DEVICE: &str = "org.freedesktop.UPower.Device";

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// What the battery is doing (UPower's `State`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    /// Flat.
    Empty,
    FullyCharged,
    /// Plugged in but not charging yet (a charge threshold, say).
    PendingCharge,
    /// On battery but not discharging yet.
    PendingDischarge,
    Unknown,
}

impl BatteryState {
    fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }

    /// Mains power is connected: charging, charged, or waiting to charge.
    pub fn on_ac(self) -> bool {
        matches!(
            self,
            Self::Charging | Self::FullyCharged | Self::PendingCharge
        )
    }
}

/// The display device as of one read. There is no value for "no battery": [`Power::battery`]
/// returns `None` then.
#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    /// Charge, 0 to 100.
    pub percentage: f64,
    pub state: BatteryState,
    /// Time until empty, while discharging and once UPower has an estimate.
    pub time_to_empty: Option<Duration>,
    /// Time until full, while charging and once UPower has an estimate.
    pub time_to_full: Option<Duration>,
}

impl Battery {
    /// The time that matters now: until empty when discharging, until full when charging.
    pub fn time_left(&self) -> Option<Duration> {
        match self.state {
            BatteryState::Discharging => self.time_to_empty,
            BatteryState::Charging => self.time_to_full,
            _ => None,
        }
    }

    /// Reads the display device's properties; `None` when no battery is present.
    fn from_properties(props: &HashMap<String, OwnedValue>) -> Option<Self> {
        if !get::<bool>(props, "IsPresent")? {
            return None;
        }
        let seconds = |name| {
            get::<i64>(props, name)
                .and_then(|s| u64::try_from(s).ok())
                // UPower says 0 when it has no estimate.
                .filter(|&s| s > 0)
                .map(Duration::from_secs)
        };
        Some(Self {
            percentage: get::<f64>(props, "Percentage")?.clamp(0.0, 100.0),
            state: BatteryState::from_code(get::<u32>(props, "State").unwrap_or(0)),
            time_to_empty: seconds("TimeToEmpty"),
            time_to_full: seconds("TimeToFull"),
        })
    }
}

fn get<T>(props: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    props.get(name)?.try_clone().ok()?.try_into().ok()
}

/// A UPower client.
#[derive(Debug, Clone)]
pub struct Power {
    conn: Connection,
}

impl Power {
    /// A client on `conn`: the system bus, or a peer-to-peer connection in tests.
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Connects to the system bus.
    pub fn system() -> Result<Self, Error> {
        Ok(Self::new(Connection::system()?))
    }

    /// The battery now; `None` when the machine has none. An error when UPower isn't there.
    pub fn battery(&self) -> Result<Option<Battery>, Error> {
        let reply = self.conn.call_method(
            Some(DESTINATION),
            DISPLAY_DEVICE_PATH,
            Some(PROPERTIES),
            "GetAll",
            &(DEVICE,),
        )?;
        let props: HashMap<String, OwnedValue> = reply.body().deserialize()?;
        Ok(Battery::from_properties(&props))
    }

    /// Subscribes to changes of the display device (adds a match rule on a bus).
    pub fn changes(&self) -> Result<PowerChanges, Error> {
        PowerChanges::new(&self.conn)
    }
}

/// Says that the display device changed, so the caller reads it again with
/// [`Power::battery`]. Blocking: iterate it on a thread of its own. It ends when the
/// connection closes.
pub struct PowerChanges {
    messages: MessageIterator,
}

impl std::fmt::Debug for PowerChanges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PowerChanges").finish_non_exhaustive()
    }
}

impl PowerChanges {
    fn new(conn: &Connection) -> Result<Self, Error> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(PROPERTIES)?
            .member("PropertiesChanged")?
            .path(DISPLAY_DEVICE_PATH)?
            .build();
        // The caller re-reads on any change, so a burst beyond this loses nothing.
        let messages = MessageIterator::for_match_rule(rule, conn, Some(16))?;
        Ok(Self { messages })
    }
}

impl Iterator for PowerChanges {
    type Item = ();

    fn next(&mut self) -> Option<()> {
        loop {
            match self.messages.next()? {
                Ok(_) => return Some(()),
                Err(err) => tracing::debug!(%err, "bad message on the UPower signal stream"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use std::sync::mpsc;
    use std::thread;

    use super::*;

    /// A stand-in for UPower's display device.
    struct Fake {
        present: bool,
        percentage: f64,
        state: u32,
        to_empty: i64,
        to_full: i64,
    }

    #[zbus::interface(name = "org.freedesktop.UPower.Device")]
    impl Fake {
        #[zbus(property)]
        fn is_present(&self) -> bool {
            self.present
        }
        #[zbus(property)]
        fn percentage(&self) -> f64 {
            self.percentage
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            self.state
        }
        #[zbus(property)]
        fn time_to_empty(&self) -> i64 {
            self.to_empty
        }
        #[zbus(property)]
        fn time_to_full(&self) -> i64 {
            self.to_full
        }
    }

    fn fake(present: bool, percentage: f64, state: u32, to_empty: i64, to_full: i64) -> Fake {
        Fake {
            present,
            percentage,
            state,
            to_empty,
            to_full,
        }
    }

    fn client_of(fake: Fake) -> (Connection, Power) {
        let (server, client) = crate::testing::pair().unwrap();
        server
            .object_server()
            .at(DISPLAY_DEVICE_PATH, fake)
            .unwrap();
        (server, Power::new(client))
    }

    #[test]
    fn no_battery_is_none() {
        let (_server, power) = client_of(fake(false, 0.0, 0, 0, 0));
        assert_eq!(power.battery().unwrap(), None);
    }

    #[test]
    fn discharging_reports_time_until_empty() {
        let (_server, power) = client_of(fake(true, 81.5, 2, 5_400, 0));
        let battery = power.battery().unwrap().unwrap();
        assert_eq!(battery.percentage, 81.5);
        assert_eq!(battery.state, BatteryState::Discharging);
        assert!(!battery.state.on_ac());
        assert_eq!(battery.time_left(), Some(Duration::from_secs(5_400)));
    }

    #[test]
    fn charging_reports_time_until_full() {
        // The stale time-to-empty of an earlier discharge isn't the time left.
        let (_server, power) = client_of(fake(true, 40.0, 1, 7_200, 1_800));
        let battery = power.battery().unwrap().unwrap();
        assert_eq!(battery.state, BatteryState::Charging);
        assert!(battery.state.on_ac());
        assert_eq!(battery.time_left(), Some(Duration::from_secs(1_800)));
    }

    #[test]
    fn no_estimate_and_full_have_no_time_left() {
        let (_server, power) = client_of(fake(true, 70.0, 2, 0, 0));
        assert_eq!(power.battery().unwrap().unwrap().time_left(), None);
        let (_server, power) = client_of(fake(true, 100.0, 4, 0, 0));
        let battery = power.battery().unwrap().unwrap();
        assert_eq!(battery.state, BatteryState::FullyCharged);
        assert_eq!(battery.time_left(), None);
    }

    #[test]
    fn an_odd_percentage_is_clamped_and_a_new_state_is_unknown() {
        let (_server, power) = client_of(fake(true, 103.0, 99, 0, 0));
        let battery = power.battery().unwrap().unwrap();
        assert_eq!(battery.percentage, 100.0);
        assert_eq!(battery.state, BatteryState::Unknown);
    }

    #[test]
    fn a_missing_upower_is_an_error() {
        let (_server, client) = crate::testing::pair().unwrap();
        assert!(Power::new(client).battery().is_err());
    }

    #[test]
    fn changes_are_signalled_and_read_again() {
        let (server, power) = client_of(fake(true, 50.0, 2, 3_600, 0));
        let changes = power.changes().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for () in changes {
                if tx.send(()).is_err() {
                    return;
                }
            }
        });
        let iface = server
            .object_server()
            .interface::<_, Fake>(DISPLAY_DEVICE_PATH)
            .unwrap();
        iface.get_mut().percentage = 49.0;
        zbus::block_on(iface.get().percentage_changed(iface.signal_emitter())).unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(power.battery().unwrap().unwrap().percentage, 49.0);
    }
}
