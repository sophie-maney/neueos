//! Starting an entry, or one of its actions, as a detached process.

use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use thiserror::Error;

use crate::entry::{DesktopEntry, EntryType, ExecError};

/// What to start and how.
#[derive(Clone, Copy, Debug, Default)]
pub struct LaunchOptions<'a> {
    /// An action's id (`new-window`) instead of the entry's own `Exec`.
    pub action: Option<&'a str>,
    /// Files or URLs to open (the `%f %F %u %U` codes).
    pub targets: &'a [String],
    /// The `xdg-activation-v1` token the compositor gave the shell for this launch. The app
    /// finds it in `XDG_ACTIVATION_TOKEN` and uses it to take focus when its window opens.
    pub activation_token: Option<&'a str>,
    /// The command that runs another one in a terminal window (`["foot", "-e"]`), for
    /// entries with `Terminal=true`. Without it those entries cannot be launched.
    pub terminal: Option<&'a [String]>,
}

/// Why an entry was not started.
#[derive(Debug, Error)]
pub enum LaunchError {
    /// The entry is a link or a directory, not an application.
    #[error("not an application")]
    NotAnApplication,
    /// The command line is missing or malformed.
    #[error(transparent)]
    Exec(#[from] ExecError),
    /// `Terminal=true` and no terminal command was given.
    #[error("the entry needs a terminal and none was given")]
    NoTerminal,
    /// The only way to start the app is D-Bus activation, which is not supported yet.
    #[error("the entry can only be started through D-Bus activation")]
    DBusOnly,
    /// The process could not be started.
    #[error("cannot start {program}: {source}")]
    Spawn {
        /// The program.
        program: String,
        /// What went wrong.
        source: std::io::Error,
    },
}

/// The command [`launch`] would run, not started yet: the program and arguments, the working
/// directory (`Path`, when it exists) and the environment (`XDG_ACTIVATION_TOKEN`,
/// `DESKTOP_STARTUP_ID`, `GIO_LAUNCHED_DESKTOP_FILE`).
pub fn command(entry: &DesktopEntry, options: &LaunchOptions<'_>) -> Result<Command, LaunchError> {
    if entry.kind != EntryType::Application {
        return Err(LaunchError::NotAnApplication);
    }
    let own = match options.action {
        Some(id) => entry.action(id).and_then(|a| a.exec.as_ref()),
        None => entry.exec.as_ref(),
    };
    if own.is_none() && options.action.is_none() && entry.dbus_activatable {
        return Err(LaunchError::DBusOnly);
    }
    let mut argv = entry.exec_argv(options.action, options.targets)?;
    // A terminal app starts through the terminal; the action of a terminal app too.
    if entry.terminal {
        let terminal = options.terminal.filter(|t| !t.is_empty());
        let mut full = terminal.ok_or(LaunchError::NoTerminal)?.to_vec();
        full.append(&mut argv);
        argv = full;
    }
    let mut parts = argv.into_iter();
    let program = parts.next().ok_or(ExecError::Malformed)?;
    let mut command = Command::new(program);
    command
        .args(parts)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = entry.working_dir.as_deref().filter(|d| Path::is_dir(d)) {
        command.current_dir(dir);
    }
    if let Some(token) = options.activation_token.filter(|t| !t.is_empty()) {
        command.env("XDG_ACTIVATION_TOKEN", token);
        // Older toolkits read the X11 startup id; Wayland ones accept the same value.
        command.env("DESKTOP_STARTUP_ID", token);
    } else {
        command.env_remove("XDG_ACTIVATION_TOKEN");
        command.env_remove("DESKTOP_STARTUP_ID");
    }
    if !entry.path.as_os_str().is_empty() {
        command.env("GIO_LAUNCHED_DESKTOP_FILE", &entry.path);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: the shell's signals (Ctrl+C in a terminal, a restart) don't
        // reach it, and it outlives the shell.
        command.process_group(0);
    }
    Ok(command)
}

/// Starts the entry and returns the process id. The process is detached: a background
/// thread waits for it so it never stays a zombie.
pub fn launch(entry: &DesktopEntry, options: &LaunchOptions<'_>) -> Result<u32, LaunchError> {
    let mut command = command(entry, options)?;
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .spawn()
        .map_err(|source| LaunchError::Spawn { program, source })?;
    let pid = child.id();
    thread::spawn(move || {
        // Only reaping: the exit status is nobody's business.
        let _ = child.wait();
    });
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;
    use crate::keyfile::Locale;

    fn entry(extra: &str) -> DesktopEntry {
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=T\nExec=prog --x %f\nActions=w;\n{extra}\n\
             [Desktop Action w]\nName=W\nExec=prog --window\n"
        );
        DesktopEntry::parse("t.desktop", &text, &Locale::none()).unwrap()
    }

