//! Backdrop blur (task 3.4, ADR 0071): `ext-background-effect-v1` and a dual-Kawase blur
//! in the GLES renderer.
//!
//! A client marks a blur region on a surface; [`crate::render`] puts a [`BlurElement`]
//! right below that surface's render element. The element is a Smithay *framebuffer
//! effect*: when anything behind it changed, the damage tracker redraws everything under
//! the element's whole area and calls [`BlurElement::capture_framebuffer`], which copies
//! that area out of the framebuffer and blurs it into a cached texture. Every draw then
//! paints the cached texture inside the blur region, so a still background is blurred
//! once and partial redraws (the client's own animations, the cursor) reuse it.
//!
//! The element's area is the region's bounds grown by the blur's reach, so damage within
//! that distance of the region re-blurs it, and the blur never samples stale pixels.

use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement};
use smithay::backend::renderer::gles::{
    Capability, GlesError, GlesFrame, GlesRenderer, GlesTexProgram, GlesTexture, Uniform,
    UniformName, UniformType, ffi,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{Bind, Frame, FrameContext, Offscreen, Renderer, Texture};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::background_effect::{
    BackgroundEffectSurfaceCachedState, Capability as EffectCapability, ExtBackgroundEffectHandler,
};
use smithay::wayland::compositor::{RectangleKind, RegionAttributes, SurfaceData};

use crate::state::State;

/// Offset of the dual-Kawase samples, in texels of the pass's target. With [`passes`]
/// passes down and up the blur reaches about `2^(passes + 1) * OFFSET` physical pixels.
const OFFSET: f32 = 2.0;

/// Down- and up-sampling passes at output `scale`: one more on HiDPI outputs, so the blur
/// is as wide in logical pixels (32) at scale 2 as at scale 1.
fn passes(scale: f64) -> usize {
    if scale > 1.5 { 4 } else { 3 }
}

/// How far (in physical pixels) the blur at a point reads around it.
fn reach(passes: usize) -> i32 {
    // At most 4 passes: 2^5 * 2.0 = 64, exact in f32.
    (f32::from(1u16 << (passes + 1)) * OFFSET).ceil() as i32
}

impl ExtBackgroundEffectHandler for State {
    fn capabilities(&self) -> EffectCapability {
        EffectCapability::Blur
    }
}

/// The blur region of one surface, collected by [`surface_regions`].
#[derive(Debug, Clone)]
pub struct SurfaceBlur {
    /// The render element id of the surface (`Id::from_wayland_resource`).
    surface: Id,
    /// The id of its blur element, kept with the surface so it is stable across frames.
    blur: Id,
    /// The region in surface-local logical coordinates, as disjoint rectangles.
    rects: Vec<Rectangle<i32, Logical>>,
}

/// Id of a surface's blur element, kept in its `SurfaceData`.
struct BlurId(Id);

/// The surfaces with a non-empty blur region among those `visit` walks (a window's or a
/// layer surface's `with_surfaces`).
pub fn surface_regions(
    visit: impl FnOnce(&mut dyn FnMut(&WlSurface, &SurfaceData)),
) -> Vec<SurfaceBlur> {
    let mut found = Vec::new();
    visit(&mut |surface, states| {
        let region = states
            .cached_state
            .get::<BackgroundEffectSurfaceCachedState>()
            .current()
            .blur_region
            .clone();
        let Some(rects) = region
            .as_ref()
            .map(disjoint_rects)
            .filter(|r| !r.is_empty())
        else {
            return;
        };
        let blur = states
            .data_map
            .get_or_insert_threadsafe(|| BlurId(Id::new()))
            .0
            .clone();
        found.push(SurfaceBlur {
            surface: Id::from_wayland_resource(surface),
            blur,
            rects,
        });
    });
    found
}

/// A `wl_region` as disjoint rectangles: each added rectangle minus what is already in,
/// and every subtracted one cut out of all of them.
fn disjoint_rects(region: &RegionAttributes) -> Vec<Rectangle<i32, Logical>> {
    let mut rects: Vec<Rectangle<i32, Logical>> = Vec::new();
    for (kind, rect) in &region.rects {
        if rect.is_empty() {
            continue;
        }
        match kind {
            RectangleKind::Add => {
                let new = rect.subtract_rects(rects.iter().copied());
                rects.extend(new);
            }
            RectangleKind::Subtract => {
                rects = Rectangle::subtract_rects_many_in_place(rects, [*rect]);
            }
        }
    }
    rects
}

/// Whether `renderer` can blur: it needs `glBlitFramebuffer` (GLES 3) to copy what is
/// behind a surface out of the framebuffer.
pub fn supported(renderer: &GlesRenderer) -> bool {
    let supported = renderer.capabilities().contains(&Capability::Blit);
    if !supported {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| tracing::warn!("no GLES 3 framebuffer blits: backdrop blur is off"));
    }
    supported
}

