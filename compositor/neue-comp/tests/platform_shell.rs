//! neue-platform's Wayland driver as the shell (task 3.2a, ADR 0067): it hears about the
//! outputs and puts a layer surface on each, gets the window list and the active window
//! over `neue_shell_v1`, and its requests act on windows. Needs Mesa's software EGL, as
//! `platform.rs` does.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use harness::Server;
use masonry::core::{ErasedAction, NewWidget, WidgetId};
use masonry::peniko::Blob;
use masonry::widgets::Label;
use neue_comp::WindowInfo;
use neue_platform::desktop::{
    OutputEvent, OutputId, ShellEvent, ShellRequest, SystemKey, SystemKeyState, Toplevel,
    ToplevelId,
};
use neue_platform::{
    Anchor, App, Ctx, LayerSpec, Options, TokenRequest, Waker, WindowId, WindowSpec,
};
use smithay::backend::input::{InputTime, KeyState as Key};
use smithay::input::keyboard::Keycode;
use wayland_client::Connection;

const NAMESPACE: &str = "test-bar";

/// What the driver thread reports to the test.
#[derive(Debug)]
enum Report {
    Started {
        waker: Waker,
        shell: bool,
    },
    Output(OutputEvent),
    Shell(ShellEvent),
    /// The answer to an `Ask::Token`.
    Token(Option<String>),
}

/// What the test asks the driver thread to do.
#[derive(Debug)]
enum Ask {
    Shell(ShellRequest),
    /// An activation token for the first bar (`Ctx::activation_token`).
    Token,
}

/// A shell in miniature: a bar per output; requests come from the test over a channel.
struct TestShell {
    reports: Sender<Report>,
    requests: Receiver<Ask>,
    bars: Vec<WindowId>,
}

impl App for TestShell {
    fn on_start(&mut self, cx: &mut Ctx<'_>) {
        cx.keep_alive();
        let report = Report::Started {
            waker: cx.waker(),
            shell: cx.has_shell(),
        };
        self.reports.send(report).unwrap();
    }

    fn on_action(&mut self, _: &mut Ctx<'_>, _: WindowId, _: WidgetId, _: ErasedAction) {}

    fn on_wake(&mut self, cx: &mut Ctx<'_>) {
        while let Ok(ask) = self.requests.try_recv() {
            match ask {
                Ask::Shell(request) => cx.shell(request),
                Ask::Token => {
                    cx.activation_token(self.bars[0]);
                }
            }
        }
    }

    fn on_activation_token(&mut self, _: &mut Ctx<'_>, _: TokenRequest, token: Option<String>) {
        let _ = self.reports.send(Report::Token(token));
    }

    fn on_output(&mut self, cx: &mut Ctx<'_>, event: OutputEvent) {
        if let OutputEvent::Added(info) = &event {
            let layer = LayerSpec {
                exclusive_zone: 20,
                output: Some(info.id),
                ..LayerSpec::new(NAMESPACE, Anchor::TOP_EDGE)
            };
            let root = NewWidget::new(Label::new("bar")).erased();
            let bar = cx.open_window(WindowSpec {
                kind: neue_platform::WindowKind::Layer(layer),
                ..WindowSpec::new("bar", (0, 20), root)
            });
            self.bars.push(bar);
        }
        let _ = self.reports.send(Report::Output(event));
    }

    fn on_shell(&mut self, _: &mut Ctx<'_>, event: ShellEvent) {
        let _ = self.reports.send(Report::Shell(event));
    }
}

fn options() -> Options {
    static FONT: &[u8] = include_bytes!("../../../crates/neue-paint/fonts/DejaVuSans.ttf");
    Options {
        default_properties: Arc::new(masonry::theme::default_property_set()),
        font: Some(Blob::new(Arc::new(FONT))),
        extra_fonts: Vec::new(),
    }
}

/// Polls `f` until it returns something, failing after 30 s (llvmpipe is slow).
fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The test's end of the driver: its reports so far and a way to send requests.
struct Driver {
    reports: Receiver<Report>,
    requests: Sender<Ask>,
    waker: Waker,
    tokens: Vec<Option<String>>,
    toplevels: Vec<(ToplevelId, Toplevel)>,
    active: Vec<Option<ToplevelId>>,
    outputs: Vec<OutputEvent>,
    failures: Vec<String>,
    /// The output of every `ShellEvent::FocusMenuBar`.
    focus_menu_bar: Vec<Option<OutputId>>,
    /// Every `ShellEvent::SystemKey`, without modifiers.
    system_keys: Vec<(SystemKey, SystemKeyState)>,
}

impl Driver {
    fn start(server: &Server) -> (Self, bool) {
        let stream = server.connect_shell_raw();
        let (report_tx, reports) = mpsc::channel();
        let (requests, request_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut app = TestShell {
                reports: report_tx,
                requests: request_rx,
                bars: Vec::new(),
            };
            let conn = Connection::from_socket(stream).unwrap();
            // Kept alive, it runs until the compositor goes away and the connection breaks.
            let _ = neue_platform::wayland::run_with_connection(conn, &mut app, options());
        });
        let Report::Started { waker, shell } = reports
            .recv_timeout(Duration::from_secs(30))
            .expect("the driver to start")
        else {
            panic!("the first report is Started");
        };
        let driver = Self {
            reports,
            requests,
            waker,
            tokens: Vec::new(),
            toplevels: Vec::new(),
            active: Vec::new(),
            outputs: Vec::new(),
            failures: Vec::new(),
            focus_menu_bar: Vec::new(),
            system_keys: Vec::new(),
        };
        (driver, shell)
    }

