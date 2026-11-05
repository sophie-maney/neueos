//! The proof-of-concept sheet: linen with a normal and a pressed Aqua button. Shared by the
//! golden test and the `poc` example.

use vello_common::kurbo::{Affine, Circle, Point, Rect, Size};
use vello_common::peniko::Gradient;

use crate::materials::dock_shelf::{self, ShelfStyle};
use crate::materials::{
    ButtonState, Edge, aqua_button, fill_linen, hud, menu_bar, menu_highlight, menu_panel,
    menu_separator, popover, sheet as sheet_material,
};
use crate::peniko::Color;
use crate::text::TextEngine;
use crate::{Appearance, Backdrop, Scene, Variant};
use neue_theme::materials::{menu, popover as popover_material};
use neue_theme::metrics::control::REGULAR_HEIGHT;

/// Sheet width in logical pixels.
pub const SHEET_WIDTH: f64 = 240.0;
/// Sheet height in logical pixels.
pub const SHEET_HEIGHT: f64 = 80.0;

/// Draw the sheet for `appearance` with its top-left corner at the current origin.
pub fn draw_sheet(scene: &mut Scene, appearance: Appearance) {
    fill_linen(
        scene,
        &Rect::new(0.0, 0.0, SHEET_WIDTH, SHEET_HEIGHT),
        appearance,
    );
    let y = (SHEET_HEIGHT - REGULAR_HEIGHT) / 2.0;
    for (x, state) in [(24.0, ButtonState::Normal), (132.0, ButtonState::Pressed)] {
        aqua_button(
            scene,
            Rect::new(x, y, x + 84.0, y + REGULAR_HEIGHT),
            state,
            appearance,
        );
    }
}

/// A scene containing just the sheet for `appearance`.
pub fn sheet(appearance: Appearance) -> Scene {
    let mut scene = Scene::new(SHEET_WIDTH, SHEET_HEIGHT);
    draw_sheet(&mut scene, appearance);
    scene
}

/// Button tile width in logical pixels: one Aqua button on linen.
pub const BUTTON_TILE_WIDTH: f64 = 108.0;
/// Button tile height in logical pixels.
pub const BUTTON_TILE_HEIGHT: f64 = 38.0;

/// Draw one button in `state` on linen, with the tile's top-left corner at the current
/// origin. Shared by the golden tests and the Masonry spike (task 2.1), so a Masonry
/// widget's render can be compared against neue-paint's own.
pub fn draw_button_tile(scene: &mut Scene, variant: impl Into<Variant>, state: ButtonState) {
    let variant = variant.into();
    fill_linen(
        scene,
        &Rect::new(0.0, 0.0, BUTTON_TILE_WIDTH, BUTTON_TILE_HEIGHT),
        variant,
    );
    aqua_button(scene, Rect::new(12.0, 8.0, 96.0, 30.0), state, variant);
}

/// A scene containing just the button tile.
pub fn button_tile(variant: impl Into<Variant>, state: ButtonState) -> Scene {
    let mut scene = Scene::new(BUTTON_TILE_WIDTH, BUTTON_TILE_HEIGHT);
    draw_button_tile(&mut scene, variant, state);
    scene
}

/// The translucent materials, each drawn over a stand-in for a blurred desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialTile {
    /// The menu bar with a few menu titles.
    MenuBar,
    /// The dock shelf in perspective.
    DockShelf,
    /// The flat dock shelf (docks on the left and right edges).
    DockShelfFlat,
    /// A HUD panel with a title.
    Hud,
    /// A menu panel with a few items, one highlighted.
    Menu,
    /// A popover with its arrow on top.
    Popover,
    /// A sheet hanging from a title bar's edge.
    Sheet,
}

impl MaterialTile {
    /// Every tile.
    pub const ALL: [Self; 7] = [
        Self::MenuBar,
        Self::DockShelf,
        Self::DockShelfFlat,
        Self::Hud,
        Self::Menu,
        Self::Popover,
        Self::Sheet,
    ];

    /// Size of the tile in logical pixels.
    pub fn size(self) -> (f64, f64) {
        match self {
            Self::MenuBar => (240.0, 34.0),
            Self::DockShelf | Self::DockShelfFlat => (320.0, 56.0),
            Self::Hud => (160.0, 120.0),
            Self::Menu => (180.0, 120.0),
            Self::Popover => (200.0, 140.0),
            Self::Sheet => (200.0, 120.0),
        }
    }

    /// Short name, as used in golden file names.
    pub fn name(self) -> &'static str {
        match self {
            Self::MenuBar => "menu-bar",
            Self::DockShelf => "dock-shelf",
            Self::DockShelfFlat => "dock-shelf-flat",
            Self::Hud => "hud",
            Self::Menu => "menu",
            Self::Popover => "popover",
            Self::Sheet => "sheet",
        }
    }
}

