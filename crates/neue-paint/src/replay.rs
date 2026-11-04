//! Replays a [`Scene`](crate::Scene) into a vello render context.
//!
//! `vello_cpu::RenderContext` and `vello_gpu::Scene` expose the same imperative methods over
//! the same `vello_common` types but share no trait, so the replay loop is a macro that
//! expands once per backend.

use vello_common::kurbo::Affine;
use vello_common::paint::{Image, ImageSource, PaintType};
use vello_common::peniko::{Extend, ImageQuality, ImageSampler};

use crate::scene::{Brush, TextureBrush};

/// Paint and paint transform for `brush`, drawn with the logical-space shape transform
/// `shape_transform` at device `scale`. Returns `None` when the brush can't cover any
/// pixels (a degenerate shape transform under a texture).
pub(crate) fn paint_for(
    brush: &Brush,
    shape_transform: Affine,
    scale: f64,
    resolve: impl FnOnce(&TextureBrush) -> ImageSource,
) -> Option<(PaintType, Affine)> {
    match brush {
        Brush::Solid(color) => Some((PaintType::Solid(*color), Affine::IDENTITY)),
        Brush::Gradient(gradient) => {
            Some((PaintType::Gradient(gradient.clone()), Affine::IDENTITY))
        }
        Brush::Texture(texture) => {
            let paint_transform = texture_paint_transform(texture, shape_transform, scale)?;
            let image = Image {
                image: resolve(texture),
                sampler: TEXTURE_SAMPLER,
            };
            Some((PaintType::Image(image), paint_transform))
        }
    }
}

/// How texture tiles are sampled: repeated in both directions.
pub(crate) const TEXTURE_SAMPLER: ImageSampler = ImageSampler {
    x_extend: Extend::Repeat,
    y_extend: Extend::Repeat,
    quality: ImageQuality::Medium,
    alpha: 1.0,
};

/// Paint transform of `texture` drawn under the logical-space shape transform
/// `shape_transform` at device `scale`, relative to that shape transform (vello's and
/// imaging's convention). `None` when the shape transform is degenerate.
pub(crate) fn texture_paint_transform(
    texture: &TextureBrush,
    shape_transform: Affine,
    scale: f64,
) -> Option<Affine> {
    if shape_transform.determinant().abs() < f64::EPSILON {
        return None;
    }
    // The tile is rendered at device scale, so one texel is 1/scale logical pixels. The
    // paint transform is relative to the shape's transform; undo it so the texture is
    // anchored in scene space by `texture.transform`.
    Some(shape_transform.inverse() * texture.transform * Affine::scale(1.0 / scale))
}

/// Replay `$scene` into `$ctx` at `$scale`. `$resources` are the backend's glyph and image
/// resources; `$resolve` maps them and a texture brush to an `ImageSource` the backend can
/// sample.
macro_rules! replay {
    ($ctx:expr, $scene:expr, $scale:expr, $resources:expr, $resolve:expr) => {{
        use vello_common::kurbo::Affine;
        use $crate::scene::Command;

        let ctx = $ctx;
        let scale: f64 = $scale;
        let resources = $resources;
        let resolve = $resolve;
        let root = Affine::scale(scale);
        for command in $scene.commands() {
            match command {
                Command::Fill {
                    transform,
                    path,
                    brush,
                } => {
                    if let Some((paint, paint_transform)) =
                        $crate::replay::paint_for(brush, *transform, scale, |b| {
                            resolve(&mut *resources, b)
                        })
                    {
                        ctx.set_transform(root * *transform);
                        ctx.set_paint(paint);
                        ctx.set_paint_transform(paint_transform);
                        ctx.fill_path(path);
                    }
                }
                Command::Stroke {
                    transform,
                    path,
                    style,
                    brush,
                } => {
                    if let Some((paint, paint_transform)) =
                        $crate::replay::paint_for(brush, *transform, scale, |b| {
                            resolve(&mut *resources, b)
                        })
                    {
                        ctx.set_transform(root * *transform);
                        ctx.set_paint(paint);
                        ctx.set_paint_transform(paint_transform);
                        ctx.set_stroke(style.clone());
                        ctx.stroke_path(path);
                    }
                }
                Command::BlurredRoundedRect {
                    transform,
                    rect,
                    radius,
                    std_dev,
                    color,
                } => {
                    ctx.set_transform(root * *transform);
                    ctx.set_paint(*color);
                    ctx.reset_paint_transform();
                    // vello takes f32 radii; logical sizes are small, so this is lossless
                    // in practice.
                    #[allow(clippy::cast_possible_truncation, reason = "f64 → f32 for vello")]
                    ctx.fill_blurred_rounded_rect(rect, *radius as f32, *std_dev as f32, false);
                }
                Command::PushLayer {
                    transform,
                    clip,
                    opacity,
                } => {
                    ctx.set_transform(root * *transform);
                    ctx.push_layer(clip.as_ref(), None, Some(*opacity), None, None);
                }
                Command::PopLayer => ctx.pop_layer(),
                Command::Glyphs {
                    transform,
                    font,
                    size,
                    glyphs,
                    color,
                } => {
                    ctx.set_transform(root * *transform);
                    ctx.set_paint(*color);
                    ctx.reset_paint_transform();
                    // A glyph that fails to render is left blank; the rest of the run is
                    // still drawn, which is the best we can do mid-frame.
                    let _ = ctx
                        .glyph_run(&mut *resources, font)
                        .font_size(*size)
                        .hint(true)
                        .fill_glyphs(glyphs.iter().copied());
                }
            }
        }
        for _ in 0..$scene.open_layers() {
            ctx.pop_layer();
        }
    }};
}

pub(crate) use replay;
