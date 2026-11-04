//! X11 apps through `xwayland-satellite`, started on demand (task 1.18, ADR 0026, ADR 0001).
//!
//! The compositor never speaks X11. It reserves an X display number (the lock file and the
//! two listening sockets that Xorg would own) and watches the sockets. The first X client
//! that connects wakes the loop, which spawns `xwayland-satellite :N -listenfd A -listenfd B`
//! and hands it the listening fds; satellite starts Xwayland on them and shows X windows as
//! ordinary Wayland toplevels. Until then nothing X-related runs. When satellite exits, the
//! sockets are watched again, after a short pause if it died young so that a crashing
//! satellite cannot spin the loop. The reservation is released on drop.
//!
//! Reference: niri's `src/utils/xwayland` (GPL-3.0-or-later).

use std::ffi::OsString;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{SocketAddr, UnixListener};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use smithay::reexports::calloop::channel::{self, Event};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{Interest, Mode, PostAction, RegistrationToken};
use tracing::{info, warn};

use crate::state::State;

/// Highest display number tried (`:0` to `:32`), the range Xorg's `-displayfd` scans.
const MAX_DISPLAY: u32 = 32;
/// A satellite that ran at least this long is not a crash loop: watch the sockets at once.
const HEALTHY_RUN: Duration = Duration::from_secs(5);
/// How long the sockets stay unwatched after satellite died young or failed to start.
const COOLDOWN: Duration = Duration::from_secs(1);

/// How X11 apps are supported.
#[derive(Debug, Clone)]
pub struct XwaylandOptions {
    /// The `xwayland-satellite` command: a name looked up in `PATH`, or a path. Needs a
    /// satellite that understands `-listenfd` (0.7 or later).
    pub satellite: PathBuf,
    /// Directory of the X11 unix sockets, `/tmp/.X11-unix` for real. The lock files go in
    /// its parent (`/tmp/.X0-lock`). Tests point it at a temporary directory.
    pub x11_dir: PathBuf,
}

impl Default for XwaylandOptions {
    fn default() -> Self {
        Self {
            satellite: "xwayland-satellite".into(),
            x11_dir: "/tmp/.X11-unix".into(),
        }
    }
}

/// The reserved display: a lock file and the two listening sockets of an X server.
#[derive(Debug)]
struct Sockets {
    display: u32,
    /// The filesystem socket `x11_dir/X<n>`.
    unix: UnixListener,
    /// The abstract socket `@x11_dir/X<n>`, which libxcb tries first.
    abstract_: UnixListener,
    socket_path: PathBuf,
    lock_path: PathBuf,
}

impl Drop for Sockets {
    fn drop(&mut self) {
        // We hold the lock, so both files are ours. The abstract socket goes with its fd.
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.lock_path);
    }
}

