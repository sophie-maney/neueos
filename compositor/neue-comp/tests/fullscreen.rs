//! Full-screen windows (task 1.15a, ADR 0018) on the headless backend.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{Anchor, Layer, LayerHandle, Server, TestClient, XdgMode};
use neue_ipc::{ActionParams, WindowInfo};
use smithay::utils::Point;
use wayland_protocols::xdg::shell::client::xdg_toplevel;

const APP: &str = "org.neueos.Test";
const PANEL: i32 = 30;

fn window(server: &Server, title: &str) -> WindowInfo {
    server
        .with_state(|state| state.windows())
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn rect(w: &WindowInfo) -> (i32, i32, i32, i32) {
    (w.x, w.y, w.width, w.height)
}

fn action(server: &Server, name: &str, w: &WindowInfo) {
    server.ipc().action(name, Some(w.id)).unwrap();
}

fn is_fullscreen_configure(client: &mut TestClient, t: &harness::Toplevel) -> bool {
    client.roundtrip();
    client
        .events(t)
        .states
        .contains(&xdg_toplevel::State::Fullscreen)
}

/// A panel along the top edge that reserves its space (`top`), or ignores the others'
/// (`overlay`, so it sits at the very top too).
fn top_panel(client: &mut TestClient, layer: Layer) -> LayerHandle {
    let zone = if layer == Layer::Overlay { -1 } else { PANEL };
    let (w, _) = neue_comp::HEADLESS_OUTPUT_SIZE;
    client.create_layer_surface(layer, "panel", (w, PANEL), |surface| {
        surface.set_size(0, PANEL as u32);
        surface.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
        surface.set_exclusive_zone(zone);
    })
}

/// Whether the surface under `pos` belongs to a layer surface.
fn layer_under(server: &Server, (x, y): (f64, f64)) -> bool {
    server.with_state(move |state| {
        state
            .surface_under(Point::from((x, y)))
            .is_some_and(|(surface, _)| state.layer_for_surface(&surface).is_some())
    })
}

#[test]
fn fullscreen_fills_the_output_and_restores_the_window() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let mut t = client.create_toplevel("Game", APP, (300, 200));
    let before = window(&server, "Game");

    t.toplevel.set_fullscreen(None);
    assert!(is_fullscreen_configure(&mut client, &t));
    assert_eq!(client.events(&t).size, (out_w, out_h));
    client.ack_and_commit(&mut t);
    let full = window(&server, "Game");
    assert!(full.fullscreen);
    assert_eq!(rect(&full), (0, 0, out_w, out_h));

    t.toplevel.unset_fullscreen();
    assert!(!is_fullscreen_configure(&mut client, &t));
    assert_eq!(client.events(&t).size, (300, 200));
    client.ack_and_commit(&mut t);
    let back = window(&server, "Game");
    assert!(!back.fullscreen);
    assert_eq!(rect(&back), rect(&before));

    // The IPC action toggles the same state.
    action(&server, "fullscreen-window", &back);
    client.ack_and_commit(&mut t);
    assert_eq!(rect(&window(&server, "Game")), (0, 0, out_w, out_h));
    action(&server, "fullscreen-window", &back);
    client.ack_and_commit(&mut t);
    assert_eq!(rect(&window(&server, "Game")), rect(&before));
}

#[test]
fn a_window_can_start_full_screen() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let t = client.create_toplevel_with("Game", APP, (300, 200), |t| t.set_fullscreen(None));
    assert!(is_fullscreen_configure(&mut client, &t));
    let info = window(&server, "Game");
    assert!(info.fullscreen && info.focused);
    assert_eq!(rect(&info), (0, 0, out_w, out_h));
}

#[test]
fn a_smaller_full_screen_window_is_centred() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let mut t = client.create_toplevel("Old game", APP, (300, 200));
    t.toplevel.set_fullscreen(None);
    client.ack_and_commit_size(&mut t, (640, 480));
    let info = window(&server, "Old game");
    assert_eq!(
        rect(&info),
        ((out_w - 640) / 2, (out_h - 480) / 2, 640, 480)
    );
}

#[test]
fn full_screen_ignores_panels_and_hides_the_top_layer() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let _panel = top_panel(&mut client, Layer::Top);
    let mut t = client.create_toplevel("Game", APP, (300, 200));
    let at_panel = (100.0, f64::from(PANEL) / 2.0);
    assert!(layer_under(&server, at_panel));

    t.toplevel.set_fullscreen(None);
    client.ack_and_commit(&mut t);
    assert_eq!(rect(&window(&server, "Game")), (0, 0, out_w, out_h));
    // The window is above the top layer now.
    assert!(!layer_under(&server, at_panel));

    // An overlay surface stays above it.
    let _overlay = top_panel(&mut client, Layer::Overlay);
    assert!(layer_under(&server, at_panel));
}