/// A smooth, colourful stand-in for a blurred desktop, so translucency shows: a gradient
/// with a bright and a dark blob. The dark appearance gets a dimmer one.
fn blurred_desktop(scene: &mut Scene, variant: Variant) {
    let (w, h) = (scene.width(), scene.height());
    let dim = if variant.is_dark() { 0.45 } else { 1.0 };
    let stop = |r: f32, g: f32, b: f32| Color::new([r * dim, g * dim, b * dim, 1.0]);
    let gradient = Gradient::new_linear(Point::new(0.0, 0.0), Point::new(w, 0.0)).with_stops([
        (0.0, stop(0.30, 0.50, 0.85)),
        (0.5, stop(0.92, 0.72, 0.38)),
        (1.0, stop(0.80, 0.34, 0.50)),
    ]);
    scene.fill(&Rect::new(0.0, 0.0, w, h), gradient);
    let r = h.min(w) * 0.35;
    scene.fill(
        &Circle::new((w * 0.3, h * 0.55), r),
        Color::new([1.0, 1.0, 1.0, 0.55 * dim]),
    );
    scene.fill(
        &Circle::new((w * 0.72, h * 0.45), r),
        Color::new([0.0, 0.0, 0.0, 0.45]),
    );
}

/// Draw `tile` for `variant` with its top-left corner at the current origin.
pub fn draw_material_tile(
    scene: &mut Scene,
    tile: MaterialTile,
    variant: impl Into<Variant>,
    backdrop: Backdrop,
) {
    let variant = variant.into();
    let (w, h) = tile.size();
    blurred_desktop(scene, variant);
    match tile {
        MaterialTile::MenuBar => {
            menu_bar::menu_bar(
                scene,
                Rect::new(0.0, 0.0, w, menu_bar::HEIGHT),
                variant,
                backdrop,
            );
            let line = TextEngine::new().line("Edit      View      Help", menu_bar::FONT_SIZE);
            menu_bar::label(scene, &line, 12.0, 0.0, variant);
        }
        MaterialTile::DockShelf => dock_shelf::dock_shelf(
            scene,
            Rect::new(8.0, h - 10.0 - dock_shelf::DEPTH, w - 8.0, h - 10.0),
            ShelfStyle::Perspective,
            variant,
            backdrop,
        ),
        MaterialTile::DockShelfFlat => dock_shelf::dock_shelf(
            scene,
            Rect::new(8.0, 12.0, w - 8.0, h - 12.0),
            ShelfStyle::Flat,
            variant,
            backdrop,
        ),
        MaterialTile::Hud => {
            let margin = hud::SHADOW_REACH + 2.0;
            let rect = Rect::new(margin, margin - 4.0, w - margin, h - margin - 4.0);
            hud::hud(scene, rect, variant, backdrop);
            let palette = neue_theme::materials::hud::Palette::new(variant, backdrop);
            let line = TextEngine::new().line("Volume", neue_theme::typography::BODY);
            let x = rect.x0 + (rect.width() - line.width()) / 2.0;
            scene.draw_text(&line, Point::new(x, rect.center().y + 5.0), palette.label);
        }
        MaterialTile::Menu => {
            let palette = neue_theme::materials::menu::Palette::new(variant, backdrop);
            let margin = menu::SHADOW_REACH.min(12.0);
            let rect = Rect::new(margin, 8.0, w - margin, h - 12.0);
            menu_panel(scene, rect, variant, backdrop);
            let mut engine = TextEngine::new();
            let row = menu::ITEM_HEIGHT;
            let top = rect.y0 + menu::PADDING_Y;
            let band = Rect::new(rect.x0 + 1.0, top + row, rect.x1 - 1.0, top + 2.0 * row);
            menu_highlight(scene, band, variant);
            let separator = Rect::new(rect.x0, top + 2.0 * row, rect.x1, top + 2.0 * row + 10.0);
            menu_separator(scene, separator, variant);
            for (i, (title, color)) in [
                ("Open", palette.label),
                ("Copy", palette.highlighted_label),
                ("Paste", palette.disabled_label),
            ]
            .into_iter()
            .enumerate()
            {
                let y = if i == 2 {
                    top + 2.0 * row + 10.0
                } else {
                    top + i as f64 * row
                };
                let line = engine.line(title, neue_theme::typography::BODY);
                scene.draw_text(
                    &line,
                    Point::new(rect.x0 + menu::LABEL_INSET, y + 14.0),
                    color,
                );
            }
        }
        MaterialTile::Popover => {
            let margin = popover_material::SHADOW_REACH.min(18.0);
            let size = Size::new(w - 2.0 * margin, h - 2.0 * margin);
            scene.with_transform(Affine::translate((margin, margin)), |scene| {
                popover(scene, size, Edge::Top, size.width / 2.0, variant, backdrop);
            });
        }
        MaterialTile::Sheet => {
            let size = Size::new(w - 24.0, h - 24.0);
            scene.with_transform(Affine::translate((12.0, 0.0)), |scene| {
                sheet_material(scene, size, variant, backdrop);
            });
        }
    }
}

/// A scene containing just `tile`.
pub fn material_tile(tile: MaterialTile, variant: impl Into<Variant>, backdrop: Backdrop) -> Scene {
    let (w, h) = tile.size();
    let mut scene = Scene::new(w, h);
    draw_material_tile(&mut scene, tile, variant, backdrop);
    scene
}