impl Sockets {
    /// Reserve the lowest free display number, the way Xorg does.
    fn reserve(x11_dir: &Path) -> io::Result<Self> {
        ensure_x11_dir(x11_dir)?;
        let lock_dir = x11_dir.parent().unwrap_or(Path::new("/"));
        for number in 0..=MAX_DISPLAY {
            let lock_path = lock_dir.join(format!(".X{number}-lock"));
            if !take_lock(&lock_path)? {
                continue;
            }
            match Self::bind(x11_dir, number, lock_path.clone()) {
                Ok(sockets) => return Ok(sockets),
                Err(err) => {
                    // Not ours after all (a socket without a lock file): try the next one.
                    info!(number, %err, "X display is taken");
                    let _ = fs::remove_file(&lock_path);
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("no free X display in :0 to :{MAX_DISPLAY}"),
        ))
    }

    /// Bind both sockets of `display`. The caller holds the lock file.
    fn bind(x11_dir: &Path, display: u32, lock_path: PathBuf) -> io::Result<Self> {
        let socket_path = x11_dir.join(format!("X{display}"));
        // The lock is ours, so a socket file here is left over from a crash.
        match fs::remove_file(&socket_path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
        let unix = UnixListener::bind(&socket_path)?;
        // Same name as the path, with a NUL in front: what libxcb connects to first.
        let name = socket_path.to_string_lossy().into_owned();
        let abstract_ = UnixListener::bind_addr(&SocketAddr::from_abstract_name(name)?)
            .inspect_err(|_| {
                let _ = fs::remove_file(&socket_path);
            })?;
        unix.set_nonblocking(true)?;
        abstract_.set_nonblocking(true)?;
        Ok(Self {
            display,
            unix,
            abstract_,
            socket_path,
            lock_path,
        })
    }
}

/// `x11_dir`, world-writable with the sticky bit like Xorg makes it.
fn ensure_x11_dir(x11_dir: &Path) -> io::Result<()> {
    match DirBuilder::new().mode(0o1777).create(x11_dir) {
        Ok(()) => {
            // `mode` is masked by the umask; X clients of other users need the full 1777.
            fs::set_permissions(x11_dir, fs::Permissions::from_mode(0o1777))
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(err) => Err(err),
    }
}

/// Create the lock file `lock_path` holding our pid. `false` if a live process holds it.
/// A lock whose process is gone is stale and replaced.
fn take_lock(lock_path: &Path) -> io::Result<bool> {
    for _ in 0..2 {
        let created = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o444)
            .open(lock_path);
        match created {
            Ok(mut file) => {
                // Xorg's format: the pid, right-aligned in ten columns.
                writeln!(file, "{:>10}", std::process::id())?;
                return Ok(true);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                let holder = fs::read_to_string(lock_path)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok());
                match holder {
                    Some(pid) if Path::new(&format!("/proc/{pid}")).exists() => return Ok(false),
                    _ => match fs::remove_file(lock_path) {
                        Ok(()) => {}
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                        Err(err) => return Err(err),
                    },
                }
            }
            Err(err) => return Err(err),
        }
    }
    Ok(false)
}

/// Where `satellite` is, if it can be run: a path as given, or a name found in `PATH`.
fn find_command(satellite: &Path) -> Option<PathBuf> {
    if satellite.components().count() > 1 {
        return satellite.is_file().then(|| satellite.to_owned());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(satellite))
        .find(|candidate| candidate.is_file())
}

/// Whether the sockets are watched, and what satellite is doing.
#[derive(Debug)]
enum Phase {
    /// The sockets are watched; nothing runs.
    Armed,
    /// Satellite runs, started at the instant.
    Running(Instant),
    /// Satellite ended (or failed to start) and the sockets are not watched yet.
    Cooling,
}

/// A running satellite ended.
#[derive(Debug)]
struct Exit {
    success: bool,
    description: String,
}

/// The reserved display and the on-demand satellite. Part of [`State`].
#[derive(Debug)]
pub(crate) struct Xwayland {
    sockets: Sockets,
    command: PathBuf,
    /// Our Wayland socket's name, for satellite's `WAYLAND_DISPLAY`.
    wayland_socket: OsString,
    /// One source per socket, with whether it is disabled right now.
    watches: [(RegistrationToken, bool); 2],
    phase: Phase,
    exits: channel::Sender<Exit>,
}

impl Xwayland {
    /// `:N`, what children get in `DISPLAY`.
    pub(crate) fn display(&self) -> String {
        format!(":{}", self.sockets.display)
    }

    /// Spawn satellite with the listening fds, and stop watching the sockets.
    fn start(&mut self) {
        let name = self.display();
        let fds = [&self.sockets.unix, &self.sockets.abstract_].map(rustix::io::dup);
        let [Ok(unix_fd), Ok(abstract_fd)] = fds else {
            warn!("duplicating the X sockets failed: X11 apps are unavailable for now");
            self.phase = Phase::Cooling;
            return;
        };
        // `dup` makes the copies without close-on-exec: the child inherits them under the
        // same numbers, which are what `-listenfd` names.
        let mut command = Command::new(&self.command);
        command
            .arg(&name)
            .arg("-listenfd")
            .arg(unix_fd.as_raw_fd().to_string())
            .arg("-listenfd")
            .arg(abstract_fd.as_raw_fd().to_string())
            .env("WAYLAND_DISPLAY", &self.wayland_socket)
            // Satellite starts its own Xwayland and must not find one through us.
            .env_remove("DISPLAY")
            .stdin(Stdio::null());
        match command.spawn() {
            Ok(mut child) => {
                info!(pid = child.id(), display = %name, "started xwayland-satellite");
                self.phase = Phase::Running(Instant::now());
                let exits = self.exits.clone();
                // A blocking wait on its own thread keeps the event loop free; the channel
                // brings the result back to it.
                let waiter = std::thread::Builder::new()
                    .name("neue-comp-xwayland".into())
                    .spawn(move || {
                        let exit = match child.wait() {
                            Ok(status) => Exit {
                                success: status.success(),
                                description: status.to_string(),
                            },
                            Err(err) => Exit {
                                success: false,
                                description: err.to_string(),
                            },
                        };
                        let _ = exits.send(exit);
                    });
                if let Err(err) = waiter {
                    warn!(%err, "failed to start the xwayland-satellite waiter thread");
                }
            }
            Err(err) => {
                warn!(command = %self.command.display(), %err, "failed to start xwayland-satellite");
                self.phase = Phase::Cooling;
            }
        }
        // `unix_fd` and `abstract_fd` drop here: only the child keeps its copies.
    }
}

impl State {
    /// `:N`, the `DISPLAY` for children, once X11 support is set up.
    pub fn xwayland_display(&self) -> Option<String> {
        self.xwayland.as_ref().map(Xwayland::display)
    }

