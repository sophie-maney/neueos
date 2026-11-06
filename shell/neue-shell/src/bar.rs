//! The menu bar's pieces (task 3.2b): the logo menu, the active app's name, the app's own
//! menus (the global menu, task 3.7a), and the clock, battery, network and Bluetooth extras
//! (tasks 3.11b and 3.12b), put together in a [`MenuBar`]. Portable: the shell builds one per output, and the
//! gallery's Shell page and the goldens build the same.

use std::time::Duration;

use neue_kit::desktop::{Backdrop, ExtraIcon, MenuBar, MenuTitle, NetworkIcon};
use neue_kit::{Handle, Menu, MenuItem};

/// What the logo menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    /// A panel about this computer.
    About,
    /// The Preferences app (task 3.24; the item is disabled until then).
    Preferences,
    /// Locks the session (logind; the lock screen is task 3.18).
    Lock,
    /// Ends the session (the compositor quits, ADR 0027).
    LogOut,
    Restart,
    ShutDown,
}

/// What assistive technologies call the logo title.
pub const LOGO_NAME: &str = "System";

/// The logo menu, each item sending `message(command)`.
pub fn logo_menu<M: 'static>(message: impl Fn(SystemCommand) -> M + Clone + 'static) -> Menu<M> {
    let item = |label: &str, command| {
        let message = message.clone();
        MenuItem::new(label).on_choose_with(move || message(command))
    };
    Menu::new()
        .with(item("About This Computer", SystemCommand::About))
        .separator()
        // Until the Preferences app exists (task 3.24).
        .with(item("Preferences…", SystemCommand::Preferences).disabled(true))
        .separator()
        .with(item("Lock Screen", SystemCommand::Lock))
        .separator()
        .with(item("Restart", SystemCommand::Restart))
        .with(item("Shut Down", SystemCommand::ShutDown))
        .separator()
        .with(item("Log Out", SystemCommand::LogOut))
}

/// The clock extra's menu: today's full date, for reading only.
pub fn clock_menu<M: 'static>(full_date: &str) -> Menu<M> {
    Menu::new().with(MenuItem::new(full_date).disabled(true))
}

/// The battery as the bar shows it; `neue-dbus`'s `Battery` is mapped to it (task 3.11c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryView {
    /// Charge, 0 to 100.
    pub percent: u8,
    /// Charging right now.
    pub charging: bool,
    /// Mains power is connected (charging, or charged and waiting).
    pub on_ac: bool,
    /// Until empty when discharging, until full when charging, once there is an estimate.
    pub time_left: Option<Duration>,
}

/// A paired Bluetooth device as the bar shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceView {
    pub name: String,
    pub connected: bool,
}

/// The Bluetooth adapter as the bar shows it; `neue-dbus`'s `State` is mapped to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothView {
    pub powered: bool,
    /// Paired devices, in the order the menu lists them.
    pub devices: Vec<DeviceView>,
}

impl BluetoothView {
    /// How many devices are connected.
    pub fn connected(&self) -> usize {
        self.devices.iter().filter(|d| d.connected).count()
    }
}

/// What the Bluetooth menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothCommand {
    /// Turn the adapter on (`true`) or off.
    SetPowered(bool),
    /// Connect the device at this index of [`BluetoothView::devices`].
    Connect(usize),
    /// Disconnect the device at this index.
    Disconnect(usize),
}

/// A wired (Ethernet) device as the bar shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WiredView {
    /// The connection it is on, or its interface name.
    pub name: String,
    /// A cable is plugged in.
    pub carrier: bool,
    pub connected: bool,
    /// Connecting, not there yet.
    pub connecting: bool,
}

/// A Wi-Fi network in range as the bar shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetworkView {
    pub ssid: String,
    /// Signal strength, 0 to 100.
    pub strength: u8,
    /// Needs a password.
    pub secured: bool,
    /// A saved connection exists, so joining needs no password.
    pub saved: bool,
    /// The Wi-Fi device is on this network.
    pub active: bool,
}

