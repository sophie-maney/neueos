//! Full-screen windows (task 1.15a, ADR 0018): the `xdg_toplevel` fullscreen state.
//!
//! A stopgap until full-screen Spaces (Phase 4). Rules, in short:
//! - A full-screen window fills its output: the one it asked for, else the one it is
//!   mostly on, else the active one. Exclusive zones don't apply and it has no title bar.
//! - A window that commits a smaller size is centred on a black backdrop (xdg-shell).
//! - While the topmost window on an output is full screen (or a child of one), that
//!   output's `top` layer goes below the windows; `overlay` stays above.
//! - Leaving full screen restores the place and size it had (a zoomed window is refitted
//!   to its work area instead). Move and resize grabs are refused while full screen;
//!   `move-window`/`resize-window` leave full screen first. Zoom requests get a configure
//!   and are otherwise ignored.
//! - When its output goes away it moves to the first remaining output and stays full
//!   screen there; mode and scale changes refit it.

use smithay::desktop::{Space, Window};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::utils::{Logical, Point, Rectangle};
use tracing::debug;

use crate::placement;
use crate::state::State;

impl State {
    /// The output `window` is full screen on, if it is full screen.
    pub fn fullscreen_output(window: &Window) -> Option<Output> {
        Self::with_wm_data(window, |d| d.fullscreen.clone())
    }

    /// Whether `window` is full screen.
    pub fn is_fullscreen(window: &Window) -> bool {
        Self::with_wm_data(window, |d| d.fullscreen.is_some())
    }

