//! Golden-image tests: render the proof-of-concept sheet with `vello_cpu` and compare it
//! against committed PNGs in `tests/golden/`.
//!
//! Regenerate with `cargo xtask golden --bless` (sets `NEUE_BLESS=1`) and inspect the diff
//! before committing. On a mismatch, the actual image and a diff mask are written to
//! `target/tmp/golden-failures/`.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use neue_paint::cpu::{CpuRenderer, Quality};
use neue_paint::materials::title_bar::{self, TitleBarStyle};
use neue_paint::text::TextEngine;
use neue_paint::{Appearance, Pixmap, Scene, demo};

/// Largest per-channel difference that still counts as equal. vello_cpu output differs
/// slightly between SIMD levels and architectures (docs/research/vello-cpu.md).
const CHANNEL_TOLERANCE: u8 = 2;
/// Share of pixels allowed to exceed `CHANNEL_TOLERANCE`.
const PIXEL_TOLERANCE: f64 = 0.001;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn bless() -> bool {
    std::env::var_os("NEUE_BLESS").is_some_and(|v| v == "1")
}

fn check(name: &str, actual: Pixmap) {
    let path = golden_dir().join(format!("{name}.png"));
    if bless() {
        fs::create_dir_all(golden_dir()).unwrap();
        fs::write(&path, actual.into_png().unwrap()).unwrap();
        return;
    }
    let bytes = fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}; run `cargo xtask golden --bless`",
            path.display()
        )
    });
    let expected = Pixmap::from_png(Cursor::new(bytes)).unwrap();
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height()),
        "{name}: size differs from golden"
    );

    let mut worst = 0u8;
    let mut bad = 0usize;
    let mut mask = Pixmap::new(actual.width(), actual.height());
    for ((a, e), m) in actual
        .data()
        .iter()
        .zip(expected.data())
        .zip(mask.data_mut())
    {
        let diff = [
            a.r.abs_diff(e.r),
            a.g.abs_diff(e.g),
            a.b.abs_diff(e.b),
            a.a.abs_diff(e.a),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        worst = worst.max(diff);
        if diff > CHANNEL_TOLERANCE {
            bad += 1;
            m.r = 255;
        }
        m.a = 255;
    }
    let total = actual.data().len();
    let share = bad as f64 / total as f64;
    if share > PIXEL_TOLERANCE {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden-failures");
        fs::create_dir_all(&out).unwrap();
        fs::write(
            out.join(format!("{name}.actual.png")),
            actual.into_png().unwrap(),
        )
        .unwrap();
        fs::write(
            out.join(format!("{name}.diff.png")),
            mask.into_png().unwrap(),
        )
        .unwrap();
        panic!(
            "{name}: {bad}/{total} pixels differ by more than {CHANNEL_TOLERANCE} \
             (max {worst}); see {}",
            out.display()
        );
    }
}

fn render_sheet(appearance: Appearance, scale: f64) -> Pixmap {
    CpuRenderer::new(Quality::Reference)
        .render(&demo::sheet(appearance), scale)
        .unwrap()
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

fn render_title_bar(appearance: Appearance, focused: bool, scale: f64) -> Pixmap {
    let style = TitleBarStyle {
        appearance,
        focused,
    };
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
                check(
                    &format!("titlebar-{f}-{a}@{s}"),
                    render_title_bar(appearance, focused, scale),
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
