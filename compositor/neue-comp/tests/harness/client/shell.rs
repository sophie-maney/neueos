//! The shell-facing protocols on the test client (task 1.12): layer shell, session lock,
//! idle notify and inhibit, foreign toplevel list, xdg-foreign, xdg-dialog, toplevel icon,
//! system bell and keyboard-shortcuts inhibit.
//!
//! Managers are bound on first use. Objects are kept in [`ShellData`] and addressed by
//! index, like toplevels.

use wayland_client::protocol::{wl_output::WlOutput, wl_surface::WlSurface};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop, event_created_child,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use wayland_protocols::wp::idle_inhibit::zv1::client::{
    zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::{self, ZwpKeyboardShortcutsInhibitorV1},
};
use wayland_protocols::xdg::dialog::v1::client::{
    xdg_dialog_v1::XdgDialogV1, xdg_wm_dialog_v1::XdgWmDialogV1,
};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::ZxdgExporterV2,
    zxdg_imported_v2::ZxdgImportedV2,
    zxdg_importer_v2::ZxdgImporterV2,
};
use wayland_protocols::xdg::system_bell::v1::client::xdg_system_bell_v1::XdgSystemBellV1;
use wayland_protocols::xdg::toplevel_icon::v1::client::{
    xdg_toplevel_icon_manager_v1::{self, XdgToplevelIconManagerV1},
    xdg_toplevel_icon_v1::XdgToplevelIconV1,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};

pub use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::Anchor,
    zwlr_layer_surface_v1::KeyboardInteractivity,
};

use super::{ClientData, TestClient, Toplevel};
use wayland_client::protocol::wl_buffer::WlBuffer;

/// A mapped layer surface.
pub struct LayerHandle {
    index: usize,
    pub surface: WlSurface,
    pub layer_surface: ZwlrLayerSurfaceV1,
    buffer: Option<WlBuffer>,
    /// Size of the attached buffer.
    pub size: (i32, i32),
}

/// What the compositor told one layer surface.
#[derive(Debug, Default, Clone)]
pub struct LayerEvents {
    pub configure_serial: Option<u32>,
    /// Size of the last configure.
    pub size: (u32, u32),
    pub closed: bool,
}

/// A session lock the client asked for.
pub struct SessionLock {
    index: usize,
    pub lock: ExtSessionLockV1,
}

/// What the compositor told a session lock.
#[derive(Debug, Default, Clone, Copy)]
pub struct LockEvents {
    pub locked: bool,
    pub finished: bool,
}

/// A lock surface and its buffer.
pub struct LockSurfaceHandle {
    index: usize,
    pub surface: WlSurface,
    pub lock_surface: ExtSessionLockSurfaceV1,
    buffer: Option<WlBuffer>,
}

/// What the compositor told a lock surface.
#[derive(Debug, Default, Clone, Copy)]
pub struct LockSurfaceEvents {
    pub configure_serial: Option<u32>,
    pub size: (u32, u32),
}

/// What the compositor told an idle notification.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdleEvents {
    pub idled: u32,
    pub resumed: u32,
}

/// One entry of the foreign toplevel list, as the client sees it.
#[derive(Debug, Default, Clone)]
pub struct ForeignToplevel {
    pub identifier: String,
    pub title: String,
    pub app_id: String,
    pub closed: bool,
}

#[derive(Default)]
pub(super) struct ShellData {
    output: Option<WlOutput>,
    layers: Vec<LayerEvents>,
    locks: Vec<LockEvents>,
    lock_surfaces: Vec<LockSurfaceEvents>,
    idle: Vec<IdleEvents>,
    foreign: Vec<(ExtForeignToplevelHandleV1, ForeignToplevel)>,
    exported_handle: Option<String>,
    icon_sizes: Vec<i32>,
    shortcut_inhibitors: Vec<(ZwpKeyboardShortcutsInhibitorV1, bool)>,
    /// Objects that must stay alive for their effect to last.
    dialogs: Vec<XdgDialogV1>,
    imported: Vec<ZxdgImportedV2>,
    exported: Vec<ZxdgExportedV2>,
}

