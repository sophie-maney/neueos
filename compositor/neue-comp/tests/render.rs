//! The headless backend's offscreen GLES renderer (task 1.16a, ADR 0021). Needs Mesa's
//! software EGL device; CI installs it.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::Server;
use neue_comp::{HEADLESS_OUTPUT_NAME, HEADLESS_OUTPUT_SIZE, OutputPixels, WindowInfo};

/// The clear colour (`render::CLEAR_COLOR`, 0.18/0.2/0.24) as 8-bit ARGB.
const BACKGROUND: u32 = 0xff2e333d;
const RED: u32 = 0xffc03020;
const BLUE: u32 = 0xff2040c0;

fn render(server: &Server) -> OutputPixels {
    server
        .with_state(|state| state.headless_render(HEADLESS_OUTPUT_NAME))
        .expect("rendering the headless output")
}

fn only_window(server: &Server) -> WindowInfo {
    let windows = server.with_state(|state| state.windows());
    assert_eq!(windows.len(), 1, "{windows:#?}");
    windows.into_iter().next().expect("one window")
}

/// Each channel of `actual` within 1 of `expected` (GL may round the clear colour
/// either way).
fn assert_close(actual: u32, expected: u32, what: &str) {
    let close = actual
        .to_be_bytes()
        .iter()
        .zip(expected.to_be_bytes())
        .all(|(a, e)| a.abs_diff(e) <= 1);
    assert!(close, "{what}: {actual:#010x}, expected {expected:#010x}");
}

#[test]
fn shm_window_is_drawn_where_it_is() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let mut toplevel = client.create_toplevel("Red", "org.neueos.Test", (100, 60));
    client.fill(&mut toplevel, RED);
    let window = only_window(&server);
    assert_eq!((window.width, window.height), (100, 60));
    let (x, y) = (window.x, window.y);

    let pixels = render(&server);
    assert_eq!(pixels.size, HEADLESS_OUTPUT_SIZE);
    let (w, h) = pixels.size;
    assert_eq!(pixels.data.len(), (w * h * 4) as usize);

    // The window covers exactly its rectangle, corners included.
    for (px, py) in [
        (x, y),
        (x + 99, y),
        (x, y + 59),
        (x + 99, y + 59),
        (x + 50, y + 30),
    ] {
        assert_eq!(pixels.pixel(px, py), RED, "window pixel ({px}, {py})");
    }
    // The background is around it. The window sits in the top half, so the mirrored
    // point catches an upside-down read-back.
    assert!(y + 60 < h / 2, "{window:#?}");
    assert_close(
        pixels.pixel(x + 50, h - 1 - (y + 30)),
        BACKGROUND,
        "mirrored",
    );
    assert_close(
        pixels.pixel(w - 1, h - 1),
        BACKGROUND,
        "bottom-right corner",
    );
    assert_close(
        pixels.pixel(x + 150, y + 30),
        BACKGROUND,
        "right of the window",
    );
}

#[test]
fn damage_follows_changes() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let mut toplevel = client.create_toplevel("Colours", "org.neueos.Test", (80, 40));
    client.fill(&mut toplevel, RED);
    let window = only_window(&server);
    let (x, y) = (window.x, window.y);

    // The first render draws everything.
    let first = render(&server);
    let (w, h) = first.size;
    assert_eq!(first.damage.len(), 1, "{:?}", first.damage);
    assert_eq!(first.damage[0].size, (w, h).into());

    // Nothing changed: nothing damaged, same pixels.
    let second = render(&server);
    assert!(second.damage.is_empty(), "{:?}", second.damage);
    assert_eq!(second.data, first.data);

    // A new buffer damages the window and only the window.
    client.fill(&mut toplevel, BLUE);
    let third = render(&server);
    assert!(!third.damage.is_empty());
    let window_rect = smithay::utils::Rectangle::new((x, y).into(), (80, 40).into());
    for rect in &third.damage {
        assert!(
            window_rect.contains_rect(*rect),
            "{rect:?} outside {window_rect:?}"
        );
    }
    assert_eq!(third.pixel(x + 40, y + 20), BLUE);
}

#[test]
fn resized_output_renders_at_its_new_size() {
    let server = Server::start_rendering();
    render(&server);
    server
        .with_state(|state| state.headless_set_mode(HEADLESS_OUTPUT_NAME, (640, 480)))
        .expect("changing the mode");
    let pixels = render(&server);
    assert_eq!(pixels.size, (640, 480));
    assert_close(pixels.pixel(639, 479), BACKGROUND, "bottom-right corner");
}

#[test]
fn rendering_needs_the_renderer() {
    let server = Server::start();
    let result = server.with_state(|state| state.headless_render(HEADLESS_OUTPUT_NAME));
    assert!(result.is_err(), "{result:?}");
}
