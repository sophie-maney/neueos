//! Compositor integration tests on the headless backend with scripted clients.
#![cfg(target_os = "linux")]

mod harness;

use harness::Server;
use wayland_protocols::xdg::shell::client::xdg_toplevel;

#[test]
fn no_windows_at_start() {
    let server = Server::start();
    assert!(server.with_state(|state| state.windows()).is_empty());
}

#[test]
fn toplevel_maps_and_is_listed() {
    let server = Server::start();
    let mut client = server.connect();
    let toplevel = client.create_toplevel("Hello", "org.neueos.Test", (320, 200));

    let windows = server.with_state(|state| state.windows());
    assert_eq!(windows.len(), 1, "{windows:#?}");
    let window = &windows[0];
    assert_eq!(window.title.as_deref(), Some("Hello"));
    assert_eq!(window.app_id.as_deref(), Some("org.neueos.Test"));
    assert_eq!((window.width, window.height), (320, 200));
    assert!(window.focused);

    // The window lies on the headless output.
    let (out_w, out_h) = neue_comp::HEADLESS_OUTPUT_SIZE;
    let (x, y) = (window.x, window.y);
    assert!(
        x >= 0 && y >= 0 && x + 320 <= out_w && y + 200 <= out_h,
        "{window:#?}"
    );

    // Focus is reported to the client as the activated state.
    client.roundtrip();
    assert!(
        client
            .events(&toplevel)
            .states
            .contains(&xdg_toplevel::State::Activated)
    );
}

#[test]
fn newest_toplevel_is_on_top_and_focused() {
    let server = Server::start();
    let mut client = server.connect();
    let first = client.create_toplevel("First", "org.neueos.Test", (300, 200));
    let _second = client.create_toplevel("Second", "org.neueos.Test", (300, 200));

    let titles = |server: &Server| {
        server.with_state(|state| {
            state
                .windows()
                .into_iter()
                .map(|w| (w.title.unwrap_or_default(), w.focused))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        titles(&server),
        [("First".to_owned(), false), ("Second".to_owned(), true)]
    );

    // Destroying a window removes it from the list.
    client.destroy_toplevel(first);
    assert_eq!(titles(&server), [("Second".to_owned(), true)]);
}

#[test]
fn closing_the_focused_window_focuses_the_next() {
    let server = Server::start();
    let mut client = server.connect();
    let _first = client.create_toplevel("First", "org.neueos.Test", (300, 200));
    let second = client.create_toplevel("Second", "org.neueos.Test", (300, 200));

    client.destroy_toplevel(second);
    let windows = server.with_state(|state| state.windows());
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].title.as_deref(), Some("First"));
    assert!(windows[0].focused);
}
