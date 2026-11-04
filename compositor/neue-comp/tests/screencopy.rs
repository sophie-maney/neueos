//! Screencopy of outputs and windows: `ext-image-copy-capture-v1` with output and toplevel
//! sources (tasks 1.16b and 1.16c, ADRs 0022 and 0023). Captures need the headless
//! renderer, so Mesa's software EGL device.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{FrameState, Server};
use neue_comp::{HEADLESS_OUTPUT_NAME, HEADLESS_OUTPUT_SIZE, WindowInfo};
use wayland_client::protocol::wl_shm;

const RED: u32 = 0xffc03020;
const BLUE: u32 = 0xff2040c0;
/// `ext_image_copy_capture_frame_v1.failure_reason`.
const BUFFER_CONSTRAINTS: u32 = 1;

fn only_window(server: &Server) -> WindowInfo {
    let windows = server.with_state(|state| state.windows());
    assert_eq!(windows.len(), 1, "{windows:#?}");
    windows.into_iter().next().expect("one window")
}

fn ready_damage(state: FrameState) -> Vec<(i32, i32, i32, i32)> {
    match state {
        FrameState::Ready { damage, transform } => {
            assert_eq!(transform, 0, "captures are upright (transform normal)");
            damage
        }
        other => panic!("frame not ready: {other:?}"),
    }
}

#[test]
fn capture_shows_what_the_output_shows() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let mut toplevel = client.create_toplevel("Red", "org.neueos.Test", (100, 60));
    client.fill(&mut toplevel, RED);
    let window = only_window(&server);

    let session = client.capture_output(0, false);
    let events = client.session_events(&session).clone();
    let (w, h) = HEADLESS_OUTPUT_SIZE;
    assert_eq!(events.buffer_size, Some((w as u32, h as u32)));
    assert!(
        events.shm_formats.contains(&wl_shm::Format::Argb8888),
        "{events:?}"
    );
    assert!(
        events.shm_formats.contains(&wl_shm::Format::Xrgb8888),
        "{events:?}"
    );
    assert_eq!(events.done, 1);
    assert!(!events.stopped);

    let frame = client.capture_frame(&session);
    let damage = ready_damage(client.wait_for_frame(&frame));
    assert_eq!(damage, vec![(0, 0, w, h)], "the first frame is all damage");
    assert_eq!(frame.pixel(window.x + 50, window.y + 30), RED);

    let rendered = server
        .with_state(|state| state.headless_render(HEADLESS_OUTPUT_NAME))
        .expect("rendering the headless output");
    assert!(
        frame.data() == rendered.data,
        "capture differs from the output"
    );
}

#[test]
fn frame_waits_for_damage_and_reports_it() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let mut toplevel = client.create_toplevel("Colours", "org.neueos.Test", (80, 40));
    client.fill(&mut toplevel, RED);
    let window = only_window(&server);
    let session = client.capture_output(0, false);
    let first = client.capture_frame(&session);
    ready_damage(client.wait_for_frame(&first));

    // Nothing changed: the next frame waits (several frame-timer ticks).
    let second = client.capture_frame(&session);
    std::thread::sleep(std::time::Duration::from_millis(100));
    client.roundtrip();
    assert_eq!(client.frame_state(&second), FrameState::Pending);

    // A new buffer: the frame completes with damage inside the window only.
    client.fill(&mut toplevel, BLUE);
    let damage = ready_damage(client.wait_for_frame(&second));
    assert!(!damage.is_empty());
    for (x, y, w, h) in damage {
        assert!(
            x >= window.x
                && y >= window.y
                && x + w <= window.x + window.width
                && y + h <= window.y + window.height,
            "damage {:?} outside {window:?}",
            (x, y, w, h)
        );
    }
    assert_eq!(second.pixel(window.x + 40, window.y + 20), BLUE);
}

