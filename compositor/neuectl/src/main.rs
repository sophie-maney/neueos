//! `neuectl`: query and control the running neueOS compositor over its IPC socket.
//!
//! Output is human-readable by default; `--json` prints the raw reply payload (one JSON
//! document, or one line per event for `subscribe`), for scripts and agents.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use neue_ipc::{
    ActionInfo, ActionParams, Client, OutputInfo, Topic, VersionInfo, WindowArg, WindowId,
    WindowInfo,
};

#[derive(Debug, Parser)]
#[command(name = "neuectl", version, about = "Control the neueOS compositor")]
struct Cli {
    /// Socket path (default: $NEUE_SOCKET, else derived from $XDG_RUNTIME_DIR and
    /// $WAYLAND_DISPLAY).
    #[arg(long, global = true, value_name = "PATH")]
    socket: Option<PathBuf>,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compositor version and protocol version.
    Version,
    /// List windows, bottom to top.
    Windows,
    /// List outputs.
    Outputs,
    /// Show the focused window.
    FocusedWindow,
    /// List the actions `neuectl action` accepts.
    Actions,
    /// Run a named action.
    Action {
        /// Action name (see `neuectl actions`).
        name: String,
        /// Target window id (default: the focused window, for actions that take one).
        #[arg(long, value_name = "ID")]
        window: Option<u64>,
        /// New x position (`move-window`), in global logical pixels.
        #[arg(long, allow_negative_numbers = true)]
        x: Option<i32>,
        /// New y position (`move-window`).
        #[arg(long, allow_negative_numbers = true)]
        y: Option<i32>,
        /// New width (`resize-window`), in logical pixels.
        #[arg(long)]
        width: Option<i32>,
        /// New height (`resize-window`).
        #[arg(long)]
        height: Option<i32>,
    },
    /// Re-read the config files now; fails with file, line and column if they are invalid.
    ReloadConfig,
    /// Stream window and config events until interrupted.
    #[command(alias = "events")]
    Subscribe,
    /// Stop the compositor.
    Quit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut client = match &cli.socket {
        Some(path) => Client::connect(path),
        None => Client::connect_default(),
    }
    .context("cannot reach the compositor (is neue-comp running?)")?;

    let mut out = std::io::stdout().lock();
    match cli.command {
        Command::Version => {
            let info = client.version()?;
            emit(&mut out, cli.json, &info, format_version)?;
        }
        Command::Windows => {
            let windows = client.windows()?;
            emit(&mut out, cli.json, &windows, |w| format_windows(w))?;
        }
        Command::Outputs => {
            let outputs = client.outputs()?;
            emit(&mut out, cli.json, &outputs, |o| format_outputs(o))?;
        }
        Command::FocusedWindow => {
            let window = client.focused_window()?;
            emit(&mut out, cli.json, &window, |w| match w {
                Some(w) => format_windows(std::slice::from_ref(w)),
                None => "no focused window\n".to_owned(),
            })?;
        }
        Command::Actions => {
            let actions = client.actions()?;
            emit(&mut out, cli.json, &actions, |a| format_actions(a))?;
        }
        Command::Action {
            name,
            window,
            x,
            y,
            width,
            height,
        } => client.action_with(ActionParams {
            name,
            window: window.map(WindowId),
            x,
            y,
            width,
            height,
        })?,
        Command::ReloadConfig => client.reload_config()?,
        Command::Quit => client.quit()?,
        Command::Subscribe => {
            for event in client.subscribe(vec![Topic::Windows, Topic::Config, Topic::Outputs])? {
                let event = event?;
                // Events are one JSON line each in both modes: stable and greppable.
                writeln!(out, "{}", serde_json::to_string(&event)?)?;
                out.flush()?;
            }
        }
    }
    Ok(())
}