impl WifiNetworkView {
    /// Joining can work: the network is open or saved. (Joining a new protected network
    /// needs a secret agent, which doesn't exist yet.)
    pub fn joinable(&self) -> bool {
        self.saved || !self.secured
    }

    /// The signal as arcs of the Wi-Fi fan to light, 1 to 3.
    pub fn bars(&self) -> u8 {
        signal_bars(self.strength)
    }
}

/// The Wi-Fi device as the bar shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiView {
    pub connected: bool,
    /// Connecting, not there yet.
    pub connecting: bool,
    /// In the order the menu lists them (the active one first, then the strongest).
    pub networks: Vec<WifiNetworkView>,
}

/// The network as the bar shows it; `neue-dbus`'s `State` is mapped to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkView {
    /// The Wi-Fi switch.
    pub wifi_enabled: bool,
    pub wired: Vec<WiredView>,
    /// `None` without a Wi-Fi device.
    pub wifi: Option<WifiView>,
}

/// What the network menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkCommand {
    /// Switch Wi-Fi on (`true`) or off.
    SetWifi(bool),
    /// Join the network at this index of [`WifiView::networks`].
    Join(usize),
    /// Leave the network Wi-Fi is on.
    DisconnectWifi,
    /// Connect the wired device at this index of [`NetworkView::wired`].
    ConnectWired(usize),
    /// Disconnect the wired device at this index.
    DisconnectWired(usize),
    /// Look for networks again; sent when the menu opens.
    Scan,
}

/// Arcs of the Wi-Fi fan a signal of `strength` (0 to 100) lights: 1 to 3.
pub fn signal_bars(strength: u8) -> u8 {
    match strength {
        0..=33 => 1,
        34..=66 => 2,
        _ => 3,
    }
}

/// `h:mm` for a time left, rounded down to the minute.
fn hours_minutes(time: Duration) -> String {
    let minutes = time.as_secs() / 60;
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// The battery's icon and what assistive technologies call it.
pub fn battery_icon(view: &BatteryView) -> (ExtraIcon, String) {
    let icon = ExtraIcon::Battery {
        level: f64::from(view.percent) / 100.0,
        charging: view.charging,
    };
    let state = if view.charging { ", charging" } else { "" };
    (icon, format!("Battery, {} percent{state}", view.percent))
}

/// The battery extra's menu, for reading only: the charge and the time left, and the
/// power source.
pub fn battery_menu<M: 'static>(view: &BatteryView) -> Menu<M> {
    let charge = match view.time_left {
        Some(time) if view.charging => {
            format!("{}% · {} until full", view.percent, hours_minutes(time))
        }
        Some(time) => format!("{}% · {} remaining", view.percent, hours_minutes(time)),
        None => format!("{}%", view.percent),
    };
    let source = if view.on_ac {
        "Power Source: Power Adapter"
    } else {
        "Power Source: Battery"
    };
    Menu::new()
        .with(MenuItem::new(charge).disabled(true))
        .separator()
        .with(MenuItem::new(source).disabled(true))
}

/// The network extra's icon and what assistive technologies call it. A wired connection
/// that is up shows before Wi-Fi, as it is the one NetworkManager prefers.
pub fn network_icon(view: &NetworkView) -> (ExtraIcon, String) {
    let (icon, name) = if let Some(wired) = view.wired.iter().find(|w| w.connected) {
        (
            NetworkIcon::Wired,
            format!("Network, wired, {}", wired.name),
        )
    } else if let Some(wifi) = &view.wifi {
        if !view.wifi_enabled {
            (NetworkIcon::WifiOff, "Network, Wi-Fi off".to_owned())
        } else if let Some(net) = wifi.networks.iter().find(|n| n.active) {
            (
                NetworkIcon::Wifi { level: net.bars() },
                format!(
                    "Network, Wi-Fi, {}, signal {} percent",
                    net.ssid, net.strength
                ),
            )
        } else if wifi.connected {
            // Connected to a network the last scan doesn't list.
            (
                NetworkIcon::Wifi { level: 1 },
                "Network, Wi-Fi, connected".to_owned(),
            )
        } else if wifi.connecting {
            (
                NetworkIcon::Wifi { level: 0 },
                "Network, Wi-Fi, connecting".to_owned(),
            )
        } else {
            (
                NetworkIcon::Wifi { level: 0 },
                "Network, Wi-Fi on, not connected".to_owned(),
            )
        }
    } else if view.wired.iter().any(|w| w.connecting) {
        (
            NetworkIcon::Disconnected,
            "Network, wired, connecting".to_owned(),
        )
    } else {
        (
            NetworkIcon::Disconnected,
            "Network, disconnected".to_owned(),
        )
    };
    (ExtraIcon::Network(icon), name)
}

