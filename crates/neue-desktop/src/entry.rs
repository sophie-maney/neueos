//! Desktop entries (the Desktop Entry Specification): parsing, translated names, actions,
//! visibility, and the `Exec` line with its field codes.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::keyfile::{KeyFile, Locale};

/// A desktop entry larger than this is refused; real ones are a few kilobytes.
const MAX_SIZE: u64 = 1 << 20;

/// Why an entry could not be read.
#[derive(Debug, Error)]
pub enum EntryError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// The file is over the 1 MiB limit.
    #[error("{0} is too large for a desktop entry")]
    TooLarge(PathBuf),
    /// There is no `[Desktop Entry]` group.
    #[error("no [Desktop Entry] group")]
    NoGroup,
    /// `Type` is missing or unknown.
    #[error("missing or unknown Type")]
    BadType,
    /// A required key (`Name`, or `Exec` for an application that is not D-Bus activatable)
    /// is missing.
    #[error("missing {0}")]
    Missing(&'static str),
}

/// The `Type` of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryType {
    /// A program.
    Application,
    /// A link to a URL (`URL=`).
    Link,
    /// A menu directory.
    Directory,
}

/// An entry in `Actions` (`[Desktop Action id]`): an extra thing the app can start, such as
/// a new window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    /// The action's identifier (`new-window`).
    pub id: String,
    /// Its translated name.
    pub name: String,
    /// Its icon name or path, if it has one of its own.
    pub icon: Option<String>,
    /// Its command line.
    pub exec: Option<String>,
}

/// A desktop entry, with the strings translated for the locale it was parsed with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopEntry {
    /// The desktop file ID: the path under `applications/` with `/` turned into `-`
    /// (`org.gnome.Nautilus.desktop`, `kde-konsole.desktop`).
    pub id: String,
    /// Where it was read from (empty for [`DesktopEntry::parse`] without a file).
    pub path: PathBuf,
    /// `Type`.
    pub kind: EntryType,
    /// `Name`.
    pub name: String,
    /// `GenericName`.
    pub generic_name: Option<String>,
    /// `Comment`.
    pub comment: Option<String>,
    /// `Icon`: a theme name or an absolute path.
    pub icon: Option<String>,
    /// `Exec`, raw (see [`DesktopEntry::exec_argv`]).
    pub exec: Option<String>,
    /// `TryExec`: the program that must exist for the entry to count as installed.
    pub try_exec: Option<String>,
    /// `Path`: the working directory.
    pub working_dir: Option<PathBuf>,
    /// `Terminal`: runs in a terminal window.
    pub terminal: bool,
    /// `NoDisplay`: not shown in menus, but still a valid entry for app ids and MIME.
    pub no_display: bool,
    /// `Hidden`: the entry is deleted (it masks a lower-priority entry of the same id).
    pub hidden: bool,
    /// `OnlyShowIn`.
    pub only_show_in: Vec<String>,
    /// `NotShowIn`.
    pub not_show_in: Vec<String>,
    /// `StartupWMClass`: the window class or app id its windows have.
    pub startup_wm_class: Option<String>,
    /// `X-Flatpak`: the Flatpak application id, on entries Flatpak exports.
    pub flatpak_id: Option<String>,
    /// `StartupNotify`.
    pub startup_notify: bool,
    /// `DBusActivatable`.
    pub dbus_activatable: bool,
    /// `Categories`.
    pub categories: Vec<String>,
    /// `Keywords`, translated.
    pub keywords: Vec<String>,
    /// `MimeType`.
    pub mime_types: Vec<String>,
    /// The actions named by `Actions`, in that order (ones without a group are dropped).
    pub actions: Vec<Action>,
}

impl DesktopEntry {
    /// Reads the entry at `path`; `id` is its desktop file ID.
    pub fn from_file(id: &str, path: &Path, locale: &Locale) -> Result<Self, EntryError> {
        let io = |source| EntryError::Io {
            path: path.to_owned(),
            source,
        };
        if fs::metadata(path).map_err(io)?.len() > MAX_SIZE {
            return Err(EntryError::TooLarge(path.to_owned()));
        }
        let bytes = fs::read(path).map_err(io)?;
        let mut entry = Self::parse(id, &String::from_utf8_lossy(&bytes), locale)?;
        entry.path = path.to_owned();
        Ok(entry)
    }

