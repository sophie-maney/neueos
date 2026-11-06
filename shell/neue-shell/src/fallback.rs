//! The fallback menu (task 3.9, ADR 0005 §4, ADR 0082) for windows whose app exports no
//! menus: App, File, Edit and Window (Help is the shell's, see [`crate::help`]). The menus
//! are described once, as [`Section`]s, and become the bar's titles, the entries menu search
//! finds and the requests the compositor gets.

use neue_kit::desktop::{ChordModifiers, MenuTitle, ShellRequest, ToplevelId};
use neue_kit::{CheckState, Menu, MenuItem, Shortcut};

use crate::search::Entry;

/// What the Edit menu sends to the app as a key chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Undo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl Edit {
    /// The key (a letter, so it also names the XKB keysym).
    const fn key(self) -> char {
        match self {
            Self::Undo => 'z',
            Self::Cut => 'x',
            Self::Copy => 'c',
            Self::Paste => 'v',
            Self::SelectAll => 'a',
        }
    }

    /// The chord that does it in the app: Ctrl and the key; in a terminal only Copy and
    /// Paste exist, as Ctrl+Shift+C and Ctrl+Shift+V (Ctrl+C there interrupts).
    pub fn chord(self, terminal: bool) -> Option<(ChordModifiers, u32)> {
        let shift = match (terminal, self) {
            (false, _) => false,
            (true, Self::Copy | Self::Paste) => true,
            (true, _) => return None,
        };
        let modifiers = ChordModifiers {
            ctrl: true,
            shift,
            ..ChordModifiers::default()
        };
        // Latin-1 keysyms are the character codes.
        Some((modifiers, u32::from(self.key())))
    }
}

/// What an item of the fallback menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Not possible for an app we know nothing about: shown disabled.
    About,
    /// Minimises the app's windows.
    Hide,
    /// Asks every window of the app to close.
    Quit,
    /// Runs the entry's `new-window` action (the shell does; see `Context::can_new_window`).
    NewWindow,
    CloseWindow,
    Edit(Edit),
    Minimize,
    Zoom,
    /// Raises the app's windows, the active one on top.
    BringAllToFront,
    /// Raises one of the app's windows.
    Show(ToplevelId),
}

/// One of the app's windows, for the Window menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowEntry {
    pub id: ToplevelId,
    pub title: String,
    pub active: bool,
    pub minimized: bool,
}

/// What the menu depends on: the active window's app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub app_name: String,
    /// The app's windows (the active one is marked).
    pub windows: Vec<WindowEntry>,
    /// The app's desktop entry has a `new-window` action.
    pub can_new_window: bool,
    /// Super reaches the app as Control (`command_key`, ADR 0081).
    pub super_as_ctrl: bool,
    /// The app is a terminal emulator: its copy and paste are Ctrl+Shift+C and V.
    pub terminal: bool,
}

/// An item or a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Item {
        label: String,
        command: Command,
        shortcut: Option<Shortcut>,
        enabled: bool,
        /// A window listed in the Window menu; `true` marks the active one.
        checked: Option<bool>,
    },
    Separator,
}

/// A menu: its title and rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub rows: Vec<Row>,
}

fn item(label: impl Into<String>, command: Command, enabled: bool) -> Row {
    Row::Item {
        label: label.into(),
        command,
        shortcut: None,
        enabled,
        checked: None,
    }
}

impl Context {
    /// The shortcut the user types for `edit`, if it has one here: ⌘ where Super is
    /// sent as Control, else the chord itself.
    fn shortcut(&self, edit: Edit) -> Option<Shortcut> {
        let (modifiers, _) = edit.chord(self.terminal)?;
        let key = edit.key().to_ascii_uppercase().to_string();
        Some(if self.super_as_ctrl && !modifiers.shift {
            Shortcut::command(key)
        } else if modifiers.shift {
            Shortcut::new(key).ctrl().shift()
        } else {
            Shortcut::new(key).ctrl()
        })
    }