/// The network extra's menu: the Wi-Fi switch and the networks in range (a tick on the one
/// joined, a signal glyph and a padlock for each; a protected network that isn't saved
/// is greyed out), a Disconnect item, then each wired device with a submenu to connect or
/// disconnect it; each item sends `message(command)`.
pub fn network_menu<M: 'static>(
    view: &NetworkView,
    message: impl Fn(NetworkCommand) -> M + Clone + 'static,
) -> Menu<M> {
    let item = |label: &str, command| {
        let message = message.clone();
        MenuItem::new(label).on_choose_with(move || message(command))
    };
    let mut menu = Menu::new();
    if let Some(wifi) = &view.wifi {
        let (state, switch) = if view.wifi_enabled {
            ("Wi-Fi: On", "Turn Wi-Fi Off")
        } else {
            ("Wi-Fi: Off", "Turn Wi-Fi On")
        };
        menu = menu
            .with(MenuItem::new(state).disabled(true))
            .with(item(switch, NetworkCommand::SetWifi(!view.wifi_enabled)));
        if view.wifi_enabled {
            menu = menu.separator();
            if wifi.networks.is_empty() {
                menu = menu.with(MenuItem::new("No Networks").disabled(true));
            }
            for (i, net) in wifi.networks.iter().enumerate() {
                menu = menu.with(
                    item(&net.ssid, NetworkCommand::Join(i))
                        .checked(net.active)
                        .signal(net.bars(), net.secured)
                        .disabled(!net.joinable()),
                );
            }
            if wifi.connected {
                menu = menu
                    .separator()
                    .with(item("Disconnect Wi-Fi", NetworkCommand::DisconnectWifi));
            }
        }
    }
    if !view.wired.is_empty() {
        if view.wifi.is_some() {
            menu = menu.separator();
        }
        for (i, wired) in view.wired.iter().enumerate() {
            let action = if wired.connected {
                item("Disconnect", NetworkCommand::DisconnectWired(i))
            } else if wired.carrier {
                item("Connect", NetworkCommand::ConnectWired(i))
            } else {
                MenuItem::new("Cable Unplugged").disabled(true)
            };
            menu = menu.with(
                MenuItem::submenu(
                    format!("Ethernet: {}", wired.name),
                    Menu::new().with(action),
                )
                .checked(wired.connected),
            );
        }
    }
    if view.wifi.is_none() && view.wired.is_empty() {
        menu = menu.with(MenuItem::new("No Network Devices").disabled(true));
    }
    menu
}

/// The Bluetooth extra's icon and what assistive technologies call it.
pub fn bluetooth_icon(view: &BluetoothView) -> (ExtraIcon, String) {
    let connected = view.connected();
    let icon = ExtraIcon::Bluetooth {
        powered: view.powered,
        connected: connected > 0,
    };
    let name = match (view.powered, connected) {
        (false, _) => "Bluetooth, off".to_owned(),
        (true, 0) => "Bluetooth, on".to_owned(),
        (true, 1) => "Bluetooth, on, 1 device connected".to_owned(),
        (true, n) => format!("Bluetooth, on, {n} devices connected"),
    };
    (icon, name)
}

