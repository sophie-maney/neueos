//! The menu bar's pieces (task 3.2b): the logo menu, the active app's name and the clock
//! extra, put together in a [`MenuBar`]. Portable: the shell builds one per output, and the
//! gallery's Shell page and the goldens build the same.

use neue_kit::desktop::{MenuBar, MenuTitle};
use neue_kit::{Handle, Menu, MenuItem};

/// What the logo menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    /// A panel about this computer.
    About,
    /// The Preferences app (task 3.24; the item is disabled until then).
    Preferences,
    /// Locks the session (logind; the lock screen is task 3.18).
    Lock,
    /// Ends the session (the compositor quits, ADR 0027).
    LogOut,
    Restart,
    ShutDown,
}

/// What assistive technologies call the logo title.
pub const LOGO_NAME: &str = "System";

/// The logo menu, each item sending `message(command)`.
pub fn logo_menu<M: 'static>(message: impl Fn(SystemCommand) -> M + Clone + 'static) -> Menu<M> {
    let item = |label: &str, command| {
        let message = message.clone();
        MenuItem::new(label).on_choose_with(move || message(command))
    };
    Menu::new()
        .with(item("About This Computer", SystemCommand::About))
        .separator()
        // Until the Preferences app exists (task 3.24).
        .with(item("Preferences…", SystemCommand::Preferences).disabled(true))
        .separator()
        .with(item("Lock Screen", SystemCommand::Lock))
        .separator()
        .with(item("Restart", SystemCommand::Restart))
        .with(item("Shut Down", SystemCommand::ShutDown))
        .separator()
        .with(item("Log Out", SystemCommand::LogOut))
}

/// The clock extra's menu: today's full date, for reading only.
pub fn clock_menu<M: 'static>(full_date: &str) -> Menu<M> {
    Menu::new().with(MenuItem::new(full_date).disabled(true))
}

/// Handles to the parts of one bar that change.
#[derive(Debug)]
pub struct BarHandles<M> {
    /// The active app's name.
    pub app: Handle<MenuTitle<M>>,
    /// The clock extra.
    pub clock: Handle<MenuTitle<M>>,
}

impl<M> Default for BarHandles<M> {
    fn default() -> Self {
        Self {
            app: Handle::default(),
            clock: Handle::default(),
        }
    }
}

/// What a bar shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarContent {
    /// The active app's name, in bold after the logo.
    pub app_name: String,
    /// The clock extra's text.
    pub time: String,
    /// The first item of the clock's menu.
    pub full_date: String,
}

/// A menu bar showing `content`, its logo menu sending `message(command)`; `handles` are
/// pointed at the parts that change.
pub fn menu_bar<M: 'static>(
    content: &BarContent,
    message: impl Fn(SystemCommand) -> M + Clone + 'static,
    handles: &mut BarHandles<M>,
) -> MenuBar<M> {
    MenuBar::new()
        .with(MenuTitle::logo(LOGO_NAME, logo_menu(message)))
        // The app's own menus come with the global menu (task 3.7).
        .with(
            MenuTitle::new(content.app_name.clone(), Menu::new())
                .bold(true)
                .bind(&mut handles.app),
        )
        .with_extra(
            MenuTitle::new(content.time.clone(), clock_menu(&content.full_date))
                .bind(&mut handles.clock),
        )
}
