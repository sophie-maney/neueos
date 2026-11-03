//! The blocking client against a small scripted server on a temporary socket.
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

use std::io::{BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread::JoinHandle;

use neue_ipc::*;
use serde_json::json;

fn window(id: u64) -> WindowInfo {
    WindowInfo {
        id: WindowId(id),
        title: Some("Hello".into()),
        app_id: None,
        x: 10,
        y: 20,
        width: 300,
        height: 200,
        focused: true,
        minimized: false,
        zoomed: false,
        demands_attention: false,
        parent: None,
    }
}

/// Serve one connection with `script`, which gets a reader and a writer for it.
fn serve(
    script: impl FnOnce(&mut BufReader<UnixStream>, &mut UnixStream) + Send + 'static,
) -> (tempfile::TempDir, std::path::PathBuf, JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("comp.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        script(&mut reader, &mut writer);
    });
    (dir, path, handle)
}

fn next_request(reader: &mut BufReader<UnixStream>) -> RequestEnvelope {
    let line = read_line(reader).unwrap().expect("client hung up");
    parse_request(&line).expect("client sent a bad request")
}

fn reply_ok(writer: &mut UnixStream, id: u64, value: serde_json::Value) {
    let reply = Reply {
        id: Some(id),
        outcome: Outcome::Ok(value),
    };
    writer.write_all(&encode_line(&reply)).unwrap();
}

#[test]
fn typed_calls_and_error_replies() {
    let (_dir, path, server) = serve(|reader, writer| {
        let env = next_request(reader);
        assert_eq!(env.request, Request::Windows);
        reply_ok(writer, env.id, serde_json::to_value([window(1)]).unwrap());

        let env = next_request(reader);
        assert_eq!(
            env.request,
            Request::Action(ActionParams {
                name: "close-window".into(),
                window: Some(WindowId(9)),
                ..ActionParams::default()
            })
        );
        let reply = Reply {
            id: Some(env.id),
            outcome: Outcome::Error(ErrorBody::new(ErrorCode::UnknownWindow, "no window 9")),
        };
        writer.write_all(&encode_line(&reply)).unwrap();

        let env = next_request(reader);
        assert_eq!(env.request, Request::FocusedWindow);
        reply_ok(writer, env.id, json!(null));
    });

    let mut client = Client::connect(&path).unwrap();
    assert_eq!(client.windows().unwrap(), [window(1)]);
    match client.action("close-window", Some(WindowId(9))) {
        Err(ClientError::Remote(body)) => assert_eq!(body.code, ErrorCode::UnknownWindow),
        other => panic!("expected a remote error, got {other:?}"),
    }
    assert_eq!(client.focused_window().unwrap(), None);
    server.join().unwrap();
}

#[test]
fn subscription_yields_events_until_disconnect() {
    let (_dir, path, server) = serve(|reader, writer| {
        let env = next_request(reader);
        assert_eq!(
            env.request,
            Request::Subscribe(SubscribeParams {
                topics: vec![Topic::Windows]
            })
        );
        reply_ok(writer, env.id, json!(null));
        for event in [
            Event::WindowOpened(window(1)),
            Event::FocusChanged {
                window: Some(WindowId(1)),
            },
            Event::WindowClosed {
                window: WindowId(1),
            },
        ] {
            writer.write_all(&encode_line(&event)).unwrap();
        }
        // Dropping the stream ends the subscription.
    });

    let client = Client::connect(&path).unwrap();
    let events: Vec<Event> = client
        .subscribe(vec![Topic::Windows])
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], Event::WindowOpened(window(1)));
    assert_eq!(
        events[2],
        Event::WindowClosed {
            window: WindowId(1)
        }
    );
    server.join().unwrap();
}

#[test]
fn closed_connection_is_an_error() {
    let (_dir, path, server) = serve(|reader, _| {
        let _ = next_request(reader);
        // Hang up without replying.
    });
    let mut client = Client::connect(&path).unwrap();
    assert!(matches!(client.version(), Err(ClientError::Closed)));
    server.join().unwrap();
}

#[test]
fn socket_path_is_derived_from_the_display() {
    let path = socket_path_for_display(std::path::Path::new("/run/user/1000"), "wayland-1");
    assert_eq!(
        path,
        std::path::Path::new("/run/user/1000/neueos/comp.wayland-1.sock")
    );
}
