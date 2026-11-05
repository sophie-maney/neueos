//! Surface and role handlers: scale, frame callbacks, configures, close requests, popups.

use masonry_core::core::PointerEvent as MasonryPointer;
use masonry_core::kurbo::Point;
use smithay_client_toolkit::compositor::CompositorHandler;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Connection, QueueHandle};

use super::{AppEvent, State, input};
use crate::PopoverLanded;
use crate::app::Host as _;

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        // Only without fractional scale; otherwise wp_fractional_scale_v1 decides.
        if self.g.fractional.is_some() {
            return;
        }
        if let Some((id, None)) = self.locate(surface) {
            if let Some(w) = self.reg.windows.get_mut(&id) {
                w.surf.int_scale = factor;
            }
            self.set_scale(id, f64::from(factor));
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let Some((id, popup)) = self.locate(surface) else {
            return;
        };
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        let surf = match popup {
            Some(pi) => &mut w.popups[pi].surf,
            None => &mut w.surf,
        };
        surf.frame_pending = false;
        if surf.stale || w.root.needs_anim() {
            w.dirty = true;
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl WindowHandler for State {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some((id, None)) = self.locate(window.wl_surface()) {
            self.events.push(AppEvent::CloseRequested(id));
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let Some((id, None)) = self.locate(window.wl_surface()) else {
            return;
        };
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        let constrained = configure.is_maximized()
            || configure.is_fullscreen()
            || configure.is_tiled()
            || configure.is_resizing();
        w.maximized = configure.is_maximized();
        let size = configured_size(configure.new_size, w.free_size);
        w.server_sized = configure.new_size.0.is_some() || configure.new_size.1.is_some();
        if w.server_sized && !constrained {
            // Back to floating at this size: keep it for the next unmaximise.
            w.free_size = size;
        }
        self.configure_window(id, size);
    }
}

/// The logical size a toplevel configure asks for: the compositor's on each axis it set,
/// ours on the others.
fn configured_size(
    new_size: (Option<std::num::NonZeroU32>, Option<std::num::NonZeroU32>),
    free_size: (u32, u32),
) -> (u32, u32) {
    (
        new_size.0.map_or(free_size.0, |v| v.get()),
        new_size.1.map_or(free_size.1, |v| v.get()),
    )
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        // The compositor destroyed the surface (its output went away): it can't come back.
        if let Some((id, None)) = self.locate(layer.wl_surface()) {
            tracing::info!(%id, "layer surface closed by the compositor");
            self.reg.close_window(id);
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some((id, None)) = self.locate(layer.wl_surface()) else {
            return;
        };
        let Some(w) = self.reg.windows.get(&id) else {
            return;
        };
        // 0: the compositor leaves that axis to us.
        let pick = |v: u32, ours: u32| if v == 0 { ours } else { v };
        let size = (
            pick(configure.new_size.0, w.free_size.0),
            pick(configure.new_size.1, w.free_size.1),
        );
        self.configure_window(id, size);
    }
}

impl PopupHandler for State {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        let Some((id, Some(pi))) = self.locate(popup.wl_surface()) else {
            return;
        };
        let fractional = self.g.fractional.is_some();
        let (Some(gpu), Some(w)) = (&self.gpu, self.reg.windows.get_mut(&id)) else {
            return;
        };
        let origin = Point::new(f64::from(config.position.0), f64::from(config.position.1));
        tracing::debug!(
            ?origin,
            width = config.width,
            height = config.height,
            "popup configured"
        );
        // The position is of the window geometry, relative to the parent's: the window, or
        // the menu it nests on.
        let origin = origin + w.parent_origin(w.popups[pi].parent) - w.popups[pi].geometry();
        let p = &mut w.popups[pi];
        let popover = p.placement.popover;
        if origin != p.origin || popover {
            // The compositor flipped or slid the popup: move the Masonry layer to match.
            // A popover is told where it landed, to point its arrow at its anchor.
            p.origin = origin;
            let (root_id, at) = (p.root_id, origin + p.layer_offset);
            w.root.reposition_layer(root_id, at);
            if popover && w.root.get_widget(root_id).is_some() {
                w.root.edit_widget(root_id, |mut root| {
                    root.insert_prop(PopoverLanded(Some(at)));
                });
            }
        }
        let p = &mut w.popups[pi];
        let reach = 2 * p.placement.reach.max(0);
        p.surf.size = (
            (config.width + reach).max(1) as u32,
            (config.height + reach).max(1) as u32,
        );
        p.surf.apply(gpu, w.scale, fractional);
        p.configured = true;
        // Drawn by the next `draw_dirty`, whether or not the window's frame is pending.
        p.surf.stale = true;
        w.dirty = true;
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        let Some((id, Some(pi))) = self.locate(popup.wl_surface()) else {
            return;
        };
        let Some(w) = self.reg.windows.get_mut(&id) else {
            return;
        };
        tracing::debug!(%id, "popup dismissed by the compositor");
        // Menus nested on this one are dismissed with it.
        let gone = w.take_popups(pi);
        // Masonry's menu layers close themselves on a pointer cancel; whatever is still
        // there after the signals are handled is removed in `pump`.
        w.root
            .handle_pointer_event(MasonryPointer::Cancel(input::MOUSE));
        w.dismissed.extend(gone);
        w.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn configure_size_falls_back_per_axis() {
        let n = |v| NonZeroU32::new(v);
        assert_eq!(configured_size((None, None), (440, 360)), (440, 360));
        assert_eq!(configured_size((n(800), n(600)), (440, 360)), (800, 600));
        assert_eq!(configured_size((n(800), None), (440, 360)), (800, 360));
    }
}