    /// Parses entry text.
    pub fn parse(id: &str, text: &str, locale: &Locale) -> Result<Self, EntryError> {
        let file = KeyFile::parse(text);
        let main = file.group("Desktop Entry").ok_or(EntryError::NoGroup)?;
        let kind = match main.raw("Type").map(str::trim) {
            Some("Application") => EntryType::Application,
            Some("Link") => EntryType::Link,
            Some("Directory") => EntryType::Directory,
            _ => return Err(EntryError::BadType),
        };
        let name = main
            .localized("Name", locale)
            .ok_or(EntryError::Missing("Name"))?;
        let dbus_activatable = main.boolean("DBusActivatable").unwrap_or(false);
        let exec = main.string("Exec");
        if kind == EntryType::Application && exec.is_none() && !dbus_activatable {
            return Err(EntryError::Missing("Exec"));
        }
        let actions = main
            .list("Actions")
            .into_iter()
            .filter_map(|action_id| {
                let group = file.group(&format!("Desktop Action {action_id}"))?;
                Some(Action {
                    name: group.localized("Name", locale)?,
                    icon: group.string("Icon"),
                    exec: group.string("Exec"),
                    id: action_id,
                })
            })
            .collect();
        Ok(Self {
            id: id.to_owned(),
            path: PathBuf::new(),
            kind,
            name,
            generic_name: main.localized("GenericName", locale),
            comment: main.localized("Comment", locale),
            icon: main.string("Icon"),
            exec,
            try_exec: main.string("TryExec"),
            working_dir: main.string("Path").map(PathBuf::from),
            terminal: main.boolean("Terminal").unwrap_or(false),
            no_display: main.boolean("NoDisplay").unwrap_or(false),
            hidden: main.boolean("Hidden").unwrap_or(false),
            only_show_in: main.list("OnlyShowIn"),
            not_show_in: main.list("NotShowIn"),
            startup_wm_class: main.string("StartupWMClass"),
            flatpak_id: main.string("X-Flatpak"),
            startup_notify: main.boolean("StartupNotify").unwrap_or(false),
            dbus_activatable,
            categories: main.list("Categories"),
            keywords: main.localized_list("Keywords", locale),
            mime_types: main.list("MimeType"),
            actions,
        })
    }

    /// Whether the entry may show in a desktop named by `current` (`XDG_CURRENT_DESKTOP`
    /// split at `:`; neueOS is `["neueOS"]`): `OnlyShowIn` must name one of them and
    /// `NotShowIn` none.
    pub fn show_in(&self, current: &[&str]) -> bool {
        let names = |list: &[String]| list.iter().any(|n| current.contains(&n.as_str()));
        (self.only_show_in.is_empty() || names(&self.only_show_in)) && !names(&self.not_show_in)
    }

    /// Whether `TryExec` (if there is one) names a program that exists: an absolute path
    /// that is a file, or a name found in `$PATH`.
    pub fn try_exec_found(&self) -> bool {
        match &self.try_exec {
            None => true,
            Some(program) => which(program),
        }
    }

    /// Whether a menu, the launcher or the Applications list shows the entry: an
    /// application that is not `Hidden` or `NoDisplay`, shows in `current`, and whose
    /// `TryExec` exists.
    pub fn is_listed(&self, current: &[&str]) -> bool {
        self.kind == EntryType::Application
            && !self.hidden
            && !self.no_display
            && self.show_in(current)
            && self.try_exec_found()
    }

    /// The action with this id.
    pub fn action(&self, id: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.id == id)
    }

    /// The action that opens a new window (`new-window`, which Files, browsers and
    /// terminals offer), if there is one: the dock's "New Window" item.
    pub fn new_window_action(&self) -> Option<&Action> {
        self.action("new-window")
    }

    /// The command line of the entry (`action` `None`) or of one of its actions, with field
    /// codes expanded for `targets` (paths or URLs).
    pub fn exec_argv(
        &self,
        action: Option<&str>,
        targets: &[String],
    ) -> Result<Vec<String>, ExecError> {
        let (exec, icon) = match action {
            None => (self.exec.as_deref(), self.icon.as_deref()),
            Some(id) => {
                let action = self.action(id).ok_or(ExecError::NoSuchAction)?;
                (
                    action.exec.as_deref(),
                    action.icon.as_deref().or(self.icon.as_deref()),
                )
            }
        };
        let exec = exec.ok_or(ExecError::NoExec)?;
        expand_exec(
            exec,
            &FieldCodes {
                targets,
                icon,
                name: &self.name,
                path: &self.path,
            },
        )
    }
}

/// Why an `Exec` line could not be turned into a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ExecError {
    /// The entry (or action) has no `Exec`.
    #[error("no Exec line")]
    NoExec,
    /// The entry has no action with that id.
    #[error("no such action")]
    NoSuchAction,
    /// The line has an unterminated quote, or is empty.
    #[error("malformed Exec line")]
    Malformed,
}

