//! Running as a systemd user service (task 1.19, ADR 0027): handing `WAYLAND_DISPLAY` and
//! `DISPLAY` to the user manager, and `READY=1`.
//!
//! The compositor picks its own socket and X display, so nothing else can know them before
//! it runs. With `--notify` it tells the user manager once everything is listening, before
//! it says `READY=1`, so units ordered after `neue-comp.service` see the right environment.

use std::ffi::OsStr;
use std::io;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::{SocketAddr, UnixDatagram};
use std::process::Command;

use tracing::{info, warn};

use crate::state::State;

/// The `NAME=value` pairs for the user manager: where clients connect.
pub fn environment(state: &State) -> Vec<String> {
    let mut vars = Vec::new();
    if let Some(name) = state.socket_name.as_ref().and_then(|name| name.to_str()) {
        vars.push(format!("WAYLAND_DISPLAY={name}"));
    }
    if let Some(display) = state.xwayland_display() {
        vars.push(format!("DISPLAY={display}"));
    }
    vars
}

/// Import the environment into the systemd user manager, then send `READY=1`. Failures are
/// logged, never fatal: the compositor still works, the rest of the session does not.
pub fn announce(state: &State) {
    let vars = environment(state);
    let result = Command::new("systemctl")
        .args(["--user", "set-environment"])
        .args(&vars)
        .status();
    match result {
        Ok(status) if status.success() => info!(?vars, "imported into the user manager"),
        Ok(status) => warn!(%status, "systemctl set-environment failed"),
        Err(err) => warn!(%err, "cannot run systemctl: the session will not see the sockets"),
    }
    match std::env::var_os("NOTIFY_SOCKET") {
        Some(path) => match notify(&path, "READY=1\nSTATUS=Running\n") {
            Ok(()) => info!("told the service manager we are ready"),
            Err(err) => warn!(%err, "sd_notify failed"),
        },
        None => warn!("--notify without NOTIFY_SOCKET: run under a Type=notify unit"),
    }
}

/// Send `message` to a systemd notify socket: a path, or `@name` for an abstract socket.
/// The protocol is one datagram, so this needs no library.
pub fn notify(socket: &OsStr, message: &str) -> io::Result<()> {
    let bytes = socket.as_bytes();
    let socket_out = UnixDatagram::unbound()?;
    match bytes.strip_prefix(b"@") {
        Some(name) => {
            socket_out.send_to_addr(message.as_bytes(), &SocketAddr::from_abstract_name(name)?)?
        }
        None => socket_out.send_to(message.as_bytes(), socket)?,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_reaches_a_path_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notify");
        let server = UnixDatagram::bind(&path).unwrap();
        notify(path.as_os_str(), "READY=1\n").unwrap();
        let mut buf = [0u8; 32];
        let len = server.recv(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"READY=1\n");
    }

    #[test]
    fn notify_reaches_an_abstract_socket() {
        let name = format!("neue-comp-test-{}", std::process::id());
        let server =
            UnixDatagram::bind_addr(&SocketAddr::from_abstract_name(name.as_bytes()).unwrap())
                .unwrap();
        notify(OsStr::new(&format!("@{name}")), "READY=1\n").unwrap();
        let mut buf = [0u8; 32];
        let len = server.recv(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"READY=1\n");
    }

    #[test]
    fn notify_without_a_listener_errors() {
        assert!(notify(OsStr::new("/nonexistent/notify"), "READY=1\n").is_err());
    }
}
