//! The `neuectl` socket, driven with `neue-ipc`'s client against the headless compositor.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use harness::Server;
use neue_ipc::{ClientError, ErrorCode, Event, PROTOCOL_VERSION, Topic, WindowArg, WindowId};
use wayland_protocols::xdg::shell::client::xdg_toplevel;

/// A raw connection for sending hand-written lines.
fn raw(server: &Server) -> (UnixStream, std::io::BufReader<UnixStream>) {
    let path = server.with_state(|state| state.ipc.path().map(ToOwned::to_owned));
    let stream = UnixStream::connect(path.expect("ipc is listening")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let reader = std::io::BufReader::new(stream.try_clone().unwrap());
    (stream, reader)
}

fn read_json(reader: &mut std::io::BufReader<UnixStream>) -> serde_json::Value {
    let line = neue_ipc::read_line(reader)
        .unwrap()
        .expect("connection closed");
    serde_json::from_slice(&line).unwrap()
}

#[test]
fn version_outputs_and_empty_session() {
    let server = Server::start();
    let mut ipc = server.ipc();

    let version = ipc.version().unwrap();
    assert_eq!(version.compositor, "neue-comp");
    assert_eq!(version.protocol, PROTOCOL_VERSION);

    let outputs = ipc.outputs().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].name, "HEADLESS-1");
    let (w, h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    assert_eq!((outputs[0].width, outputs[0].height), (w, h));
    assert_eq!(outputs[0].refresh_mhz, 60_000);

    assert!(ipc.windows().unwrap().is_empty());
    assert_eq!(ipc.focused_window().unwrap(), None);
}

#[test]
fn windows_and_focused_window_match_the_compositor() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("First", "org.neueos.Test", (300, 200));
    client.create_toplevel("Second", "org.neueos.Test", (320, 240));

    let mut ipc = server.ipc();
    let windows = ipc.windows().unwrap();
    assert_eq!(windows, server.with_state(|state| state.windows()));
    assert_eq!(windows.len(), 2);
    assert_ne!(windows[0].id, windows[1].id);
    assert_eq!(windows[1].title.as_deref(), Some("Second"));
    assert_eq!((windows[1].width, windows[1].height), (320, 240));

    let focused = ipc.focused_window().unwrap().expect("a focused window");
    assert_eq!(focused, windows[1]);
    assert!(focused.focused && !windows[0].focused);
}

#[test]
fn actions_are_listed() {
    let server = Server::start();
    let actions = server.ipc().actions().unwrap();
    let find = |name: &str| actions.iter().find(|a| a.name == name);
    assert_eq!(find("quit").unwrap().window, WindowArg::None);
    assert_eq!(find("close-window").unwrap().window, WindowArg::Optional);
    assert_eq!(find("focus-window").unwrap().window, WindowArg::Required);
}

#[test]
fn focus_window_action_raises_and_focuses() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("First", "org.neueos.Test", (300, 200));
    client.create_toplevel("Second", "org.neueos.Test", (300, 200));

    let mut ipc = server.ipc();
    let first = ipc.windows().unwrap()[0].id;
    ipc.action("focus-window", Some(first)).unwrap();

    let windows = ipc.windows().unwrap();
    assert_eq!(windows.last().unwrap().id, first, "raised to the top");
    assert!(windows.last().unwrap().focused);
    assert_eq!(ipc.focused_window().unwrap().unwrap().id, first);
}

#[test]
fn close_window_action_asks_the_client_to_close() {
    let server = Server::start();
    let mut client = server.connect();
    let first = client.create_toplevel("First", "org.neueos.Test", (300, 200));
    let second = client.create_toplevel("Second", "org.neueos.Test", (300, 200));

    let mut ipc = server.ipc();
    // Without a window it targets the focused one (the second).
    ipc.action("close-window", None).unwrap();
    client.roundtrip();
    assert!(client.events(&second).close_requested);
    assert!(!client.events(&first).close_requested);

    // An explicit id works too.
    let first_id = ipc.windows().unwrap()[0].id;
    ipc.action("close-window", Some(first_id)).unwrap();
    client.roundtrip();
    assert!(client.events(&first).close_requested);
    // The client decides when to go away; the window is still listed.
    assert_eq!(ipc.windows().unwrap().len(), 2);
    let _ = xdg_toplevel::State::Activated; // keep the protocol import used in all cfgs
}

#[test]
fn errors_have_stable_codes() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("Only", "org.neueos.Test", (300, 200));
    let mut ipc = server.ipc();

    let code = |result: Result<(), ClientError>| match result {
        Err(ClientError::Remote(body)) => body.code,
        other => panic!("expected a remote error, got {other:?}"),
    };
    assert_eq!(
        code(ipc.action("no-such-action", None)),
        ErrorCode::UnknownAction
    );
    assert_eq!(
        code(ipc.action("close-window", Some(WindowId(9999)))),
        ErrorCode::UnknownWindow
    );
    assert_eq!(code(ipc.action("focus-window", None)), ErrorCode::NoWindow);

    // The connection survives errors.
    assert_eq!(ipc.windows().unwrap().len(), 1);
}

