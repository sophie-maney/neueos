//! The trash: state and emptying, against fixture directories.

// Test helpers (outside `#[test]` fns) unwrap: a failed fixture should fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use neue_desktop::{Trash, TrashState};

fn trashed(dir: &std::path::Path, name: &str) {
    fs::create_dir_all(dir.join("files")).unwrap();
    fs::create_dir_all(dir.join("info")).unwrap();
    fs::write(dir.join("files").join(name), "x").unwrap();
    fs::write(
        dir.join("info").join(format!("{name}.trashinfo")),
        "[Trash Info]\n",
    )
    .unwrap();
}

#[test]
fn a_missing_or_empty_trash_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let trash = Trash::new(dir.path().join("Trash"));
    assert_eq!(trash.state().unwrap(), TrashState::Empty);
    assert!(trash.is_empty().unwrap());
    assert_eq!(trash.empty().unwrap(), 0);
    fs::create_dir_all(dir.path().join("Trash/files")).unwrap();
    assert_eq!(trash.state().unwrap(), TrashState::Empty);
}

#[test]
fn trashed_items_make_it_full_and_emptying_removes_them() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("Trash");
    trashed(&home, "a.txt");
    trashed(&home, "b.txt");
    fs::create_dir_all(home.join("files/folder/inner")).unwrap();
    fs::write(home.join("files/folder/inner/f"), "x").unwrap();
    fs::write(home.join("directorysizes"), "1 2 folder\n").unwrap();
    let trash = Trash::new(&home);
    assert_eq!(trash.state().unwrap(), TrashState::Full);
    assert_eq!(trash.len().unwrap(), 3);

    assert_eq!(trash.empty().unwrap(), 3);
    assert_eq!(trash.state().unwrap(), TrashState::Empty);
    assert!(
        home.join("files").is_dir() && home.join("info").is_dir(),
        "directories stay"
    );
    assert_eq!(fs::read_dir(home.join("info")).unwrap().count(), 0);
    assert!(!home.join("directorysizes").exists());
}

#[test]
fn volume_trashes_count_too_and_links_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("Trash");
    let volume = dir.path().join("vol/.Trash-1000");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep"), "x").unwrap();
    trashed(&volume, "v.txt");
    fs::create_dir_all(home.join("files")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, home.join("files/link")).unwrap();

    let trash = Trash::new(&home).with_dir(&volume);
    assert_eq!(trash.state().unwrap(), TrashState::Full);
    trash.empty().unwrap();
    assert_eq!(trash.state().unwrap(), TrashState::Empty);
    assert!(
        outside.join("keep").exists(),
        "the link's target is untouched"
    );
}
