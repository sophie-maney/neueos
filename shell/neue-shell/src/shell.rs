//! The shell process (Linux): a menu bar on every output, the active app's name from the
//! compositor's window list, the clock, and the logo menu's commands.

use std::collections::BTreeMap;

use anyhow::Context as _;
use neue_config::{AppearanceConfig, Layers};
use neue_dbus::login::Login;
use neue_kit::desktop::{
    Anchor, Desktop, Fit, LayerKeyboard, LayerSpec, OutputEvent, OutputId, Picture, ShellEvent,
    ShellLayer, ShellRequest,
};
use neue_kit::{Application, Cx, Handle, Settings, WindowId};
use neue_shell::about::About;
use neue_shell::bar::{self, BarContent, BarHandles, SystemCommand};
use neue_shell::clock;
use neue_shell::picture;
use neue_shell::windows::Windows;

use crate::appearance;

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
        let root = bar::menu_bar(&self.content, Message::System, &mut handles, cx.backdrop());
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
        self.tick(cx);
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        match message {
            Message::Tick => self.tick(cx),
            Message::Sync => self.sync_bars(cx),
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
        self.windows.apply(event);
        self.app_changed(cx);
    }
}

pub fn run() -> anyhow::Result<()> {
    let settings = Settings {
        app_id: "org.neueos.Shell".to_owned(),
        theme: appearance::start_theme(&Layers::standard()),
    };
    neue_kit::run(Shell::new(), settings).context("the shell's event loop failed")
}
