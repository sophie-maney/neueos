//! `neue_window_v1` on the test client (task 2.35, ADR 0064).

use neue_protocols::neue_window::client::{
    neue_window_manager_v1::NeueWindowManagerV1, neue_window_v1::NeueWindowV1,
};
use wayland_client::delegate_noop;
use wayland_client::protocol::wl_surface::WlSurface;

use super::{ClientData, TestClient};

#[derive(Default)]
pub(super) struct WindowData {
    manager: Option<NeueWindowManagerV1>,
}

impl TestClient {
    /// `neue_window_manager_v1.get_window` for `surface` (binding the manager once).
    pub fn neue_window(&mut self, surface: &WlSurface) -> NeueWindowV1 {
        let manager = match &self.data.window.manager {
            Some(manager) => manager.clone(),
            None => {
                let manager: NeueWindowManagerV1 = self
                    ._globals
                    .bind(&self.qh, 1..=1, ())
                    .expect("binding neue_window_manager_v1");
                self.data.window.manager = Some(manager.clone());
                manager
            }
        };
        manager.get_window(surface, &self.qh, ())
    }
}

delegate_noop!(ClientData: ignore NeueWindowManagerV1);
delegate_noop!(ClientData: ignore NeueWindowV1);
