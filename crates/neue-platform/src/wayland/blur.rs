//! Backdrop blur through `ext-background-effect-v1` (task 3.5, ADR 0072).
//!
//! A window that asks for blur ([`crate::WindowSpec::blur`]) gets an effect object on its
//! surface; its blur region is the whole window, minus the rounded corners, which a region
//! of rectangles approximates one pixel row at a time. The region is double-buffered and
//! applies on the next commit, which the next frame's present makes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use smithay_client_toolkit::compositor::{CompositorState, Region};
use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, QueueHandle, WEnum};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, Capability, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use super::{NoEvents, State};

/// The manager and whether it currently offers blur.
pub(super) struct BlurManager {
    manager: ExtBackgroundEffectManagerV1,
    available: Arc<AtomicBool>,
}

/// User data of the manager: where the capabilities event records whether blur is offered.
pub(super) struct Capabilities(Arc<AtomicBool>);

impl BlurManager {
    pub(super) fn new(manager: ExtBackgroundEffectManagerV1, available: Arc<AtomicBool>) -> Self {
        Self { manager, available }
    }

    pub(super) fn available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    /// An effect object for `surface`, if blur is on offer.
    pub(super) fn effect(
        &self,
        surface: &WlSurface,
        qh: &QueueHandle<State>,
    ) -> Option<ExtBackgroundEffectSurfaceV1> {
        self.available()
            .then(|| self.manager.get_background_effect(surface, qh, NoEvents))
    }
}

impl Capabilities {
    pub(super) fn new(available: &Arc<AtomicBool>) -> Self {
        Self(Arc::clone(available))
    }
}

impl Dispatch2<ExtBackgroundEffectManagerV1, State> for Capabilities {
    fn event(
        &self,
        _: &mut State,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let ext_background_effect_manager_v1::Event::Capabilities { flags } = event else {
            return;
        };
        let blur = match flags {
            WEnum::Value(flags) => flags.contains(Capability::Blur),
            WEnum::Unknown(bits) => bits & Capability::Blur.bits() != 0,
        };
        tracing::debug!(blur, "background effect capabilities");
        self.0.store(blur, Ordering::Relaxed);
    }
}

/// A window's effect object and the corner radius its region leaves out.
pub(super) struct WindowBlur {
    effect: ExtBackgroundEffectSurfaceV1,
    corner_radius: f64,
}

impl WindowBlur {
    pub(super) fn new(effect: ExtBackgroundEffectSurfaceV1, corner_radius: f64) -> Self {
        Self {
            effect,
            corner_radius,
        }
    }

    /// Blurs the whole window of logical `size` (applies on the next commit).
    pub(super) fn set_size(&self, compositor: &CompositorState, size: (u32, u32)) {
        let region = match Region::new(compositor) {
            Ok(region) => region,
            Err(err) => {
                tracing::debug!(%err, "no wl_region for the blur");
                return;
            }
        };
        for (x, y, w, h) in region_rects(size, self.corner_radius) {
            region.add(x, y, w, h);
        }
        self.effect.set_blur_region(Some(region.wl_region()));
    }

    pub(super) fn destroy(self) {
        self.effect.destroy();
    }
}

/// Rectangles `(x, y, width, height)` covering a `size` window with corners of `radius`
/// cut round: a full-width body, and a one-row strip per row of each rounded edge.
fn region_rects(size: (u32, u32), radius: f64) -> Vec<(i32, i32, i32, i32)> {
    let (w, h) = (to_i32(size.0), to_i32(size.1));
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    // Rounded up, and never more than half of either side.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a corner radius of a few pixels"
    )]
    let r = (radius.max(0.0).ceil() as i32).min(w / 2).min(h / 2);
    if r == 0 {
        return vec![(0, 0, w, h)];
    }
    let mut rects = Vec::with_capacity(2 * r as usize + 1);
    for row in 0..r {
        // Distance from the arc's centre to the row's middle, and how far the arc has
        // moved in from the edge there.
        let dy = f64::from(r) - (f64::from(row) + 0.5);
        let rf = f64::from(r);
        #[expect(clippy::cast_possible_truncation, reason = "at most the radius")]
        let inset = (rf - (rf * rf - dy * dy).max(0.0).sqrt()).round() as i32;
        let width = w - 2 * inset;
        if width <= 0 {
            continue;
        }
        rects.push((inset, row, width, 1));
        rects.push((inset, h - 1 - row, width, 1));
    }
    if h > 2 * r {
        rects.push((0, r, w, h - 2 * r));
    }
    rects
}

fn to_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rects: &[(i32, i32, i32, i32)]) -> i32 {
        rects.iter().map(|&(_, _, w, h)| w * h).sum()
    }

    #[test]
    fn square_window_is_one_rect() {
        assert_eq!(region_rects((100, 40), 0.0), vec![(0, 0, 100, 40)]);
    }

    #[test]
    fn empty_window_has_no_region() {
        assert!(region_rects((0, 40), 5.0).is_empty());
    }

    #[test]
    fn rounded_corners_are_cut() {
        let rects = region_rects((100, 40), 6.0);
        let total = area(&rects);
        assert!(total < 100 * 40, "corners are left out");
        // The corner pixel is out, the middle of the top edge is in.
        let covers = |x: i32, y: i32| {
            rects
                .iter()
                .any(|&(rx, ry, w, h)| x >= rx && x < rx + w && y >= ry && y < ry + h)
        };
        assert!(!covers(0, 0) && !covers(99, 39) && !covers(0, 39) && !covers(99, 0));
        assert!(covers(50, 0) && covers(0, 20));
        // A 6 px corner cuts about (1 - pi/4) * 36 = 7.7 px, four times.
        let cut = 100 * 40 - total;
        assert!((20..=40).contains(&cut), "cut {cut}");
    }

    #[test]
    fn radius_is_limited_to_half_the_window() {
        let rects = region_rects((10, 10), 50.0);
        assert!(area(&rects) > 0);
        assert!(
            rects
                .iter()
                .all(|&(x, y, w, h)| x >= 0 && y >= 0 && x + w <= 10 && y + h <= 10)
        );
    }
}
