//! `neue_shell_v1` (task 3.1, ADR 0066): only the shell sees it; it hears about every
//! window, the active one and the system keys, and can act on windows.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::{Duration, Instant};

use harness::{Anchor, KeyboardInteractivity, Layer, Server, ShellMenu, TestClient};
use neue_ipc::WindowInfo;
use neue_protocols::neue_shell::client::neue_shell_manager_v1::{KeyState, Modifiers, SystemKey};
use smithay::backend::input::{InputTime, KeyState as Key};
use smithay::desktop::layer_map_for_output;
use smithay::input::keyboard::Keycode;

const MINIMIZED: u32 = 1;
const ATTENTION: u32 = 2;

/// xkb keycodes (evdev + 8) in the default `us` keymap.
const SHIFT_L: u32 = 42 + 8;
const VOLUME_UP: u32 = 115 + 8;
const BRIGHTNESS_UP: u32 = 225 + 8;
/// evdev codes, as `wl_keyboard.key` reports them.
const EVDEV_CTRL_L: u32 = 29;
const EVDEV_C: u32 = 46;
/// `Control`'s bit in the `us` keymap's modifier masks.
const CONTROL_MASK: u32 = 4;

fn windows(server: &Server) -> Vec<WindowInfo> {
    server.with_state(|state| state.windows())
}

fn window(server: &Server, title: &str) -> WindowInfo {
    windows(server)
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn ident(server: &Server, title: &str) -> String {
    format!("window-{}", window(server, title).id.0)
}

fn key(server: &Server, code: u32, state: Key) {
    server.with_state(move |s| s.press_key(Keycode::new(code), state, InputTime::now()));
}

/// A shell client with the manager bound.
fn shell(server: &Server) -> TestClient {
    let mut shell = server.connect_shell();
    shell.bind_neue_shell();
    shell
}

#[test]
fn only_the_shell_sees_the_global_and_others_cannot_bind_it() {
    let server = Server::start();
    let mut app = server.connect();
    let _win = app.create_toplevel("App", "org.neueos.App", (200, 100));
    let mut shell = server.connect_shell();
    assert!(shell.has_global("neue_shell_manager_v1"));
    assert!(!app.has_global("neue_shell_manager_v1"));

    // A second client that binds the name anyway is refused with a protocol error ...
    let name = shell.global_name("neue_shell_manager_v1").unwrap();
    let mut intruder = server.connect();
    let error = intruder.force_bind_neue_shell(name).unwrap_err();
    assert!(!error.is_empty());

    // ... while the shell binds it and gets the window list.
    shell.bind_neue_shell();
    assert_eq!(shell.shell_windows().len(), 1);
    app.roundtrip();
}

#[test]
fn windows_are_announced_with_ids_matching_neuectl_and_the_foreign_toplevel_list() {
    let server = Server::start();
    let mut app = server.connect();
    let _a = app.create_toplevel("First", "org.neueos.A", (200, 100));
    let _b = app.create_toplevel("Second", "org.neueos.B", (200, 100));
    app.track_foreign_toplevels();
    let mut shell = shell(&server);

    let seen = shell.shell_windows();
    assert_eq!(seen.len(), 2);
    let first = &seen[0];
    assert_eq!(first.identifier, ident(&server, "First"));
    assert_eq!(
        (first.title.as_str(), first.app_id.as_str()),
        ("First", "org.neueos.A")
    );
    assert_eq!(first.state, 0);
    assert_eq!(first.menu, ShellMenu::None);
    assert_eq!(first.icon_name, None);
    assert_eq!(first.done, 1);
    assert_eq!(first.events[0], "identifier");
    let foreign: Vec<String> = app
        .foreign_toplevels()
        .into_iter()
        .map(|t| t.identifier)
        .collect();
    let ours: Vec<String> = seen.iter().map(|w| w.identifier.clone()).collect();
    assert_eq!(foreign, ours);
    // The last mapped window has the focus.
    assert_eq!(shell.shell_active(), &[Some(ident(&server, "Second"))]);

    // A new window is announced and becomes active.
    let third = app.create_toplevel("Third", "org.neueos.C", (200, 100));
    shell.roundtrip();
    assert_eq!(shell.shell_windows().len(), 3);
    assert_eq!(
        shell.shell_active().last().unwrap().as_deref(),
        Some(ident(&server, "Third").as_str())
    );

    // A title change sends the title alone.
    third.toplevel.set_title("Renamed".into());
    app.roundtrip();
    shell.roundtrip();
    let renamed = &shell.shell_windows()[2];
    assert_eq!(renamed.title, "Renamed");
    assert_eq!(renamed.done, 2);
    assert_eq!(
        &renamed.events[renamed.events.len() - 2..],
        ["title", "done"]
    );

    // Closing it: active moves on, and the object is told it is gone.
    app.destroy_toplevel(third);
    shell.roundtrip();
    assert!(shell.shell_windows()[2].closed);
    assert_eq!(
        shell.shell_active().last().unwrap().as_deref(),
        Some(ident(&server, "Second").as_str())
    );
}

#[test]
fn a_restarted_shell_gets_the_full_list_again() {
    let server = Server::start();
    let mut app = server.connect();
    let _a = app.create_toplevel("Kept", "org.neueos.A", (200, 100));
    let first = shell(&server);
    drop(first);
    app.roundtrip();
    let second = shell(&server);
    let seen = second.shell_windows();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].title, "Kept");
    assert_eq!(second.shell_active(), &[Some(ident(&server, "Kept"))]);
}

