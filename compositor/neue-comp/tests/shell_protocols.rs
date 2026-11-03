//! The shell-facing protocols (task 1.12, ADR 0015), one binding at a time on the headless
//! backend: layer shell, session lock, idle notify and inhibit, foreign toplevel list,
//! xdg-foreign, xdg-dialog, xdg-toplevel-icon, xdg-system-bell and keyboard-shortcuts
//! inhibit.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::Duration;

use harness::{Anchor, KeyboardInteractivity, Layer, Server, TestClient, Toplevel};
use neue_ipc::WindowInfo;
use smithay::backend::input::{ButtonState, InputTime, KeyState};
use smithay::input::keyboard::Keycode;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::shell::xdg::dialog::ToplevelDialogHint;
use smithay::wayland::xdg_toplevel_icon::ToplevelIconCachedState;
use wayland_client::Proxy;

const APP: &str = "org.neueos.Test";
const WIDTH: i32 = 1920;
const HEIGHT: i32 = 1080;
/// Evdev codes plus 8 are xkb keycodes.
const KEY_SUPER_L: u32 = 125 + 8;
const KEY_W: u32 = 17 + 8;
const BTN_LEFT: u32 = 0x110;

fn windows(server: &Server) -> Vec<WindowInfo> {
    server.with_state(|state| state.windows())
}

fn window(server: &Server, title: &str) -> WindowInfo {
    windows(server)
        .into_iter()
        .find(|w| w.title.as_deref() == Some(title))
        .unwrap_or_else(|| panic!("no window titled {title}"))
}

fn key(server: &Server, code: u32, state: KeyState) {
    server.with_state(move |s| s.press_key(Keycode::new(code), state, InputTime::now()));
}

/// Press and release Super+W, which `compositor.toml` binds to `close-window`.
fn super_w(server: &Server) {
    key(server, KEY_SUPER_L, KeyState::Pressed);
    key(server, KEY_W, KeyState::Pressed);
    key(server, KEY_W, KeyState::Released);
    key(server, KEY_SUPER_L, KeyState::Released);
}

fn pointer_to(server: &Server, x: f64, y: f64) {
    server.with_state(move |s| s.move_pointer((x, y).into(), InputTime::now()));
}

/// A 300×200 toplevel called `title`.
fn toplevel(client: &mut TestClient, title: &str) -> Toplevel {
    client.create_toplevel(title, APP, (300, 200))
}

/// Whether the keyboard focus is a layer surface.
fn layer_has_keyboard(server: &Server) -> bool {
    server.with_state(|state| {
        state
            .keyboard
            .current_focus()
            .is_some_and(|focus| state.layer_for_surface(&focus).is_some())
    })
}

/// A 30 px panel along the top edge that reserves its space.
fn top_panel(client: &mut TestClient) -> harness::LayerHandle {
    client.create_layer_surface(Layer::Top, "panel", (WIDTH, 30), |surface| {
        surface.set_size(0, 30);
        surface.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
        surface.set_exclusive_zone(30);
    })
}

// ---- wlr-layer-shell ----

#[test]
fn layer_shell_is_advertised_and_sizes_an_anchored_panel() {
    let server = Server::start();
    let mut client = server.connect();
    assert!(client.has_global("zwlr_layer_shell_v1"));
    let panel = top_panel(&mut client);
    // Anchored to both sides with width 0: the compositor picks the output's width.
    assert_eq!(client.layer_events(&panel).size, (WIDTH as u32, 30));
    assert_eq!(panel.size, (WIDTH, 30));
}

#[test]
fn a_panel_reserves_space_and_windows_stay_below_it() {
    let server = Server::start();
    let mut client = server.connect();
    let full = server.with_state(|s| {
        let output = s.outputs().next().cloned().unwrap();
        s.work_area(&output)
    });
    assert_eq!((full.loc.y, full.size.h), (0, HEIGHT));

    let panel = top_panel(&mut client);
    let work = server.with_state(|s| {
        let output = s.outputs().next().cloned().unwrap();
        s.work_area(&output)
    });
    assert_eq!((work.loc.y, work.size.h), (30, HEIGHT - 30));

    let _window = toplevel(&mut client, "below the panel");
    assert!(window(&server, "below the panel").y >= 30);

    // The panel goes away and the space comes back.
    client.destroy_layer(panel);
    let work = server.with_state(|s| {
        let output = s.outputs().next().cloned().unwrap();
        s.work_area(&output)
    });
    assert_eq!((work.loc.y, work.size.h), (0, HEIGHT));
}

