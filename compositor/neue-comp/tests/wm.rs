//! Floating window management (task 1.5): focus, stacking, placement, minimise, zoom,
//! move, resize and xdg-activation, on the headless backend.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::Duration;

use harness::{Server, TestClient};
use neue_ipc::{ActionParams, ErrorCode, Event, Topic, WindowId, WindowInfo};
use smithay::backend::input::{ButtonState, InputTime};
use smithay::utils::Point;
use wayland_protocols::xdg::shell::client::xdg_toplevel;

const BTN_LEFT: u32 = 0x110;
const APP: &str = "org.neueos.Test";

fn windows(server: &Server) -> Vec<WindowInfo> {
    server.with_state(|state| state.windows())
}

fn window(server: &Server, title: &str) -> WindowInfo {
    windows(server)
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

/// Titles bottom to top (minimised windows first), the focused one marked with `*`.
fn stack(server: &Server) -> Vec<String> {
    windows(server)
        .into_iter()
        .map(|w| {
            let title = w.title.unwrap_or_default();
            if w.focused {
                format!("*{title}")
            } else {
                title
            }
        })
        .collect()
}

fn pointer_to(server: &Server, (x, y): (i32, i32)) {
    server.with_state(move |state| {
        state.move_pointer(Point::from((f64::from(x), f64::from(y))), InputTime::now());
    });
}

fn button(server: &Server, state: ButtonState) {
    server.with_state(move |s| s.press_button(BTN_LEFT, state, InputTime::now()));
}

fn click(server: &Server, at: (i32, i32)) {
    pointer_to(server, at);
    button(server, ButtonState::Pressed);
    button(server, ButtonState::Released);
}

fn action(server: &Server, name: &str, window: WindowId) {
    server
        .ipc()
        .action(name, Some(window))
        .unwrap_or_else(|err| panic!("{name} failed: {err}"));
}

/// Press inside `toplevel` and return the serial its client saw, for move/resize.
fn press_on(server: &Server, client: &mut TestClient, info: &WindowInfo) -> u32 {
    pointer_to(server, (info.x + 10, info.y + 10));
    button(server, ButtonState::Pressed);
    client.roundtrip();
    client.button_serial()
}

#[test]
fn clicking_a_window_raises_and_focuses_it() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("A", APP, (300, 200));
    client.create_toplevel("B", APP, (300, 200));
    assert_eq!(stack(&server), ["A", "*B"]);

    // B cascades 24 px down and right of A, so A's top-left corner is still visible.
    let a = window(&server, "A");
    click(&server, (a.x + 5, a.y + 5));
    assert_eq!(stack(&server), ["B", "*A"]);

    // Clicking the background clears focus but keeps the stacking.
    click(&server, (1900, 1070));
    assert_eq!(stack(&server), ["B", "A"]);
}

#[test]
fn placement_centres_cascades_and_remembers() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    client.create_toplevel("First", APP, (300, 200));
    let a = window(&server, "First");
    assert_eq!((a.x, a.y), ((out_w - 300) / 2, (out_h - 200) / 3));

    client.create_toplevel("Second", APP, (300, 200));
    let b = window(&server, "Second");
    assert_eq!((b.x, b.y), (a.x + 24, a.y + 24));

    // Another app starts centred again.
    client.create_toplevel("Other", "org.neueos.Other", (300, 200));
    let other = window(&server, "Other");
    assert_eq!((other.x, other.y), (a.x, a.y));
}

/// Once all of an app's windows are closed, its next window opens where the last one was.
#[test]
fn reopened_app_uses_the_remembered_position() {
    let server = Server::start();
    let mut client = server.connect();
    let toplevel = client.create_toplevel("Doc", APP, (300, 200));
    let id = window(&server, "Doc").id;
    let mut ipc = server.ipc();
    ipc.action_with(ActionParams {
        name: "move-window".into(),
        window: Some(id),
        x: Some(100),
        y: Some(150),
        ..ActionParams::default()
    })
    .unwrap();
    client.destroy_toplevel(toplevel);

    client.create_toplevel("Doc again", APP, (300, 200));
    let again = window(&server, "Doc again");
    assert_eq!((again.x, again.y), (100, 150));
}

