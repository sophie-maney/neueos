//! Icons: a small set of original glyphs drawn in one colour, and vector icons from SVG path
//! data (task 2.20a, ADR 0051). Everything is vector, so icons stay sharp at any scale.
//!
//! An [`Icon`] is drawn into a square (or any) rectangle with [`icon`]: glyphs follow the
//! colour given, a [`VectorIcon`]'s shapes either follow it too ([`IconPaint::Current`]) or
//! have their own colours for light and dark ([`IconPaint::Fixed`]). A raster icon
//! ([`Icon::raster`], task 3.3) is a [`Picture`]: the pixels of an icon-theme PNG, or of an
//! SVG that `neue-desktop` rasterised at the size it will be drawn; it keeps its own colours
//! and is resampled for the device scale like the desktop picture.

use std::sync::Arc;

use thiserror::Error;
use vello_common::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Stroke};
use vello_common::peniko::{Color, ImageAlphaType};
use vello_common::pixmap::{PixelMetadata, Pixmap};

use crate::Variant;
use crate::materials::desktop::Picture;
use crate::scene::{ProceduralTexture, Scene, TextureBrush};

/// The named glyphs: original flat shapes on a 16-unit grid, drawn in one colour so they
/// follow the surrounding text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Glyph {
    /// A folder.
    Folder,
    /// A page with a folded corner.
    Document,
    /// A framed picture of a hill and the sun.
    Picture,
    /// A musical note.
    Music,
    /// A five-pointed star.
    Star,
    /// A clock face.
    Clock,
}

impl Glyph {
    /// What the glyph shows, in words, for assistive technology.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Folder => "Folder",
            Self::Document => "Document",
            Self::Picture => "Picture",
            Self::Music => "Music",
            Self::Star => "Star",
            Self::Clock => "Clock",
        }
    }
}

/// How one shape of a [`VectorIcon`] is coloured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconPaint {
    /// The colour the icon is drawn in (the text colour, say), with its alpha multiplied by
    /// `alpha` (1.0 for as is, less for a fainter part).
    Current {
        /// The factor for the colour's alpha.
        alpha: f32,
    },
    /// Its own colour, one for each appearance. Not dimmed by the caller's colour, so a
    /// disabled icon needs the caller to draw it with `opacity`.
    Fixed {
        /// In the light appearance.
        light: Color,
        /// In the dark appearance.
        dark: Color,
    },
}

impl IconPaint {
    /// The icon's colour as is.
    pub const CURRENT: Self = Self::Current { alpha: 1.0 };

    fn resolve(self, current: Color, variant: Variant) -> Color {
        match self {
            Self::Current { alpha } => current.multiply_alpha(alpha),
            Self::Fixed { light, dark } => {
                if variant.is_dark() {
                    dark
                } else {
                    light
                }
            }
        }
    }
}

/// Why SVG path data was refused.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum IconError {
    /// The path data didn't parse.
    #[error("bad SVG path data: {0}")]
    Path(String),
    /// The view box has no area.
    #[error("the view box must have a positive width and height")]
    ViewBox,
}

/// An icon made of filled shapes, each from SVG path data, on a view box. The paths are
/// parsed once, when the icon is built.
///
/// ```
/// use neue_paint::icon::{Icon, IconPaint, VectorIcon};
///
/// let dot = VectorIcon::new(16.0, 16.0)
///     .unwrap()
///     .shape("M8 2a6 6 0 1 0 0.01 0z", IconPaint::CURRENT)
///     .unwrap();
/// let icon = Icon::from(dot);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct VectorIcon {
    view_box: Rect,
    shapes: Vec<(BezPath, IconPaint)>,
}

