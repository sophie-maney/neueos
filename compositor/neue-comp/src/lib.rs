//! neueOS Wayland compositor.
//!
//! Linux-only: the whole crate is gated, so on other hosts it compiles to an empty
//! library and stays a workspace member.
//!
//! Structure:
//! - [`state`]: the Smithay state shared by every backend.
//! - [`Compositor`]: the event loop, the Wayland display and the state, wired together.
//! - `handlers`: Wayland protocol handlers (compositor, shm, dmabuf, seat, output, xdg-shell).
//! - `config`: `compositor.toml` loading, applying, hot reload and key bindings (ADR 0009).
//! - `input`: input dispatch, focus on click, compositor key bindings.
//! - [`placement`]: where new windows go (a cascading stack).
//! - [`outputs`]: output layout, hotplug, per-output scale and frame callbacks.
//! - `ipc`: the `neuectl` socket (ADR 0003, ADR 0007).
//! - `headless`: a virtual output without rendering or input; used by tests.
//! - `tty`: DRM/KMS, libinput and libseat on real hardware and the VM (cargo feature `tty`).
//! - `cursor`: the built-in pointer image.
//! - `winit`: the nested development backend (cargo feature `winit`).
//!
//! The skeleton follows Smithay's `smallvil` and `anvil` examples (MIT); see the comments in
//! each module for what was taken from where.
#![cfg(target_os = "linux")]

pub mod config;
#[cfg(feature = "tty")]
mod cursor;
mod grabs;
mod handlers;
mod headless;
mod input;
mod ipc;
pub mod outputs;
pub mod placement;
pub mod state;
#[cfg(feature = "tty")]
mod tty;
#[cfg(feature = "winit")]
mod winit;
pub mod wm;

use std::fmt;
use std::process::Command;

use neue_config::Layers;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::dmabuf::DmabufState;
use tracing::{info, warn};

pub use headless::{OUTPUT_NAME as HEADLESS_OUTPUT_NAME, OUTPUT_SIZE as HEADLESS_OUTPUT_SIZE};
pub use ipc::IpcSocket;
pub use neue_ipc::WindowInfo;
pub use state::State;

/// Crate name, used by smoke tests and diagnostics.
pub const NAME: &str = "neue-comp";

/// Which backend drives outputs and input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Nested in a window of another Wayland compositor (development).
    Winit,
    /// A virtual output, no rendering and no input devices (tests).
    Headless,
    /// DRM/KMS and libinput on a seat: real hardware, or the VM console.
    Tty,
}

/// Start-up options, parsed from the command line by the binary.
#[derive(Debug, Clone)]
pub struct Options {
    /// Backend to run on.
    pub backend: BackendKind,
    /// Shell commands to spawn once the Wayland socket is up (run through `sh -c`).
    pub commands: Vec<String>,
    /// Listen on a Wayland socket in `XDG_RUNTIME_DIR`. Tests turn this off and connect
    /// their clients with [`State::insert_client`].
    pub listen: bool,
    /// Where the `neuectl` socket goes.
    pub ipc_socket: IpcSocket,
    /// Where `compositor.toml` is read from and watched. [`Layers::none`] means defaults only.
    pub config: Layers,
}

impl Options {
    /// Options for `backend` that listen on a socket and spawn nothing.
    pub fn new(backend: BackendKind) -> Self {
        Self {
            backend,
            commands: Vec::new(),
            listen: true,
            ipc_socket: IpcSocket::Auto,
            config: Layers::standard(),
        }
    }
}

/// Errors that stop the compositor.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A start-up step failed. `context` names the step.
    #[error("{context}")]
    Init {
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The requested backend is not compiled into this binary.
    #[error("backend {0:?} is not compiled in (enable its cargo feature: `winit` or `tty`)")]
    BackendUnavailable(BackendKind),
}

impl Error {
    /// Helper for `map_err`: wraps any error with the name of the failing step.
    pub(crate) fn init<E>(context: &'static str) -> impl FnOnce(E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        move |source| Self::Init {
            context,
            source: source.into(),
        }
    }
}

/// A compositor ready to run: event loop, Wayland display and state.
///
/// Built and run on the same thread (none of it is `Send`). Tests add their own event
/// sources through [`Compositor::loop_handle`] before calling [`Compositor::run`].
pub struct Compositor {
    event_loop: EventLoop<'static, State>,
    display: Display<State>,
    state: State,
}

