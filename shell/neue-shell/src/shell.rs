//! The shell process (Linux): a menu bar on every output, the active app's name and menus
//! from the compositor's window list, the clock, and the logo menu's commands.

use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

use anyhow::Context as _;
use neue_config::{AppearanceConfig, Layers};
use neue_dbus::bluetooth::{Bluetooth, State as BluetoothState};
use neue_dbus::login::Login;
use neue_dbus::network::{Network, State as NetworkState};
use neue_dbus::settings::Settings as SettingsClient;
use neue_dbus::{Host, MenuNode};
use neue_desktop::{AppDatabase, LaunchOptions, Locale};
use neue_kit::desktop::{
    Anchor, Desktop, Fit, LayerKeyboard, LayerSpec, OutputEvent, OutputId, Picture, ShellEvent,
    ShellLayer, ShellRequest,
};
use neue_kit::{Application, Cx, Handle, Settings, WindowId};
use neue_shell::about::About;
use neue_shell::bar::{
    self, BarContent, BarHandles, BatteryView, BluetoothCommand, ExtraMessages, NetworkCommand,
    NotificationsCommand, NotificationsView, SystemCommand, TrayActions, TrayItemView,
    VolumeCommand,
};
use neue_shell::clock;
use neue_shell::fallback::{self, Command, Context, WindowEntry};
use neue_shell::help::{self, PanelHandles};
use neue_shell::picture;
use neue_shell::search::{self, Entry};
use neue_shell::windows::Windows;

use crate::appearance;
use crate::extras;
use crate::global_menu::{self, Address, Input, MenuEvent, Menus};
use crate::notify::{self, Notifier};
use crate::osd;
use crate::tray;

/// The bar's layer-shell namespace.
const NAMESPACE: &str = "neue-menubar";
/// The background's layer-shell namespace.
const BACKGROUND_NAMESPACE: &str = "neue-desktop";

#[derive(Debug, Clone)]
enum Message {
    /// Time to update the clock.
    Tick,
    /// Show the current content on every bar: a bar opened in the same callback as a
    /// change missed it, since a queued window's widgets can't be edited yet.
    Sync,
    System(SystemCommand),
    /// Something chosen in the network menu (task 3.12c wires it up).
    Network(NetworkCommand),
    /// NetworkManager's devices or networks changed (`None`: no network device, or
    /// NetworkManager isn't on the bus).
    NetworkState(Option<NetworkState>),
    /// The network worker stopped (it logged why).
    NetworkEnded,
    /// A NetworkManager call (Wi-Fi switch, join, connect, disconnect) finished.
    NetworkCall(Result<(), String>),
    /// Something chosen in the Bluetooth menu.
    Bluetooth(BluetoothCommand),
    /// Something done in the volume menu (task 3.14c).
    Volume(VolumeCommand),
    /// Something chosen in the notifications menu (task 3.15d4).
    Notifications(NotificationsCommand),
    /// Do Not Disturb changed in `shell.toml` (from the menu, Preferences or a hand edit).
    DoNotDisturb(bool),
    /// The Do Not Disturb worker stopped (it logged why).
    DoNotDisturbEnded,
    /// Saving Do Not Disturb through `neue-settingsd` finished.
    DoNotDisturbSaved(Result<(), String>),
    /// UPower's battery changed (`None`: there is none, or UPower isn't on the bus).
    Battery(Option<BatteryView>),
    /// The battery worker stopped (it logged why).
    BatteryEnded,
    /// BlueZ's adapter or devices changed (`None`: no adapter, or BlueZ isn't on the bus).
    BluetoothState(Option<BluetoothState>),
    /// The Bluetooth worker stopped (it logged why).
    BluetoothEnded,
    /// A BlueZ call (power, connect, disconnect) finished.
    BluetoothCall(Result<(), String>),
    /// The tray worker's host, for the actions on items.
    TrayHost(Host),
    /// A status item appeared, changed, got its menu or left.
    Tray(tray::Update),
    /// The tray worker stopped (it logged why).
    TrayEnded,
    /// A primary press on item `id`: its app's `Activate`.
    TrayPress(String),
    /// A secondary press on item `id` that has no menu to open: its app's `ContextMenu`.
    TrayContext(String),
    /// Item `n` of the menu of item `id` was chosen.
    TrayMenu(String, i32),
    /// A tray menu went up (`true`) or came down.
    TrayMenuUp(bool),
    /// A call on a status item finished.
    TrayCall(Result<(), String>),
    /// A logind call finished.
    Login(Result<(), String>),
    /// The system keys' timers and workers.
    Osd(osd::Event),
    /// The notification server, the banners and their timers (task 3.15d3).
    Notify(notify::Event),
    /// The configured picture was decoded, or why not.
    Picture(Result<(Picture, Fit), String>),
    /// `appearance.toml` changed.
    Appearance(Box<AppearanceConfig>),
    /// The `appearance.toml` watcher stopped (it logged why).
    AppearanceWatchEnded,
    /// The active app's menus were read.
    AppMenus(Menus),
    /// The user did something with the app's menus.
    AppMenu(MenuEvent),
    /// The global-menu worker stopped (it logged why).
    GlobalMenuEnded,
    /// An item of the fallback menu was chosen.
    Fallback(Command),
    /// A fallback or Help menu went up (`true`) or came down.
    FallbackMenu(bool),
    /// The installed apps were read.
    Apps(Arc<AppDatabase>),
    /// Help ▸ Search Menus… was chosen.
    OpenSearch,
    /// The search field's text changed.
    SearchQuery(String),
    /// Return in the search field.
    SearchSubmit(String),
    /// Hit `n` of the search was pressed.
    SearchPick(usize),
    /// The search panel closed by itself.
    SearchClosed,
}

/// A BlueZ call, with the object paths it is made on.
enum BluetoothCall {
    /// Turns the adapter at this path on or off.
    Power(String, bool),
    Connect(String),
    Disconnect(String),
}

