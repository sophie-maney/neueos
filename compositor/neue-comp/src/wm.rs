//! Floating window management (task 1.5, ADR 0008): focus and stacking, placement,
//! minimise, zoom, interactive and scripted move and resize, and attention requests.
//!
//! Rules, in short:
//! - **Stacking:** a raised window takes its children (dialogs, `xdg_toplevel.set_parent`)
//!   with it, so children always stay above their parent.
//! - **Sheets:** a modal dialog hangs from its parent's chrome and follows it (`sheets.rs`).
//! - **Focus:** pressing a button on a window raises and focuses it; new windows are
//!   focused when they map. A window with a sheet open passes the keyboard to the sheet. When the focused window goes away, focus moves to its parent,
//!   else the topmost window of the same client, else the topmost window.
//! - **Minimise** hides a window and its children; they keep their position and come back
//!   together. Minimised windows are never focused or under the pointer.
//! - **Zoom** is the OS X name for the green button; on Wayland it is the `maximized`
//!   state, filling the work area. Dragging a zoomed window unzooms it, once the pointer
//!   has travelled the drag threshold (a click leaves it zoomed).
//! - **Full screen** is in `fullscreen.rs` (ADR 0018).
//! - **xdg-activation** only grants focus to tokens made by the focused client (see
//!   `handlers.rs`); a refused request marks the window as demanding attention.

use std::cell::RefCell;

use smithay::desktop::{Window, layer_map_for_output};
use smithay::input::Seat;
use smithay::input::pointer::{Focus, GrabStartData};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Serial, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
use tracing::debug;

use neue_ipc::{Event, WindowInfo};

use crate::grabs::{self, Edges, MoveGrab, ResizeGrab};
use crate::placement::{self, NewWindow, Rect};
use crate::state::State;

/// Window-management state kept in each window's user data.
#[derive(Debug, Default)]
pub struct WmData {
    /// Whether we put the window in the zoomed (maximized) state.
    pub zoomed: bool,
    /// Geometry before zooming, restored when unzooming. `None` if it was zoomed before it
    /// was first mapped.
    pub unzoomed: Option<Rectangle<i32, Logical>>,
    /// The output the window is full screen on (`fullscreen.rs`).
    pub fullscreen: Option<Output>,
    /// Geometry before going full screen, restored when leaving it. `None` if it was
    /// zoomed or not mapped yet.
    pub unfullscreen: Option<Rectangle<i32, Logical>>,
    /// An activation request was refused (see [`State::request_attention`]).
    pub demands_attention: bool,
    /// An interactive move or resize is running; changes are reported when it ends.
    pub grabbed: bool,
    pub resize: ResizeState,
    /// The last [`WindowInfo`] subscribers saw, with `focused` cleared. `None` until the
    /// window is mapped and `window_opened` was sent.
    pub reported: Option<WindowInfo>,
}

/// Progress of an interactive resize. While resizing from the top or left, each commit
/// moves the window so the opposite edges stay put.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResizeState {
    #[default]
    Idle,
    Resizing {
        edges: Edges,
        initial: Rectangle<i32, Logical>,
    },
    /// The grab ended; the next commit carries the final size.
    WaitingForLastCommit {
        edges: Edges,
        initial: Rectangle<i32, Logical>,
    },
}

/// `rect` as a placement tuple.
pub(crate) fn rect(r: Rectangle<i32, Logical>) -> Rect {
    (r.loc.x, r.loc.y, r.size.w, r.size.h)
}

/// The client that owns `window`.
fn client_of(window: &Window) -> Option<ClientId> {
    window
        .toplevel()
        .and_then(|t| t.wl_surface().client())
        .map(|c| c.id())
}

/// Title and app id of `window`.
pub fn title_and_app_id(window: &Window) -> (Option<String>, Option<String>) {
    let Some(toplevel) = window.toplevel() else {
        return (None, None);
    };
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok())
            .map(|data| (data.title.clone(), data.app_id.clone()))
            .unwrap_or_default()
    })
}

impl State {
    /// Run `f` on `window`'s [`WmData`].
    pub fn with_wm_data<T>(window: &Window, f: impl FnOnce(&mut WmData) -> T) -> T {
        let user_data = window.user_data();
        user_data.insert_if_missing(|| RefCell::new(WmData::default()));
        // Inserted just above; only the compositor thread touches window user data.
        match user_data.get::<RefCell<WmData>>() {
            Some(data) => f(&mut data.borrow_mut()),
            None => unreachable!("WmData was just inserted"),
        }
    }

