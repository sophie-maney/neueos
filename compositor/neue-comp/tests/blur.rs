//! Backdrop blur (task 3.4, ADR 0071): `ext-background-effect-v1` and the dual-Kawase
//! pass, read back from the headless GLES renderer. Needs Mesa's software EGL device.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{Server, TestClient, Toplevel};
use neue_comp::{HEADLESS_OUTPUT_NAME, OutputPixels};
use smithay::utils::Rectangle;

const BLACK: u32 = 0xff00_0000;
const WHITE: u32 = 0xffff_ffff;
const RED: u32 = 0xffc0_3020;

/// Where the striped window's client area goes, and its size.
const STRIPES_AT: (i32, i32) = (100, 100);
const STRIPES_SIZE: (i32, i32) = (400, 300);
/// Where the see-through window goes (over the stripes), and its size.
const GLASS_AT: (i32, i32) = (200, 200);
const GLASS_SIZE: (i32, i32) = (200, 100);

/// Black and white vertical stripes, 4 pixels wide.
fn stripe(x: i32, _y: i32) -> u32 {
    if (x / 4) % 2 == 0 { BLACK } else { WHITE }
}

fn render(server: &Server) -> OutputPixels {
    server
        .with_state(|state| state.headless_render(HEADLESS_OUTPUT_NAME))
        .expect("rendering the headless output")
}

fn place(server: &Server, title: &str, at: (i32, i32)) {
    let title = title.to_owned();
    server.with_state(move |state| {
        let id = state
            .windows()
            .into_iter()
            .find(|w| w.title.as_deref() == Some(title.as_str()))
            .expect("window is mapped")
            .id;
        let window = state.window_by_id(id).expect("window is mapped");
        state.move_window_to(&window, at.into());
    });
}

/// A striped window with a fully transparent window over it, the top one asking for blur
/// over all of itself.
struct Scene {
    server: Server,
    client: TestClient,
    stripes: Toplevel,
    glass: Toplevel,
    effect: wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
}

fn scene() -> Scene {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let mut stripes = client.create_toplevel("Stripes", "org.neueos.Test", STRIPES_SIZE);
    client.fill_with(&mut stripes, stripe);
    place(&server, "Stripes", STRIPES_AT);
    // `create_toplevel` attaches a transparent buffer: the window shows only its blur.
    let glass = client.create_toplevel("Glass", "org.neueos.Test", GLASS_SIZE);
    place(&server, "Glass", GLASS_AT);
    let effect = client.background_effect(&glass.surface);
    client.set_blur_region(
        &effect,
        &glass.surface,
        Some(&[(0, 0, GLASS_SIZE.0, GLASS_SIZE.1)]),
    );
    Scene {
        server,
        client,
        stripes,
        glass,
        effect,
    }
}

fn red(argb: u32) -> u8 {
    argb.to_be_bytes()[1]
}

/// The glass window's inside, 8 pixels in from its edges.
fn glass_inside() -> impl Iterator<Item = (i32, i32)> {
    inside(GLASS_AT)
}

/// The inside of a glass window at `(x0, y0)`, 8 pixels in from its edges.
fn inside((x0, y0): (i32, i32)) -> impl Iterator<Item = (i32, i32)> {
    let (w, h) = GLASS_SIZE;
    (y0 + 8..y0 + h - 8).flat_map(move |y| (x0 + 8..x0 + w - 8).map(move |x| (x, y)))
}

/// Blurred stripes in the glass window: grey everywhere, no stripe edges left.
fn assert_blurred(pixels: &OutputPixels) {
    assert_blurred_at(pixels, GLASS_AT);
}

/// Blurred stripes in a glass window at `at`.
fn assert_blurred_at(pixels: &OutputPixels, at: (i32, i32)) {
    for (x, y) in inside(at) {
        let here = red(pixels.pixel(x, y));
        assert!(
            (0x50..=0xb0).contains(&here),
            "({x}, {y}) is {here:#04x}, not a blurred grey"
        );
        let next = red(pixels.pixel(x + 1, y));
        assert!(
            here.abs_diff(next) < 0x10,
            "a stripe edge survives at ({x}, {y}): {here:#04x} then {next:#04x}"
        );
    }
}