#[test]
fn mode_change_sends_new_constraints() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let session = client.capture_output(0, false);
    let first = client.capture_frame(&session);
    ready_damage(client.wait_for_frame(&first));
    let waiting = client.capture_frame(&session);

    server
        .with_state(|state| state.headless_set_mode(HEADLESS_OUTPUT_NAME, (640, 480)))
        .expect("changing the mode");
    client.roundtrip();
    let events = client.session_events(&session).clone();
    assert_eq!(events.buffer_size, Some((640, 480)));
    assert_eq!(events.done, 2);
    assert_eq!(
        client.frame_state(&waiting),
        FrameState::Failed(BUFFER_CONSTRAINTS),
        "the waiting frame's buffer no longer fits"
    );

    let frame = client.capture_frame(&session);
    assert_eq!(frame.size, (640, 480));
    let damage = ready_damage(client.wait_for_frame(&frame));
    assert_eq!(damage, vec![(0, 0, 640, 480)]);
}

#[test]
fn too_small_buffer_fails() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let session = client.capture_output(0, false);
    let frame = client.capture_frame_into(&session, (100, 100), wl_shm::Format::Argb8888);
    assert_eq!(
        client.wait_for_frame(&frame),
        FrameState::Failed(BUFFER_CONSTRAINTS)
    );
}

#[test]
fn xrgb_buffer_is_accepted() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    let session = client.capture_output(0, false);
    let frame = client.capture_frame_into(&session, HEADLESS_OUTPUT_SIZE, wl_shm::Format::Xrgb8888);
    ready_damage(client.wait_for_frame(&frame));
}

#[test]
fn removed_output_stops_its_sessions() {
    let server = Server::start_rendering();
    server
        .with_state(|state| state.headless_add_output("HEADLESS-2", (800, 600)))
        .expect("adding an output");
    let mut client = server.connect();
    let first = client.capture_output(0, false);
    let second = client.capture_output(1, false);
    assert_eq!(client.session_events(&second).buffer_size, Some((800, 600)));

    server
        .with_state(|state| state.headless_remove_output("HEADLESS-2"))
        .expect("removing the output");
    client.roundtrip();
    assert!(client.session_events(&second).stopped);
    assert!(!client.session_events(&first).stopped);
}

#[test]
fn nothing_to_capture_without_a_renderer() {
    let server = Server::start();
    let mut client = server.connect();
    let session = client.capture_output(0, false);
    let events = client.session_events(&session);
    assert!(events.stopped, "{events:?}");
    assert_eq!(events.buffer_size, None);
}

// ---- toplevel sources (1.16c, ADR 0023) ----

const TRANSPARENT: u32 = 0;

#[test]
fn toplevel_capture_shows_only_that_window() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut back = client.create_toplevel("Back", "org.neueos.Test", (100, 60));
    client.fill(&mut back, RED);
    let mut front = client.create_toplevel("Front", "org.neueos.Test", (80, 50));
    client.fill(&mut front, BLUE);
    let windows = server.with_state(|state| state.windows());
    let find = |title: &str| {
        windows
            .iter()
            .find(|w| w.title.as_deref() == Some(title))
            .expect("window")
            .clone()
    };
    let (back_info, front_info) = (find("Back"), find("Front"));
    // The front window sits on the back one (new windows are cascaded), so an output
    // capture would show blue there; the window capture must not.
    assert!(
        front_info.x < back_info.x + back_info.width
            && front_info.y < back_info.y + back_info.height,
        "{back_info:?} {front_info:?}"
    );
    let toplevels = client.foreign_toplevels();
    let n = toplevels
        .iter()
        .position(|t| t.title == "Back")
        .expect("Back in the list");

    let session = client.capture_toplevel(n);
    let events = client.session_events(&session).clone();
    assert_eq!(events.buffer_size, Some((100, 60)), "{events:?}");
    assert!(events.shm_formats.contains(&wl_shm::Format::Argb8888));
    assert_eq!(events.done, 1);

    let frame = client.capture_frame(&session);
    let damage = ready_damage(client.wait_for_frame(&frame));
    assert_eq!(damage, vec![(0, 0, 100, 60)]);
    for (x, y) in [(0, 0), (99, 0), (0, 59), (99, 59), (50, 30)] {
        assert_eq!(frame.pixel(x, y), RED, "at {x},{y}");
    }
    // Right under the front window's top-left corner.
    let (ox, oy) = (front_info.x - back_info.x, front_info.y - back_info.y);
    assert_eq!(frame.pixel(ox.max(0) + 5, oy.max(0) + 5), RED);
}