impl VectorIcon {
    /// An icon with no shapes yet, on a view box from the origin to `width` by `height`.
    ///
    /// # Errors
    /// [`IconError::ViewBox`] if either is not a positive, finite number.
    pub fn new(width: f64, height: f64) -> Result<Self, IconError> {
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(IconError::ViewBox);
        }
        Ok(Self {
            view_box: Rect::new(0.0, 0.0, width, height),
            shapes: Vec::new(),
        })
    }

    /// Adds a filled shape from SVG path data (the `d` attribute), painted with `paint`.
    ///
    /// # Errors
    /// [`IconError::Path`] if `data` doesn't parse.
    pub fn shape(mut self, data: &str, paint: IconPaint) -> Result<Self, IconError> {
        let path = BezPath::from_svg(data).map_err(|e| IconError::Path(e.to_string()))?;
        self.shapes.push((path, paint));
        Ok(self)
    }

    /// The view box the shapes are drawn on.
    pub fn view_box(&self) -> Rect {
        self.view_box
    }

    /// The number of shapes.
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// Whether there are no shapes.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}

/// An icon: a named [`Glyph`] or a [`VectorIcon`]. Cheap to clone.
#[derive(Clone, Debug)]
pub struct Icon(Repr);

#[derive(Clone, Debug)]
enum Repr {
    Glyph(Glyph),
    Vector(Arc<VectorIcon>),
    Raster(Picture),
}

impl Icon {
    /// The glyph, if this is one.
    pub fn glyph(&self) -> Option<Glyph> {
        match self.0 {
            Repr::Glyph(g) => Some(g),
            Repr::Vector(_) | Repr::Raster(_) => None,
        }
    }

    /// The vector icon, if this is one.
    pub fn vector(&self) -> Option<&VectorIcon> {
        match &self.0 {
            Repr::Vector(v) => Some(v),
            Repr::Glyph(_) | Repr::Raster(_) => None,
        }
    }

    /// An icon made of the pixels of `picture` (kept as they are, in their own colours).
    pub fn raster(picture: Picture) -> Self {
        Self(Repr::Raster(picture))
    }

    /// The picture, if this is a raster icon.
    pub fn picture(&self) -> Option<&Picture> {
        match &self.0 {
            Repr::Raster(p) => Some(p),
            Repr::Glyph(_) | Repr::Vector(_) => None,
        }
    }

    /// Whether the icon has colours of its own (a vector icon with fixed paints, or a
    /// raster one) rather than following the colour it is drawn in.
    pub fn has_own_colors(&self) -> bool {
        !matches!(self.0, Repr::Glyph(_))
    }
}

/// Glyphs compare by value; vector and raster icons by identity (clones of one are equal).
impl PartialEq for Icon {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Glyph(a), Repr::Glyph(b)) => a == b,
            (Repr::Vector(a), Repr::Vector(b)) => Arc::ptr_eq(a, b),
            (Repr::Raster(a), Repr::Raster(b)) => a.id() == b.id(),
            _ => false,
        }
    }
}

impl Eq for Icon {}

impl From<Glyph> for Icon {
    fn from(glyph: Glyph) -> Self {
        Self(Repr::Glyph(glyph))
    }
}

impl From<VectorIcon> for Icon {
    fn from(vector: VectorIcon) -> Self {
        Self(Repr::Vector(Arc::new(vector)))
    }
}

/// Draws `icon` centred in `rect`, as large as fits, in `color` (and for `variant`: see
/// [`IconPaint::Fixed`]).
pub fn icon(scene: &mut Scene, rect: Rect, icon: &Icon, color: Color, variant: Variant) {
    match &icon.0 {
        Repr::Glyph(glyph) => draw_glyph(scene, rect, *glyph, color),
        Repr::Vector(vector) => {
            let view = vector.view_box;
            let k = (rect.width() / view.width()).min(rect.height() / view.height());
            let x = rect.x0 + (rect.width() - view.width() * k) / 2.0;
            let y = rect.y0 + (rect.height() - view.height() * k) / 2.0;
            let to_rect = Affine::translate((x, y)) * Affine::scale(k);
            for (path, paint) in &vector.shapes {
                let mut path = path.clone();
                path.apply_affine(to_rect);
                scene.fill(&path, paint.resolve(color, variant));
            }
        }
        Repr::Raster(picture) => draw_raster(scene, rect, picture),
    }
}