/// Print `value` as pretty JSON, or as text via `text`.
fn emit<T: serde::Serialize + ?Sized>(
    out: &mut impl Write,
    json: bool,
    value: &T,
    text: impl FnOnce(&T) -> String,
) -> Result<()> {
    if json {
        writeln!(out, "{}", serde_json::to_string_pretty(value)?)?;
    } else {
        write!(out, "{}", text(value))?;
    }
    Ok(())
}

fn format_version(info: &VersionInfo) -> String {
    format!(
        "{} {} (protocol {})\n",
        info.compositor, info.version, info.protocol
    )
}

fn format_windows(windows: &[WindowInfo]) -> String {
    if windows.is_empty() {
        return "no windows\n".to_owned();
    }
    let mut text = String::new();
    for w in windows {
        text.push_str(&format!(
            "{}{:>4}  {}x{}+{}+{}  {}  {}{}\n",
            if w.focused { '*' } else { ' ' },
            w.id,
            w.width,
            w.height,
            w.x,
            w.y,
            w.app_id.as_deref().unwrap_or("-"),
            w.title.as_deref().unwrap_or(""),
            window_flags(w),
        ));
    }
    text
}

/// ` [minimized] [zoomed] …` for the states a window is in, or nothing.
fn window_flags(w: &WindowInfo) -> String {
    let mut flags = String::new();
    for (on, name) in [
        (w.minimized, "minimized"),
        (w.zoomed, "zoomed"),
        (w.fullscreen, "fullscreen"),
        (w.demands_attention, "attention"),
    ] {
        if on {
            flags.push_str(&format!("  [{name}]"));
        }
    }
    if let Some(parent) = w.parent {
        flags.push_str(&format!("  [child of {parent}]"));
    }
    flags
}

fn format_outputs(outputs: &[OutputInfo]) -> String {
    let mut text = String::new();
    for o in outputs {
        text.push_str(&format!(
            "{}  {}x{}@{:.2}Hz  +{}+{}  scale {}  {} {}\n",
            o.name,
            o.width,
            o.height,
            f64::from(o.refresh_mhz) / 1000.0,
            o.x,
            o.y,
            o.scale,
            o.make,
            o.model,
        ));
    }
    text
}

fn format_actions(actions: &[ActionInfo]) -> String {
    let width = actions.iter().map(|a| a.name.len()).max().unwrap_or(0);
    let mut text = String::new();
    for a in actions {
        let window = match a.window {
            WindowArg::None => "",
            WindowArg::Optional => " [--window ID]",
            WindowArg::Required => " --window ID",
        };
        let args: String = a.args.iter().map(|arg| format!(" --{arg} N")).collect();
        text.push_str(&format!(
            "{:width$}  {}{window}{args}\n",
            a.name, a.description
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    #[test]
    fn windows_table_marks_focus() {
        let window = |id, focused| WindowInfo {
            id: WindowId(id),
            title: Some("Term".into()),
            app_id: Some("foot".into()),
            x: 1,
            y: 2,
            width: 30,
            height: 40,
            focused,
            minimized: id == 1,
            zoomed: false,
            fullscreen: false,
            demands_attention: false,
            parent: None,
        };
        let text = format_windows(&[window(1, false), window(2, true)]);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with(' '));
        assert!(lines[1].starts_with('*'));
        assert!(lines[0].contains("[minimized]"));
        assert!(lines[1].contains("30x40+1+2  foot  Term"));
    }

    #[test]
    fn parses_action_with_window() {
        let cli = Cli::try_parse_from(["neuectl", "action", "close-window", "--window", "7"])
            .expect("valid command line");
        assert!(matches!(
            cli.command,
            Command::Action {
                window: Some(7),
                ..
            }
        ));
    }

    #[test]
    fn parses_move_with_negative_position() {
        let cli =
            Cli::try_parse_from(["neuectl", "action", "move-window", "--x", "-20", "--y", "5"])
                .expect("valid command line");
        assert!(matches!(
            cli.command,
            Command::Action {
                x: Some(-20),
                y: Some(5),
                ..
            }
        ));
    }
}
