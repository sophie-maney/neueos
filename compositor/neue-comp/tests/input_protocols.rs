//! The input protocols (task 1.13, ADR 0016) on the headless backend: pointer constraints,
//! relative pointer, pointer gestures, text-input-v3 and input-method-v2.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{GestureSeen, Server, TestClient, Toplevel};
use neue_comp::input_protocols::Gesture;
use neue_ipc::WindowInfo;
use smithay::backend::input::{ButtonState, InputTime};
use smithay::desktop::{PopupKind, PopupManager};

const APP: &str = "org.neueos.Test";
const BTN_LEFT: u32 = 0x110;

fn window(server: &Server, title: &str) -> WindowInfo {
    server
        .with_state(|state| state.windows())
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn pointer_to(server: &Server, x: f64, y: f64) {
    server.with_state(move |s| s.move_pointer((x, y).into(), InputTime::now()));
}

fn pointer(server: &Server) -> (f64, f64) {
    server.with_state(|s| {
        let p = s.pointer.current_location();
        (p.x, p.y)
    })
}

fn move_by(server: &Server, dx: f64, dy: f64) {
    server.with_state(move |s| {
        s.move_pointer_by(
            (dx, dy).into(),
            (dx * 2.0, dy * 2.0).into(),
            InputTime::now(),
        );
    });
}

/// Press and release the left button where the pointer is: focuses the window under it.
fn click(server: &Server) {
    server.with_state(|s| {
        s.press_button(BTN_LEFT, ButtonState::Pressed, InputTime::now());
        s.press_button(BTN_LEFT, ButtonState::Released, InputTime::now());
    });
}

/// A 300×200 window, focused, with the pointer at `(dx, dy)` inside it.
fn focused_window(
    server: &Server,
    client: &mut TestClient,
    title: &str,
    (dx, dy): (f64, f64),
) -> (Toplevel, WindowInfo) {
    let toplevel = client.create_toplevel(title, APP, (300, 200));
    let info = window(server, title);
    pointer_to(server, f64::from(info.x) + dx, f64::from(info.y) + dy);
    click(server);
    client.roundtrip();
    (toplevel, info)
}

#[test]
fn input_protocols_are_advertised() {
    let server = Server::start();
    let client = server.connect();
    for global in [
        "zwp_pointer_constraints_v1",
        "zwp_relative_pointer_manager_v1",
        "zwp_pointer_gestures_v1",
        "zwp_text_input_manager_v3",
        "zwp_input_method_manager_v2",
    ] {
        assert!(client.has_global(global), "{global} is not advertised");
    }
}

// ---- relative-pointer ----

#[test]
fn relative_motion_reaches_the_focused_client_and_moves_the_pointer() {
    let server = Server::start();
    let mut client = server.connect();
    let _win = focused_window(&server, &mut client, "game", (100.0, 100.0));
    client.track_relative_pointer();
    let (x, y) = pointer(&server);

    move_by(&server, 3.0, 4.0);
    client.roundtrip();
    assert_eq!(client.relative_motions(), &[[3.0, 4.0, 6.0, 8.0]]);
    assert_eq!(pointer(&server), (x + 3.0, y + 4.0));
}

#[test]
fn relative_motion_goes_only_to_the_client_under_the_pointer() {
    let server = Server::start();
    let mut under = server.connect();
    let mut other = server.connect();
    let _win = focused_window(&server, &mut under, "under", (100.0, 100.0));
    under.track_relative_pointer();
    other.track_relative_pointer();

    move_by(&server, 1.0, 1.0);
    under.roundtrip();
    other.roundtrip();
    assert_eq!(under.relative_motions().len(), 1);
    assert!(other.relative_motions().is_empty());
}

// ---- pointer-constraints ----

#[test]
fn a_locked_pointer_stays_put_and_still_gets_relative_motion() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, _) = focused_window(&server, &mut client, "game", (100.0, 100.0));
    client.track_relative_pointer();
    let lock = client.lock_pointer(&win, None, false);
    let events = client.constraint_events(&lock);
    assert!(events.active, "locked at once: the window has both foci");
    assert_eq!(events.activations, 1);

    let at = pointer(&server);
    move_by(&server, 40.0, 25.0);
    client.roundtrip();
    assert_eq!(pointer(&server), at, "the pointer does not move");
    assert_eq!(client.relative_motions(), &[[40.0, 25.0, 80.0, 50.0]]);

    // Absolute motion (a tablet, a VM pointer) is ignored while locked.
    pointer_to_via_input(&server, at.0 + 50.0, at.1);
    assert_eq!(pointer(&server), at);

    // Destroying the lock frees the pointer.
    lock.destroy();
    client.roundtrip();
    move_by(&server, 10.0, 0.0);
    assert_eq!(pointer(&server), (at.0 + 10.0, at.1));
}