/// Draws a raster icon centred in `rect` at the largest whole-logical-pixel size that fits,
/// keeping its aspect. The texture is resampled from the picture at the device scale.
pub(crate) fn draw_raster(scene: &mut Scene, rect: Rect, picture: &Picture) {
    let (pw, ph) = (f64::from(picture.width()), f64::from(picture.height()));
    let k = (rect.width() / pw).min(rect.height() / ph);
    // Sizes are below 65536 (a picture's sides are), so the casts keep their value.
    let w = (pw * k).round().clamp(1.0, 65535.0);
    let h = (ph * k).round().clamp(1.0, 65535.0);
    let x = (rect.x0 + (rect.width() - w) / 2.0).round();
    let y = (rect.y0 + (rect.height() - h) / 2.0).round();
    let brush = TextureBrush {
        texture: Arc::new(RasterTexture {
            picture: picture.clone(),
            size: (w as u16, h as u16),
        }),
        // Texture brushes are placed in scene space; the icon's follows the scene's
        // transform with its rect (a dock icon's mirror image, ADR 0092).
        transform: scene.transform() * Affine::translate((x, y)),
    };
    scene.fill(&Rect::new(x, y, x + w, y + h), brush);
}

/// A raster icon placed at a size in logical pixels: what the backends cache and draw.
struct RasterTexture {
    picture: Picture,
    size: (u16, u16),
}

impl ProceduralTexture for RasterTexture {
    fn cache_key(&self) -> u64 {
        // Distinct from the desktop picture's keys by the top bit; the id is unique per
        // picture and the size places it.
        (1 << 63)
            ^ (self.picture.id() << 32)
            ^ (u64::from(self.size.0) << 16)
            ^ u64::from(self.size.1)
    }

    fn tile_size(&self) -> (u16, u16) {
        self.size
    }

    fn render(&self, scale: f64) -> Pixmap {
        let to_px = |logical: u16| (f64::from(logical) * scale).round().clamp(1.0, 65535.0) as u32;
        let (w, h) = (to_px(self.size.0), to_px(self.size.1));
        let pic = &self.picture;
        let (sx, sy) = (
            f64::from(pic.width()) / f64::from(w),
            f64::from(pic.height()) / f64::from(h),
        );
        let radius = sx.min(sy).floor().max(1.0) as i64;
        let mut data = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                let c = pic.sample((f64::from(x) + 0.5) * sx, (f64::from(y) + 0.5) * sy, radius);
                data.extend(c.map(|v| (v + 0.5).clamp(0.0, 255.0) as u8));
            }
        }
        Pixmap::from_parts(
            data,
            w as u16,
            h as u16,
            PixelMetadata {
                may_have_transparency: !pic.is_opaque(),
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
        )
    }
}

