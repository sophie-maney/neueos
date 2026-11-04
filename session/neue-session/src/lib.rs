//! `neue-session`: starts the neueOS user session under systemd and waits for it to end
//! (task 1.19, ADR 0027).
//!
//! greetd (through `wayland-sessions/neueos.desktop`) runs `neue-session` as the user. It
//! hands the login environment to the systemd user manager, starts `neue-session.target`
//! (which pulls in `neue-comp.service` and `graphical-session.target`) and blocks until the
//! target stops, which is logout. The compositor adds `WAYLAND_DISPLAY` and `DISPLAY` to the
//! manager itself once it is listening (`neue-comp --notify`).
//!
//! Everything goes through the [`Manager`] trait, so the sequence is tested without systemd.

use std::io;
use std::process::{Command, ExitStatus};

use tracing::{info, warn};

/// The target that stands for "the neueOS session is running".
pub const TARGET: &str = "neue-session.target";

/// Login variables worth giving to every unit: the seat and logind session (libseat and
/// portals need them) and the locale. `LC_*` is added by [`login_variables`].
const LOGIN_VARIABLES: &[&str] = &[
    "XDG_SEAT",
    "XDG_SESSION_ID",
    "XDG_SESSION_CLASS",
    "XDG_VTNR",
    "LANG",
    "LANGUAGE",
];

/// Variables set for the whole session. `XDG_CURRENT_DESKTOP` is what portals, GTK and Qt
/// use to pick their backends and configs.
const SESSION_VARIABLES: &[(&str, &str)] = &[
    ("XDG_SESSION_TYPE", "wayland"),
    ("XDG_CURRENT_DESKTOP", "neueOS"),
    ("XDG_SESSION_DESKTOP", "neueOS"),
];

