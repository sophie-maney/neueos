//! Materials: procedural textures and vector-drawn controls. Each has a light and a dark
//! variant. The translucent ones (menu bar, dock shelf, HUD) also take a
//! [`crate::Backdrop`].

mod aqua_button;
pub mod dock_shelf;
mod focus_ring;
pub mod hud;
mod linen;
pub mod menu_bar;
pub mod title_bar;

pub use aqua_button::{ButtonState, PushButton, aqua_button, push_button, push_button_outline};
pub use focus_ring::focus_ring;
pub use linen::{Linen, fill_linen};
