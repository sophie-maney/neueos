//! `neue-settingsd`: the settings service (PLAN.md §3.4, ADR 0074).
//!
//! The neueOS TOML files (`neue-config`) are the source of truth. The service serves them
//! as typed keys on `org.neueos.Settings1`, edits the user's files when a key is set, and
//! derives what other toolkits read from `appearance.toml`: GTK's `settings.ini` and the
//! `org.gnome.desktop.*` dconf keys.
//!
//! - [`Store`] (portable) is the key store: get, set and reset over the layered files,
//!   validated against the schema. [`Store::in_memory`] is the mock that Preferences uses
//!   on macOS.
//! - [`gtk`] and [`dconf`] (portable) turn an [`AppearanceConfig`] into `settings.ini`
//!   lines and dconf changesets.
//! - `daemon` (Linux) is the D-Bus service.
//!
//! [`AppearanceConfig`]: neue_config::AppearanceConfig

#[cfg(target_os = "linux")]
pub mod daemon;
pub mod dconf;
pub mod gtk;
mod key;
mod store;

pub use key::{File, Key};
pub use store::{Changes, Error, Store};
