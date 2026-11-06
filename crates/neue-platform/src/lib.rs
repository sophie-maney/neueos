//! Windowing drivers for neueOS's toolkit (PLAN.md §3.2, ADR 0002 §4).
//!
//! neue-platform owns the event loops that drive Masonry's `RenderRoot`:
//! - [`wayland`] (Linux, ADR 0033): a smithay-client-toolkit driver: xdg-toplevels and
//!   layer-shell surfaces, fractional scale, frame callbacks, cursor shapes, key repeat,
//!   text-input-v3, and Masonry layers realised as xdg-popups.
//! - [`winit`] (macOS, ADR 0032): windows, scale, pointer and keyboard, IME, cursors,
//!   clipboard, animation frames and AccessKit, through winit. Not built on Linux, where
//!   winit would bring X11 crates; Linux apps use the Wayland driver.
//!
//! Both run the same [`App`] (opening windows through [`Ctx`]) and render through the
//! shared `gpu` module: vello_gpu into an offscreen texture, then a blit to the window's
//! wgpu surface. [`run`] is the host's driver.
//!
//! [`desktop`] carries what only the shell uses: outputs and the compositor's window list
//! (task 3.2a, ADR 0067).
//!
//! App crates never use this crate directly; neue-kit wraps it (task 2.7).

use std::sync::Arc;

use masonry_core::core::DefaultProperties;
use masonry_core::peniko::Blob;

mod app;
pub mod desktop;
mod gpu;
mod popover;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(target_os = "macos")]
pub mod winit;

pub use app::{
    Anchor, App, Blur, ChromeHeight, Ctx, Decorations, LayerKeyboard, LayerSpec, Margin,
    MenuAddress, ShellLayer, TokenRequest, Waker, WindowId, WindowKind, WindowSpec,
};
pub use popover::{
    PopoverAnchor, PopoverLanded, PopoverRequest, PopupBlur, PopupBlurRequest, Side,
};
#[cfg(target_os = "linux")]
pub use wayland::{Error, run};
#[cfg(target_os = "macos")]
pub use winit::{Error, run};

/// Options shared by every window of a driver.
#[derive(Debug)]
pub struct Options {
    pub default_properties: Arc<DefaultProperties>,
    /// Font added to the Latin fallback chain (no system fonts: ADR 0002, ADR 0030).
    pub font: Option<Blob<u8>>,
    /// More faces of the UI font (bold), registered after `font` (ADR 0068).
    pub extra_fonts: Vec<Blob<u8>>,
}
