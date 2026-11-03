//! `wlr-layer-shell`: panels, the dock, the menu bar, wallpapers and launchers (task 1.12,
//! ADR 0015).
//!
//! Smithay's [`LayerMap`] (one per output) does the layout: anchors, margins and exclusive
//! zones. This module maps surfaces into it, sends the first configure, re-arranges on
//! every commit, refits zoomed windows when a panel changes the work area, and decides who
//! gets keyboard focus:
//!
//! - A surface on `top` or `overlay` with `exclusive` interactivity takes the keyboard
//!   while it has a buffer, and windows cannot take it back until it goes away.
//! - `on_demand` surfaces (and `exclusive` ones on `bottom`/`background`) get it when
//!   clicked.
//! - Layer popups (`xdg_popup` with a layer parent) are tracked like window popups.
//!
//! Rendering and pointer hit-testing live in `render.rs` and [`State::surface_under`].

use smithay::desktop::{
    LayerSurface as DesktopLayer, PopupKind, WindowSurfaceType, layer_map_for_output,
};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::shell::wlr_layer::{
    KeyboardInteractivity, Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler,
    WlrLayerShellState,
};
use smithay::wayland::shell::xdg::PopupSurface;
use tracing::{debug, warn};

use crate::state::State;

/// Layers drawn above windows, topmost first.
pub const ABOVE_WINDOWS: [Layer; 2] = [Layer::Overlay, Layer::Top];
/// Layers drawn below windows, topmost first.
pub const BELOW_WINDOWS: [Layer; 2] = [Layer::Bottom, Layer::Background];

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        // The client's choice, else the output under the pointer.
        let output = output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.active_output());
        let Some(output) = output else {
            // No output to put it on. The client may try again after a hotplug.
            surface.send_close();
            return;
        };
        debug!(namespace, output = output.name(), "new layer surface");
        let layer = DesktopLayer::new(surface, namespace);
        if let Err(err) = layer_map_for_output(&output).map_layer(&layer) {
            warn!(%err, "mapping a layer surface failed");
        }
    }

    fn new_popup(&mut self, _parent: WlrLayerSurface, popup: PopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::Xdg(popup)) {
            warn!(?err, "failed to track a layer popup");
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let was_focused = self.keyboard.current_focus().as_ref() == Some(surface.wl_surface());
        for output in self.outputs().cloned().collect::<Vec<_>>() {
            let mut map = layer_map_for_output(&output);
            let layer = map
                .layer_for_surface(surface.wl_surface(), WindowSurfaceType::TOPLEVEL)
                .cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
                drop(map);
                self.fit_windows_to_outputs();
                self.layers_changed(was_focused);
                return;
            }
        }
    }
}

impl State {
    /// The layer surface (and its output) that `surface`, or a popup or subsurface of it,
    /// belongs to.
    pub fn layer_for_surface(&self, surface: &WlSurface) -> Option<(DesktopLayer, Output)> {
        self.outputs().find_map(|output| {
            let map = layer_map_for_output(output);
            let layer = map.layer_for_surface(surface, WindowSurfaceType::ALL)?;
            Some((layer.clone(), output.clone()))
        })
    }

    /// The surface of a layer in `layers` under `pos` (global logical coordinates) and its
    /// origin, trying `layers` in order.
    pub fn layer_surface_under(
        &self,
        pos: Point<f64, Logical>,
        layers: &[Layer],
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output = self.space.output_under(pos).next()?;
        let output_loc = self.space.output_geometry(output)?.loc;
        let map = layer_map_for_output(output);
        let local = pos - output_loc.to_f64();
        layers.iter().find_map(|&layer| {
            let surface = map.layer_under(layer, local)?;
            let geo = map.layer_geometry(surface)?;
            let (wl_surface, offset) =
                surface.surface_under(local - geo.loc.to_f64(), WindowSurfaceType::ALL)?;
            Some((wl_surface, (offset + geo.loc + output_loc).to_f64()))
        })
    }

