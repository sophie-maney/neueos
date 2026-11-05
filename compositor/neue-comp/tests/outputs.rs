//! Several outputs on the headless backend: layout, per-output fractional scale, windows
//! moving between outputs, hotplug and mode changes (task 1.7, ADR 0010).
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::Duration;

use harness::{Anchor, Layer, Server, TestClient, Toplevel};
use neue_ipc::{
    ActionParams, AdaptiveSyncInfo, AdaptiveSyncMode, Event, OutputInfo, Subscription, Topic,
    WindowId, WindowInfo,
};

/// `HEADLESS-2` at 1.5: 1920×1080 pixels, 1280×720 logical.
const CONFIG: &str = "[outputs.\"HEADLESS-2\"]\nscale = 1.5\n";

/// A compositor with `HEADLESS-1` (scale 1) and `HEADLESS-2` (scale 1.5), both 1920×1080.
fn two_outputs() -> Server {
    let server = Server::start_with_config(Some(CONFIG));
    server
        .with_state(|state| state.headless_add_output("HEADLESS-2", (1920, 1080)))
        .unwrap();
    server
}

fn outputs(server: &Server) -> Vec<OutputInfo> {
    server.ipc().outputs().unwrap()
}

fn output(server: &Server, name: &str) -> OutputInfo {
    outputs(server)
        .into_iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("no output {name}"))
}

/// Logical geometry of the output called `name`, as `(x, y, w, h)`.
fn logical_geometry(server: &Server, name: &'static str) -> Option<(i32, i32, i32, i32)> {
    server.with_state(move |state| {
        let output = state.output_by_name(name)?;
        let g = state.space.output_geometry(&output)?;
        Some((g.loc.x, g.loc.y, g.size.w, g.size.h))
    })
}

fn only_window(server: &Server) -> WindowInfo {
    let windows = server.ipc().windows().unwrap();
    assert_eq!(windows.len(), 1, "{windows:?}");
    windows.into_iter().next().unwrap()
}