    fn env<'a>(c: &'a Command, key: &str) -> Option<Option<&'a OsStr>> {
        c.get_envs().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    #[test]
    fn the_command_carries_arguments_token_and_directory() {
        let dir = std::env::temp_dir();
        let e = entry(&format!("Path={}", dir.display()));
        let targets = ["/tmp/f".to_owned()];
        let c = command(
            &e,
            &LaunchOptions {
                targets: &targets,
                activation_token: Some("tok"),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(c.get_program(), "prog");
        assert_eq!(c.get_args().collect::<Vec<_>>(), ["--x", "/tmp/f"]);
        assert_eq!(c.get_current_dir(), Some(dir.as_path()));
        assert_eq!(env(&c, "XDG_ACTIVATION_TOKEN"), Some(Some("tok".as_ref())));
        assert_eq!(env(&c, "DESKTOP_STARTUP_ID"), Some(Some("tok".as_ref())));
    }

    #[test]
    fn no_token_clears_an_inherited_one_and_actions_use_their_exec() {
        let e = entry("");
        let c = command(
            &e,
            &LaunchOptions {
                action: Some("w"),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(c.get_args().collect::<Vec<_>>(), ["--window"]);
        // `None` means "removed" in `get_envs`.
        assert_eq!(env(&c, "XDG_ACTIVATION_TOKEN"), Some(None));
        assert!(
            command(
                &e,
                &LaunchOptions {
                    action: Some("nope"),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn terminal_apps_need_a_terminal_and_dbus_only_apps_are_refused() {
        let e = entry("Terminal=true");
        assert!(matches!(
            command(&e, &LaunchOptions::default()),
            Err(LaunchError::NoTerminal)
        ));
        let term = ["term".to_owned(), "-e".to_owned()];
        let c = command(
            &e,
            &LaunchOptions {
                terminal: Some(&term),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(c.get_program(), "term");
        assert_eq!(c.get_args().collect::<Vec<_>>(), ["-e", "prog", "--x"]);

        let d = DesktopEntry::parse(
            "d.desktop",
            "[Desktop Entry]\nType=Application\nName=D\nDBusActivatable=true\n",
            &Locale::none(),
        )
        .unwrap();
        assert!(matches!(
            command(&d, &LaunchOptions::default()),
            Err(LaunchError::DBusOnly)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn launch_starts_a_process() {
        let e = DesktopEntry::parse(
            "t.desktop",
            "[Desktop Entry]\nType=Application\nName=T\nExec=true\n",
            &Locale::none(),
        )
        .unwrap();
        assert!(launch(&e, &LaunchOptions::default()).unwrap() > 0);
        let missing = DesktopEntry::parse(
            "m.desktop",
            "[Desktop Entry]\nType=Application\nName=M\nExec=/nonexistent/neue-prog\n",
            &Locale::none(),
        )
        .unwrap();
        assert!(matches!(
            launch(&missing, &LaunchOptions::default()),
            Err(LaunchError::Spawn { .. })
        ));
    }
}
