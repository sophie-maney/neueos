//! Exports a small menu bar on the session bus and prints its address, then waits for
//! clicks (printed) until killed. For checking DBusMenu clients by hand:
//!
//! ```sh
//! cargo run -p neue-dbus --example menu_demo
//! gdbus introspect --session --dest <service> --object-path /org/neueos/MenuBar/0
//! ```

use anyhow::Result;
use neue_dbus::{MENU_ROOT, MenuExport, MenuNode, Toggle, ToggleState};

fn main() -> Result<()> {
    let conn = neue_dbus::connect_session()?;
    let mut open = MenuNode::item(2, "Open…");
    open.shortcut = vec!["Control".into(), "O".into()];
    let mut wrap = MenuNode::item(5, "Word Wrap");
    wrap.toggle = Toggle::Check(ToggleState::On);
    let items = vec![
        MenuNode::submenu(1, "File", vec![open, MenuNode::separator(3)]),
        MenuNode::submenu(4, "View", vec![wrap]),
    ];
    let path = format!("{MENU_ROOT}/0");
    let _export = MenuExport::new(&conn, &path, items, |id| println!("clicked {id}"))?;
    println!(
        "{} {path}",
        conn.unique_name()
            .map(|n| n.to_string())
            .unwrap_or_default()
    );
    loop {
        std::thread::park();
    }
}
