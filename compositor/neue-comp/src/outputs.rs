//! Outputs: layout, hotplug, per-output scale and frame callbacks (task 1.7, ADR 0010).
//!
//! Backends create [`Output`]s and hand them to [`State::add_output`] and
//! [`State::remove_output`], and call [`State::output_mode_changed`] when a mode changes
//! (a monitor's preferred mode, or the host window of the nested backend). Everything
//! else lives here, the same for every backend:
//!
//! - **Layout:** outputs with `[outputs."NAME"] position` go there; the others are placed
//!   left to right after them, in connection order, top edges aligned ([`layout`]).
//! - **Scale:** `[outputs."NAME"] scale`, else 1. Surfaces get the preferred scale of the
//!   output they overlap most (fractional-scale-v1, and `wl_surface.preferred_buffer_scale`
//!   rounded up), refreshed once per loop iteration ([`State::refresh_surface_scales`]).
//! - **Hotplug:** a removed output's windows move to the first remaining output at the same
//!   offset, clamped into its work area; zoomed windows are refitted when their output
//!   changes size; windows left on no output are pulled onto the first one.
//! - **Frame callbacks** go to each window from the output it overlaps most, so a window is
//!   paced by one display ([`send_frames`]).
//! - **IPC:** `output_added`, `output_changed` and `output_removed` (topic `outputs`) are
//!   derived by comparing with what was last reported ([`State::report_outputs`]).

use std::time::Duration;

use neue_ipc::{Event, OutputInfo};
use smithay::backend::input::InputTime;
use smithay::desktop::{Space, Window, layer_map_for_output};
use smithay::output::{Output, Scale};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::utils::{Logical, Point, Rectangle, Transform};
use smithay::wayland::compositor::send_surface_state;
use smithay::wayland::fractional_scale::with_fractional_scale;
use tracing::{info, warn};

use crate::input::clamp_to;
use crate::placement;
use crate::state::State;

/// How long a removed output's global stays (disabled) before it is destroyed, so clients
/// that bind it in the meantime don't hit a protocol error.
const GLOBAL_REMOVAL_DELAY: Duration = Duration::from_secs(10);

/// An output in the layout and its `wl_output` global.
#[derive(Debug)]
pub struct LaidOutOutput {
    pub output: Output,
    global: GlobalId,
}

/// One output for [`layout`]: its logical size and configured position, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutItem {
    pub size: (i32, i32),
    pub position: Option<(i32, i32)>,
}

/// Top-left corners for `items`, in the same order. Configured positions are used as
/// given (overlaps are the user's choice). Every other output goes to the right of all
/// outputs placed so far, its top edge on the topmost one's; the first lands at `(0, 0)`.
pub fn layout(items: &[LayoutItem]) -> Vec<(i32, i32)> {
    let mut placed: Vec<Option<(i32, i32)>> = items.iter().map(|i| i.position).collect();
    for index in 0..items.len() {
        if placed[index].is_some() {
            continue;
        }
        let others = || {
            placed
                .iter()
                .zip(items)
                .filter_map(|(pos, item)| pos.map(|p| (p, item.size)))
        };
        let right = others().map(|((x, _), (w, _))| x + w).max();
        let top = others().map(|((_, y), _)| y).min();
        placed[index] = Some((right.unwrap_or(0), top.unwrap_or(0)));
    }
    placed.into_iter().map(Option::unwrap_or_default).collect()
}

/// The output whose geometry overlaps `window` most. `None` if it is on no output.
pub fn primary_output(space: &Space<Window>, window: &Window) -> Option<Output> {
    let bbox = space.element_bbox(window)?;
    space
        .outputs()
        .filter_map(|output| {
            let overlap = space.output_geometry(output)?.intersection(bbox)?;
            Some((
                output,
                i64::from(overlap.size.w) * i64::from(overlap.size.h),
            ))
        })
        .max_by_key(|&(_, area)| area)
        .map(|(output, _)| output.clone())
}

/// Send frame callbacks to the windows `output` paces: those it overlaps most, plus (if it
/// is the first output) those on no output, so off-screen clients don't stall.
pub fn send_frames(space: &Space<Window>, output: &Output, time: Duration) {
    State::send_layer_frames(output, time);
    crate::lock::send_lock_frames(output, time);
    let first = space.outputs().next() == Some(output);
    for window in space.elements() {
        let primary = primary_output(space, window);
        if primary.as_ref() == Some(output) || (primary.is_none() && first) {
            window.send_frame(output, time, Some(Duration::ZERO), |_, _| {
                Some(output.clone())
            });
        }
    }
}

