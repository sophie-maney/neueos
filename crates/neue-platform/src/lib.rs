//! Windowing drivers for neueOS's toolkit (PLAN.md §3.2, ADR 0002 §4).
//!
//! neue-platform owns the event loops that drive Masonry's `RenderRoot`:
//! - [`wayland`] (Linux): a smithay-client-toolkit driver. Task 2.2 is its spike: one
//!   xdg-toplevel, one layer-shell surface, wgpu, fractional scale, text-input-v3, and
//!   Masonry layers realised as xdg-popups. Task 2.5 hardens it.
//! - [`winit`] (macOS, ADR 0032): windows, scale, pointer and keyboard, IME, cursors,
//!   clipboard, animation frames and AccessKit, through winit. Not built on Linux, where
//!   winit would bring X11 crates; Linux apps use the Wayland driver.
//!
//! Both render through the shared `gpu` module: vello_gpu into an offscreen texture, then
//! a blit to the window's wgpu surface.
//!
//! App crates never use this crate directly; neue-kit wraps it (task 2.7).

use std::sync::Arc;

use masonry_core::core::DefaultProperties;
use masonry_core::peniko::Blob;

mod gpu;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(target_os = "macos")]
pub mod winit;

/// Options shared by every window of a driver.
#[derive(Debug)]
pub struct Options {
    pub default_properties: Arc<DefaultProperties>,
    /// Font added to the Latin fallback chain (no system fonts: ADR 0002, ADR 0030).
    pub font: Option<Blob<u8>>,
}
