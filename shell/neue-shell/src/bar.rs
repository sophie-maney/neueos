//! The menu bar's pieces (task 3.2b): the logo menu, the active app's name, the app's own
//! menus (the global menu, task 3.7a), and the clock, battery and Bluetooth extras (task
//! 3.11b), put together in a [`MenuBar`]. Portable: the shell builds one per output, and the
//! gallery's Shell page and the goldens build the same.

use std::time::Duration;

use neue_kit::desktop::{Backdrop, ExtraIcon, MenuBar, MenuTitle};
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
    /// The Bluetooth extra's state; `None` (no adapter) hides it.
    pub bluetooth: Option<BluetoothView>,
}

/// A menu bar showing `content` and the app's `menus` after its name, its logo menu
/// sending `message(command)` and its Bluetooth menu `bluetooth(command)`; `handles` are pointed at the parts that change. `backdrop`
/// is translucent when the compositor blurs behind the bar (task 3.5).
pub fn menu_bar<M: 'static>(
    content: &BarContent,
    menus: Vec<MenuTitle<M>>,
    message: impl Fn(SystemCommand) -> M + Clone + 'static,
    bluetooth: impl Fn(BluetoothCommand) -> M + Clone + 'static,
    handles: &mut BarHandles<M>,
    backdrop: Backdrop,
) -> MenuBar<M> {
    // Both extras are always in the bar, so that they can appear without rebuilding it.
    let battery = content.battery.unwrap_or(BatteryView {
        percent: 100,
        charging: false,
        on_ac: false,
        time_left: None,
    });
    let (battery_icon, battery_name) = battery_icon(&battery);
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
                bluetooth_icon,
                bluetooth_name,
                bluetooth_menu(&bluetooth_view, bluetooth),
            )
            .hidden(content.bluetooth.is_none())
            .bind(&mut handles.bluetooth),
        )
        .bind(&mut handles.bar)
}
