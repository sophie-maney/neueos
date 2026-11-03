//! Procedural, vector-based materials and painting primitives.
//!
//! Drawing is recorded into a [`Scene`] (logical pixels) and replayed by a backend:
//! - [`cpu::CpuRenderer`] (`vello_cpu`), always available, used for golden tests and
//!   compositor chrome;
//! - `gpu::GpuRenderer` (`vello_gpu` on wgpu), behind the `gpu` feature.
//!
//! Materials in [`materials`] draw into a [`Scene`], so both backends share them. Text is
//! laid out by [`text::TextEngine`] (parley, bundled fonts only).
//!
//! Portable crate: must build and pass tests on macOS and Linux.

pub mod cpu;
pub mod demo;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod materials;
mod replay;
mod scene;
pub mod text;

pub use scene::{Brush, Command, ProceduralTexture, Scene, TextureBrush};
pub use vello_common::kurbo;
pub use vello_common::peniko;
pub use vello_common::peniko::color;
pub use vello_common::pixmap::Pixmap;

/// Light or dark appearance. Every material has both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Appearance {
    /// The default, light appearance.
    #[default]
    Light,
    /// The dark appearance.
    Dark,
}

/// Errors from rendering a [`Scene`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The scene's device size is zero, not finite, or larger than vello's 65535 px limit.
    #[error("scene of {width}x{height} logical px at scale {scale} has no valid device size")]
    InvalidSize {
        /// Logical width.
        width: f64,
        /// Logical height.
        height: f64,
        /// Requested scale.
        scale: f64,
    },
    /// The GPU renderer failed.
    #[cfg(feature = "gpu")]
    #[error("GPU render failed: {0:?}")]
    Gpu(vello_gpu::RenderError),
}

/// Device size of `scene` at `scale`, rounded up to whole pixels.
pub(crate) fn device_size(scene: &Scene, scale: f64) -> Result<(u16, u16), Error> {
    let to_px = |v: f64| {
        let px = (v * scale).ceil();
        // Range-checked just above, so the cast cannot truncate.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "range-checked"
        )]
        (px.is_finite() && px >= 1.0 && px <= f64::from(u16::MAX)).then_some(px as u16)
    };
    match (to_px(scene.width()), to_px(scene.height())) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(Error::InvalidSize {
            width: scene.width(),
            height: scene.height(),
            scale,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_size_rounds_up_and_rejects_bad_sizes() {
        assert_eq!(device_size(&Scene::new(10.5, 3.0), 2.0).unwrap(), (21, 6));
        assert_eq!(device_size(&Scene::new(10.2, 3.0), 1.0).unwrap(), (11, 3));
        assert!(device_size(&Scene::new(0.0, 3.0), 1.0).is_err());
        assert!(device_size(&Scene::new(70_000.0, 3.0), 1.0).is_err());
    }
}
