//! Masonry spike (task 2.1, ADR 0002 §4): a `masonry_core` widget paints the button tile
//! through the `imaging` replay target, `masonry_testing` renders it, and the result must
//! match neue-paint's own button goldens (`golden.rs`) within the same tolerance.
//!
//! This test never blesses: the neue-paint goldens are the authority.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

#[path = "common/aqua_widget.rs"]
mod aqua_widget;
mod common;

use aqua_widget::AquaButtonTile;
use common::{button_golden_name, compare};
use masonry_core::core::{DefaultProperties, NewWidget};
use masonry_core::dpi::PhysicalSize;
use masonry_core::peniko::Color;
use masonry_testing::{TestHarness, TestHarnessParams};
use neue_paint::demo::{BUTTON_TILE_HEIGHT, BUTTON_TILE_WIDTH};
use neue_paint::materials::ButtonState;
use neue_paint::{Appearance, Pixmap};

/// Render the tile through Masonry at `scale` into a premultiplied pixmap.
fn render_through_masonry(appearance: Appearance, state: ButtonState, scale: f64) -> Pixmap {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small positive sizes"
    )]
    let size = |v: f64| (v * scale).ceil() as u32;
    let mut params = TestHarnessParams::default();
    params.window_size = PhysicalSize::new(size(BUTTON_TILE_WIDTH), size(BUTTON_TILE_HEIGHT));
    params.scale_factor = scale;
    params.root_padding = 0;
    params.background_color = Color::TRANSPARENT;
    let widget = NewWidget::new(AquaButtonTile::with_state(appearance, state));
    let mut harness = TestHarness::create_with(DefaultProperties::new(), widget, params);
    let image = harness.render();

    // masonry_testing returns unpremultiplied RGBA8; our goldens are premultiplied.
    let (width, height) = image.dimensions();
    let mut bytes = image.into_raw();
    vello_common::pixmap::premultiply_rgba8(&mut bytes);
    let mut pixmap = Pixmap::new(
        u16::try_from(width).unwrap(),
        u16::try_from(height).unwrap(),
    );
    pixmap.data_as_u8_slice_mut().copy_from_slice(&bytes);
    pixmap
}

#[test]
fn masonry_button_matches_neue_paint_goldens() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for state in [ButtonState::Normal, ButtonState::Pressed] {
            for scale in [1.0, 2.0] {
                let name = button_golden_name(appearance, state, scale);
                compare(&name, render_through_masonry(appearance, state, scale));
            }
        }
    }
}
