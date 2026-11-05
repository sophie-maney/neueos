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
use neue_platform::desktop::{OutputEvent, ShellEvent, ShellRequest, Toplevel, ToplevelId};
use neue_platform::{Anchor, App, Ctx, LayerSpec, Options, Waker, WindowId, WindowSpec};
use wayland_client::Connection;

const NAMESPACE: &str = "test-bar";

/// What the driver thread reports to the test.
#[derive(Debug)]
enum Report {
    Started { waker: Waker, shell: bool },
    Output(OutputEvent),
    Shell(ShellEvent),
}

/// A shell in miniature: a bar per output; requests come from the test over a channel.
struct TestShell {
    reports: Sender<Report>,
    requests: Receiver<ShellRequest>,
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
        while let Ok(request) = self.requests.try_recv() {
            cx.shell(request);
        }
    }

    fn on_output(&mut self, cx: &mut Ctx<'_>, event: OutputEvent) {
        if let OutputEvent::Added(info) = &event {
            let layer = LayerSpec {
                exclusive_zone: 20,
                output: Some(info.id),
                ..LayerSpec::new(NAMESPACE, Anchor::TOP_EDGE)
            };
            let root = NewWidget::new(Label::new("bar")).erased();
            cx.open_window(WindowSpec {
                kind: neue_platform::WindowKind::Layer(layer),
                ..WindowSpec::new("bar", (0, 20), root)
            });
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
    requests: Sender<ShellRequest>,
    waker: Waker,
    toplevels: Vec<(ToplevelId, Toplevel)>,
    active: Vec<Option<ToplevelId>>,
    outputs: Vec<OutputEvent>,
    failures: Vec<String>,
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
            toplevels: Vec::new(),
            active: Vec::new(),
            outputs: Vec::new(),
            failures: Vec::new(),
        };
        (driver, shell)
    }

    fn send(&self, request: ShellRequest) {
        self.requests.send(request).unwrap();
        self.waker.wake();
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