#[test]
fn dialogs_stay_above_their_parent_and_return_focus_to_it() {
    let server = Server::start();
    let mut client = server.connect();
    let parent = client.create_toplevel("Parent", APP, (600, 400));
    let dialog = client.create_toplevel_with("Dialog", APP, (200, 100), |t| {
        t.set_parent(Some(&parent.toplevel));
    });
    let mut other = server.connect();
    other.create_toplevel("Other", "org.neueos.Other", (300, 200));
    assert_eq!(stack(&server), ["Parent", "Dialog", "*Other"]);

    // The dialog is centred on its parent and names it.
    let (p, d) = (window(&server, "Parent"), window(&server, "Dialog"));
    assert_eq!((d.x, d.y), (p.x + 200, p.y + 150));
    assert_eq!(d.parent, Some(p.id));

    // Raising the parent brings the dialog along, above it.
    action(&server, "focus-window", p.id);
    assert_eq!(stack(&server), ["Other", "*Parent", "Dialog"]);

    // Closing the focused dialog gives focus back to its parent (raising it), even though
    // another window is above the parent.
    action(&server, "focus-window", window(&server, "Other").id);
    action(&server, "focus-window", d.id);
    assert_eq!(stack(&server), ["Parent", "Other", "*Dialog"]);
    client.destroy_toplevel(dialog);
    assert_eq!(stack(&server), ["Other", "*Parent"]);
}

#[test]
fn minimize_hides_and_restores_in_place() {
    let server = Server::start();
    let mut client = server.connect();
    let a = client.create_toplevel("A", APP, (300, 200));
    client.create_toplevel("B", APP, (300, 200));
    let before = window(&server, "B");

    action(&server, "minimize-window", before.id);
    assert_eq!(stack(&server), ["B", "*A"]);
    let hidden = window(&server, "B");
    assert!(hidden.minimized && !hidden.focused);
    assert_eq!((hidden.x, hidden.y), (before.x, before.y));
    // A minimised window is not under the pointer.
    server.with_state(move |state| {
        let point = Point::from((f64::from(before.x + 250), f64::from(before.y + 190)));
        assert!(state.surface_under(point).is_none());
    });

    action(&server, "unminimize-window", before.id);
    assert_eq!(stack(&server), ["A", "*B"]);
    let shown = window(&server, "B");
    assert!(!shown.minimized);
    assert_eq!((shown.x, shown.y), (before.x, before.y));

    // Clients can minimise themselves; `focus-window` restores.
    a.toplevel.set_minimized();
    client.roundtrip();
    assert!(window(&server, "A").minimized);
    action(&server, "focus-window", window(&server, "A").id);
    assert_eq!(stack(&server), ["B", "*A"]);
}

#[test]
fn zoom_fills_the_work_area_and_toggles_back() {
    let server = Server::start();
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let mut client = server.connect();
    let mut t = client.create_toplevel("Doc", APP, (300, 200));
    let before = window(&server, "Doc");

    action(&server, "zoom-window", before.id);
    client.roundtrip();
    let events = client.events(&t).clone();
    assert!(events.states.contains(&xdg_toplevel::State::Maximized));
    assert_eq!(events.size, (out_w, out_h));
    client.ack_and_commit(&mut t);
    let zoomed = window(&server, "Doc");
    assert!(zoomed.zoomed);
    assert_eq!(
        (zoomed.x, zoomed.y, zoomed.width, zoomed.height),
        (0, 0, out_w, out_h)
    );

    action(&server, "zoom-window", before.id);
    client.roundtrip();
    let events = client.events(&t).clone();
    assert!(!events.states.contains(&xdg_toplevel::State::Maximized));
    assert_eq!(events.size, (300, 200));
    client.ack_and_commit(&mut t);
    let back = window(&server, "Doc");
    assert!(!back.zoomed);
    assert_eq!(
        (back.x, back.y, back.width, back.height),
        (before.x, before.y, 300, 200)
    );

    // The client's own maximize request does the same.
    t.toplevel.set_maximized();
    client.ack_and_commit(&mut t);
    assert!(window(&server, "Doc").zoomed);
}

#[test]
fn move_and_resize_actions() {
    let server = Server::start();
    let mut client = server.connect();
    let mut t = client.create_toplevel("Doc", APP, (300, 200));
    let id = window(&server, "Doc").id;
    let mut ipc = server.ipc();

    // The top edge can't go above the work area.
    ipc.action_with(ActionParams {
        name: "move-window".into(),
        x: Some(-40),
        y: Some(-50),
        ..ActionParams::default()
    })
    .unwrap();
    let moved = window(&server, "Doc");
    assert_eq!((moved.x, moved.y), (-40, 0));

    ipc.action_with(ActionParams {
        name: "resize-window".into(),
        window: Some(id),
        width: Some(400),
        height: Some(300),
        ..ActionParams::default()
    })
    .unwrap();
    client.roundtrip();
    assert_eq!(client.events(&t).size, (400, 300));
    client.ack_and_commit(&mut t);
    let resized = window(&server, "Doc");
    assert_eq!(
        (resized.x, resized.y, resized.width, resized.height),
        (-40, 0, 400, 300)
    );

    // Missing arguments are reported.
    let err = ipc
        .action_with(ActionParams {
            name: "move-window".into(),
            x: Some(1),
            ..ActionParams::default()
        })
        .unwrap_err();
    assert!(
        matches!(err, neue_ipc::ClientError::Remote(ref body) if body.code == ErrorCode::InvalidParams),
        "{err:?}"
    );
}