/// A NetworkManager call, with the object paths it is made on.
enum NetworkCall {
    SetWifi(bool),
    Scan(String),
    /// The Wi-Fi device and the access point to join.
    Join(String, String),
    Connect(String),
    Disconnect(String),
}

/// What choosing a hit of menu search does.
#[derive(Debug, Clone)]
enum Target {
    /// An item of the app's own menus.
    App(MenuEvent),
    /// An item of the fallback menu.
    Fallback(Command),
}

/// Menu search as the UI thread keeps it.
#[derive(Default)]
struct Search {
    /// The panel is open (under the Help title; the bars' menus stay as they are).
    open: bool,
    /// The bar whose Help title the panel is under.
    anchor: Option<Handle<neue_kit::desktop::MenuTitle<Message>>>,
    handles: PanelHandles<Message>,
    /// Everything that can be found, as of opening.
    entries: Vec<Entry<Target>>,
    /// What the field's text found.
    hits: Vec<Entry<Target>>,
}

struct Bar {
    window: WindowId,
    handles: BarHandles<Message>,
}

struct Background {
    window: WindowId,
    desktop: Handle<Desktop>,
}

/// A status item as the UI thread keeps it.
struct TrayEntry {
    item: tray::Item,
    /// Its DBusMenu as last read.
    menu: Vec<MenuNode>,
}

/// The status-icon tray as the UI thread keeps it.
#[derive(Default)]
struct Tray {
    /// For the actions on items; `None` until the worker has it, and after it stopped.
    host: Option<Host>,
    /// In order of arrival (the first is rightmost).
    items: Vec<TrayEntry>,
    /// One of the tray's menus is up.
    open: bool,
    /// The bars' tray titles need replacing (an item came or left, got another menu, or
    /// acts differently now): done once no tray menu is up.
    dirty: bool,
}

/// What item `view` does in the bar: a primary press activates its app and a secondary one
/// opens its menu, except for an item that is only a menu (`ItemIsMenu`), which opens it
/// on any press.
fn tray_actions(tray: &Tray, view: &TrayItemView) -> TrayActions<Message> {
    let Some(entry) = tray.items.iter().find(|e| e.item.view.id == view.id) else {
        return TrayActions::menu(neue_kit::Menu::new());
    };
    let id = view.id.clone();
    let menu = {
        let id = id.clone();
        global_menu::app_menu(&entry.menu, 0, false, move |event| {
            Message::TrayMenu(id.clone(), event.id)
        })
    };
    let only_menu = entry.item.item_is_menu && entry.item.has_menu;
    TrayActions {
        menu,
        press: (!only_menu).then(|| Message::TrayPress(id.clone())),
        context: Some(Message::TrayContext(id)),
        on_open: Some(Message::TrayMenuUp(true)),
        on_close: Some(Message::TrayMenuUp(false)),
    }
}

struct Shell {
    bars: BTreeMap<OutputId, Bar>,
    backgrounds: BTreeMap<OutputId, Background>,
    /// The configured picture once decoded; until then (and when there is none, or it
    /// can't be loaded) the built-in one.
    photo: Option<(Picture, Fit)>,
    windows: Windows,
    /// What every bar shows.
    content: BarContent,
    about: Option<WindowId>,
    global_menu: GlobalMenu,
    /// The installed apps, for each app's `new-window` action and whether it's a terminal.
    apps: Arc<AppDatabase>,
    /// What the bars' fallback menu was built from (`None`: the app's own menus, or none).
    fallback_view: Option<Context>,
    search: Search,
    /// BlueZ as of its last change: the menu's device indices are looked up here.
    bluetooth: Option<BluetoothState>,
    /// NetworkManager as of its last change: the menu's indices are looked up here.
    network: Option<NetworkState>,
    tray: Tray,
    /// The volume and brightness keys and their OSD.
    keys: osd::Keys,
    /// The notification server and its banners.
    notifications: Notifier<Message>,
}

/// The global menu as the UI thread keeps it.
#[derive(Default)]
struct GlobalMenu {
    /// To the worker; `None` until it starts, and after it stopped.
    worker: Option<mpsc::Sender<Input>>,
    /// The address the worker follows.
    following: Option<Address>,
    /// Counts [`Input::Follow`]s as the worker does: menus of another generation were read
    /// for an earlier window.
    generation: u64,
    /// What the bars show.
    shown: Option<Menus>,
    /// Newer menus, held back while one of the shown menus is up.
    pending: Option<Menus>,
    /// One of the bar's menus is up.
    open: bool,
    /// The bars' menus need building again once nothing is up.
    dirty: bool,
    /// The app's menus were read for the current generation and have no items.
    empty: bool,
    /// The active window's app gets Super as Control: its Control shortcuts show as ⌘
    /// (`command_key`, ADR 0081).
    super_as_ctrl: bool,
}

impl GlobalMenu {
    fn send(&mut self, input: Input) {
        if let Some(worker) = &self.worker
            && worker.send(input).is_err()
        {
            self.worker = None;
        }
    }
}

impl Shell {
    fn new() -> Self {
        let now = clock::now();
        let windows = Windows::default();
        Self {
            bars: BTreeMap::new(),
            backgrounds: BTreeMap::new(),
            photo: None,
            content: BarContent {
                app_name: windows.active_app_name(),
                time: clock::time_text(now),
                full_date: clock::full_date_text(now),
                // Shown once the workers (`start_extras`) find a battery and an adapter.
                battery: None,
                // Shown once PipeWire reports a default sink.
                volume: None,
                // Shown once the network worker finds a network device.
                network: None,
                bluetooth: None,
                // The Do Not Disturb worker reads it as soon as it starts.
                notifications: NotificationsView::default(),
                // Filled as the tray's worker finds status items.
                tray: Vec::new(),
            },
            windows,
            about: None,
            global_menu: GlobalMenu::default(),
            apps: Arc::default(),
            fallback_view: None,
            search: Search::default(),
            bluetooth: None,
            network: None,
            tray: Tray::default(),
            keys: osd::Keys::default(),
            notifications: Notifier::new(Message::Notify),
        }
    }