    /// Whether `window` is minimised.
    pub fn is_minimized(&self, window: &Window) -> bool {
        self.minimized.iter().any(|(w, _)| w == window)
    }

    /// Geometry of a mapped or minimised window.
    pub fn window_geometry(&self, window: &Window) -> Option<Rectangle<i32, Logical>> {
        self.space.element_geometry(window).or_else(|| {
            self.minimized
                .iter()
                .find(|(w, _)| w == window)
                .map(|(w, loc)| Rectangle::new(*loc, w.geometry().size))
        })
    }

    /// The mapped or minimised window `window` is a child of.
    pub fn parent_window(&self, window: &Window) -> Option<Window> {
        let parent = window.toplevel()?.parent()?;
        self.window_for_surface(&parent)
    }

    /// The output under the pointer, else the first one.
    pub fn active_output(&self) -> Option<Output> {
        let pos = self.pointer.current_location();
        self.space
            .output_under(pos)
            .next()
            .or_else(|| self.space.outputs().next())
            .cloned()
    }

    /// The part of `output` windows may use: everything outside panels' exclusive zones.
    pub fn work_area(&self, output: &Output) -> Rectangle<i32, Logical> {
        let Some(geo) = self.space.output_geometry(output) else {
            return Rectangle::default();
        };
        let mut zone = layer_map_for_output(output).non_exclusive_zone();
        zone.loc += geo.loc;
        zone
    }

    /// Where `window`'s client area may go on the output it is mostly on (or the active
    /// one): the work area below its title bar.
    fn work_area_for(&self, window: &Window) -> Rectangle<i32, Logical> {
        let output =
            crate::outputs::primary_output(&self.space, window).or_else(|| self.active_output());
        let work = output.map(|o| self.work_area(&o)).unwrap_or_default();
        self.content_area(window, work)
    }

    /// Where `window`'s client area may go on the active output.
    pub(crate) fn active_content_area(&self, window: &Window) -> Rectangle<i32, Logical> {
        let work = self
            .active_output()
            .map(|o| self.work_area(&o))
            .unwrap_or_default();
        self.content_area(window, work)
    }

    /// Children of `window`, bottom to top.
    fn children(&self, window: &Window) -> Vec<Window> {
        let Some(surface) = window.toplevel().map(|t| t.wl_surface().clone()) else {
            return Vec::new();
        };
        self.space
            .elements()
            .filter(|w| w.toplevel().and_then(|t| t.parent()).as_ref() == Some(&surface))
            .cloned()
            .collect()
    }

