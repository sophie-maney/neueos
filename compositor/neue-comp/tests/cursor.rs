//! Cursors (task 1.9): `cursor-shape-v1` and the cursor the compositor picks during an
//! interactive resize, on the headless backend. Drawing (XCursor themes, scales, the
//! cursor plane) happens in the tty backend and is checked in the VM.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::Server;
use neue_ipc::WindowInfo;
use smithay::backend::input::{ButtonState, InputTime};
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use smithay::utils::Point;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;
use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;

const BTN_LEFT: u32 = 0x110;
const APP: &str = "org.neueos.Test";

fn cursor(server: &Server) -> CursorImageStatus {
    server.with_state(|state| state.cursor_status.clone())
}

fn window(server: &Server, title: &str) -> WindowInfo {
    server
        .with_state(|state| state.windows())
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn pointer_to(server: &Server, (x, y): (i32, i32)) {
    server.with_state(move |state| {
        state.move_pointer(Point::from((f64::from(x), f64::from(y))), InputTime::now());
    });
}

fn button(server: &Server, state: ButtonState) {
    server.with_state(move |s| s.press_button(BTN_LEFT, state, InputTime::now()));
}

#[test]
fn the_global_is_advertised() {
    let server = Server::start();
    let client = server.connect();
    assert!(client.has_global("wp_cursor_shape_manager_v1"));
}

#[test]
fn a_client_names_its_cursor() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("Doc", APP, (300, 200));
    let info = window(&server, "Doc");

    pointer_to(&server, (info.x + 50, info.y + 50));
    client.roundtrip();
    assert_eq!(cursor(&server), CursorImageStatus::default_named());

    client.set_cursor_shape(Shape::Text);
    assert_eq!(cursor(&server), CursorImageStatus::Named(CursorIcon::Text));

    // Leaving the window puts the arrow back.
    pointer_to(&server, (1900, 1070));
    client.roundtrip();
    assert_eq!(cursor(&server), CursorImageStatus::default_named());
}

#[test]
fn a_stale_serial_is_ignored() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("Doc", APP, (300, 200));
    let info = window(&server, "Doc");
    pointer_to(&server, (info.x + 50, info.y + 50));
    client.roundtrip();
    let serial = client.pointer_enter_serial().unwrap();

    // The pointer leaves and comes back: the first enter's serial no longer counts.
    pointer_to(&server, (1900, 1070));
    pointer_to(&server, (info.x + 60, info.y + 60));
    client.roundtrip();
    assert_ne!(client.pointer_enter_serial(), Some(serial));
    client.set_cursor_shape_with(serial, Shape::Wait);
    assert_eq!(cursor(&server), CursorImageStatus::default_named());
}

#[test]
fn a_resize_shows_the_edge_cursor() {
    let server = Server::start();
    let mut client = server.connect();
    let t = client.create_toplevel("Doc", APP, (300, 200));
    let info = window(&server, "Doc");
    pointer_to(&server, (info.x + 10, info.y + 10));
    button(&server, ButtonState::Pressed);
    client.roundtrip();
    // The client asked for a text cursor; the grab overrides it.
    client.set_cursor_shape(Shape::Text);
    t.toplevel
        .resize(&client.seat, client.button_serial(), ResizeEdge::TopLeft);
    client.roundtrip();
    pointer_to(&server, (info.x + 20, info.y + 20));
    client.roundtrip();
    assert_eq!(
        cursor(&server),
        CursorImageStatus::Named(CursorIcon::NwResize)
    );

    // The client cannot override it while the grab runs.
    client.set_cursor_shape(Shape::Pointer);
    assert_eq!(
        cursor(&server),
        CursorImageStatus::Named(CursorIcon::NwResize)
    );

    button(&server, ButtonState::Released);
    client.roundtrip();
    assert_eq!(cursor(&server), CursorImageStatus::default_named());
}