impl TestClient {
    fn bind_output(&mut self) -> WlOutput {
        if let Some(output) = &self.data.shell.output {
            return output.clone();
        }
        let output: WlOutput = self
            ._globals
            .bind(&self.qh, 1..=4, ())
            .expect("binding wl_output");
        self.data.shell.output = Some(output.clone());
        self.roundtrip();
        output
    }

    /// Bind the `n`th `wl_output` global (in the order the compositor added them).
    pub fn output_global(&mut self, n: usize) -> WlOutput {
        let name = self
            ._globals
            .contents()
            .with_list(|list| {
                list.iter()
                    .filter(|g| g.interface == "wl_output")
                    .nth(n)
                    .map(|g| g.name)
            })
            .expect("no such wl_output");
        let output = self
            ._globals
            .registry()
            .bind::<WlOutput, _, _>(name, 4, &self.qh, ());
        self.roundtrip();
        output
    }

    // ---- wlr-layer-shell ----

    /// Create a layer surface on the first output: `setup` sets size, anchor and so on
    /// before the first (buffer-less) commit; then ack the configure and commit a buffer
    /// of the configured size (`fallback` where the compositor leaves a side to us).
    pub fn create_layer_surface(
        &mut self,
        layer: Layer,
        namespace: &str,
        fallback: (i32, i32),
        setup: impl FnOnce(&ZwlrLayerSurfaceV1),
    ) -> LayerHandle {
        let shell: ZwlrLayerShellV1 = self
            ._globals
            .bind(&self.qh, 1..=4, ())
            .expect("binding zwlr_layer_shell_v1");
        let index = self.data.shell.layers.len();
        self.data.shell.layers.push(LayerEvents::default());
        let surface = self.compositor.create_surface(&self.qh, ());
        let layer_surface =
            shell.get_layer_surface(&surface, None, layer, namespace.to_owned(), &self.qh, index);
        setup(&layer_surface);
        surface.commit();
        self.roundtrip();

        let mut handle = LayerHandle {
            index,
            surface,
            layer_surface,
            buffer: None,
            size: fallback,
        };
        self.ack_layer(&mut handle, fallback);
        handle
    }

    /// Ack the last configure of `layer` and commit a buffer of the configured size
    /// (`fallback` for a side the compositor left at 0).
    pub fn ack_layer(&mut self, layer: &mut LayerHandle, fallback: (i32, i32)) {
        self.roundtrip();
        let events = self.data.shell.layers[layer.index].clone();
        let serial = events.configure_serial.expect("no layer configure to ack");
        layer.layer_surface.ack_configure(serial);
        let side = |configured: u32, fallback: i32| {
            if configured > 0 {
                configured as i32
            } else {
                fallback
            }
        };
        let size = (
            side(events.size.0, fallback.0),
            side(events.size.1, fallback.1),
        );
        let buffer = self.shm_buffer(size);
        layer.surface.attach(Some(&buffer), 0, 0);
        layer.surface.damage_buffer(0, 0, size.0, size.1);
        if let Some(old) = layer.buffer.replace(buffer) {
            old.destroy();
        }
        layer.size = size;
        layer.surface.commit();
        self.roundtrip();
    }

    /// What the compositor told `layer`.
    pub fn layer_events(&self, layer: &LayerHandle) -> &LayerEvents {
        &self.data.shell.layers[layer.index]
    }

    /// Destroy `layer` and its surface.
    pub fn destroy_layer(&mut self, layer: LayerHandle) {
        layer.layer_surface.destroy();
        layer.surface.destroy();
        if let Some(buffer) = layer.buffer {
            buffer.destroy();
        }
        self.roundtrip();
    }

    // ---- ext-session-lock ----

    /// Ask to lock the session.
    pub fn lock_session(&mut self) -> SessionLock {
        let manager: ExtSessionLockManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding ext_session_lock_manager_v1");
        let index = self.data.shell.locks.len();
        self.data.shell.locks.push(LockEvents::default());
        let lock = manager.lock(&self.qh, index);
        self.roundtrip();
        SessionLock { index, lock }
    }

