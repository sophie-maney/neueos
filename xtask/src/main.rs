//! Developer tasks for neueOS. Run with `cargo xtask <cmd>`.
//!
//! This is a host-side dev tool, so it prints to stdout/stderr directly instead of using
//! `tracing`.

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

/// Workspace crates that only build on Linux. They are excluded from `check` on other
/// hosts. Keep this list in sync with the platform matrix in `AGENTS.md`.
const LINUX_ONLY_CRATES: &[&str] = &["neue-comp"];

/// OrbStack machine names. Override with `NEUE_ORB_ARM64` / `NEUE_ORB_AMD64`.
/// The real names are fixed in task 0.4 (`docs/dev-setup.md`).
const ORB_ARM64_DEFAULT: &str = "neueos-arm64";
const ORB_AMD64_DEFAULT: &str = "neueos-amd64";

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  check                   fmt --check + clippy -D warnings + tests for this host
  linux <cmd...>          run <cmd> in the OrbStack Arch machine (arm64)
  linux-amd64 <cmd...>    run <cmd> in the OrbStack Arch machine (amd64, Rosetta)
  gallery                 run the widget gallery (not implemented yet)
  golden [--bless]        check or regenerate golden images
  vm sync|run             sync to the Parallels VM / start a session (not implemented yet)
  package <crate>         build a pacman package (not implemented yet)
  iso [--arch <arch>]     build the install ISO (not implemented yet)
";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<()> {
    let Some((cmd, rest)) = args.split_first() else {
        print!("{USAGE}");
        return Ok(());
    };
    match cmd.as_str() {
        "check" => check(),
        "linux" => linux(&machine("NEUE_ORB_ARM64", ORB_ARM64_DEFAULT), rest),
        "linux-amd64" => linux(&machine("NEUE_ORB_AMD64", ORB_AMD64_DEFAULT), rest),
        "golden" => golden(rest),
        "gallery" | "vm" | "package" | "iso" => {
            println!("xtask {cmd}: not implemented yet");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        other => bail!("unknown command `{other}`\n\n{USAGE}"),
    }
}

/// Workspace root: the parent of this crate's manifest directory.
fn workspace_root() -> Result<PathBuf> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .parent()
        .map(PathBuf::from)
        .context("xtask manifest directory has no parent")
}

fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Run a command from the workspace root, failing with a readable message on non-zero exit.
fn step(label: &str, mut cmd: Command) -> Result<()> {
    println!("==> {label}");
    let status = cmd
        .current_dir(workspace_root()?)
        .status()
        .with_context(|| format!("failed to start `{label}`"))?;
    if !status.success() {
        bail!("`{label}` failed ({status})");
    }
    Ok(())
}

/// `--exclude` arguments for Linux-only crates when not running on Linux.
fn excludes() -> Vec<String> {
    if cfg!(target_os = "linux") {
        return Vec::new();
    }
    LINUX_ONLY_CRATES
        .iter()
        .flat_map(|name| ["--exclude".to_owned(), (*name).to_owned()])
        .collect()
}

fn check() -> Result<()> {
    let excl = excludes();

    let mut fmt = cargo();
    fmt.args(["fmt", "--all", "--check"]);
    step("cargo fmt --check", fmt)?;

    let mut clippy = cargo();
    // --all-features so feature-gated code (e.g. neue-paint's GPU backend and its
    // example) is linted too.
    clippy.args(["clippy", "--workspace", "--all-targets", "--all-features"]);
    clippy.args(&excl);
    clippy.args(["--", "-D", "warnings"]);
    step("cargo clippy", clippy)?;

    let mut test = cargo();
    test.args(["test", "--workspace"]);
    test.args(&excl);
    step("cargo test", test)?;

    println!("==> all checks passed");
    Ok(())
}

/// Crates with golden-image tests, run by `cargo xtask golden`.
const GOLDEN_CRATES: &[&str] = &["neue-paint", "neue-comp"];

/// Run the golden-image tests. With `--bless`, rewrite the goldens instead of comparing.
fn golden(args: &[String]) -> Result<()> {
    let bless = match args {
        [] => false,
        [flag] if flag == "--bless" => true,
        _ => bail!("usage: cargo xtask golden [--bless]"),
    };
    for krate in GOLDEN_CRATES {
        let mut test = cargo();
        test.args(["test", "-p", krate, "--test", "golden"]);
        if bless {
            test.env("NEUE_BLESS", "1");
        }
        step(&format!("golden tests ({krate})"), test)?;
    }
    if bless {
        println!(
            "==> goldens rewritten; review `git diff --stat` and the images before committing"
        );
    }
    Ok(())
}

fn machine(var: &str, default: &str) -> String {
    env::var(var).unwrap_or_else(|_| default.to_owned())
}

/// Run `cmd` inside an OrbStack machine. OrbStack mounts the Mac filesystem at the same
/// path, so the repo is used in place. Linux builds go to a separate target directory so
/// they don't clobber the macOS one.
fn linux(machine: &str, cmd: &[String]) -> Result<()> {
    if cmd.is_empty() {
        bail!("missing command, e.g. `cargo xtask linux cargo check`");
    }
    let root = workspace_root()?;
    let mut orb = Command::new("orb");
    orb.args(["run", "-m", machine, "-w"])
        .arg(&root)
        .arg("env")
        .arg(format!("CARGO_TARGET_DIR=target/{machine}"))
        .args(cmd);
    step(&format!("orb run -m {machine} {}", cmd.join(" ")), orb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_is_an_error() {
        assert!(run(vec!["bogus".into()]).is_err());
    }

    #[test]
    fn stub_commands_succeed() {
        assert!(run(vec!["gallery".into()]).is_ok());
    }

    #[test]
    fn golden_rejects_unknown_flags() {
        assert!(golden(&["--nope".into()]).is_err());
    }

    #[test]
    fn linux_requires_a_command() {
        assert!(linux("m", &[]).is_err());
    }

    #[test]
    fn excludes_pair_each_crate_off_linux() {
        if !cfg!(target_os = "linux") {
            assert_eq!(excludes().len(), LINUX_ONLY_CRATES.len() * 2);
        }
    }
}