    /// Reads the desktop entries on a thread of their own.
    fn load_apps(&self, cx: &mut Cx<'_, Message>) {
        cx.spawn(|_| {
            let dirs = neue_desktop::apps::default_application_dirs();
            Message::Apps(Arc::new(AppDatabase::load(&dirs, &Locale::from_env())))
        });
    }

    /// Starts the global-menu worker on its own thread.
    fn start_global_menu(&mut self, cx: &mut Cx<'_, Message>) {
        let (tx, rx) = mpsc::channel();
        let signals = tx.clone();
        self.global_menu.worker = Some(tx);
        cx.spawn(move |sender| {
            let send = |menus| sender.send(Message::AppMenus(menus));
            if let Err(err) = global_menu::run(&rx, signals, send) {
                tracing::warn!(%err, "no global menu: the session bus is unavailable");
            }
            Message::GlobalMenuEnded
        });
    }

    /// Starts the battery, network and Bluetooth workers, each on a thread of its own.
    fn start_extras(cx: &mut Cx<'_, Message>) {
        cx.spawn(|sender| {
            if let Err(err) = extras::run_battery(|view| sender.send(Message::Battery(view))) {
                tracing::warn!(%err, "no battery extra: the system bus is unavailable");
            }
            Message::BatteryEnded
        });
        cx.spawn(|sender| {
            if let Err(err) = extras::run_network(|state| sender.send(Message::NetworkState(state)))
            {
                tracing::warn!(%err, "no network extra: the system bus is unavailable");
            }
            Message::NetworkEnded
        });
        cx.spawn(|sender| {
            if let Err(err) =
                extras::run_bluetooth(|state| sender.send(Message::BluetoothState(state)))
            {
                tracing::warn!(%err, "no Bluetooth extra: the system bus is unavailable");
            }
            Message::BluetoothEnded
        });
    }

    /// Follows Do Not Disturb in `shell.toml` on a thread of its own.
    fn start_do_not_disturb(cx: &mut Cx<'_, Message>) {
        cx.spawn(|sender| {
            if let Err(err) = extras::run_do_not_disturb(|on| sender.send(Message::DoNotDisturb(on)))
            {
                tracing::warn!(%err, "Do Not Disturb is not followed: the session bus is unavailable");
            }
            Message::DoNotDisturbEnded
        });
    }

    /// Does what the notifications menu chose: saves Do Not Disturb through
    /// `neue-settingsd`, on a thread of its own. The extra changes when the service's
    /// change signal comes back, so what it shows is what the file says.
    fn notifications_command(cx: &mut Cx<'_, Message>, command: NotificationsCommand) {
        let NotificationsCommand::SetDoNotDisturb(on) = command;
        cx.spawn(move |_| {
            Message::DoNotDisturbSaved(
                SettingsClient::session()
                    .and_then(|settings| settings.set_bool(extras::DO_NOT_DISTURB_KEY, on))
                    .map_err(|e| e.to_string()),
            )
        });
    }

    /// Shows Do Not Disturb on every bar.
    fn set_do_not_disturb(&mut self, cx: &mut Cx<'_, Message>, on: bool) {
        if self.content.notifications.do_not_disturb != on {
            self.content.notifications.do_not_disturb = on;
            self.sync_bars(cx);
        }
    }

    /// Starts the status-icon tray's worker on a thread of its own.
    fn start_tray(cx: &mut Cx<'_, Message>) {
        cx.spawn(|sender| {
            let result = tray::run(
                |host| sender.send(Message::TrayHost(host)),
                |update| sender.send(Message::Tray(update)),
            );
            if let Err(err) = result {
                tracing::warn!(%err, "no status-icon tray: the session bus is unavailable or another watcher owns its name");
            }
            Message::TrayEnded
        });
    }

    /// Takes what the tray worker said into the items.
    fn tray_update(&mut self, cx: &mut Cx<'_, Message>, update: tray::Update) {
        let items = &mut self.tray.items;
        match update {
            tray::Update::Item(item) => {
                match items.iter_mut().find(|e| e.item.view.id == item.view.id) {
                    Some(entry) => {
                        // Another menu or press behaviour needs new titles; the icon, the
                        // name, the tooltip and the status change in place.
                        if entry.item.has_menu != item.has_menu
                            || entry.item.item_is_menu != item.item_is_menu
                        {
                            self.tray.dirty = true;
                        }
                        if !item.has_menu && !entry.menu.is_empty() {
                            entry.menu.clear();
                            self.tray.dirty = true;
                        }
                        entry.item = item;
                    }
                    None => {
                        items.push(TrayEntry {
                            item,
                            menu: Vec::new(),
                        });
                        self.tray.dirty = true;
                    }
                }
            }
            tray::Update::Menu { id, nodes } => {
                if let Some(entry) = items.iter_mut().find(|e| e.item.view.id == id)
                    && entry.menu != nodes
                {
                    entry.menu = nodes;
                    self.tray.dirty = true;
                }
            }
            tray::Update::Removed(id) => {
                items.retain(|e| e.item.view.id != id);
                self.tray.dirty = true;
            }
        }
        self.sync_tray(cx);
    }

    /// Shows the tray's items on every bar: in place while the same icons are there, by
    /// replacing the tray's titles when items came, left or changed their menus (held back
    /// while a tray menu is up).
    fn sync_tray(&mut self, cx: &mut Cx<'_, Message>) {
        self.content.tray = self
            .tray
            .items
            .iter()
            .map(|e| e.item.view.clone())
            .collect();
        if self.tray.dirty {
            if self.tray.open {
                return;
            }
            self.tray.dirty = false;
            let Self {
                bars,
                tray,
                content,
                ..
            } = self;
            for bar in bars.values_mut() {
                let titles = bar::tray_titles(
                    &content.tray,
                    |view| tray_actions(tray, view),
                    &mut bar.handles.tray,
                );
                bar.handles.bar.set_tray(cx, titles);
            }
            return;
        }
        for bar in self.bars.values() {
            for (handle, view) in bar.handles.tray.iter().zip(&self.content.tray) {
                bar::update_tray_title(handle, cx, view);
            }
        }
    }

