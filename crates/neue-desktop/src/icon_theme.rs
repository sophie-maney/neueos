//! Icon theme lookup (the Icon Theme Specification): by name, size and scale, through a
//! theme and the themes it inherits, then the fallback theme (Papirus) and `hicolor`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::keyfile::KeyFile;

/// The file formats that are looked up, best first. (XPM is not loaded.)
const FORMATS: [(&str, IconFormat); 2] = [("png", IconFormat::Png), ("svg", IconFormat::Svg)];

/// Inheritance can't loop; this stops a theme that does.
const MAX_THEMES: usize = 16;

/// The format of an icon file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconFormat {
    /// A PNG image.
    Png,
    /// An SVG image.
    Svg,
}

/// An icon file found by [`IconLookup::find`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconFile {
    /// Where it is.
    pub path: PathBuf,
    /// What it is.
    pub format: IconFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirKind {
    Fixed,
    Scalable,
    Threshold,
}

#[derive(Clone, Debug)]
struct ThemeDir {
    /// Relative to the theme directory (`48x48/apps`).
    path: String,
    size: i64,
    scale: i64,
    kind: DirKind,
    min: i64,
    max: i64,
    threshold: i64,
}

impl ThemeDir {
    fn matches(&self, size: i64, scale: i64) -> bool {
        if self.scale != scale {
            return false;
        }
        match self.kind {
            DirKind::Fixed => self.size == size,
            DirKind::Scalable => self.min <= size && size <= self.max,
            DirKind::Threshold => {
                self.size - self.threshold <= size && size <= self.size + self.threshold
            }
        }
    }

    /// How far this directory's sizes are from `size` at `scale` (in device pixels).
    fn distance(&self, size: i64, scale: i64) -> i64 {
        let want = size * scale;
        let (lo, hi) = match self.kind {
            DirKind::Fixed => (self.size * self.scale, self.size * self.scale),
            DirKind::Scalable => (self.min * self.scale, self.max * self.scale),
            DirKind::Threshold => (
                (self.size - self.threshold) * self.scale,
                (self.size + self.threshold) * self.scale,
            ),
        };
        if want < lo {
            lo - want
        } else if want > hi {
            want - hi
        } else {
            0
        }
    }
}

#[derive(Debug)]
struct Theme {
    name: String,
    /// Where each directory of the theme might be: `<root>/<name>`.
    bases: Vec<PathBuf>,
    dirs: Vec<ThemeDir>,
}

/// Finds icon files by name.
///
/// The search order is the theme asked for, its `Inherits` (depth first), the fallback
/// theme and its `Inherits`, `hicolor`, and last the `pixmaps` directories.
#[derive(Debug)]
pub struct IconLookup {
    themes: Vec<Theme>,
    pixmaps: Vec<PathBuf>,
}

impl IconLookup {
    /// `roots` are the `icons` directories in priority order (`~/.local/share/icons`, …);
    /// `theme` is the theme the user chose and `fallback` the one tried after it (neueOS
    /// ships Papirus). `pixmaps` are directories of loose icons, searched last.
    pub fn new(
        roots: &[PathBuf],
        theme: &str,
        fallback: Option<&str>,
        pixmaps: &[PathBuf],
    ) -> Self {
        let mut names: Vec<String> = Vec::new();
        for start in [Some(theme), fallback, Some("hicolor")]
            .into_iter()
            .flatten()
        {
            add_with_parents(roots, start, &mut names);
        }
        let themes = names.iter().map(|n| load_theme(roots, n)).collect();
        Self {
            themes,
            pixmaps: pixmaps.to_vec(),
        }
    }

    /// The system's lookup: `icons` under [`crate::all_data_dirs`] (and `~/.icons`), and
    /// `/usr/share/pixmaps`.
    pub fn system(theme: &str, fallback: Option<&str>) -> Self {
        let mut roots: Vec<PathBuf> = Vec::new();
        roots.extend(std::env::home_dir().map(|h| h.join(".icons")));
        roots.extend(crate::all_data_dirs().into_iter().map(|d| d.join("icons")));
        Self::new(
            &roots,
            theme,
            fallback,
            &[PathBuf::from("/usr/share/pixmaps")],
        )
    }

    /// The theme names searched, in order.
    pub fn theme_names(&self) -> Vec<&str> {
        self.themes.iter().map(|t| t.name.as_str()).collect()
    }

