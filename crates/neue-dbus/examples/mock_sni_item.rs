//! A stand-in tray app on the **session** bus, to check the shell's status-icon tray in the
//! VM (task 3.13c): a StatusNotifierItem "Mock Tray" with a drawn pixmap icon, a tooltip,
//! and a DBusMenu (Open, Mark Attention, a check item, Quit). It prints each call it gets
//! and runs until killed or until Quit is clicked.
//!
//! ```sh
//! cargo run -p neue-dbus --example mock_sni_item [--named]
//! ```
//!
//! `--named` gives the icon by theme name (`mail-unread`) instead of by pixmap. It waits
//! for a watcher to appear, so it can start before the shell. Mark Attention switches the
//! status to NeedsAttention (the attention icon is a red pixmap) and back.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use neue_dbus::status_notifier::item::{DEFAULT_PATH, INTERFACE};
use neue_dbus::status_notifier::watcher::{NAME as WATCHER, PATH as WATCHER_PATH};
use neue_dbus::zbus::object_server::SignalEmitter;
use neue_dbus::zbus::zvariant::{ObjectPath, OwnedObjectPath};
use neue_dbus::{MenuExport, MenuNode, Toggle, ToggleState, zbus};

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
type ToolTip = (String, Pixmaps, String, String);

const SIZE: i32 = 22;
const MENU_PATH: &str = "/StatusNotifierItem/Menu";

/// A filled disc in `argb` (alpha, red, green, blue) as the wire wants it.
fn disc(rgb: [u8; 3]) -> Pixmaps {
    let mut data = Vec::new();
    let c = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let alpha = (c + 0.5 - d).clamp(0.0, 1.0);
            data.extend_from_slice(&[(alpha * 255.0) as u8, rgb[0], rgb[1], rgb[2]]);
        }
    }
    vec![(SIZE, SIZE, data)]
}

struct Tray {
    named: bool,
    attention: Arc<Mutex<bool>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Tray {
    #[zbus(property)]
    fn id(&self) -> String {
        "neue-mock-tray".into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
        "Mock Tray".into()
    }
    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        if *self.attention.lock().unwrap_or_else(|e| e.into_inner()) {
            "NeedsAttention".into()
        } else {
            "Active".into()
        }
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        if self.named { "mail-unread" } else { "" }.into()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        if self.named {
            Vec::new()
        } else {
            disc([0x30, 0x80, 0xE0])
        }
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Pixmaps {
        disc([0xE0, 0x30, 0x30])
    }
    #[zbus(property)]
    fn tool_tip(&self) -> ToolTip {
        (
            String::new(),
            Vec::new(),
            "Mock Tray".into(),
            "Nothing to report".into(),
        )
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        ObjectPath::from_static_str_unchecked(MENU_PATH).into()
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, x: i32, y: i32) {
        println!("Activate {x} {y}");
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        println!("SecondaryActivate {x} {y}");
    }
    fn context_menu(&self, x: i32, y: i32) {
        println!("ContextMenu {x} {y}");
    }
    fn scroll(&self, delta: i32, orientation: &str) {
        println!("Scroll {delta} {orientation}");
    }

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

fn main() -> Result<()> {
    let named = std::env::args().any(|a| a == "--named");
    let conn = neue_dbus::connect_session()?;
    let attention = Arc::new(Mutex::new(false));
    conn.object_server().at(
        DEFAULT_PATH,
        Tray {
            named,
            attention: Arc::clone(&attention),
        },
    )?;
    let mut wrap = MenuNode::item(4, "Check Item");
    wrap.toggle = Toggle::Check(ToggleState::On);
    let menu_conn = conn.clone();
    let _menu = MenuExport::new(
        &conn,
        MENU_PATH,
        // A tray menu's items hang straight off the root, as in real tray apps (an app
        // menu has titles there).
        vec![
            MenuNode::item(2, "Open"),
            MenuNode::item(3, "Mark Attention"),
            wrap,
            MenuNode::separator(5),
            MenuNode::item(6, "Quit"),
        ],
        move |id| {
            println!("clicked {id}");
            match id {
                3 => {
                    let now = {
                        let mut a = attention.lock().unwrap_or_else(|e| e.into_inner());
                        *a = !*a;
                        *a
                    };
                    let status = if now { "NeedsAttention" } else { "Active" };
                    match SignalEmitter::new(menu_conn.inner(), DEFAULT_PATH) {
                        Ok(emitter) => {
                            let _ = zbus::block_on(Tray::new_status(&emitter, status));
                            let _ = zbus::block_on(Tray::new_icon(&emitter));
                        }
                        Err(err) => eprintln!("signal: {err}"),
                    }
                }
                6 => std::process::exit(0),
                _ => {}
            }
        },
    )?;
    let name = format!("org.example.MockTray-{}", std::process::id());
    conn.request_name(name.as_str())?;
    // Wait for a watcher (the shell may start later), then register as items do.
    loop {
        let registered = conn.call_method(
            Some(WATCHER),
            WATCHER_PATH,
            Some("org.kde.StatusNotifierWatcher"),
            "RegisterStatusNotifierItem",
            &(name.as_str(),),
        );
        match registered {
            Ok(_) => break,
            Err(err) => {
                eprintln!("waiting for a status-notifier watcher ({err})");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
    println!("registered as {name} (interface {INTERFACE})");
    loop {
        std::thread::park();
    }
}