/// Logical size of `output` at its current mode, scale and transform (as [`Space`] computes
/// it).
fn logical_size(output: &Output) -> (i32, i32) {
    let Some(mode) = output.current_mode() else {
        return (0, 0);
    };
    let size = output
        .current_transform()
        .transform_size(mode.size)
        .to_f64()
        .to_logical(output.current_scale().fractional_scale())
        .to_i32_ceil();
    (size.w, size.h)
}

/// `point` moved onto the nearest of `rects` (unchanged if it is on one already).
pub fn clamp_to_nearest(
    point: Point<f64, Logical>,
    rects: impl IntoIterator<Item = Rectangle<i32, Logical>>,
) -> Option<Point<f64, Logical>> {
    let distance = |p: Point<f64, Logical>| (p.x - point.x).powi(2) + (p.y - point.y).powi(2);
    rects
        .into_iter()
        .map(|rect| clamp_to(point, rect))
        .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
}

impl State {
    /// The outputs in the layout, in connection order.
    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.outputs.iter().map(|o| &o.output)
    }

    /// The output called `name`.
    pub fn output_by_name(&self, name: &str) -> Option<Output> {
        self.outputs().find(|o| o.name() == name).cloned()
    }

    /// The output containing `point`, else the nearest one.
    pub fn output_near(&self, point: Point<i32, Logical>) -> Option<Output> {
        let point = point.to_f64();
        let distance = |o: &Output| {
            let geo = self.space.output_geometry(o)?;
            let p = clamp_to(point, geo);
            Some((p.x - point.x).powi(2) + (p.y - point.y).powi(2))
        };
        self.outputs()
            .filter_map(|o| Some((o, distance(o)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(o, _)| o.clone())
    }

    /// Advertise `output`, apply its config and add it to the layout.
    pub fn add_output(&mut self, output: Output) {
        info!(output = output.name(), "output added");
        let global = output.create_global::<State>(&self.display_handle);
        self.outputs.push(LaidOutOutput {
            output: output.clone(),
            global,
        });
        self.apply_output_scale(&output);
        self.lock_new_output(&output);
        self.arrange_outputs();
        self.fit_windows_to_outputs();
        self.report_outputs();
    }

    /// Take `output` out of the layout and move its windows to the remaining outputs.
    pub fn remove_output(&mut self, output: &Output) {
        let Some(index) = self.outputs.iter().position(|o| &o.output == output) else {
            return;
        };
        info!(output = output.name(), "output removed");
        let old_geometry = self.space.output_geometry(output);
        let stranded: Vec<Window> = self
            .space
            .elements()
            .filter(|w| primary_output(&self.space, w).as_ref() == Some(output))
            .cloned()
            .collect();
        // Its layer surfaces have no output left; their clients get `closed`.
        for layer in layer_map_for_output(output).layers() {
            layer.layer_surface().send_close();
        }
        let removed = self.outputs.remove(index);
        self.space.unmap_output(output);
        self.retire_global(removed.global);
        self.arrange_outputs();

        // Same offset on the first remaining output, clamped into its work area.
        let target = self.outputs().next().cloned();
        if let (Some(old), Some(target)) = (old_geometry, target) {
            let target_work = self.work_area(&target);
            for window in stranded {
                let work = self.content_area(&window, target_work);
                if Self::with_wm_data(&window, |d| d.zoomed) {
                    continue; // refitted below
                }
                let Some(geo) = self.space.element_geometry(&window) else {
                    continue;
                };
                let offset = geo.loc - old.loc;
                let pos = work.loc + offset;
                let loc = placement::constrain(
                    (work.loc.x, work.loc.y, work.size.w, work.size.h),
                    (geo.size.w, geo.size.h),
                    (pos.x, pos.y),
                );
                self.place_window(&window, loc.into());
            }
        }
        self.fit_windows_to_outputs();
        self.warp_pointer_onto_outputs();
        self.report_outputs();
    }

    /// `output`'s mode changed: lay out again and refit windows.
    pub fn output_mode_changed(&mut self, output: &Output) {
        info!(output = output.name(), mode = ?output.current_mode(), "output mode changed");
        self.arrange_outputs();
        self.fit_windows_to_outputs();
        self.warp_pointer_onto_outputs();
        self.report_outputs();
    }

    /// Apply `[outputs]` from the active config to every output (after a reload).
    pub(crate) fn apply_output_config(&mut self) {
        for output in self.outputs().cloned().collect::<Vec<_>>() {
            self.apply_output_scale(&output);
        }
        self.arrange_outputs();
        self.fit_windows_to_outputs();
        self.warp_pointer_onto_outputs();
        self.report_outputs();
    }

    /// Set `output`'s scale from the config (1 if unset).
    fn apply_output_scale(&mut self, output: &Output) {
        let name = output.name();
        let scale = self
            .config
            .active
            .outputs
            .get(&name)
            .and_then(|o| o.scale)
            .unwrap_or(1.0);
        if output.current_scale().fractional_scale() != scale {
            output.change_current_state(None, None, Some(Scale::Fractional(scale)), None);
            info!(output = name, scale, "output scale set");
        }
    }

    /// Compute every output's position and (re)map it into the space.
    fn arrange_outputs(&mut self) {
        let items: Vec<LayoutItem> = self
            .outputs()
            .map(|output| LayoutItem {
                size: logical_size(output),
                position: self
                    .config
                    .active
                    .outputs
                    .get(&output.name())
                    .and_then(|o| o.position)
                    .map(|[x, y]| (x, y)),
            })
            .collect();
        let positions = layout(&items);
        let outputs: Vec<Output> = self.outputs().cloned().collect();
        for (output, (x, y)) in outputs.iter().zip(positions) {
            let location = Point::from((x, y));
            if self.space.output_geometry(output).map(|g| g.loc) != Some(location) {
                output.change_current_state(None, None, None, Some(location));
            }
            // Also refreshes the space's idea of the output's size.
            self.space.map_output(output, location);
            // The layer map caches the output size its work area is computed from.
            layer_map_for_output(output).arrange();
        }
    }

    /// Refit zoomed windows to their output's work area and pull windows that are on no
    /// output onto the first one.
    pub(crate) fn fit_windows_to_outputs(&mut self) {
        let Some(first) = self.outputs().next().cloned() else {
            return;
        };
        let windows: Vec<Window> = self.space.elements().cloned().collect();
        for window in windows {
            let primary = primary_output(&self.space, &window);
            if Self::with_wm_data(&window, |d| d.zoomed) {
                let work =
                    self.content_area(&window, self.work_area(primary.as_ref().unwrap_or(&first)));
                let Some(toplevel) = window.toplevel() else {
                    continue;
                };
                let current = self.space.element_geometry(&window);
                if current != Some(work) {
                    toplevel.with_pending_state(|s| s.size = Some(work.size));
                    if toplevel.is_initial_configure_sent() {
                        toplevel.send_pending_configure();
                    }
                    self.place_window(&window, work.loc);
                }
            } else if primary.is_none()
                && let Some(geo) = self.space.element_geometry(&window)
            {
                let work = self.content_area(&window, self.work_area(&first));
                let loc = placement::constrain(
                    (work.loc.x, work.loc.y, work.size.w, work.size.h),
                    (geo.size.w, geo.size.h),
                    (geo.loc.x, geo.loc.y),
                );
                self.place_window(&window, loc.into());
            }
        }
        // Minimised windows come back where they were; keep that somewhere visible.
        let areas: Vec<Rectangle<i32, Logical>> =
            self.outputs().map(|o| self.work_area(o)).collect();
        let first_work = self.work_area(&first);
        for (window, loc) in &mut self.minimized {
            let size = window.geometry().size;
            let rect = Rectangle::new(*loc, size);
            if !areas.iter().any(|a| a.overlaps(rect)) {
                let bar = crate::decorations::title_bar_height(window).min(first_work.size.h);
                let mut w = first_work;
                w.loc.y += bar;
                w.size.h -= bar;
                *loc = placement::constrain(
                    (w.loc.x, w.loc.y, w.size.w, w.size.h),
                    (size.w, size.h),
                    (loc.x, loc.y),
                )
                .into();
            }
        }
    }

    /// Move a mapped window's geometry to `location` and report the change.
    fn place_window(&mut self, window: &Window, location: Point<i32, Logical>) {
        // The space keeps a window's geometry origin as its location.
        self.space.map_element(window.clone(), location, false);
        self.notify_window_changed(window);
    }

    /// If the pointer is on no output (its output went away), put it in the middle of the
    /// first one.
    fn warp_pointer_onto_outputs(&mut self) {
        let pos = self.pointer.current_location();
        if self.space.output_under(pos).next().is_some() {
            return;
        }
        let Some(geo) = self
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };
        let centre = geo.loc.to_f64() + geo.size.to_f64().downscale(2.0).to_point();
        self.move_pointer(centre, InputTime::now());
    }

    /// Disable a removed output's global now and destroy it after a delay.
    fn retire_global(&mut self, global: GlobalId) {
        self.display_handle.disable_global::<State>(global.clone());
        let timer = Timer::from_duration(GLOBAL_REMOVAL_DELAY);
        let result = self.loop_handle.insert_source(timer, move |_, _, state| {
            state.display_handle.remove_global::<State>(global.clone());
            TimeoutAction::Drop
        });
        if let Err(err) = result {
            warn!(err = %err.error, "cannot schedule removing an output global");
        }
    }

    /// `output` as `neuectl outputs` shows it.
    pub fn output_info(&self, output: &Output) -> OutputInfo {
        let mode = output.current_mode();
        let location = self
            .space
            .output_geometry(output)
            .map_or_else(|| output.current_location(), |g| g.loc);
        let props = output.physical_properties();
        OutputInfo {
            name: output.name(),
            make: props.make,
            model: props.model,
            x: location.x,
            y: location.y,
            width: mode.map_or(0, |m| m.size.w),
            height: mode.map_or(0, |m| m.size.h),
            refresh_mhz: mode.map_or(0, |m| m.refresh),
            scale: output.current_scale().fractional_scale(),
        }
    }

    /// Emit `output_added`, `output_changed` and `output_removed` for what differs from the
    /// last report.
    fn report_outputs(&mut self) {
        let current: Vec<OutputInfo> = self.outputs().map(|o| self.output_info(o)).collect();
        let previous = std::mem::replace(&mut self.reported_outputs, current.clone());
        for old in &previous {
            if !current.iter().any(|c| c.name == old.name) {
                self.emit(Event::OutputRemoved {
                    name: old.name.clone(),
                });
            }
        }
        for info in current {
            match previous.iter().find(|p| p.name == info.name) {
                None => self.emit(Event::OutputAdded(info)),
                Some(old) if *old != info => self.emit(Event::OutputChanged(info)),
                Some(_) => {}
            }
        }
    }

    /// Tell every surface the scale of the output it is (mostly) on: fractional-scale-v1
    /// and `wl_surface.preferred_buffer_scale`. Windows not yet mapped get the scale of the
    /// output they will appear on. Only changes are sent. Called once per loop iteration.
    pub fn refresh_surface_scales(&self) {
        let fallback = self.active_output();
        for window in self.space.elements() {
            if let Some(output) = primary_output(&self.space, window).or_else(|| fallback.clone()) {
                send_scale(window, &output);
            }
        }
        if let Some(output) = &fallback {
            for window in &self.unmapped {
                send_scale(window, output);
            }
        }
    }
}