    /// Whether `surface` has a buffer, i.e. is mapped.
    fn has_buffer(surface: &WlSurface) -> bool {
        smithay::wayland::compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<smithay::backend::renderer::utils::RendererSurfaceStateUserData>()
                .is_some_and(|data| data.lock().is_ok_and(|d| d.buffer().is_some()))
        })
    }

    /// The `exclusive` surface that has the keyboard, if any: one on `overlay` or `top`
    /// with a buffer. `overlay` wins, then the surface that was mapped last.
    pub fn exclusive_layer_surface(&self) -> Option<WlSurface> {
        ABOVE_WINDOWS.iter().find_map(|&layer| {
            self.outputs().find_map(|output| {
                let map = layer_map_for_output(output);
                map.layers_on(layer)
                    .rev()
                    .find(|l| {
                        l.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive
                            && Self::has_buffer(l.wl_surface())
                    })
                    .map(|l| l.wl_surface().clone())
            })
        })
    }

    /// Give `surface` (a layer surface) keyboard focus and deactivate every window.
    pub fn focus_layer(&mut self, surface: &WlSurface) {
        if self.is_locked() {
            return;
        }
        for window in self.space.elements() {
            window.set_activated(false);
        }
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, Some(surface.clone()), SERIAL_COUNTER.next_serial());
        self.send_pending_configures();
    }

    /// A layer surface committed: lay the output out again, send the first configure, and
    /// react to a changed work area and keyboard interactivity. Does nothing for other
    /// surfaces.
    pub(crate) fn commit_layer(&mut self, surface: &WlSurface) {
        let Some((layer, output)) = self.layer_for_surface(surface) else {
            return;
        };
        if layer.wl_surface() != surface {
            return; // a popup or subsurface
        }
        let before = layer_map_for_output(&output).non_exclusive_zone();
        let initial_configure_sent = smithay::wayland::compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<std::sync::Mutex<smithay::wayland::shell::wlr_layer::LayerSurfaceAttributes>>()
                .is_some_and(|data| data.lock().is_ok_and(|d| d.initial_configure_sent))
        });
        let after = {
            let mut map = layer_map_for_output(&output);
            // Arrange first, so the first configure respects the size the client asked for.
            map.arrange();
            if !initial_configure_sent {
                layer.layer_surface().send_configure();
            }
            map.non_exclusive_zone()
        };
        if before != after {
            self.fit_windows_to_outputs();
        }
        self.layers_changed(false);
    }

    /// Re-decide keyboard focus after a layer surface changed or went away.
    /// `lost_focus`: the surface that had the keyboard is gone.
    fn layers_changed(&mut self, lost_focus: bool) {
        if self.is_locked() {
            return;
        }
        if let Some(exclusive) = self.exclusive_layer_surface() {
            if self.keyboard.current_focus().as_ref() != Some(&exclusive) {
                self.focus_layer(&exclusive);
            }
        } else if lost_focus {
            self.focus_topmost();
        } else if let Some(focus) = self.keyboard.current_focus()
            && let Some((layer, _)) = self.layer_for_surface(&focus)
            && !layer.can_receive_keyboard_focus()
        {
            // It switched its interactivity off while it had the keyboard.
            self.focus_topmost();
        }
        self.report_focus(self.focused_window_id());
    }

    /// A press at the pointer's position landed on a layer surface. Returns whether the
    /// click is used up: it focused the surface, or the surface is above the windows and
    /// takes clicks without focus. A press on a `bottom` or `background` surface with
    /// nothing in front falls through (the desktop).
    pub(crate) fn click_layer(&mut self, surface: &WlSurface) -> bool {
        let Some((layer, _)) = self.layer_for_surface(surface) else {
            return false;
        };
        let above = ABOVE_WINDOWS.contains(&layer.layer());
        if layer.can_receive_keyboard_focus() {
            if self.exclusive_layer_surface().is_none() {
                self.focus_layer(layer.wl_surface());
                self.report_focus(self.focused_window_id());
            }
            return true;
        }
        above
    }

    /// Frame callbacks for the layer surfaces of `output`.
    pub fn send_layer_frames(output: &Output, time: std::time::Duration) {
        let map = layer_map_for_output(output);
        for layer in map.layers() {
            layer.send_frame(output, time, Some(std::time::Duration::ZERO), |_, _| {
                Some(output.clone())
            });
        }
    }
}