impl fmt::Debug for Compositor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Compositor")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Compositor {
    /// Set up the backend, the protocol globals and (if requested) the listening socket.
    pub fn new(options: &Options) -> Result<Self, Error> {
        let event_loop: EventLoop<'static, State> =
            EventLoop::try_new().map_err(Error::init("creating the event loop"))?;
        let mut display: Display<State> =
            Display::new().map_err(Error::init("creating the Wayland display"))?;
        let dh = display.handle();

        // Only winit and tty add a global.
        #[cfg_attr(not(any(feature = "winit", feature = "tty")), allow(unused_mut))]
        let mut dmabuf_state = DmabufState::new();
        let backend = match options.backend {
            #[cfg(feature = "winit")]
            BackendKind::Winit => {
                let (backend, source) = winit::init(&dh, &mut dmabuf_state)?;
                event_loop
                    .handle()
                    .insert_source(source, |event, _, state| winit::handle_event(event, state))
                    .map_err(|e| e.error)
                    .map_err(Error::init("adding the winit event source"))?;
                backend
            }
            #[cfg(not(feature = "winit"))]
            BackendKind::Winit => return Err(Error::BackendUnavailable(BackendKind::Winit)),
            #[cfg(feature = "tty")]
            BackendKind::Tty => tty::init(&dh, &mut dmabuf_state, &event_loop.handle())?,
            #[cfg(not(feature = "tty"))]
            BackendKind::Tty => return Err(Error::BackendUnavailable(BackendKind::Tty)),
            BackendKind::Headless => {
                headless::start_frame_timer(&event_loop.handle())?;
                headless::init()
            }
        };

        let mut state = State::new(&event_loop, dh, backend, dmabuf_state, options.listen)?;
        if let Some(path) = ipc_socket_path(options, &state) {
            state.ipc.listen(&path)?;
        }
        state.init_config(options.config.clone());

        // Wake the loop when clients send requests. They are dispatched in the loop callback
        // of `run`, which owns the `Display`; this keeps the display out of `State` and
        // avoids the `unsafe` `NoIoDrop::get_mut` that smallvil uses.
        let poll_fd = display
            .backend()
            .poll_fd()
            .try_clone_to_owned()
            .map_err(Error::init("duplicating the Wayland poll fd"))?;
        event_loop
            .handle()
            .insert_source(
                Generic::new(poll_fd, Interest::READ, Mode::Level),
                |_, _, _| Ok(PostAction::Continue),
            )
            .map_err(|e| e.error)
            .map_err(Error::init("adding the Wayland display source"))?;

        Ok(Self {
            event_loop,
            display,
            state,
        })
    }

    /// Handle for adding event sources to the compositor's loop.
    pub fn loop_handle(&self) -> LoopHandle<'static, State> {
        self.event_loop.handle()
    }

    pub fn state(&mut self) -> &mut State {
        &mut self.state
    }

    /// Run until [`State::loop_signal`] is stopped (Super+Q, the host window closing, or a
    /// test shutting down).
    pub fn run(self) -> Result<(), Error> {
        let Self {
            mut event_loop,
            mut display,
            mut state,
        } = self;
        info!(socket = ?state.socket_name, "neue-comp is running");
        event_loop
            .run(None, &mut state, |state| {
                if let Err(err) = display.dispatch_clients(state) {
                    warn!(%err, "dispatching clients failed");
                }
                state.space.refresh();
                state.popups.cleanup();
                state.refresh_surface_scales();
                if let Err(err) = display.flush_clients() {
                    warn!(%err, "flushing clients failed");
                }
            })
            .map_err(Error::init("running the event loop"))?;
        info!("neue-comp exited");
        Ok(())
    }
}

/// Where to put the `neuectl` socket, if anywhere.
fn ipc_socket_path(options: &Options, state: &State) -> Option<std::path::PathBuf> {
    match &options.ipc_socket {
        IpcSocket::Off => None,
        IpcSocket::Path(path) => Some(path.clone()),
        IpcSocket::Auto => {
            let display = state.socket_name.as_ref()?.to_str()?;
            let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
                warn!("XDG_RUNTIME_DIR is not set: no neuectl socket");
                return None;
            };
            Some(neue_ipc::socket_path_for_display(
                std::path::Path::new(&runtime_dir),
                display,
            ))
        }
    }
}

/// Run the compositor until it is asked to quit.
pub fn run(options: Options) -> Result<(), Error> {
    let compositor = Compositor::new(&options)?;
    for command in &options.commands {
        spawn(command, &compositor.state);
    }
    compositor.run()
}

/// Spawn `command` through `sh -c` as a client of this compositor.
pub fn spawn(command: &str, state: &State) {
    let Some(socket_name) = &state.socket_name else {
        warn!(
            command,
            "not spawning: the compositor has no Wayland socket"
        );
        return;
    };
    let mut process = Command::new("/bin/sh");
    process
        .arg("-c")
        .arg(command)
        .env("WAYLAND_DISPLAY", socket_name)
        // Never let children fall back to an X server.
        .env_remove("DISPLAY");
    match state.ipc.path() {
        Some(path) => process.env(neue_ipc::SOCKET_ENV, path),
        None => process.env_remove(neue_ipc::SOCKET_ENV),
    };
    let result = process.spawn();
    match result {
        Ok(mut child) => {
            info!(command, pid = child.id(), "spawned");
            // Reap the child when it exits so it doesn't linger as a zombie. A blocking
            // wait on its own thread keeps the event loop free.
            let reaper = std::thread::Builder::new()
                .name("neue-comp-reaper".into())
                .spawn(move || child.wait());
            if let Err(err) = reaper {
                warn!(%err, "failed to start the reaper thread");
            }
        }
        Err(err) => warn!(command, %err, "failed to spawn"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matches_crate() {
        assert_eq!(NAME, env!("CARGO_PKG_NAME"));
    }
}