#[test]
fn a_panel_changing_its_zone_refits_a_zoomed_window() {
    let server = Server::start();
    let mut client = server.connect();
    let mut win = toplevel(&mut client, "zoomed");
    let id = window(&server, "zoomed").id;
    server.with_state(move |s| {
        let w = s.window_by_id(id).unwrap();
        s.set_zoomed(&w, true, None);
    });
    client.ack_and_commit(&mut win);
    let before = window(&server, "zoomed");

    let _panel = top_panel(&mut client);
    // The compositor asks the window to shrink; it complies.
    client.ack_and_commit(&mut win);
    let after = window(&server, "zoomed");
    assert_eq!(after.height, before.height - 30, "{before:?} {after:?}");
    assert_eq!(after.y, before.y + 30);
}

#[test]
fn pointer_input_goes_to_layers_above_windows_first() {
    let server = Server::start();
    let mut client = server.connect();
    let _window = toplevel(&mut client, "w");
    let panel = top_panel(&mut client);

    // Over the panel the pointer is on the panel's surface; on the empty desktop nowhere.
    let panel_id = panel.surface.id().protocol_id();
    let on_panel = server.with_state(|s| s.surface_under((400.0, 10.0).into()));
    assert!(on_panel.is_some_and(|(surface, _)| surface.id().protocol_id() == panel_id));
    assert!(
        server
            .with_state(|s| s.surface_under((400.0, 700.0).into()))
            .is_none()
    );
}

#[test]
fn an_exclusive_layer_surface_takes_the_keyboard_and_gives_it_back() {
    let server = Server::start();
    let mut client = server.connect();
    let _window = toplevel(&mut client, "w");
    let id = window(&server, "w").id;
    assert_eq!(server.with_state(|s| s.focused_window_id()), Some(id));

    let launcher = client.create_layer_surface(Layer::Overlay, "launcher", (400, 60), |surface| {
        surface.set_size(400, 60);
        surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    });
    assert!(layer_has_keyboard(&server));
    assert_eq!(server.with_state(|s| s.focused_window_id()), None);

    // A window cannot take the keyboard back while the launcher is up.
    server.with_state(move |s| {
        let w = s.window_by_id(id).unwrap();
        s.focus_window(&w);
    });
    assert!(layer_has_keyboard(&server));

    client.destroy_layer(launcher);
    assert!(!layer_has_keyboard(&server));
    assert_eq!(server.with_state(|s| s.focused_window_id()), Some(id));
}

#[test]
fn an_on_demand_layer_surface_gets_the_keyboard_when_clicked() {
    let server = Server::start();
    let mut client = server.connect();
    let _window = toplevel(&mut client, "w");
    let _dock = client.create_layer_surface(Layer::Top, "dock", (200, 50), |surface| {
        surface.set_size(200, 50);
        surface.set_anchor(Anchor::Bottom);
        surface.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
    });
    // Mapping alone does not take the keyboard.
    assert!(!layer_has_keyboard(&server));

    pointer_to(&server, f64::from(WIDTH / 2), f64::from(HEIGHT - 20));
    server.with_state(|s| s.press_button(BTN_LEFT, ButtonState::Pressed, InputTime::now()));
    server.with_state(|s| s.press_button(BTN_LEFT, ButtonState::Released, InputTime::now()));
    assert!(layer_has_keyboard(&server));
}

#[test]
fn a_layer_surface_goes_away_with_its_client() {
    let server = Server::start();
    let panel_zone = |server: &Server| {
        server.with_state(|s| {
            let output = s.outputs().next().cloned().unwrap();
            s.work_area(&output).loc.y
        })
    };
    {
        let mut client = server.connect();
        let _panel = top_panel(&mut client);
        assert_eq!(panel_zone(&server), 30);
    }
    // Another round trip gives the compositor a loop iteration to notice the client left.
    server.connect().roundtrip();
    assert_eq!(panel_zone(&server), 0);
}

// ---- ext-session-lock ----

#[test]
fn session_lock_locks_gives_the_lock_surface_input_and_unlocks() {
    let server = Server::start_with_config(Some("[keybindings]\n\"Super+W\" = \"close-window\"\n"));
    let mut app = server.connect();
    let mut win = toplevel(&mut app, "secret");
    let mut locker = server.connect();
    assert!(locker.has_global("ext_session_lock_manager_v1"));

    let lock = locker.lock_session();
    assert!(locker.lock_events(&lock).locked);
    assert!(server.with_state(|s| s.is_locked()));

    let mut surface = locker.create_lock_surface(&lock);
    let events = locker.lock_surface_events(&surface);
    assert_eq!(events.size, (WIDTH as u32, HEIGHT as u32));
    locker.ack_lock_surface(&mut surface);

    // Only the lock surface is under the pointer and has the keyboard.
    let under = server.with_state(|s| s.surface_under((300.0, 300.0).into()));
    let lock_id = surface.surface.id().protocol_id();
    assert!(under.is_some_and(|(s, _)| s.id().protocol_id() == lock_id));
    let focused = server.with_state(|s| s.keyboard.current_focus().map(|f| f.id().protocol_id()));
    assert_eq!(focused, Some(lock_id));
    assert_eq!(server.with_state(|s| s.focused_window_id()), None);

    // Bindings and window focus are off while locked.
    super_w(&server);
    app.roundtrip();
    assert!(!app.events(&win).close_requested);
    app.ack_and_commit(&mut win);

    locker.unlock(lock);
    assert!(!server.with_state(|s| s.is_locked()));
    let id = window(&server, "secret").id;
    assert_eq!(server.with_state(|s| s.focused_window_id()), Some(id));
    super_w(&server);
    app.roundtrip();
    assert!(app.events(&win).close_requested);
}