    /// What the compositor told `lock`.
    pub fn lock_events(&self, lock: &SessionLock) -> LockEvents {
        self.data.shell.locks[lock.index]
    }

    /// Make a lock surface for the first output. It has no buffer yet: see
    /// [`TestClient::ack_lock_surface`].
    pub fn create_lock_surface(&mut self, lock: &SessionLock) -> LockSurfaceHandle {
        let output = self.bind_output();
        let index = self.data.shell.lock_surfaces.len();
        self.data
            .shell
            .lock_surfaces
            .push(LockSurfaceEvents::default());
        let surface = self.compositor.create_surface(&self.qh, ());
        let lock_surface = lock
            .lock
            .get_lock_surface(&surface, &output, &self.qh, index);
        self.roundtrip();
        LockSurfaceHandle {
            index,
            surface,
            lock_surface,
            buffer: None,
        }
    }

    /// What the compositor told `surface`.
    pub fn lock_surface_events(&self, surface: &LockSurfaceHandle) -> LockSurfaceEvents {
        self.data.shell.lock_surfaces[surface.index]
    }

    /// Ack the configure of `surface` and commit a buffer of the configured size.
    pub fn ack_lock_surface(&mut self, surface: &mut LockSurfaceHandle) {
        self.roundtrip();
        let events = self.data.shell.lock_surfaces[surface.index];
        let serial = events.configure_serial.expect("no lock surface configure");
        surface.lock_surface.ack_configure(serial);
        let size = (events.size.0 as i32, events.size.1 as i32);
        let buffer = self.shm_buffer(size);
        surface.surface.attach(Some(&buffer), 0, 0);
        surface.surface.damage_buffer(0, 0, size.0, size.1);
        if let Some(old) = surface.buffer.replace(buffer) {
            old.destroy();
        }
        surface.surface.commit();
        self.roundtrip();
    }

    /// Unlock the session and destroy the lock.
    pub fn unlock(&mut self, lock: SessionLock) {
        lock.lock.unlock_and_destroy();
        self.roundtrip();
    }

    // ---- ext-idle-notify and idle-inhibit ----

    /// Ask to be told when the seat has been idle for `timeout_ms`.
    pub fn idle_notification(&mut self, timeout_ms: u32) -> usize {
        let notifier: ExtIdleNotifierV1 = self
            ._globals
            .bind(&self.qh, 1..=2, ())
            .expect("binding ext_idle_notifier_v1");
        let index = self.data.shell.idle.len();
        self.data.shell.idle.push(IdleEvents::default());
        notifier.get_idle_notification(timeout_ms, &self.seat, &self.qh, index);
        self.roundtrip();
        index
    }

    /// What the compositor told idle notification `index`.
    pub fn idle_events(&self, index: usize) -> IdleEvents {
        self.data.shell.idle[index]
    }

    /// Inhibit idling while `surface` exists.
    pub fn inhibit_idle(&mut self, surface: &WlSurface) -> ZwpIdleInhibitorV1 {
        let manager: ZwpIdleInhibitManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zwp_idle_inhibit_manager_v1");
        let inhibitor = manager.create_inhibitor(surface, &self.qh, ());
        self.roundtrip();
        inhibitor
    }

    // ---- ext-foreign-toplevel-list ----

    /// Bind the foreign toplevel list; [`TestClient::foreign_toplevels`] fills in.
    pub fn track_foreign_toplevels(&mut self) {
        let _list: ExtForeignToplevelListV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding ext_foreign_toplevel_list_v1");
        self.roundtrip();
    }

    /// The `n`th toplevel the list has announced (for a capture source).
    pub(super) fn foreign_handle(&self, n: usize) -> ExtForeignToplevelHandleV1 {
        self.data.shell.foreign[n].0.clone()
    }

