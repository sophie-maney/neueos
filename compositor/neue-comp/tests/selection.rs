//! Clipboard, primary selection, data-control and drag-and-drop (task 1.8) on the headless
//! backend. `set_*`/`request_*` mirror what `wl-copy` and `wl-paste` do on the wire.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{Server, TestClient, Toplevel, read_all};
use neue_ipc::WindowInfo;
use smithay::backend::input::{ButtonState, InputTime, KeyState};
use smithay::input::keyboard::Keycode;
use smithay::utils::Point;

const BTN_LEFT: u32 = 0x110;
const APP: &str = "org.neueos.Test";
const SIZE: (i32, i32) = (200, 150);

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

/// A point that only window `title` covers however the two windows are stacked: A's
/// top-left corner, B's bottom-right one (B is placed after A, down and to the right).
fn exclusive_point(info: &WindowInfo) -> (i32, i32) {
    if info.title.as_deref() == Some("A") {
        (info.x + 10, info.y + 10)
    } else {
        (info.x + info.width - 5, info.y + info.height - 5)
    }
}

/// Click window `title`, which focuses it, and return its client's keyboard-enter
/// serial.
fn focus(server: &Server, client: &mut TestClient, title: &str) -> u32 {
    let info = window(server, title);
    pointer_to(server, exclusive_point(&info));
    button(server, ButtonState::Pressed);
    button(server, ButtonState::Released);
    client.roundtrip();
    assert!(window(server, title).focused, "{title} did not get focus");
    client.enter_serial().expect("no keyboard enter")
}

struct Pair {
    server: Server,
    a: TestClient,
    b: TestClient,
    a_window: Toplevel,
    /// Kept alive so B's window stays mapped.
    _b_window: Toplevel,
}

/// Two clients with one window each; "B" is the newer one.
fn pair() -> Pair {
    let server = Server::start();
    let mut a = server.connect();
    let a_window = a.create_toplevel("A", APP, SIZE);
    let mut b = server.connect();
    let b_window = b.create_toplevel("B", APP, SIZE);
    Pair {
        server,
        a,
        b,
        a_window,
        _b_window: b_window,
    }
}

#[test]
fn clipboard_round_trips_to_the_next_focused_client() {
    let Pair {
        server,
        mut a,
        mut b,
        ..
    } = pair();
    let serial = focus(&server, &mut a, "A");
    a.set_clipboard(serial, "text/plain", b"hello clipboard");

    // B has no focus yet, so it is offered nothing.
    assert!(!b.has_clipboard());

    focus(&server, &mut b, "B");
    let pipe = b
        .request_clipboard("text/plain")
        .expect("no clipboard offer");
    a.roundtrip(); // the owner writes when the compositor forwards the request
    assert_eq!(read_all(pipe), b"hello clipboard");
    assert!(b.offered_mime_types().iter().any(|m| m == "text/plain"));
}

#[test]
fn a_new_clipboard_owner_cancels_the_old_source() {
    let Pair {
        server,
        mut a,
        mut b,
        ..
    } = pair();
    let serial = focus(&server, &mut a, "A");
    a.set_clipboard(serial, "text/plain", b"first");
    let serial = focus(&server, &mut b, "B");
    b.set_clipboard(serial, "text/plain", b"second");
    a.roundtrip();
    assert_eq!(a.cancelled_sources(), 1);

    focus(&server, &mut a, "A");
    let pipe = a
        .request_clipboard("text/plain")
        .expect("no clipboard offer");
    b.roundtrip();
    assert_eq!(read_all(pipe), b"second");
}

#[test]
fn data_control_reads_and_writes_without_focus() {
    let Pair {
        server,
        mut a,
        mut b,
        ..
    } = pair();
    let mut manager = server.connect(); // `wl-paste` and `wl-copy`: no window, no focus

    // wl-paste sees what a focused app copied.
    let serial = focus(&server, &mut a, "A");
    a.set_clipboard(serial, "text/plain", b"from an app");
    let pipe = manager
        .control_paste(false, "text/plain")
        .expect("no selection through data-control");
    a.roundtrip();
    assert_eq!(read_all(pipe), b"from an app");

    // wl-copy replaces it, and the next focused app pastes it.
    manager.control_copy(false, "text/plain", b"from wl-copy");
    a.roundtrip();
    assert_eq!(a.cancelled_sources(), 1);
    focus(&server, &mut b, "B");
    let pipe = b
        .request_clipboard("text/plain")
        .expect("no clipboard offer");
    manager.roundtrip();
    assert_eq!(read_all(pipe), b"from wl-copy");
}

