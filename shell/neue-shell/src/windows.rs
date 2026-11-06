//! The compositor's window list as the shell keeps it (task 3.2b): every window from
//! `neue_shell_v1`, and which one is active, to name the active app in the menu bar.

use std::collections::BTreeMap;

use neue_kit::desktop::{ShellEvent, Toplevel, ToplevelId};

/// What the bar shows when no window is active (the desktop was clicked, or nothing is
/// open).
pub const DESKTOP_NAME: &str = "Desktop";

/// The window list.
#[derive(Debug, Default)]
pub struct Windows {
    windows: BTreeMap<ToplevelId, Toplevel>,
    active: Option<ToplevelId>,
}

impl Windows {
    /// Applies `event`.
    pub fn apply(&mut self, event: ShellEvent) {
        match event {
            ShellEvent::Toplevel(id, toplevel) => {
                self.windows.insert(id, toplevel);
            }
            ShellEvent::Closed(id) => {
                self.windows.remove(&id);
                if self.active == Some(id) {
                    self.active = None;
                }
            }
            ShellEvent::Active(id) => self.active = id,
            ShellEvent::ActionFailed { name, message } => {
                tracing::warn!(%name, %message, "compositor action failed");
            }
            // The bars handle it (`Shell::shell`).
            ShellEvent::FocusMenuBar { .. } | ShellEvent::SystemKey { .. } => {}
        }
    }

    /// The active window, if any.
    pub fn active(&self) -> Option<&Toplevel> {
        self.windows.get(&self.active?)
    }

    /// The active window's id.
    pub fn active_id(&self) -> Option<ToplevelId> {
        self.active
    }

    /// The windows of the active window's app, the active one included, by id: those
    /// with the same app id, or the active window alone if it has none.
    pub fn app_windows(&self) -> Vec<(ToplevelId, &Toplevel)> {
        let Some(active) = self
            .active
            .and_then(|id| Some((id, self.windows.get(&id)?)))
        else {
            return Vec::new();
        };
        if active.1.app_id.is_empty() {
            return vec![active];
        }
        self.windows
            .iter()
            .filter(|(_, w)| w.app_id == active.1.app_id)
            .map(|(id, w)| (*id, w))
            .collect()
    }

    /// The menu bar's bold name: the active window's app.
    pub fn active_app_name(&self) -> String {
        self.active()
            .map_or_else(|| DESKTOP_NAME.to_owned(), app_name)
    }
}

/// A window's app name, until desktop entries give the real one (task 3.3): the last part
/// of a reverse-DNS app id (`org.neueos.Gallery` → `Gallery`), or the whole id
/// capitalised (`foot` → `Foot`); the title if there is no app id.
pub fn app_name(toplevel: &Toplevel) -> String {
    let id = toplevel.app_id.trim();
    if id.is_empty() {
        return if toplevel.title.trim().is_empty() {
            DESKTOP_NAME.to_owned()
        } else {
            toplevel.title.clone()
        };
    }
    let last = id.rsplit('.').next().unwrap_or(id);
    let mut chars = last.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(app_id: &str, title: &str) -> Toplevel {
        Toplevel {
            app_id: app_id.to_owned(),
            title: title.to_owned(),
            ..Toplevel::default()
        }
    }

    #[test]
    fn names_come_from_the_app_id_or_the_title() {
        assert_eq!(app_name(&window("org.neueos.Gallery", "x")), "Gallery");
        assert_eq!(app_name(&window("foot", "x")), "Foot");
        assert_eq!(app_name(&window("", "Untitled")), "Untitled");
        assert_eq!(app_name(&window("", "")), DESKTOP_NAME);
    }

    #[test]
    fn an_apps_windows_share_its_app_id() {
        let ids = [1, 2, 3, 4].map(ToplevelId::from_raw);
        let mut list = Windows::default();
        assert!(list.app_windows().is_empty());
        list.apply(ShellEvent::Toplevel(ids[0], window("foot", "a")));
        list.apply(ShellEvent::Toplevel(
            ids[1],
            window("org.neueos.Gallery", "b"),
        ));
        list.apply(ShellEvent::Toplevel(ids[2], window("foot", "c")));
        list.apply(ShellEvent::Toplevel(ids[3], window("", "d")));
        list.apply(ShellEvent::Active(Some(ids[2])));
        let found: Vec<_> = list.app_windows().iter().map(|(id, _)| *id).collect();
        assert_eq!(found, [ids[0], ids[2]]);
        // No app id: the window alone.
        list.apply(ShellEvent::Active(Some(ids[3])));
        assert_eq!(list.app_windows().len(), 1);
    }

    #[test]
    fn the_name_follows_the_active_window() {
        let (a, b) = (ToplevelId::from_raw(1), ToplevelId::from_raw(2));
        let mut list = Windows::default();
        assert_eq!(list.active_app_name(), DESKTOP_NAME);
        list.apply(ShellEvent::Toplevel(a, window("org.neueos.Gallery", "")));
        list.apply(ShellEvent::Active(Some(a)));
        assert_eq!(list.active_app_name(), "Gallery");
        list.apply(ShellEvent::Toplevel(b, window("foot", "")));
        list.apply(ShellEvent::Active(Some(b)));
        assert_eq!(list.active_app_name(), "Foot");
        list.apply(ShellEvent::Closed(b));
        assert_eq!(list.active_app_name(), DESKTOP_NAME);
        list.apply(ShellEvent::Active(None));
        assert_eq!(list.active_app_name(), DESKTOP_NAME);
    }
}