fn draw_glyph(scene: &mut Scene, rect: Rect, glyph: Glyph, color: Color) {
    // Glyphs are drawn on a 16-unit grid and scaled to the rectangle.
    let k = rect.width().min(rect.height()) / 16.0;
    let at = |x: f64, y: f64| Point::new(rect.x0 + x * k, rect.y0 + y * k);
    let mut path = BezPath::new();
    match glyph {
        Glyph::Folder => {
            path.move_to(at(1.0, 3.5));
            path.line_to(at(6.0, 3.5));
            path.line_to(at(8.0, 5.5));
            path.line_to(at(15.0, 5.5));
            path.line_to(at(15.0, 13.5));
            path.line_to(at(1.0, 13.5));
            path.close_path();
            scene.fill(&path, color);
        }
        Glyph::Document => {
            path.move_to(at(3.0, 1.0));
            path.line_to(at(9.5, 1.0));
            path.line_to(at(9.5, 4.5));
            path.line_to(at(13.0, 4.5));
            path.line_to(at(13.0, 15.0));
            path.line_to(at(3.0, 15.0));
            path.close_path();
            scene.fill(&path, color);
            // The turned-down corner, fainter than the page.
            let mut fold = BezPath::new();
            fold.move_to(at(9.5, 1.0));
            fold.line_to(at(9.5, 4.5));
            fold.line_to(at(13.0, 4.5));
            fold.close_path();
            scene.fill(&fold, color.with_alpha(0.5));
        }
        Glyph::Picture => {
            let frame = Rect::new(
                rect.x0 + k,
                rect.y0 + 2.5 * k,
                rect.x1 - k,
                rect.y1 - 2.5 * k,
            );
            scene.stroke(&frame.inset(-k * 0.5), &Stroke::new(k), color);
            scene.fill(&Circle::new(at(10.5, 6.5), 1.4 * k), color);
            path.move_to(at(2.0, 12.5));
            path.line_to(at(6.0, 7.5));
            path.line_to(at(9.0, 11.0));
            path.line_to(at(11.0, 9.5));
            path.line_to(at(14.0, 12.5));
            path.close_path();
            scene.fill(&path, color);
        }
        Glyph::Music => {
            scene.fill(&Circle::new(at(5.0, 12.5), 2.4 * k), color);
            scene.fill(
                &Rect::new(
                    rect.x0 + 6.8 * k,
                    rect.y0 + 2.0 * k,
                    rect.x0 + 8.2 * k,
                    rect.y0 + 12.5 * k,
                ),
                color,
            );
            path.move_to(at(8.2, 2.0));
            path.line_to(at(12.5, 4.0));
            path.line_to(at(12.5, 7.0));
            path.line_to(at(8.2, 5.2));
            path.close_path();
            scene.fill(&path, color);
        }
        Glyph::Star => {
            let (c, outer, inner) = (at(8.0, 8.6), 7.0 * k, 3.0 * k);
            for i in 0..10 {
                let angle = (f64::from(i) * 36.0 - 90.0).to_radians();
                let r = if i % 2 == 0 { outer } else { inner };
                let p = Point::new(c.x + r * angle.cos(), c.y + r * angle.sin());
                if i == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            path.close_path();
            scene.fill(&path, color);
        }
        Glyph::Clock => {
            scene.stroke(
                &Circle::new(at(8.0, 8.0), 6.4 * k),
                &Stroke::new(1.4 * k),
                color,
            );
            scene.stroke(
                &Line::new(at(8.0, 8.0), at(8.0, 4.0)),
                &Stroke::new(1.4 * k),
                color,
            );
            scene.stroke(
                &Line::new(at(8.0, 8.0), at(11.0, 9.5)),
                &Stroke::new(1.4 * k),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    const SQUARE: &str = "M0 0H8V8H0z";

    fn painted(icon_: &Icon, rect: Rect, variant: Variant) -> usize {
        let mut scene = Scene::new(64.0, 64.0);
        icon(
            &mut scene,
            rect,
            icon_,
            Color::from_rgb8(10, 20, 30),
            variant,
        );
        scene.commands().len()
    }

    #[test]
    fn bad_path_data_and_view_boxes_are_refused() {
        assert!(matches!(
            VectorIcon::new(16.0, 16.0)
                .unwrap()
                .shape("M0 0 Q", IconPaint::CURRENT),
            Err(IconError::Path(_))
        ));
        assert_eq!(VectorIcon::new(0.0, 4.0), Err(IconError::ViewBox));
        assert_eq!(VectorIcon::new(f64::NAN, 4.0), Err(IconError::ViewBox));
    }

    #[test]
    fn vector_icons_compare_by_identity_and_glyphs_by_value() {
        let make = || {
            Icon::from(
                VectorIcon::new(8.0, 8.0)
                    .unwrap()
                    .shape(SQUARE, IconPaint::CURRENT)
                    .unwrap(),
            )
        };
        let a = make();
        assert_eq!(a, a.clone());
        assert_ne!(a, make());
        assert_eq!(Icon::from(Glyph::Star), Icon::from(Glyph::Star));
        assert_ne!(Icon::from(Glyph::Star), Icon::from(Glyph::Clock));
        assert_ne!(Icon::from(Glyph::Star), a);
        assert_eq!(a.vector().map(VectorIcon::len), Some(1));
        assert_eq!(Icon::from(Glyph::Star).glyph(), Some(Glyph::Star));
    }

    #[test]
    fn a_vector_icon_is_fitted_and_centred_in_its_rectangle() {
        let icon_ = Icon::from(
            VectorIcon::new(8.0, 8.0)
                .unwrap()
                .shape(SQUARE, IconPaint::CURRENT)
                .unwrap(),
        );
        // 40 wide, 20 high: the 8-unit square becomes 20 px, centred horizontally.
        let rect = Rect::new(10.0, 4.0, 50.0, 24.0);
        let mut scene = Scene::new(64.0, 64.0);
        icon(&mut scene, rect, &icon_, Color::BLACK, Variant::default());
        let crate::Command::Fill { path, .. } = &scene.commands()[0] else {
            panic!("expected a fill, got {:?}", scene.commands());
        };
        let bounds = vello_common::kurbo::Shape::bounding_box(path);
        assert_eq!(bounds, Rect::new(20.0, 4.0, 40.0, 24.0));
    }

    #[test]
    fn fixed_paints_follow_the_appearance_and_current_follows_the_caller() {
        let light = Color::from_rgb8(255, 0, 0);
        let dark = Color::from_rgb8(0, 0, 255);
        let fixed = IconPaint::Fixed { light, dark };
        let caller = Color::from_rgb8(1, 2, 3);
        let light_v = Variant::new(Appearance::Light);
        let dark_v = Variant::new(Appearance::Dark);
        assert_eq!(fixed.resolve(caller, light_v), light);
        assert_eq!(fixed.resolve(caller, dark_v), dark);
        assert_eq!(IconPaint::CURRENT.resolve(caller, dark_v), caller);
        let faint = IconPaint::Current { alpha: 0.5 }.resolve(caller, light_v);
        assert_eq!(faint, caller.multiply_alpha(0.5));
    }

    #[test]
    fn glyphs_paint_and_follow_the_rectangle() {
        for glyph in [
            Glyph::Folder,
            Glyph::Document,
            Glyph::Picture,
            Glyph::Music,
            Glyph::Star,
            Glyph::Clock,
        ] {
            let small = painted(
                &glyph.into(),
                Rect::new(0.0, 0.0, 16.0, 16.0),
                Variant::default(),
            );
            assert!(small > 0, "{glyph:?} draws something");
            assert!(!glyph.name().is_empty());
        }
    }

    #[test]
    fn raster_icons_keep_their_colours_and_aspect() {
        // A 2×1 picture: red, then half-transparent blue (straight alpha in).
        let pic = Picture::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        let icon_ = Icon::raster(pic.clone());
        assert!(icon_.has_own_colors() && icon_.picture().is_some());
        assert_eq!(icon_, Icon::raster(pic));
        assert!(icon_.glyph().is_none() && icon_.vector().is_none());

        // Drawn into a 16×16 box it is 16×8, centred vertically.
        let mut scene = Scene::new(16.0, 16.0);
        icon(
            &mut scene,
            Rect::new(0.0, 0.0, 16.0, 16.0),
            &icon_,
            Color::BLACK,
            Variant::default(),
        );
        let out = crate::cpu::CpuRenderer::new(crate::cpu::Quality::Reference)
            .render(&scene, 1.0)
            .unwrap();
        let px = |x: usize, y: usize| {
            let i = (y * 16 + x) * 4;
            out.data_as_u8_slice()[i..i + 4].to_vec()
        };
        assert_eq!(px(2, 8), [255, 0, 0, 255]);
        let blue = px(13, 8);
        assert!(blue[2] > 120 && blue[3] > 120 && blue[3] < 136, "{blue:?}");
        assert_eq!(px(2, 2), [0, 0, 0, 0], "above the picture is clear");
    }
}
