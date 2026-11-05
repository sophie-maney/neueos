//! GTK's `settings.ini` (`$XDG_CONFIG_HOME/gtk-3.0/` and `gtk-4.0/`), derived from
//! `appearance.toml` (ADR 0074 §4). Only the keys below are ours; other lines in the file
//! are kept.

use std::path::{Path, PathBuf};

use neue_config::AppearanceConfig;
use neue_config::appearance::Mode;

/// The GTK versions whose `settings.ini` is written.
const DIRS: [&str; 2] = ["gtk-3.0", "gtk-4.0"];

/// The `[Settings]` keys for `appearance`, in file order.
pub fn settings(appearance: &AppearanceConfig) -> Vec<(&'static str, String)> {
    let fonts = &appearance.fonts;
    vec![
        (
            "gtk-application-prefer-dark-theme",
            (appearance.mode == Mode::Dark).to_string(),
        ),
        ("gtk-font-name", format!("{} {}", fonts.ui, fonts.ui_size)),
        ("gtk-cursor-theme-name", appearance.cursor.theme.clone()),
        ("gtk-cursor-theme-size", appearance.cursor.size.to_string()),
        (
            "gtk-enable-animations",
            (!appearance.reduced_motion).to_string(),
        ),
    ]
}

/// `text` (a `settings.ini`, possibly empty) with `pairs` set in its `[Settings]` group:
/// existing lines for these keys are replaced (repeats dropped), missing ones are added at
/// the end of the group, and everything else stays as it was.
pub fn edit_ini(text: &str, pairs: &[(&str, String)]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut written = vec![false; pairs.len()];
    let mut in_settings = false;
    let mut saw_settings = false;
    // Where the [Settings] group's last non-blank line is, to add missing keys after it.
    let mut group_end = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_settings = trimmed == "[Settings]";
            saw_settings |= in_settings;
            out.push(line.to_owned());
            if in_settings {
                group_end = Some(out.len());
            }
            continue;
        }
        if in_settings {
            let name = trimmed.split('=').next().unwrap_or("").trim();
            if let Some(i) = pairs.iter().position(|(k, _)| *k == name) {
                if !written[i] {
                    written[i] = true;
                    out.push(format!("{}={}", pairs[i].0, pairs[i].1));
                    group_end = Some(out.len());
                }
                continue;
            }
            if !trimmed.is_empty() {
                group_end = Some(out.len() + 1);
            }
        }
        out.push(line.to_owned());
    }

    let missing = pairs
        .iter()
        .zip(&written)
        .filter(|(_, done)| !**done)
        .map(|((k, v), _)| format!("{k}={v}"));
    if saw_settings {
        let at = group_end.unwrap_or(out.len());
        out.splice(at..at, missing);
    } else {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[Settings]".to_owned());
        out.extend(missing);
    }
    let mut result = out.join("\n");
    result.push('\n');
    result
}

/// The `settings.ini` files under `config_home` (normally `$XDG_CONFIG_HOME`).
pub fn files(config_home: &Path) -> Vec<PathBuf> {
    DIRS.iter()
        .map(|d| config_home.join(d).join("settings.ini"))
        .collect()
}

/// Write `appearance` into every `settings.ini` under `config_home`. Files that would not
/// change are left alone.
pub fn write(config_home: &Path, appearance: &AppearanceConfig) -> std::io::Result<()> {
    let pairs = settings(appearance);
    for path in files(config_home) {
        let old = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err),
        };
        let new = edit_ini(&old, &pairs);
        if new != old {
            crate::store::write_atomically(&path, new.as_bytes())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs() -> Vec<(&'static str, String)> {
        vec![("a", "1".to_owned()), ("b", "2".to_owned())]
    }

    #[test]
    fn an_empty_file_gets_a_group() {
        assert_eq!(edit_ini("", &pairs()), "[Settings]\na=1\nb=2\n");
    }

    #[test]
    fn other_lines_survive_and_ours_are_replaced() {
        let text = "# hi\n[Settings]\nb = 9\nmine=x\nb=8\n\n[Other]\na=5\n";
        assert_eq!(
            edit_ini(text, &pairs()),
            "# hi\n[Settings]\nb=2\nmine=x\na=1\n\n[Other]\na=5\n"
        );
    }

    #[test]
    fn a_file_without_our_group_gets_one_at_the_end() {
        assert_eq!(
            edit_ini("[Other]\nx=1", &pairs()),
            "[Other]\nx=1\n\n[Settings]\na=1\nb=2\n"
        );
    }

    #[test]
    fn editing_twice_is_stable() {
        let once = edit_ini("[Settings]\nz=0\n", &pairs());
        assert_eq!(edit_ini(&once, &pairs()), once);
    }

    #[test]
    fn dark_mode_and_fonts() {
        let mut appearance = AppearanceConfig {
            mode: Mode::Dark,
            ..AppearanceConfig::default()
        };
        appearance.fonts.ui_size = 11.5;
        let pairs = settings(&appearance);
        assert!(pairs.contains(&("gtk-application-prefer-dark-theme", "true".into())));
        assert!(pairs.contains(&("gtk-font-name", "DejaVu Sans 11.5".into())));
        assert!(pairs.contains(&("gtk-enable-animations", "true".into())));
    }

    #[test]
    fn write_creates_both_files() {
        let home = tempfile::tempdir().unwrap();
        write(home.path(), &AppearanceConfig::default()).unwrap();
        for path in files(home.path()) {
            let text = std::fs::read_to_string(path).unwrap();
            assert!(
                text.contains("gtk-application-prefer-dark-theme=false"),
                "{text}"
            );
        }
    }
}
