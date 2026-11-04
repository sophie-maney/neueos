//! Third replay target: an `imaging::Painter`, the paint API Masonry widgets draw through
//! (ADR 0002 §4).
//!
//! Masonry records widget paint in logical pixels and applies the window scale when it
//! renders, so commands are replayed without a root scale. Procedural textures are still
//! generated at the device scale (`PaintCtx::scale_factor`) so they stay sharp.

use std::collections::HashMap;

use imaging::record::Glyph;
use imaging::{BlurredRoundedRect, Brush, ClipRef, Composite, GroupRef, ImageBrush, Painter};
use vello_common::kurbo::Affine;
use vello_common::peniko::{BlendMode, Blob, Fill, ImageAlphaType, ImageData, ImageFormat, Style};

use crate::replay::{TEXTURE_SAMPLER, texture_paint_transform};
use crate::scene::{self, Command, Scene, TextureBrush};

/// Replays [`Scene`]s into `imaging` painters. Keeps procedural textures cached between
/// replays, so keep one per window (or per widget tree) and reuse it.
#[derive(Debug, Default)]
pub struct ImagingReplayer {
    /// Texture tiles per `(cache_key, scale bits)`. `ImageData` shares its pixels, and the
    /// stable blob id lets backends cache the upload too.
    textures: HashMap<(u64, u64), ImageData>,
}

impl ImagingReplayer {
    /// A replayer with an empty texture cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replay `scene` into `painter`. Coordinates stay logical; `scale` is the device scale
    /// the result will be rendered at, used only to generate textures.
    pub fn replay<S>(&mut self, scene: &Scene, scale: f64, painter: &mut Painter<'_, S>)
    where
        S: imaging::PaintSink + ?Sized,
    {
        let glyph_style = Style::Fill(Fill::NonZero);
        let mut open = 0usize;
        for command in scene.commands() {
            match command {
                Command::Fill {
                    transform,
                    path,
                    brush,
                } => {
                    if let Some((brush, brush_transform)) = self.brush(brush, *transform, scale) {
                        painter
                            .fill(path, &brush)
                            .transform(*transform)
                            .brush_transform(brush_transform)
                            .draw();
                    }
                }
                Command::Stroke {
                    transform,
                    path,
                    style,
                    brush,
                } => {
                    if let Some((brush, brush_transform)) = self.brush(brush, *transform, scale) {
                        painter
                            .stroke(path, style, &brush)
                            .transform(*transform)
                            .brush_transform(brush_transform)
                            .draw();
                    }
                }
                Command::BlurredRoundedRect {
                    transform,
                    rect,
                    radius,
                    std_dev,
                    color,
                } => painter.blurred_rounded_rect(BlurredRoundedRect {
                    transform: *transform,
                    rect: *rect,
                    color: *color,
                    radius: *radius,
                    std_dev: *std_dev,
                    composite: Composite::default(),
                }),
                Command::PushLayer {
                    transform,
                    clip,
                    opacity,
                } => {
                    let mut group = GroupRef::new()
                        .with_composite(Composite::new(BlendMode::default(), *opacity));
                    if let Some(clip) = clip {
                        group = group.with_clip(ClipRef::fill(clip).with_transform(*transform));
                    }
                    painter.push_group(group);
                    open += 1;
                }
                Command::PopLayer => {
                    // `Scene` never records an unbalanced pop, but a hand-built command
                    // list must not pop a group the caller pushed.
                    if open > 0 {
                        painter.pop_group();
                        open -= 1;
                    }
                }
                Command::Glyphs {
                    transform,
                    font,
                    size,
                    glyphs,
                    color,
                } => painter
                    .glyphs(font, *color)
                    .transform(*transform)
                    .font_size(*size)
                    .hint(true)
                    .draw(
                        &glyph_style,
                        glyphs.iter().map(|g| Glyph {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                        }),
                    ),
            }
        }
        for _ in 0..open {
            painter.pop_group();
        }
    }

    /// The `imaging` brush and brush transform for `brush` under `shape_transform`.
    fn brush(
        &mut self,
        brush: &scene::Brush,
        shape_transform: Affine,
        scale: f64,
    ) -> Option<(Brush, Option<Affine>)> {
        match brush {
            scene::Brush::Solid(color) => Some((Brush::Solid(*color), None)),
            scene::Brush::Gradient(gradient) => Some((Brush::Gradient(gradient.clone()), None)),
            scene::Brush::Texture(texture) => {
                let transform = texture_paint_transform(texture, shape_transform, scale)?;
                let image =
                    ImageBrush::new(self.texture(texture, scale)).with_sampler(TEXTURE_SAMPLER);
                Some((Brush::Image(image), Some(transform)))
            }
        }
    }

    /// The cached tile of `texture` at `scale`, rendering it on first use.
    fn texture(&mut self, texture: &TextureBrush, scale: f64) -> ImageData {
        let key = (texture.texture.cache_key(), scale.to_bits());
        self.textures
            .entry(key)
            .or_insert_with(|| {
                let pixmap = texture.texture.render(scale);
                ImageData {
                    data: Blob::from(pixmap.data_as_u8_slice().to_vec()),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::AlphaPremultiplied,
                    width: u32::from(pixmap.width()),
                    height: u32::from(pixmap.height()),
                }
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use imaging::record;

    use super::*;
    use crate::Appearance;
    use crate::demo;

    #[test]
    fn replays_every_command_and_balances_groups() {
        let scene = demo::sheet(Appearance::Light);
        let mut recorded = record::Scene::new();
        ImagingReplayer::new().replay(&scene, 2.0, &mut Painter::new(&mut recorded));
        assert!(recorded.validate().is_ok());
    }

    #[test]
    fn caches_textures_per_scale() {
        let scene = demo::sheet(Appearance::Dark);
        let mut replayer = ImagingReplayer::new();
        for scale in [1.0, 2.0, 2.0] {
            let mut recorded = record::Scene::new();
            replayer.replay(&scene, scale, &mut Painter::new(&mut recorded));
        }
        assert_eq!(replayer.textures.len(), 2);
    }
}
