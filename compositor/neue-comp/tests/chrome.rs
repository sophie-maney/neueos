//! Chrome interaction (task 1.11): hit-testing the server-side title bar, traffic-light
//! actions, drag to move, double click and the invisible resize edges, on the headless
//! backend with scripted pointer input.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{Server, TestClient, Toplevel};
use neue_comp::decorations::{Hover, TITLE_BAR_HEIGHT};
use neue_ipc::WindowInfo;
use neue_paint::materials::title_bar::TrafficLight;
use smithay::backend::input::{ButtonState, InputTime};
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::utils::Point;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const APP: &str = "org.neueos.Test";

fn info(server: &Server, title: &str) -> WindowInfo {
    let title = title.to_owned();
    server
        .with_state(move |state| state.windows())
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title.as_str()))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

/// A decorated 300×200 window called `title`, placed with its client at (`x`, `y`).
fn window(server: &Server, client: &mut TestClient, title: &str, (x, y): (i32, i32)) -> Toplevel {
    let t = client.create_decorated_toplevel(title, APP, (300, 200), None);
    let id = info(server, title).id;
    server.with_state(move |state| {
        let window = state.window_by_id(id).expect("window is mapped");
        state.move_window_to(&window, (x, y).into());
    });
    t
}

fn pointer_to(server: &Server, x: f64, y: f64) {
    server.with_state(move |state| state.move_pointer(Point::from((x, y)), InputTime::now()));
}

fn press_at(server: &Server, button: u32, state: ButtonState, time_ms: u32) {
    server.with_state(move |s| s.press_button(button, state, InputTime::from_millis(time_ms)));
}

fn press(server: &Server) {
    press_at(server, BTN_LEFT, ButtonState::Pressed, 1000);
}

fn release(server: &Server) {
    press_at(server, BTN_LEFT, ButtonState::Released, 1010);
}

/// Click at the current pointer position at `time_ms`.
fn click(server: &Server, time_ms: u32) {
    press_at(server, BTN_LEFT, ButtonState::Pressed, time_ms);
    press_at(server, BTN_LEFT, ButtonState::Released, time_ms + 10);
}

/// The centre of `which` on `window`'s title bar.
fn light(window: &WindowInfo, which: TrafficLight) -> (f64, f64) {
    let r = which.rect();
    (
        f64::from(window.x) + r.center().x,
        f64::from(window.y - TITLE_BAR_HEIGHT) + r.center().y,
    )
}

/// A spot on `window`'s title bar clear of the buttons.
fn bar(window: &WindowInfo) -> (f64, f64) {
    (
        f64::from(window.x + window.width / 2),
        f64::from(window.y - TITLE_BAR_HEIGHT / 2),
    )
}

fn cursor(server: &Server) -> CursorImageStatus {
    server.with_state(|state| state.cursor_status.clone())
}

fn hover(server: &Server) -> Option<Hover> {
    server.with_state(|state| state.chrome.hover.clone())
}

#[test]
fn the_pointer_over_the_bar_is_not_over_the_client() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let i = info(&server, "ssd");

    let (x, y) = bar(&i);
    pointer_to(&server, x, y);
    client.roundtrip();
    assert_eq!(client.pointer_enter_serial(), None);

    pointer_to(&server, x, f64::from(i.y + 10));
    client.roundtrip();
    assert!(client.pointer_enter_serial().is_some());
}

#[test]
fn a_press_on_the_bar_never_reaches_the_client() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    click(&server, 1000);
    client.roundtrip();
    assert_eq!(client.pointer_enter_serial(), None);
}

#[test]
fn the_close_light_asks_the_client_to_close() {
    let server = Server::start();
    let mut client = server.connect();
    let t = window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = light(&info(&server, "ssd"), TrafficLight::Close);
    pointer_to(&server, x, y);
    press(&server);
    client.roundtrip();
    assert!(!client.events(&t).close_requested, "closed on press");
    release(&server);
    client.roundtrip();
    assert!(client.events(&t).close_requested);
}

