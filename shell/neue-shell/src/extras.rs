//! The battery, Bluetooth and network workers (tasks 3.11c and 3.12c, ADR 0083): each follows its service on the
//! system bus on a thread of its own and sends the UI thread a new snapshot whenever it
//! differs from the last one. `None` means "nothing to show" (no battery, no adapter, or
//! the service isn't on the bus), which hides the extra.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use neue_dbus::bluetooth::{Bluetooth, State};
use neue_dbus::network::{Network, State as NetworkState};
use neue_dbus::power::{Battery, Power};
use neue_shell::bar::{
    BatteryView, BluetoothView, DeviceView, NetworkView, WifiNetworkView, WifiView, WiredView,
};

/// How often a service is read again without a signal: signals say nothing about a service
/// that appears on or leaves the bus, or an adapter that is plugged in.
const POLL: Duration = Duration::from_secs(5);

/// How long NetworkManager must be quiet before it is read again: a scan touches every
/// access point, and a connection coming up changes several objects.
const NETWORK_DEBOUNCE: Duration = Duration::from_millis(300);

/// The longest a burst of signals delays a read.
const DEBOUNCE_MAX: Duration = Duration::from_secs(2);

/// The bar's view of UPower's battery.
pub fn battery_view(battery: &Battery) -> BatteryView {
    // The percentage is clamped to 0..=100 by `neue-dbus`; rounding picks the nearest.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a percentage clamped to 0..=100"
    )]
    let percent = battery.percentage.round().clamp(0.0, 100.0) as u8;
    BatteryView {
        percent,
        charging: battery.state == neue_dbus::power::BatteryState::Charging,
        on_ac: battery.state.on_ac(),
        time_left: battery.time_left(),
    }
}

/// The bar's view of BlueZ's adapter and its paired devices, in the same order as
/// `state.devices` (the menu's device indices are looked up there).
pub fn bluetooth_view(state: &State) -> BluetoothView {
    BluetoothView {
        powered: state.adapter.powered,
        devices: state
            .devices
            .iter()
            .map(|device| DeviceView {
                name: device.name.clone(),
                connected: device.connected,
            })
            .collect(),
    }
}

/// The bar's view of NetworkManager's devices, in the same order as `state` (the menu's
/// indices are looked up there).
pub fn network_view(state: &NetworkState) -> NetworkView {
    NetworkView {
        wifi_enabled: state.wifi_enabled,
        wired: state
            .wired
            .iter()
            .map(|wired| WiredView {
                name: wired.name.clone(),
                carrier: wired.carrier,
                connected: wired.connected,
                connecting: wired.connecting,
            })
            .collect(),
        wifi: state.wifi.as_ref().map(|wifi| WifiView {
            connected: wifi.connected,
            connecting: wifi.connecting,
            networks: wifi
                .access_points
                .iter()
                .map(|ap| WifiNetworkView {
                    ssid: ap.ssid.clone(),
                    strength: ap.strength,
                    secured: ap.secured,
                    saved: ap.saved,
                    active: ap.active,
                })
                .collect(),
        }),
    }
}

/// Follows UPower until `send` says the shell is gone.
pub fn run_battery(send: impl Fn(Option<BatteryView>) -> bool) -> Result<(), neue_dbus::Error> {
    let power = Power::system()?;
    let signals = signals("upower-signals", power.changes()?)?;
    follow("UPower", &signals, Duration::ZERO, send, || {
        Ok(power.battery()?.as_ref().map(battery_view))
    });
    Ok(())
}

/// Follows BlueZ until `send` says the shell is gone.
pub fn run_bluetooth(send: impl Fn(Option<State>) -> bool) -> Result<(), neue_dbus::Error> {
    let bluetooth = Bluetooth::system()?;
    let signals = signals("bluez-signals", bluetooth.changes()?)?;
    follow("BlueZ", &signals, Duration::ZERO, send, || {
        bluetooth.state()
    });
    Ok(())
}

/// Follows NetworkManager until `send` says the shell is gone. A machine with neither an
/// Ethernet nor a Wi-Fi device counts as having no network to show.
pub fn run_network(send: impl Fn(Option<NetworkState>) -> bool) -> Result<(), neue_dbus::Error> {
    let network = Network::system()?;
    let signals = signals("nm-signals", network.changes()?)?;
    follow("NetworkManager", &signals, NETWORK_DEBOUNCE, send, || {
        let state = network.state()?;
        Ok((!state.wired.is_empty() || state.wifi.is_some()).then_some(state))
    });
    Ok(())
}

/// Turns a blocking stream of change signals into a channel, on a thread of its own that
/// ends with the stream or the receiver.
fn signals(
    name: &str,
    changes: impl Iterator<Item = ()> + Send + 'static,
) -> Result<mpsc::Receiver<()>, neue_dbus::Error> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            for () in changes {
                if tx.send(()).is_err() {
                    return;
                }
            }
        })
        .map_err(|e| neue_dbus::Error::from(neue_dbus::zbus::Error::from(e)))?;
    Ok(rx)
}