    /// Calls `call` on item `id`'s app, on a thread of its own (apps answer slowly or not
    /// at all).
    fn tray_call(
        &self,
        cx: &mut Cx<'_, Message>,
        id: String,
        call: impl FnOnce(&Host, &str) -> Result<(), neue_dbus::Error> + Send + 'static,
    ) {
        let Some(host) = self.tray.host.clone() else {
            return;
        };
        cx.spawn(move |_| Message::TrayCall(call(&host, &id).map_err(|e| e.to_string())));
    }

    /// The bar's titles: the fallback menu's, or the app's menus being shown, and Help
    /// after them. `help` is pointed at the Help title.
    fn app_titles(
        &self,
        help: &mut Handle<neue_kit::desktop::MenuTitle<Message>>,
    ) -> Vec<neue_kit::desktop::MenuTitle<Message>> {
        *help = Handle::default();
        let up = |title: neue_kit::desktop::MenuTitle<Message>| {
            title
                .on_open(Message::FallbackMenu(true))
                .on_close(Message::FallbackMenu(false))
        };
        if let Some(context) = &self.fallback_view {
            let mut titles: Vec<_> = fallback::titles(&context.sections(), &Message::Fallback)
                .into_iter()
                .map(up)
                .collect();
            titles.push(up(help::title(Message::OpenSearch, help)));
            return titles;
        }
        let Some(menus) = self.global_menu.shown.as_ref() else {
            return Vec::new();
        };
        let mut titles = global_menu::bar_titles(
            &menus.titles,
            menus.generation,
            self.global_menu.super_as_ctrl,
            Some(&Message::OpenSearch),
            help,
            Message::AppMenu,
        );
        // No Help of the app's own: ours, unless the app shows nothing yet.
        if !help.is_bound() && !titles.is_empty() {
            titles.push(up(help::title(Message::OpenSearch, help)));
        }
        titles
    }

    /// The menu under the app's name: the fallback's App menu, the app menu GTK apps
    /// export, else nothing.
    fn app_name_menu(&self) -> neue_kit::Menu<Message> {
        if let Some(context) = &self.fallback_view {
            return fallback::menu(&context.app_section().rows, &Message::Fallback);
        }
        self.global_menu
            .shown
            .as_ref()
            .map_or_else(neue_kit::Menu::new, |m| {
                global_menu::app_menu(
                    &m.app_menu,
                    m.generation,
                    self.global_menu.super_as_ctrl,
                    Message::AppMenu,
                )
            })
    }

    /// Shows the app's menus on every bar.
    fn sync_menus(&mut self, cx: &mut Cx<'_, Message>) {
        let outputs: Vec<OutputId> = self.bars.keys().copied().collect();
        for output in outputs {
            let mut help = Handle::default();
            let titles = self.app_titles(&mut help);
            let name_menu = self.app_name_menu();
            if let Some(bar) = self.bars.get_mut(&output) {
                bar.handles.help = help;
                bar.handles.bar.set_menus(cx, titles);
                bar.handles.app.set_menu(cx, name_menu);
            }
        }
    }

    /// A menu is up, or the search panel (which hangs off a title the bar would replace).
    fn busy(&self) -> bool {
        self.global_menu.open || self.search.open
    }

    /// Shows what the menus now are on the bars: at once, or when nothing is up.
    fn refresh(&mut self, cx: &mut Cx<'_, Message>) {
        if self.busy() {
            self.global_menu.dirty = true;
            return;
        }
        if let Some(menus) = self.global_menu.pending.take() {
            self.global_menu.shown = Some(menus);
        }
        self.global_menu.dirty = false;
        self.sync_menus(cx);
    }

    /// Shows `menus`, or holds them back while a menu is up.
    fn show_menus(&mut self, cx: &mut Cx<'_, Message>, menus: Menus) {
        self.global_menu.pending = Some(menus);
        self.refresh(cx);
    }

    /// Something that kept the bars' menus from changing ended.
    fn unbusy(&mut self, cx: &mut Cx<'_, Message>) {
        if !self.busy() && self.global_menu.dirty {
            self.refresh(cx);
        }
    }

    /// The fallback menu's context if the active window should have the fallback menu:
    /// it exports no menus, or none that could be read.
    fn fallback_context(&self) -> Option<Context> {
        let active = self.windows.active()?;
        let menus = &self.global_menu;
        if !(menus.following.is_none() || menus.empty || menus.worker.is_none()) {
            return None;
        }
        let current = self.windows.active_id();
        let windows = self
            .windows
            .app_windows()
            .into_iter()
            .map(|(id, w)| WindowEntry {
                id,
                title: w.title.clone(),
                active: Some(id) == current,
                minimized: w.minimized,
            })
            .collect();
        let entry = self.apps.find_by_app_id(&active.app_id);
        Some(Context {
            app_name: self.windows.active_app_name(),
            windows,
            can_new_window: entry.is_some_and(|e| e.new_window_action().is_some()),
            super_as_ctrl: active.super_as_ctrl,
            terminal: entry.is_some_and(|e| e.categories.iter().any(|c| c == "TerminalEmulator")),
        })
    }

    /// Works out the fallback menu again; true if it changed.
    fn set_fallback(&mut self) -> bool {
        let now = self.fallback_context();
        if now == self.fallback_view {
            return false;
        }
        self.fallback_view = now;
        true
    }

    /// Follows the active window's menus.
    fn follow_menus(&mut self, cx: &mut Cx<'_, Message>) {
        let super_as_ctrl = self.windows.active().is_some_and(|w| w.super_as_ctrl);
        let mut changed = super_as_ctrl != self.global_menu.super_as_ctrl;
        self.global_menu.super_as_ctrl = super_as_ctrl;
        let address = self.windows.active().and_then(|w| Address::of(&w.menu));
        if address != self.global_menu.following {
            self.global_menu.following.clone_from(&address);
            self.global_menu.generation += 1;
            self.global_menu.empty = false;
            // The old app's menus go at once; the new ones follow when read.
            if self.global_menu.shown.is_some() {
                self.global_menu.pending = Some(Menus::empty(self.global_menu.generation));
            }
            self.global_menu.send(Input::Follow(address));
            changed = true;
        }
        // After the follow, which says whether the menus are to be read.
        changed |= self.set_fallback();
        if changed {
            self.refresh(cx);
        }
    }

