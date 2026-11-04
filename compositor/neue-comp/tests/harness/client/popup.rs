//! `xdg_popup` on the test client: menus and their teardown (bug B5, task 1.21).

use wayland_client::delegate_noop;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_surface::WlSurface};
use wayland_protocols::xdg::shell::client::{
    xdg_popup::XdgPopup, xdg_positioner::XdgPositioner, xdg_surface::XdgSurface,
};

use super::{ClientData, TestClient, Toplevel, ToplevelEvents};

/// A mapped popup. Its configure serials go into [`ClientData::toplevels`] like a
/// toplevel's.
pub struct Popup {
    pub surface: WlSurface,
    pub xdg_surface: XdgSurface,
    pub popup: XdgPopup,
    buffer: WlBuffer,
}

impl TestClient {
    /// Open a `size` popup anchored to the top-left of `parent`, ack its configure and
    /// map it with a buffer.
    pub fn create_popup(&mut self, parent: &Toplevel, size: (i32, i32)) -> Popup {
        let index = self.data.toplevels.len();
        self.data.toplevels.push(ToplevelEvents::default());

        let surface = self.compositor.create_surface(&self.qh, ());
        let xdg_surface = self.wm_base.get_xdg_surface(&surface, &self.qh, index);
        let positioner = self.wm_base.create_positioner(&self.qh, ());
        positioner.set_size(size.0, size.1);
        positioner.set_anchor_rect(0, 0, 1, 1);
        let popup = xdg_surface.get_popup(Some(&parent.xdg_surface), &positioner, &self.qh, ());
        positioner.destroy();
        surface.commit();
        self.roundtrip();

        let serial = self.data.toplevels[index]
            .configure_serial
            .expect("no initial popup configure after the first commit");
        xdg_surface.ack_configure(serial);
        let buffer = self.shm_buffer(size);
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        self.roundtrip();
        Popup {
            surface,
            xdg_surface,
            popup,
            buffer,
        }
    }

    /// Close a menu the way GTK does: destroy the `xdg_popup` and `xdg_surface`, then
    /// detach the buffer and commit the `wl_surface`, which the client keeps for reuse.
    pub fn close_popup_keep_surface(&mut self, popup: Popup) -> WlSurface {
        popup.popup.destroy();
        popup.xdg_surface.destroy();
        popup.surface.attach(None, 0, 0);
        popup.surface.commit();
        popup.buffer.destroy();
        self.roundtrip();
        popup.surface
    }
}

delegate_noop!(ClientData: XdgPositioner);
delegate_noop!(ClientData: ignore XdgPopup);
