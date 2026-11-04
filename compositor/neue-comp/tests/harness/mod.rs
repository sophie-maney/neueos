//! Test harness: a headless compositor on its own thread and scripted Wayland clients.
//!
//! ```ignore
//! let server = Server::start();
//! let mut client = server.connect();
//! client.create_toplevel("Title", "org.example.App", (200, 100));
//! let windows = server.with_state(|state| state.windows());
//! ```
//!
//! The compositor has no Wayland listening socket: each client gets one end of a socket
//! pair, so tests need no `XDG_RUNTIME_DIR` and can run in parallel. It does serve
//! `neuectl` on a socket in a temporary directory ([`Server::ipc`]). Client calls block
//! until the compositor has answered (`wl_display.sync`), so after a client call returns,
//! [`Server::with_state`] and the IPC socket see every request that client sent before it.

#![allow(dead_code)]
// Each test binary uses a different subset of the harness.
// Test code: a failed step should fail the test with a message, like in `#[test]` fns.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod client;

use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use neue_comp::{BackendKind, Compositor, IpcSocket, Options, State};
use smithay::reexports::calloop::channel::{self, Event, Sender};

#[allow(unused_imports)] // Not every test binary names both types.
pub use client::{
    Anchor, CaptureFrame, CaptureSession, Constraint, ConstraintEvents, Drag, Feedback,
    ForeignToplevel, FrameState, GestureSeen, InputMethodEvents, KdeDefaultMode, KdeMode,
    KeyboardInteractivity, Layer, LayerHandle, RelativeMotion, SessionEvents, SessionLock,
    TestClient, TextInputEvents, Toplevel, XdgMode, global_names, read_all,
};

/// How long [`Server::with_state`] waits for the compositor before failing the test.
const STATE_TIMEOUT: Duration = Duration::from_secs(10);

type Job = Box<dyn FnOnce(&mut State) + Send>;

/// A headless compositor running on a background thread. Stopped on drop.
pub struct Server {
    jobs: Sender<Job>,
    socket: std::path::PathBuf,
    config_dir: std::path::PathBuf,
    _socket_dir: tempfile::TempDir,
    thread: Option<JoinHandle<Result<(), neue_comp::Error>>>,
}

impl Server {
    /// Start a headless compositor and wait until it is ready.
    pub fn start() -> Self {
        Self::start_with_config(None)
    }

    /// Start a headless compositor whose `compositor.toml` holds `config` (none: no file).
    /// The file lives in a temporary user config directory, never the real one.
    pub fn start_with_config(config: Option<&str>) -> Self {
        Self::start_with(config, false)
    }

    /// Start a headless compositor with its GLES renderer, for tests that read pixels back
    /// (`State::headless_render`). Needs Mesa's software EGL device.
    pub fn start_rendering() -> Self {
        Self::start_with(None, true)
    }

    fn start_with(config: Option<&str>, headless_renderer: bool) -> Self {
        init_tracing();
        let socket_dir = tempfile::tempdir().expect("creating a temporary directory");
        let socket = socket_dir.path().join("comp.sock");
        let config_dir = socket_dir.path().join("config");
        std::fs::create_dir(&config_dir).expect("creating the config directory");
        if let Some(text) = config {
            std::fs::write(config_dir.join("compositor.toml"), text).expect("writing the config");
        }
        let layers = neue_config::Layers::from_dirs([config_dir.clone()]);
        let ipc_socket = IpcSocket::Path(socket.clone());
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("neue-comp".into())
            .spawn(move || {
                let options = Options {
                    listen: false,
                    ipc_socket,
                    config: layers,
                    headless_renderer,
                    ..Options::new(BackendKind::Headless)
                };
                let compositor = Compositor::new(&options)?;
                let (jobs, receiver) = channel::channel::<Job>();
                compositor
                    .loop_handle()
                    .insert_source(receiver, |event, _, state| {
                        if let Event::Msg(job) = event {
                            job(state);
                        }
                    })
                    .expect("adding the job channel");
                ready_tx.send(jobs).expect("test thread is waiting");
                compositor.run()
            })
            .expect("spawning the compositor thread");

        match ready_rx.recv() {
            Ok(jobs) => Self {
                jobs,
                socket,
                config_dir,
                _socket_dir: socket_dir,
                thread: Some(thread),
            },
            // The thread returned before sending: report its error.
            Err(_) => match thread.join() {
                Ok(Err(err)) => panic!("compositor failed to start: {err:#?}"),
                _ => panic!("compositor thread exited during start-up"),
            },
        }
    }

    /// Run `f` on the compositor thread, between event loop dispatches, and return its
    /// result.
    pub fn with_state<T, F>(&self, f: F) -> T
    where
        T: Send + 'static,
        F: FnOnce(&mut State) -> T + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.jobs
            .send(Box::new(move |state| {
                let _ = tx.send(f(state));
            }))
            .expect("compositor thread is gone");
        rx.recv_timeout(STATE_TIMEOUT)
            .expect("compositor did not answer in time")
    }

    /// Replace `compositor.toml` the way editors do: write a temporary file, rename it over.
    pub fn write_config(&self, text: &str) {
        let temp = self.config_dir.join(".compositor.toml.tmp");
        std::fs::write(&temp, text).expect("writing the config");
        std::fs::rename(&temp, self.config_dir.join("compositor.toml")).expect("renaming");
    }

    /// Connect a new `neuectl` client to the compositor's IPC socket.
    pub fn ipc(&self) -> neue_ipc::Client {
        neue_ipc::Client::connect(&self.socket).expect("connecting to the IPC socket")
    }

    /// Connect a new scripted client.
    pub fn connect(&self) -> TestClient {
        let (ours, theirs) = UnixStream::pair().expect("creating a socket pair");
        self.with_state(move |state| {
            state
                .insert_client(theirs)
                .map(drop)
                .map_err(|err| err.to_string())
        })
        .expect("inserting the client");
        TestClient::new(ours)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self
            .jobs
            .send(Box::new(|state: &mut State| state.loop_signal.stop()));
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    if !std::thread::panicking() {
                        panic!("compositor failed: {err:#?}");
                    }
                }
                Err(_) => {
                    if !std::thread::panicking() {
                        panic!("compositor thread panicked");
                    }
                }
            }
        }
    }
}

/// Log to the test output, filtered by `RUST_LOG` (default: warnings).
fn init_tracing() {
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into());
    // Fails when another test in this binary already installed it; that's fine.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
}