struct FieldCodes<'a> {
    targets: &'a [String],
    icon: Option<&'a str>,
    name: &'a str,
    path: &'a Path,
}

/// Splits `exec` into arguments (double quotes group; inside them `\"`, `` \` ``, `\$` and
/// `\\` are escapes) and expands `%f %F %u %U %i %c %k %%`. Deprecated codes are dropped.
fn expand_exec(exec: &str, codes: &FieldCodes<'_>) -> Result<Vec<String>, ExecError> {
    let mut out = Vec::new();
    for arg in split_args(exec)? {
        match arg.as_str() {
            "%F" => out.extend(codes.targets.iter().filter_map(|t| as_path(t))),
            "%U" => out.extend(codes.targets.iter().map(|t| as_url(t))),
            "%f" => out.extend(codes.targets.first().and_then(|t| as_path(t))),
            "%u" => out.extend(codes.targets.first().map(|t| as_url(t))),
            "%i" => {
                if let Some(icon) = codes.icon {
                    out.push("--icon".to_owned());
                    out.push(icon.to_owned());
                }
            }
            _ => out.push(expand_inline(&arg, codes)),
        }
    }
    if out.is_empty() {
        return Err(ExecError::Malformed);
    }
    Ok(out)
}

/// Codes inside a longer argument: `%c`, `%k`, `%%`, and (leniently) one target for
/// `%f`/`%u`. Unknown codes vanish.
fn expand_inline(arg: &str, codes: &FieldCodes<'_>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut chars = arg.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('c') => out.push_str(codes.name),
            Some('k') => out.push_str(&codes.path.to_string_lossy()),
            Some('f') => out.extend(codes.targets.first().and_then(|t| as_path(t))),
            Some('u') => out.extend(codes.targets.first().map(|t| as_url(t))),
            Some(_) | None => {}
        }
    }
    out
}

fn split_args(exec: &str) -> Result<Vec<String>, ExecError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => match chars.next() {
                Some(e @ ('"' | '`' | '$' | '\\')) => current.push(e),
                Some(other) => {
                    current.push('\\');
                    current.push(other);
                }
                None => return Err(ExecError::Malformed),
            },
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(ExecError::Malformed);
    }
    if started {
        args.push(current);
    }
    Ok(args)
}

/// A target as a path: `file://` URLs are decoded, other URLs have none.
fn as_path(target: &str) -> Option<String> {
    match target.strip_prefix("file://") {
        Some(rest) => {
            // `file://host/path`: only the local host (empty or `localhost`) is a path.
            let path = rest.strip_prefix("localhost").unwrap_or(rest).to_owned();
            path.starts_with('/').then(|| percent_decode(&path))
        }
        None if has_scheme(target) => None,
        None => Some(target.to_owned()),
    }
}