    fn app_menu(&mut self, cx: &mut Cx<'_, Message>, event: MenuEvent) {
        match event.event {
            global_menu::ItemEvent::Opened => self.global_menu.open = true,
            global_menu::ItemEvent::Closed => {
                self.global_menu.open = false;
                self.unbusy(cx);
            }
            global_menu::ItemEvent::Clicked => {}
        }
        self.global_menu.send(Input::Event(event));
    }

    /// Runs the fallback menu's `command` for the active window's app.
    fn fallback_command(&mut self, cx: &mut Cx<'_, Message>, command: Command) {
        let Some(context) = self.fallback_context() else {
            return;
        };
        if command == Command::NewWindow {
            self.new_window();
        }
        for request in context.requests(command) {
            cx.shell(request);
        }
    }

    /// Runs the active app's `new-window` action.
    fn new_window(&self) {
        let Some(active) = self.windows.active() else {
            return;
        };
        let Some(entry) = self.apps.find_by_app_id(&active.app_id) else {
            return;
        };
        let options = LaunchOptions {
            action: Some("new-window"),
            ..LaunchOptions::default()
        };
        if let Err(error) = neue_desktop::launch::launch(entry, &options) {
            tracing::warn!(%error, app = %active.app_id, "can't open a new window");
        }
    }

    /// What menu search can find now.
    fn search_entries(&self) -> Vec<Entry<Target>> {
        if let Some(context) = &self.fallback_view {
            return context
                .entries()
                .into_iter()
                .map(|e| Entry {
                    path: e.path,
                    shortcut: e.shortcut,
                    target: Target::Fallback(e.target),
                })
                .collect();
        }
        let menus = self
            .global_menu
            .pending
            .as_ref()
            .or(self.global_menu.shown.as_ref());
        menus.map_or_else(Vec::new, |m| {
            global_menu::entries(
                m,
                &self.windows.active_app_name(),
                self.global_menu.super_as_ctrl,
            )
            .into_iter()
            .map(|e| Entry {
                path: e.path,
                shortcut: e.shortcut,
                target: Target::App(e.target),
            })
            .collect()
        })
    }

    /// Opens the search panel under the Help title of the first bar that has one.
    fn open_search(&mut self, cx: &mut Cx<'_, Message>) {
        let Some(anchor) = self
            .bars
            .values()
            .map(|b| b.handles.help)
            .find(Handle::is_bound)
        else {
            return;
        };
        self.search.entries = self.search_entries();
        self.search.hits.clear();
        self.search.anchor = Some(anchor);
        self.search.open = true;
        help::show(
            cx,
            &anchor,
            &mut self.search.handles,
            Message::SearchQuery,
            Message::SearchSubmit,
            Message::SearchClosed,
        );
    }

    fn search_query(&mut self, cx: &mut Cx<'_, Message>, text: &str) {
        self.search.hits = search::search(&self.search.entries, text)
            .into_iter()
            .cloned()
            .collect();
        let texts: Vec<String> = self.search.hits.iter().map(Entry::text).collect();
        help::set_hits(cx, &self.search.handles, &texts, Message::SearchPick);
    }

    fn close_search(&mut self, cx: &mut Cx<'_, Message>) {
        if let Some(anchor) = self.search.anchor.take() {
            help::close(cx, &anchor);
        }
        self.search.open = false;
        self.unbusy(cx);
    }

    /// Closes the panel and runs hit `n`.
    fn search_pick(&mut self, cx: &mut Cx<'_, Message>, n: usize) {
        let Some(hit) = self.search.hits.get(n).cloned() else {
            return;
        };
        // First, so that the keyboard is back with the app before a chord is typed.
        self.close_search(cx);
        match hit.target {
            Target::App(event) => self.global_menu.send(Input::Event(event)),
            Target::Fallback(command) => self.fallback_command(cx, command),
        }
    }

    fn open_background(&mut self, cx: &mut Cx<'_, Message>, output: OutputId) {
        if self.backgrounds.contains_key(&output) {
            return;
        }
        let mut handle = Handle::default();
        let mut desktop = Desktop::new().bind(&mut handle);
        if let Some((picture, fit)) = &self.photo {
            desktop = desktop.photo(picture.clone(), *fit);
        }
        let layer = LayerSpec {
            layer: ShellLayer::Background,
            // Under the bar and the windows, over the whole output whatever other
            // surfaces reserve.
            exclusive_zone: -1,
            keyboard: LayerKeyboard::None,
            output: Some(output),
            ..LayerSpec::new(BACKGROUND_NAMESPACE, Anchor::ALL)
        };
        let window = neue_kit::Window::new("Desktop", (0, 0), desktop).layer(layer);
        let window = cx.open_window(window);
        self.backgrounds.insert(
            output,
            Background {
                window,
                desktop: handle,
            },
        );
    }

    /// Follows `appearance.toml` from now on: the watcher thread sends each change.
    fn watch_appearance(&self, cx: &mut Cx<'_, Message>) {
        let layers = Layers::standard();
        let current = appearance::load(&layers);
        cx.spawn(move |tx| {
            appearance::watch(&layers, current, |config| {
                tx.send(Message::Appearance(Box::new(config.clone())))
            });
            Message::AppearanceWatchEnded
        });
    }

