//! The Help menu and its search panel (task 3.9, ADR 0082). Help ▸ "Search Menus…" opens a
//! popover under the Help title with a search field and the items found in the menus being
//! shown, whatever their source (an app's, or the fallback menu).

use neue_kit::desktop::MenuTitle;
use neue_kit::{Button, Cx, Handle, Menu, MenuItem, Popover, Stack, TextField};

/// The title of the menu. An app that exports a menu of this name gets the search item at
/// the top of it, rather than a second Help.
pub const TITLE: &str = "Help";

/// The item that opens the search panel.
pub const SEARCH_LABEL: &str = "Search Menus…";

/// The field's width, in logical pixels.
const FIELD_WIDTH: f64 = 280.0;

/// Whether `label` names a Help menu.
pub fn is_help(label: &str) -> bool {
    label.trim().eq_ignore_ascii_case(TITLE)
}

/// The item that opens the panel, sending `open`.
pub fn search_item<M: Clone + 'static>(open: M) -> MenuItem<M> {
    MenuItem::new(SEARCH_LABEL).on_choose(open)
}

/// The shell's own Help menu, for windows whose menus have none; `handle` is pointed at
/// its title, which the panel opens under.
pub fn title<M: Clone + 'static>(open: M, handle: &mut Handle<MenuTitle<M>>) -> MenuTitle<M> {
    MenuTitle::new(TITLE, Menu::new().with(search_item(open))).bind(handle)
}

/// Handles to the open panel's parts.
#[derive(Debug)]
pub struct PanelHandles<M> {
    pub field: Handle<TextField<M>>,
    pub hits: Handle<Stack<M>>,
}

impl<M> Default for PanelHandles<M> {
    fn default() -> Self {
        Self {
            field: Handle::default(),
            hits: Handle::default(),
        }
    }
}

/// Shows the search panel under `anchor` (the Help title): a search field that sends
/// `query(text)` as it changes and `submit(text)` on Return, and an empty list of hits,
/// filled by [`set_hits`]. `closed` is sent when the panel goes away by itself (Escape, a
/// click elsewhere).
pub fn show<M: Clone + 'static>(
    cx: &mut Cx<'_, M>,
    anchor: &Handle<MenuTitle<M>>,
    handles: &mut PanelHandles<M>,
    query: fn(String) -> M,
    submit: fn(String) -> M,
    closed: M,
) {
    let content = Stack::column()
        .spacing(6.0)
        .with(
            TextField::search()
                .placeholder("Search")
                .label("Search menus")
                .width(FIELD_WIDTH)
                .on_change(query)
                .on_submit(submit)
                .bind(&mut handles.field),
        )
        .with(Stack::column().spacing(4.0).bind(&mut handles.hits));
    cx.show_popover(
        anchor,
        Popover::new(content).focus(&handles.field).on_close(closed),
    );
}

/// Closes the panel, if it is open.
pub fn close<M: Clone + 'static>(cx: &mut Cx<'_, M>, anchor: &Handle<MenuTitle<M>>) {
    cx.close_popover(anchor);
}

/// Replaces the hits under the field with `texts`, each pressed sending `pick(index)`.
pub fn set_hits<M: Clone + 'static>(
    cx: &mut Cx<'_, M>,
    handles: &PanelHandles<M>,
    texts: &[String],
    pick: fn(usize) -> M,
) {
    handles.hits.clear(cx);
    for (i, text) in texts.iter().enumerate() {
        handles
            .hits
            .push(cx, Button::new(text.clone()).on_press_with(move || pick(i)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_menus_are_found_by_name() {
        assert!(is_help("Help"));
        assert!(is_help(" help "));
        assert!(!is_help("Helpers"));
        assert!(!is_help("File"));
    }

    #[test]
    fn the_title_opens_the_search() {
        let mut handle = Handle::default();
        let _ = title(1u8, &mut handle);
        assert!(handle.is_bound());
        assert_eq!(search_item(1u8).label(), SEARCH_LABEL);
    }
}
