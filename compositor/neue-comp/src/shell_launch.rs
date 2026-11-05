//! Starting the shell and restarting it when it exits (task 3.1, ADR 0066).
//!
//! `neue-comp --shell <command>` runs `command` through `sh -c` with one end of a socket
//! pair in `WAYLAND_SOCKET`, and inserts the other end as the shell client
//! ([`State::insert_shell_client`]): the only client that sees `neue_shell_manager_v1`.
//! A waiter thread blocks on the child and reports its exit through a calloop channel;
//! the shell is then started again, at once if it ran for a while, after a growing pause
//! if it keeps dying young.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use smithay::reexports::calloop::channel::{self, Event};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::backend::{ClientId, DisconnectReason};
use tracing::{info, warn};

use crate::state::State;

/// A shell that ran this long before exiting is restarted at once.
const HEALTHY_RUN: Duration = Duration::from_secs(10);
/// The first pause after an early exit; it doubles with each further one.
const FIRST_PAUSE: Duration = Duration::from_millis(250);
/// The longest pause between restarts.
const MAX_PAUSE: Duration = Duration::from_secs(30);

/// The shell command and its running instance. Part of [`State`].
#[derive(Debug)]
pub(crate) struct ShellLauncher {
    command: String,
    exits: channel::Sender<String>,
    /// When the current instance started.
    started: Instant,
    /// Early exits in a row.
    failures: u32,
    /// The running instance's Wayland client.
    client: Option<ClientId>,
}

/// How long to wait before restarting a shell that exited after `ran_for`, having
/// already failed `failures` times in a row (this exit not counted). Returns the pause
/// and the new failure count.
fn restart_pause(ran_for: Duration, failures: u32) -> (Duration, u32) {
    if ran_for >= HEALTHY_RUN {
        return (Duration::ZERO, 0);
    }
    let pause = FIRST_PAUSE
        .saturating_mul(1 << failures.min(10))
        .min(MAX_PAUSE);
    (pause, failures + 1)
}

impl State {
    /// Start `command` as the shell and keep it running. Never fatal.
    pub fn init_shell(&mut self, command: String) {
        let (exits, receiver) = channel::channel::<String>();
        let inserted = self
            .loop_handle
            .insert_source(receiver, |event, _, state: &mut State| {
                if let Event::Msg(status) = event {
                    state.shell_exited(&status);
                }
            });
        if let Err(err) = inserted {
            warn!(err = %err.error, "watching the shell failed: no shell");
            return;
        }
        self.shell_launcher = Some(ShellLauncher {
            command,
            exits,
            started: Instant::now(),
            failures: 0,
            client: None,
        });
        self.start_shell();
    }

    /// Spawn the shell on a fresh socket pair.
    fn start_shell(&mut self) {
        let Some(command) = self.shell_launcher.as_ref().map(|l| l.command.clone()) else {
            return;
        };
        let (ours, theirs) = match UnixStream::pair() {
            Ok(pair) => pair,
            Err(err) => {
                warn!(%err, "creating the shell's socket failed");
                self.schedule_shell_restart(MAX_PAUSE);
                return;
            }
        };
        let client = match self.insert_shell_client(ours) {
            Ok(client) => client,
            Err(err) => {
                warn!(%err, "adding the shell client failed");
                self.schedule_shell_restart(MAX_PAUSE);
                return;
            }
        };
        // `dup` makes a copy without close-on-exec, so the child inherits it under the
        // number `WAYLAND_SOCKET` names. Our copies drop at the end of this function.
        let fd = match rustix::io::dup(&theirs) {
            Ok(fd) => fd,
            Err(err) => {
                warn!(%err, "duplicating the shell's socket failed");
                self.kill_shell_client(client.id());
                self.schedule_shell_restart(MAX_PAUSE);
                return;
            }
        };
        let mut process = crate::client_command(&command, self);
        process.env("WAYLAND_SOCKET", fd.as_raw_fd().to_string());
        match process.spawn() {
            Ok(mut child) => {
                info!(command, pid = child.id(), "started the shell");
                let Some(launcher) = self.shell_launcher.as_mut() else {
                    return;
                };
                launcher.started = Instant::now();
                launcher.client = Some(client.id());
                let exits = launcher.exits.clone();
                // A blocking wait on its own thread keeps the event loop free.
                let waiter = std::thread::Builder::new()
                    .name("neue-comp-shell".into())
                    .spawn(move || {
                        let status = match child.wait() {
                            Ok(status) => status.to_string(),
                            Err(err) => err.to_string(),
                        };
                        let _ = exits.send(status);
                    });
                if let Err(err) = waiter {
                    warn!(%err, "failed to start the shell's waiter thread: no restarts");
                }
            }
            Err(err) => {
                warn!(command, %err, "failed to start the shell");
                self.kill_shell_client(client.id());
                self.shell_exited(&err.to_string());
            }
        }
    }

    /// The shell process ended: drop its client (a child it left behind may still hold
    /// the socket) and start it again.
    fn shell_exited(&mut self, status: &str) {
        let Some(launcher) = self.shell_launcher.as_mut() else {
            return;
        };
        let client = launcher.client.take();
        let (pause, failures) = restart_pause(launcher.started.elapsed(), launcher.failures);
        launcher.failures = failures;
        warn!(status, ?pause, "the shell exited; restarting it");
        if let Some(client) = client {
            self.kill_shell_client(client);
        }
        self.schedule_shell_restart(pause);
    }

    fn kill_shell_client(&mut self, client: ClientId) {
        self.display_handle
            .backend_handle()
            .kill_client(client, DisconnectReason::ConnectionClosed);
    }

    fn schedule_shell_restart(&mut self, pause: Duration) {
        if pause.is_zero() {
            self.start_shell();
            return;
        }
        let inserted = self.loop_handle.insert_source(
            Timer::from_duration(pause),
            |_, _, state: &mut State| {
                state.start_shell();
                TimeoutAction::Drop
            },
        );
        if let Err(err) = inserted {
            warn!(%err, "scheduling the shell restart failed; restarting at once");
            self.start_shell();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_shell_restarts_at_once_and_resets_the_count() {
        assert_eq!(restart_pause(HEALTHY_RUN, 5), (Duration::ZERO, 0));
    }

    #[test]
    fn early_exits_back_off_up_to_the_cap() {
        let quick = Duration::from_millis(10);
        assert_eq!(restart_pause(quick, 0), (FIRST_PAUSE, 1));
        assert_eq!(restart_pause(quick, 1), (FIRST_PAUSE * 2, 2));
        assert_eq!(restart_pause(quick, 3), (FIRST_PAUSE * 8, 4));
        assert_eq!(restart_pause(quick, 40), (MAX_PAUSE, 41));
    }
}