#[test]
fn minimise_unminimise_activate_and_close() {
    let server = Server::start();
    let mut app = server.connect();
    let back = app.create_toplevel("Back", "org.neueos.A", (200, 100));
    let _front = app.create_toplevel("Front", "org.neueos.B", (200, 100));
    let mut shell = shell(&server);
    let back_id = ident(&server, "Back");
    let front_id = ident(&server, "Front");
    let front = shell.shell_toplevel(&front_id);

    front.minimize();
    shell.roundtrip();
    assert!(window(&server, "Front").minimized);
    assert_eq!(shell.shell_windows()[1].state, MINIMIZED);
    assert_eq!(shell.shell_active().last().unwrap(), &Some(back_id.clone()));

    front.unminimize();
    shell.roundtrip();
    assert!(!window(&server, "Front").minimized);
    assert_eq!(shell.shell_windows()[1].state, 0);
    assert_eq!(shell.shell_active().last().unwrap(), &Some(front_id));

    shell.shell_toplevel(&back_id).activate();
    shell.roundtrip();
    assert!(window(&server, "Back").focused);
    assert_eq!(shell.shell_active().last().unwrap(), &Some(back_id.clone()));

    shell.shell_toplevel(&back_id).close();
    shell.roundtrip();
    app.roundtrip();
    assert!(app.events(&back).close_requested);
}

#[test]
fn attention_menus_and_icons_are_passed_on() {
    let server = Server::start();
    let mut app = server.connect();
    let back = app.create_toplevel("Back", "org.neueos.A", (200, 100));
    let _front = app.create_toplevel("Front", "org.neueos.B", (200, 100));
    let mut shell = shell(&server);

    // The bell on a background window asks for attention.
    app.ring_bell(Some(&back.surface));
    shell.roundtrip();
    assert_eq!(shell.shell_windows()[0].state, ATTENTION);

    // Menu addresses (ADR 0025).
    let appmenu = app.kde_appmenu(&back.surface);
    appmenu.set_address(":1.42".into(), "/MenuBar".into());
    app.roundtrip();
    shell.roundtrip();
    assert_eq!(
        shell.shell_windows()[0].menu,
        ShellMenu::DbusMenu(":1.42".into(), "/MenuBar".into())
    );
    appmenu.release();
    app.roundtrip();
    shell.roundtrip();
    assert_eq!(shell.shell_windows()[0].menu, ShellMenu::None);

    // An icon name, then an image.
    app.set_icon_name(&back, "folder");
    shell.roundtrip();
    assert_eq!(
        shell.shell_windows()[0].icon_name.as_deref(),
        Some("folder")
    );
    app.set_icon_image(&back, None, 64, 0xff_33_66_99);
    shell.roundtrip();
    let icon = shell.shell_windows()[0].clone();
    assert_eq!(icon.icon_name, None);
    assert_eq!(icon.icon_images.len(), 1);
    let (w, h, scale, pixels) = &icon.icon_images[0];
    assert_eq!((*w, *h, *scale), (64, 64, 1));
    assert_eq!(&pixels[..4], &0xff_33_66_99_u32.to_le_bytes());
    // A commit that doesn't change the icon sends nothing.
    let events = icon.events.len();
    back.surface.commit();
    app.roundtrip();
    shell.roundtrip();
    assert_eq!(shell.shell_windows()[0].events.len(), events);

    // Activating clears the attention.
    shell.shell_toplevel(&icon.identifier).activate();
    shell.roundtrip();
    assert_eq!(shell.shell_windows()[0].state, 0);
}

#[test]
fn run_action_reports_its_outcome() {
    let server = Server::start();
    let mut app = server.connect();
    let _back = app.create_toplevel("Back", "org.neueos.A", (200, 100));
    let _front = app.create_toplevel("Front", "org.neueos.B", (200, 100));
    let mut shell = shell(&server);
    let back = shell.shell_toplevel(&ident(&server, "Back"));

    assert_eq!(shell.shell_run_action("zoom-window", Some(&back)), Ok(()));
    app.roundtrip();
    assert!(window(&server, "Back").zoomed);
    // No toplevel: the active window.
    assert_eq!(shell.shell_run_action("minimize-window", None), Ok(()));
    assert!(window(&server, "Front").minimized);
    let error = shell.shell_run_action("no-such-action", None).unwrap_err();
    assert!(error.contains("no-such-action"), "{error}");
    let error = shell.shell_run_action("move-window", None).unwrap_err();
    assert!(error.contains('x'), "{error}");
}

