//! Windowing drivers for neueOS's toolkit (PLAN.md §3.2, ADR 0002 §4).
//!
//! neue-platform owns the event loops that drive Masonry's `RenderRoot`:
//! - [`wayland`] (Linux): a smithay-client-toolkit driver. Task 2.2 is its spike: one
//!   xdg-toplevel, one layer-shell surface, wgpu, fractional scale, text-input-v3, and
//!   Masonry layers realised as xdg-popups. Task 2.5 hardens it.
//! - winit (macOS and dev): task 2.4.
//!
//! App crates never use this crate directly; neue-kit wraps it (task 2.7).

#[cfg(target_os = "linux")]
pub mod wayland;