    fn send(&self, request: ShellRequest) {
        self.requests.send(Ask::Shell(request)).unwrap();
        self.waker.wake();
    }

    /// Asks for an activation token for the first bar and waits for the answer.
    fn token(&mut self) -> Option<String> {
        self.requests.send(Ask::Token).unwrap();
        self.waker.wake();
        let before = self.tokens.len();
        self.until("an activation token", |d| d.tokens.len() > before);
        self.tokens.pop().unwrap()
    }

    /// Takes in reports until `done` holds.
    fn until(&mut self, what: &str, mut done: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done(self) {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.reports.recv_timeout(left) {
                Ok(Report::Output(event)) => self.outputs.push(event),
                Ok(Report::Shell(ShellEvent::Toplevel(id, info))) => {
                    self.toplevels.retain(|(t, _)| *t != id);
                    self.toplevels.push((id, info));
                }
                Ok(Report::Shell(ShellEvent::Closed(id))) => {
                    self.toplevels.retain(|(t, _)| *t != id);
                }
                Ok(Report::Shell(ShellEvent::Active(id))) => self.active.push(id),
                Ok(Report::Shell(ShellEvent::ActionFailed { name, .. })) => {
                    self.failures.push(name);
                }
                Ok(Report::Shell(ShellEvent::FocusMenuBar { output })) => {
                    self.focus_menu_bar.push(output);
                }
                Ok(Report::Shell(ShellEvent::SystemKey { key, state, .. })) => {
                    self.system_keys.push((key, state));
                }
                Ok(Report::Token(token)) => self.tokens.push(token),
                Ok(Report::Started { .. }) => panic!("started twice"),
                Err(_) => panic!("timed out waiting for {what}; got {:#?}", self.toplevels),
            }
        }
    }

    fn toplevel(&self, title: &str) -> Option<(ToplevelId, &Toplevel)> {
        self.toplevels
            .iter()
            .find(|(_, t)| t.title == title)
            .map(|(id, t)| (*id, t))
    }
}

