//! The shell process (Linux): a menu bar on every output, the active app's name and menus
//! from the compositor's window list, the clock, and the logo menu's commands.

use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

use anyhow::Context as _;
use neue_config::{AppearanceConfig, Layers};
use neue_dbus::login::Login;
use neue_desktop::{AppDatabase, LaunchOptions, Locale};
use neue_kit::desktop::{
    Anchor, Desktop, Fit, LayerKeyboard, LayerSpec, OutputEvent, OutputId, Picture, ShellEvent,
    ShellLayer, ShellRequest,
};
use neue_kit::{Application, Cx, Handle, Settings, WindowId};
use neue_shell::about::About;
use neue_shell::bar::{self, BarContent, BarHandles, SystemCommand};
use neue_shell::clock;
use neue_shell::fallback::{self, Command, Context, WindowEntry};
use neue_shell::help::{self, PanelHandles};
use neue_shell::picture;
use neue_shell::search::{self, Entry};
use neue_shell::windows::Windows;

use crate::appearance;
use crate::global_menu::{self, Address, Input, MenuEvent, Menus};

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
    /// A logind call finished.
    Login(Result<(), String>),
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
            },
            windows,
            about: None,
            global_menu: GlobalMenu::default(),
            apps: Arc::default(),
            fallback_view: None,
            search: Search::default(),
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
        }
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
        self.load_apps(cx);
        self.tick(cx);
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        match message {
            Message::Tick => self.tick(cx),
            Message::Sync => {
                self.sync_bars(cx);
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
            Message::Login(Ok(())) => {}
            Message::Login(Err(error)) => tracing::warn!(%error, "logind call failed"),
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