    /// Starts decoding the picture `shell.toml` asks for, if any.
    fn load_picture(&self, cx: &mut Cx<'_, Message>) {
        let loaded = match neue_config::load::<neue_config::ShellConfig>(
            &neue_config::Layers::standard(),
        ) {
            Ok(loaded) => loaded,
            Err(error) => {
                tracing::warn!(%error, "shell.toml is invalid: using the built-in desktop picture");
                return;
            }
        };
        for key in &loaded.unknown_keys {
            tracing::warn!(key, "unknown key in shell.toml");
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let Some(wanted) = picture::wanted(&loaded.config.desktop, home.as_deref()) else {
            return;
        };
        cx.spawn(move |_| {
            Message::Picture(
                picture::load(&wanted.path)
                    .map(|picture| (picture, wanted.fit))
                    .map_err(|error| error.to_string()),
            )
        });
    }

    fn open_bar(&mut self, cx: &mut Cx<'_, Message>, output: OutputId) {
        if self.bars.contains_key(&output) {
            return;
        }
        let mut handles = BarHandles::default();
        let titles = self.app_titles(&mut handles.help);
        let root = bar::menu_bar(
            &self.content,
            titles,
            Message::System,
            ExtraMessages {
                volume: Message::Volume,
                network: Message::Network,
                bluetooth: Message::Bluetooth,
                notifications: Message::Notifications,
            },
            |view| tray_actions(&self.tray, view),
            &mut handles,
            cx.backdrop(),
        );
        let height = neue_kit::tokens::materials::menu_bar::HEIGHT;
        #[expect(clippy::cast_possible_truncation, reason = "the bar's 22 px height")]
        let layer = LayerSpec {
            exclusive_zone: height as i32,
            // Never takes keyboard focus from the app it names; its menus are grabbing
            // popups, which get the keyboard while they're up.
            keyboard: LayerKeyboard::None,
            output: Some(output),
            ..LayerSpec::new(NAMESPACE, Anchor::TOP_EDGE)
        };
        #[expect(clippy::cast_possible_truncation, reason = "the bar's 22 px height")]
        // Translucent over a blur when the compositor offers one, else the opaque tint.
        let window = neue_kit::Window::new("Menu Bar", (0, height as u32), root)
            .layer(layer)
            .blur_behind();
        let window = cx.open_window(window);
        self.bars.insert(output, Bar { window, handles });
        // Delivered after the driver has created the window.
        cx.spawn(|_| Message::Sync);
    }

    /// Shows `self.content` on every bar (each edit is a no-op when nothing changed).
    fn sync_bars(&self, cx: &mut Cx<'_, Message>) {
        for bar in self.bars.values() {
            bar.handles.app.set_text(cx, self.content.app_name.clone());
            bar.handles.clock.set_text(cx, self.content.time.clone());
            bar.handles
                .clock
                .set_menu(cx, bar::clock_menu(&self.content.full_date));
            let battery = &bar.handles.battery;
            match &self.content.battery {
                Some(view) => {
                    let (icon, name) = bar::battery_icon(view);
                    battery.set_icon(cx, icon, name);
                    battery.set_menu(cx, bar::battery_menu(view));
                    battery.set_hidden(cx, false);
                }
                None => battery.set_hidden(cx, true),
            }
            let volume = &bar.handles.volume;
            match &self.content.volume {
                Some(view) => {
                    let (icon, name) = bar::volume_icon(view);
                    volume.set_icon(cx, icon, name);
                    volume.set_menu(cx, bar::volume_menu(view, Message::Volume));
                    volume.set_hidden(cx, false);
                }
                None => volume.set_hidden(cx, true),
            }
            let notifications = &bar.handles.notifications;
            let (icon, name) = bar::notifications_icon(&self.content.notifications);
            notifications.set_icon(cx, icon, name);
            notifications.set_menu(
                cx,
                bar::notifications_menu(&self.content.notifications, Message::Notifications),
            );
            let network = &bar.handles.network;
            match &self.content.network {
                Some(view) => {
                    let (icon, name) = bar::network_icon(view);
                    network.set_icon(cx, icon, name);
                    network.set_menu(cx, bar::network_menu(view, Message::Network));
                    network.set_hidden(cx, false);
                }
                None => network.set_hidden(cx, true),
            }
            let bluetooth = &bar.handles.bluetooth;
            match &self.content.bluetooth {
                Some(view) => {
                    let (icon, name) = bar::bluetooth_icon(view);
                    bluetooth.set_icon(cx, icon, name);
                    bluetooth.set_menu(cx, bar::bluetooth_menu(view, Message::Bluetooth));
                    bluetooth.set_hidden(cx, false);
                }
                None => bluetooth.set_hidden(cx, true),
            }
        }
    }

    /// Shows a new battery on every bar.
    fn set_battery(&mut self, cx: &mut Cx<'_, Message>, battery: Option<BatteryView>) {
        if self.content.battery != battery {
            self.content.battery = battery;
            self.sync_bars(cx);
        }
    }

    /// Shows NetworkManager's new state on every bar.
    fn set_network(&mut self, cx: &mut Cx<'_, Message>, state: Option<NetworkState>) {
        let view = state.as_ref().map(extras::network_view);
        self.network = state;
        if self.content.network != view {
            self.content.network = view;
            self.sync_bars(cx);
        }
    }

    /// Does what the network menu chose, on a thread of its own (joining takes seconds).
    /// The result is only logged: NetworkManager's change signals bring the new state.
    fn network_command(&mut self, cx: &mut Cx<'_, Message>, command: NetworkCommand) {
        let Some(state) = &self.network else {
            return;
        };
        let wifi = state.wifi.as_ref();
        let wired = |i: usize| state.wired.get(i).map(|w| w.device.clone());
        let call = match command {
            NetworkCommand::SetWifi(on) => Some(NetworkCall::SetWifi(on)),
            // A switched-off radio can't scan.
            NetworkCommand::Scan => wifi
                .filter(|_| state.wifi_enabled)
                .map(|w| NetworkCall::Scan(w.device.clone())),
            NetworkCommand::Join(i) => wifi.and_then(|w| {
                let ap = w.access_points.get(i)?;
                Some(NetworkCall::Join(w.device.clone(), ap.path.clone()))
            }),
            NetworkCommand::DisconnectWifi => {
                wifi.map(|w| NetworkCall::Disconnect(w.device.clone()))
            }
            NetworkCommand::ConnectWired(i) => wired(i).map(NetworkCall::Connect),
            NetworkCommand::DisconnectWired(i) => wired(i).map(NetworkCall::Disconnect),
        };
        let Some(call) = call else {
            return;
        };
        cx.spawn(move |_| {
            let result = Network::system().and_then(|network| match &call {
                NetworkCall::SetWifi(on) => network.set_wifi_enabled(*on),
                NetworkCall::Scan(device) => network.request_scan(device),
                NetworkCall::Join(device, ap) => network.join(device, ap),
                NetworkCall::Connect(device) => network.connect(device),
                NetworkCall::Disconnect(device) => network.disconnect(device),
            });
            Message::NetworkCall(match (&call, result) {
                // NetworkManager refuses scans that follow each other closely; the list
                // is current enough.
                (NetworkCall::Scan(_), Err(err)) => {
                    tracing::debug!(%err, "Wi-Fi scan refused");
                    Ok(())
                }
                (_, result) => result.map_err(|e| e.to_string()),
            })
        });
    }

    /// Shows BlueZ's new state on every bar.
    fn set_bluetooth(&mut self, cx: &mut Cx<'_, Message>, state: Option<BluetoothState>) {
        let view = state.as_ref().map(extras::bluetooth_view);
        self.bluetooth = state;
        if self.content.bluetooth != view {
            self.content.bluetooth = view;
            self.sync_bars(cx);
        }
    }

    /// Does what the Bluetooth menu chose, on a thread of its own (connecting takes
    /// seconds). The result needs no handling beyond logging: BlueZ's change signals bring
    /// the new state.
    fn bluetooth_command(&mut self, cx: &mut Cx<'_, Message>, command: BluetoothCommand) {
        let Some(state) = &self.bluetooth else {
            return;
        };
        let path = |index: usize| state.devices.get(index).map(|d| d.path.clone());
        let call = match command {
            BluetoothCommand::SetPowered(on) => {
                BluetoothCall::Power(state.adapter.path.clone(), on)
            }
            BluetoothCommand::Connect(i) => match path(i) {
                Some(path) => BluetoothCall::Connect(path),
                None => return,
            },
            BluetoothCommand::Disconnect(i) => match path(i) {
                Some(path) => BluetoothCall::Disconnect(path),
                None => return,
            },
        };
        cx.spawn(move |_| {
            Message::BluetoothCall(
                Bluetooth::system()
                    .and_then(|bluetooth| match &call {
                        BluetoothCall::Power(adapter, on) => bluetooth.set_powered(adapter, *on),
                        BluetoothCall::Connect(device) => bluetooth.connect(device),
                        BluetoothCall::Disconnect(device) => bluetooth.disconnect(device),
                    })
                    .map_err(|e| e.to_string()),
            )
        });
    }

    fn tick(&mut self, cx: &mut Cx<'_, Message>) {
        let now = clock::now();
        let (time, full_date) = (clock::time_text(now), clock::full_date_text(now));
        if self.content.time != time || self.content.full_date != full_date {
            self.content.time = time;
            self.content.full_date = full_date;
            self.sync_bars(cx);
        }
        cx.spawn(move |_| {
            std::thread::sleep(clock::next_tick(now));
            Message::Tick
        });
    }

    fn app_changed(&mut self, cx: &mut Cx<'_, Message>) {
        let name = self.windows.active_app_name();
        if self.content.app_name == name {
            return;
        }
        self.content.app_name = name;
        self.sync_bars(cx);
    }

    fn system(&mut self, cx: &mut Cx<'_, Message>, command: SystemCommand) {
        let login = |cx: &mut Cx<'_, Message>, call: fn(&Login) -> Result<(), neue_dbus::Error>| {
            cx.spawn(move |_| {
                Message::Login(
                    Login::system()
                        .and_then(|login| call(&login))
                        .map_err(|e| e.to_string()),
                )
            });
        };
        match command {
            SystemCommand::About => {
                if let Some(window) = self.about.take() {
                    cx.close_window(window);
                }
                self.about = Some(cx.open_window(About::read().window()));
            }
            // Disabled until the Preferences app exists (task 3.24).
            SystemCommand::Preferences => {}
            SystemCommand::Lock => login(cx, Login::lock_session),
            SystemCommand::Restart => login(cx, Login::reboot),
            SystemCommand::ShutDown => login(cx, Login::power_off),
            SystemCommand::LogOut => cx.shell(ShellRequest::RunAction {
                name: "quit".to_owned(),
                window: None,
            }),
        }
    }
}

