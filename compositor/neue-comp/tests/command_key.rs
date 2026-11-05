//! Super as ⌘ (task 3.10a, ADR 0079): Super+key reaches third-party apps as Ctrl+key;
//! exempt apps (terminals), neue-kit apps and apps that inhibit shortcuts get Super, and
//! compositor bindings come first.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use harness::{Server, TestClient, Toplevel};
use smithay::backend::input::{InputTime, KeyState};
use smithay::input::keyboard::Keycode;

/// Evdev codes; plus 8 they are xkb keycodes.
const EVDEV_SUPER_L: u32 = 125;
const EVDEV_C: u32 = 46;
const EVDEV_W: u32 = 17;
/// `mods_depressed` bits of the default `us` keymap.
const CONTROL: u32 = 1 << 2;
const MOD4: u32 = 1 << 6;

fn key(server: &Server, evdev: u32, state: KeyState) {
    server.with_state(move |s| s.press_key(Keycode::new(evdev + 8), state, InputTime::now()));
}

/// Press and release Super+`evdev`.
fn super_chord(server: &Server, evdev: u32) {
    key(server, EVDEV_SUPER_L, KeyState::Pressed);
    key(server, evdev, KeyState::Pressed);
    key(server, evdev, KeyState::Released);
    key(server, EVDEV_SUPER_L, KeyState::Released);
}

/// A focused window of `app_id` on a new client.
fn app(server: &Server, app_id: &str) -> (TestClient, Toplevel) {
    let mut client = server.connect();
    let toplevel = client.create_toplevel(app_id, app_id, (300, 200));
    client.roundtrip();
    (client, toplevel)
}

/// Type Super+C into `client`'s window: whether it got C, and every modifier bit it saw
/// held meanwhile. Ends with nothing held.
fn super_c(server: &Server, client: &mut TestClient) -> (bool, u32) {
    let before = client.app_keys().len();
    super_chord(server, EVDEV_C);
    client.roundtrip();
    let got_c = client.app_keys()[before..].contains(&(EVDEV_C, true));
    let (now, seen) = client.app_modifiers();
    assert_eq!(now, 0, "the modifiers are released");
    (got_c, seen)
}

#[test]
fn super_reaches_a_third_party_app_as_ctrl() {
    let server = Server::start();
    let (mut firefox, _window) = app(&server, "firefox");
    let (got_c, seen) = super_c(&server, &mut firefox);
    assert!(got_c);
    assert_eq!(seen & (CONTROL | MOD4), CONTROL, "Ctrl+C, not Super+C");
}

#[test]
fn exempt_apps_get_super() {
    let server = Server::start();
    let (mut foot, _window) = app(&server, "foot");
    let (got_c, seen) = super_c(&server, &mut foot);
    assert!(got_c);
    assert_eq!(seen & (CONTROL | MOD4), MOD4, "terminals keep Super");
}

#[test]
fn kit_apps_get_super() {
    let server = Server::start();
    let (mut kit, window) = app(&server, "org.neueos.Gallery");
    // neue-kit apps bind neue_window_manager_v1 when they start.
    let _chrome = kit.neue_window(&window.surface);
    kit.roundtrip();
    let (got_c, seen) = super_c(&server, &mut kit);
    assert!(got_c);
    assert_eq!(seen & (CONTROL | MOD4), MOD4);
}

#[test]
fn apps_that_inhibit_shortcuts_get_super() {
    let server = Server::start();
    let (mut viewer, window) = app(&server, "virt-manager");
    viewer.inhibit_shortcuts(&window.surface);
    viewer.roundtrip();
    let (_, seen) = super_c(&server, &mut viewer);
    assert_eq!(seen & (CONTROL | MOD4), MOD4);
}

#[test]
fn the_exceptions_come_from_the_config() {
    let server =
        Server::start_with_config(Some("[super-as-command]\nexceptions = [\"Firefox\"]\n"));
    let (mut firefox, _window) = app(&server, "firefox");
    let (_, seen) = super_c(&server, &mut firefox);
    assert_eq!(seen & (CONTROL | MOD4), MOD4);
    drop(firefox);

    let (mut foot, _window) = app(&server, "foot");
    let (_, seen) = super_c(&server, &mut foot);
    assert_eq!(
        seen & (CONTROL | MOD4),
        CONTROL,
        "the list replaced the default"
    );
}

#[test]
fn compositor_bindings_still_win() {
    let server = Server::start_with_config(Some("[keybindings]\n\"Super+W\" = \"close-window\"\n"));
    let (mut firefox, window) = app(&server, "firefox");
    let before = firefox.app_keys().len();
    super_chord(&server, EVDEV_W);
    firefox.roundtrip();
    assert!(
        !firefox.app_keys()[before..]
            .iter()
            .any(|&(key, _)| key == EVDEV_W),
        "the binding took W"
    );
    assert!(firefox.events(&window).close_requested);
}
