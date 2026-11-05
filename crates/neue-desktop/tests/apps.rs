//! The application database and app id matching, against fixture directories.

// Test helpers (outside `#[test]` fns) unwrap: a failed fixture should fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use neue_desktop::{AppDatabase, Locale};

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn app(name: &str, extra: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/{}\n{extra}",
        name.to_lowercase()
    )
}

#[test]
fn directories_nest_ids_and_the_user_wins() {
    let user = tempfile::tempdir().unwrap();
    let system = tempfile::tempdir().unwrap();
    write(system.path(), "files.desktop", &app("Files", ""));
    write(system.path(), "kde/konsole.desktop", &app("Konsole", ""));
    write(system.path(), "broken.desktop", "not a desktop file");
    write(system.path(), "readme.txt", "ignored");
    // The user's entry replaces the system's of the same id; a Hidden one deletes it.
    write(user.path(), "files.desktop", &app("My Files", ""));
    write(user.path(), "konsole-hidden.desktop", &app("Hid", ""));
    write(system.path(), "old.desktop", &app("Old", ""));
    write(user.path(), "old.desktop", &app("Old", "Hidden=true\n"));

    let db = AppDatabase::load(
        &[
            user.path().to_owned(),
            system.path().to_owned(),
            "/nonexistent".into(),
        ],
        &Locale::none(),
    );
    let ids: Vec<_> = db.all().iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"kde-konsole.desktop"));
    assert!(!ids.contains(&"broken.desktop"));
    assert_eq!(db.by_id("files").unwrap().name, "My Files");
    assert_eq!(db.by_id("files.desktop").unwrap().name, "My Files");
    assert!(
        db.by_id("old").is_none(),
        "a hidden entry masks the one below"
    );

    let listed: Vec<_> = db.listed(&["neueOS"]).map(|e| e.name.as_str()).collect();
    assert_eq!(listed, ["Hid", "Konsole", "My Files"], "sorted, no hidden");
}

#[test]
fn app_ids_match_entries_by_the_documented_rules() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(
        r,
        "org.mozilla.firefox.desktop",
        &app("Firefox", "X-Flatpak=org.mozilla.firefox\n"),
    );
    write(r, "org.gnome.Nautilus.desktop", &app("Files-nautilus", ""));
    write(
        r,
        "code.desktop",
        &app("Code", "StartupWMClass=Code - Insiders\n"),
    );
    write(r, "kde-konsole.desktop", &app("Terminal", ""));
    write(
        r,
        "gimp-2.10.desktop",
        &app("GIMP", "StartupWMClass=Gimp-2.10\nNoDisplay=true\n"),
    );
    write(
        r,
        "fix.desktop",
        &app("Alacritty", "Exec=/usr/bin/alacritty --x\n"),
    );
    write(
        r,
        "tool.desktop",
        "[Desktop Entry]\nType=Application\nName=Pretty Name\nExec=/opt/tool/run\n",
    );
    write(
        r,
        "link.desktop",
        "[Desktop Entry]\nType=Link\nName=Web\nURL=https://example.com\n",
    );
    let db = AppDatabase::load(&[r.to_owned()], &Locale::none());
    let id = |app_id: &str| db.find_by_app_id(app_id).map(|e| e.id.as_str());

    assert_eq!(
        id("org.mozilla.firefox"),
        Some("org.mozilla.firefox.desktop")
    ); // file name
    assert_eq!(id("firefox"), Some("org.mozilla.firefox.desktop")); // last part of the id
    assert_eq!(id("Firefox"), Some("org.mozilla.firefox.desktop")); // case
    assert_eq!(id("nautilus"), Some("org.gnome.Nautilus.desktop"));
    assert_eq!(id("Code - Insiders"), Some("code.desktop")); // StartupWMClass
    assert_eq!(id("code - insiders"), Some("code.desktop"));
    assert_eq!(id("konsole"), Some("kde-konsole.desktop")); // after the vendor prefix
    assert_eq!(id("Gimp-2.10"), Some("gimp-2.10.desktop")); // NoDisplay still matches
    assert_eq!(id("alacritty"), Some("fix.desktop")); // Exec program
    assert_eq!(id("pretty name"), Some("tool.desktop")); // Name
    assert_eq!(id("com.example.Missing"), None);
    assert_eq!(id(""), None);
    assert_eq!(id("web"), None, "links are not applications");
}

#[test]
fn the_exact_file_name_beats_a_class_that_happens_to_match() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "a.desktop", &app("A", "StartupWMClass=b\n"));
    write(r, "b.desktop", &app("B", ""));
    let db = AppDatabase::load(&[r.to_owned()], &Locale::none());
    assert_eq!(db.find_by_app_id("b").unwrap().id, "b.desktop");
}
