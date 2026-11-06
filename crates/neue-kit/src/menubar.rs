//! Window menu bars for the global menu (task 2.18, ADR 0005, ADR 0047).
//!
//! On Linux each window's menu bar is exported over the session bus as
//! `com.canonical.dbusmenu` (neue-dbus) at `/org/neueos/MenuBar/<window>`, and the driver
//! tells the compositor where (`org_kde_kwin_appmenu`). The menu bar draws it; a chosen
//! item comes back on zbus's thread as `(window, item id)`, and the UI thread turns it into
//! the item's message when it drains its queue. Item ids are the menu model's depth-first
//! ids plus one (0 is DBusMenu's root).
//!
//! Elsewhere (macOS hosts) menu bars are kept but not shown yet.

use std::collections::HashMap;
use std::sync::mpsc;

use neue_platform::WindowId;

use crate::app::Host;
use crate::menu::{ItemHandler, Menu};
use crate::task::Wake;

/// Every window's menu bar.
pub(crate) struct MenuBars<M> {
    bars: HashMap<WindowId, Bar<M>>,
    /// Chosen items, from zbus's thread.
    clicks_tx: mpsc::Sender<(WindowId, usize)>,
    clicks: mpsc::Receiver<(WindowId, usize)>,
    wake: Wake,
    #[cfg(target_os = "linux")]
    bus: linux::Bus,
}

struct Bar<M> {
    handlers: Vec<Option<ItemHandler<M>>>,
    #[cfg(target_os = "linux")]
    export: Option<neue_dbus::MenuExport>,
}

