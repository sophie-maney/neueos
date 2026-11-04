//! A scripted input method (input-method-v2) for checking text-input-v3 clients (task 2.2).
//!
//! On every activation it sets a preedit, waits 1.5 s, then commits a string:
//! `ime_probe [PREEDIT] [COMMIT]`, by default "にほん" and "日本". Pass Latin text to check
//! rendering: the spike only has DejaVu Sans, which has no CJK glyphs.
//! fcitx5 can't be used under neue-comp yet: it needs `zwp_virtual_keyboard_manager_v1`,
//! which neue-comp doesn't offer (ADR 0016).
//!
//! Start it before the client gets keyboard focus (Smithay sends text-input `enter` only
//! while an input method is bound):
//!
//! ```sh
//! cargo run -p neue-platform --example ime_probe
//! ```

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::time::Duration;

    use wayland_client::globals::{GlobalListContents, registry_queue_init};
    use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
    use wayland_client::{Connection, Dispatch, QueueHandle};
    use wayland_protocols_misc::zwp_input_method_v2::client::{
        zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        zwp_input_method_v2::{self, ZwpInputMethodV2},
    };

    let mut args = std::env::args().skip(1);
    let preedit = args.next().unwrap_or_else(|| "にほん".into());
    let commit = args.next().unwrap_or_else(|| "日本".into());

    #[derive(Default)]
    struct Probe {
        /// Number of `done` events: the serial `commit` must carry.
        serial: u32,
        pending_active: Option<bool>,
        active: bool,
        unavailable: bool,
        /// Set when an activation needs the script run.
        run: bool,
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Probe {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    impl Dispatch<WlSeat, ()> for Probe {
        fn event(
            _: &mut Self,
            _: &WlSeat,
            _: wayland_client::protocol::wl_seat::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    impl Dispatch<ZwpInputMethodManagerV2, ()> for Probe {
        fn event(
            _: &mut Self,
            _: &ZwpInputMethodManagerV2,
            _: <ZwpInputMethodManagerV2 as wayland_client::Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    impl Dispatch<ZwpInputMethodV2, ()> for Probe {
        fn event(
            state: &mut Self,
            _: &ZwpInputMethodV2,
            event: zwp_input_method_v2::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                zwp_input_method_v2::Event::Activate => state.pending_active = Some(true),
                zwp_input_method_v2::Event::Deactivate => state.pending_active = Some(false),
                zwp_input_method_v2::Event::Done => {
                    state.serial += 1;
                    if let Some(active) = state.pending_active.take() {
                        if active && !state.active {
                            state.run = true;
                        }
                        state.active = active;
                        eprintln!(
                            "ime_probe: {}",
                            if active { "activated" } else { "deactivated" }
                        );
                    }
                }
                zwp_input_method_v2::Event::Unavailable => state.unavailable = true,
                _ => {}
            }
        }
    }

    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Probe>(&conn)?;
    let qh = queue.handle();
    let seat: WlSeat = globals.bind(&qh, 1..=9, ())?;
    let manager: ZwpInputMethodManagerV2 = globals.bind(&qh, 1..=1, ())?;
    let im = manager.get_input_method(&seat, &qh, ());
    let mut probe = Probe::default();
    eprintln!("ime_probe: input method bound; waiting for a text input");
    loop {
        queue.blocking_dispatch(&mut probe)?;
        if probe.unavailable {
            anyhow::bail!("another input method is already bound");
        }
        if probe.run {
            probe.run = false;
            let len = i32::try_from(preedit.len())?;
            im.set_preedit_string(preedit.clone(), len, len);
            im.commit(probe.serial);
            conn.flush()?;
            eprintln!("ime_probe: preedit {preedit}");
            std::thread::sleep(Duration::from_millis(1500));
            queue.dispatch_pending(&mut probe)?;
            if probe.active {
                im.commit_string(commit.clone());
                im.commit(probe.serial);
                conn.flush()?;
                eprintln!("ime_probe: commit {commit}");
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("ime_probe runs on Linux under a Wayland compositor only");
}