fn window(server: &Server, title: &'static str) -> WindowInfo {
    server
        .with_state(|state| state.windows())
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn bars(server: &Server) -> Vec<String> {
    let mut outputs: Vec<String> = server
        .with_state(|state| state.layer_surfaces())
        .into_iter()
        .filter(|(namespace, _)| namespace == NAMESPACE)
        .map(|(_, output)| output)
        .collect();
    outputs.sort();
    outputs
}

#[test]
fn the_driver_as_shell_gets_outputs_and_windows_and_acts_on_them() {
    let server = Server::start_rendering();
    server
        .with_state(|state| state.headless_add_output("HEADLESS-2", (1920, 1080)))
        .unwrap();
    let mut app = server.connect();
    let _back = app.create_toplevel("Back", "org.neueos.A", (200, 100));
    let _front = app.create_toplevel("Front", "org.neueos.B", (200, 100));

    let (mut driver, shell) = Driver::start(&server);
    assert!(shell, "the compositor offers neue_shell_v1 to its shell");

    // A bar on each output, placed by the app's `LayerSpec::output`.
    driver.until("both outputs", |d| d.outputs.len() >= 2);
    wait_for("a bar on each output", || {
        (bars(&server) == ["HEADLESS-1", "HEADLESS-2"]).then_some(())
    });

    // The window list, with the ids neuectl uses, and the active window.
    driver.until("both windows and the active one", |d| {
        d.toplevel("Back").is_some() && d.toplevel("Front").is_some() && !d.active.is_empty()
    });
    let (back, back_info) = driver.toplevel("Back").unwrap();
    assert_eq!(back_info.app_id, "org.neueos.A");
    assert_eq!(
        back_info.identifier,
        format!("window-{}", window(&server, "Back").id.0)
    );
    let (front, _) = driver.toplevel("Front").unwrap();
    assert_eq!(driver.active.last(), Some(&Some(front)));

    // Requests act on windows, and their effects come back as events.
    driver.send(ShellRequest::Activate(back));
    driver.until("Back to become active", |d| {
        d.active.last() == Some(&Some(back))
    });
    assert!(window(&server, "Back").focused);

    driver.send(ShellRequest::Minimize(front));
    driver.until("Front to be minimised", |d| {
        d.toplevel("Front").is_some_and(|(_, t)| t.minimized)
    });
    assert!(window(&server, "Front").minimized);

    driver.send(ShellRequest::RunAction {
        name: "no-such-action".into(),
        window: None,
    });
    driver.until("the action to fail", |d| !d.failures.is_empty());

    // focus-menu-bar (Ctrl+F2) reaches the shell with the active window's output.
    driver.send(ShellRequest::RunAction {
        name: "focus-menu-bar".into(),
        window: None,
    });
    driver.until("focus_menu_bar", |d| !d.focus_menu_bar.is_empty());
    let first = driver.outputs.iter().find_map(|e| match e {
        OutputEvent::Added(info) if info.name == "HEADLESS-1" => Some(info.id),
        _ => None,
    });
    assert_eq!(driver.focus_menu_bar, [first]);

    // Unplugging an output tells the app, and its bar goes with it.
    server
        .with_state(|state| state.headless_remove_output("HEADLESS-2"))
        .unwrap();
    driver.until("the output's removal", |d| {
        d.outputs
            .iter()
            .any(|e| matches!(e, OutputEvent::Removed(_)))
    });
    wait_for("the second bar to go", || {
        (bars(&server) == ["HEADLESS-1"]).then_some(())
    });
}

/// xkb keycodes (evdev + 8) in the default `us` keymap.
const VOLUME_UP: u32 = 115 + 8;
const MUTE: u32 = 113 + 8;

fn key(server: &Server, code: u32, state: Key) {
    server.with_state(move |s| s.press_key(Keycode::new(code), state, InputTime::now()));
}

#[test]
fn system_keys_reach_the_driver_and_held_steps_repeat() {
    let server = Server::start_rendering();
    let (mut driver, shell) = Driver::start(&server);
    assert!(shell);
    driver.until("an output", |d| !d.outputs.is_empty());

    // Held past the repeat delay, Volume Up repeats until it is released.
    key(&server, VOLUME_UP, Key::Pressed);
    driver.until("a repeat", |d| {
        d.system_keys
            .contains(&(SystemKey::VolumeUp, SystemKeyState::Repeated))
    });
    key(&server, VOLUME_UP, Key::Released);
    driver.until("the release", |d| {
        d.system_keys
            .contains(&(SystemKey::VolumeUp, SystemKeyState::Released))
    });
    let repeats = driver.system_keys.len();

    // Mute is a toggle: held, it doesn't repeat. Its release comes after the volume
    // key's repeat has stopped.
    key(&server, MUTE, Key::Pressed);
    std::thread::sleep(Duration::from_millis(1000));
    key(&server, MUTE, Key::Released);
    driver.until("mute's release", |d| {
        d.system_keys
            .contains(&(SystemKey::Mute, SystemKeyState::Released))
    });
    assert_eq!(
        driver.system_keys[0],
        (SystemKey::VolumeUp, SystemKeyState::Pressed)
    );
    assert_eq!(
        driver.system_keys[repeats..],
        [
            (SystemKey::Mute, SystemKeyState::Pressed),
            (SystemKey::Mute, SystemKeyState::Released)
        ]
    );
}

/// The windows from the bottom of the stack up, the focused one marked with `*`.
fn stack(server: &Server) -> Vec<String> {
    server
        .with_state(|state| state.windows())
        .into_iter()
        .map(|w| {
            let title = w.title.unwrap_or_default();
            if w.focused {
                format!("*{title}")
            } else {
                title
            }
        })
        .collect()
}

#[test]
fn the_shells_token_activates_another_clients_window_without_focus() {
    let server = Server::start_rendering();
    let (mut driver, _) = Driver::start(&server);
    wait_for("the bar", || {
        (bars(&server) == ["HEADLESS-1"]).then_some(())
    });
    let mut a = server.connect();
    let a_window = a.create_toplevel("A", "org.neueos.A", (300, 200));
    let mut b = server.connect();
    let b_window = b.create_toplevel("B", "org.neueos.B", (300, 200));
    assert_eq!(stack(&server), ["A", "*B"]);

    // The shell's bar never has keyboard focus, yet its token (handed to A, as a clicked
    // notification's `ActivationToken` would be) brings A forward.
    let shell_token = driver.token().expect("a token for the shell");
    a.activate(&a_window, &shell_token);
    assert_eq!(stack(&server), ["B", "*A"]);
    assert!(!window(&server, "A").demands_attention);

    // An ordinary client without focus still can't: B only asks for attention.
    let token = b.activation_token(None, Some(&b_window.surface));
    b.activate(&b_window, &token);
    assert_eq!(stack(&server), ["B", "*A"]);
    assert!(window(&server, "B").demands_attention);

    // The shell's token was single-use; a fresh one works again.
    b.activate(&b_window, &shell_token);
    assert_eq!(stack(&server), ["B", "*A"]);
    let fresh = driver.token().expect("another token");
    b.activate(&b_window, &fresh);
    assert_eq!(stack(&server), ["A", "*B"]);
    assert!(!window(&server, "B").demands_attention);
}

#[test]
fn an_ordinary_client_has_no_shell_protocol() {
    let server = Server::start_rendering();
    let stream = server.connect_raw();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        struct Probe(Sender<bool>);
        impl App for Probe {
            fn on_start(&mut self, cx: &mut Ctx<'_>) {
                self.0.send(cx.has_shell()).unwrap();
                cx.exit();
            }
            fn on_action(&mut self, _: &mut Ctx<'_>, _: WindowId, _: WidgetId, _: ErasedAction) {}
        }
        let conn = Connection::from_socket(stream).unwrap();
        neue_platform::wayland::run_with_connection(conn, &mut Probe(tx), options()).unwrap();
    });
    assert!(!rx.recv_timeout(Duration::from_secs(30)).unwrap());
}
