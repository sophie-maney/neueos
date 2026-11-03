//! CPU backend: replays a [`Scene`] with `vello_cpu` into a [`Pixmap`].
//!
//! Used for golden tests and, later, compositor chrome.

use std::collections::HashMap;
use std::sync::Arc;

use vello_common::fearless_simd::Level;
use vello_common::paint::ImageSource;
use vello_common::pixmap::Pixmap;
use vello_cpu::{RasterizerSettings, RenderContext, RenderMode, RenderSettings, Resources};

use crate::scene::{Scene, TextureBrush};
use crate::{Error, device_size};

/// Speed/accuracy trade-off for the CPU backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    /// u8 pipeline with the best SIMD level the CPU offers. For on-screen rendering.
    #[default]
    Fast,
    /// f32 pipeline with the baseline SIMD level of the target architecture. Slower, but as
    /// reproducible across machines as vello allows. Golden tests use this.
    Reference,
}

/// Renders [`Scene`]s on the CPU. Keeps procedural textures cached between renders.
#[derive(Debug, Default)]
pub struct CpuRenderer {
    quality: Quality,
    resources: Resources,
    textures: HashMap<(u64, u64), Arc<Pixmap>>,
}

impl CpuRenderer {
    /// A renderer with the given quality.
    pub fn new(quality: Quality) -> Self {
        Self {
            quality,
            ..Self::default()
        }
    }

    /// Render `scene` at `scale` (device pixels per logical pixel) onto a transparent
    /// pixmap of `ceil(size * scale)` device pixels.
    pub fn render(&mut self, scene: &Scene, scale: f64) -> Result<Pixmap, Error> {
        let (width, height) = device_size(scene, scale)?;
        let (settings, mode) = match self.quality {
            Quality::Fast => (RenderSettings::default(), RenderMode::OptimizeSpeed),
            Quality::Reference => (
                RenderSettings {
                    level: Level::baseline(),
                    num_threads: 0,
                },
                RenderMode::OptimizeQuality,
            ),
        };
        let mut ctx = RenderContext::new_with(width, height, settings);
        let textures = &mut self.textures;
        let mut resolve = |brush: &TextureBrush| {
            let key = (brush.texture.cache_key(), scale.to_bits());
            let pixmap = textures
                .entry(key)
                .or_insert_with(|| Arc::new(brush.texture.render(scale)));
            ImageSource::Pixmap(Arc::clone(pixmap))
        };
        crate::replay::replay!(&mut ctx, scene, scale, &mut resolve);

        let mut target = Pixmap::new(width, height);
        ctx.flush();
        ctx.render_with(
            &mut target,
            &mut self.resources,
            RasterizerSettings {
                render_mode: mode,
                ..RasterizerSettings::default()
            },
        );
        Ok(target)
    }
}