    /// Raise `window`, then its children (and theirs) in their current order.
    pub fn raise_window(&mut self, window: &Window) {
        self.space.raise_element(window, true);
        let mut pending = self.children(window);
        // Parent loops are a protocol error, but don't spin if a client manages one.
        let mut budget = self.space.elements().count();
        while let Some(child) = (!pending.is_empty()).then(|| pending.remove(0)) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            self.space.raise_element(&child, false);
            pending.extend(self.children(&child));
        }
    }

    /// Where keyboard focus meant for `window` goes: its topmost sheet (a modal child),
    /// that sheet's topmost sheet, and so on; `window` itself when it has none. A window
    /// with a sheet open takes no keyboard input of its own (task 2.36).
    pub(crate) fn modal_focus_target(&self, window: &Window) -> Window {
        let mut target = window.clone();
        // Parent loops are a protocol error, but don't spin if a client manages one.
        for _ in 0..self.space.elements().count() {
            let sheet = self
                .children(&target)
                .into_iter()
                .rev()
                .find(|c| self.is_sheet(c));
            match sheet {
                Some(sheet) => target = sheet,
                None => break,
            }
        }
        target
    }

    /// Raise `window`, give it keyboard focus and mark it activated (and every other
    /// window deactivated). A minimised window is restored first. If `window` has a sheet
    /// open, the sheet gets the keyboard and the activated state instead
    /// ([`Self::modal_focus_target`]); pointer input to `window` is unchanged.
    pub fn focus_window(&mut self, window: &Window) {
        if self.is_minimized(window) {
            self.restore_minimized(window);
        }
        self.raise_window(window);
        let target = self.modal_focus_target(window);
        if &target != window {
            // As `raise_element(.., true)` does for `window`, and as when the sheet mapped.
            for w in self.space.elements() {
                w.set_activated(w == &target);
            }
        }
        // A locked session or an exclusive layer surface (a launcher) keeps the keyboard.
        if self.is_locked() || self.exclusive_layer_surface().is_some() {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        let surface = target.toplevel().map(|t| t.wl_surface().clone());
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, surface, serial);
        let was_asking = Self::with_wm_data(window, |d| std::mem::take(&mut d.demands_attention));
        self.send_pending_configures();
        if was_asking {
            self.notify_window_changed(window);
        }
    }

    /// Focus the topmost window, or nothing when no window is left.
    pub fn focus_topmost(&mut self) {
        self.focus_next(None, None);
    }

    /// Move focus after the focused window went away: to `parent` if it is mapped, else
    /// to the topmost window of `client`, else to the topmost window.
    pub fn focus_next(&mut self, parent: Option<Window>, client: Option<ClientId>) {
        let mapped = |w: &Window| self.space.elements().any(|e| e == w);
        let next = parent
            .filter(mapped)
            .or_else(|| {
                client.and_then(|c| {
                    self.space
                        .elements()
                        .rev()
                        .find(|w| client_of(w).as_ref() == Some(&c))
                        .cloned()
                })
            })
            .or_else(|| self.space.elements().next_back().cloned());
        match next {
            Some(window) => self.focus_window(&window),
            None => self.clear_focus(),
        }
    }

    /// Deactivate every window and drop keyboard focus.
    pub fn clear_focus(&mut self) {
        if self.is_locked() {
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        for window in self.space.elements() {
            window.set_activated(false);
        }
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, None, serial);
        // Smithay does not call `focus_changed` when focus is dropped.
        self.refresh_pointer_constraint_for(None);
        self.send_pending_configures();
    }

    pub(crate) fn send_pending_configures(&self) {
        let minimized = self.minimized.iter().map(|(w, _)| w);
        for window in self.space.elements().chain(minimized) {
            if let Some(toplevel) = window.toplevel() {
                toplevel.send_pending_configure();
            }
        }
    }

    /// Whether `window` (or one of its children) has keyboard focus.
    fn has_focus_in_family(&self, window: &Window) -> bool {
        let Some(focus) = self.keyboard.current_focus() else {
            return false;
        };
        let mut current = self.window_for_surface(&focus);
        while let Some(w) = current {
            if &w == window {
                return true;
            }
            current = self.parent_window(&w);
        }
        false
    }

    /// Place and map a window that just committed its first buffer, then focus it.
    pub(crate) fn map_new_window(&mut self, window: Window) {
        let work = self.active_content_area(&window);
        let size = window.geometry().size;
        let fullscreen = Self::fullscreen_output(&window)
            .and_then(|o| self.space.output_geometry(&o))
            .map(|g| g.loc);
        let location = if let Some(loc) = fullscreen {
            loc
        } else if Self::with_wm_data(&window, |d| d.zoomed) {
            work.loc
        } else if let Some(loc) = self.sheet_location(&window) {
            loc
        } else {
            let (_, app_id) = title_and_app_id(&window);
            let parent = self
                .parent_window(&window)
                .and_then(|p| self.space.element_geometry(&p));
            let sibling = app_id.as_ref().and_then(|id| {
                self.space
                    .elements()
                    .rev()
                    .find(|w| {
                        title_and_app_id(w).1.as_ref() == Some(id)
                            && self.parent_window(w).is_none()
                    })
                    .and_then(|w| self.space.element_geometry(w))
            });
            let remembered = app_id.and_then(|id| self.remembered.get(&id).copied());
            let new = NewWindow {
                size: (size.w, size.h),
                parent: parent.map(rect),
                sibling: sibling.map(rect),
                remembered,
            };
            placement::place(rect(work), &new).into()
        };
        debug!(?location, ?size, "mapping window");
        self.space.map_element(window.clone(), location, true);
        self.announce_toplevel(&window);
        if let Some(info) = self.window_info(&window, None) {
            Self::with_wm_data(&window, |d| d.reported = Some(info.clone()));
            self.emit(Event::WindowOpened(info));
        }
        self.focus_window(&window);
    }

    /// Forget a destroyed toplevel: unmap it, remember where its app's window was, report
    /// it and move focus on.
    pub(crate) fn window_destroyed(&mut self, window: &Window) {
        let was_focused =
            self.keyboard.current_focus().as_ref() == window.toplevel().map(|t| t.wl_surface());
        let parent = self.parent_window(window);
        let client = client_of(window);
        self.chrome_window_gone(window);
        let geometry = Self::with_wm_data(window, |d| {
            d.unfullscreen.or(d.zoomed.then_some(d.unzoomed).flatten())
        })
        .or_else(|| self.window_geometry(window));
        if let ((_, Some(app_id)), None, Some(geo)) = (title_and_app_id(window), &parent, geometry)
        {
            self.remembered.insert(app_id, (geo.loc.x, geo.loc.y));
        }

        self.retire_foreign_toplevel(window);
        self.space.unmap_elem(window);
        self.minimized.retain(|(w, _)| w != window);
        if let Some(id) = self.window_id(window) {
            self.emit(Event::WindowClosed { window: id });
        }
        if was_focused {
            self.focus_next(parent, client);
            // Smithay does not call `focus_changed` when the destroyed surface already
            // cleared the focus, so report it ourselves (a no-op if it was reported).
            self.report_focus(self.focused_window_id());
        }
    }

    /// Hide `window` and its children, keeping their positions.
    pub fn minimize_window(&mut self, window: &Window) {
        if self.is_minimized(window) || !self.space.elements().any(|w| w == window) {
            return;
        }
        let had_focus = self.has_focus_in_family(window);
        let mut family = vec![window.clone()];
        let mut i = 0;
        while let Some(w) = family.get(i).cloned() {
            let children: Vec<Window> = self
                .children(&w)
                .into_iter()
                .filter(|c| !family.contains(c))
                .collect();
            family.extend(children);
            i += 1;
        }
        for w in &family {
            if let Some(loc) = self.space.element_location(w) {
                self.space.unmap_elem(w);
                w.set_activated(false);
                self.minimized.push((w.clone(), loc));
            }
        }
        if had_focus {
            self.focus_next(self.parent_window(window), client_of(window));
        }
        self.send_pending_configures();
        for w in &family {
            self.notify_window_changed(w);
        }
    }

    /// Show a minimised `window` and its children again and focus it.
    pub fn unminimize_window(&mut self, window: &Window) {
        if self.is_minimized(window) {
            self.focus_window(window);
        }
    }

    /// Map `window` and its minimised children back where they were.
    fn restore_minimized(&mut self, window: &Window) {
        let mut family = vec![window.clone()];
        let mut i = 0;
        while let Some(w) = family.get(i).cloned() {
            let surface = w.toplevel().map(|t| t.wl_surface().clone());
            let children: Vec<Window> = self
                .minimized
                .iter()
                .map(|(c, _)| c)
                .filter(|c| surface.is_some() && c.toplevel().and_then(|t| t.parent()) == surface)
                .cloned()
                .collect();
            family.extend(children);
            i += 1;
        }
        for w in &family {
            if let Some(index) = self.minimized.iter().position(|(m, _)| m == w) {
                let (w, loc) = self.minimized.remove(index);
                self.space.map_element(w.clone(), loc, false);
            }
        }
        for w in &family {
            self.notify_window_changed(w);
        }
    }

    /// Zoom (`maximized`) or unzoom `window`. `location` overrides where an unzoomed
    /// window goes (used when a drag unzooms it).
    pub fn set_zoomed(
        &mut self,
        window: &Window,
        zoom: bool,
        location: Option<Point<i32, Logical>>,
    ) {
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        if Self::is_fullscreen(window) {
            // Ignored while full screen (ADR 0018), but xdg-shell wants a configure.
            if toplevel.is_initial_configure_sent() {
                toplevel.send_configure();
            }
            return;
        }
        let mapped = self.space.element_geometry(window);
        let work = match mapped {
            Some(_) => self.work_area_for(window),
            None => self.active_content_area(window),
        };
        let was_zoomed = Self::with_wm_data(window, |d| d.zoomed);
        if zoom && !was_zoomed {
            Self::with_wm_data(window, |d| {
                d.zoomed = true;
                d.unzoomed = mapped;
            });
            toplevel.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Maximized);
                state.size = Some(work.size);
            });
            if mapped.is_some() {
                self.space.map_element(window.clone(), work.loc, false);
            }
        } else if !zoom && was_zoomed {
            let unzoomed = Self::with_wm_data(window, |d| {
                d.zoomed = false;
                d.unzoomed.take()
            });
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Maximized);
                state.size = unzoomed.map(|g| g.size);
            });
            let target = location.or(unzoomed.map(|g| g.loc));
            if let (Some(_), Some(loc)) = (mapped, target) {
                let size = unzoomed.map_or(window.geometry().size, |g| g.size);
                let loc = placement::constrain(rect(work), (size.w, size.h), (loc.x, loc.y));
                self.space.map_element(window.clone(), loc, false);
            }
        }
        self.attach_sheets();
        // xdg-shell wants a configure in reply to every (un)maximize request.
        if toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
    }

    /// Fit a zoomed `window` to its work area again, e.g. after its title bar came or went.
    pub(crate) fn refit_zoomed(&mut self, window: &Window) {
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        let work = self.work_area_for(window);
        toplevel.with_pending_state(|state| state.size = Some(work.size));
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        if self.space.element_geometry(window).is_some() {
            self.space.map_element(window.clone(), work.loc, false);
        }
    }

    /// Move `window` so its geometry starts at `location` (top edge kept in the work area
    /// of the output the middle of the top edge lands on, or the nearest one).
    /// A sheet stays attached to its parent and ignores this.
    pub fn move_window_to(&mut self, window: &Window, location: Point<i32, Logical>) {
        if self.is_sheet(window) {
            return;
        }
        let top_middle = location + Point::from((window.geometry().size.w / 2, 0));
        let work = self
            .output_near(top_middle)
            .map(|o| self.content_area(window, self.work_area(&o)))
            .unwrap_or_else(|| self.work_area_for(window));
        let loc = placement::constrain_move(rect(work), (location.x, location.y));
        if let Some(index) = self.minimized.iter().position(|(w, _)| w == window) {
            self.minimized[index].1 = loc.into();
        } else {
            // Relocated, not remapped: remapping would raise it above its children.
            self.space.relocate_element(window, loc);
            self.attach_sheets();
        }
    }

    /// Ask `window` to take `size` (within its limits). Its top-left corner stays put.
    pub fn resize_window(&mut self, window: &Window, size: Size<i32, Logical>) {
        let Some(toplevel) = window.toplevel().cloned() else {
            return;
        };
        if Self::is_fullscreen(window) {
            self.unset_fullscreen(window);
        }
        if Self::with_wm_data(window, |d| d.zoomed) {
            self.set_zoomed(window, false, None);
        }
        let limits = grabs::size_limits(toplevel.wl_surface());
        let size = grabs::resized(size, Edges::default(), Point::default(), limits);
        toplevel.with_pending_state(|state| state.size = Some(size));
        toplevel.send_pending_configure();
    }

    /// The pointer grab a move or resize request may start, if `serial` is the press that
    /// started the current implicit grab on one of `surface`'s client's surfaces.
    fn check_grab(
        &self,
        seat: &Seat<State>,
        surface: &WlSurface,
        serial: Serial,
    ) -> Option<GrabStartData<State>> {
        let pointer = seat.get_pointer()?;
        if !pointer.has_grab(serial) {
            return None;
        }
        let start = pointer.grab_start_data()?;
        let (focus, _) = start.focus.as_ref()?;
        focus.id().same_client_as(&surface.id()).then_some(start)
    }

    /// `xdg_toplevel.move`: drag the window with the pointer, once it has travelled the
    /// drag threshold; a zoomed window is unzoomed then (see [`MoveGrab`]), so a click
    /// leaves it zoomed. Refused while full screen and for a sheet.
    pub(crate) fn start_move(&mut self, surface: &WlSurface, seat: &Seat<State>, serial: Serial) {
        let Some(start_data) = self.check_grab(seat, surface, serial) else {
            return;
        };
        let Some(window) = self.window_for_surface(surface) else {
            return;
        };
        if !self.can_move(&window) {
            return;
        }
        let grab = MoveGrab::new(start_data, window);
        let pointer = self.pointer.clone();
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    /// Whether `window` may be dragged: it is mapped, not full screen and not a sheet.
    fn can_move(&self, window: &Window) -> bool {
        !Self::is_fullscreen(window)
            && !self.is_sheet(window)
            && self.space.element_geometry(window).is_some()
    }

    /// Get `window` ready to be dragged with the pointer at `pointer`: a zoomed window is
    /// unzoomed first, keeping the pointer at the same relative spot of its title bar, and
    /// the window is marked as grabbed. Returns where its geometry starts, or `None` if it
    /// is not mapped, is full screen or is a sheet.
    pub(crate) fn begin_move(
        &mut self,
        window: &Window,
        pointer: Point<f64, Logical>,
    ) -> Option<Point<i32, Logical>> {
        if !self.can_move(window) {
            return None;
        }
        let mut geo = self.space.element_geometry(window)?;
        if Self::with_wm_data(window, |d| d.zoomed) {
            let unzoomed = Self::with_wm_data(window, |d| d.unzoomed);
            let size = unzoomed.map_or(geo.size, |g| g.size);
            let fraction = (pointer.x - f64::from(geo.loc.x)) / f64::from(geo.size.w.max(1));
            // Rounded to whole logical pixels on purpose.
            let x = (pointer.x - fraction * f64::from(size.w)).round() as i32;
            let loc = Point::from((x, geo.loc.y));
            self.set_zoomed(window, false, Some(loc));
            geo = self.space.element_geometry(window).unwrap_or(geo);
        }
        Self::with_wm_data(window, |d| d.grabbed = true);
        Some(geo.loc)
    }

    /// `xdg_toplevel.resize`: drag `edge` with the pointer. Ignored while zoomed or full
    /// screen.
    pub(crate) fn start_resize(
        &mut self,
        surface: &WlSurface,
        seat: &Seat<State>,
        serial: Serial,
        edge: xdg_toplevel::ResizeEdge,
    ) {
        let Some(start_data) = self.check_grab(seat, surface, serial) else {
            return;
        };
        let Some(window) = self.window_for_surface(surface) else {
            return;
        };
        let Some(geo) = self.space.element_geometry(&window) else {
            return;
        };
        if Self::with_wm_data(&window, |d| d.zoomed) || Self::is_fullscreen(&window) {
            return;
        }
        Self::with_wm_data(&window, |d| d.grabbed = true);
        let grab = ResizeGrab::start(start_data, window, edge.into(), geo);
        self.set_cursor_override(Some(grab.edges.cursor_icon()));
        let pointer = self.pointer.clone();
        pointer.set_grab(self, grab, serial, Focus::Clear);
    }

    /// A mapped or minimised window committed: keep the anchored edges of a resize in
    /// place and report what changed.
    pub(crate) fn window_committed(&mut self, window: &Window) {
        let resize = Self::with_wm_data(window, |d| {
            let resize = d.resize;
            if let ResizeState::WaitingForLastCommit { .. } = resize {
                d.resize = ResizeState::Idle;
                d.grabbed = false;
            }
            resize
        });
        let grab_ended = matches!(resize, ResizeState::WaitingForLastCommit { .. });
        if let ResizeState::Resizing { edges, initial }
        | ResizeState::WaitingForLastCommit { edges, initial } = resize
        {
            let loc = edges.anchored_location(initial, window.geometry().size);
            if self
                .space
                .element_location(window)
                .is_some_and(|l| l != loc)
            {
                self.space.map_element(window.clone(), loc, false);
            }
        }
        self.fullscreen_committed(window);
        self.attach_sheets();
        if grab_ended {
            self.grab_ended(window);
        } else {
            self.notify_window_changed(window);
        }
    }

    /// A move or resize of `window` ended: report it and the sheets that followed it.
    pub(crate) fn grab_ended(&mut self, window: &Window) {
        Self::with_wm_data(window, |d| d.grabbed = false);
        self.notify_window_changed(window);
        for child in self.children(window) {
            self.notify_window_changed(&child);
        }
    }

    /// Mark `window` as wanting focus it wasn't given (the dock will bounce its icon).
    pub fn request_attention(&mut self, window: &Window) {
        let focused =
            self.keyboard.current_focus().as_ref() == window.toplevel().map(|t| t.wl_surface());
        if !focused {
            Self::with_wm_data(window, |d| d.demands_attention = true);
            self.notify_window_changed(window);
        }
    }

    /// Emit `window_changed` if `window`'s info (ignoring focus) differs from what
    /// subscribers last saw. Quiet while a move or resize of it, or of the parent a sheet
    /// follows, is in progress.
    pub fn notify_window_changed(&mut self, window: &Window) {
        if Self::with_wm_data(window, |d| d.reported.is_none()) {
            return;
        }
        if self.is_sheet(window)
            && self
                .parent_window(window)
                .is_some_and(|p| Self::with_wm_data(&p, |d| d.grabbed))
        {
            return;
        }
        let focus = self.keyboard.current_focus();
        let Some(info) = self.window_info(window, focus.as_ref()) else {
            return;
        };
        let unfocused = WindowInfo {
            focused: false,
            ..info.clone()
        };
        let changed = Self::with_wm_data(window, |d| {
            if d.grabbed || d.reported.as_ref() == Some(&unfocused) {
                false
            } else {
                d.reported = Some(unfocused);
                true
            }
        });
        if changed {
            self.emit(Event::WindowChanged(info));
        }
    }
}