/// The blur behind one surface on one output. See the module docs.
#[derive(Debug)]
pub struct BlurElement {
    id: Id,
    /// What the blur reads: the region's bounds grown by the blur's reach, clipped to the
    /// output (physical, output-relative).
    geometry: Rectangle<i32, Physical>,
    /// Where the blur shows, relative to `geometry`.
    rects: Vec<Rectangle<i32, Physical>>,
    passes: usize,
    /// Changes when `rects` do, so a new region is re-blurred. (Moves are the damage
    /// tracker's business.)
    commit: CommitCounter,
}

impl BlurElement {
    /// The blur for `blur`, whose surface's render element covers `surface_geometry`, on an
    /// output of `output_size` physical pixels at `scale`. `None` if the region is outside
    /// the surface or the output.
    pub fn new(
        blur: &SurfaceBlur,
        surface_geometry: Rectangle<i32, Physical>,
        scale: f64,
        output_size: Size<i32, Physical>,
    ) -> Option<Self> {
        let output = Rectangle::from_size(output_size);
        let visible: Vec<Rectangle<i32, Physical>> = blur
            .rects
            .iter()
            .filter_map(|rect| {
                let mut rect = rect.to_f64().to_physical(scale).to_i32_round();
                rect.loc += surface_geometry.loc;
                rect.intersection(surface_geometry)?.intersection(output)
            })
            .collect();
        let bounds = visible.iter().copied().reduce(|a, b| a.merge(b))?;
        let passes = passes(scale);
        let reach = reach(passes);
        let grown = Rectangle::new(
            bounds.loc - Point::from((reach, reach)),
            bounds.size + Size::from((2 * reach, 2 * reach)),
        );
        let geometry = grown.intersection(output)?;
        let rects: Vec<Rectangle<i32, Physical>> = visible
            .into_iter()
            .map(|mut rect| {
                rect.loc -= geometry.loc;
                rect
            })
            .collect();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for rect in &rects {
            (rect.loc.x, rect.loc.y, rect.size.w, rect.size.h).hash(&mut hasher);
        }
        passes.hash(&mut hasher);
        // Truncating the hash on 32-bit targets is fine: it only has to change.
        #[allow(clippy::cast_possible_truncation)]
        let commit = CommitCounter::from(hasher.finish() as usize);
        Some(Self {
            id: blur.blur.clone(),
            geometry,
            rects,
            passes,
            commit,
        })
    }
}

/// Pair each element of a surface tree with the blur below it: `elements` (topmost first)
/// are pushed into `out`, each followed by its surface's [`BlurElement`] if `blurs` has
/// one.
pub fn push_with_blur<E, O>(
    out: &mut Vec<O>,
    elements: Vec<E>,
    blurs: &[SurfaceBlur],
    scale: f64,
    output_size: Size<i32, Physical>,
) where
    E: Element,
    O: From<E> + From<BlurElement>,
{
    for element in elements {
        let blur = blurs
            .iter()
            .find(|blur| &blur.surface == element.id())
            .and_then(|blur| {
                BlurElement::new(
                    blur,
                    element.geometry(Scale::from(scale)),
                    scale,
                    output_size,
                )
            });
        out.push(O::from(element));
        if let Some(blur) = blur {
            out.push(O::from(blur));
        }
    }
}

impl Element for BlurElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size(Size::from((
            f64::from(self.geometry.size.w),
            f64::from(self.geometry.size.h),
        )))
    }

    fn geometry(&self, _scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.geometry
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        if commit == Some(self.commit) {
            DamageSet::default()
        } else {
            DamageSet::from_slice(&[Rectangle::from_size(self.geometry(scale).size)])
        }
    }

    fn opaque_regions(&self, _scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }

    fn kind(&self) -> Kind {
        Kind::Unspecified
    }

    fn is_framebuffer_effect(&self) -> bool {
        true
    }
}