impl Application for Shell {
    type Message = Message;

    fn start(&mut self, cx: &mut Cx<'_, Message>) {
        // No surface until the outputs arrive, and none while every output is unplugged.
        cx.keep_alive();
        if !cx.has_shell() {
            tracing::warn!("the compositor doesn't offer neue_shell_v1: no window list");
        }
        for output in cx.outputs() {
            self.open_background(cx, output.id);
            self.open_bar(cx, output.id);
        }
        self.load_picture(cx);
        self.watch_appearance(cx);
        self.start_global_menu(cx);
        Self::start_extras(cx);
        Self::start_do_not_disturb(cx);
        Self::start_tray(cx);
        self.keys.start(cx, Message::Osd);
        self.notifications.start(cx);
        self.load_apps(cx);
        self.tick(cx);
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        match message {
            Message::Tick => self.tick(cx),
            Message::Sync => {
                self.sync_bars(cx);
                self.sync_tray(cx);
                // A new bar can't take the menus while another bar's menu is up.
                if !self.busy() {
                    self.sync_menus(cx);
                }
            }
            Message::AppMenus(menus) => {
                // Menus read for an earlier window are stale.
                if menus.generation == self.global_menu.generation {
                    self.global_menu.empty = menus.titles.is_empty() && menus.app_menu.is_empty();
                    self.set_fallback();
                    self.show_menus(cx, menus);
                }
            }
            Message::AppMenu(event) => self.app_menu(cx, event),
            Message::GlobalMenuEnded => {
                self.global_menu.worker = None;
                if self.set_fallback() {
                    self.refresh(cx);
                }
            }
            Message::Fallback(command) => self.fallback_command(cx, command),
            Message::FallbackMenu(open) => {
                self.global_menu.open = open;
                self.unbusy(cx);
            }
            Message::Apps(apps) => {
                self.apps = apps;
                if self.set_fallback() {
                    self.refresh(cx);
                }
            }
            Message::OpenSearch => self.open_search(cx),
            Message::SearchQuery(text) => self.search_query(cx, &text),
            Message::SearchSubmit(text) => {
                // Return before the change was seen: search first.
                if self.search.hits.is_empty() {
                    self.search_query(cx, &text);
                }
                self.search_pick(cx, 0);
            }
            Message::SearchPick(n) => self.search_pick(cx, n),
            Message::SearchClosed => {
                self.search.open = false;
                self.search.anchor = None;
                self.unbusy(cx);
            }
            Message::System(command) => self.system(cx, command),
            Message::Network(command) => self.network_command(cx, command),
            Message::NetworkState(state) => self.set_network(cx, state),
            Message::NetworkEnded => self.set_network(cx, None),
            Message::NetworkCall(result) => {
                if let Err(err) = result {
                    tracing::warn!(%err, "a network call failed");
                }
            }
            Message::Bluetooth(command) => self.bluetooth_command(cx, command),
            Message::Notifications(command) => Self::notifications_command(cx, command),
            Message::DoNotDisturb(on) => self.set_do_not_disturb(cx, on),
            Message::DoNotDisturbEnded => {}
            Message::DoNotDisturbSaved(result) => {
                if let Err(err) = result {
                    tracing::warn!(%err, "Do Not Disturb was not saved: is neue-settingsd running?");
                }
            }
            Message::Battery(battery) => self.set_battery(cx, battery),
            Message::BatteryEnded => self.set_battery(cx, None),
            Message::BluetoothState(state) => self.set_bluetooth(cx, state),
            Message::BluetoothEnded => self.set_bluetooth(cx, None),
            Message::BluetoothCall(result) => {
                if let Err(err) = result {
                    tracing::warn!(%err, "a Bluetooth call failed");
                }
            }
            Message::TrayHost(host) => self.tray.host = Some(host),
            Message::Tray(update) => self.tray_update(cx, update),
            Message::TrayEnded => {
                self.tray = Tray {
                    dirty: true,
                    ..Tray::default()
                };
                self.sync_tray(cx);
            }
            // The position is a hint for apps that place their own windows; the bar's
            // surface isn't asked for it.
            Message::TrayPress(id) => self.tray_call(cx, id, |host, id| host.activate(id, 0, 0)),
            Message::TrayContext(id) => {
                self.tray_call(cx, id, |host, id| host.context_menu(id, 0, 0));
            }
            Message::TrayMenu(id, item) => self.tray_call(cx, id, move |host, id| {
                host.menu(id)?
                    .map_or(Ok(()), |menu| menu.event(item, "clicked"))
            }),
            Message::TrayMenuUp(open) => {
                self.tray.open = open;
                if !open && self.tray.dirty {
                    self.sync_tray(cx);
                }
            }
            Message::TrayCall(result) => {
                if let Err(err) = result {
                    tracing::warn!(%err, "a status item didn't take a call");
                }
            }
            Message::Login(Ok(())) => {}
            Message::Login(Err(error)) => tracing::warn!(%error, "logind call failed"),
            Message::Osd(event) => {
                let audio = matches!(event, osd::Event::Audio(_));
                self.keys.event(cx, Message::Osd, event);
                // PipeWire's news also moves the volume extra.
                let view = self.keys.volume_view();
                if audio && self.content.volume != view {
                    self.content.volume = view;
                    self.sync_bars(cx);
                }
            }
            Message::Notify(event) => self.notifications.event(cx, event),
            Message::Volume(command) => self.keys.volume_command(command),
            Message::Picture(Ok((picture, fit))) => {
                for background in self.backgrounds.values() {
                    background.desktop.set_photo(cx, picture.clone(), fit);
                }
                self.photo = Some((picture, fit));
            }
            Message::Appearance(config) => cx.set_theme(config.theme()),
            Message::AppearanceWatchEnded => {}
            Message::Picture(Err(error)) => {
                tracing::warn!(%error, "can't load the desktop picture: using the built-in one");
            }
        }
    }

