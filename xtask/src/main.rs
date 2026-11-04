//! Developer tasks for neueOS. Run with `cargo xtask <cmd>`.
//!
//! This is a host-side dev tool, so it prints to stdout/stderr directly instead of using
//! `tracing`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
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
  package <crate> [--arch aarch64|x86_64|all]
                          build the pacman package that ships <crate> in the OrbStack Arch
                          machine(s); default aarch64; output in target/package/
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
        "package" => package(rest),
        "gallery" | "vm" | "iso" => {
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

/// Crates that ship in a pacman package: (crate, directory under `distro/pkgbuilds/`).
/// One PKGBUILD can be a split package, so several crates share a directory.
const PACKAGES: &[(&str, &str)] = &[
    ("neue-comp", "neue-comp"),
    ("neuectl", "neue-comp"),
    ("neue-session", "neue-comp"),
];

/// Target architectures of `cargo xtask package`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arch {
    Aarch64,
    X86_64,
}

impl Arch {
    fn machine(self) -> String {
        match self {
            Arch::Aarch64 => machine("NEUE_ORB_ARM64", ORB_ARM64_DEFAULT),
            Arch::X86_64 => machine("NEUE_ORB_AMD64", ORB_AMD64_DEFAULT),
        }
    }
}

/// Parse `<crate> [--arch aarch64|x86_64|all]` into the PKGBUILD directory and architectures.
fn parse_package_args(args: &[String]) -> Result<(&'static str, Vec<Arch>)> {
    const USAGE: &str = "usage: cargo xtask package <crate> [--arch aarch64|x86_64|all]";
    let mut krate = None;
    let mut arches = vec![Arch::Aarch64];
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--arch" => {
                arches = match it.next().map(String::as_str) {
                    Some("aarch64") => vec![Arch::Aarch64],
                    Some("x86_64") => vec![Arch::X86_64],
                    Some("all") => vec![Arch::Aarch64, Arch::X86_64],
                    _ => bail!("{USAGE}"),
                };
            }
            flag if flag.starts_with('-') => bail!("{USAGE}"),
            name if krate.is_none() => krate = Some(name),
            _ => bail!("{USAGE}"),
        }
    }
    let krate = krate.with_context(|| USAGE)?;
    let dir = PACKAGES
        .iter()
        .find(|(name, _)| *name == krate)
        .map(|(_, dir)| *dir)
        .with_context(|| {
            let known: Vec<_> = PACKAGES.iter().map(|(name, _)| *name).collect();
            format!("no PKGBUILD ships `{krate}` (known: {})", known.join(", "))
        })?;
    Ok((dir, arches))
}

/// Files to put in the source tarball: tracked and untracked-but-not-ignored files that
/// still exist, so uncommitted edits are packaged and deleted files do not break `tar`.
fn source_files(root: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .context("failed to run `git ls-files`")?;
    if !out.status.success() {
        bail!("`git ls-files` failed ({})", out.status);
    }
    let mut files = Vec::new();
    for name in String::from_utf8(out.stdout)?
        .split('\0')
        .filter(|n| !n.is_empty())
    {
        if name.contains('\n') {
            bail!("file name with a newline is not supported: {name:?}");
        }
        if root.join(name).exists() && !files.iter().any(|f| f == name) {
            files.push(name.to_owned());
        }
    }
    Ok(files)
}

/// Build the pacman package(s) for `<crate>` with `makepkg` in the OrbStack Arch machine(s).
///
/// The source tarball is the working tree (see [`source_files`]); `prepare()` in the PKGBUILD
/// fetches the locked dependencies, git ones included. Output: `target/package/<arch>/`.
fn package(args: &[String]) -> Result<()> {
    let (dir, arches) = parse_package_args(args)?;
    let root = workspace_root()?;
    let pkgbuild = root.join("distro/pkgbuilds").join(dir).join("PKGBUILD");
    let stage = root.join("target/package").join(dir);
    fs::create_dir_all(&stage)?;
    fs::copy(&pkgbuild, stage.join("PKGBUILD"))
        .with_context(|| format!("failed to copy {}", pkgbuild.display()))?;

    println!("==> source tarball");
    let list = stage.join("files.txt");
    fs::write(&list, source_files(&root)?.join("\n"))?;
    let mut tar = Command::new("tar");
    // COPYFILE_DISABLE: keep macOS tar from adding `._*` AppleDouble files.
    tar.env("COPYFILE_DISABLE", "1")
        .arg("-czf")
        .arg(stage.join("neueos-src.tar.gz"))
        .arg("-C")
        .arg(&root)
        .arg("-T")
        .arg(&list);
    step("tar", tar)?;

    for arch in arches {
        let machine = arch.machine();
        let out = root
            .join("target/package")
            .join(format!("{arch:?}").to_lowercase());
        fs::create_dir_all(&out)?;
        let cmd: Vec<String> = [
            "env".into(),
            "-C".into(),
            stage.display().to_string(),
            // Outside the source tree and shared between runs, so rebuilds are incremental.
            format!(
                "CARGO_TARGET_DIR={}",
                root.join("target")
                    .join(format!("{machine}-package"))
                    .display()
            ),
            format!(
                "BUILDDIR={}",
                root.join("target")
                    .join(format!("{machine}-pkgbuild"))
                    .display()
            ),
            format!("PKGDEST={}", out.display()),
            "makepkg".into(),
            "--syncdeps".into(),
            "--noconfirm".into(),
            "--force".into(),
            "--cleanbuild".into(),
        ]
        .into();
        linux(&machine, &cmd)?;
        println!("==> packages in {}", out.display());
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
    fn package_args_resolve_crate_and_arch() {
        let (dir, arches) = parse_package_args(&["neuectl".into()]).unwrap();
        assert_eq!(dir, "neue-comp");
        assert_eq!(arches, [Arch::Aarch64]);
        let args = ["neue-session".into(), "--arch".into(), "all".into()];
        assert_eq!(parse_package_args(&args).unwrap().1.len(), 2);
    }

    #[test]
    fn package_args_reject_bad_input() {
        assert!(parse_package_args(&[]).is_err());
        assert!(parse_package_args(&["nope".into()]).is_err());
        assert!(parse_package_args(&["neuectl".into(), "--arch".into()]).is_err());
        assert!(parse_package_args(&["neuectl".into(), "--arch".into(), "ppc".into()]).is_err());
    }

    #[test]
    fn every_package_has_a_pkgbuild() {
        let root = workspace_root().unwrap();
        for (_, dir) in PACKAGES {
            assert!(
                root.join("distro/pkgbuilds")
                    .join(dir)
                    .join("PKGBUILD")
                    .is_file()
            );
        }
    }

    #[test]
    fn source_files_skip_deleted_and_ignored() {
        let files = source_files(&workspace_root().unwrap()).unwrap();
        assert!(files.iter().any(|f| f == "Cargo.toml"));
        assert!(!files.iter().any(|f| f.starts_with("target/")));
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