impl RenderElement<GlesRenderer> for BlurElement {
    fn capture_framebuffer(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), GlesError> {
        cache.insert_if_missing(|| RefCell::new(Blurred::default()));
        let Some(blurred) = cache.get::<RefCell<Blurred>>() else {
            return Ok(());
        };
        let mut blurred = blurred.borrow_mut();
        // The framebuffer is in the output's transform: copy the rectangle it holds `dst`
        // in, blur it as it is (a blur does not care), and draw it back with the
        // output's transform as the texture's own.
        let transform = frame.transformation();
        let area = transform.transform_rect_in(dst, &frame.output_size());
        blurred.valid = false;
        let programs = {
            let mut guard = frame.renderer();
            let renderer = guard.as_mut();
            blurred.ensure_textures(renderer, area.size, self.passes)?;
            Programs::get(renderer)?
        };
        let target = blurred.textures[0].tex_id();
        frame.with_context(|gl| {
            // SAFETY: the frame's context is current; `target` is a live texture of
            // `area.size` (ensured above); only state this restores is touched.
            #[allow(unsafe_code)] // raw GL, see SAFETY
            unsafe {
                copy_from_framebuffer(gl, area, target);
            }
        })?;
        {
            let mut guard = frame.renderer();
            let renderer = guard.as_mut();
            blurred.blur(renderer, &programs)?;
        }
        blurred.transform = transform;
        blurred.valid = true;
        Ok(())
    }

    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let Some(blurred) = cache.and_then(|cache| cache.get::<RefCell<Blurred>>()) else {
            return Ok(());
        };
        let blurred = blurred.borrow();
        let transform = frame.transformation();
        let Some(texture) = blurred.textures.first() else {
            return Ok(());
        };
        let fb_size = transform.transform_size(dst.size);
        if !blurred.valid
            || blurred.transform != transform
            || texture.size() != Size::<i32, Buffer>::from((fb_size.w, fb_size.h))
        {
            return Ok(());
        }
        let visible: Vec<Rectangle<i32, Physical>> = damage
            .iter()
            .flat_map(|damage| {
                self.rects
                    .iter()
                    .filter_map(move |rect| damage.intersection(*rect))
            })
            .collect();
        if visible.is_empty() {
            return Ok(());
        }
        // Opaque: the blurred backdrop replaces what was behind (no blending).
        frame.render_texture_from_to(
            texture,
            Rectangle::from_size(texture.size()).to_f64(),
            dst,
            &visible,
            &visible,
            transform,
            1.0,
            None,
            &[],
        )
    }
}

/// A blurred backdrop cached in the damage tracker's per-element cache: one tracker (one
/// output buffer or screencopy session) each.
#[derive(Debug, Default)]
struct Blurred {
    /// Level 0 is full size and ends up holding the result; level `i` is `1/2^i` of it.
    textures: Vec<GlesTexture>,
    /// The output transform the textures are in.
    transform: Transform,
    /// Level 0 holds a finished blur.
    valid: bool,
}

impl Blurred {
    /// Make sure there are `passes + 1` levels for a full size of `size`.
    fn ensure_textures(
        &mut self,
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
        passes: usize,
    ) -> Result<(), GlesError> {
        let sizes: Vec<Size<i32, Buffer>> = (0..=passes)
            .map(|level| {
                let shrink = |v: i32| ((v + (1 << level) - 1) >> level).max(1);
                Size::from((shrink(size.w), shrink(size.h)))
            })
            .collect();
        if self.textures.len() == sizes.len()
            && self
                .textures
                .iter()
                .zip(&sizes)
                .all(|(t, s)| t.size() == *s)
        {
            return Ok(());
        }
        self.textures = sizes
            .into_iter()
            .map(|size| Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size))
            .collect::<Result<_, _>>()?;
        let ids: Vec<u32> = self.textures.iter().map(GlesTexture::tex_id).collect();
        renderer.with_context(|gl| {
            // SAFETY: the context is current inside `with_context`; the ids are textures
            // just created on it. Clamping keeps the blur from wrapping around an edge.
            #[allow(unsafe_code)] // raw GL, see SAFETY
            unsafe {
                for id in ids {
                    gl.BindTexture(ffi::TEXTURE_2D, id);
                    gl.TexParameteri(
                        ffi::TEXTURE_2D,
                        ffi::TEXTURE_WRAP_S,
                        ffi::CLAMP_TO_EDGE as i32,
                    );
                    gl.TexParameteri(
                        ffi::TEXTURE_2D,
                        ffi::TEXTURE_WRAP_T,
                        ffi::CLAMP_TO_EDGE as i32,
                    );
                }
                gl.BindTexture(ffi::TEXTURE_2D, 0);
            }
        })
    }

    /// Blur level 0 in place: down through every level, then back up.
    fn blur(&mut self, renderer: &mut GlesRenderer, programs: &Programs) -> Result<(), GlesError> {
        let levels = self.textures.len();
        for level in 1..levels {
            self.pass(renderer, level - 1, level, &programs.down)?;
        }
        for level in (0..levels - 1).rev() {
            self.pass(renderer, level + 1, level, &programs.up)?;
        }
        Ok(())
    }

    /// Draw level `from` into level `to` with `program`.
    fn pass(
        &mut self,
        renderer: &mut GlesRenderer,
        from: usize,
        to: usize,
        program: &GlesTexProgram,
    ) -> Result<(), GlesError> {
        let source = self.textures[from].clone();
        let size = self.textures[to].size();
        let size: Size<i32, Physical> = (size.w, size.h).into();
        let mut target = renderer.bind(&mut self.textures[to])?;
        let mut frame = renderer.render(&mut target, size, Transform::Normal)?;
        let all = Rectangle::from_size(size);
        let halfpixel = [0.5 / size.w as f32, 0.5 / size.h as f32];
        frame.render_texture_from_to(
            &source,
            Rectangle::from_size(source.size()).to_f64(),
            all,
            &[all],
            &[all],
            Transform::Normal,
            1.0,
            Some(program),
            &[
                Uniform::new("halfpixel", halfpixel),
                Uniform::new("offset", OFFSET),
            ],
        )?;
        // Later passes and the final draw run on the same context, in order: no need to
        // wait for this one.
        let _ = frame.finish()?;
        Ok(())
    }
}