#[test]
fn primary_selection_round_trips() {
    let Pair {
        server,
        mut a,
        mut b,
        ..
    } = pair();
    let mut manager = server.connect();

    let serial = focus(&server, &mut a, "A");
    a.set_primary(serial, "text/plain", b"selected text");
    assert!(b.request_primary("text/plain").is_none(), "B is unfocused");

    focus(&server, &mut b, "B");
    let pipe = b.request_primary("text/plain").expect("no primary offer");
    a.roundtrip();
    assert_eq!(read_all(pipe), b"selected text");

    // Through data-control (`wl-paste --primary`) too.
    let pipe = manager
        .control_paste(true, "text/plain")
        .expect("no primary through data-control");
    a.roundtrip();
    assert_eq!(read_all(pipe), b"selected text");
}

#[test]
fn drag_icon_lives_for_the_drag_and_follows_its_offset() {
    let Pair {
        server,
        mut a,
        a_window,
        ..
    } = pair();
    let info = window(&server, "A");
    pointer_to(&server, (info.x + 10, info.y + 10));
    button(&server, ButtonState::Pressed);
    a.roundtrip();
    let serial = a.button_serial();

    let drag = a.start_drag(&a_window.surface, serial, true);
    assert!(
        server.with_state(|s| s.dnd_icon.is_some()),
        "no icon while dragging"
    );

    // The icon's own offset moves with its commits.
    let icon = drag.icon.as_ref().unwrap();
    icon.offset(-4, -3);
    icon.commit();
    a.roundtrip();
    let offset = server.with_state(|s| s.dnd_icon.as_ref().map(|i| i.offset));
    assert_eq!(offset, Some(Point::from((-4, -3))));

    button(&server, ButtonState::Released);
    assert!(
        server.with_state(|s| s.dnd_icon.is_none()),
        "icon outlived the drag"
    );
}

#[test]
fn a_drag_without_an_icon_has_none() {
    let Pair {
        server,
        mut a,
        a_window,
        ..
    } = pair();
    let info = window(&server, "A");
    pointer_to(&server, (info.x + 10, info.y + 10));
    button(&server, ButtonState::Pressed);
    a.roundtrip();
    let serial = a.button_serial();
    let _drag = a.start_drag(&a_window.surface, serial, false);
    assert!(server.with_state(|s| s.dnd_icon.is_none()));
    button(&server, ButtonState::Released);
}

#[test]
fn a_file_can_be_dragged_between_clients() {
    let Pair {
        server,
        mut a,
        mut b,
        a_window,
        ..
    } = pair();
    let from = window(&server, "A");
    let to = window(&server, "B");

    pointer_to(&server, (from.x + 10, from.y + 10));
    button(&server, ButtonState::Pressed);
    a.roundtrip();
    let serial = a.button_serial();
    let _drag = a.start_drag(&a_window.surface, serial, true);
    assert!(server.with_state(|s| s.dnd_icon.is_some()));

    // Over B's bottom-right corner, clear of A's window.
    pointer_to(&server, exclusive_point(&to));
    b.roundtrip();
    assert_eq!(b.dnd_target_events().0, 1, "B was not entered");
    b.roundtrip(); // sends the accept and action choice B made on entry

    button(&server, ButtonState::Released);
    b.roundtrip(); // receives the drop and answers with `finish`
    b.roundtrip(); // flushes the `finish`
    a.roundtrip();
    assert_eq!(b.dnd_target_events(), (1, 1), "B did not get the drop");
    assert_eq!(
        a.dnd_source_events(),
        (1, 1),
        "A did not see the drop finish"
    );
    assert!(server.with_state(|s| s.dnd_icon.is_none()));
}

#[test]
fn escape_cancels_a_drag_and_clients_never_see_it() {
    let Pair {
        server,
        mut a,
        mut b,
        a_window,
        ..
    } = pair();
    let from = window(&server, "A");
    let to = window(&server, "B");
    pointer_to(&server, (from.x + 10, from.y + 10));
    button(&server, ButtonState::Pressed);
    a.roundtrip();
    let serial = a.button_serial();
    let _drag = a.start_drag(&a_window.surface, serial, true);
    pointer_to(&server, exclusive_point(&to));
    b.roundtrip();
    assert!(server.with_state(|s| s.dnd_icon.is_some()));

    // Escape is evdev KEY_ESC (1) + 8.
    let esc = Keycode::new(1 + 8);
    server.with_state(move |s| s.press_key(esc, KeyState::Pressed, InputTime::now()));
    server.with_state(move |s| s.press_key(esc, KeyState::Released, InputTime::now()));
    a.roundtrip();
    b.roundtrip();

    assert!(
        server.with_state(|s| s.dnd_icon.is_none()),
        "icon survived Escape"
    );
    assert!(!server.with_state(|s| s.dnd_active));
    assert_eq!(a.cancelled_sources(), 1, "the source was not cancelled");
    assert_eq!(b.dnd_target_events().1, 0, "the cancelled drag was dropped");
    assert_eq!(a.key_presses(), 0, "Escape reached the dragging client");

    // The button release afterwards drops nothing.
    button(&server, ButtonState::Released);
    b.roundtrip();
    assert_eq!(b.dnd_target_events().1, 0);
}