#[test]
fn toplevel_capture_is_transparent_where_the_window_is() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut window = client.create_toplevel("Clear", "org.neueos.Test", (40, 30));
    client.fill(&mut window, TRANSPARENT);
    let session = client.capture_toplevel(0);
    let frame = client.capture_frame(&session);
    ready_damage(client.wait_for_frame(&frame));
    assert_eq!(frame.pixel(20, 15), TRANSPARENT);
}

#[test]
fn toplevel_frame_waits_for_the_window_to_change() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut window = client.create_toplevel("Doc", "org.neueos.Test", (80, 40));
    client.fill(&mut window, RED);
    let session = client.capture_toplevel(0);
    let first = client.capture_frame(&session);
    ready_damage(client.wait_for_frame(&first));

    let second = client.capture_frame(&session);
    std::thread::sleep(std::time::Duration::from_millis(100));
    client.roundtrip();
    assert_eq!(client.frame_state(&second), FrameState::Pending);

    client.fill(&mut window, BLUE);
    let damage = ready_damage(client.wait_for_frame(&second));
    assert_eq!(damage, vec![(0, 0, 80, 40)]);
    assert_eq!(second.pixel(40, 20), BLUE);
}

#[test]
fn window_resize_sends_new_constraints() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut window = client.create_toplevel("Doc", "org.neueos.Test", (80, 40));
    client.fill(&mut window, RED);
    let id = only_window(&server).id;
    let session = client.capture_toplevel(0);
    let first = client.capture_frame(&session);
    ready_damage(client.wait_for_frame(&first));
    let waiting = client.capture_frame(&session);

    server.with_state(move |state| {
        let window = state.window_by_id(id).expect("window");
        state.resize_window(&window, (120, 90).into());
    });
    client.ack_and_commit(&mut window);
    client.fill(&mut window, BLUE);
    assert_eq!(
        client.wait_for_frame(&waiting),
        FrameState::Failed(BUFFER_CONSTRAINTS)
    );
    let events = client.session_events(&session).clone();
    assert_eq!(events.buffer_size, Some((120, 90)));
    assert_eq!(events.done, 2);

    let frame = client.capture_frame(&session);
    assert_eq!(frame.size, (120, 90));
    ready_damage(client.wait_for_frame(&frame));
    assert_eq!(frame.pixel(60, 45), BLUE);
}

#[test]
fn minimised_window_can_be_captured() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut window = client.create_toplevel("Doc", "org.neueos.Test", (60, 40));
    client.fill(&mut window, RED);
    let id = only_window(&server).id;
    server.with_state(move |state| {
        let window = state.window_by_id(id).expect("window");
        state.minimize_window(&window);
    });
    assert!(only_window(&server).minimized);
    let session = client.capture_toplevel(0);
    let frame = client.capture_frame(&session);
    ready_damage(client.wait_for_frame(&frame));
    assert_eq!(frame.pixel(30, 20), RED);
}

#[test]
fn closed_window_stops_its_session() {
    let server = Server::start_rendering();
    let mut client = server.connect();
    client.track_foreign_toplevels();
    let mut keep = client.create_toplevel("Keep", "org.neueos.Test", (60, 40));
    client.fill(&mut keep, RED);
    let mut gone = client.create_toplevel("Gone", "org.neueos.Test", (60, 40));
    client.fill(&mut gone, BLUE);
    let kept = client.capture_toplevel(0);
    let closed = client.capture_toplevel(1);
    assert!(!client.session_events(&closed).stopped);

    client.destroy_toplevel(gone);
    client.roundtrip();
    assert!(client.session_events(&closed).stopped);
    assert!(!client.session_events(&kept).stopped);
}
