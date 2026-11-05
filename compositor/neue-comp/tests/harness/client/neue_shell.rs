//! `neue_shell_v1` on the test client (task 3.1, ADR 0066): the shell's side, plus the
//! key events an app sees (for `send_shortcut`) and icons with images.

use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt;

use neue_protocols::neue_shell::client::{
    neue_shell_action_v1::{self, NeueShellActionV1},
    neue_shell_manager_v1::{self, KeyState, Modifiers, NeueShellManagerV1, SystemKey},
    neue_shell_toplevel_v1::{self, NeueShellToplevelV1},
};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, event_created_child};
use wayland_protocols::xdg::toplevel_icon::v1::client::{
    xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1, xdg_toplevel_icon_v1::XdgToplevelIconV1,
};

use super::{ClientData, TestClient, Toplevel};

/// How the shell sees one window. Events are applied as they arrive; `done` counts the
/// batches.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShellWindow {
    pub identifier: String,
    pub title: String,
    pub app_id: String,
    pub state: u32,
    pub icon_name: Option<String>,
    /// `(width, height, scale, pixels)` of each icon image.
    pub icon_images: Vec<(i32, i32, i32, Vec<u8>)>,
    pub menu: ShellMenu,
    pub done: usize,
    pub closed: bool,
    /// Every event name in order, to check that only changes are sent.
    pub events: Vec<&'static str>,
}

/// The menu events.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ShellMenu {
    #[default]
    Unknown,
    None,
    DbusMenu(String, String),
    Gmenu(String, Option<String>),
}

#[derive(Default)]
pub(super) struct NeueShellData {
    manager: Option<NeueShellManagerV1>,
    windows: Vec<(NeueShellToplevelV1, ShellWindow)>,
    /// Identifier of each `active` event's window (`None` for null), in order.
    active: Vec<Option<String>>,
    keys: Vec<(SystemKey, KeyState, Modifiers)>,
    /// `(serial, has an output)` of every `focus_menu_bar`.
    focus_menu_bar: Vec<(u32, bool)>,
    /// Outcomes of `run_action`, in order.
    actions: Vec<Result<(), String>>,
    /// `(key, pressed)` of every `wl_keyboard.key` (evdev codes).
    pub(super) app_keys: Vec<(u32, bool)>,
    /// `mods_depressed` of the last `wl_keyboard.modifiers`.
    pub(super) mods_depressed: u32,
    /// The highest `mods_depressed` seen.
    pub(super) mods_seen: u32,
}

impl TestClient {
    /// The name of `interface`'s global, if this client sees it.
    pub fn global_name(&self, interface: &str) -> Option<u32> {
        self._globals.contents().with_list(|list| {
            list.iter()
                .find(|g| g.interface == interface)
                .map(|g| g.name)
        })
    }

    /// Bind `neue_shell_manager_v1` (the client must be the shell) and take in the windows.
    pub fn bind_neue_shell(&mut self) -> NeueShellManagerV1 {
        let manager: NeueShellManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=2, ())
            .expect("binding neue_shell_manager_v1");
        self.data.neue_shell.manager = Some(manager.clone());
        self.roundtrip();
        manager
    }

    /// Bind global `name` as `neue_shell_manager_v1` whether we see it or not, and
    /// return the protocol error that follows, if any.
    pub fn force_bind_neue_shell(&mut self, name: u32) -> Result<(), String> {
        let registry: &WlRegistry = self._globals.registry();
        let _manager: NeueShellManagerV1 = registry.bind(name, 1, &self.qh, ());
        self.queue
            .roundtrip(&mut self.data)
            .map(drop)
            .map_err(|err| err.to_string())
    }

    /// The windows the shell was told about, oldest first (closed ones stay, flagged).
    pub fn shell_windows(&self) -> Vec<ShellWindow> {
        self.data
            .neue_shell
            .windows
            .iter()
            .map(|(_, w)| w.clone())
            .collect()
    }

    /// The shell's object for the window `identifier`.
    pub fn shell_toplevel(&self, identifier: &str) -> NeueShellToplevelV1 {
        self.data
            .neue_shell
            .windows
            .iter()
            .find(|(_, w)| w.identifier == identifier)
            .map(|(t, _)| t.clone())
            .unwrap_or_else(|| panic!("the shell has no {identifier}"))
    }

    /// Every `active` event so far (identifiers; `None` for null).
    pub fn shell_active(&self) -> &[Option<String>] {
        &self.data.neue_shell.active
    }

    /// Every `focus_menu_bar` event so far: `(serial, has an output)`.
    pub fn shell_focus_menu_bar(&self) -> &[(u32, bool)] {
        &self.data.neue_shell.focus_menu_bar
    }

    /// Every `system_key` event so far.
    pub fn shell_keys(&self) -> &[(SystemKey, KeyState, Modifiers)] {
        &self.data.neue_shell.keys
    }

    /// `run_action` and its outcome.
    pub fn shell_run_action(
        &mut self,
        name: &str,
        toplevel: Option<&NeueShellToplevelV1>,
    ) -> Result<(), String> {
        let manager = self.data.neue_shell.manager.clone().expect("not bound");
        let before = self.data.neue_shell.actions.len();
        manager.run_action(name.to_owned(), toplevel, &self.qh, ());
        // The outcome is sent after the dispatch that ran it, so after the first sync.
        self.roundtrip();
        self.roundtrip();
        assert_eq!(self.data.neue_shell.actions.len(), before + 1, "no outcome");
        self.data.neue_shell.actions[before].clone()
    }

    /// `send_shortcut`.
    pub fn shell_send_shortcut(&mut self, modifiers: Modifiers, keysym: u32) {
        let manager = self.data.neue_shell.manager.clone().expect("not bound");
        manager.send_shortcut(modifiers, keysym);
        self.roundtrip();
    }

    /// The `(evdev key, pressed)` events this client's keyboard got.
    pub fn app_keys(&self) -> &[(u32, bool)] {
        &self.data.neue_shell.app_keys
    }

    /// The modifiers depressed now, and the most seen at once.
    pub fn app_modifiers(&self) -> (u32, u32) {
        (
            self.data.neue_shell.mods_depressed,
            self.data.neue_shell.mods_seen,
        )
    }

    /// Give `toplevel` an icon with the name `name` (if any) and one `size` × `size`
    /// image of `argb` at scale 1, and commit it.
    pub fn set_icon_image(
        &mut self,
        toplevel: &Toplevel,
        name: Option<&str>,
        size: i32,
        argb: u32,
    ) {
        let manager: XdgToplevelIconManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding xdg_toplevel_icon_manager_v1");
        let icon: XdgToplevelIconV1 = manager.create_icon(&self.qh, ());
        if let Some(name) = name {
            icon.set_name(name.to_owned());
        }
        let buffer = self.shm_buffer_filled((size, size), argb);
        icon.add_buffer(&buffer, 1);
        manager.set_icon(&toplevel.toplevel, Some(&icon));
        toplevel.surface.commit();
        self.roundtrip();
        // The buffer stays alive with the client; the icon object can go.
        icon.destroy();
        self.roundtrip();
    }
}

