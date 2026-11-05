//! Modal dialogs as sheets (task 2.23c, ADR 0058).
//!
//! A toplevel with a parent that `xdg-dialog` marks modal is a *sheet*: its frame is
//! centred horizontally on the parent's client area with its top against the top of it
//! (under a server-side title bar, or at the top of a window that draws its own chrome),
//! and it stays there. [`State::attach_sheets`] puts every mapped sheet back where its
//! parent says. The window manager calls it after its own moves, zooms and commits, so a
//! state read right after sees the sheet in place; the event loop also runs it once per
//! iteration, which catches every other way a parent can move (outputs going away, full
//! screen, a minimised family coming back). A sheet doesn't move on its own and gets no
//! resize edges. No slide animation yet (Phase 4).

use smithay::desktop::Window;
use smithay::utils::{Logical, Point};
use smithay::wayland::shell::xdg::dialog::ToplevelDialogHint;

use crate::decorations::title_bar_height;
use crate::placement;
use crate::state::State;
use crate::wm::rect;

impl State {
    /// Whether `window` is shown as a sheet: a modal `xdg-dialog` with a parent.
    pub fn is_sheet(&self, window: &Window) -> bool {
        window.toplevel().and_then(|t| t.parent()).is_some()
            && self.dialog_hint(window) == ToplevelDialogHint::Modal
    }

    /// Where sheet `window`'s geometry goes, given where its parent is now. `None` if it
    /// isn't a sheet, its parent isn't mapped, or it is zoomed or full screen (it then
    /// follows those rules instead).
    pub(crate) fn sheet_location(&self, window: &Window) -> Option<Point<i32, Logical>> {
        if !self.is_sheet(window)
            || Self::is_fullscreen(window)
            || Self::with_wm_data(window, |d| d.zoomed)
        {
            return None;
        }
        let parent = self.parent_window(window)?;
        let parent = self.space.element_geometry(&parent)?;
        let size = window.geometry().size;
        let bar = title_bar_height(window);
        let (x, y) = placement::sheet(rect(parent), (size.w, size.h + bar));
        Some(Point::from((x, y + bar)))
    }

    /// Move every mapped sheet back under its parent's chrome, after the parent moved or
    /// resized or the sheet changed size. Children are above their parents in the stack,
    /// so one pass from the bottom also settles a sheet on a sheet.
    pub(crate) fn attach_sheets(&mut self) {
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        for window in windows {
            if let Some(loc) = self.sheet_location(&window)
                && self.space.element_location(&window) != Some(loc)
            {
                // Relocated, not remapped: remapping would raise it above its siblings.
                self.space.relocate_element(&window, loc);
                self.notify_window_changed(&window);
            }
        }
    }
}