/// The absolute-motion path of a backend: constraints apply, unlike `move_pointer`.
fn pointer_to_via_input(server: &Server, x: f64, y: f64) {
    server.with_state(move |s| s.move_pointer_towards((x, y).into(), InputTime::now()));
}

#[test]
fn a_lock_waits_for_the_pointer_and_keyboard_focus() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, info) = focused_window(&server, &mut client, "game", (100.0, 100.0));
    // The pointer is away from the window: nothing to lock yet.
    pointer_to(&server, 1900.0, 1000.0);
    let lock = client.lock_pointer(&win, None, false);
    assert!(!client.constraint_events(&lock).active);

    // Entering the window activates it.
    pointer_to(&server, f64::from(info.x) + 50.0, f64::from(info.y) + 50.0);
    client.roundtrip();
    assert!(client.constraint_events(&lock).active);

    // Another window taking the keyboard releases it.
    let mut other = server.connect();
    let _other = other.create_toplevel("other", APP, (50, 50));
    let other_info = window(&server, "other");
    server.with_state(move |s| {
        let w = s
            .windows()
            .into_iter()
            .find(|w| w.id == other_info.id)
            .unwrap();
        let window = s.window_by_id(w.id).unwrap();
        s.focus_window(&window);
    });
    client.roundtrip();
    let events = client.constraint_events(&lock);
    assert!(!events.active, "the keyboard moved to another window");
    assert_eq!(events.deactivations, 1);
}

#[test]
fn a_oneshot_lock_ends_when_it_is_deactivated() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, info) = focused_window(&server, &mut client, "game", (100.0, 100.0));
    let lock = client.lock_pointer(&win, None, true);
    assert!(client.constraint_events(&lock).active);

    // Focus leaves the window: the lock ends and does not come back.
    server.with_state(|s| s.clear_focus());
    client.roundtrip();
    assert_eq!(client.constraint_events(&lock).deactivations, 1);
    pointer_to(&server, f64::from(info.x) + 20.0, f64::from(info.y) + 20.0);
    click(&server);
    client.roundtrip();
    assert_eq!(client.constraint_events(&lock).activations, 1, "not again");
}

#[test]
fn a_lock_with_a_region_only_starts_inside_it() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, info) = focused_window(&server, &mut client, "game", (10.0, 10.0));
    let lock = client.lock_pointer(&win, Some((100, 100, 50, 50)), false);
    assert!(
        !client.constraint_events(&lock).active,
        "pointer is outside"
    );

    pointer_to(
        &server,
        f64::from(info.x) + 120.0,
        f64::from(info.y) + 120.0,
    );
    client.roundtrip();
    assert!(client.constraint_events(&lock).active);
}

#[test]
fn the_cursor_position_hint_places_the_pointer_when_the_lock_ends() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, info) = focused_window(&server, &mut client, "game", (100.0, 100.0));
    let lock = client.lock_pointer(&win, None, false);
    assert!(client.constraint_events(&lock).active);

    lock.set_cursor_position_hint(20.0, 30.0);
    win.surface.commit();
    client.roundtrip();
    lock.destroy();
    client.roundtrip();
    assert_eq!(
        pointer(&server),
        (f64::from(info.x) + 20.0, f64::from(info.y) + 30.0)
    );
}

#[test]
fn a_confined_pointer_stops_at_the_edge_of_the_window() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, info) = focused_window(&server, &mut client, "game", (100.0, 100.0));
    let confine = client.confine_pointer(&win, None, false);
    assert!(client.constraint_events(&confine).active);

    move_by(&server, 5000.0, 0.0);
    let (x, y) = pointer(&server);
    let right = f64::from(info.x + info.width);
    assert!(
        x > right - 3.0 && x < right,
        "stopped at the right edge, at {x}"
    );
    assert_eq!(y, f64::from(info.y) + 100.0);

    // A diagonal push slides along the edge.
    move_by(&server, 5000.0, 30.0);
    let (x2, y2) = pointer(&server);
    assert!(x2 < right);
    assert!(y2 > y + 25.0, "slid down the edge: {y} to {y2}");
}

#[test]
fn a_confinement_region_limits_the_pointer_to_it() {
    let server = Server::start();
    let mut client = server.connect();
    let (win, info) = focused_window(&server, &mut client, "game", (100.0, 100.0));
    let confine = client.confine_pointer(&win, Some((50, 50, 100, 100)), false);
    assert!(client.constraint_events(&confine).active);

    move_by(&server, -5000.0, -5000.0);
    let (x, y) = pointer(&server);
    assert!(
        x >= f64::from(info.x) + 49.0 && x <= f64::from(info.x) + 51.0,
        "{x}"
    );
    assert!(
        y >= f64::from(info.y) + 49.0 && y <= f64::from(info.y) + 51.0,
        "{y}"
    );
    // Absolute motion out of the region does not escape either.
    pointer_to_via_input(&server, 1900.0, 1000.0);
    let (x, _) = pointer(&server);
    assert!(x <= f64::from(info.x) + 151.0);
}

