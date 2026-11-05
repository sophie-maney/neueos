//! The trash (the freedesktop Trash specification): whether it is empty, and emptying it.
//!
//! v0 covers the home trash, `$XDG_DATA_HOME/Trash`. Trash directories on other volumes
//! (`$topdir/.Trash/$uid`, `$topdir/.Trash-$uid`) can be added with [`Trash::with_dir`]; the
//! shell finds the mounts (task 3.16).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Whether the trash has anything in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashState {
    /// Nothing is trashed (the dock shows the empty can).
    Empty,
    /// At least one item is trashed.
    Full,
}

/// One or more trash directories, each with `files/` and `info/`.
#[derive(Clone, Debug)]
pub struct Trash {
    dirs: Vec<PathBuf>,
}

impl Trash {
    /// The trash directory `dir` (`~/.local/share/Trash`).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dirs: vec![dir.into()],
        }
    }

    /// The user's home trash, or `None` without a home directory.
    pub fn home() -> Option<Self> {
        crate::data_home().map(|d| Self::new(d.join("Trash")))
    }

    /// Also counts and empties the trash directory `dir` (one on another volume).
    #[must_use]
    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dirs.push(dir.into());
        self
    }

    /// The trash directories.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// Empty if no directory has an item in `files/` (a missing directory is empty).
    pub fn state(&self) -> io::Result<TrashState> {
        for dir in &self.dirs {
            if !is_empty_dir(&dir.join("files"))? {
                return Ok(TrashState::Full);
            }
        }
        Ok(TrashState::Empty)
    }

    /// The number of trashed items.
    pub fn len(&self) -> io::Result<usize> {
        let mut count = 0;
        for dir in &self.dirs {
            count += match fs::read_dir(dir.join("files")) {
                Ok(read) => read.count(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => 0,
                Err(e) => return Err(e),
            };
        }
        Ok(count)
    }

    /// Whether the trash is empty ([`Trash::state`] as a bool).
    pub fn is_empty(&self) -> io::Result<bool> {
        Ok(self.state()? == TrashState::Empty)
    }

    /// Deletes everything in `files/` and `info/` (and the `directorysizes` cache), keeping
    /// the directories. It never follows symbolic links. It carries on past an item it
    /// cannot delete and then returns that first error; the `Ok` value is the number of
    /// items removed from `files/`.
    pub fn empty(&self) -> io::Result<usize> {
        let mut removed = 0;
        let mut first_error = None;
        for dir in &self.dirs {
            for sub in ["files", "info"] {
                let (count, error) = remove_contents(&dir.join(sub));
                if sub == "files" {
                    removed += count;
                }
                first_error = first_error.or(error);
            }
            match fs::remove_file(dir.join("directorysizes")) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    first_error = first_error.or(Some(e));
                }
                _ => {}
            }
        }
        first_error.map_or(Ok(removed), Err)
    }
}

fn is_empty_dir(dir: &Path) -> io::Result<bool> {
    match fs::read_dir(dir) {
        Ok(mut read) => Ok(read.next().is_none()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e),
    }
}

/// Removes what is inside `dir`; returns how many items went and the first error.
fn remove_contents(dir: &Path) -> (usize, Option<io::Error>) {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (0, None),
        Err(e) => return (0, Some(e)),
    };
    let (mut removed, mut first_error) = (0, None);
    for item in read {
        let result = item.and_then(|item| {
            let path = item.path();
            // `file_type` does not follow links: a link to a directory is removed as a file.
            if item.file_type()?.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
        });
        match result {
            Ok(()) => removed += 1,
            Err(e) => first_error = first_error.or(Some(e)),
        }
    }
    (removed, first_error)
}