/// Left over from an earlier session in a user manager that outlived it (lingering). The
/// compositor sets the real ones once it runs.
const STALE_VARIABLES: &[&str] = &["WAYLAND_DISPLAY", "DISPLAY", "NEUE_SOCKET"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot run systemctl: {0}")]
    Spawn(#[from] io::Error),
    #[error("{0} is already active: another neueOS session is running for this user")]
    AlreadyRunning(&'static str),
    #[error("systemctl --user {args} failed ({status}); see `journalctl --user -u neue-comp`")]
    Failed { args: String, status: ExitStatus },
}

/// The systemd user manager. [`Systemctl`] is the real one.
pub trait Manager {
    /// Run `systemctl --user <args>`.
    fn run(&mut self, args: &[String]) -> io::Result<ExitStatus>;
}

/// `systemctl --user`, inheriting our stdio so its errors reach the console or journal.
#[derive(Debug)]
pub struct Systemctl;

impl Manager for Systemctl {
    fn run(&mut self, args: &[String]) -> io::Result<ExitStatus> {
        Command::new("systemctl").arg("--user").args(args).status()
    }
}

/// The login variables to forward, picked from `env` (the process environment).
pub fn login_variables(env: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut vars: Vec<_> = env
        .into_iter()
        .filter(|(name, _)| LOGIN_VARIABLES.contains(&name.as_str()) || name.starts_with("LC_"))
        .collect();
    vars.sort();
    vars
}

/// Everything `set-environment` gets: the login variables, then ours (ours win).
pub fn session_variables(env: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut vars = login_variables(env);
    for (name, value) in SESSION_VARIABLES {
        vars.retain(|(existing, _)| existing != name);
        vars.push(((*name).to_owned(), (*value).to_owned()));
    }
    vars
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

fn check(manager: &mut impl Manager, args: Vec<String>) -> Result<(), Error> {
    let status = manager.run(&args)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Failed {
            args: args.join(" "),
            status,
        })
    }
}

/// Start the session and block until it ends. The environment is always cleaned up after,
/// even when the start failed.
pub fn run(
    manager: &mut impl Manager,
    env: impl IntoIterator<Item = (String, String)>,
) -> Result<(), Error> {
    if manager
        .run(&strings(&["is-active", "--quiet", TARGET]))?
        .success()
    {
        return Err(Error::AlreadyRunning(TARGET));
    }

    let vars = session_variables(env);
    let mut unset = strings(&["unset-environment"]);
    unset.extend(STALE_VARIABLES.iter().map(|name| (*name).to_owned()));
    check(manager, unset)?;
    let mut set = strings(&["set-environment"]);
    set.extend(vars.iter().map(|(name, value)| format!("{name}={value}")));
    check(manager, set)?;

    info!("starting {TARGET}");
    let result = check(manager, strings(&["start", "--wait", TARGET]));

    // Forget the session in a user manager that outlives it. Best effort.
    let mut cleanup = strings(&["unset-environment"]);
    cleanup.extend(STALE_VARIABLES.iter().map(|name| (*name).to_owned()));
    cleanup.extend(vars.into_iter().map(|(name, _)| name));
    if let Err(err) = check(manager, cleanup) {
        warn!(%err, "cleaning up the environment failed");
    }
    info!("session ended");
    result
}

/// Stop the session (logout): the compositor exits, then `neue-session` returns.
pub fn stop(manager: &mut impl Manager) -> Result<(), Error> {
    check(manager, strings(&["stop", TARGET]))
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;

    use super::*;

    /// Records the commands; `fail` makes the ones starting with that word exit 1, and
    /// `active` makes `is-active` succeed.
    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        fail: Option<&'static str>,
        active: bool,
    }

    impl Manager for Fake {
        fn run(&mut self, args: &[String]) -> io::Result<ExitStatus> {
            self.calls.push(args.join(" "));
            let failed =
                self.fail == Some(args[0].as_str()) || (args[0] == "is-active" && !self.active);
            Ok(ExitStatus::from_raw(if failed { 1 << 8 } else { 0 }))
        }
    }

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn picks_login_variables_only() {
        let vars = login_variables(env(&[
            ("XDG_SEAT", "seat0"),
            ("XDG_VTNR", "1"),
            ("LC_TIME", "en_GB.UTF-8"),
            ("LANG", "en_US.UTF-8"),
            ("PATH", "/usr/bin"),
            ("SECRET_TOKEN", "x"),
            ("DISPLAY", ":7"),
        ]));
        let names: Vec<_> = vars.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["LANG", "LC_TIME", "XDG_SEAT", "XDG_VTNR"]);
    }

    #[test]
    fn our_variables_win_over_the_login_ones() {
        let vars = session_variables(env(&[("XDG_SESSION_TYPE", "tty"), ("LANG", "C")]));
        assert!(vars.contains(&("XDG_SESSION_TYPE".into(), "wayland".into())));
        assert!(!vars.contains(&("XDG_SESSION_TYPE".into(), "tty".into())));
        assert!(vars.contains(&("XDG_CURRENT_DESKTOP".into(), "neueOS".into())));
        assert!(vars.contains(&("LANG".into(), "C".into())));
    }

    #[test]
    fn starts_the_target_and_cleans_up() {
        let mut fake = Fake::default();
        run(&mut fake, env(&[("XDG_SEAT", "seat0")])).unwrap();
        assert_eq!(fake.calls[0], "is-active --quiet neue-session.target");
        assert_eq!(
            fake.calls[1],
            "unset-environment WAYLAND_DISPLAY DISPLAY NEUE_SOCKET"
        );
        assert!(fake.calls[2].starts_with("set-environment XDG_SEAT=seat0 "));
        assert_eq!(fake.calls[3], "start --wait neue-session.target");
        assert!(
            fake.calls[4]
                .starts_with("unset-environment WAYLAND_DISPLAY DISPLAY NEUE_SOCKET XDG_SEAT")
        );
        assert_eq!(fake.calls.len(), 5);
    }

    #[test]
    fn refuses_a_second_session() {
        let mut fake = Fake {
            active: true,
            ..Fake::default()
        };
        let err = run(&mut fake, env(&[])).unwrap_err();
        assert!(matches!(err, Error::AlreadyRunning(_)));
        assert_eq!(fake.calls.len(), 1, "must not touch the environment");
    }

    #[test]
    fn a_failed_start_still_cleans_up_and_reports() {
        let mut fake = Fake {
            fail: Some("start"),
            ..Fake::default()
        };
        let err = run(&mut fake, env(&[])).unwrap_err();
        assert!(matches!(err, Error::Failed { .. }));
        assert!(fake.calls.last().unwrap().starts_with("unset-environment"));
    }

    #[test]
    fn stop_stops_the_target() {
        let mut fake = Fake::default();
        stop(&mut fake).unwrap();
        assert_eq!(fake.calls, ["stop neue-session.target"]);
    }
}