#[test]
fn a_window_above_brings_the_top_layer_back_but_a_dialog_does_not() {
    let server = Server::start();
    let mut client = server.connect();
    let _panel = top_panel(&mut client, Layer::Top);
    let mut game = client.create_toplevel("Game", APP, (300, 200));
    game.toplevel.set_fullscreen(None);
    client.ack_and_commit(&mut game);
    let at_panel = (100.0, f64::from(PANEL) / 2.0);

    // The game's own dialog keeps the panel hidden.
    let parent = game.toplevel.clone();
    let _dialog = client.create_toplevel_with("Settings", APP, (200, 100), |t| {
        t.set_parent(Some(&parent));
    });
    assert!(!layer_under(&server, at_panel));

    // Another app's window on top shows it again.
    let _other = client.create_toplevel("Other", "org.example.Other", (200, 100));
    assert!(layer_under(&server, at_panel));
}

#[test]
fn a_full_screen_window_has_no_title_bar_and_refuses_moves() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let mut t = client.create_decorated_toplevel("Doc", APP, (300, 200), Some(XdgMode::ServerSide));
    t.toplevel.set_fullscreen(None);
    client.ack_and_commit(&mut t);
    let info = window(&server, "Doc");
    // No room is kept for the title bar, and nothing at the top edge is chrome.
    assert_eq!(rect(&info), (0, 0, out_w, out_h));
    let chrome = server.with_state(|state| state.chrome_hit_at(Point::from((100.0, 5.0))));
    assert!(chrome.is_none());

    // Zoom requests are ignored while full screen.
    t.toplevel.set_maximized();
    client.roundtrip();
    let info = window(&server, "Doc");
    assert!(info.fullscreen && !info.zoomed);

    // `move-window` leaves full screen first.
    server
        .ipc()
        .action_with(ActionParams {
            name: "move-window".into(),
            window: Some(info.id),
            x: Some(200),
            y: Some(200),
            ..ActionParams::default()
        })
        .unwrap();
    client.ack_and_commit(&mut t);
    let moved = window(&server, "Doc");
    assert!(!moved.fullscreen);
    assert_eq!((moved.x, moved.y, moved.width), (200, 200, 300));
}

#[test]
fn a_zoomed_window_goes_back_to_zoomed() {
    let server = Server::start();
    let mut client = server.connect();
    let mut t = client.create_toplevel("Doc", APP, (300, 200));
    t.toplevel.set_maximized();
    client.ack_and_commit(&mut t);
    let zoomed = window(&server, "Doc");
    assert!(zoomed.zoomed);

    t.toplevel.set_fullscreen(None);
    client.ack_and_commit(&mut t);
    t.toplevel.unset_fullscreen();
    client.roundtrip();
    let events = client.events(&t).clone();
    assert!(events.states.contains(&xdg_toplevel::State::Maximized));
    assert!(!events.states.contains(&xdg_toplevel::State::Fullscreen));
    client.ack_and_commit(&mut t);
    assert_eq!(rect(&window(&server, "Doc")), rect(&zoomed));
}

#[test]
fn full_screen_follows_its_output() {
    let server = Server::start();
    server
        .with_state(|state| state.headless_add_output("HEADLESS-2", (1280, 800)))
        .unwrap();
    let (first_w, first_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let mut t = client.create_toplevel("Game", APP, (300, 200));

    // On the output the client asks for.
    let second = client.output_global(1);
    t.toplevel.set_fullscreen(Some(&second));
    client.ack_and_commit(&mut t);
    assert_eq!(rect(&window(&server, "Game")), (first_w, 0, 1280, 800));

    // A mode change refits it.
    server
        .with_state(|state| state.headless_set_mode("HEADLESS-2", (1024, 768)))
        .unwrap();
    client.ack_and_commit(&mut t);
    assert_eq!(rect(&window(&server, "Game")), (first_w, 0, 1024, 768));

    // Unplugged: it fills the first output and stays full screen.
    server
        .with_state(|state| state.headless_remove_output("HEADLESS-2"))
        .unwrap();
    assert!(is_fullscreen_configure(&mut client, &t));
    client.ack_and_commit(&mut t);
    let info = window(&server, "Game");
    assert!(info.fullscreen);
    assert_eq!(rect(&info), (0, 0, first_w, first_h));
}
