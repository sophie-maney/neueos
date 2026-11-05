//! The shell process (Linux): a menu bar on every output, the active app's name from the
//! compositor's window list, the clock, and the logo menu's commands.

use std::collections::BTreeMap;

use anyhow::Context as _;
use neue_dbus::login::Login;
use neue_kit::desktop::{
    Anchor, LayerKeyboard, LayerSpec, OutputEvent, OutputId, ShellEvent, ShellRequest,
};
use neue_kit::{Application, Cx, Settings, WindowId};
use neue_shell::about::About;
use neue_shell::bar::{self, BarContent, BarHandles, SystemCommand};
use neue_shell::clock;
use neue_shell::windows::Windows;

/// The bar's layer-shell namespace.
const NAMESPACE: &str = "neue-menubar";

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
}

struct Bar {
    window: WindowId,
    handles: BarHandles<Message>,
}

struct Shell {
    bars: BTreeMap<OutputId, Bar>,
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
            content: BarContent {
                app_name: windows.active_app_name(),
                time: clock::time_text(now),
                full_date: clock::full_date_text(now),
            },
            windows,
            about: None,
        }
    }

    fn open_bar(&mut self, cx: &mut Cx<'_, Message>, output: OutputId) {
        if self.bars.contains_key(&output) {
            return;
        }
        let mut handles = BarHandles::default();
        let root = bar::menu_bar(&self.content, Message::System, &mut handles);
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
        let window = neue_kit::Window::new("Menu Bar", (0, height as u32), root).layer(layer);
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
            self.open_bar(cx, output.id);
        }
        self.tick(cx);
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        match message {
            Message::Tick => self.tick(cx),
            Message::Sync => self.sync_bars(cx),
            Message::System(command) => self.system(cx, command),
            Message::Login(Ok(())) => {}
            Message::Login(Err(error)) => tracing::warn!(%error, "logind call failed"),
        }
    }

    fn output(&mut self, cx: &mut Cx<'_, Message>, event: OutputEvent) {
        match event {
            OutputEvent::Added(info) => self.open_bar(cx, info.id),
            // The layer surface follows the output's size on its own.
            OutputEvent::Changed(_) => {}
            OutputEvent::Removed(id) => {
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
        ..Settings::default()
    };
    neue_kit::run(Shell::new(), settings).context("the shell's event loop failed")
}