    /// The toplevels the list has announced, oldest first (closed ones stay, flagged).
    pub fn foreign_toplevels(&self) -> Vec<ForeignToplevel> {
        self.data
            .shell
            .foreign
            .iter()
            .map(|(_, info)| info.clone())
            .collect()
    }

    // ---- xdg-foreign ----

    /// Export `toplevel` and return its handle.
    pub fn export_toplevel(&mut self, toplevel: &Toplevel) -> String {
        let exporter: ZxdgExporterV2 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zxdg_exporter_v2");
        self.data.shell.exported_handle = None;
        let exported = exporter.export_toplevel(&toplevel.surface, &self.qh, ());
        self.data.shell.exported.push(exported);
        self.roundtrip();
        self.data
            .shell
            .exported_handle
            .take()
            .expect("no xdg-foreign handle")
    }

    /// Import `handle` and make `child` a child of the exported toplevel.
    pub fn import_parent_of(&mut self, handle: &str, child: &Toplevel) {
        let importer: ZxdgImporterV2 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zxdg_importer_v2");
        let imported = importer.import_toplevel(handle.to_owned(), &self.qh, ());
        imported.set_parent_of(&child.surface);
        self.data.shell.imported.push(imported);
        self.roundtrip();
    }

    // ---- xdg-dialog ----

    /// A `setup` for `create_toplevel_with` and friends that makes the new toplevel a
    /// modal dialog of `parent` before its first commit: a sheet.
    pub fn modal_of(
        &mut self,
        parent: &Toplevel,
    ) -> impl FnOnce(&wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel) + use<>
    {
        let manager: XdgWmDialogV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding xdg_wm_dialog_v1");
        let qh = self.qh.clone();
        let parent = parent.toplevel.clone();
        move |toplevel| {
            toplevel.set_parent(Some(&parent));
            manager.get_xdg_dialog(toplevel, &qh, ()).set_modal();
        }
    }

    /// Mark `toplevel` as a modal dialog.
    pub fn set_modal(&mut self, toplevel: &Toplevel) {
        let manager: XdgWmDialogV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding xdg_wm_dialog_v1");
        let dialog = manager.get_xdg_dialog(&toplevel.toplevel, &self.qh, ());
        dialog.set_modal();
        self.data.shell.dialogs.push(dialog);
        self.roundtrip();
    }

    // ---- xdg-toplevel-icon ----

    /// The icon sizes the compositor announced (binds the manager on first use).
    pub fn toplevel_icon_sizes(&mut self) -> Vec<i32> {
        let _manager = self.icon_manager();
        self.data.shell.icon_sizes.clone()
    }

    fn icon_manager(&mut self) -> XdgToplevelIconManagerV1 {
        let manager: XdgToplevelIconManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding xdg_toplevel_icon_manager_v1");
        self.roundtrip();
        manager
    }

    /// Give `toplevel` the icon called `name` and commit it.
    pub fn set_icon_name(&mut self, toplevel: &Toplevel, name: &str) {
        let manager = self.icon_manager();
        let icon: XdgToplevelIconV1 = manager.create_icon(&self.qh, ());
        icon.set_name(name.to_owned());
        manager.set_icon(&toplevel.toplevel, Some(&icon));
        icon.destroy();
        toplevel.surface.commit();
        self.roundtrip();
    }

    // ---- xdg-system-bell ----

    /// Ring the system bell for `surface` (or the whole client).
    pub fn ring_bell(&mut self, surface: Option<&WlSurface>) {
        let bell: XdgSystemBellV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding xdg_system_bell_v1");
        bell.ring(surface);
        self.roundtrip();
    }

    // ---- keyboard-shortcuts-inhibit ----

    /// Ask to keep the keyboard shortcuts while `surface` has focus.
    pub fn inhibit_shortcuts(&mut self, surface: &WlSurface) -> usize {
        let manager: ZwpKeyboardShortcutsInhibitManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zwp_keyboard_shortcuts_inhibit_manager_v1");
        let index = self.data.shell.shortcut_inhibitors.len();
        let inhibitor = manager.inhibit_shortcuts(surface, &self.seat, &self.qh, index);
        self.data.shell.shortcut_inhibitors.push((inhibitor, false));
        self.roundtrip();
        index
    }

