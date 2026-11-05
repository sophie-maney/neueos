//! `appearance.toml` for the shell process (ADR 0075): the theme at start-up, and a
//! watcher that reports every later change, from `neue-settingsd` or a hand edit.

use std::os::fd::AsFd;

use neue_config::{AppearanceConfig, ConfigFile, Layers};
use neue_kit::Theme;
use rustix::fs::inotify::{self, CreateFlags, ReadFlags, WatchFlags};
use rustix::io::Errno;

/// The current `appearance.toml`. A file that doesn't load is logged and the defaults
/// apply.
pub fn load(layers: &Layers) -> AppearanceConfig {
    match neue_config::load::<AppearanceConfig>(layers) {
        Ok(loaded) => {
            for key in &loaded.unknown_keys {
                tracing::warn!(key, "unknown key in appearance.toml");
            }
            loaded.config
        }
        Err(error) => {
            tracing::warn!(%error, "appearance.toml is invalid: using the default look");
            AppearanceConfig::default()
        }
    }
}

/// The theme to start with.
pub fn start_theme(layers: &Layers) -> Theme {
    load(layers).theme()
}

/// Blocks the calling thread, calling `changed` with the new config each time
/// `appearance.toml` (in any layer) is written and differs from the last one. Returns when
/// watching fails or `changed` returns false.
pub fn watch(
    layers: &Layers,
    mut last: AppearanceConfig,
    mut changed: impl FnMut(&AppearanceConfig) -> bool,
) {
    let fd = match inotify::init(CreateFlags::CLOEXEC) {
        Ok(fd) => fd,
        Err(error) => {
            tracing::warn!(%error, "not watching appearance.toml: changes need a shell restart");
            return;
        }
    };
    let mut watched = 0;
    for dir in layers.dirs() {
        let flags = WatchFlags::CLOSE_WRITE
            | WatchFlags::MOVED_TO
            | WatchFlags::MOVED_FROM
            | WatchFlags::CREATE
            | WatchFlags::DELETE;
        // System layers may not exist.
        if inotify::add_watch(&fd, dir, flags).is_ok() {
            watched += 1;
        }
    }
    if watched == 0 {
        tracing::warn!("no config directory to watch: appearance.toml changes need a restart");
        return;
    }
    let name = AppearanceConfig::FILE_NAME.as_bytes();
    let mut buffer = [std::mem::MaybeUninit::<u8>::uninit(); 4096];
    let mut reader = inotify::Reader::new(fd.as_fd(), &mut buffer);
    loop {
        let relevant = match reader.next() {
            Ok(event) => {
                event.events().contains(ReadFlags::QUEUE_OVERFLOW)
                    || event.file_name().is_some_and(|n| n.to_bytes() == name)
            }
            Err(Errno::INTR) => false,
            Err(error) => {
                tracing::warn!(%error, "watching appearance.toml failed");
                return;
            }
        };
        if !relevant {
            continue;
        }
        let new = load(layers);
        if new != last {
            last = new;
            if !changed(&last) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // test code

    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_watcher_reports_edits_of_appearance_toml_only() {
        let dir = tempfile::tempdir().unwrap();
        let layers = Layers::from_dirs([dir.path().to_owned()]);
        let (tx, rx) = mpsc::channel();
        let watched = layers.clone();
        std::thread::spawn(move || {
            watch(&watched, AppearanceConfig::default(), |config| {
                tx.send(config.clone()).is_ok()
            });
        });
        // The watch is added before the first read; give the thread a moment.
        std::thread::sleep(Duration::from_millis(200));

        std::fs::write(dir.path().join("shell.toml"), "").unwrap();
        std::fs::write(dir.path().join("appearance.toml"), "mode = \"dark\"\n").unwrap();
        let config = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(config.theme().variant.is_dark());
        assert!(start_theme(&layers).variant.is_dark());

        // Same content again: nothing new to report; a real change is.
        std::fs::write(dir.path().join("appearance.toml"), "mode = \"dark\"\n").unwrap();
        std::fs::write(dir.path().join("appearance.toml"), "").unwrap();
        let config = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!config.theme().variant.is_dark());
    }
}