impl NeueShellData {
    fn window(&mut self, toplevel: &NeueShellToplevelV1) -> &mut ShellWindow {
        &mut self
            .windows
            .iter_mut()
            .find(|(t, _)| t == toplevel)
            .expect("an event for an unknown toplevel")
            .1
    }
}

impl Dispatch<NeueShellManagerV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &NeueShellManagerV1,
        event: neue_shell_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let shell = &mut data.neue_shell;
        match event {
            neue_shell_manager_v1::Event::Toplevel { toplevel } => {
                shell.windows.push((toplevel, ShellWindow::default()));
            }
            neue_shell_manager_v1::Event::Active { toplevel } => {
                let id = toplevel.map(|t| shell.window(&t).identifier.clone());
                shell.active.push(id);
            }
            neue_shell_manager_v1::Event::SystemKey {
                key: WEnum::Value(key),
                state: WEnum::Value(state),
                modifiers: WEnum::Value(modifiers),
            } => shell.keys.push((key, state, modifiers)),
            neue_shell_manager_v1::Event::FocusMenuBar { serial, output } => {
                shell.focus_menu_bar.push((serial, output.is_some()));
            }
            _ => {}
        }
    }

    event_created_child!(ClientData, NeueShellManagerV1, [
        neue_shell_manager_v1::EVT_TOPLEVEL_OPCODE => (NeueShellToplevelV1, ()),
    ]);
}

impl Dispatch<NeueShellToplevelV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        toplevel: &NeueShellToplevelV1,
        event: neue_shell_toplevel_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let window = data.neue_shell.window(toplevel);
        use neue_shell_toplevel_v1::Event as E;
        let name = match event {
            E::Identifier { identifier } => {
                window.identifier = identifier;
                "identifier"
            }
            E::Title { title } => {
                window.title = title;
                "title"
            }
            E::AppId { app_id } => {
                window.app_id = app_id;
                "app_id"
            }
            E::State { state } => {
                window.state = match state {
                    WEnum::Value(s) => s.bits(),
                    WEnum::Unknown(bits) => bits,
                };
                "state"
            }
            E::Icon { name } => {
                window.icon_name = name;
                window.icon_images.clear();
                "icon"
            }
            E::IconBuffer {
                data,
                width,
                height,
                scale,
            } => {
                window.icon_images.push((
                    width,
                    height,
                    scale,
                    read_memfd(data, width * height * 4),
                ));
                "icon_buffer"
            }
            E::MenuDbusmenu { service, path } => {
                window.menu = ShellMenu::DbusMenu(service, path);
                "menu_dbusmenu"
            }
            E::MenuGmenu {
                bus_name,
                menubar_path,
                ..
            } => {
                window.menu = ShellMenu::Gmenu(bus_name, menubar_path);
                "menu_gmenu"
            }
            E::MenuNone => {
                window.menu = ShellMenu::None;
                "menu_none"
            }
            E::Done => {
                window.done += 1;
                "done"
            }
            E::Closed => {
                window.closed = true;
                toplevel.destroy();
                "closed"
            }
            _ => "?",
        };
        window.events.push(name);
    }
}

impl Dispatch<NeueShellActionV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &NeueShellActionV1,
        event: neue_shell_action_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let outcome = match event {
            neue_shell_action_v1::Event::Done => Ok(()),
            neue_shell_action_v1::Event::Failed { message } => Err(message),
            _ => return,
        };
        data.neue_shell.actions.push(outcome);
    }
}

/// `len` bytes from the start of a (sealed) memfd. Its file offset is not ours to rely
/// on, so read at offset 0.
fn read_memfd(fd: OwnedFd, len: i32) -> Vec<u8> {
    let file = std::fs::File::from(fd);
    let mut bytes = vec![0; usize::try_from(len).expect("negative size")];
    file.read_exact_at(&mut bytes, 0).expect("reading the icon");
    bytes
}
