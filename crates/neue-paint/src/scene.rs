//! A small recorded command list that every backend replays.
//!
//! `vello_cpu` and `vello_gpu` have near-identical but separate APIs and no shared scene
//! type, so `neue-paint` records drawing into its own [`Scene`] and replays it per backend
//! (see `docs/research/vello-cpu.md`). Coordinates are logical pixels; the backend applies
//! the output scale when it replays.

use std::fmt;
use std::sync::Arc;

use vello_common::kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use vello_common::peniko::{Color, Gradient};
use vello_common::pixmap::Pixmap;

/// A procedurally generated, tileable texture (for example linen).
///
/// Textures are generated at the device scale they are drawn at, so they stay sharp at any
/// scale. Backends cache the result per `(cache_key, scale)`.
pub trait ProceduralTexture: Send + Sync {
    /// Identifies this texture's content. Two textures with the same key must render
    /// identical pixels at the same scale.
    fn cache_key(&self) -> u64;

    /// Size of one tile in logical pixels. Must give a whole number of device pixels at
    /// every scale the texture is rendered at.
    fn tile_size(&self) -> (u16, u16);

    /// Render one tile at `scale`. The result is `tile_size() * scale` device pixels,
    /// premultiplied RGBA8.
    fn render(&self, scale: f64) -> Pixmap;
}

/// A texture brush: a procedural texture repeated in both directions.
#[derive(Clone)]
pub struct TextureBrush {
    /// The texture to tile.
    pub texture: Arc<dyn ProceduralTexture>,
    /// Places the texture in logical space (one texture unit = one logical pixel).
    pub transform: Affine,
}

impl fmt::Debug for TextureBrush {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextureBrush")
            .field("cache_key", &self.texture.cache_key())
            .field("transform", &self.transform)
            .finish()
    }
}

/// How a shape is painted.
#[derive(Clone, Debug)]
pub enum Brush {
    /// A solid colour.
    Solid(Color),
    /// A linear, radial or sweep gradient in the shape's coordinate space.
    Gradient(Gradient),
    /// A repeated procedural texture.
    Texture(TextureBrush),
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<Gradient> for Brush {
    fn from(gradient: Gradient) -> Self {
        Self::Gradient(gradient)
    }
}

impl From<TextureBrush> for Brush {
    fn from(texture: TextureBrush) -> Self {
        Self::Texture(texture)
    }
}

/// One recorded drawing command. Every command carries its own transform.
#[derive(Clone, Debug)]
pub enum Command {
    /// Fill a path (non-zero winding).
    Fill {
        /// Logical-space transform of the path and brush.
        transform: Affine,
        /// The shape.
        path: BezPath,
        /// The paint.
        brush: Brush,
    },
    /// Stroke a path.
    Stroke {
        /// Logical-space transform of the path and brush.
        transform: Affine,
        /// The shape.
        path: BezPath,
        /// Stroke style.
        style: Stroke,
        /// The paint.
        brush: Brush,
    },
    /// A Gaussian-blurred rounded rectangle, used for shadows and glows.
    BlurredRoundedRect {
        /// Logical-space transform.
        transform: Affine,
        /// The rectangle before blurring.
        rect: Rect,
        /// Corner radius.
        radius: f64,
        /// Standard deviation of the blur, in logical pixels.
        std_dev: f64,
        /// Shadow colour.
        color: Color,
    },
    /// Start an isolated layer, clipped to `clip` and composited with `opacity`.
    PushLayer {
        /// Logical-space transform of the clip path.
        transform: Affine,
        /// Optional clip path.
        clip: Option<BezPath>,
        /// Group opacity, 0 to 1.
        opacity: f32,
    },
    /// End the most recent layer.
    PopLayer,
}

/// A recorded scene in logical pixels.
#[derive(Clone, Debug)]
pub struct Scene {
    width: f64,
    height: f64,
    commands: Vec<Command>,
    transform: Affine,
    depth: usize,
}

impl Scene {
    /// An empty scene of `width` × `height` logical pixels.
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            commands: Vec::new(),
            transform: Affine::IDENTITY,
            depth: 0,
        }
    }

    /// Logical width.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Logical height.
    pub fn height(&self) -> f64 {
        self.height
    }

    /// Recorded commands, in order.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// The transform applied to commands recorded from now on.
    pub fn transform(&self) -> Affine {
        self.transform
    }

    /// Set the transform applied to commands recorded from now on.
    pub fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    /// Record commands with an extra transform, then restore the previous one.
    pub fn with_transform(&mut self, transform: Affine, f: impl FnOnce(&mut Self)) {
        let saved = self.transform;
        self.transform = saved * transform;
        f(self);
        self.transform = saved;
    }

    /// Fill `shape` with `brush`.
    pub fn fill(&mut self, shape: &impl Shape, brush: impl Into<Brush>) {
        self.commands.push(Command::Fill {
            transform: self.transform,
            path: shape.to_path(0.1),
            brush: brush.into(),
        });
    }

    /// Stroke `shape` with `brush`.
    pub fn stroke(&mut self, shape: &impl Shape, style: &Stroke, brush: impl Into<Brush>) {
        self.commands.push(Command::Stroke {
            transform: self.transform,
            path: shape.to_path(0.1),
            style: style.clone(),
            brush: brush.into(),
        });
    }

    /// Draw a blurred rounded rectangle (a shadow).
    pub fn blurred_rounded_rect(&mut self, rect: Rect, radius: f64, std_dev: f64, color: Color) {
        self.commands.push(Command::BlurredRoundedRect {
            transform: self.transform,
            rect,
            radius,
            std_dev,
            color,
        });
    }

    /// Start a layer clipped to `clip`. Must be balanced with [`Self::pop_layer`].
    pub fn push_clip(&mut self, clip: &impl Shape) {
        self.push_layer(Some(clip.to_path(0.1)), 1.0);
    }

    /// Start a group with `opacity`. Must be balanced with [`Self::pop_layer`].
    pub fn push_opacity(&mut self, opacity: f32) {
        self.push_layer(None, opacity);
    }

    fn push_layer(&mut self, clip: Option<BezPath>, opacity: f32) {
        self.depth += 1;
        self.commands.push(Command::PushLayer {
            transform: self.transform,
            clip,
            opacity: opacity.clamp(0.0, 1.0),
        });
    }

    /// End the most recent layer. Extra pops are ignored.
    pub fn pop_layer(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
            self.commands.push(Command::PopLayer);
        }
    }

    /// Number of layers pushed and not yet popped. Backends close them when replaying.
    pub fn open_layers(&self) -> usize {
        self.depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello_common::kurbo::Rect;
    use vello_common::peniko::color::palette::css;

    #[test]
    fn unbalanced_pop_is_ignored() {
        let mut scene = Scene::new(10.0, 10.0);
        scene.pop_layer();
        assert!(scene.commands().is_empty());
        scene.push_opacity(0.5);
        assert_eq!(scene.open_layers(), 1);
        scene.pop_layer();
        assert_eq!(scene.open_layers(), 0);
        assert_eq!(scene.commands().len(), 2);
    }

    #[test]
    fn with_transform_restores() {
        let mut scene = Scene::new(10.0, 10.0);
        scene.with_transform(Affine::translate((2.0, 3.0)), |s| {
            s.fill(&Rect::new(0.0, 0.0, 1.0, 1.0), css::RED);
        });
        assert_eq!(scene.transform(), Affine::IDENTITY);
        let Command::Fill { transform, .. } = &scene.commands()[0] else {
            panic!("expected a fill");
        };
        assert_eq!(transform.translation().x, 2.0);
    }
}