    fn output(&mut self, cx: &mut Cx<'_, Message>, event: OutputEvent) {
        match event {
            OutputEvent::Added(info) => {
                self.open_background(cx, info.id);
                self.open_bar(cx, info.id);
            }
            // The layer surface follows the output's size on its own.
            OutputEvent::Changed(_) => {}
            OutputEvent::Removed(id) => {
                if let Some(background) = self.backgrounds.remove(&id) {
                    cx.close_window(background.window);
                }
                if let Some(bar) = self.bars.remove(&id) {
                    cx.close_window(bar.window);
                }
            }
        }
    }

    fn shell(&mut self, cx: &mut Cx<'_, Message>, event: ShellEvent) {
        if let ShellEvent::SystemKey {
            key,
            state,
            modifiers,
        } = event
        {
            self.keys.key(cx, Message::Osd, key, state, modifiers);
            return;
        }
        if let ShellEvent::FocusMenuBar { output } = event {
            // Ctrl+F2: the first menu, from the keyboard. Right away, so that its popup
            // grabs with the event's serial.
            let bar = output
                .and_then(|o| self.bars.get(&o))
                .or_else(|| self.bars.values().next());
            if let Some(bar) = bar {
                bar.handles.bar.open_first_menu(cx);
            }
            return;
        }
        self.windows.apply(event);
        self.app_changed(cx);
        self.follow_menus(cx);
    }
}

pub fn run() -> anyhow::Result<()> {
    let settings = Settings {
        app_id: "org.neueos.Shell".to_owned(),
        theme: appearance::start_theme(&Layers::standard()),
    };
    neue_kit::run(Shell::new(), settings).context("the shell's event loop failed")
}
