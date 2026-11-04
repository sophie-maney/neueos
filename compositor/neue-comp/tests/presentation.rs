//! Presentation and sync (task 1.14, ADR 0017): presentation-time feedback, tearing-control
//! hints, security-context filtering and explicit sync availability, on the headless
//! backend.
#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use harness::{Feedback, Server, global_names};
use neue_comp::State;

/// `CLOCK_MONOTONIC` from `<time.h>`.
const CLOCK_MONOTONIC: u32 = 1;
/// One refresh of the headless outputs (60 Hz), in nanoseconds.
const HEADLESS_REFRESH_NS: u32 = 16_666_666;

/// Globals a sandboxed client must not see.
const PRIVILEGED: [&str; 7] = [
    "wp_security_context_manager_v1",
    "ext_session_lock_manager_v1",
    "zwlr_layer_shell_v1",
    "ext_data_control_manager_v1",
    "zwlr_data_control_manager_v1",
    "zwp_input_method_manager_v2",
    "ext_foreign_toplevel_list_v1",
];

/// Whether the (only) window's surface asks for tearing.
fn first_window_wants_tearing(server: &Server) -> bool {
    server.with_state(|state| {
        let id = state.windows()[0].id;
        let window = state.window_by_id(id).unwrap();
        State::wants_tearing(window.toplevel().unwrap().wl_surface())
    })
}

#[test]
fn presentation_uses_the_monotonic_clock() {
    let server = Server::start();
    let mut client = server.connect();
    assert_eq!(client.presentation_clock(), CLOCK_MONOTONIC);
}

#[test]
fn feedback_is_presented_with_the_output_refresh() {
    let server = Server::start();
    let mut client = server.connect();
    let mut window = client.create_toplevel("Clock", "org.example.Clock", (200, 100));

    let first = client.request_feedback(&window.surface);
    client.commit_frame(&mut window);
    let Feedback::Presented {
        refresh_ns,
        sequence,
        flags,
        tv_sec,
    } = client.wait_for_feedback(first)
    else {
        panic!("not presented: {:?}", client.feedback(first));
    };
    assert_eq!(refresh_ns, HEADLESS_REFRESH_NS);
    // Nothing is really shown headless: no vsync, clock or zero-copy claims.
    assert_eq!(flags, 0);
    assert!(tv_sec > 0, "a monotonic time since boot");

    let second = client.request_feedback(&window.surface);
    client.commit_frame(&mut window);
    let Feedback::Presented { sequence: next, .. } = client.wait_for_feedback(second) else {
        panic!("second frame not presented");
    };
    assert!(next > sequence, "sequence {next} after {sequence}");
}

#[test]
fn superseded_feedback_is_discarded() {
    let server = Server::start();
    let mut client = server.connect();
    let mut window = client.create_toplevel("Fast", "org.example.Fast", (200, 100));

    // Two commits in one flush: the compositor applies both before the next frame.
    let first = client.request_feedback(&window.surface);
    client.commit_frame(&mut window);
    let second = client.request_feedback(&window.surface);
    client.commit_frame(&mut window);

    assert!(matches!(
        client.wait_for_feedback(second),
        Feedback::Presented { .. }
    ));
    assert_eq!(client.feedback(first), Feedback::Discarded);
}

#[test]
fn feedback_of_a_destroyed_surface_is_discarded() {
    let server = Server::start();
    let mut client = server.connect();
    // Not a window: on no output, so never presented.
    let surface = client.create_bare_surface();
    let feedback = client.request_feedback(&surface);
    surface.commit();
    client.roundtrip();
    std::thread::sleep(Duration::from_millis(50));
    client.roundtrip();
    assert_eq!(client.feedback(feedback), Feedback::Pending);

    surface.destroy();
    assert_eq!(client.wait_for_feedback(feedback), Feedback::Discarded);
}

#[test]
fn tearing_hint_is_double_buffered() {
    use wayland_protocols::wp::tearing_control::v1::client::wp_tearing_control_v1::PresentationHint;

    let server = Server::start();
    let mut client = server.connect();
    let window = client.create_toplevel("Game", "org.example.Game", (320, 200));
    assert!(!first_window_wants_tearing(&server), "vsync by default");

    let control = client.tearing_control(&window.surface);
    control.set_presentation_hint(PresentationHint::Async);
    client.roundtrip();
    assert!(
        !first_window_wants_tearing(&server),
        "applied only on commit"
    );

    window.surface.commit();
    client.roundtrip();
    assert!(first_window_wants_tearing(&server));

    // Destroying the object reverts to vsync, again on the next commit.
    control.destroy();
    client.roundtrip();
    assert!(first_window_wants_tearing(&server));
    window.surface.commit();
    client.roundtrip();
    assert!(!first_window_wants_tearing(&server));

    // Once destroyed, the surface may get a new one.
    let again = client.tearing_control(&window.surface);
    again.set_presentation_hint(PresentationHint::Async);
    window.surface.commit();
    client.roundtrip();
    assert!(first_window_wants_tearing(&server));
}

#[test]
fn a_second_tearing_control_is_a_protocol_error() {
    let server = Server::start();
    let mut client = server.connect();
    let window = client.create_toplevel("Game", "org.example.Game", (320, 200));
    let _first = client.tearing_control(&window.surface);
    let _second = client.tearing_control(&window.surface);
    assert_eq!(
        client.roundtrip_protocol_error(),
        // `tearing_control_exists`
        Some(("wp_tearing_control_manager_v1".into(), 0))
    );
}

#[test]
fn sandboxed_clients_lose_privileged_globals() {
    let server = Server::start();
    let mut creator = server.connect();
    for interface in PRIVILEGED {
        assert!(creator.has_global(interface), "{interface} missing");
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sandbox.sock");
    let listener = UnixListener::bind(&path).unwrap();
    // The compositor polls `close_fd` for a hang-up; we keep the write end.
    let (close_read, close_write) = std::io::pipe().unwrap();
    let _context = creator.create_security_context(&listener, &close_read, "org.example.Sandboxed");
    drop(listener);
    drop(close_read);

    let names = global_names(UnixStream::connect(&path).unwrap());
    for interface in PRIVILEGED {
        assert!(!names.iter().any(|n| n == interface), "{interface} visible");
    }
    for interface in ["wl_compositor", "xdg_wm_base", "wl_seat", "wp_presentation"] {
        assert!(names.iter().any(|n| n == interface), "{interface} missing");
    }
    drop(close_write);
}

#[test]
fn security_context_ends_when_the_sandbox_closes_it() {
    let server = Server::start();
    let mut creator = server.connect();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sandbox.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let (close_read, close_write) = std::io::pipe().unwrap();
    let _context = creator.create_security_context(&listener, &close_read, "org.example.Gone");
    drop(listener);
    drop(close_read);
    assert!(!global_names(UnixStream::connect(&path).unwrap()).is_empty());

    drop(close_write);
    let mut refused = false;
    for _ in 0..200 {
        creator.roundtrip();
        if UnixStream::connect(&path).is_err() {
            refused = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(refused, "the listener outlived its close_fd");
}

#[test]
fn explicit_sync_needs_a_drm_device() {
    let server = Server::start();
    let client = server.connect();
    // Headless has no GPU: no syncobj global, but the rest of 1.14 is there.
    assert!(!client.has_global("wp_linux_drm_syncobj_manager_v1"));
    assert!(client.has_global("wp_presentation"));
    assert!(client.has_global("wp_tearing_control_manager_v1"));
}