    fn edit(&self, label: &str, edit: Edit) -> Row {
        Row::Item {
            label: label.to_owned(),
            command: Command::Edit(edit),
            shortcut: self.shortcut(edit),
            enabled: edit.chord(self.terminal).is_some(),
            checked: None,
        }
    }

    /// The menu under the app's name.
    pub fn app_section(&self) -> Section {
        let name = &self.app_name;
        Section {
            title: name.clone(),
            rows: vec![
                item(format!("About {name}"), Command::About, false),
                Row::Separator,
                item(format!("Hide {name}"), Command::Hide, true),
                Row::Separator,
                item(format!("Quit {name}"), Command::Quit, true),
            ],
        }
    }

    /// File, Edit and Window.
    pub fn sections(&self) -> Vec<Section> {
        let mut window = vec![
            item("Minimise", Command::Minimize, true),
            item("Zoom", Command::Zoom, true),
            Row::Separator,
            item("Bring All to Front", Command::BringAllToFront, true),
        ];
        if !self.windows.is_empty() {
            window.push(Row::Separator);
        }
        window.extend(self.windows.iter().map(|w| Row::Item {
            label: if w.title.trim().is_empty() {
                self.app_name.clone()
            } else {
                w.title.clone()
            },
            command: Command::Show(w.id),
            shortcut: None,
            enabled: true,
            checked: Some(w.active),
        }));
        vec![
            Section {
                title: "File".to_owned(),
                rows: vec![
                    item("New Window", Command::NewWindow, self.can_new_window),
                    Row::Separator,
                    item("Close Window", Command::CloseWindow, true),
                ],
            },
            Section {
                title: "Edit".to_owned(),
                rows: vec![
                    self.edit("Undo", Edit::Undo),
                    Row::Separator,
                    self.edit("Cut", Edit::Cut),
                    self.edit("Copy", Edit::Copy),
                    self.edit("Paste", Edit::Paste),
                    self.edit("Select All", Edit::SelectAll),
                ],
            },
            Section {
                title: "Window".to_owned(),
                rows: window,
            },
        ]
    }

    /// The items menu search finds: every enabled one in the app menu and the others.
    pub fn entries(&self) -> Vec<Entry<Command>> {
        std::iter::once(self.app_section())
            .chain(self.sections())
            .flat_map(|section| {
                section.rows.into_iter().filter_map(move |row| match row {
                    Row::Item {
                        label,
                        command,
                        shortcut,
                        enabled: true,
                        ..
                    } => Some(Entry {
                        path: vec![section.title.clone(), label],
                        shortcut,
                        target: command,
                    }),
                    _ => None,
                })
            })
            .collect()
    }

    /// What `command` asks of the compositor. `NewWindow` and `About` ask nothing of it.
    pub fn requests(&self, command: Command) -> Vec<ShellRequest> {
        let active = self.windows.iter().find(|w| w.active).map(|w| w.id);
        match command {
            Command::About | Command::NewWindow => Vec::new(),
            Command::Hide => self
                .windows
                .iter()
                .filter(|w| !w.minimized)
                .map(|w| ShellRequest::Minimize(w.id))
                .collect(),
            Command::Quit => self
                .windows
                .iter()
                .map(|w| ShellRequest::Close(w.id))
                .collect(),
            Command::CloseWindow => active.map(ShellRequest::Close).into_iter().collect(),
            Command::Minimize => active.map(ShellRequest::Minimize).into_iter().collect(),
            Command::Zoom => vec![ShellRequest::RunAction {
                name: "zoom-window".to_owned(),
                window: None,
            }],
            Command::Edit(edit) => edit
                .chord(self.terminal)
                .map(|(modifiers, keysym)| ShellRequest::SendShortcut { modifiers, keysym })
                .into_iter()
                .collect(),
            // The active window last, so it stays on top.
            Command::BringAllToFront => self
                .windows
                .iter()
                .filter(|w| !w.active && !w.minimized)
                .chain(self.windows.iter().filter(|w| w.active))
                .map(|w| ShellRequest::Activate(w.id))
                .collect(),
            Command::Show(id) => vec![ShellRequest::Activate(id)],
        }
    }
}

