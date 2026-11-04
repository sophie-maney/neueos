//! Golden-image tests of screencopy (tasks 1.16b and 1.16c, ADRs 0022 and 0023): a
//! scripted client captures a headless output or a window and the capture is compared with `tests/golden/*.png`.
//!
//! The goldens are rendered by Mesa's llvmpipe; regenerate them on Linux with
//! `cargo xtask linux cargo xtask golden --bless` and inspect the images before
//! committing. On a mismatch the capture is written to `target/tmp/golden-failures/`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use harness::{FrameState, Server};
use neue_comp::HEADLESS_OUTPUT_NAME;
use neue_paint::Pixmap;
use neue_paint::color::PremulRgba8;

/// Largest per-channel difference that still counts as equal: llvmpipe may round
/// blending differently between Mesa versions (ADR 0021).
const CHANNEL_TOLERANCE: u8 = 2;

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

fn bless() -> bool {
    std::env::var_os("NEUE_BLESS").is_some_and(|v| v == "1")
}

/// ARGB8888 rows (bytes B, G, R, A) as a pixmap. Captures are opaque, so straight and
/// premultiplied alpha are the same.
fn to_pixmap((w, h): (i32, i32), data: &[u8]) -> Pixmap {
    let mut pixmap = Pixmap::new(w as u16, h as u16);
    for (pixel, [b, g, r, a]) in pixmap.data_mut().iter_mut().zip(data.as_chunks::<4>().0) {
        *pixel = PremulRgba8 {
            r: *r,
            g: *g,
            b: *b,
            a: *a,
        };
    }
    pixmap
}

fn check(name: &str, actual: Pixmap) {
    let path = golden_path(name);
    if bless() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual.into_png().unwrap()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}; run `cargo xtask linux cargo xtask golden --bless`",
            path.display()
        )
    });
    let expected = Pixmap::from_png(Cursor::new(bytes)).unwrap();
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height()),
        "{name}: size differs from the golden"
    );
    let bad = actual
        .data()
        .iter()
        .zip(expected.data())
        .filter(|(a, e)| {
            a.r.abs_diff(e.r) > CHANNEL_TOLERANCE
                || a.g.abs_diff(e.g) > CHANNEL_TOLERANCE
                || a.b.abs_diff(e.b) > CHANNEL_TOLERANCE
                || a.a.abs_diff(e.a) > CHANNEL_TOLERANCE
        })
        .count();
    if bad > 0 {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/golden-failures");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join(format!("{name}.png"));
        std::fs::write(&out, actual.into_png().unwrap()).unwrap();
        panic!(
            "{name}: {bad} pixels differ from the golden; capture written to {}",
            out.display()
        );
    }
}

#[test]
fn screencopy_output() {
    let server = Server::start_rendering();
    server
        .with_state(|state| state.headless_set_mode(HEADLESS_OUTPUT_NAME, (320, 240)))
        .expect("changing the mode");
    let mut client = server.connect();
    let mut back = client.create_toplevel("Back", "org.neueos.Test", (120, 80));
    client.fill(&mut back, 0xffc03020);
    let mut front = client.create_toplevel("Front", "org.neueos.Test", (90, 60));
    client.fill(&mut front, 0xff2040c0);

    let session = client.capture_output(0, false);
    let frame = client.capture_frame(&session);
    let state = client.wait_for_frame(&frame);
    assert!(matches!(state, FrameState::Ready { .. }), "{state:?}");
    check("screencopy-output", to_pixmap(frame.size, &frame.data()));
}

/// `neuectl screenshot` of the same scene as `screencopy_output`: the PNG on disk matches
/// the same golden (task 1.16d, ADR 0024).
#[test]
fn neuectl_screenshot() {
    let server = Server::start_rendering();
    server
        .with_state(|state| state.headless_set_mode(HEADLESS_OUTPUT_NAME, (320, 240)))
        .expect("changing the mode");
    let mut client = server.connect();
    let mut back = client.create_toplevel("Back", "org.neueos.Test", (120, 80));
    client.fill(&mut back, 0xffc03020);
    let mut front = client.create_toplevel("Front", "org.neueos.Test", (90, 60));
    client.fill(&mut front, 0xff2040c0);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.png");
    let info = server
        .ipc()
        .screenshot(neue_ipc::ScreenshotParams {
            path: path.to_str().unwrap().to_owned(),
            ..neue_ipc::ScreenshotParams::default()
        })
        .expect("taking the screenshot");
    assert_eq!(
        (info.output.as_str(), info.width, info.height),
        (HEADLESS_OUTPUT_NAME, 320, 240)
    );
    let png = Pixmap::from_png(Cursor::new(std::fs::read(&path).unwrap())).unwrap();
    check("screencopy-output", png);
}

#[test]
fn screencopy_toplevel() {
    let server = Server::start_rendering();
    server
        .with_state(|state| state.headless_set_mode(HEADLESS_OUTPUT_NAME, (320, 240)))
        .expect("changing the mode");
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut back = client.create_toplevel("Back", "org.neueos.Test", (120, 80));
    client.fill(&mut back, 0xffc03020);
    let mut front = client.create_toplevel("Front", "org.neueos.Test", (90, 60));
    client.fill(&mut front, 0xff2040c0);

    // The back window, with the front one on top of it: only the back one is captured.
    let session = client.capture_toplevel(0);
    let frame = client.capture_frame(&session);
    let state = client.wait_for_frame(&frame);
    assert!(matches!(state, FrameState::Ready { .. }), "{state:?}");
    check("screencopy-toplevel", to_pixmap(frame.size, &frame.data()));
}
