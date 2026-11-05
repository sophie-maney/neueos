//! `xdg-decoration` and KDE server-decoration on the test client.

use std::collections::HashMap;

use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::xdg::decoration::zv1::client::{
    zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
    zxdg_toplevel_decoration_v1::{self, ZxdgToplevelDecorationV1},
};
use wayland_protocols_misc::server_decoration::client::{
    org_kde_kwin_server_decoration::{self, OrgKdeKwinServerDecoration},
    org_kde_kwin_server_decoration_manager::{self, OrgKdeKwinServerDecorationManager},
};

pub use org_kde_kwin_server_decoration::Mode as KdeMode;
pub use org_kde_kwin_server_decoration_manager::Mode as KdeDefaultMode;
pub use zxdg_toplevel_decoration_v1::Mode as XdgMode;

use super::{ClientData, TestClient, Toplevel};

/// Decoration objects and what the compositor said about them, keyed by toplevel index.
#[derive(Default)]
pub(super) struct Decorations {
    xdg_manager: Option<ZxdgDecorationManagerV1>,
    kde_manager: Option<OrgKdeKwinServerDecorationManager>,
    kde_default: Option<KdeDefaultMode>,
    xdg: HashMap<usize, (ZxdgToplevelDecorationV1, Option<XdgMode>)>,
    kde: HashMap<usize, (OrgKdeKwinServerDecoration, Option<KdeMode>)>,
}

impl TestClient {
    fn xdg_decoration_manager(&mut self) -> ZxdgDecorationManagerV1 {
        if let Some(manager) = &self.data.decorations.xdg_manager {
            return manager.clone();
        }
        let manager: ZxdgDecorationManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zxdg_decoration_manager_v1");
        self.data.decorations.xdg_manager = Some(manager.clone());
        manager
    }

    /// Create a toplevel that gets an `xdg-decoration` object before its first commit and
    /// asks for `mode` (`None`: no preference), then map it like `create_toplevel`.
    pub fn create_decorated_toplevel(
        &mut self,
        title: &str,
        app_id: &str,
        size: (i32, i32),
        mode: Option<XdgMode>,
    ) -> Toplevel {
        self.create_decorated_toplevel_with(title, app_id, size, mode, |_| {})
    }

    /// [`TestClient::create_decorated_toplevel`], calling `setup` on the `xdg_toplevel`
    /// before the first commit (e.g. to set size limits).
    pub fn create_decorated_toplevel_with(
        &mut self,
        title: &str,
        app_id: &str,
        size: (i32, i32),
        mode: Option<XdgMode>,
        setup: impl FnOnce(&wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel),
    ) -> Toplevel {
        let manager = self.xdg_decoration_manager();
        let qh = self.qh.clone();
        let index = self.data.toplevels.len();
        let mut decoration = None;
        let toplevel = self.create_toplevel_with(title, app_id, size, |t| {
            let d = manager.get_toplevel_decoration(t, &qh, index);
            if let Some(mode) = mode {
                d.set_mode(mode);
            }
            decoration = Some(d);
            setup(t);
        });
        let decoration = decoration.expect("setup ran");
        let configured = self.data.decorations.xdg.get(&index).and_then(|(_, m)| *m);
        self.data
            .decorations
            .xdg
            .insert(index, (decoration, configured));
        toplevel
    }

    /// The decoration mode last configured for `toplevel` through `xdg-decoration`.
    pub fn xdg_decoration_mode(&self, toplevel: &Toplevel) -> Option<XdgMode> {
        self.data
            .decorations
            .xdg
            .get(&toplevel.index)
            .and_then(|(_, m)| *m)
    }

    /// Ask for `mode` on `toplevel`'s existing decoration object.
    pub fn set_xdg_decoration_mode(&mut self, toplevel: &Toplevel, mode: XdgMode) {
        if let Some((d, _)) = self.data.decorations.xdg.get(&toplevel.index) {
            d.set_mode(mode);
        }
        self.roundtrip();
    }

    /// The default mode the KDE manager announced when bound (binds it on first use).
    pub fn kde_default_mode(&mut self) -> Option<KdeDefaultMode> {
        if self.data.decorations.kde_manager.is_none() {
            let manager: OrgKdeKwinServerDecorationManager = self
                ._globals
                .bind(&self.qh, 1..=1, ())
                .expect("binding org_kde_kwin_server_decoration_manager");
            self.data.decorations.kde_manager = Some(manager);
            self.roundtrip();
        }
        self.data.decorations.kde_default
    }

    /// Create a KDE decoration object for `toplevel`'s surface.
    pub fn kde_decorate(&mut self, toplevel: &Toplevel) {
        self.kde_default_mode();
        let manager = self
            .data
            .decorations
            .kde_manager
            .clone()
            .expect("bound above");
        let d = manager.create(&toplevel.surface, &self.qh, toplevel.index);
        self.data.decorations.kde.insert(toplevel.index, (d, None));
        self.roundtrip();
    }

    /// The mode the compositor last sent on `toplevel`'s KDE decoration.
    pub fn kde_mode(&self, toplevel: &Toplevel) -> Option<KdeMode> {
        self.data
            .decorations
            .kde
            .get(&toplevel.index)
            .and_then(|(_, m)| *m)
    }

    /// Ask for `mode` on `toplevel`'s KDE decoration.
    pub fn kde_request_mode(&mut self, toplevel: &Toplevel, mode: KdeMode) {
        if let Some((d, _)) = self.data.decorations.kde.get(&toplevel.index) {
            d.request_mode(mode);
        }
        self.roundtrip();
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        decoration: &ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure {
            mode: WEnum::Value(mode),
        } = event
        {
            data.decorations
                .xdg
                .entry(*index)
                .or_insert_with(|| (decoration.clone(), None))
                .1 = Some(mode);
        }
    }
}

impl Dispatch<OrgKdeKwinServerDecorationManager, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &OrgKdeKwinServerDecorationManager,
        event: org_kde_kwin_server_decoration_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let org_kde_kwin_server_decoration_manager::Event::DefaultMode {
            mode: WEnum::Value(mode),
        } = event
        {
            data.decorations.kde_default = Some(mode);
        }
    }
}

impl Dispatch<OrgKdeKwinServerDecoration, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &OrgKdeKwinServerDecoration,
        event: org_kde_kwin_server_decoration::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let org_kde_kwin_server_decoration::Event::Mode {
            mode: WEnum::Value(mode),
        } = event
            && let Some(entry) = data.decorations.kde.get_mut(index)
        {
            entry.1 = Some(mode);
        }
    }
}

wayland_client::delegate_noop!(ClientData: ignore ZxdgDecorationManagerV1);
