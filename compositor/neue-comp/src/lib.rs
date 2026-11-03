//! neueOS Wayland compositor.
//!
//! Linux-only: the whole crate is gated, so on other hosts it compiles to an empty
//! library and stays a workspace member.
//!
//! Structure (task 1.1, the skeleton):
//! - [`state`]: the Smithay state shared by every backend, and the event loop setup.
//! - `handlers`: Wayland protocol handlers (compositor, shm, dmabuf, seat, output, xdg-shell).
//! - `input`: input dispatch, focus on click, compositor key bindings.
//! - [`placement`]: where new windows go (a cascading stack).
//! - `winit`: the nested development backend (cargo feature `winit`).
//!
//! The skeleton follows Smithay's `smallvil` and `anvil` examples (MIT); see the comments in
//! each module for what was taken from where.
#![cfg(target_os = "linux")]

// Until the tty (1.4) and headless (1.2) backends land, winit is the only backend.
#[cfg(not(feature = "winit"))]
compile_error!("neue-comp needs a backend feature: enable `winit`");

mod handlers;
mod input;
pub mod placement;
pub mod state;
#[cfg(feature = "winit")]
mod winit;

use std::process::Command;

use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::dmabuf::DmabufState;
use tracing::{info, warn};

pub use state::State;

/// Crate name, used by smoke tests and diagnostics.
pub const NAME: &str = "neue-comp";

/// Which backend drives outputs and input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Nested in a window of another Wayland compositor (development).
    Winit,
}

/// Start-up options, parsed from the command line by the binary.
#[derive(Debug, Clone)]
pub struct Options {
    /// Backend to run on.
    pub backend: BackendKind,
    /// Shell commands to spawn once the Wayland socket is up (run through `sh -c`).
    pub commands: Vec<String>,
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
    #[error("backend {0:?} is not compiled in (enable the `winit` cargo feature)")]
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

/// Run the compositor until it is asked to quit.
pub fn run(options: Options) -> Result<(), Error> {
    let mut event_loop: EventLoop<'static, State> =
        EventLoop::try_new().map_err(Error::init("creating the event loop"))?;
    let mut display: Display<State> =
        Display::new().map_err(Error::init("creating the Wayland display"))?;
    let dh = display.handle();

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
        kind => return Err(Error::BackendUnavailable(kind)),
    };

    let mut state = State::new(&event_loop, dh, backend, dmabuf_state)?;

    // Wake the loop when clients send requests. They are dispatched in the loop callback
    // below, which owns the `Display`; this keeps the display out of `State` and avoids the
    // `unsafe` `NoIoDrop::get_mut` that smallvil uses.
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

    info!(socket = ?state.socket_name, "neue-comp is running");
    for command in &options.commands {
        spawn(command, &state);
    }

    event_loop
        .run(None, &mut state, |state| {
            if let Err(err) = display.dispatch_clients(state) {
                warn!(%err, "dispatching clients failed");
            }
            state.space.refresh();
            state.popups.cleanup();
            if let Err(err) = display.flush_clients() {
                warn!(%err, "flushing clients failed");
            }
        })
        .map_err(Error::init("running the event loop"))?;

    info!("neue-comp exited");
    Ok(())
}

/// Spawn `command` through `sh -c` as a client of this compositor.
pub fn spawn(command: &str, state: &State) {
    let result = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("WAYLAND_DISPLAY", &state.socket_name)
        // Never let children fall back to an X server.
        .env_remove("DISPLAY")
        .spawn();
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