/// The stripes as drawn, unblurred, along row `y` from `x0` to `x1`.
fn assert_sharp(pixels: &OutputPixels, y: i32, x0: i32, x1: i32) {
    for x in x0..x1 {
        let expected = stripe(x - STRIPES_AT.0, y - STRIPES_AT.1);
        assert_eq!(
            pixels.pixel(x, y),
            expected,
            "({x}, {y}) should be a sharp stripe"
        );
    }
}

#[test]
fn the_global_offers_blur() {
    let server = Server::start();
    let mut client = server.connect();
    assert_eq!(client.background_effect_capabilities(), Some(1));
}

#[test]
fn the_blur_region_blurs_what_is_behind_it_and_nothing_else() {
    let s = scene();
    let pixels = render(&s.server);
    assert_blurred(&pixels);
    // Right above and below the glass the stripes stay sharp: the blur reads around its
    // region but only draws inside it.
    let (x0, y0) = GLASS_AT;
    let (w, h) = GLASS_SIZE;
    assert_sharp(&pixels, y0 - 1, x0, x0 + w);
    assert_sharp(&pixels, y0 + h, x0, x0 + w);
    assert_sharp(&pixels, y0 - 40, x0, x0 + w);
}

#[test]
fn a_still_backdrop_is_blurred_once_and_reused() {
    let mut s = scene();
    let first = render(&s.server);

    // Nothing changed: nothing is drawn (no extra frames when idle, ADR 0019).
    let idle = render(&s.server);
    assert!(idle.damage.is_empty(), "{:?}", idle.damage);
    assert_eq!(idle.data, first.data);

    // The glass window redraws itself: only it is damaged, and the cached blur fills it
    // again without being redone.
    s.client.fill(&mut s.glass, 0);
    let redrawn = render(&s.server);
    let glass = Rectangle::new(GLASS_AT.into(), GLASS_SIZE.into());
    assert!(!redrawn.damage.is_empty());
    for rect in &redrawn.damage {
        assert!(glass.contains_rect(*rect), "{rect:?} outside {glass:?}");
    }
    assert_eq!(redrawn.data, first.data);
}

#[test]
fn a_change_behind_the_glass_is_blurred_again() {
    let mut s = scene();
    render(&s.server);
    s.client.fill(&mut s.stripes, RED);
    let pixels = render(&s.server);
    for (x, y) in glass_inside() {
        let here = pixels.pixel(x, y);
        let close = here
            .to_be_bytes()
            .iter()
            .zip(RED.to_be_bytes())
            .all(|(a, e)| a.abs_diff(e) <= 2);
        assert!(
            close,
            "({x}, {y}) is {here:#010x}, expected the red behind it"
        );
    }
}

#[test]
fn a_change_just_outside_the_region_is_blurred_in() {
    let mut s = scene();
    render(&s.server);
    // Black above the glass up to 4 pixels from it, stripes below: within the blur's
    // reach, so the glass's top rows darken.
    let edge = GLASS_AT.1 - STRIPES_AT.1 - 4;
    s.client.fill_with(
        &mut s.stripes,
        |x, y| if y < edge { BLACK } else { stripe(x, y) },
    );
    let pixels = render(&s.server);
    let x = GLASS_AT.0 + GLASS_SIZE.0 / 2;
    let top = red(pixels.pixel(x, GLASS_AT.1));
    let middle = red(pixels.pixel(x, GLASS_AT.1 + GLASS_SIZE.1 / 2));
    assert!(top + 0x10 < middle, "top {top:#04x}, middle {middle:#04x}");
}

#[test]
fn removing_the_region_removes_the_blur() {
    let mut s = scene();
    render(&s.server);
    let surface = s.glass.surface.clone();
    s.client.set_blur_region(&s.effect, &surface, None);
    let pixels = render(&s.server);
    let (x0, y0) = GLASS_AT;
    assert_sharp(&pixels, y0 + GLASS_SIZE.1 / 2, x0, x0 + GLASS_SIZE.0);
}

#[test]
fn a_moved_window_takes_its_blur_along() {
    let s = scene();
    render(&s.server);
    let moved = (GLASS_AT.0 + 60, GLASS_AT.1 + 50);
    place(&s.server, "Glass", moved);
    let pixels = render(&s.server);
    assert_blurred_at(&pixels, moved);
    // Where it was and no longer is, the stripes are sharp again.
    let (x0, y0) = GLASS_AT;
    assert_sharp(&pixels, y0 + 10, x0, x0 + GLASS_SIZE.0);
}