#[test]
fn interactive_move_follows_the_pointer() {
    let server = Server::start();
    let mut client = server.connect();
    let t = client.create_toplevel("Doc", APP, (300, 200));
    let start = window(&server, "Doc");

    let serial = press_on(&server, &mut client, &start);
    t.toplevel._move(&client.seat, serial);
    client.roundtrip();
    pointer_to(&server, (start.x + 10 + 100, start.y + 10 + 50));
    pointer_to(&server, (start.x + 10 + 150, start.y + 10 - 2000));
    let during = window(&server, "Doc");
    // The title bar stays below the top of the work area.
    assert_eq!((during.x, during.y), (start.x + 150, 0));
    pointer_to(&server, (start.x + 10 + 100, start.y + 10 + 50));
    button(&server, ButtonState::Released);

    let end = window(&server, "Doc");
    assert_eq!((end.x, end.y), (start.x + 100, start.y + 50));
}

#[test]
fn interactive_resize_from_the_top_left_keeps_the_bottom_right() {
    let server = Server::start();
    let mut client = server.connect();
    let mut t = client.create_toplevel("Doc", APP, (300, 200));
    let start = window(&server, "Doc");

    let serial = press_on(&server, &mut client, &start);
    t.toplevel
        .resize(&client.seat, serial, xdg_toplevel::ResizeEdge::TopLeft);
    client.roundtrip();
    pointer_to(&server, (start.x + 10 - 40, start.y + 10 - 20));
    client.roundtrip();
    let events = client.events(&t).clone();
    assert!(events.states.contains(&xdg_toplevel::State::Resizing));
    assert_eq!(events.size, (340, 220));
    client.ack_and_commit(&mut t);
    let during = window(&server, "Doc");
    assert_eq!(
        (during.x, during.y, during.width, during.height),
        (start.x - 40, start.y - 20, 340, 220)
    );

    button(&server, ButtonState::Released);
    client.roundtrip();
    assert!(
        !client
            .events(&t)
            .states
            .contains(&xdg_toplevel::State::Resizing)
    );
    client.ack_and_commit(&mut t);
    let end = window(&server, "Doc");
    assert_eq!(
        (end.x + end.width, end.y + end.height),
        (start.x + 300, start.y + 200)
    );
}

#[test]
fn activation_needs_a_token_from_the_focused_client() {
    let server = Server::start();
    let mut a = server.connect();
    let a_window = a.create_toplevel("A", "org.neueos.A", (300, 200));
    let mut b = server.connect();
    b.create_toplevel("B", "org.neueos.B", (300, 200));
    assert_eq!(stack(&server), ["A", "*B"]);

    // A is not focused, so its own token can't steal focus; it asks for attention instead.
    let token = a.activation_token(None, Some(&a_window.surface));
    a.activate(&a_window, &token);
    assert_eq!(stack(&server), ["A", "*B"]);
    assert!(window(&server, "A").demands_attention);

    // B has focus and passes a token to A (as a launcher or a link handler would).
    b.roundtrip();
    let serial = b.enter_serial();
    let token = b.activation_token(serial, None);
    a.activate(&a_window, &token);
    assert_eq!(stack(&server), ["B", "*A"]);
    assert!(!window(&server, "A").demands_attention);

    // Tokens are single-use.
    action(&server, "focus-window", window(&server, "B").id);
    a.activate(&a_window, &token);
    assert_eq!(stack(&server), ["A", "*B"]);
}

#[test]
fn window_changes_are_reported() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("A", APP, (300, 200));
    let id = window(&server, "A").id;
    let mut events = server.ipc().subscribe(vec![Topic::Windows]).unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();

    action(&server, "minimize-window", id);
    let changed = loop {
        match events.next_event().unwrap().expect("event stream ended") {
            Event::WindowChanged(info) => break info,
            Event::FocusChanged { .. } => {}
            other => panic!("unexpected event {other:?}"),
        }
    };
    assert_eq!(changed.id, id);
    assert!(changed.minimized);
}

#[test]
fn window_actions_are_listed_with_their_arguments() {
    let server = Server::start();
    let actions = server.ipc().actions().unwrap();
    let args = |name: &str| {
        actions
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .args
            .clone()
    };
    for name in ["minimize-window", "unminimize-window", "zoom-window"] {
        assert!(args(name).is_empty());
    }
    assert_eq!(args("move-window"), ["x", "y"]);
    assert_eq!(args("resize-window"), ["width", "height"]);
}