    /// Whether the compositor has activated shortcut inhibitor `index`.
    pub fn shortcuts_inhibited(&self, index: usize) -> bool {
        self.data.shell.shortcut_inhibitors[index].1
    }

    /// Destroy shortcut inhibitor `index`.
    pub fn destroy_shortcut_inhibitor(&mut self, index: usize) {
        self.data.shell.shortcut_inhibitors[index].0.destroy();
        self.data.shell.shortcut_inhibitors[index].1 = false;
        self.roundtrip();
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let events = &mut data.shell.layers[*index];
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                events.configure_serial = Some(serial);
                events.size = (width, height);
            }
            zwlr_layer_surface_v1::Event::Closed => events.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtSessionLockV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let events = &mut data.shell.locks[*index];
        match event {
            ext_session_lock_v1::Event::Locked => events.locked = true,
            ext_session_lock_v1::Event::Finished => events.finished = true,
            _ => {}
        }
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        {
            data.shell.lock_surfaces[*index] = LockSurfaceEvents {
                configure_serial: Some(serial),
                size: (width, height),
            };
        }
    }
}

impl Dispatch<ExtIdleNotificationV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let events = &mut data.shell.idle[*index];
        match event {
            ext_idle_notification_v1::Event::Idled => events.idled += 1,
            ext_idle_notification_v1::Event::Resumed => events.resumed += 1,
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            data.shell
                .foreign
                .push((toplevel, ForeignToplevel::default()));
        }
    }

    event_created_child!(ClientData, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some((_, info)) = data
            .shell
            .foreign
            .iter_mut()
            .find(|(h, _)| h.id() == handle.id())
        else {
            return;
        };
        match event {
            ext_foreign_toplevel_handle_v1::Event::Closed => info.closed = true,
            ext_foreign_toplevel_handle_v1::Event::Title { title } => info.title = title,
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => info.app_id = app_id,
            ext_foreign_toplevel_handle_v1::Event::Identifier { identifier } => {
                info.identifier = identifier;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZxdgExportedV2, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            data.shell.exported_handle = Some(handle);
        }
    }
}

impl Dispatch<XdgToplevelIconManagerV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &XdgToplevelIconManagerV1,
        event: xdg_toplevel_icon_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel_icon_manager_v1::Event::IconSize { size } = event {
            data.shell.icon_sizes.push(size);
        }
    }
}

impl Dispatch<ZwpKeyboardShortcutsInhibitorV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpKeyboardShortcutsInhibitorV1,
        event: zwp_keyboard_shortcuts_inhibitor_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_keyboard_shortcuts_inhibitor_v1::Event::Active => {
                data.shell.shortcut_inhibitors[*index].1 = true;
            }
            zwp_keyboard_shortcuts_inhibitor_v1::Event::Inactive => {
                data.shell.shortcut_inhibitors[*index].1 = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, ()> for ClientData {
    fn event(
        _: &mut Self,
        _: &WlOutput,
        _: wayland_client::protocol::wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(ClientData: ZwlrLayerShellV1);
delegate_noop!(ClientData: ExtSessionLockManagerV1);
delegate_noop!(ClientData: ExtIdleNotifierV1);
delegate_noop!(ClientData: ZwpIdleInhibitManagerV1);
delegate_noop!(ClientData: ZwpIdleInhibitorV1);
delegate_noop!(ClientData: ZxdgExporterV2);
delegate_noop!(ClientData: ZxdgImporterV2);
delegate_noop!(ClientData: ZxdgImportedV2);
delegate_noop!(ClientData: XdgWmDialogV1);
delegate_noop!(ClientData: XdgDialogV1);
delegate_noop!(ClientData: XdgToplevelIconV1);
delegate_noop!(ClientData: XdgSystemBellV1);
delegate_noop!(ClientData: ZwpKeyboardShortcutsInhibitManagerV1);
