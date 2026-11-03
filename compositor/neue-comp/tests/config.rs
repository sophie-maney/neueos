//! `compositor.toml` against the headless compositor: start-up, hot reload through the file
//! watcher, `neuectl reload-config` and rejected files (ADR 0004, ADR 0009).
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::time::Duration;

use harness::Server;
use neue_ipc::{ClientError, ConfigErrorInfo, ErrorCode, Event, Subscription, Topic};
use smithay::input::keyboard::{Keysym, ModifiersState};

/// Subscribe to config events on a new connection.
fn config_events(server: &Server) -> Subscription {
    let subscription = server.ipc().subscribe(vec![Topic::Config]).unwrap();
    subscription
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    subscription
}

/// The next config event; fails the test if none arrives in time.
fn next(events: &mut Subscription) -> Event {
    events
        .next_event()
        .expect("waiting for a config event")
        .expect("connection closed")
}

fn config_error(event: Event) -> ConfigErrorInfo {
    match event {
        Event::ConfigError(info) => info,
        other => panic!("expected config_error, got {other:?}"),
    }
}

/// Names of the xkb layouts the keyboard has now.
fn layouts(server: &Server) -> Vec<String> {
    server.with_state(|state| {
        let keyboard = state.keyboard.clone();
        keyboard.with_xkb_state(state, |context| {
            let xkb = context.xkb().lock().unwrap();
            xkb.layouts()
                .map(|layout| xkb.layout_name(layout).to_owned())
                .collect()
        })
    })
}

fn bound(server: &Server, logo: bool, shift: bool, keysym: Keysym) -> Option<String> {
    server.with_state(move |state| {
        let modifiers = ModifiersState {
            logo,
            shift,
            ..ModifiersState::default()
        };
        state
            .config
            .bindings
            .lookup(&modifiers, keysym)
            .map(ToOwned::to_owned)
    })
}

#[test]
fn startup_applies_the_file() {
    let server = Server::start_with_config(Some(
        "[keyboard]\nlayouts = [\"us\", \"de\"]\n[keybindings]\n\"Super+W\" = \"close-window\"\n",
    ));
    assert_eq!(layouts(&server).len(), 2);
    assert_eq!(
        bound(&server, true, false, Keysym::w).as_deref(),
        Some("close-window")
    );
    // The built-in default stays.
    assert_eq!(
        bound(&server, true, false, Keysym::q).as_deref(),
        Some("quit")
    );
}

#[test]
fn no_file_means_defaults() {
    let server = Server::start();
    assert_eq!(layouts(&server).len(), 1);
    assert_eq!(
        bound(&server, true, false, Keysym::q).as_deref(),
        Some("quit")
    );
}

#[test]
fn changing_the_xkb_layout_applies_without_restart() {
    let server = Server::start_with_config(Some("[keyboard]\nlayouts = [\"us\"]\n"));
    let mut events = config_events(&server);
    assert_eq!(layouts(&server).len(), 1);

    server.write_config(
        "[keyboard]\nlayouts = [\"us\", \"de\"]\noptions = [\"grp:alt_shift_toggle\"]\n",
    );
    assert!(matches!(next(&mut events), Event::ConfigReloaded { .. }));
    let after = layouts(&server);
    assert_eq!(after.len(), 2, "{after:?}");
    assert!(after[1].contains("German"), "{after:?}");

    // Back to one layout, and the repeat rate changes too.
    server.write_config("[keyboard]\nlayouts = [\"us\"]\nrepeat-rate = 50\n");
    assert!(matches!(next(&mut events), Event::ConfigReloaded { .. }));
    assert_eq!(layouts(&server).len(), 1);
    assert_eq!(
        server.with_state(|s| s.config.active.keyboard.repeat_rate),
        50
    );
}

#[test]
fn keybindings_reload_and_unbind() {
    let server = Server::start();
    let mut events = config_events(&server);
    server.write_config(
        "[keybindings]\n\"Super+Q\" = \"none\"\n\"super+shift+w\" = \"close-window\"\n",
    );
    assert!(matches!(next(&mut events), Event::ConfigReloaded { .. }));
    assert_eq!(bound(&server, true, false, Keysym::q), None);
    assert_eq!(
        bound(&server, true, true, Keysym::W).as_deref(),
        Some("close-window")
    );
}

#[test]
fn an_invalid_file_is_rejected_and_the_old_config_stays() {
    let server = Server::start_with_config(Some("[keyboard]\nlayouts = [\"us\", \"de\"]\n"));
    let mut events = config_events(&server);

    // Wrong type on line 3.
    server.write_config("[keyboard]\nlayouts = [\"us\"]\nrepeat-rate = \"fast\"\n");
    let info = config_error(next(&mut events));
    assert!(info.file.ends_with("compositor.toml"), "{info}");
    assert_eq!(info.line, Some(3), "{info}");
    assert_eq!(layouts(&server).len(), 2);

    // A syntax error.
    server.write_config("[keyboard\n");
    let info = config_error(next(&mut events));
    assert_eq!(info.line, Some(1), "{info}");
    assert_eq!(layouts(&server).len(), 2);

    // Fixing the file recovers.
    server.write_config("[keyboard]\nlayouts = [\"us\"]\n");
    assert!(matches!(next(&mut events), Event::ConfigReloaded { .. }));
    assert_eq!(layouts(&server).len(), 1);
}

#[test]
fn values_only_the_compositor_can_check_are_rejected_too() {
    let server = Server::start_with_config(Some("[keyboard]\nlayouts = [\"us\", \"de\"]\n"));
    let mut events = config_events(&server);

    for (text, needle) in [
        ("[keyboard]\nlayouts = [\"nonexistent-layout\"]\n", "xkb"),
        (
            "[keybindings]\n\"Super+X\" = \"no-such-action\"\n",
            "unknown action",
        ),
        (
            "[keybindings]\n\"Super+Nonsense\" = \"quit\"\n",
            "not a key name",
        ),
    ] {
        server.write_config(text);
        let info = config_error(next(&mut events));
        assert!(info.message.contains(needle), "{info}");
        assert_eq!(layouts(&server).len(), 2, "{text}");
    }
}

#[test]
fn reload_config_forces_a_reload_and_reports_errors() {
    let server = Server::start();
    let mut ipc = server.ipc();
    ipc.reload_config().unwrap();
    assert!(
        ipc.actions()
            .unwrap()
            .iter()
            .any(|a| a.name == "reload-config")
    );

    // Write straight into the file and ask for a reload before the watcher's debounce is up:
    // the reply carries the verdict either way.
    server.write_config("[keyboard]\nrepeat-delay = 0\n");
    match ipc.reload_config() {
        Err(ClientError::Remote(body)) => {
            assert_eq!(body.code, ErrorCode::ConfigError);
            assert!(
                body.message.contains("compositor.toml:2:"),
                "{}",
                body.message
            );
        }
        other => panic!("expected a config_error reply, got {other:?}"),
    }
    assert_eq!(
        server.with_state(|s| s.config.active.keyboard.repeat_delay),
        300
    );

    server.write_config("[keyboard]\nrepeat-delay = 250\n");
    ipc.reload_config().unwrap();
    assert_eq!(
        server.with_state(|s| s.config.active.keyboard.repeat_delay),
        250
    );
}

#[test]
fn the_reload_config_action_works_over_neuectl() {
    let server = Server::start();
    server.write_config("[pointer]\nnatural-scroll = true\n");
    server.ipc().action("reload-config", None).unwrap();
    assert!(server.with_state(|s| s.config.active.pointer.natural_scroll));
}