#[test]
fn releasing_away_from_the_light_does_nothing() {
    let server = Server::start();
    let mut client = server.connect();
    let t = window(&server, &mut client, "ssd", (100, 100));
    let i = info(&server, "ssd");
    let (x, y) = light(&i, TrafficLight::Close);
    pointer_to(&server, x, y);
    press(&server);
    assert_eq!(
        hover(&server).map(|h| h.pressed),
        Some(Some(TrafficLight::Close))
    );
    // Dragged off the button: it pops back up.
    pointer_to(&server, x + 150.0, y);
    assert_eq!(hover(&server), None);
    // And down again when the pointer comes back.
    pointer_to(&server, x, y);
    assert_eq!(
        hover(&server).map(|h| h.pressed),
        Some(Some(TrafficLight::Close))
    );
    pointer_to(&server, x + 150.0, y);
    release(&server);
    client.roundtrip();
    assert!(!client.events(&t).close_requested);
    assert_eq!(hover(&server), None);
}

#[test]
fn the_minimize_and_zoom_lights_work() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));

    let (x, y) = light(&info(&server, "ssd"), TrafficLight::Zoom);
    pointer_to(&server, x, y);
    click(&server, 1000);
    assert!(info(&server, "ssd").zoomed);
    // The light is on the zoomed window's bar now; click again to restore.
    let (x, y) = light(&info(&server, "ssd"), TrafficLight::Zoom);
    pointer_to(&server, x, y);
    click(&server, 2000);
    let i = info(&server, "ssd");
    assert!(!i.zoomed);
    assert_eq!((i.x, i.y), (100, 100));

    let (x, y) = light(&i, TrafficLight::Minimize);
    pointer_to(&server, x, y);
    click(&server, 3000);
    assert!(info(&server, "ssd").minimized);
}

#[test]
fn hovering_the_lights_lights_the_group() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let i = info(&server, "ssd");
    let (x, y) = light(&i, TrafficLight::Minimize);
    pointer_to(&server, x, y);
    assert_eq!(hover(&server).map(|h| h.pressed), Some(None));
    let (x, y) = bar(&i);
    pointer_to(&server, x, y);
    assert_eq!(hover(&server), None);
}

#[test]
fn dragging_the_bar_moves_the_window() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    press(&server);
    pointer_to(&server, x + 40.0, y + 30.0);
    pointer_to(&server, x + 60.0, y + 50.0);
    release(&server);
    let i = info(&server, "ssd");
    assert_eq!((i.x, i.y), (160, 150));
}

#[test]
fn a_small_wobble_is_a_click_not_a_drag() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    press(&server);
    pointer_to(&server, x + 1.0, y + 1.0);
    release(&server);
    let i = info(&server, "ssd");
    assert_eq!((i.x, i.y), (100, 100));
}

#[test]
fn dragging_a_zoomed_window_unzooms_it() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = light(&info(&server, "ssd"), TrafficLight::Zoom);
    pointer_to(&server, x, y);
    click(&server, 1000);
    assert!(info(&server, "ssd").zoomed);

    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    press(&server);
    pointer_to(&server, x + 20.0, y + 20.0);
    release(&server);
    let i = info(&server, "ssd");
    assert!(!i.zoomed);
    assert_eq!(i.width, 300);
}

#[test]
fn a_press_on_the_bar_raises_and_focuses_the_window() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "back", (100, 100));
    window(&server, &mut client, "front", (600, 100));
    assert!(info(&server, "front").focused);

    let (x, y) = bar(&info(&server, "back"));
    pointer_to(&server, x, y);
    click(&server, 1000);
    assert!(info(&server, "back").focused);
    assert!(!info(&server, "front").focused);
}

#[test]
fn a_right_click_on_the_bar_focuses_but_does_not_drag() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "back", (100, 100));
    window(&server, &mut client, "front", (600, 100));
    let (x, y) = bar(&info(&server, "back"));
    pointer_to(&server, x, y);
    press_at(&server, BTN_RIGHT, ButtonState::Pressed, 1000);
    pointer_to(&server, x + 50.0, y + 50.0);
    press_at(&server, BTN_RIGHT, ButtonState::Released, 1010);
    let back = info(&server, "back");
    assert!(back.focused);
    assert_eq!((back.x, back.y), (100, 100));
}