/// A target as a URL: paths become `file://` URLs.
fn as_url(target: &str) -> String {
    if has_scheme(target) {
        return target.to_owned();
    }
    let mut url = String::from("file://");
    for b in target.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            url.push(char::from(b));
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

fn has_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && target.contains("://")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| hex(*b)),
                bytes.get(i + 2).and_then(|b| hex(*b)),
            )
        {
            // Two hex digits are at most 255.
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn which(program: &str) -> bool {
    let is_file = |p: &Path| p.is_file();
    if program.contains('/') {
        return is_file(Path::new(program));
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| is_file(&dir.join(program))))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\n\
        GenericName=File Manager\nComment=Browse\nIcon=system-file-manager\n\
        Exec=files --name \"My \\\\\"Files\\\\\"\" %U\nTerminal=false\nStartupWMClass=Files\n\
        X-Flatpak=org.neueos.Files\nActions=new-window;missing;empty-trash;\n\
        Keywords=folder;Dateien\\;x;\nKeywords[de]=Ordner;\nOnlyShowIn=neueOS;GNOME;\n\
        MimeType=inode/directory;\n\n\
        [Desktop Action new-window]\nName=New Window\nName[de]=Neues Fenster\nExec=files --new-window\n\n\
        [Desktop Action empty-trash]\nName=Empty Trash\nIcon=user-trash\nExec=files --empty-trash %f\n";

    fn entry() -> DesktopEntry {
        DesktopEntry::parse("org.neueos.Files.desktop", ENTRY, &Locale::none()).unwrap()
    }

    #[test]
    fn fields_localise_and_actions_resolve() {
        let e = entry();
        assert_eq!(e.name, "Files");
        assert_eq!(e.generic_name.as_deref(), Some("File Manager"));
        assert_eq!(e.flatpak_id.as_deref(), Some("org.neueos.Files"));
        assert_eq!(e.mime_types, ["inode/directory"]);
        let ids: Vec<_> = e.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["new-window", "empty-trash"]); // `missing` has no group
        assert_eq!(e.new_window_action().unwrap().name, "New Window");

        let de = DesktopEntry::parse("a.desktop", ENTRY, &Locale::parse("de_DE")).unwrap();
        assert_eq!(de.name, "Dateien");
        assert_eq!(de.new_window_action().unwrap().name, "Neues Fenster");
        assert_eq!(de.keywords, ["Ordner"]);
        assert_eq!(e.keywords, ["folder", "Dateien;x"]);
    }

    #[test]
    fn visibility_follows_only_show_in_and_flags() {
        let mut e = entry();
        assert!(e.is_listed(&["neueOS"]));
        assert!(e.is_listed(&["GNOME"]));
        assert!(!e.is_listed(&["KDE"]));
        e.only_show_in.clear();
        e.not_show_in = vec!["neueOS".into()];
        assert!(!e.show_in(&["neueOS"]));
        assert!(e.show_in(&["KDE"]));
        e.not_show_in.clear();
        e.no_display = true;
        assert!(!e.is_listed(&["neueOS"]));
        e.no_display = false;
        e.hidden = true;
        assert!(!e.is_listed(&["neueOS"]));
        e.hidden = false;
        e.try_exec = Some("/nonexistent/binary".into());
        assert!(!e.is_listed(&["neueOS"]));
    }

    #[test]
    fn exec_lines_expand_field_codes() {
        let e = entry();
        let none: [String; 0] = [];
        assert_eq!(
            e.exec_argv(None, &none).unwrap(),
            ["files", "--name", "My \"Files\""]
        );
        let targets = [
            "/tmp/a b.txt".to_owned(),
            "https://example.com/x".to_owned(),
        ];
        assert_eq!(
            e.exec_argv(None, &targets).unwrap(),
            [
                "files",
                "--name",
                "My \"Files\"",
                "file:///tmp/a%20b.txt",
                "https://example.com/x"
            ]
        );
        assert_eq!(
            e.exec_argv(Some("empty-trash"), &targets).unwrap(),
            ["files", "--empty-trash", "/tmp/a b.txt"]
        );
        assert_eq!(
            e.exec_argv(Some("new-window"), &none).unwrap(),
            ["files", "--new-window"]
        );
        assert_eq!(
            e.exec_argv(Some("nope"), &none),
            Err(ExecError::NoSuchAction)
        );
    }

    #[test]
    fn icon_name_and_key_codes_and_file_urls() {
        let mut e = entry();
        e.path = PathBuf::from("/usr/share/applications/f.desktop");
        e.exec = Some("app %i --title=%c --from=%k 100%% %F".into());
        let args = e
            .exec_argv(
                None,
                &["file:///tmp/x%20y".to_owned(), "ssh://h/p".to_owned()],
            )
            .unwrap();
        assert_eq!(
            args,
            [
                "app",
                "--icon",
                "system-file-manager",
                "--title=Files",
                "--from=/usr/share/applications/f.desktop",
                "100%",
                "/tmp/x y"
            ]
        );
    }

    #[test]
    fn bad_entries_are_refused() {
        let parse = |t: &str| DesktopEntry::parse("a.desktop", t, &Locale::none());
        assert!(matches!(parse("Name=x"), Err(EntryError::NoGroup)));
        assert!(matches!(
            parse("[Desktop Entry]\nName=x\n"),
            Err(EntryError::BadType)
        ));
        assert!(matches!(
            parse("[Desktop Entry]\nType=Application\nExec=x\n"),
            Err(EntryError::Missing("Name"))
        ));
        assert!(matches!(
            parse("[Desktop Entry]\nType=Application\nName=x\n"),
            Err(EntryError::Missing("Exec"))
        ));
        // D-Bus activatable apps may omit Exec.
        assert!(parse("[Desktop Entry]\nType=Application\nName=x\nDBusActivatable=true\n").is_ok());
        let mut e =
            parse("[Desktop Entry]\nType=Application\nName=x\nExec=\"unterminated\n").unwrap();
        assert_eq!(e.exec_argv(None, &[]), Err(ExecError::Malformed));
        e.exec = None;
        assert_eq!(e.exec_argv(None, &[]), Err(ExecError::NoExec));
    }
}
