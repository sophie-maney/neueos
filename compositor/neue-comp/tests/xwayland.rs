//! On-demand `xwayland-satellite` (task 1.18), with a shell script standing in for it.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::fs;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use harness::Server;
use neue_comp::XwaylandOptions;

/// Tests run one at a time: writing an executable while another thread forks makes `exec`
/// fail with "text file busy".
static SERIAL: Mutex<()> = Mutex::new(());

struct Fixture {
    dir: tempfile::TempDir,
    server: Server,
    x11_dir: PathBuf,
    satellite: PathBuf,
}

impl Fixture {
    fn runs(&self) -> Vec<String> {
        fs::read_to_string(self.dir.path().join("runs"))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    fn wait_for_runs(&self, count: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let runs = self.runs();
            if runs.len() >= count {
                return runs;
            }
            assert!(Instant::now() < deadline, "satellite ran {runs:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn init(&self) {
        let options = XwaylandOptions {
            satellite: self.satellite.clone(),
            x11_dir: self.x11_dir.clone(),
        };
        self.server.with_state(move |state| {
            state.socket_name = Some("wayland-test".into());
            state.init_xwayland(&options);
        });
    }

    fn display(&self) -> Option<String> {
        self.server.with_state(|state| state.xwayland_display())
    }

    fn connect_unix(&self, display: &str) -> UnixStream {
        UnixStream::connect(self.x11_dir.join(format!("X{}", &display[1..]))).unwrap()
    }
}

/// A satellite that records its arguments, whether its `-listenfd`s are open, and its
/// environment, then runs until `quit` appears (`exit_code` is `None`) or exits at once.
fn fixture(exit_code: Option<u8>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().display();
    let finish = match exit_code {
        Some(code) => format!("exit {code}"),
        None => format!("while [ ! -e {path}/quit ]; do sleep 0.02; done"),
    };
    let script = format!(
        "#!/bin/sh\n\
         fds=''\n\
         while [ $# -gt 0 ]; do\n\
           if [ \"$1\" = -listenfd ]; then\n\
             [ -S \"/proc/self/fd/$2\" ] && fds=\"$fds open:$2\" || fds=\"$fds closed:$2\"\n\
             shift\n\
           else first=\"${{first:-$1}}\"; fi\n\
           shift\n\
         done\n\
         echo \"$first wayland=$WAYLAND_DISPLAY display=${{DISPLAY-unset}}$fds\" >> {path}/runs\n\
         {finish}\n"
    );
    let satellite = dir.path().join("satellite");
    fs::write(&satellite, script).unwrap();
    fs::set_permissions(&satellite, fs::Permissions::from_mode(0o755)).unwrap();
    let x11_dir = dir.path().join(".X11-unix");
    Fixture {
        server: Server::start(),
        x11_dir,
        satellite,
        dir,
    }
}

#[test]
fn nothing_runs_until_an_x_client_connects() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fx = fixture(None);
    fx.init();
    let display = fx.display().expect("display reserved");
    assert_eq!(display, ":0");

    // Reserved like Xorg does: lock file with our pid, a filesystem and an abstract socket.
    let lock = fs::read_to_string(fx.dir.path().join(".X0-lock")).unwrap();
    assert_eq!(lock, format!("{:>10}\n", std::process::id()));
    assert!(fx.x11_dir.join("X0").exists());
    std::thread::sleep(Duration::from_millis(200));
    assert!(fx.runs().is_empty(), "satellite started without a client");

    let _client = fx.connect_unix(&display);
    let runs = fx.wait_for_runs(1);
    // Display name, our Wayland socket, no inherited DISPLAY, both listening fds open.
    assert_eq!(runs.len(), 1);
    let line = &runs[0];
    assert!(
        line.starts_with(":0 wayland=wayland-test display=unset "),
        "{line}"
    );
    assert_eq!(line.matches(" open:").count(), 2, "{line}");

    // More clients (on either socket) do not start a second satellite while it runs.
    let _second = fx.connect_unix(&display);
    let abstract_name = fx.x11_dir.join("X0").display().to_string();
    let _third =
        UnixStream::connect_addr(&SocketAddr::from_abstract_name(abstract_name).unwrap()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(fx.runs().len(), 1);

    fs::write(fx.dir.path().join("quit"), "").unwrap();
    drop(fx.server);
    assert!(!fx.x11_dir.join("X0").exists(), "socket left behind");
    assert!(!fx.dir.path().join(".X0-lock").exists(), "lock left behind");
}

#[test]
fn a_satellite_that_exits_is_started_again_for_the_next_client() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fx = fixture(Some(1));
    fx.init();
    let display = fx.display().unwrap();
    let _client = fx.connect_unix(&display);
    // The fake never accepts, so the connection stays queued: after the pause the sockets
    // are watched again and wake at once.
    let started = Instant::now();
    fx.wait_for_runs(2);
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "a satellite that died young was restarted without a pause"
    );
}

#[test]
fn without_satellite_there_is_no_display() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut fx = fixture(None);
    fx.satellite = fx.dir.path().join("missing");
    fx.init();
    assert_eq!(fx.display(), None);
    assert!(
        !fx.x11_dir.exists(),
        "reserved a display with nothing to serve it"
    );
}

#[test]
fn stale_locks_are_replaced_and_live_ones_respected() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let fx = fixture(None);
    fs::create_dir_all(&fx.x11_dir).unwrap();
    // :0 belongs to a process that is gone, :1 to this one.
    lock(fx.dir.path(), 0, 4_000_000_000);
    lock(fx.dir.path(), 1, std::process::id());
    fx.init();
    assert_eq!(fx.display().as_deref(), Some(":0"));
    let lock = fs::read_to_string(fx.dir.path().join(".X0-lock")).unwrap();
    assert_eq!(lock.trim(), std::process::id().to_string());
}

fn lock(dir: &Path, number: u32, pid: u32) {
    fs::write(dir.join(format!(".X{number}-lock")), format!("{pid:>10}\n")).unwrap();
}
