//! Server-side title bars (task 1.10): xdg-decoration and KDE server-decoration
//! negotiation, and window geometry that leaves room for the bar, on the headless backend.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{KdeDefaultMode, KdeMode, Server, TestClient, Toplevel, XdgMode};
use neue_comp::decorations::{TITLE_BAR_HEIGHT, is_server_decorated};
use neue_comp::{HEADLESS_OUTPUT_SIZE, WindowInfo};

const APP: &str = "org.neueos.Test";

fn info(server: &Server, title: &str) -> WindowInfo {
    let title = title.to_owned();
    server
        .with_state(move |state| state.windows())
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title.as_str()))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn decorated(server: &Server, title: &str) -> bool {
    let id = info(server, title).id;
    server.with_state(move |state| {
        let window = state.window_by_id(id).expect("window is mapped");
        is_server_decorated(&window)
    })
}

/// Zoom `title` and let the client follow the configure.
fn zoom(server: &Server, client: &mut TestClient, toplevel: &mut Toplevel, title: &str) {
    let id = info(server, title).id;
    server.with_state(move |state| {
        let window = state.window_by_id(id).expect("window is mapped");
        state.set_zoomed(&window, true, None);
    });
    client.ack_and_commit(toplevel);
}

#[test]
fn server_side_is_the_default() {
    let server = Server::start();
    let mut client = server.connect();
    let t = client.create_decorated_toplevel("ssd", APP, (300, 200), None);
    assert_eq!(client.xdg_decoration_mode(&t), Some(XdgMode::ServerSide));
    assert!(decorated(&server, "ssd"));
}

#[test]
fn a_client_side_request_is_honoured() {
    let server = Server::start();
    let mut client = server.connect();
    let t = client.create_decorated_toplevel("csd", APP, (300, 200), Some(XdgMode::ClientSide));
    assert_eq!(client.xdg_decoration_mode(&t), Some(XdgMode::ClientSide));
    assert!(!decorated(&server, "csd"));
}

#[test]
fn clients_without_a_decoration_object_draw_their_own() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_toplevel("plain", APP, (300, 200));
    assert!(!decorated(&server, "plain"));
}

#[test]
fn zoom_leaves_room_for_the_title_bar() {
    let (w, h) = HEADLESS_OUTPUT_SIZE;
    let server = Server::start();
    let mut client = server.connect();
    let mut ssd = client.create_decorated_toplevel("ssd", APP, (300, 200), None);
    zoom(&server, &mut client, &mut ssd, "ssd");
    let i = info(&server, "ssd");
    assert_eq!(
        (i.x, i.y, i.width, i.height),
        (0, TITLE_BAR_HEIGHT, w, h - TITLE_BAR_HEIGHT)
    );

    let mut csd =
        client.create_decorated_toplevel("csd", APP, (300, 200), Some(XdgMode::ClientSide));
    zoom(&server, &mut client, &mut csd, "csd");
    let i = info(&server, "csd");
    assert_eq!((i.x, i.y, i.width, i.height), (0, 0, w, h));
}

#[test]
fn switching_to_client_side_refits_a_zoomed_window() {
    let (w, h) = HEADLESS_OUTPUT_SIZE;
    let server = Server::start();
    let mut client = server.connect();
    let mut t = client.create_decorated_toplevel("switch", APP, (300, 200), None);
    zoom(&server, &mut client, &mut t, "switch");
    client.set_xdg_decoration_mode(&t, XdgMode::ClientSide);
    assert_eq!(client.xdg_decoration_mode(&t), Some(XdgMode::ClientSide));
    client.ack_and_commit(&mut t);
    let i = info(&server, "switch");
    assert!(!decorated(&server, "switch"));
    assert_eq!((i.x, i.y, i.width, i.height), (0, 0, w, h));
}

#[test]
fn moves_keep_the_title_bar_on_screen() {
    let server = Server::start();
    let mut client = server.connect();
    client.create_decorated_toplevel("ssd", APP, (300, 200), None);
    let id = info(&server, "ssd").id;
    server.with_state(move |state| {
        let window = state.window_by_id(id).expect("window is mapped");
        state.move_window_to(&window, (40, -50).into());
    });
    let i = info(&server, "ssd");
    assert_eq!((i.x, i.y), (40, TITLE_BAR_HEIGHT));
}

#[test]
fn new_windows_are_placed_below_the_top_edge() {
    let (_, h) = HEADLESS_OUTPUT_SIZE;
    let server = Server::start();
    let mut client = server.connect();
    // As tall as the output: placement must still keep the bar visible.
    client.create_decorated_toplevel("tall", APP, (300, h), None);
    assert!(info(&server, "tall").y >= TITLE_BAR_HEIGHT);
}

#[test]
fn kde_server_decoration() {
    let server = Server::start();
    let mut client = server.connect();
    assert_eq!(client.kde_default_mode(), Some(KdeDefaultMode::Server));
    let t = client.create_toplevel("kde", APP, (300, 200));
    client.kde_decorate(&t);
    assert_eq!(client.kde_mode(&t), Some(KdeMode::Server));
    assert!(decorated(&server, "kde"));

    client.kde_request_mode(&t, KdeMode::Client);
    assert_eq!(client.kde_mode(&t), Some(KdeMode::Client));
    assert!(!decorated(&server, "kde"));

    client.kde_request_mode(&t, KdeMode::Server);
    assert_eq!(client.kde_mode(&t), Some(KdeMode::Server));
    assert!(decorated(&server, "kde"));
}