#[test]
fn send_shortcut_types_the_chord_into_the_active_window() {
    let server = Server::start();
    let mut app = server.connect();
    let _win = app.create_toplevel("Editor", "org.neueos.A", (200, 100));
    let mut shell = shell(&server);

    // The shell's menu takes the keyboard: the editor stays active.
    let _menu = shell.create_layer_surface(Layer::Top, "menu", (300, 30), |layer| {
        layer.set_size(300, 30);
        layer.set_anchor(Anchor::Top | Anchor::Left);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    });
    server.with_state(|s| {
        let output = s.space.outputs().next().unwrap().clone();
        let surface = layer_map_for_output(&output)
            .layers()
            .next()
            .unwrap()
            .wl_surface()
            .clone();
        s.focus_layer(&surface);
    });
    shell.roundtrip();
    assert!(!window(&server, "Editor").focused);
    assert_eq!(
        shell.shell_active().last().unwrap(),
        &Some(ident(&server, "Editor"))
    );

    // Ctrl+C: the editor gets the keyboard back and sees Control, then c.
    app.roundtrip();
    let before = app.app_keys().len();
    shell.shell_send_shortcut(Modifiers::Ctrl, 0x63);
    app.roundtrip();
    assert!(window(&server, "Editor").focused);
    assert_eq!(
        &app.app_keys()[before..],
        [
            (EVDEV_CTRL_L, true),
            (EVDEV_C, true),
            (EVDEV_C, false),
            (EVDEV_CTRL_L, false)
        ]
    );
    let (now, seen) = app.app_modifiers();
    assert_eq!(now, 0);
    assert_ne!(seen & CONTROL_MASK, 0);
}

#[test]
fn system_keys_go_to_the_shell_once_it_is_bound() {
    let server = Server::start();
    let mut app = server.connect();
    let _win = app.create_toplevel("App", "org.neueos.A", (200, 100));

    // No shell: the focused client gets the key.
    key(&server, VOLUME_UP, Key::Pressed);
    key(&server, VOLUME_UP, Key::Released);
    app.roundtrip();
    assert_eq!(app.key_presses(), 1);

    let mut shell = shell(&server);
    key(&server, VOLUME_UP, Key::Pressed);
    key(&server, VOLUME_UP, Key::Released);
    key(&server, SHIFT_L, Key::Pressed);
    key(&server, BRIGHTNESS_UP, Key::Pressed);
    key(&server, BRIGHTNESS_UP, Key::Released);
    key(&server, SHIFT_L, Key::Released);
    shell.roundtrip();
    app.roundtrip();
    assert_eq!(
        shell.shell_keys(),
        [
            (SystemKey::VolumeUp, KeyState::Pressed, Modifiers::empty()),
            (SystemKey::VolumeUp, KeyState::Released, Modifiers::empty()),
            (SystemKey::BrightnessUp, KeyState::Pressed, Modifiers::Shift),
            (
                SystemKey::BrightnessUp,
                KeyState::Released,
                Modifiers::Shift
            ),
        ]
    );
    // The app saw Shift only.
    assert_eq!(app.key_presses(), 2);
}

#[test]
fn the_minimise_rectangle_follows_the_shell_surface() {
    let server = Server::start();
    let mut app = server.connect();
    let _win = app.create_toplevel("App", "org.neueos.A", (200, 100));
    let mut shell = shell(&server);
    let dock = shell.create_layer_surface(Layer::Top, "dock", (400, 60), |layer| {
        layer.set_size(400, 60);
        layer.set_anchor(Anchor::Top | Anchor::Left);
        layer.set_margin(0, 0, 0, 100);
    });
    let toplevel = shell.shell_toplevel(&ident(&server, "App"));
    let id = window(&server, "App").id;
    let target = move |server: &Server| {
        server.with_state(move |s| {
            let window = s.window_by_id(id).unwrap();
            s.minimize_target(&window)
                .map(|r| (r.loc.x, r.loc.y, r.size.w, r.size.h))
        })
    };

    toplevel.set_minimize_rectangle(&dock.surface, 10, 5, 48, 48);
    shell.roundtrip();
    let output = server.with_state(|s| {
        let o = s.space.outputs().next().unwrap().clone();
        s.space.output_geometry(&o).unwrap().loc
    });
    assert_eq!(
        target(&server),
        Some((output.x + 110, output.y + 5, 48, 48))
    );

    // A surface of another client is ignored; width 0 unsets.
    toplevel.set_minimize_rectangle(&dock.surface, 0, 0, 0, 48);
    shell.roundtrip();
    assert_eq!(target(&server), None);
}

#[test]
fn the_shell_command_is_started_on_a_private_socket_and_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("starts");
    let command = format!("echo \"$WAYLAND_SOCKET\" >> '{}'", log.display());
    let _server = Server::start_with_shell(&command);
    // It exits at once, so it is restarted after a short pause (ADR 0066).
    let deadline = Instant::now() + Duration::from_secs(10);
    let starts = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let starts: Vec<String> = text.lines().map(ToOwned::to_owned).collect();
        if starts.len() >= 2 || Instant::now() > deadline {
            break starts;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(starts.len() >= 2, "started {} times", starts.len());
    for fd in starts {
        assert!(fd.parse::<i32>().is_ok(), "WAYLAND_SOCKET={fd:?}");
    }
}