/// The Bluetooth extra's menu: the adapter's state and a switch for it, then (while it is
/// on) each paired device with a submenu to connect or disconnect it, each item sending
/// `message(command)`.
pub fn bluetooth_menu<M: 'static>(
    view: &BluetoothView,
    message: impl Fn(BluetoothCommand) -> M + Clone + 'static,
) -> Menu<M> {
    let item = |label: &str, command| {
        let message = message.clone();
        MenuItem::new(label).on_choose_with(move || message(command))
    };
    let (state, switch) = if view.powered {
        ("Bluetooth: On", "Turn Bluetooth Off")
    } else {
        ("Bluetooth: Off", "Turn Bluetooth On")
    };
    let mut menu = Menu::new()
        .with(MenuItem::new(state).disabled(true))
        .with(item(switch, BluetoothCommand::SetPowered(!view.powered)));
    if !view.powered {
        return menu;
    }
    menu = menu.separator();
    if view.devices.is_empty() {
        return menu.with(MenuItem::new("No Devices").disabled(true));
    }
    for (i, device) in view.devices.iter().enumerate() {
        let action = if device.connected {
            item("Disconnect", BluetoothCommand::Disconnect(i))
        } else {
            item("Connect", BluetoothCommand::Connect(i))
        };
        // A connected device shows a tick, as in the Wi-Fi list.
        menu = menu.with(
            MenuItem::submenu(device.name.clone(), Menu::new().with(action))
                .checked(device.connected),
        );
    }
    menu
}

/// Handles to the parts of one bar that change.
#[derive(Debug)]
pub struct BarHandles<M> {
    /// The bar, to replace the app's menus.
    pub bar: Handle<MenuBar<M>>,
    /// The active app's name.
    pub app: Handle<MenuTitle<M>>,
    /// The clock extra.
    pub clock: Handle<MenuTitle<M>>,
    /// The battery extra (hidden without a battery).
    pub battery: Handle<MenuTitle<M>>,
    /// The network extra (hidden without NetworkManager).
    pub network: Handle<MenuTitle<M>>,
    /// The Bluetooth extra (hidden without an adapter).
    pub bluetooth: Handle<MenuTitle<M>>,
    /// The Help title, which menu search opens under (unbound when the bar has none).
    pub help: Handle<MenuTitle<M>>,
}

impl<M> Default for BarHandles<M> {
    fn default() -> Self {
        Self {
            bar: Handle::default(),
            app: Handle::default(),
            clock: Handle::default(),
            battery: Handle::default(),
            network: Handle::default(),
            bluetooth: Handle::default(),
            help: Handle::default(),
        }
    }
}

/// What a bar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarContent {
    /// The active app's name, in bold after the logo.
    pub app_name: String,
    /// The clock extra's text.
    pub time: String,
    /// The first item of the clock's menu.
    pub full_date: String,
    /// The battery extra's state; `None` (no battery) hides it.
    pub battery: Option<BatteryView>,
    /// The network extra's state; `None` (no NetworkManager) hides it.
    pub network: Option<NetworkView>,
    /// The Bluetooth extra's state; `None` (no adapter) hides it.
    pub bluetooth: Option<BluetoothView>,
}