/// A kit menu for `rows`, each item chosen sending `message(command)`.
pub fn menu<M: 'static>(
    rows: &[Row],
    message: &(impl Fn(Command) -> M + Clone + 'static),
) -> Menu<M> {
    rows.iter().fold(Menu::new(), |built, row| match row {
        Row::Separator => built.separator(),
        Row::Item {
            label,
            command,
            shortcut,
            enabled,
            checked,
        } => {
            let (message, command) = (message.clone(), *command);
            let mut item = MenuItem::new(label.clone())
                .on_choose_with(move || message(command))
                .disabled(!enabled);
            if let Some(shortcut) = shortcut {
                item = item.shortcut(shortcut.clone());
            }
            if let Some(active) = checked {
                item = item.state(if *active {
                    CheckState::On
                } else {
                    CheckState::Off
                });
            }
            built.with(item)
        }
    })
}

/// The bar's titles for `sections`.
pub fn titles<M: 'static>(
    sections: &[Section],
    message: &(impl Fn(Command) -> M + Clone + 'static),
) -> Vec<MenuTitle<M>> {
    sections
        .iter()
        .map(|s| MenuTitle::new(s.title.clone(), menu(&s.rows, message)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> [ToplevelId; 3] {
        [1, 2, 3].map(ToplevelId::from_raw)
    }

    fn context(terminal: bool, super_as_ctrl: bool) -> Context {
        let [a, b, c] = ids();
        let window = |id, title: &str, active, minimized| WindowEntry {
            id,
            title: title.to_owned(),
            active,
            minimized,
        };
        Context {
            app_name: "Firefox".to_owned(),
            windows: vec![
                window(a, "One", false, false),
                window(b, "Two", true, false),
                window(c, "Three", false, true),
            ],
            can_new_window: true,
            super_as_ctrl,
            terminal,
        }
    }

    fn find(context: &Context, label: &str) -> Row {
        std::iter::once(context.app_section())
            .chain(context.sections())
            .flat_map(|s| s.rows)
            .find(|r| matches!(r, Row::Item { label: l, .. } if l == label))
            .unwrap_or_else(|| panic!("no {label}"))
    }

    #[test]
    fn the_menus_are_app_file_edit_and_window() {
        let c = context(false, true);
        assert_eq!(c.app_section().title, "Firefox");
        let titles: Vec<_> = c.sections().into_iter().map(|s| s.title).collect();
        assert_eq!(titles, ["File", "Edit", "Window"]);
    }

    #[test]
    fn edit_sends_control_chords_to_apps() {
        let c = context(false, true);
        for (edit, key) in [
            (Edit::Undo, 'z'),
            (Edit::Cut, 'x'),
            (Edit::Copy, 'c'),
            (Edit::Paste, 'v'),
            (Edit::SelectAll, 'a'),
        ] {
            let ShellRequest::SendShortcut { modifiers, keysym } =
                c.requests(Command::Edit(edit))[0]
            else {
                panic!("not a chord");
            };
            assert!(modifiers.ctrl && !modifiers.shift && !modifiers.alt && !modifiers.logo);
            assert_eq!(keysym, u32::from(key));
        }
    }

    #[test]
    fn terminals_copy_and_paste_with_shift_and_have_no_other_edits() {
        let c = context(true, false);
        for (edit, key) in [(Edit::Copy, 'c'), (Edit::Paste, 'v')] {
            let ShellRequest::SendShortcut { modifiers, keysym } =
                c.requests(Command::Edit(edit))[0]
            else {
                panic!("not a chord");
            };
            assert!(modifiers.ctrl && modifiers.shift);
            assert_eq!(keysym, u32::from(key));
        }
        for edit in [Edit::Undo, Edit::Cut, Edit::SelectAll] {
            assert!(c.requests(Command::Edit(edit)).is_empty());
        }
        assert!(matches!(find(&c, "Cut"), Row::Item { enabled: false, .. }));
        assert!(matches!(find(&c, "Copy"), Row::Item { enabled: true, .. }));
    }

    #[test]
    fn shortcuts_show_what_the_user_types() {
        let shortcut = |c: &Context| match find(c, "Copy") {
            Row::Item { shortcut, .. } => shortcut,
            Row::Separator => None,
        };
        assert_eq!(
            shortcut(&context(false, true)),
            Some(Shortcut::command("C"))
        );
        assert_eq!(
            shortcut(&context(false, false)),
            Some(Shortcut::new("C").ctrl())
        );
        assert_eq!(
            shortcut(&context(true, false)),
            Some(Shortcut::new("C").ctrl().shift())
        );
    }

    #[test]
    fn new_window_needs_the_entrys_action() {
        let mut c = context(false, true);
        assert!(matches!(
            find(&c, "New Window"),
            Row::Item { enabled: true, .. }
        ));
        c.can_new_window = false;
        assert!(matches!(
            find(&c, "New Window"),
            Row::Item { enabled: false, .. }
        ));
        assert!(c.entries().iter().all(|e| e.target != Command::NewWindow));
    }

    #[test]
    fn window_items_ask_the_compositor() {
        let c = context(false, true);
        let [a, b, c3] = ids();
        assert_eq!(c.requests(Command::Minimize), [ShellRequest::Minimize(b)]);
        assert_eq!(c.requests(Command::CloseWindow), [ShellRequest::Close(b)]);
        assert_eq!(
            c.requests(Command::Zoom),
            [ShellRequest::RunAction {
                name: "zoom-window".to_owned(),
                window: None
            }]
        );
        // Windows already hidden stay as they are.
        assert_eq!(
            c.requests(Command::Hide),
            [ShellRequest::Minimize(a), ShellRequest::Minimize(b)]
        );
        assert_eq!(
            c.requests(Command::Quit),
            [
                ShellRequest::Close(a),
                ShellRequest::Close(b),
                ShellRequest::Close(c3)
            ]
        );
        // The active window goes last, to stay on top; the minimised one is left alone.
        assert_eq!(
            c.requests(Command::BringAllToFront),
            [ShellRequest::Activate(a), ShellRequest::Activate(b)]
        );
        assert_eq!(c.requests(Command::Show(c3)), [ShellRequest::Activate(c3)]);
    }

    #[test]
    fn the_window_menu_lists_the_windows_with_the_active_one_checked() {
        let c = context(false, true);
        let rows = &c.sections()[2].rows;
        let listed: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Item {
                    label,
                    checked: Some(on),
                    ..
                } => Some((label.as_str(), *on)),
                _ => None,
            })
            .collect();
        assert_eq!(listed, [("One", false), ("Two", true), ("Three", false)]);
    }

    #[test]
    fn search_entries_leave_out_what_cannot_be_chosen() {
        let c = context(false, true);
        let paths: Vec<String> = c.entries().iter().map(|e| e.path.join("/")).collect();
        // About is disabled.
        assert!(!paths.iter().any(|p| p.contains("About")));
        assert!(paths.contains(&"Firefox/Quit Firefox".to_owned()));
        assert!(paths.contains(&"Edit/Paste".to_owned()));
    }

    #[test]
    fn the_kit_menu_has_a_row_for_every_row() {
        let c = context(false, true);
        let sections = c.sections();
        let message = |command: Command| command;
        let built = menu(&sections[1].rows, &message);
        assert_eq!(built.items().len(), sections[1].rows.len());
        assert_eq!(built.items()[0].label(), "Undo");
        assert!(built.items()[1].is_separator());
        assert_eq!(titles(&sections, &message).len(), 3);
    }
}
