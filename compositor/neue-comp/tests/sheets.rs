//! Modal dialogs placed as sheets (task 2.23c), on the headless backend.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::Duration;

use harness::{Server, TestClient, XdgMode};
use neue_comp::decorations::TITLE_BAR_HEIGHT;
use neue_ipc::{ActionParams, Event, Topic, WindowId, WindowInfo};
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

fn titles(server: &Server) -> Vec<String> {
    windows(server)
        .into_iter()
        .map(|w| w.title.unwrap_or_default())
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

fn move_window(server: &Server, id: WindowId, (x, y): (i32, i32)) {
    server
        .ipc()
        .action_with(ActionParams {
            name: "move-window".into(),
            window: Some(id),
            x: Some(x),
            y: Some(y),
            ..ActionParams::default()
        })
        .unwrap();
}

fn resize_window(server: &Server, id: WindowId, (w, h): (i32, i32)) {
    server
        .ipc()
        .action_with(ActionParams {
            name: "resize-window".into(),
            window: Some(id),
            width: Some(w),
            height: Some(h),
            ..ActionParams::default()
        })
        .unwrap();
}

/// Where `sheet` should be: centred on `parent`, its top at the top of the parent's
/// client area, below its own title bar if it has one.
fn hangs_from(sheet: &WindowInfo, parent: &WindowInfo, bar: i32) -> bool {
    (sheet.x, sheet.y) == (parent.x + (parent.width - sheet.width) / 2, parent.y + bar)
}

#[test]
fn modal_dialogs_hang_under_their_parents_chrome() {
    let server = Server::start();
    let mut client = server.connect();

    // A parent that draws its own chrome: the sheet starts at the top of its geometry.
    let parent = client.create_toplevel("Doc", APP, (600, 400));
    let setup = client.modal_of(&parent);
    client.create_toplevel_with("Sheet", APP, (200, 100), setup);
    let (p, s) = (window(&server, "Doc"), window(&server, "Sheet"));
    assert_eq!(s.parent, Some(p.id));
    assert!(hangs_from(&s, &p, 0), "{s:?} on {p:?}");
    assert_eq!(titles(&server), ["Doc", "Sheet"]);

    // A parent with our title bar: the sheet starts just below the bar, where the client
    // area begins; a sheet with a bar of its own puts that bar there.
    let decorated =
        client.create_decorated_toplevel("Decorated", APP, (500, 300), Some(XdgMode::ServerSide));
    let setup = client.modal_of(&decorated);
    client.create_toplevel_with("Plain sheet", APP, (240, 120), setup);
    let setup = client.modal_of(&decorated);
    client.create_decorated_toplevel_with(
        "Barred sheet",
        APP,
        (100, 80),
        Some(XdgMode::ServerSide),
        setup,
    );
    let p = window(&server, "Decorated");
    assert!(hangs_from(&window(&server, "Plain sheet"), &p, 0));
    assert!(hangs_from(
        &window(&server, "Barred sheet"),
        &p,
        TITLE_BAR_HEIGHT
    ));
}

#[test]
fn sheets_follow_their_parent() {
    let server = Server::start();
    let mut client = server.connect();
    let mut parent = client.create_toplevel("Doc", APP, (600, 400));
    let setup = client.modal_of(&parent);
    let mut sheet = client.create_toplevel_with("Sheet", APP, (200, 100), setup);
    client.create_toplevel("Other", "org.neueos.Other", (300, 200));
    let (p, s) = (window(&server, "Doc"), window(&server, "Sheet"));

    // A scripted move of the parent, and none of the sheet: it stays attached.
    move_window(&server, p.id, (50, 300));
    assert_eq!(
        (window(&server, "Sheet").x, window(&server, "Sheet").y),
        (250, 300)
    );
    move_window(&server, s.id, (700, 10));
    assert_eq!(
        (window(&server, "Sheet").x, window(&server, "Sheet").y),
        (250, 300)
    );
    // Following doesn't raise it above other windows.
    assert_eq!(titles(&server), ["Doc", "Sheet", "Other"]);

    // Dragging the parent: the sheet follows during the drag.
    let start = window(&server, "Doc");
    let serial = press_on(&server, &mut client, &start);
    parent.toplevel._move(&client.seat, serial);
    client.roundtrip();
    pointer_to(&server, (start.x + 10 + 120, start.y + 10 + 40));
    let s = window(&server, "Sheet");
    assert_eq!((s.x, s.y), (start.x + 120 + 200, start.y + 40));
    button(&server, ButtonState::Released);

    // The parent resizes: the sheet is centred again.
    resize_window(&server, p.id, (800, 500));
    client.ack_and_commit(&mut parent);
    let (p, s) = (window(&server, "Doc"), window(&server, "Sheet"));
    assert_eq!(p.width, 800);
    assert!(hangs_from(&s, &p, 0), "{s:?} on {p:?}");

    // The sheet resizes: still centred.
    resize_window(&server, s.id, (300, 150));
    client.ack_and_commit(&mut sheet);
    let s = window(&server, "Sheet");
    assert_eq!(s.width, 300);
    assert!(hangs_from(&s, &p, 0), "{s:?} on {p:?}");
}

#[test]
fn sheets_hang_under_the_chrome_their_parent_announces() {
    let server = Server::start();
    let mut client = server.connect();
    let parent = client.create_toplevel("Doc", APP, (600, 400));
    let setup = client.modal_of(&parent);
    client.create_toplevel_with("Sheet", APP, (200, 100), setup);
    let sheet_at = |server: &Server| {
        let (p, s) = (window(server, "Doc"), window(server, "Sheet"));
        assert_eq!(s.x, p.x + (p.width - s.width) / 2, "{s:?} on {p:?}");
        s.y - p.y
    };
    assert_eq!(sheet_at(&server), 0);

    // A title bar and toolbar 52 px tall: the sheet hangs under them once committed.
    let chrome = client.neue_window(&parent.surface);
    chrome.set_chrome_height(52);
    client.roundtrip();
    assert_eq!(sheet_at(&server), 0, "applied before the commit");
    parent.surface.commit();
    client.roundtrip();
    assert_eq!(sheet_at(&server), 52);

    // The toolbar changes its display mode: the sheet follows.
    chrome.set_chrome_height(30);
    parent.surface.commit();
    client.roundtrip();
    assert_eq!(sheet_at(&server), 30);

    // Never below the parent's bottom edge.
    chrome.set_chrome_height(1000);
    parent.surface.commit();
    client.roundtrip();
    assert_eq!(sheet_at(&server), 400);

    // Forgotten with the object: back to the top of the window geometry.
    chrome.destroy();
    parent.surface.commit();
    client.roundtrip();
    assert_eq!(sheet_at(&server), 0);

    // A new object may be made for the surface.
    let chrome = client.neue_window(&parent.surface);
    chrome.set_chrome_height(22);
    parent.surface.commit();
    client.roundtrip();
    assert_eq!(sheet_at(&server), 22);
}

#[test]
fn a_dialog_made_modal_after_mapping_becomes_a_sheet() {
    let server = Server::start();
    let mut client = server.connect();
    let parent = client.create_toplevel("Doc", APP, (600, 400));
    let child = client.create_toplevel_with("Dialog", APP, (200, 100), |t| {
        t.set_parent(Some(&parent.toplevel));
    });
    // A plain dialog is centred on its parent and stays put when the parent moves.
    let (p, d) = (window(&server, "Doc"), window(&server, "Dialog"));
    assert_eq!((d.x, d.y), (p.x + 200, p.y + 150));
    move_window(&server, p.id, (p.x + 30, p.y + 30));
    assert_eq!(
        (window(&server, "Dialog").x, window(&server, "Dialog").y),
        (d.x, d.y)
    );

    client.set_modal(&child);
    let (p, d) = (window(&server, "Doc"), window(&server, "Dialog"));
    assert!(hangs_from(&d, &p, 0), "{d:?} on {p:?}");
}

#[test]
fn a_sheet_that_follows_a_drag_is_reported_once_it_ends() {
    let server = Server::start();
    let mut client = server.connect();
    let parent = client.create_toplevel("Doc", APP, (600, 400));
    let setup = client.modal_of(&parent);
    client.create_toplevel_with("Sheet", APP, (200, 100), setup);
    let mut other_client = server.connect();
    other_client.create_toplevel("Other", "org.neueos.Other", (100, 100));
    let (start, sheet, other_id) = (
        window(&server, "Doc"),
        window(&server, "Sheet").id,
        window(&server, "Other").id,
    );
    let mut events = server.ipc().subscribe(vec![Topic::Windows]).unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();

    let serial = press_on(&server, &mut client, &start);
    parent.toplevel._move(&client.seat, serial);
    client.roundtrip();
    for step in 1..=3 {
        pointer_to(
            &server,
            (start.x + 10 + 20 * step, start.y + 10 + 10 * step),
        );
    }
    button(&server, ButtonState::Released);
    // A marker: everything before it has been sent.
    server
        .ipc()
        .action("minimize-window", Some(other_id))
        .unwrap();

    let mut sheet_changes = Vec::new();
    loop {
        match events.next_event().unwrap().expect("event stream ended") {
            Event::WindowChanged(info) if info.id == other_id => break,
            Event::WindowChanged(info) if info.id == sheet => sheet_changes.push(info),
            _ => {}
        }
    }
    let [last] = sheet_changes.as_slice() else {
        panic!("expected one change for the sheet, got {sheet_changes:?}");
    };
    assert_eq!((last.x, last.y), (start.x + 60 + 200, start.y + 30));
}

#[test]
fn a_window_with_a_sheet_passes_the_keyboard_to_it() {
    let server = Server::start();
    let mut client = server.connect();
    let parent = client.create_toplevel("Doc", APP, (600, 400));
    let setup = client.modal_of(&parent);
    let sheet = client.create_toplevel_with("Sheet", APP, (200, 100), setup);
    let mut other_client = server.connect();
    other_client.create_toplevel("Other", "org.neueos.Other", (100, 100));
    let focused = |server: &Server| {
        windows(server)
            .into_iter()
            .filter(|w| w.focused)
            .map(|w| w.title.unwrap_or_default())
            .collect::<Vec<_>>()
    };
    assert_eq!(focused(&server), ["Other"]);

    // A click on the parent, away from the sheet: the pair comes up and the sheet gets
    // the keyboard and the activated state; the parent doesn't.
    let p = window(&server, "Doc");
    pointer_to(&server, (p.x + 5, p.y + p.height - 5));
    button(&server, ButtonState::Pressed);
    button(&server, ButtonState::Released);
    client.roundtrip();
    assert_eq!(titles(&server), ["Other", "Doc", "Sheet"]);
    assert_eq!(focused(&server), ["Sheet"]);
    let activated = |t| {
        client
            .events(t)
            .states
            .contains(&xdg_toplevel::State::Activated)
    };
    assert!(activated(&sheet) && !activated(&parent));

    // Focusing the parent by other means does the same.
    server
        .ipc()
        .action("focus-window", Some(window(&server, "Other").id))
        .unwrap();
    assert_eq!(focused(&server), ["Other"]);
    server.ipc().action("focus-window", Some(p.id)).unwrap();
    assert_eq!(focused(&server), ["Sheet"]);

    // A sheet on the sheet takes it from there; when it closes, the keyboard goes back
    // to the first sheet, not to the window under both.
    let setup = client.modal_of(&sheet);
    let inner = client.create_toplevel_with("Inner", APP, (100, 50), setup);
    server
        .ipc()
        .action("focus-window", Some(window(&server, "Other").id))
        .unwrap();
    server.ipc().action("focus-window", Some(p.id)).unwrap();
    assert_eq!(focused(&server), ["Inner"]);
    client.destroy_toplevel(inner);
    assert_eq!(focused(&server), ["Sheet"]);

    // Once the sheet is gone the parent takes the keyboard itself again.
    client.destroy_toplevel(sheet);
    assert_eq!(focused(&server), ["Doc"]);
}

/// Press inside `info` and return the serial its client saw, for move/resize.
fn press_on(server: &Server, client: &mut TestClient, info: &WindowInfo) -> u32 {
    pointer_to(server, (info.x + 10, info.y + 10));
    button(server, ButtonState::Pressed);
    client.roundtrip();
    client.button_serial()
}
