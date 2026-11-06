//! The edit menu of text fields and text views (task 2.17b, ADR 0046): Cut, Copy and
//! Select All, opened by a secondary press, the Menu key or Shift-F10, or an assistive
//! technology. Paste is missing until a widget can ask the platform for the clipboard
//! (the drivers hand its text over only on the paste key).

use std::sync::Arc;

use masonry_core::core::{BrushIndex, MutateCtx};
use masonry_core::parley::PlainEditor;

use crate::menu::{ItemSpec, MenuSpec, MenuToggle, Shortcut, SpecKind};

/// What an item of the edit menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditCommand {
    Cut,
    Copy,
    SelectAll,
}

/// The menu's items in order; an item's id is its index.
const ITEMS: [(Option<EditCommand>, &str, &str); 4] = [
    (Some(EditCommand::Cut), "Cut", "X"),
    (Some(EditCommand::Copy), "Copy", "C"),
    (None, "", ""),
    (Some(EditCommand::SelectAll), "Select All", "A"),
];

impl EditCommand {
    /// The command of the item `id`.
    pub(crate) fn from_id(id: usize) -> Option<Self> {
        ITEMS.get(id).and_then(|(command, ..)| *command)
    }
}

/// The edit menu: Cut needs a selection in an `editable` widget, Copy a selection, Select
/// All some text.
pub(crate) fn spec(editable: bool, has_selection: bool, has_text: bool) -> MenuSpec {
    MenuSpec(
        ITEMS
            .iter()
            .enumerate()
            .map(|(id, &(command, label, key))| ItemSpec {
                id,
                label: label.to_owned(),
                shortcut: command.map(|_| Shortcut::command(key)),
                signal: None,
                enabled: match command {
                    Some(EditCommand::Cut) => editable && has_selection,
                    Some(EditCommand::Copy) => has_selection,
                    Some(EditCommand::SelectAll) => has_text,
                    None => true,
                },
                toggle: MenuToggle::None,
                kind: if command.is_some() {
                    SpecKind::Action
                } else {
                    SpecKind::Separator
                },
            })
            .collect::<Arc<[_]>>(),
    )
}

/// Runs `command` on `editor`, from a mutation of its widget; true if the text changed.
pub(crate) fn run(
    command: EditCommand,
    editor: &mut PlainEditor<BrushIndex>,
    ctx: &mut MutateCtx<'_>,
    editable: bool,
) -> bool {
    match command {
        EditCommand::SelectAll => {
            let (fonts, layouts) = ctx.text_contexts();
            editor.driver(fonts, layouts).select_all();
            false
        }
        EditCommand::Copy => {
            if let Some(text) = editor.selected_text() {
                ctx.set_clipboard(text.to_owned());
            }
            false
        }
        EditCommand::Cut => {
            let Some(text) = editor.selected_text().filter(|_| editable) else {
                return false;
            };
            ctx.set_clipboard(text.to_owned());
            let (fonts, layouts) = ctx.text_contexts();
            editor.driver(fonts, layouts).delete_selection();
            true
        }
    }
}