/// Send `output`'s scale to every surface of `window`.
fn send_scale(window: &Window, output: &Output) {
    let scale = output.current_scale();
    window.with_surfaces(|surface, states| {
        // Buffer transforms only pay off with direct scanout (task 1.15).
        send_surface_state(surface, states, scale.integer_scale(), Transform::Normal);
        with_fractional_scale(states, |fs| {
            fs.set_preferred_scale(scale.fractional_scale());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(size: (i32, i32), position: Option<(i32, i32)>) -> LayoutItem {
        LayoutItem { size, position }
    }

    #[test]
    fn unconfigured_outputs_go_left_to_right() {
        let items = [item((1920, 1080), None), item((1280, 720), None)];
        assert_eq!(layout(&items), [(0, 0), (1920, 0)]);
    }

    #[test]
    fn configured_outputs_stay_and_others_follow_them() {
        let items = [
            item((1920, 1080), None),
            item((2560, 1440), Some((-2560, -200))),
            item((1280, 1024), Some((0, 0))),
        ];
        assert_eq!(layout(&items), [(1280, -200), (-2560, -200), (0, 0)]);
    }

    #[test]
    fn a_single_output_is_at_the_origin() {
        assert_eq!(layout(&[item((1280, 853), None)]), [(0, 0)]);
        assert!(layout(&[]).is_empty());
    }

    #[test]
    fn points_clamp_to_the_nearest_output() {
        let a = Rectangle::new((0, 0).into(), (1920, 1080).into());
        let b = Rectangle::new((1920, 0).into(), (1280, 720).into());
        let p = |x: f64, y: f64| Point::<f64, Logical>::from((x, y));
        // Just below the shorter output: back onto its bottom row.
        assert_eq!(
            clamp_to_nearest(p(2000.0, 730.0), [a, b]),
            Some(p(2000.0, 719.0))
        );
        // Deep in the gap, nearer the taller output's edge: onto that one.
        assert_eq!(
            clamp_to_nearest(p(2000.0, 900.0), [a, b]),
            Some(p(1919.0, 900.0))
        );
        // On an output: unchanged.
        assert_eq!(
            clamp_to_nearest(p(100.5, 50.0), [a, b]),
            Some(p(100.5, 50.0))
        );
        assert_eq!(clamp_to_nearest(p(-10.0, -10.0), [a, b]), Some(p(0.0, 0.0)));
        assert_eq!(clamp_to_nearest(p(0.0, 0.0), []), None);
    }
}