/// Reads with `read` at start, on every signal and every [`POLL`], and sends what changed.
/// A failed read (the service isn't there) counts as `None`; it is logged when it starts.
/// A signal is acted on once nothing more has come for `debounce`. Returns when `send` says
/// the shell is gone, or the signal stream ended.
fn follow<T: Clone + PartialEq>(
    service: &str,
    signals: &mpsc::Receiver<()>,
    debounce: Duration,
    send: impl Fn(Option<T>) -> bool,
    mut read: impl FnMut() -> Result<Option<T>, neue_dbus::Error>,
) {
    let mut last: Option<Option<T>> = None;
    let mut failing = false;
    loop {
        let now = match read() {
            Ok(now) => {
                failing = false;
                now
            }
            Err(err) => {
                if !failing {
                    tracing::debug!(%err, service, "not on the system bus");
                }
                failing = true;
                None
            }
        };
        if last.as_ref() != Some(&now) {
            last = Some(now.clone());
            if !send(now) {
                return;
            }
        }
        match signals.recv_timeout(POLL) {
            // A burst of signals is read once, when it has ended.
            // Signals that never stop don't hold the read back for more than `DEBOUNCE_MAX`.
            Ok(()) => {
                let start = Instant::now();
                while start.elapsed() < DEBOUNCE_MAX {
                    match signals.recv_timeout(debounce) {
                        Ok(()) => {}
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // test code

    use std::cell::RefCell;

    use neue_dbus::bluetooth::{Adapter, Device};
    use neue_dbus::network::{AccessPoint, Wifi, Wired};
    use neue_dbus::power::BatteryState;

    use super::*;

    fn battery(percentage: f64, state: BatteryState) -> Battery {
        Battery {
            percentage,
            state,
            time_to_empty: Some(Duration::from_secs(3_600)),
            time_to_full: None,
        }
    }

    #[test]
    fn battery_view_maps_the_state() {
        let view = battery_view(&battery(81.6, BatteryState::Discharging));
        assert_eq!(view.percent, 82);
        assert!(!view.charging && !view.on_ac);
        assert_eq!(view.time_left, Some(Duration::from_secs(3_600)));
        let view = battery_view(&battery(100.0, BatteryState::FullyCharged));
        assert!(!view.charging && view.on_ac);
        assert!(battery_view(&battery(40.0, BatteryState::Charging)).charging);
    }

    #[test]
    fn bluetooth_view_keeps_the_device_order() {
        let device = |name: &str, connected| Device {
            path: format!("/org/bluez/hci0/{name}"),
            name: name.to_owned(),
            address: String::new(),
            icon: None,
            connected,
        };
        let state = State {
            adapter: Adapter {
                path: "/org/bluez/hci0".to_owned(),
                name: "neue".to_owned(),
                powered: true,
            },
            devices: vec![device("Earbuds", true), device("mouse", false)],
        };
        let view = bluetooth_view(&state);
        assert!(view.powered);
        assert_eq!(view.connected(), 1);
        let names: Vec<_> = view.devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Earbuds", "mouse"]);
    }

    #[test]
    fn network_view_keeps_the_order_and_the_flags() {
        let state = NetworkState {
            wifi_enabled: true,
            wired: vec![Wired {
                device: "/d/1".to_owned(),
                name: "Wired connection 1".to_owned(),
                carrier: true,
                connected: true,
                connecting: false,
            }],
            wifi: Some(Wifi {
                device: "/d/2".to_owned(),
                connected: true,
                connecting: false,
                access_points: vec![
                    access_point("Home", 80, true, true, true),
                    access_point("Library", 40, true, false, false),
                ],
            }),
        };
        let view = network_view(&state);
        assert!(view.wifi_enabled);
        assert_eq!(view.wired[0].name, "Wired connection 1");
        assert!(view.wired[0].connected && view.wired[0].carrier);
        let wifi = view.wifi.unwrap();
        assert!(wifi.connected);
        let ssids: Vec<_> = wifi.networks.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(ssids, ["Home", "Library"]);
        assert!(wifi.networks[0].active && wifi.networks[0].saved);
        assert!(wifi.networks[1].secured && !wifi.networks[1].joinable());
        assert!(
            network_view(&NetworkState {
                wifi: None,
                ..state
            })
            .wifi
            .is_none()
        );
    }

    fn access_point(
        ssid: &str,
        strength: u8,
        secured: bool,
        saved: bool,
        active: bool,
    ) -> AccessPoint {
        AccessPoint {
            path: format!("/ap/{ssid}"),
            ssid: ssid.to_owned(),
            strength,
            secured,
            saved,
            active,
        }
    }

    #[test]
    fn follow_sends_only_what_changed_and_a_failure_is_nothing() {
        let (signal, signals) = mpsc::channel();
        let reads = RefCell::new(vec![
            Ok(Some(1)),
            Ok(Some(1)),
            Err(neue_dbus::Error::from(neue_dbus::zbus::Error::Failure(
                "gone".into(),
            ))),
            Ok(None),
            Ok(Some(2)),
        ]);
        let sent = RefCell::new(Vec::new());
        follow(
            "test",
            &signals,
            Duration::ZERO,
            |value| {
                sent.borrow_mut().push(value);
                value != Some(2)
            },
            || {
                // Each read is followed by a signal, so the next one comes at once.
                signal.send(()).unwrap();
                reads.borrow_mut().remove(0)
            },
        );
        assert_eq!(*sent.borrow(), [Some(1), None, Some(2)]);
    }

    #[test]
    fn follow_stops_when_the_shell_is_gone() {
        let (_signal, signals) = mpsc::channel();
        follow(
            "test",
            &signals,
            Duration::ZERO,
            |_: Option<u8>| false,
            || Ok(Some(1)),
        );
    }
}