/// Copy `area` of the bound framebuffer (framebuffer pixels) into `texture`, which is
/// exactly that size.
///
/// # Safety
/// The GL context must be current with the frame's framebuffer bound, and `texture` must
/// be a live texture of `area.size`.
#[allow(unsafe_code)] // raw GL (Smithay has no API to read the bound framebuffer)
unsafe fn copy_from_framebuffer(gl: &ffi::Gles2, area: Rectangle<i32, Physical>, texture: u32) {
    // SAFETY: plain GL calls on the current context (the caller's contract); everything
    // changed is restored: the draw binding, the scissor test, the temporary FBO.
    unsafe {
        let mut previous = 0;
        gl.GetIntegerv(ffi::DRAW_FRAMEBUFFER_BINDING, &mut previous);
        let mut fbo = 0;
        gl.GenFramebuffers(1, &mut fbo);
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, fbo);
        gl.FramebufferTexture2D(
            ffi::DRAW_FRAMEBUFFER,
            ffi::COLOR_ATTACHMENT0,
            ffi::TEXTURE_2D,
            texture,
            0,
        );
        // The scissor test applies to blits; the frame leaves it on.
        gl.Disable(ffi::SCISSOR_TEST);
        let (x, y, w, h) = (area.loc.x, area.loc.y, area.size.w, area.size.h);
        gl.BlitFramebuffer(
            x,
            y,
            x + w,
            y + h,
            0,
            0,
            w,
            h,
            ffi::COLOR_BUFFER_BIT,
            ffi::NEAREST,
        );
        gl.Enable(ffi::SCISSOR_TEST);
        // `previous` came from GL as a GLint; framebuffer names are never negative.
        gl.BindFramebuffer(ffi::DRAW_FRAMEBUFFER, previous.unsigned_abs());
        gl.DeleteFramebuffers(1, &fbo);
    }
}

/// The two dual-Kawase shaders, compiled once per GL context.
#[derive(Debug, Clone)]
struct Programs {
    down: GlesTexProgram,
    up: GlesTexProgram,
}

impl Programs {
    fn get(renderer: &mut GlesRenderer) -> Result<Self, GlesError> {
        if let Some(programs) = renderer.egl_context().user_data().get::<Self>() {
            return Ok(programs.clone());
        }
        let uniforms = [
            UniformName::new("halfpixel", UniformType::_2f),
            UniformName::new("offset", UniformType::_1f),
        ];
        let programs = Self {
            down: renderer.compile_custom_texture_shader(shader(DOWN), &uniforms)?,
            up: renderer.compile_custom_texture_shader(shader(UP), &uniforms)?,
        };
        let stored = programs.clone();
        renderer
            .egl_context()
            .user_data()
            .insert_if_missing(move || stored);
        Ok(programs)
    }
}

/// A texture shader for Smithay's renderer (its `//_DEFINES_` and variables; see
/// `GlesRenderer::compile_custom_texture_shader`) whose colour is `body`.
fn shader(body: &str) -> String {
    format!(
        r"#version 100
//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
uniform vec2 halfpixel;
uniform float offset;

void main() {{
    vec2 uv = v_coords;
    vec2 o = halfpixel * offset;
    vec4 color = {body};
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif
    gl_FragColor = color * alpha;
}}
"
    )
}