    /// Make `window` full screen on `requested` (if it is in the layout), else on the
    /// output it is mostly on, else on the active output.
    pub fn set_fullscreen(&mut self, window: &Window, requested: Option<Output>) {
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        let mapped = self.window_geometry(window);
        let target = requested
            .filter(|o| self.space.output_geometry(o).is_some())
            .or_else(|| Self::fullscreen_output(window))
            .or_else(|| crate::outputs::primary_output(&self.space, window))
            .or_else(|| self.active_output());
        let Some((output, output_geo)) =
            target.and_then(|o| Some((o.clone(), self.space.output_geometry(&o)?)))
        else {
            // No output to fill: xdg-shell still wants a configure.
            if toplevel.is_initial_configure_sent() {
                toplevel.send_configure();
            }
            return;
        };
        debug!(output = output.name(), "window enters full screen");
        Self::with_wm_data(window, |d| {
            if d.fullscreen.is_none() {
                // A zoomed window is refitted on the way out instead.
                d.unfullscreen = mapped.filter(|_| !d.zoomed);
            }
            d.fullscreen = Some(output);
        });
        toplevel.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Fullscreen);
            state.size = Some(output_geo.size);
        });
        if mapped.is_some() {
            self.relocate(window, output_geo.loc);
            self.raise_window(window);
        }
        if toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
        self.notify_window_changed(window);
    }

    /// Take `window` out of full screen, back to where it was.
    pub fn unset_fullscreen(&mut self, window: &Window) {
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        let Some(restore) = Self::with_wm_data(window, |d| {
            d.fullscreen.take()?;
            Some(d.unfullscreen.take())
        }) else {
            // Not full screen: xdg-shell still wants a configure.
            if toplevel.is_initial_configure_sent() {
                toplevel.send_configure();
            }
            return;
        };
        debug!("window leaves full screen");
        toplevel.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
            state.size = restore.map(|g| g.size);
        });
        if Self::with_wm_data(window, |d| d.zoomed) {
            // Sends the configure with the work area's size.
            self.refit_zoomed(window);
        } else {
            if let Some(geo) = self.window_geometry(window) {
                let size = restore.map_or(geo.size, |g| g.size);
                let wanted = restore.map_or(geo.loc, |g| g.loc);
                let work = self.content_area(window, self.work_area_near(wanted));
                let loc = placement::constrain(
                    (work.loc.x, work.loc.y, work.size.w, work.size.h),
                    (size.w, size.h),
                    (wanted.x, wanted.y),
                );
                self.relocate(window, loc.into());
            }
            if toplevel.is_initial_configure_sent() {
                toplevel.send_configure();
            }
        }
        self.notify_window_changed(window);
    }

    /// Where a full-screen `window` goes on `output_geo`: centred if it is smaller.
    fn fullscreen_location(
        window: &Window,
        output_geo: Rectangle<i32, Logical>,
    ) -> Point<i32, Logical> {
        let size = window.geometry().size;
        let dx = (output_geo.size.w - size.w).max(0) / 2;
        let dy = (output_geo.size.h - size.h).max(0) / 2;
        output_geo.loc + Point::from((dx, dy))
    }

    /// A full-screen window committed: centre it if it is smaller than its output.
    pub(crate) fn fullscreen_committed(&mut self, window: &Window) {
        let Some(output) = Self::fullscreen_output(window) else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(&output) else {
            return;
        };
        let loc = Self::fullscreen_location(window, output_geo);
        if self.window_geometry(window).map(|g| g.loc) != Some(loc) {
            self.relocate(window, loc);
        }
    }

    /// Fit full-screen `window` to its output again (after a mode, scale or layout change),
    /// or to the first output if its own is gone.
    pub(crate) fn refit_fullscreen(&mut self, window: &Window) {
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        let current = Self::fullscreen_output(window);
        let target = current
            .filter(|o| self.space.output_geometry(o).is_some())
            .or_else(|| self.outputs().next().cloned());
        let Some((output, output_geo)) =
            target.and_then(|o| Some((o.clone(), self.space.output_geometry(&o)?)))
        else {
            return;
        };
        Self::with_wm_data(window, |d| d.fullscreen = Some(output));
        let resized = toplevel.with_pending_state(|state| {
            let changed = state.size != Some(output_geo.size);
            state.size = Some(output_geo.size);
            changed
        });
        if resized && toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        let loc = Self::fullscreen_location(window, output_geo);
        if self.window_geometry(window).map(|g| g.loc) != Some(loc) {
            self.relocate(window, loc);
        }
    }

    /// The work area of the output nearest `point` (or the active one).
    fn work_area_near(&self, point: Point<i32, Logical>) -> Rectangle<i32, Logical> {
        self.output_near(point)
            .or_else(|| self.active_output())
            .map(|o| self.work_area(&o))
            .unwrap_or_default()
    }

    /// Move a mapped or minimised `window` to `location` without changing its stacking.
    fn relocate(&mut self, window: &Window, location: Point<i32, Logical>) {
        if let Some(entry) = self.minimized.iter_mut().find(|(w, _)| w == window) {
            entry.1 = location;
        } else if self.space.element_location(window).is_some() {
            self.space.map_element(window.clone(), location, false);
        }
    }
}

/// The full-screen window that covers `output`: the topmost window on it is that window or
/// one of its children (a game's dialog keeps the panels hidden).
pub fn fullscreen_window_on(space: &Space<Window>, output: &Output) -> Option<Window> {
    let output_geo = space.output_geometry(output)?;
    let mut above: Vec<&Window> = Vec::new();
    for window in space.elements().rev() {
        if State::fullscreen_output(window).as_ref() == Some(output) {
            let children_only = above.iter().all(|w| descends_from(space, w, window));
            return children_only.then(|| window.clone());
        }
        if space
            .element_bbox(window)
            .is_some_and(|b| b.overlaps(output_geo))
        {
            above.push(window);
        }
    }
    None
}

/// Whether `window` is a child (or grandchild…) of `ancestor`, both in `space`.
fn descends_from(space: &Space<Window>, window: &Window, ancestor: &Window) -> bool {
    let parent_of = |w: &Window| {
        let parent = w.toplevel()?.parent()?;
        space
            .elements()
            .find(|e| e.toplevel().is_some_and(|t| t.wl_surface() == &parent))
            .cloned()
    };
    let mut current = parent_of(window);
    // Parent loops are a protocol error, but don't spin if a client manages one.
    let mut budget = space.elements().count();
    while let Some(w) = current {
        if &w == ancestor {
            return true;
        }
        if budget == 0 {
            return false;
        }
        budget -= 1;
        current = parent_of(&w);
    }
    false
}
