//! `ext_background_effect_manager_v1` on the test client (task 3.4, ADR 0071).

use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};

use super::{ClientData, TestClient};

#[derive(Default)]
pub(super) struct BlurData {
    manager: Option<ExtBackgroundEffectManagerV1>,
    /// The last `capabilities` event's flags.
    capabilities: Option<u32>,
}

impl TestClient {
    fn effect_manager(&mut self) -> ExtBackgroundEffectManagerV1 {
        if let Some(manager) = &self.data.blur.manager {
            return manager.clone();
        }
        let manager: ExtBackgroundEffectManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding ext_background_effect_manager_v1");
        self.data.blur.manager = Some(manager.clone());
        self.roundtrip();
        manager
    }

    /// The capability flags the manager announced on bind.
    pub fn background_effect_capabilities(&mut self) -> Option<u32> {
        self.effect_manager();
        self.data.blur.capabilities
    }

    /// `get_background_effect` for `surface`.
    pub fn background_effect(&mut self, surface: &WlSurface) -> ExtBackgroundEffectSurfaceV1 {
        let manager = self.effect_manager();
        manager.get_background_effect(surface, &self.qh, ())
    }

    /// Set `effect`'s blur region to the `rects` (x, y, width, height; `None`: no blur)
    /// and commit `surface`.
    pub fn set_blur_region(
        &mut self,
        effect: &ExtBackgroundEffectSurfaceV1,
        surface: &WlSurface,
        rects: Option<&[(i32, i32, i32, i32)]>,
    ) {
        let region: Option<WlRegion> = rects.map(|rects| {
            let region = self.compositor.create_region(&self.qh, ());
            for &(x, y, w, h) in rects {
                region.add(x, y, w, h);
            }
            region
        });
        effect.set_blur_region(region.as_ref());
        if let Some(region) = region {
            region.destroy();
        }
        surface.commit();
        self.roundtrip();
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            data.blur.capabilities = Some(match flags {
                WEnum::Value(flags) => flags.bits(),
                WEnum::Unknown(bits) => bits,
            });
        }
    }
}

delegate_noop!(ClientData: ignore ExtBackgroundEffectSurfaceV1);