#[test]
fn a_second_locker_is_refused_while_the_first_is_alive() {
    let server = Server::start();
    let mut first = server.connect();
    let mut second = server.connect();
    let lock = first.lock_session();
    assert!(first.lock_events(&lock).locked);

    let other = second.lock_session();
    assert!(second.lock_events(&other).finished);
    assert!(!second.lock_events(&other).locked);
    assert!(server.with_state(|s| s.is_locked()));
}

#[test]
fn the_session_stays_locked_when_the_locker_dies_and_a_new_one_can_unlock() {
    let server = Server::start();
    {
        let mut locker = server.connect();
        let _lock = locker.lock_session();
        assert!(server.with_state(|s| s.is_locked()));
    }
    // Wait for the compositor to see the client go.
    let mut next = server.connect();
    next.roundtrip();
    assert!(server.with_state(|s| s.is_locked()));

    let lock = next.lock_session();
    assert!(next.lock_events(&lock).locked);
    next.unlock(lock);
    assert!(!server.with_state(|s| s.is_locked()));
}

#[test]
fn an_output_added_while_locked_is_locked_too() {
    let server = Server::start();
    let mut locker = server.connect();
    let _lock = locker.lock_session();
    server
        .with_state(|s| s.headless_add_output("HEADLESS-2", (800, 600)))
        .unwrap();
    let locked = server.with_state(|s| {
        let output = s.output_by_name("HEADLESS-2").unwrap();
        neue_comp::lock::with_output_lock(&output, |lock| lock.locked)
    });
    assert!(locked);
}

// ---- ext-idle-notify and idle-inhibit ----

const IDLE_MS: u32 = 60;

fn wait_past_idle() {
    std::thread::sleep(Duration::from_millis(u64::from(IDLE_MS) * 3));
}

#[test]
fn idle_notify_reports_idle_and_resume() {
    let server = Server::start();
    let mut client = server.connect();
    assert!(client.has_global("ext_idle_notifier_v1"));
    let idle = client.idle_notification(IDLE_MS);
    assert_eq!(client.idle_events(idle).idled, 0);

    wait_past_idle();
    client.roundtrip();
    assert_eq!(client.idle_events(idle).idled, 1);

    // Any input is activity.
    pointer_to(&server, 10.0, 10.0);
    client.roundtrip();
    assert_eq!(client.idle_events(idle).resumed, 1);
}

#[test]
fn idle_inhibit_holds_idle_notifications_back() {
    let server = Server::start();
    let mut client = server.connect();
    assert!(client.has_global("zwp_idle_inhibit_manager_v1"));
    let win = toplevel(&mut client, "video");
    let inhibitor = client.inhibit_idle(&win.surface);
    let idle = client.idle_notification(IDLE_MS);

    wait_past_idle();
    client.roundtrip();
    assert_eq!(client.idle_events(idle).idled, 0, "inhibited");

    inhibitor.destroy();
    client.roundtrip();
    wait_past_idle();
    client.roundtrip();
    assert_eq!(client.idle_events(idle).idled, 1, "released");
}

// ---- ext-foreign-toplevel-list ----

#[test]
fn foreign_toplevel_list_announces_updates_and_closes() {
    let server = Server::start();
    let mut app = server.connect();
    let first = app.create_toplevel("First", "org.example.First", (200, 100));

    let mut shell = server.connect();
    assert!(shell.has_global("ext_foreign_toplevel_list_v1"));
    shell.track_foreign_toplevels();
    // Windows that exist when the list is bound are announced.
    let list = shell.foreign_toplevels();
    assert_eq!(list.len(), 1);
    let id = window(&server, "First").id;
    assert_eq!(list[0].title, "First");
    assert_eq!(list[0].app_id, "org.example.First");
    assert_eq!(list[0].identifier, format!("window-{}", id.0));

    // A new window, a changed title, a closed window.
    let second = toplevel(&mut app, "Second");
    shell.roundtrip();
    assert_eq!(shell.foreign_toplevels().len(), 2);
    first.toplevel.set_title("Renamed".into());
    app.roundtrip();
    shell.roundtrip();
    assert_eq!(shell.foreign_toplevels()[0].title, "Renamed");

    app.destroy_toplevel(second);
    shell.roundtrip();
    let list = shell.foreign_toplevels();
    assert!(!list[0].closed);
    assert!(list[1].closed);
}

