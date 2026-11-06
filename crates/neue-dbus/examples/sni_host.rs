//! Owns the StatusNotifierWatcher on the session bus, hosts its items and prints what a
//! tray would show (task 3.13a), until killed. For checking the watcher and host against
//! real tray apps (`nm-applet --indicator`) or `mock_sni_item`, with no shell running:
//!
//! ```sh
//! cargo run -p neue-dbus --example sni_host
//! ```
//!
//! Stop the shell first (it owns the watcher name once 3.13c lands).

use std::sync::mpsc;
use std::thread;

use anyhow::Result;
use neue_dbus::status_notifier::{ItemChanged, ItemSignals, WatcherEvent, WatcherSignals};
use neue_dbus::{Host, HostEvent, Watcher};

enum Input {
    Watcher(WatcherEvent),
    Item(ItemChanged),
}

fn main() -> Result<()> {
    let conn = neue_dbus::connect_session()?;
    let _watcher = Watcher::own(&conn)?;
    let host = Host::new(&conn);
    // Subscribe before reading what is there, so nothing registering meanwhile is missed.
    let (tx, rx) = mpsc::channel();
    let signals = WatcherSignals::new(&conn)?;
    let sender = tx.clone();
    thread::spawn(move || {
        for event in signals {
            if sender.send(Input::Watcher(event)).is_err() {
                return;
            }
        }
    });
    let signals = ItemSignals::new(&conn)?;
    thread::spawn(move || {
        for change in signals {
            if tx.send(Input::Item(change)).is_err() {
                return;
            }
        }
    });
    println!("host name: {}", host.announce()?);
    for event in host.initial()? {
        show(&event);
    }
    for input in rx {
        let event = match input {
            Input::Watcher(WatcherEvent::ItemRegistered(address)) => host.registered(&address),
            Input::Watcher(WatcherEvent::ItemUnregistered(address)) => host.unregistered(&address),
            Input::Item(change) => host.changed(&change),
        };
        if let Some(event) = event {
            show(&event);
        }
    }
    Ok(())
}

fn show(event: &HostEvent) {
    match event {
        HostEvent::Added { id, info } | HostEvent::Changed { id, info } => {
            let kind = if matches!(event, HostEvent::Added { .. }) {
                "added"
            } else {
                "changed"
            };
            let icon = info.current_icon();
            println!(
                "{kind} {id}: {:?} status {:?} icon {:?} +{} pixmaps, tooltip {:?}, menu {:?}",
                info.display_name(),
                info.status,
                icon.name,
                icon.pixmaps.len(),
                info.tooltip.as_ref().map(|t| t.title.as_str()),
                info.menu,
            );
        }
        HostEvent::Removed { id } => println!("removed {id}"),
    }
}
