//! `org_kde_kwin_appmenu` and `gtk_shell1` on the test client (task 1.17, ADR 0025).

use neue_protocols::gtk_shell::client::{
    gtk_shell1::{self, GtkShell1},
    gtk_surface1::GtkSurface1,
};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols_plasma::appmenu::client::{
    org_kde_kwin_appmenu::OrgKdeKwinAppmenu, org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager,
};

use super::{ClientData, TestClient};

#[derive(Default)]
pub(super) struct AppmenuData {
    manager: Option<OrgKdeKwinAppmenuManager>,
    gtk_shell: Option<GtkShell1>,
    /// The last `gtk_shell1.capabilities`.
    capabilities: Option<u32>,
}

impl TestClient {
    /// `org_kde_kwin_appmenu_manager.create` for `surface`. Set the address on the result.
    pub fn kde_appmenu(&mut self, surface: &WlSurface) -> OrgKdeKwinAppmenu {
        let manager = match &self.data.appmenu.manager {
            Some(manager) => manager.clone(),
            None => {
                let manager: OrgKdeKwinAppmenuManager = self
                    ._globals
                    .bind(&self.qh, 1..=2, ())
                    .expect("binding org_kde_kwin_appmenu_manager");
                self.data.appmenu.manager = Some(manager.clone());
                manager
            }
        };
        manager.create(surface, &self.qh, ())
    }

    /// Bind `gtk_shell1` (once) and wait for its capabilities.
    pub fn gtk_shell(&mut self) -> GtkShell1 {
        if let Some(shell) = &self.data.appmenu.gtk_shell {
            return shell.clone();
        }
        let shell: GtkShell1 = self
            ._globals
            .bind(&self.qh, 1..=5, ())
            .expect("binding gtk_shell1");
        self.data.appmenu.gtk_shell = Some(shell.clone());
        self.roundtrip();
        shell
    }

    /// `gtk_shell1.get_gtk_surface` for `surface`.
    pub fn gtk_surface(&mut self, surface: &WlSurface) -> GtkSurface1 {
        let shell = self.gtk_shell();
        shell.get_gtk_surface(surface, &self.qh, ())
    }

    /// The version we bound `gtk_shell1` at.
    pub fn gtk_shell_version(&mut self) -> u32 {
        self.gtk_shell().version()
    }

    /// The capabilities the compositor announced on `gtk_shell1`, if it has been bound.
    pub fn gtk_shell_capabilities(&self) -> Option<u32> {
        self.data.appmenu.capabilities
    }
}

impl Dispatch<GtkShell1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &GtkShell1,
        event: gtk_shell1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let gtk_shell1::Event::Capabilities { capabilities } = event {
            data.appmenu.capabilities = Some(capabilities);
        }
    }
}

delegate_noop!(ClientData: ignore GtkSurface1);
delegate_noop!(ClientData: ignore OrgKdeKwinAppmenuManager);
delegate_noop!(ClientData: ignore OrgKdeKwinAppmenu);