    /// Finds the icon named `name` (or the file at the absolute path `name`) for `size`
    /// logical pixels at `scale` (a whole number, 1 or more). `name` may carry an extension
    /// that is a known format (`Icon=foo.png` is seen in the wild).
    pub fn find(&self, name: &str, size: u32, scale: u32) -> Option<IconFile> {
        if name.is_empty() {
            return None;
        }
        if name.starts_with('/') {
            let path = PathBuf::from(name);
            let format = format_of(&path)?;
            return path.is_file().then_some(IconFile { path, format });
        }
        let name = strip_known_extension(name);
        let (size, scale) = (i64::from(size), i64::from(scale.max(1)));
        for theme in &self.themes {
            if let Some(found) = theme.find(name, size, scale) {
                return Some(found);
            }
        }
        for dir in &self.pixmaps {
            for (ext, format) in FORMATS {
                let path = dir.join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(IconFile { path, format });
                }
            }
        }
        None
    }
}

impl Theme {
    fn find(&self, name: &str, size: i64, scale: i64) -> Option<IconFile> {
        // A directory that matches exactly, in the order the theme lists them.
        for dir in self.dirs.iter().filter(|d| d.matches(size, scale)) {
            if let Some(found) = self.file_in(dir, name) {
                return Some(found);
            }
        }
        // Otherwise the closest one; the first listed wins a tie.
        let mut best: Option<(i64, IconFile)> = None;
        for dir in &self.dirs {
            let distance = dir.distance(size, scale);
            if best.as_ref().is_some_and(|(d, _)| *d <= distance) {
                continue;
            }
            if let Some(found) = self.file_in(dir, name) {
                best = Some((distance, found));
            }
        }
        best.map(|(_, f)| f)
    }

    fn file_in(&self, dir: &ThemeDir, name: &str) -> Option<IconFile> {
        for base in &self.bases {
            for (ext, format) in FORMATS {
                let path = base.join(&dir.path).join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(IconFile { path, format });
                }
            }
        }
        None
    }
}

fn format_of(path: &Path) -> Option<IconFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    FORMATS.iter().find(|(e, _)| *e == ext).map(|(_, f)| *f)
}

fn strip_known_extension(name: &str) -> &str {
    for (ext, _) in FORMATS {
        if let Some(stem) = name.strip_suffix(&format!(".{ext}")) {
            return stem;
        }
    }
    name
}

/// Adds `name` and, after it, the themes it inherits (recursively), skipping any already
/// there.
fn add_with_parents(roots: &[PathBuf], name: &str, names: &mut Vec<String>) {
    if names.len() >= MAX_THEMES || names.iter().any(|n| n == name) {
        return;
    }
    names.push(name.to_owned());
    for parent in inherits(roots, name) {
        add_with_parents(roots, &parent, names);
    }
}

fn index_files(roots: &[PathBuf], name: &str) -> Vec<(PathBuf, KeyFile)> {
    roots
        .iter()
        .map(|r| r.join(name))
        .filter_map(|base| {
            let text = fs::read_to_string(base.join("index.theme")).ok()?;
            Some((base, KeyFile::parse(&text)))
        })
        .collect()
}

fn inherits(roots: &[PathBuf], name: &str) -> Vec<String> {
    // The first index.theme in priority order is the theme's.
    index_files(roots, name)
        .into_iter()
        .next()
        .and_then(|(_, file)| file.group("Icon Theme").map(|g| g.comma_list("Inherits")))
        .unwrap_or_default()
}

fn load_theme(roots: &[PathBuf], name: &str) -> Theme {
    let indexes = index_files(roots, name);
    let bases = roots
        .iter()
        .map(|r| r.join(name))
        .filter(|b| b.is_dir())
        .collect();
    let mut dirs = Vec::new();
    if let Some((_, file)) = indexes.first()
        && let Some(main) = file.group("Icon Theme")
    {
        let mut listed = main.comma_list("Directories");
        listed.extend(main.comma_list("ScaledDirectories"));
        let mut seen = HashSet::new();
        for path in listed {
            if !seen.insert(path.clone()) {
                continue;
            }
            let Some(group) = file.group(&path) else {
                continue;
            };
            let Some(size) = group.int("Size").filter(|s| *s > 0) else {
                continue;
            };
            let kind = match group.raw("Type").map(str::trim) {
                Some("Fixed") => DirKind::Fixed,
                Some("Scalable") => DirKind::Scalable,
                _ => DirKind::Threshold,
            };
            dirs.push(ThemeDir {
                path,
                size,
                scale: group.int("Scale").filter(|s| *s > 0).unwrap_or(1),
                kind,
                min: group.int("MinSize").unwrap_or(size),
                max: group.int("MaxSize").unwrap_or(size),
                threshold: group.int("Threshold").unwrap_or(2),
            });
        }
    }
    Theme {
        name: name.to_owned(),
        bases,
        dirs,
    }
}