fn move_window(server: &Server, id: WindowId, x: i32, y: i32) {
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

/// A client with one 400×300 window whose preferred scale it tracks.
fn client_with_window(server: &Server) -> (TestClient, Toplevel, WindowId) {
    let mut client = server.connect();
    let window = client.create_toplevel("Scaled", "org.example.Scaled", (400, 300));
    client.track_scale(&window);
    let id = only_window(server).id;
    (client, window, id)
}

fn preferred_scale(client: &mut TestClient, window: &Toplevel) -> Option<u32> {
    client.roundtrip();
    client.events(window).preferred_scale
}

fn subscribe(server: &Server, topics: Vec<Topic>) -> Subscription {
    let events = server.ipc().subscribe(topics).unwrap();
    events
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    events
}

/// Read events until `pick` returns something; fails the test on a timeout.
fn wait_for<T>(events: &mut Subscription, mut pick: impl FnMut(Event) -> Option<T>) -> T {
    loop {
        let event = events
            .next_event()
            .expect("waiting for an event")
            .expect("connection closed");
        if let Some(found) = pick(event) {
            return found;
        }
    }
}

#[test]
fn outputs_are_laid_out_left_to_right_at_their_scales() {
    let server = two_outputs();
    let outputs = outputs(&server);
    let summary: Vec<_> = outputs
        .iter()
        .map(|o| (o.name.as_str(), o.x, o.y, o.width, o.height, o.scale))
        .collect();
    assert_eq!(
        summary,
        [
            ("HEADLESS-1", 0, 0, 1920, 1080, 1.0),
            ("HEADLESS-2", 1920, 0, 1920, 1080, 1.5),
        ]
    );
    assert_eq!(
        logical_geometry(&server, "HEADLESS-2"),
        Some((1920, 0, 1280, 720))
    );

    let client = server.connect();
    for global in [
        "wp_viewporter",
        "wp_fractional_scale_manager_v1",
        "wl_output",
    ] {
        assert!(client.has_global(global), "{global} is not advertised");
    }
}

#[test]
fn windows_take_the_scale_of_the_output_they_are_mostly_on() {
    let server = two_outputs();
    let (mut client, window, id) = client_with_window(&server);
    assert_eq!(preferred_scale(&mut client, &window), Some(120));

    move_window(&server, id, 2200, 100);
    assert_eq!(preferred_scale(&mut client, &window), Some(180));

    // 220 px on HEADLESS-1, 180 px on HEADLESS-2: HEADLESS-1 wins.
    move_window(&server, id, 1700, 100);
    assert_eq!(preferred_scale(&mut client, &window), Some(120));
    // 120 px on HEADLESS-1, 280 px on HEADLESS-2.
    move_window(&server, id, 1800, 100);
    assert_eq!(preferred_scale(&mut client, &window), Some(180));
}

#[test]
fn unplugging_an_output_moves_its_windows_and_the_pointer() {
    let server = two_outputs();
    let mut events = subscribe(&server, vec![Topic::Outputs]);
    let (mut client, window, id) = client_with_window(&server);
    move_window(&server, id, 2200, 100);
    server.with_state(|state| {
        state.move_pointer(
            (2500.0, 300.0).into(),
            smithay::backend::input::InputTime::now(),
        );
    });

    server
        .with_state(|state| state.headless_remove_output("HEADLESS-2"))
        .unwrap();

    let removed = wait_for(&mut events, |event| match event {
        Event::OutputRemoved { name } => Some(name),
        _ => None,
    });
    assert_eq!(removed, "HEADLESS-2");
    assert_eq!(outputs(&server).len(), 1);
    // Same offset from the output's corner: (2200 - 1920, 100).
    let info = only_window(&server);
    assert_eq!((info.x, info.y), (280, 100));
    assert_eq!(preferred_scale(&mut client, &window), Some(120));
    let pointer = server.with_state(|state| state.pointer.current_location());
    assert_eq!((pointer.x, pointer.y), (960.0, 540.0));

    // Plugging it back in lays it out again.
    server
        .with_state(|state| state.headless_add_output("HEADLESS-2", (1920, 1080)))
        .unwrap();
    let added = wait_for(&mut events, |event| match event {
        Event::OutputAdded(info) => Some(info),
        _ => None,
    });
    assert_eq!(
        (added.name.as_str(), added.x, added.scale),
        ("HEADLESS-2", 1920, 1.5)
    );
}

#[test]
fn a_mode_change_moves_later_outputs_and_refits_zoomed_windows() {
    let server = two_outputs();
    let (mut client, mut window, id) = client_with_window(&server);
    server.ipc().action("zoom-window", Some(id)).unwrap();
    client.ack_and_commit(&mut window);
    let info = only_window(&server);
    assert_eq!((info.width, info.height), (1920, 1080));

    let mut events = subscribe(&server, vec![Topic::Outputs]);
    server
        .with_state(|state| state.headless_set_mode("HEADLESS-1", (1280, 800)))
        .unwrap();
    let changed: Vec<OutputInfo> = (0..2)
        .map(|_| {
            wait_for(&mut events, |event| match event {
                Event::OutputChanged(info) => Some(info),
                _ => None,
            })
        })
        .collect();
    let first = changed.iter().find(|o| o.name == "HEADLESS-1").unwrap();
    assert_eq!((first.width, first.height), (1280, 800));
    let second = changed.iter().find(|o| o.name == "HEADLESS-2").unwrap();
    assert_eq!((second.x, second.y), (1280, 0));

    // The zoomed window is asked to fill the smaller output.
    client.ack_and_commit(&mut window);
    let info = only_window(&server);
    assert_eq!((info.x, info.y, info.width, info.height), (0, 0, 1280, 800));
}

#[test]
fn config_reload_moves_and_rescales_outputs() {
    let server = two_outputs();
    let (mut client, window, id) = client_with_window(&server);
    move_window(&server, id, 2200, 100);
    assert_eq!(preferred_scale(&mut client, &window), Some(180));

    let mut events = subscribe(&server, vec![Topic::Outputs, Topic::Config]);
    server.write_config("[outputs.\"HEADLESS-2\"]\nscale = 2\nposition = [-960, -100]\n");
    wait_for(&mut events, |event| match event {
        Event::ConfigReloaded { .. } => Some(()),
        Event::ConfigError(info) => panic!("config rejected: {info}"),
        _ => None,
    });

    let second = output(&server, "HEADLESS-2");
    assert_eq!((second.x, second.y, second.scale), (-960, -100, 2.0));
    assert_eq!(
        logical_geometry(&server, "HEADLESS-2"),
        Some((-960, -100, 960, 540))
    );
    // The window was left on no output, so it was pulled onto HEADLESS-1.
    let info = only_window(&server);
    assert!(
        info.x >= 0 && info.x + info.width <= 1920,
        "window not on HEADLESS-1: {info:?}"
    );
    assert_eq!(preferred_scale(&mut client, &window), Some(120));

    // Onto the moved output: scale 2.
    move_window(&server, id, -900, 0);
    assert_eq!(preferred_scale(&mut client, &window), Some(240));
}

#[test]
fn layer_surfaces_take_their_outputs_scale() {
    let server = Server::start_with_config(Some("[outputs.\"HEADLESS-1\"]\nscale = 1.5\n"));
    let mut client = server.connect();
    let strip = client.create_layer_surface(Layer::Top, "strip", (1280, 30), |surface| {
        surface.set_size(0, 30);
        surface.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
    });
    client.track_layer_scale(&strip);
    client.roundtrip();
    assert_eq!(client.layer_events(&strip).preferred_scale, Some(180));

    let mut events = subscribe(&server, vec![Topic::Outputs, Topic::Config]);
    server.write_config("[outputs.\"HEADLESS-1\"]\nscale = 2\n");
    wait_for(&mut events, |event| match event {
        Event::ConfigReloaded { .. } => Some(()),
        Event::ConfigError(info) => panic!("config rejected: {info}"),
        _ => None,
    });
    client.roundtrip();
    assert_eq!(client.layer_events(&strip).preferred_scale, Some(240));
}

#[test]
fn adaptive_sync_setting_is_shown_and_reloads() {
    let server = two_outputs();
    let off = AdaptiveSyncInfo::default();
    assert_eq!(output(&server, "HEADLESS-1").adaptive_sync, off);
    assert_eq!(output(&server, "HEADLESS-2").adaptive_sync, off);

    let mut events = subscribe(&server, vec![Topic::Outputs, Topic::Config]);
    server.write_config("[outputs.\"HEADLESS-2\"]\nscale = 1.5\nadaptive-sync = \"fullscreen\"\n");
    let changed = wait_for(&mut events, |event| match event {
        Event::OutputChanged(info) => Some(info),
        Event::ConfigError(info) => panic!("config rejected: {info}"),
        _ => None,
    });
    // Headless outputs cannot do it: the mode is shown, but never active.
    let fullscreen = AdaptiveSyncInfo {
        mode: AdaptiveSyncMode::Fullscreen,
        supported: false,
        active: false,
    };
    assert_eq!(changed.name, "HEADLESS-2");
    assert_eq!(changed.adaptive_sync, fullscreen);
    assert_eq!(output(&server, "HEADLESS-1").adaptive_sync, off);

    // A typo is rejected and the old setting stays.
    server.write_config("[outputs.\"HEADLESS-2\"]\nadaptive-sync = \"always\"\n");
    wait_for(&mut events, |event| match event {
        Event::ConfigError(_) => Some(()),
        _ => None,
    });
    assert_eq!(output(&server, "HEADLESS-2").adaptive_sync, fullscreen);
}