#[test]
fn a_double_click_zooms_by_default() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    click(&server, 1000);
    assert!(!info(&server, "ssd").zoomed);
    click(&server, 1200);
    assert!(info(&server, "ssd").zoomed);
}

#[test]
fn two_slow_clicks_are_not_a_double_click() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    click(&server, 1000);
    click(&server, 1600);
    click(&server, 2200);
    assert!(!info(&server, "ssd").zoomed);
}

#[test]
fn the_double_click_action_follows_the_config() {
    let server =
        Server::start_with_config(Some("[windows]\ntitle-bar-double-click = \"minimize\"\n"));
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    click(&server, 1000);
    click(&server, 1200);
    let i = info(&server, "ssd");
    assert!(i.minimized && !i.zoomed);

    let server = Server::start_with_config(Some("[windows]\ntitle-bar-double-click = \"none\"\n"));
    let mut client = server.connect();
    window(&server, &mut client, "ssd", (100, 100));
    let (x, y) = bar(&info(&server, "ssd"));
    pointer_to(&server, x, y);
    click(&server, 1000);
    click(&server, 1200);
    let i = info(&server, "ssd");
    assert!(!i.minimized && !i.zoomed);
}

#[test]
fn the_right_edge_resizes_and_shows_its_cursor() {
    let server = Server::start();
    let mut client = server.connect();
    let t = window(&server, &mut client, "ssd", (100, 100));
    let i = info(&server, "ssd");
    let (x, y) = (f64::from(i.x + i.width + 2), f64::from(i.y + 100));

    pointer_to(&server, x, y);
    assert_eq!(
        cursor(&server),
        CursorImageStatus::Named(CursorIcon::EResize)
    );
    press(&server);
    pointer_to(&server, x + 50.0, y);
    client.roundtrip();
    assert_eq!(client.events(&t).size, (350, 200));
    release(&server);
    // The client follows; the grab is over and the pointer is on the new edge.
    let mut t = t;
    client.ack_and_commit(&mut t);
    assert_eq!(info(&server, "ssd").width, 350);
    pointer_to(&server, x + 51.0, y);
    assert_eq!(
        cursor(&server),
        CursorImageStatus::Named(CursorIcon::EResize)
    );

    // Off the edge, the arrow is back.
    pointer_to(&server, 1900.0, 1000.0);
    assert_eq!(cursor(&server), CursorImageStatus::default_named());
}

#[test]
fn the_top_edge_and_corners_resize_too() {
    let server = Server::start();
    let mut client = server.connect();
    let t = window(&server, &mut client, "ssd", (300, 300));
    let i = info(&server, "ssd");

    // Top edge: just above the bar.
    let (x, y) = (f64::from(i.x + 150), f64::from(i.y - TITLE_BAR_HEIGHT - 2));
    pointer_to(&server, x, y);
    assert_eq!(
        cursor(&server),
        CursorImageStatus::Named(CursorIcon::NResize)
    );
    press(&server);
    pointer_to(&server, x, y - 20.0);
    client.roundtrip();
    assert_eq!(client.events(&t).size, (300, 220));
    release(&server);
    client.ack_and_commit(&mut { t });

    // Bottom-right corner.
    let i = info(&server, "ssd");
    let (x, y) = (f64::from(i.x + i.width + 2), f64::from(i.y + i.height + 2));
    pointer_to(&server, x, y);
    assert_eq!(
        cursor(&server),
        CursorImageStatus::Named(CursorIcon::SeResize)
    );
}

#[test]
fn a_window_in_front_hides_the_chrome_behind_it() {
    let server = Server::start();
    let mut client = server.connect();
    window(&server, &mut client, "behind", (100, 100));
    // `front`'s client area covers the right part of `behind`'s title bar.
    window(&server, &mut client, "front", (150, 60));
    let behind = info(&server, "behind");
    let (x, y) = bar(&behind);
    assert!(info(&server, "front").y <= behind.y - 10 && x > 150.0);
    pointer_to(&server, x, y);
    client.roundtrip();
    assert!(client.pointer_enter_serial().is_some());
    press(&server);
    pointer_to(&server, x + 50.0, y + 50.0);
    release(&server);
    let behind = info(&server, "behind");
    assert_eq!((behind.x, behind.y), (100, 100));
}