#[test]
fn malformed_lines_get_error_replies_and_keep_the_connection() {
    let server = Server::start();
    let (mut stream, mut reader) = raw(&server);

    stream.write_all(b"this is not json\n").unwrap();
    let reply = read_json(&mut reader);
    assert_eq!(reply["id"], serde_json::Value::Null);
    assert_eq!(reply["error"]["code"], "invalid_request");

    stream
        .write_all(b"{\"id\": 5, \"request\": \"spaces\"}\n")
        .unwrap();
    let reply = read_json(&mut reader);
    assert_eq!(reply["id"], 5);
    assert_eq!(reply["error"]["code"], "unknown_request");

    stream
        .write_all(b"{\"id\": 6, \"request\": \"action\"}\n")
        .unwrap();
    let reply = read_json(&mut reader);
    assert_eq!(reply["id"], 6);
    assert_eq!(reply["error"]["code"], "invalid_params");

    // Unknown fields are ignored, and several requests in one write are answered in order.
    stream
        .write_all(
            b"{\"id\": 7, \"request\": \"windows\", \"future\": 1}\n{\"id\": 8, \"request\": \"version\"}\n",
        )
        .unwrap();
    assert_eq!(read_json(&mut reader)["id"], 7);
    let reply = read_json(&mut reader);
    assert_eq!(reply["id"], 8);
    assert_eq!(reply["ok"]["protocol"], PROTOCOL_VERSION);
}

#[test]
fn oversized_line_is_rejected_and_disconnects() {
    let server = Server::start();
    let (mut stream, mut reader) = raw(&server);

    let junk = vec![b'x'; neue_ipc::MAX_LINE_BYTES + 1];
    // The compositor may hang up before we finish writing; that is fine.
    let _ = stream.write_all(&junk);
    let reply = read_json(&mut reader);
    assert_eq!(reply["error"]["code"], "line_too_long");
    let mut rest = Vec::new();
    // Closed after the error (a reset also counts).
    let _ = reader.read_to_end(&mut rest);

    // Other connections are unaffected.
    assert_eq!(server.ipc().version().unwrap().compositor, "neue-comp");
}

#[test]
fn subscribers_see_window_events_in_order() {
    let server = Server::start();
    let mut subscription = server.ipc().subscribe(vec![Topic::Windows]).unwrap();
    subscription
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();

    let mut client = server.connect();
    let first = client.create_toplevel("First", "org.neueos.Test", (300, 200));
    let second = client.create_toplevel("Second", "org.neueos.Test", (300, 200));
    client.destroy_toplevel(second);
    client.destroy_toplevel(first);

    let mut next = || {
        subscription
            .next_event()
            .unwrap()
            .expect("event stream ended")
    };

    let Event::WindowOpened(first_info) = next() else {
        panic!("expected window_opened for the first window");
    };
    assert_eq!(first_info.title.as_deref(), Some("First"));
    assert_eq!(
        next(),
        Event::FocusChanged {
            window: Some(first_info.id)
        }
    );

    let Event::WindowOpened(second_info) = next() else {
        panic!("expected window_opened for the second window");
    };
    assert_eq!(second_info.title.as_deref(), Some("Second"));
    assert_eq!(
        next(),
        Event::FocusChanged {
            window: Some(second_info.id)
        }
    );

    // Closing the focused window: it goes away, focus returns to the first.
    assert_eq!(
        next(),
        Event::WindowClosed {
            window: second_info.id
        }
    );
    assert_eq!(
        next(),
        Event::FocusChanged {
            window: Some(first_info.id)
        }
    );
    assert_eq!(
        next(),
        Event::WindowClosed {
            window: first_info.id
        }
    );
    assert_eq!(next(), Event::FocusChanged { window: None });
}

#[test]
fn new_subscribers_get_no_backlog_and_other_clients_keep_working() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("Before", "org.neueos.Test", (300, 200));

    let mut subscription = server.ipc().subscribe(vec![Topic::Windows]).unwrap();
    subscription
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    // Nothing is pending: the read times out rather than yielding old events.
    match subscription.next_event() {
        Err(ClientError::Io(err)) => assert!(matches!(
            err.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )),
        other => panic!("expected a timeout, got {other:?}"),
    }
    // Queries still work next to a subscriber.
    assert_eq!(server.ipc().windows().unwrap().len(), 1);
}

#[test]
fn quit_stops_the_compositor_after_replying() {
    let server = Server::start();
    let mut ipc = server.ipc();
    ipc.quit().unwrap();
    // The compositor exits: further requests fail. Dropping `server` joins its thread and
    // fails the test if the run loop returned an error.
    drop(server);
    assert!(ipc.version().is_err());
}

#[test]
fn quit_action_is_equivalent() {
    let server = Server::start();
    let mut ipc = server.ipc();
    ipc.action("quit", None).unwrap();
    drop(server);
    assert!(ipc.windows().is_err());
}