/// A menu bar showing `content` and the app's `menus` after its name, its logo menu
/// sending `message(command)`, its network menu `network(command)` and its Bluetooth menu
/// `bluetooth(command)` (the network menu also sends [`NetworkCommand::Scan`] when it
/// opens); `handles` are pointed at the parts that change. `backdrop` is translucent when
/// the compositor blurs behind the bar (task 3.5).
pub fn menu_bar<M: Clone + 'static>(
    content: &BarContent,
    menus: Vec<MenuTitle<M>>,
    message: impl Fn(SystemCommand) -> M + Clone + 'static,
    network: impl Fn(NetworkCommand) -> M + Clone + 'static,
    bluetooth: impl Fn(BluetoothCommand) -> M + Clone + 'static,
    handles: &mut BarHandles<M>,
    backdrop: Backdrop,
) -> MenuBar<M> {
    // All the extras are always in the bar, so that they can appear without rebuilding it.
    let battery = content.battery.unwrap_or(BatteryView {
        percent: 100,
        charging: false,
        on_ac: false,
        time_left: None,
    });
    let (battery_icon, battery_name) = battery_icon(&battery);
    let network_view = content.network.clone().unwrap_or(NetworkView {
        wifi_enabled: false,
        wired: Vec::new(),
        wifi: None,
    });
    let (network_icon, network_name) = network_icon(&network_view);
    let bluetooth_view = content.bluetooth.clone().unwrap_or(BluetoothView {
        powered: false,
        devices: Vec::new(),
    });
    let (bluetooth_icon, bluetooth_name) = bluetooth_icon(&bluetooth_view);
    let bar = MenuBar::new()
        .backdrop(backdrop)
        .with(MenuTitle::logo(LOGO_NAME, logo_menu(message)))
        // The app's menu: the fallback menu's (3.9) or GTK's; empty opens nothing.
        .with(
            MenuTitle::new(content.app_name.clone(), Menu::new())
                .bold(true)
                .bind(&mut handles.app),
        );
    menus
        .into_iter()
        .fold(bar, MenuBar::with_menu)
        .with_extra(
            MenuTitle::new(content.time.clone(), clock_menu(&content.full_date))
                .bind(&mut handles.clock),
        )
        .with_extra(
            MenuTitle::icon(battery_icon, battery_name, battery_menu(&battery))
                .hidden(content.battery.is_none())
                .bind(&mut handles.battery),
        )
        .with_extra(
            MenuTitle::icon(
                network_icon,
                network_name,
                network_menu(&network_view, network.clone()),
            )
            .on_open(network(NetworkCommand::Scan))
            .hidden(content.network.is_none())
            .bind(&mut handles.network),
        )
        .with_extra(
            MenuTitle::icon(
                bluetooth_icon,
                bluetooth_name,
                bluetooth_menu(&bluetooth_view, bluetooth),
            )
            .hidden(content.bluetooth.is_none())
            .bind(&mut handles.bluetooth),
        )
        .bind(&mut handles.bar)
}

#[cfg(test)]
mod tests {
    use neue_kit::{CheckState, MenuToggle};

    use super::*;

    fn net(ssid: &str, strength: u8, secured: bool, saved: bool, active: bool) -> WifiNetworkView {
        WifiNetworkView {
            ssid: ssid.to_owned(),
            strength,
            secured,
            saved,
            active,
        }
    }

    fn wired(connected: bool, carrier: bool) -> WiredView {
        WiredView {
            name: "enp0s1".to_owned(),
            carrier,
            connected,
            connecting: false,
        }
    }

    fn wifi(connected: bool, networks: Vec<WifiNetworkView>) -> WifiView {
        WifiView {
            connected,
            connecting: false,
            networks,
        }
    }

    fn view(enabled: bool, wired: Vec<WiredView>, wifi: Option<WifiView>) -> NetworkView {
        NetworkView {
            wifi_enabled: enabled,
            wired,
            wifi,
        }
    }

    fn icon(view: &NetworkView) -> NetworkIcon {
        match network_icon(view).0 {
            ExtraIcon::Network(icon) => icon,
            other => panic!("not a network icon: {other:?}"),
        }
    }

    /// The labels of the menu's items, with `-` for separators.
    fn labels(menu: &Menu<NetworkCommand>) -> Vec<String> {
        menu.items()
            .iter()
            .map(|i| {
                if i.is_separator() {
                    "-".to_owned()
                } else {
                    i.label().to_owned()
                }
            })
            .collect()
    }