    /// Set up on-demand X11 support. Needs [`State::socket_name`]. Never fatal: without it the
    /// compositor runs without X11 apps and says why.
    pub fn init_xwayland(&mut self, options: &XwaylandOptions) {
        let Some(wayland_socket) = self.socket_name.clone() else {
            return; // no Wayland socket for satellite to connect to
        };
        let Some(command) = find_command(&options.satellite) else {
            warn!(
                command = %options.satellite.display(),
                "xwayland-satellite not found: X11 apps are unavailable"
            );
            return;
        };
        match self.arm_xwayland(options, command, wayland_socket) {
            Ok(()) => {
                if let Some(name) = self.xwayland_display() {
                    info!(display = %name, "X11 apps start xwayland-satellite on demand");
                }
            }
            Err(err) => warn!(%err, "reserving an X display failed: X11 apps are unavailable"),
        }
    }

    fn arm_xwayland(
        &mut self,
        options: &XwaylandOptions,
        command: PathBuf,
        wayland_socket: OsString,
    ) -> io::Result<()> {
        let sockets = Sockets::reserve(&options.x11_dir)?;
        let handle = self.loop_handle.clone();

        let watch = |index: usize, listener: &UnixListener| -> io::Result<RegistrationToken> {
            let source = Generic::new(listener.try_clone()?, Interest::READ, Mode::Level);
            handle
                .insert_source(source, move |_, _, state: &mut State| {
                    Ok(state.xwayland_connection(index))
                })
                .map_err(|err| io::Error::other(err.to_string()))
        };
        let tokens = [watch(0, &sockets.unix)?, watch(1, &sockets.abstract_)?];

        let (exits, receiver) = channel::channel::<Exit>();
        handle
            .insert_source(receiver, |event, _, state: &mut State| {
                if let Event::Msg(exit) = event {
                    state.xwayland_exited(&exit);
                }
            })
            .map_err(|err| io::Error::other(err.to_string()))?;

        self.xwayland = Some(Xwayland {
            sockets,
            command,
            wayland_socket,
            watches: tokens.map(|token| (token, false)),
            phase: Phase::Armed,
            exits,
        });
        Ok(())
    }

    /// An X client connected to socket `index`: start satellite, unless it is running.
    fn xwayland_connection(&mut self, index: usize) -> PostAction {
        let Some(xwayland) = self.xwayland.as_mut() else {
            return PostAction::Disable;
        };
        // Both sockets are watched, so the second may fire after the first started
        // satellite, and a connection can wait through a cool-down. Either way this watch
        // is off until the next re-arm.
        xwayland.watches[index].1 = true;
        if matches!(xwayland.phase, Phase::Armed) {
            xwayland.start();
            if matches!(xwayland.phase, Phase::Cooling) {
                self.schedule_xwayland_rearm(COOLDOWN);
            }
        }
        PostAction::Disable
    }

    /// Satellite ended: watch the sockets again, after a pause if it died young.
    fn xwayland_exited(&mut self, exit: &Exit) {
        let Some(xwayland) = self.xwayland.as_mut() else {
            return;
        };
        let ran_for = match xwayland.phase {
            Phase::Running(since) => since.elapsed(),
            _ => Duration::ZERO,
        };
        if exit.success {
            info!(status = %exit.description, "xwayland-satellite exited");
        } else {
            warn!(status = %exit.description, "xwayland-satellite exited");
        }
        xwayland.phase = Phase::Cooling;
        let delay = if ran_for >= HEALTHY_RUN {
            Duration::ZERO
        } else {
            COOLDOWN
        };
        self.schedule_xwayland_rearm(delay);
    }

    fn schedule_xwayland_rearm(&mut self, delay: Duration) {
        if delay.is_zero() {
            self.rearm_xwayland();
            return;
        }
        let timer = Timer::from_duration(delay);
        let inserted = self
            .loop_handle
            .insert_source(timer, |_, _, state: &mut State| {
                state.rearm_xwayland();
                TimeoutAction::Drop
            });
        if let Err(err) = inserted {
            warn!(%err, "scheduling the X socket watch failed; re-arming at once");
            self.rearm_xwayland();
        }
    }

    /// Watch the sockets again. A client that connected meanwhile is still queued and
    /// starts satellite right away.
    fn rearm_xwayland(&mut self) {
        let Some(xwayland) = self.xwayland.as_mut() else {
            return;
        };
        xwayland.phase = Phase::Armed;
        for (token, disabled) in &mut xwayland.watches {
            if std::mem::take(disabled)
                && let Err(err) = self.loop_handle.enable(token)
            {
                warn!(%err, "watching an X socket again failed");
            }
        }
    }
}
