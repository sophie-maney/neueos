//! Golden-image tests: render the proof-of-concept sheet, button tiles and title bars with `vello_cpu` and compare it
//! against committed PNGs in `tests/golden/`.
//!
//! Regenerate with `cargo xtask golden --bless` (sets `NEUE_BLESS=1`) and inspect the diff
//! before committing. On a mismatch, the actual image and a diff mask are written to
//! `target/tmp/golden-failures/`.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

mod common;

use common::{button_golden_name, check};
use neue_paint::cpu::{CpuRenderer, Quality};
use neue_paint::materials::ButtonState;
use neue_paint::materials::title_bar::{self, TitleBarStyle, TrafficLight};
use neue_paint::text::TextEngine;
use neue_paint::{Accent, Appearance, Contrast, Pixmap, Scene, Variant, demo};

fn render_sheet(appearance: Appearance, scale: f64) -> Pixmap {
    CpuRenderer::new(Quality::Reference)
        .render(&demo::sheet(appearance), scale)
        .unwrap()
}

#[test]
fn button_tile_goldens() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        for state in [ButtonState::Normal, ButtonState::Pressed] {
            for scale in [1.0, 2.0] {
                let pixmap = CpuRenderer::new(Quality::Reference)
                    .render(&demo::button_tile(appearance, state), scale)
                    .unwrap();
                check(&button_golden_name(appearance, state, scale), pixmap);
            }
        }
    }
}

/// Graphite accent and increased contrast: button tiles × light/dark × 1x/2x.
#[test]
fn button_tile_variant_goldens() {
    let variants = [
        ("graphite", Accent::Graphite, Contrast::Standard),
        ("contrast", Accent::Blue, Contrast::Increased),
    ];
    for (v, accent, contrast) in variants {
        for (appearance, a) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            for (state, st) in [
                (ButtonState::Normal, "normal"),
                (ButtonState::Pressed, "pressed"),
            ] {
                for (scale, s) in [(1.0, "1x"), (2.0, "2x")] {
                    let variant = Variant {
                        appearance,
                        accent,
                        contrast,
                    };
                    let pixmap = CpuRenderer::new(Quality::Reference)
                        .render(&demo::button_tile(variant, state), scale)
                        .unwrap();
                    check(&format!("button-{v}-{st}-{a}@{s}"), pixmap);
                }
            }
        }
    }
}

#[test]
fn sheet_light_1x() {
    check("sheet-light@1x", render_sheet(Appearance::Light, 1.0));
}

#[test]
fn sheet_light_2x() {
    check("sheet-light@2x", render_sheet(Appearance::Light, 2.0));
}

#[test]
fn sheet_dark_1x() {
    check("sheet-dark@1x", render_sheet(Appearance::Dark, 1.0));
}

#[test]
fn sheet_dark_2x() {
    check("sheet-dark@2x", render_sheet(Appearance::Dark, 2.0));
}

/// Width of the title bar goldens: wide enough for a centred title.
const TITLE_BAR_WIDTH: f64 = 320.0;

fn render_title_bar(style: TitleBarStyle, scale: f64) -> Pixmap {
    let title = TextEngine::new().line_truncated(
        "Untitled — Edited",
        title_bar::TITLE_FONT_SIZE,
        title_bar::title_max_width(TITLE_BAR_WIDTH),
    );
    let mut scene = Scene::new(TITLE_BAR_WIDTH, title_bar::TITLE_BAR_HEIGHT);
    title_bar::title_bar(&mut scene, TITLE_BAR_WIDTH, &title, style);
    CpuRenderer::new(Quality::Reference)
        .render(&scene, scale)
        .unwrap()
}

/// Title bar goldens: focused/unfocused × light/dark × 1x/2x.
#[test]
fn title_bar_goldens() {
    for (appearance, a) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for (focused, f) in [(true, "focused"), (false, "unfocused")] {
            for (scale, s) in [(1.0, "1x"), (2.0, "2x")] {
                let style = TitleBarStyle {
                    variant: appearance.into(),
                    focused,
                    ..TitleBarStyle::default()
                };
                check(
                    &format!("titlebar-{f}-{a}@{s}"),
                    render_title_bar(style, scale),
                );
            }
        }
    }
}

/// Traffic-light interaction goldens: pointer over the buttons (focused and unfocused),
/// and each button held down, × light/dark × 1x/2x.
#[test]
fn title_bar_interaction_goldens() {
    let states = [
        ("hover-focused", true, None),
        ("hover-unfocused", false, None),
        ("pressed-close", true, Some(TrafficLight::Close)),
        ("pressed-minimize", true, Some(TrafficLight::Minimize)),
        ("pressed-zoom", true, Some(TrafficLight::Zoom)),
    ];
    for (appearance, a) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for (name, focused, pressed) in states {
            for (scale, s) in [(1.0, "1x"), (2.0, "2x")] {
                let style = TitleBarStyle {
                    variant: appearance.into(),
                    focused,
                    hover: true,
                    pressed,
                };
                check(
                    &format!("titlebar-{name}-{a}@{s}"),
                    render_title_bar(style, scale),
                );
            }
        }
    }
}

/// Graphite traffic lights (pointer over them) and an increased-contrast focused bar,
/// × light/dark × 1x/2x.
#[test]
fn title_bar_variant_goldens() {
    let variants = [
        ("hover-graphite", Accent::Graphite, Contrast::Standard, true),
        ("focused-contrast", Accent::Blue, Contrast::Increased, false),
    ];
    for (name, accent, contrast, hover) in variants {
        for (appearance, a) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            for (scale, s) in [(1.0, "1x"), (2.0, "2x")] {
                let style = TitleBarStyle {
                    variant: Variant {
                        appearance,
                        accent,
                        contrast,
                    },
                    focused: true,
                    hover,
                    pressed: None,
                };
                check(
                    &format!("titlebar-{name}-{a}@{s}"),
                    render_title_bar(style, scale),
                );
            }
        }
    }
}

/// Rendering twice in one process must give identical pixels (texture cache included).
#[test]
fn render_is_repeatable() {
    let mut renderer = CpuRenderer::new(Quality::Reference);
    let scene = demo::sheet(Appearance::Light);
    let a = renderer.render(&scene, 2.0).unwrap();
    let b = renderer.render(&scene, 2.0).unwrap();
    assert_eq!(a.data(), b.data());
}