// ---- pointer-gestures ----

#[test]
fn gestures_reach_the_client_under_the_pointer() {
    let server = Server::start();
    let mut client = server.connect();
    let _win = focused_window(&server, &mut client, "pad", (100.0, 100.0));
    client.track_gestures();

    let steps = [
        Gesture::SwipeBegin { fingers: 3 },
        Gesture::SwipeUpdate {
            delta: (1.5, -2.0).into(),
        },
        Gesture::SwipeEnd { cancelled: false },
        Gesture::PinchBegin { fingers: 2 },
        Gesture::PinchUpdate {
            delta: (0.5, 0.25).into(),
            scale: 1.5,
            rotation: 10.0,
        },
        Gesture::PinchEnd { cancelled: true },
        Gesture::HoldBegin { fingers: 4 },
        Gesture::HoldEnd { cancelled: false },
    ];
    server.with_state(move |s| {
        for step in steps {
            s.gesture(step, InputTime::now());
        }
    });
    client.roundtrip();
    assert_eq!(
        client.gestures(),
        &[
            GestureSeen::SwipeBegin(3),
            GestureSeen::SwipeUpdate(1.5, -2.0),
            GestureSeen::SwipeEnd { cancelled: false },
            GestureSeen::PinchBegin(2),
            GestureSeen::PinchUpdate {
                dx: 0.5,
                dy: 0.25,
                scale: 1.5,
                rotation: 10.0
            },
            GestureSeen::PinchEnd { cancelled: true },
            GestureSeen::HoldBegin(4),
            GestureSeen::HoldEnd { cancelled: false },
        ]
    );
}

#[test]
fn gestures_can_be_turned_off_in_the_config() {
    let server = Server::start_with_config(Some("[touchpad]\ngestures = false\n"));
    let mut client = server.connect();
    let _win = focused_window(&server, &mut client, "pad", (100.0, 100.0));
    client.track_gestures();

    server.with_state(|s| s.gesture(Gesture::SwipeBegin { fingers: 3 }, InputTime::now()));
    client.roundtrip();
    assert!(client.gestures().is_empty());
}

// ---- text-input-v3 and input-method-v2 ----

#[test]
fn an_input_method_activates_with_a_text_input_and_commits_text() {
    let server = Server::start();
    let mut app = server.connect();
    app.track_text_input();
    // Text inputs only get `enter` while an input method is there to talk to.
    let mut ime = server.connect();
    ime.become_input_method();
    let _win = focused_window(&server, &mut app, "editor", (100.0, 100.0));
    assert!(
        app.text_input_events().entered,
        "the focused surface has it"
    );
    assert!(!ime.input_method_events().unavailable);
    assert!(!ime.input_method_events().active, "no text field yet");

    app.enable_text_input((10, 20, 5, 12));
    ime.roundtrip();
    let events = ime.input_method_events();
    assert!(events.active);
    assert!(events.done >= 1);

    ime.input_method_commit_string("こんにちは");
    app.roundtrip();
    let text = app.text_input_events();
    assert_eq!(text.committed, ["こんにちは"]);
    assert!(text.done >= 1);

    app.disable_text_input();
    ime.roundtrip();
    assert!(!ime.input_method_events().active);
    assert!(ime.input_method_events().deactivations >= 1);
}

#[test]
fn only_one_input_method_is_available_at_a_time() {
    let server = Server::start();
    let mut first = server.connect();
    first.become_input_method();
    let mut second = server.connect();
    second.become_input_method();
    assert!(!first.input_method_events().unavailable);
    assert!(second.input_method_events().unavailable);
}

#[test]
fn the_input_method_popup_belongs_to_the_focused_window() {
    let server = Server::start();
    let mut app = server.connect();
    app.track_text_input();
    let _win = focused_window(&server, &mut app, "editor", (100.0, 100.0));
    let mut ime = server.connect();
    ime.become_input_method();
    app.enable_text_input((10, 20, 5, 12));
    ime.roundtrip();

    let _popup = ime.input_method_popup((120, 40));
    let (count, is_ime) = server.with_state(|s| {
        let parent = s.keyboard.current_focus().unwrap();
        let popups: Vec<_> = PopupManager::popups_for_surface(&parent).collect();
        (
            popups.len(),
            popups
                .first()
                .is_some_and(|(kind, _)| matches!(kind, PopupKind::InputMethod(_))),
        )
    });
    assert_eq!(count, 1);
    assert!(is_ime);
}