impl<M> std::fmt::Debug for MenuBars<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBars")
            .field("windows", &self.bars.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl<M> MenuBars<M> {
    pub(crate) fn new(wake: Wake) -> Self {
        let (clicks_tx, clicks) = mpsc::channel();
        Self {
            bars: HashMap::new(),
            clicks_tx,
            clicks,
            wake,
            #[cfg(target_os = "linux")]
            bus: linux::Bus::default(),
        }
    }

    /// Gives `window` the menu bar `menu`, or takes it away if `menu` is empty.
    pub(crate) fn set(&mut self, host: &mut dyn Host, window: WindowId, menu: Menu<M>) {
        if menu.is_empty() {
            if self.bars.remove(&window).is_some() {
                host.set_menu_address(window, None);
            }
            return;
        }
        let (spec, handlers) = menu.into_parts();
        #[cfg(target_os = "linux")]
        {
            let previous = self.bars.remove(&window).and_then(|bar| bar.export);
            let tx = self.clicks_tx.clone();
            let wake = self.wake.clone();
            let on_click = move |id: usize| {
                if tx.send((window, id)).is_ok() {
                    wake();
                }
            };
            let export = self.bus.export(window, &spec, previous, on_click);
            if let Some(address) = export.as_ref().and_then(linux::address) {
                host.set_menu_address(window, Some(address));
            }
            self.bars.insert(window, Bar { handlers, export });
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (spec, host, &self.clicks_tx, &self.wake);
            self.bars.insert(window, Bar { handlers });
        }
    }

    /// Forgets a closed window's menu bar.
    pub(crate) fn remove(&mut self, window: WindowId) {
        self.bars.remove(&window);
    }

    /// The messages of the items chosen since the last call, in order.
    pub(crate) fn take_messages(&mut self) -> Vec<M> {
        let mut messages = Vec::new();
        while let Ok((window, id)) = self.clicks.try_recv() {
            let handler = self
                .bars
                .get_mut(&window)
                .and_then(|bar| bar.handlers.get_mut(id))
                .and_then(Option::as_mut);
            match handler {
                Some(make) => messages.push(make()),
                None => tracing::debug!(%window, id, "menu-bar item without a message"),
            }
        }
        messages
    }

    /// Exports on `conn` instead of connecting to the session bus.
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn use_connection(&mut self, conn: neue_dbus::zbus::blocking::Connection) {
        self.bus = linux::Bus::with(conn);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use neue_dbus::zbus::blocking::Connection;
    use neue_dbus::{MENU_ROOT, MenuExport, MenuNode, Toggle, ToggleState};
    use neue_platform::{MenuAddress, WindowId};

    use crate::CheckState;
    use crate::menu::{MenuSpec, MenuToggle, Shortcut, SpecKind};

    /// The session-bus connection, made on the first export.
    #[derive(Default)]
    pub(super) enum Bus {
        #[default]
        Unconnected,
        Connected(Connection),
        /// Connecting failed; menu bars stay in the windows' model only.
        Failed,
    }

    impl Bus {
        #[cfg(test)]
        pub(super) fn with(conn: Connection) -> Self {
            Self::Connected(conn)
        }

        fn connection(&mut self) -> Option<&Connection> {
            if let Self::Unconnected = self {
                *self = match neue_dbus::connect_session() {
                    Ok(conn) => Self::Connected(conn),
                    Err(err) => {
                        tracing::warn!(%err, "no session bus: menu bars aren't exported");
                        Self::Failed
                    }
                };
            }
            match self {
                Self::Connected(conn) => Some(conn),
                _ => None,
            }
        }

        /// Exports `spec` as `window`'s menu bar, updating `previous` if there is one.
        pub(super) fn export(
            &mut self,
            window: WindowId,
            spec: &MenuSpec,
            previous: Option<MenuExport>,
            on_click: impl Fn(usize) + Send + Sync + 'static,
        ) -> Option<MenuExport> {
            let items = nodes(spec);
            if let Some(export) = previous {
                if let Err(err) = export.set_items(items) {
                    tracing::warn!(%err, %window, "can't update the menu bar");
                }
                // The callback is the same: it only names the window.
                return Some(export);
            }
            let conn = self.connection()?;
            let path = format!("{MENU_ROOT}/{}", window.to_raw());
            let on_click = move |id: i32| {
                if let Some(id) = usize::try_from(id).ok().and_then(|id| id.checked_sub(1)) {
                    on_click(id);
                }
            };
            match MenuExport::new(conn, &path, items, on_click) {
                Ok(export) => Some(export),
                Err(err) => {
                    tracing::warn!(%err, %window, "can't export the menu bar");
                    None
                }
            }
        }
    }

    /// Where `export` can be reached; `None` without a bus name (peer-to-peer).
    pub(super) fn address(export: &MenuExport) -> Option<MenuAddress> {
        Some(MenuAddress {
            service: export.service()?,
            path: export.path().to_owned(),
        })
    }

    /// The menu as DBusMenu items, with ids shifted by one past the root's.
    pub(super) fn nodes(spec: &MenuSpec) -> Vec<MenuNode> {
        spec.0
            .iter()
            .filter_map(|item| {
                let id = i32::try_from(item.id + 1).ok()?;
                let mut node = match &item.kind {
                    SpecKind::Separator => MenuNode::separator(id),
                    SpecKind::Action => MenuNode::item(id, item.label.clone()),
                    SpecKind::Submenu(sub) => MenuNode::submenu(id, item.label.clone(), nodes(sub)),
                    // DBusMenu has no slider.
                    SpecKind::Slider(_) => return None,
                };
                node.enabled = item.enabled;
                node.toggle = match item.toggle {
                    MenuToggle::None => Toggle::None,
                    MenuToggle::Check(state) => Toggle::Check(match state {
                        CheckState::Off => ToggleState::Off,
                        CheckState::On => ToggleState::On,
                        CheckState::Mixed => ToggleState::Mixed,
                    }),
                    MenuToggle::Radio(on) => Toggle::Radio(if on {
                        ToggleState::On
                    } else {
                        ToggleState::Off
                    }),
                };
                node.shortcut = item
                    .shortcut
                    .as_ref()
                    .map(Shortcut::key_names)
                    .unwrap_or_default();
                Some(node)
            })
            .collect()
    }
}