/// Dual-Kawase downsample (Bjørge, SIGGRAPH 2015): the centre and four diagonals.
const DOWN: &str = "(texture2D(tex, uv) * 4.0
        + texture2D(tex, uv - o)
        + texture2D(tex, uv + o)
        + texture2D(tex, uv + vec2(o.x, -o.y))
        + texture2D(tex, uv - vec2(o.x, -o.y))) / 8.0";

/// Dual-Kawase upsample: four axis points and four diagonals at double weight.
const UP: &str = "(texture2D(tex, uv + vec2(-o.x * 2.0, 0.0))
        + texture2D(tex, uv + vec2(-o.x, o.y)) * 2.0
        + texture2D(tex, uv + vec2(0.0, o.y * 2.0))
        + texture2D(tex, uv + vec2(o.x, o.y)) * 2.0
        + texture2D(tex, uv + vec2(o.x * 2.0, 0.0))
        + texture2D(tex, uv + vec2(o.x, -o.y)) * 2.0
        + texture2D(tex, uv + vec2(0.0, -o.y * 2.0))
        + texture2D(tex, uv + vec2(-o.x, -o.y)) * 2.0) / 12.0";

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangle kind and its x, y, width and height.
    type Part = (RectangleKind, (i32, i32, i32, i32));

    fn region(rects: &[Part]) -> RegionAttributes {
        RegionAttributes {
            rects: rects
                .iter()
                .map(|(kind, (x, y, w, h))| {
                    (*kind, Rectangle::new((*x, *y).into(), (*w, *h).into()))
                })
                .collect(),
        }
    }

    fn area(rects: &[Rectangle<i32, Logical>]) -> i32 {
        rects.iter().map(|r| r.size.w * r.size.h).sum()
    }

    #[test]
    fn overlapping_adds_become_disjoint() {
        let rects = disjoint_rects(&region(&[
            (RectangleKind::Add, (0, 0, 10, 10)),
            (RectangleKind::Add, (5, 5, 10, 10)),
        ]));
        assert_eq!(area(&rects), 100 + 100 - 25);
    }

    #[test]
    fn subtract_cuts_a_hole() {
        let rects = disjoint_rects(&region(&[
            (RectangleKind::Add, (0, 0, 10, 10)),
            (RectangleKind::Subtract, (2, 2, 6, 6)),
        ]));
        assert_eq!(area(&rects), 100 - 36);
        let hole = Rectangle::new((2, 2).into(), (6, 6).into());
        assert!(rects.iter().all(|r| !r.overlaps(hole)), "{rects:?}");
    }

    #[test]
    fn reach_doubles_with_a_pass() {
        assert_eq!(reach(passes(1.0)), 32);
        assert_eq!(reach(passes(2.0)), 64);
    }

    #[test]
    fn element_is_the_region_grown_by_the_reach_and_clipped() {
        let blur = SurfaceBlur {
            surface: Id::new(),
            blur: Id::new(),
            rects: vec![Rectangle::new((10, 10).into(), (100, 20).into())],
        };
        let surface = Rectangle::new((0, 0).into(), (200, 100).into());
        let element = BlurElement::new(&blur, surface, 1.0, (1000, 1000).into()).unwrap();
        // Grown by 32, clipped at the output's top-left corner.
        assert_eq!(
            element.geometry,
            Rectangle::new((0, 0).into(), (142, 62).into())
        );
        assert_eq!(
            element.rects,
            [Rectangle::new((10, 10).into(), (100, 20).into())]
        );

        let moved = Rectangle::new((300, 300).into(), (200, 100).into());
        let element = BlurElement::new(&blur, moved, 1.0, (1000, 1000).into()).unwrap();
        assert_eq!(
            element.geometry,
            Rectangle::new((278, 278).into(), (164, 84).into())
        );
        assert_eq!(
            element.rects,
            [Rectangle::new((32, 32).into(), (100, 20).into())]
        );
    }

    #[test]
    fn region_outside_the_surface_has_no_element() {
        let blur = SurfaceBlur {
            surface: Id::new(),
            blur: Id::new(),
            rects: vec![Rectangle::new((500, 500).into(), (10, 10).into())],
        };
        let surface = Rectangle::new((0, 0).into(), (200, 100).into());
        assert!(BlurElement::new(&blur, surface, 1.0, (1000, 1000).into()).is_none());
    }
}
