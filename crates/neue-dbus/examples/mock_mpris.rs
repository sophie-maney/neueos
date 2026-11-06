//! A stand-in media player on the **session** bus, to check the media keys in the VM (task
//! 3.14d): an MPRIS player named `org.mpris.MediaPlayer2.<name>` that starts in the given
//! state, prints each call it gets and follows it (Play, Pause, PlayPause, Stop change
//! `PlaybackStatus` and announce it). Runs until killed.
//!
//! ```sh
//! cargo run -p neue-dbus --example mock_mpris [name] [Playing|Paused|Stopped]
//! playerctl -p <name> status    # what the keys did
//! ```

use std::sync::Mutex;

use anyhow::Result;
use neue_dbus::zbus::object_server::SignalEmitter;
use neue_dbus::{mpris, zbus};

struct Player {
    name: String,
    status: Mutex<&'static str>,
}

impl Player {
    fn set(&self, status: &'static str) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }
    fn log(&self, call: &str) {
        println!("{}: {call}", self.name);
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        (*self.status.lock().unwrap_or_else(|e| e.into_inner())).to_owned()
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
    async fn play(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.log("Play");
        self.set("Playing");
        let _ = self.playback_status_changed(&emitter).await;
    }
    async fn pause(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.log("Pause");
        self.set("Paused");
        let _ = self.playback_status_changed(&emitter).await;
    }
    async fn play_pause(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.log("PlayPause");
        let playing = *self.status.lock().unwrap_or_else(|e| e.into_inner()) == "Playing";
        self.set(if playing { "Paused" } else { "Playing" });
        let _ = self.playback_status_changed(&emitter).await;
    }
    async fn stop(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.log("Stop");
        self.set("Stopped");
        let _ = self.playback_status_changed(&emitter).await;
    }
    fn next(&self) {
        self.log("Next");
    }
    fn previous(&self) {
        self.log("Previous");
    }
}

struct Root;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
        "Mock Player".into()
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "neuemock".into());
    let status = match args.next().as_deref() {
        Some("Paused") => "Paused",
        Some("Stopped") => "Stopped",
        _ => "Playing",
    };
    let conn = neue_dbus::connect_session()?;
    let objects = conn.object_server();
    objects.at(mpris::PATH, Root)?;
    objects.at(
        mpris::PATH,
        Player {
            name: name.clone(),
            status: Mutex::new(status),
        },
    )?;
    conn.request_name(format!("{}{name}", mpris::NAME_PREFIX).as_str())?;
    println!("{name}: {status}");
    std::thread::park();
    Ok(())
}