// ---- xdg-foreign ----

#[test]
fn xdg_foreign_parents_a_window_of_another_client() {
    let server = Server::start();
    let mut owner = server.connect();
    let mut dialog_client = server.connect();
    let parent = toplevel(&mut owner, "parent");
    let child = toplevel(&mut dialog_client, "child");

    let handle = owner.export_toplevel(&parent);
    dialog_client.import_parent_of(&handle, &child);

    let parent_id = window(&server, "parent").id;
    assert_eq!(window(&server, "child").parent, Some(parent_id));
}

// ---- xdg-dialog ----

#[test]
fn xdg_dialog_records_the_modal_flag() {
    let server = Server::start();
    let mut client = server.connect();
    let parent = toplevel(&mut client, "parent");
    let child = client.create_toplevel_with("modal", APP, (200, 100), |t| {
        t.set_parent(Some(&parent.toplevel));
    });
    let hint = |server: &Server| {
        let id = window(server, "modal").id;
        server.with_state(move |s| {
            let w = s.window_by_id(id).unwrap();
            s.dialog_hint(&w)
        })
    };
    assert_eq!(hint(&server), ToplevelDialogHint::Unknown);
    client.set_modal(&child);
    assert_eq!(hint(&server), ToplevelDialogHint::Modal);
}

// ---- xdg-toplevel-icon ----

#[test]
fn toplevel_icon_announces_sizes_and_stores_the_name() {
    let server = Server::start();
    let mut client = server.connect();
    let mut sizes = client.toplevel_icon_sizes();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![64, 128]);

    let win = toplevel(&mut client, "iconic");
    client.set_icon_name(&win, "folder");
    let id = window(&server, "iconic").id;
    let name = server.with_state(move |s| {
        let w = s.window_by_id(id).unwrap();
        let surface = w.toplevel().unwrap().wl_surface().clone();
        smithay::wayland::compositor::with_states(&surface, |states| {
            states
                .cached_state
                .get::<ToplevelIconCachedState>()
                .current()
                .icon_name()
                .map(ToOwned::to_owned)
        })
    });
    assert_eq!(name.as_deref(), Some("folder"));
}

// ---- xdg-system-bell ----

#[test]
fn system_bell_asks_for_attention_for_a_background_window() {
    let server = Server::start();
    let mut client = server.connect();
    let background = toplevel(&mut client, "background");
    let _front = toplevel(&mut client, "front");
    assert!(!window(&server, "background").demands_attention);

    client.ring_bell(Some(&background.surface));
    assert!(window(&server, "background").demands_attention);
    // A bell for the whole client is harmless.
    client.ring_bell(None);
}

// ---- keyboard-shortcuts-inhibit ----

#[test]
fn shortcuts_inhibit_hands_bindings_to_the_focused_client() {
    let server = Server::start_with_config(Some("[keybindings]\n\"Super+W\" = \"close-window\"\n"));
    let mut client = server.connect();
    let mut win = toplevel(&mut client, "game");
    client.ack_and_commit(&mut win);

    let inhibitor = client.inhibit_shortcuts(&win.surface);
    assert!(client.shortcuts_inhibited(inhibitor), "focused: active");
    super_w(&server);
    client.roundtrip();
    assert!(
        !client.events(&win).close_requested,
        "the key went to the app"
    );
    assert!(client.key_presses() >= 1);

    client.destroy_shortcut_inhibitor(inhibitor);
    super_w(&server);
    client.roundtrip();
    assert!(client.events(&win).close_requested, "the binding is back");
}

#[test]
fn shortcuts_inhibit_waits_for_focus() {
    let server = Server::start();
    let mut client = server.connect();
    let background = toplevel(&mut client, "background");
    let _front = toplevel(&mut client, "front");

    let inhibitor = client.inhibit_shortcuts(&background.surface);
    assert!(!client.shortcuts_inhibited(inhibitor), "not focused yet");

    let id = window(&server, "background").id;
    server.with_state(move |s| {
        let w = s.window_by_id(id).unwrap();
        s.focus_window(&w);
    });
    client.roundtrip();
    assert!(client.shortcuts_inhibited(inhibitor), "focused now");
}