    #[test]
    fn signal_strength_lights_one_to_three_arcs() {
        let bars: Vec<u8> = [0, 33, 34, 66, 67, 100].map(signal_bars).into();
        assert_eq!(bars, [1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn the_icon_follows_the_connection() {
        let home = wifi(true, vec![net("Home", 80, true, true, true)]);
        // Wi-Fi joined: the signal of its network.
        let v = view(true, vec![], Some(home.clone()));
        assert_eq!(icon(&v), NetworkIcon::Wifi { level: 3 });
        assert_eq!(
            network_icon(&v).1,
            "Network, Wi-Fi, Home, signal 80 percent"
        );
        // A wired connection that is up shows before Wi-Fi.
        let v = view(true, vec![wired(true, true)], Some(home.clone()));
        assert_eq!(icon(&v), NetworkIcon::Wired);
        // Wi-Fi on, joined to nothing.
        let v = view(
            true,
            vec![],
            Some(wifi(false, vec![net("Cafe", 50, false, false, false)])),
        );
        assert_eq!(icon(&v), NetworkIcon::Wifi { level: 0 });
        assert_eq!(network_icon(&v).1, "Network, Wi-Fi on, not connected");
        // Switched off, and the switch off with a cable that is down.
        let v = view(false, vec![wired(false, false)], Some(wifi(false, vec![])));
        assert_eq!(icon(&v), NetworkIcon::WifiOff);
        assert_eq!(network_icon(&v).1, "Network, Wi-Fi off");
        // No Wi-Fi device: wired down, or nothing at all.
        assert_eq!(
            icon(&view(false, vec![wired(false, true)], None)),
            NetworkIcon::Disconnected
        );
        let v = view(false, vec![], None);
        assert_eq!(icon(&v), NetworkIcon::Disconnected);
        assert_eq!(network_icon(&v).1, "Network, disconnected");
    }

    #[test]
    fn the_wifi_menu_lists_networks_with_a_tick_and_greys_out_unsaved_protected_ones() {
        let networks = vec![
            net("Home", 80, true, true, true),
            net("Cafe", 50, false, false, false),
            net("Library", 20, true, false, false),
        ];
        let v = view(true, vec![wired(false, true)], Some(wifi(true, networks)));
        let menu = network_menu::<NetworkCommand>(&v, |c| c);
        assert_eq!(
            labels(&menu),
            [
                "Wi-Fi: On",
                "Turn Wi-Fi Off",
                "-",
                "Home",
                "Cafe",
                "Library",
                "-",
                "Disconnect Wi-Fi",
                "-",
                "Ethernet: enp0s1",
            ]
        );
        let items = menu.items();
        assert!(!items[0].is_enabled(), "the state line is for reading");
        let ticked = |i: usize| items[i].toggle() == MenuToggle::Check(CheckState::On);
        assert!(ticked(3) && !ticked(4));
        assert!(items[3].is_enabled() && items[4].is_enabled());
        assert!(!items[5].is_enabled(), "protected and not saved");
    }

    #[test]
    fn the_menu_follows_the_switch_and_the_devices() {
        // Wi-Fi off: the switch only, no list.
        let v = view(false, vec![], Some(wifi(false, vec![])));
        let menu = network_menu::<NetworkCommand>(&v, |c| c);
        assert_eq!(labels(&menu), ["Wi-Fi: Off", "Turn Wi-Fi On"]);
        // On with nothing in range.
        let v = view(true, vec![], Some(wifi(false, vec![])));
        let menu = network_menu::<NetworkCommand>(&v, |c| c);
        assert_eq!(
            labels(&menu),
            ["Wi-Fi: On", "Turn Wi-Fi Off", "-", "No Networks"]
        );
        // No devices at all.
        let menu = network_menu::<NetworkCommand>(&view(false, vec![], None), |c| c);
        assert_eq!(labels(&menu), ["No Network Devices"]);
        // Wired only: its submenu offers Disconnect, Connect, or says the cable is out.
        let sub = |w| {
            let menu = network_menu::<NetworkCommand>(&view(false, vec![w], None), |c| c);
            let item = &menu.items()[0];
            labels(item.child_menu().expect("a submenu"))
        };
        assert_eq!(sub(wired(true, true)), ["Disconnect"]);
        assert_eq!(sub(wired(false, true)), ["Connect"]);
        assert_eq!(sub(wired(false, false)), ["Cable Unplugged"]);
    }
}
