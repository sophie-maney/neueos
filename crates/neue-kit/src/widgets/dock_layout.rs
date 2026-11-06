//! Where everything on the dock goes (tasks 3.16 and 3.17a): the surface's size, the
//! icons at rest and magnified under the pointer, on the bottom, left or right edge.
//!
//! The layout is worked out along two axes, whatever the edge: `u` runs along the dock
//! (left to right at the bottom, top to bottom on the sides) and `v` away from the screen
//! edge it stands on. [`DockLayout::surface_rect`] turns that into the surface's own
//! coordinates.
//!
//! Magnification grows each icon by a raised cosine of its distance from the pointer
//! ([`bell`]), reaching [`MAGNIFY_REACH`] tiles to either side. Over a long row that adds up
//! to the same `MAGNIFY_REACH` icons' worth wherever the pointer is, so the surface keeps
//! that much room at each end. The icons spread around the pointer so it keeps its place
//! in the tile under it: the pointer is over the magnified dock exactly when it is over
//! the dock at rest, so the input region can stay the dock at rest.

use masonry_core::kurbo::{Affine, Insets, Point, Rect, Size, Vec2};
use neue_theme::materials::dock::{
    BOUNCE_ROOM, DRAG_ROOM, FLAT_INSET, ICON, ICON_BASE, ICON_RANGE, MAGNIFIED_MAX, MAGNIFY_REACH,
    REMOVE_DISTANCE, SIDE_PADDING, SPACING,
};
use neue_theme::materials::dock_shelf::{DEPTH, FLAT_RADIUS};

/// The screen edge the dock stands on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DockEdge {
    /// Along the bottom, on a shelf seen in perspective.
    #[default]
    Bottom,
    /// Down the left edge, on a flat shelf.
    Left,
    /// Down the right edge, on a flat shelf.
    Right,
}

/// The dock's shape: its edge, its icons' size and how far they grow under the pointer.
/// Everything else (the surface's size, its exclusive zone, where the pointer passes
/// through, where each icon is) follows from it and the tile count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockLayout {
    pub edge: DockEdge,
    /// An icon's size at rest, in logical pixels.
    pub icon: f64,
    /// The size an icon grows to under the pointer; `None` (or no bigger than
    /// [`Self::icon`]): no magnification.
    pub magnified: Option<f64>,
}

impl Default for DockLayout {
    /// Today's dock: at the bottom, [`ICON`]-sized icons, no magnification.
    fn default() -> Self {
        Self {
            edge: DockEdge::Bottom,
            icon: ICON,
            magnified: None,
        }
    }
}

/// Where the tiles are for a pointer and a magnification: from [`DockLayout::place`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DockPlacement {
    /// Each icon at its size, standing on the shelf (before any bounce), in the surface's
    /// coordinates.
    pub icons: Vec<Rect>,
    /// The stretch of `u` each tile takes: its icon and half the spacing on either side.
    pub(crate) spans: Vec<(f64, f64)>,
    /// The shelf's ends along `u`.
    pub(crate) shelf: (f64, f64),
}

impl DockPlacement {
    /// The tile whose stretch of the dock `u` is in.
    pub(crate) fn tile_at(&self, u: f64) -> Option<usize> {
        self.spans.iter().position(|&(a, b)| (a..b).contains(&u))
    }
}

/// How much an icon `d` tiles from the pointer grows, from 1 under it to 0 at
/// [`MAGNIFY_REACH`] and beyond.
pub(crate) fn bell(d: f64) -> f64 {
    if d.abs() >= MAGNIFY_REACH {
        0.0
    } else {
        0.5 * (1.0 + (std::f64::consts::PI * d / MAGNIFY_REACH).cos())
    }
}

impl DockLayout {
    /// A layout with `icon` and `magnified` brought into range ([`ICON_RANGE`],
    /// [`MAGNIFIED_MAX`]); a magnified size no bigger than the icon turns magnification
    /// off.
    pub fn new(edge: DockEdge, icon: f64, magnified: Option<f64>) -> Self {
        let icon = if icon.is_finite() {
            icon.clamp(ICON_RANGE.0, ICON_RANGE.1)
        } else {
            ICON
        };
        let magnified = magnified
            .filter(|m| m.is_finite() && *m > icon)
            .map(|m| m.min(MAGNIFIED_MAX));
        Self {
            edge,
            icon,
            magnified,
        }
    }

    /// Whether icons grow under the pointer.
    pub fn magnifies(&self) -> bool {
        self.growth() > 0.0
    }

    /// How much an icon right under the pointer grows.
    fn growth(&self) -> f64 {
        self.magnified.map_or(0.0, |m| (m - self.icon).max(0.0))
    }

    /// Room kept at each end of the dock for it to grow into.
    fn reserve(&self) -> f64 {
        (self.growth() * MAGNIFY_REACH).ceil()
    }

    pub(crate) fn pitch(&self) -> f64 {
        self.icon + SPACING
    }

    /// The icons' size against [`ICON`], the size the bounce's metrics
    /// (`BOUNCE_HEIGHT`, `STILL_LIFT`) are for.
    pub(crate) fn scale(&self) -> f64 {
        self.icon / ICON
    }

    /// The room beyond the icons for a bounce.
    fn bounce_room(&self) -> f64 {
        (BOUNCE_ROOM * self.scale()).ceil()
    }

    /// Where the icons' bottoms stand, from the screen edge.
    fn base(&self) -> f64 {
        match self.edge {
            DockEdge::Bottom => DEPTH - ICON_BASE,
            DockEdge::Left | DockEdge::Right => FLAT_INSET,
        }
    }

    /// The shelf's depth, from the screen edge.
    fn shelf_depth(&self) -> f64 {
        match self.edge {
            DockEdge::Bottom => DEPTH,
            DockEdge::Left | DockEdge::Right => self.icon + 2.0 * FLAT_INSET,
        }
    }

    /// How far the dock at rest (the shelf and its icons) reaches from the screen edge: the
    /// exclusive zone, and the depth of the input region.
    pub fn band(&self) -> f64 {
        self.shelf_depth().max(self.base() + self.icon)
    }

    /// The surface's depth: the band, room for a magnified icon and for a bounce, and at
    /// least [`DRAG_ROOM`] for an icon dragged off the dock.
    fn depth(&self) -> f64 {
        let drag_room = (DRAG_ROOM * self.scale()).ceil();
        self.base() + self.icon + (self.growth() + self.bounce_room()).max(drag_room)
    }

    /// The dock's length at rest.
    fn rest_length(&self, count: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "a handful of tiles")]
        let n = count as f64;
        let tiles = if count == 0 {
            0.0
        } else {
            n * self.icon + (n - 1.0) * SPACING
        };
        2.0 * SIDE_PADDING + tiles
    }

    /// The surface's length along the dock: the dock at rest and the room at its ends.
    fn length(&self, count: usize) -> f64 {
        self.rest_length(count) + 2.0 * self.reserve()
    }

    /// The surface's size with `count` tiles.
    pub fn size(&self, count: usize) -> Size {
        let (u, v) = (self.length(count), self.depth());
        match self.edge {
            DockEdge::Bottom => Size::new(u, v),
            DockEdge::Left | DockEdge::Right => Size::new(v, u),
        }
    }

    /// What the pointer passes through at each side of the surface: the room for the
    /// bounce and for magnified icons beyond the band, and the room at the dock's ends.
    pub fn pass_through(&self) -> Insets {
        let (ends, beyond) = (self.reserve(), self.depth() - self.band());
        match self.edge {
            DockEdge::Bottom => Insets::new(ends, beyond, ends, 0.0),
            DockEdge::Left => Insets::new(0.0, ends, beyond, ends),
            DockEdge::Right => Insets::new(beyond, ends, 0.0, ends),
        }
    }

    /// `du` along the dock and `dv` away from the screen edge, in the surface.
    pub(crate) fn along(&self, du: f64, dv: f64) -> Vec2 {
        match self.edge {
            DockEdge::Bottom => Vec2::new(du, -dv),
            DockEdge::Left => Vec2::new(dv, du),
            DockEdge::Right => Vec2::new(-dv, du),
        }
    }

    /// The middle of the surface with `count` tiles, along the dock: where the dock's
    /// centre stays on the screen whatever its length.
    pub(crate) fn middle(&self, count: usize) -> f64 {
        self.length(count) / 2.0
    }

    /// Whether an icon dragged with the pointer at `p` (in the surface) leaves the dock
    /// when dropped: the pointer is more than [`REMOVE_DISTANCE`] beyond the band.
    pub(crate) fn is_off(&self, p: Point) -> bool {
        self.frame_of(p).1 > self.band() + REMOVE_DISTANCE * self.scale()
    }

    /// Where an icon dragged with the pointer at `p` (in the surface) is drawn: at its
    /// size at rest, the pointer `grab` of the way across it (along the dock, away from
    /// the edge), kept inside the surface of `count` tiles.
    pub(crate) fn dragged(&self, count: usize, p: Point, grab: (f64, f64)) -> Rect {
        let (u, v) = self.frame_of(p);
        let size = self.icon;
        let u0 = (u - grab.0 * size).clamp(0.0, (self.length(count) - size).max(0.0));
        let v0 = (v - grab.1 * size).clamp(0.0, self.depth() - size);
        self.surface_rect((u0, u0 + size), (v0, v0 + size))
    }

    /// Moves `placement` by `du` along the dock.
    pub(crate) fn shift(&self, placement: &mut DockPlacement, du: f64) {
        let by = self.along(du, 0.0);
        for icon in &mut placement.icons {
            *icon = *icon + by;
        }
        for span in &mut placement.spans {
            *span = (span.0 + du, span.1 + du);
        }
        placement.shelf = (placement.shelf.0 + du, placement.shelf.1 + du);
    }

    /// `p` (in the surface) along the dock and away from the screen edge.
    pub(crate) fn frame_of(&self, p: Point) -> (f64, f64) {
        let depth = self.depth();
        match self.edge {
            DockEdge::Bottom => (p.x, depth - p.y),
            DockEdge::Left => (p.y, p.x),
            DockEdge::Right => (p.y, depth - p.x),
        }
    }

    /// The rect from `u0` to `u1` along the dock and `v0` to `v1` from the screen edge, in
    /// the surface.
    fn surface_rect(&self, (u0, u1): (f64, f64), (v0, v1): (f64, f64)) -> Rect {
        let depth = self.depth();
        match self.edge {
            DockEdge::Bottom => Rect::new(u0, depth - v1, u1, depth - v0),
            DockEdge::Left => Rect::new(v0, u0, v1, u1),
            DockEdge::Right => Rect::new(depth - v1, u0, depth - v0, u1),
        }
    }

    /// Which way an icon moves when it bounces, one pixel's worth.
    pub(crate) fn outwards(&self) -> Vec2 {
        match self.edge {
            DockEdge::Bottom => Vec2::new(0.0, -1.0),
            DockEdge::Left => Vec2::new(1.0, 0.0),
            DockEdge::Right => Vec2::new(-1.0, 0.0),
        }
    }

    /// Whether `p` (in the surface) is on the dock at rest: along the shelf and no
    /// further from the screen edge than the band. Magnification follows the pointer
    /// there.
    pub(crate) fn is_over(&self, count: usize, p: Point) -> bool {
        let (u, v) = self.frame_of(p);
        let start = self.reserve();
        (start..=start + self.rest_length(count)).contains(&u) && (0.0..=self.band()).contains(&v)
    }

    /// Where the tiles are at rest.
    pub fn rest(&self, count: usize) -> DockPlacement {
        self.place(count, None, 0.0)
    }

    /// Where the tiles are with the pointer at `pointer` along the dock (`u`) and
    /// magnification `amount` of the way in (0 at rest, 1 in full).
    pub fn place(&self, count: usize, pointer: Option<f64>, amount: f64) -> DockPlacement {
        let pitch = self.pitch();
        let end_room = SIDE_PADDING - SPACING / 2.0;
        // Where the first tile's stretch begins.
        let start = self.reserve() + end_room;
        #[expect(clippy::cast_precision_loss, reason = "a handful of tiles")]
        let centre = |i: usize| start + pitch * (i as f64 + 0.5);
        let grow = self.growth() * amount.clamp(0.0, 1.0);
        let extra: Vec<f64> = (0..count)
            .map(|i| match pointer {
                Some(p) if grow > 0.0 => grow * bell((p - centre(i)) / pitch),
                _ => 0.0,
            })
            .collect();
        // How much the row grows before the pointer: the tile under it keeps the
        // pointer at the same share of its stretch.
        #[expect(clippy::cast_precision_loss, reason = "a handful of tiles")]
        let end = start + pitch * count as f64;
        let before = match pointer {
            Some(p) if count > 0 && p > start => {
                if p >= end {
                    extra.iter().sum()
                } else {
                    let at = (p - start) / pitch;
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a tile index inside the row"
                    )]
                    let k = (at.floor() as usize).min(count - 1);
                    #[expect(clippy::cast_precision_loss, reason = "a handful of tiles")]
                    let t = at - k as f64;
                    extra[..k].iter().sum::<f64>() + t * extra[k]
                }
            }
            _ => 0.0,
        };
        let mut spans = Vec::with_capacity(count);
        let mut icons = Vec::with_capacity(count);
        let mut at = start - before;
        let base = self.base();
        for e in &extra {
            let span = (at, at + pitch + e);
            let size = self.icon + e;
            let mid = (span.0 + span.1) / 2.0;
            icons
                .push(self.surface_rect((mid - size / 2.0, mid + size / 2.0), (base, base + size)));
            spans.push(span);
            at = span.1;
        }
        let shelf = (start - before - end_room, at + end_room);
        DockPlacement {
            icons,
            spans,
            shelf,
        }
    }

    /// The transform that puts what is drawn for a dock at the bottom of a surface of
    /// [`Self::size`] in place on this edge: none at the bottom, a quarter turn on the
    /// left, a mirror image of that on the right (the shelf is the same at both ends).
    pub(crate) fn frame_transform(&self) -> Affine {
        let depth = self.depth();
        match self.edge {
            DockEdge::Bottom => Affine::IDENTITY,
            // (x, y) → (depth − y, x)
            DockEdge::Left => Affine::new([0.0, 1.0, -1.0, 0.0, depth, 0.0]),
            // (x, y) → (y, x)
            DockEdge::Right => Affine::new([0.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        }
    }

    /// The shelf, drawn as if at the bottom (through [`Self::frame_transform`]): in
    /// perspective at the bottom; flat on the sides, its outer corners past the screen
    /// edge.
    pub(crate) fn shelf_rect(&self, placement: &DockPlacement) -> Rect {
        let depth = self.depth();
        let (u0, u1) = placement.shelf;
        match self.edge {
            DockEdge::Bottom => Rect::new(u0, depth - DEPTH, u1, depth),
            DockEdge::Left | DockEdge::Right => {
                Rect::new(u0, depth - self.shelf_depth(), u1, depth + FLAT_RADIUS)
            }
        }
    }

    /// The line the icons stand on, drawn as if at the bottom: their reflections mirror
    /// about it.
    pub(crate) fn base_line(&self) -> f64 {
        self.depth() - self.base()
    }

    /// The bottom of what is drawn as if at the bottom (the screen edge).
    pub(crate) fn frame_depth(&self) -> f64 {
        self.depth()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGNIFIED: DockLayout = DockLayout {
        edge: DockEdge::Bottom,
        icon: 48.0,
        magnified: Some(96.0),
    };

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_default_is_the_dock_of_3_16_with_room_to_drag() {
        let layout = DockLayout::default();
        let size = layout.size(3);
        // The bounce needs less room than a dragged icon.
        const { assert!(BOUNCE_ROOM < DRAG_ROOM) };
        assert_eq!(
            size,
            Size::new(2.0 * 22.0 + 3.0 * 48.0 + 2.0 * 4.0, DRAG_ROOM + 48.0 + 18.0)
        );
        let rest = layout.rest(3);
        assert_eq!(
            rest.icons[0],
            Rect::new(22.0, DRAG_ROOM, 70.0, DRAG_ROOM + 48.0)
        );
        assert_eq!(rest.icons[1].x0 - rest.icons[0].x1, SPACING);
        assert_eq!(rest.icons[2].x1 + SIDE_PADDING, size.width);
        assert_eq!(layout.band(), size.height - DRAG_ROOM);
        assert_eq!(layout.pass_through(), Insets::new(0.0, DRAG_ROOM, 0.0, 0.0));
        assert!(!layout.magnifies());
    }

    #[test]
    fn a_dragged_icon_stays_in_the_surface_and_leaves_past_the_band() {
        let layout = DockLayout::default();
        let size = layout.size(3);
        let band_top = size.height - layout.band();
        // Held by its middle, the icon is centred on the pointer.
        let p = Point::new(100.0, band_top + 10.0);
        let icon = layout.dragged(3, p, (0.5, 0.5));
        assert_eq!(icon.center(), p);
        assert!(!layout.is_off(p));
        // Far above and past the end: kept in the surface, off the dock.
        let far = Point::new(size.width + 50.0, -200.0);
        let icon = layout.dragged(3, far, (0.5, 0.5));
        assert_eq!(icon, Rect::new(size.width - 48.0, 0.0, size.width, 48.0));
        assert!(layout.is_off(far));
        assert!(!layout.is_off(Point::new(0.0, band_top - REMOVE_DISTANCE + 1.0)));
        assert!(layout.is_off(Point::new(0.0, band_top - REMOVE_DISTANCE - 1.0)));
        // Off the left edge's band, outwards is to the right.
        let left = DockLayout {
            edge: DockEdge::Left,
            ..DockLayout::default()
        };
        let beyond = left.band() + REMOVE_DISTANCE + 1.0;
        assert!(left.is_off(Point::new(beyond, 30.0)));
        assert!(!left.is_off(Point::new(left.band(), 30.0)));
    }

    #[test]
    fn shifting_moves_icons_spans_and_shelf_along_the_dock() {
        for edge in [DockEdge::Bottom, DockEdge::Left] {
            let layout = DockLayout {
                edge,
                ..DockLayout::default()
            };
            let rest = layout.rest(2);
            let mut shifted = rest.clone();
            layout.shift(&mut shifted, 26.0);
            assert_eq!(shifted.icons[1], rest.icons[1] + layout.along(26.0, 0.0));
            assert_eq!(shifted.spans[0].0, rest.spans[0].0 + 26.0);
            assert_eq!(shifted.shelf.1, rest.shelf.1 + 26.0);
            // The middle of one tile fewer, shifted half a pitch, is the middle of all.
            let mut fewer = layout.rest(1);
            layout.shift(&mut fewer, layout.pitch() / 2.0);
            let mid = |p: &DockPlacement| (p.shelf.0 + p.shelf.1) / 2.0;
            assert!(close(mid(&fewer), layout.middle(2)), "{edge:?}");
        }
    }

    #[test]
    fn the_bell_peaks_under_the_pointer_and_sums_to_the_reach() {
        assert_eq!(bell(0.0), 1.0);
        assert_eq!(bell(MAGNIFY_REACH), 0.0);
        assert!(bell(1.0) > bell(2.0) && bell(2.0) > 0.0);
        assert!(close(bell(-1.3), bell(1.3)));
        // Over a long row the icons grow by the same amount wherever the pointer is.
        for step in 0..10 {
            let phase = f64::from(step) / 10.0;
            let sum: f64 = (-10..=10).map(|k| bell(f64::from(k) + phase)).sum();
            assert!(close(sum, MAGNIFY_REACH), "{phase}: {sum}");
        }
    }

    #[test]
    fn magnification_grows_the_icon_under_the_pointer_most() {
        let rest = MAGNIFIED.rest(5);
        let under = rest.icons[2].center().x;
        let placed = MAGNIFIED.place(5, Some(under), 1.0);
        let sizes: Vec<f64> = placed.icons.iter().map(Rect::width).collect();
        assert!(close(sizes[2], 96.0), "{sizes:?}");
        assert!(close(sizes[1], sizes[3]) && sizes[1] < 96.0 && sizes[1] > 48.0);
        assert!(sizes[0] < sizes[1]);
        // Grown away from the edge: the bottoms stay on the shelf.
        assert!(placed.icons.iter().all(|r| close(r.y1, rest.icons[0].y1)));
        // The pointer keeps its place: the middle icon stays centred under it.
        assert!(close(placed.icons[2].center().x, under));
        // Half way in, half the growth.
        let half = MAGNIFIED.place(5, Some(under), 0.5);
        assert!(close(half.icons[2].width(), 72.0));
    }

    #[test]
    fn the_pointer_keeps_its_share_of_its_tile() {
        let rest = MAGNIFIED.rest(6);
        for p in [
            rest.spans[1].0 + 3.0,
            rest.spans[3].1 - 10.0,
            rest.spans[4].0 + 26.0,
        ] {
            let i = rest.tile_at(p).unwrap();
            let share = |s: (f64, f64)| (p - s.0) / (s.1 - s.0);
            let placed = MAGNIFIED.place(6, Some(p), 1.0);
            assert_eq!(placed.tile_at(p), Some(i));
            assert!(close(share(placed.spans[i]), share(rest.spans[i])));
        }
    }

    #[test]
    fn the_layout_moves_smoothly_with_the_pointer() {
        let rest = MAGNIFIED.rest(4);
        let (from, to) = (rest.shelf.0 - 5.0, rest.shelf.1 + 5.0);
        let mut last = MAGNIFIED.place(4, Some(from), 1.0);
        let mut p = from;
        while p < to {
            p += 0.25;
            let now = MAGNIFIED.place(4, Some(p), 1.0);
            for (a, b) in now.icons.iter().zip(&last.icons) {
                assert!(
                    (a.x0 - b.x0).abs() < 1.0 && (a.x1 - b.x1).abs() < 1.0,
                    "{p}"
                );
            }
            last = now;
        }
    }

    #[test]
    fn magnified_docks_stay_inside_the_surface() {
        for edge in [DockEdge::Bottom, DockEdge::Left, DockEdge::Right] {
            let layout = DockLayout { edge, ..MAGNIFIED };
            for count in [1, 3, 12] {
                let bounds = Rect::from_origin_size(Point::ZERO, layout.size(count));
                let rest = layout.rest(count);
                let mut p = rest.shelf.0 - 20.0;
                while p < rest.shelf.1 + 20.0 {
                    let placed = layout.place(count, Some(p), 1.0);
                    for icon in &placed.icons {
                        assert!(bounds.contains_rect(*icon), "{edge:?} {count} {p}");
                    }
                    let shelf = layout
                        .frame_transform()
                        .transform_rect_bbox(layout.shelf_rect(&placed));
                    let along = match edge {
                        DockEdge::Bottom => (shelf.x0, shelf.x1, bounds.x1),
                        _ => (shelf.y0, shelf.y1, bounds.y1),
                    };
                    assert!(
                        along.0 >= -1e-9 && along.1 <= along.2 + 1e-9,
                        "{edge:?} {p}"
                    );
                    p += 7.0;
                }
            }
        }
    }

    #[test]
    fn side_docks_run_down_the_edge() {
        let left = DockLayout {
            edge: DockEdge::Left,
            ..DockLayout::default()
        };
        let size = left.size(3);
        assert_eq!(size.width, FLAT_INSET + 48.0 + DRAG_ROOM);
        assert_eq!(size.height, DockLayout::default().size(3).width);
        let rest = left.rest(3);
        assert_eq!(
            rest.icons[0],
            Rect::new(FLAT_INSET, 22.0, FLAT_INSET + 48.0, 70.0)
        );
        assert_eq!(left.band(), 48.0 + 2.0 * FLAT_INSET);
        assert_eq!(
            left.pass_through(),
            Insets::new(0.0, 0.0, size.width - left.band(), 0.0)
        );
        assert_eq!(left.outwards(), Vec2::new(1.0, 0.0));

        let right = DockLayout {
            edge: DockEdge::Right,
            ..left
        };
        let icon = right.rest(3).icons[0];
        assert_eq!(icon.x1, size.width - FLAT_INSET);
        assert_eq!(icon.y0, 22.0);
        assert_eq!(right.pass_through().x0, size.width - right.band());
        // The shelf is drawn as at the bottom and turned: it ends up against the edge.
        for layout in [left, right] {
            let shelf = layout
                .frame_transform()
                .transform_rect_bbox(layout.shelf_rect(&layout.rest(3)));
            assert!(close(shelf.height(), size.height), "{shelf:?}");
            let against = if layout.edge == DockEdge::Left {
                close(shelf.x1, left.band())
            } else {
                close(shelf.x0, size.width - right.band())
            };
            assert!(against, "{:?} {shelf:?}", layout.edge);
        }
    }

    #[test]
    fn the_input_region_is_the_dock_at_rest() {
        let layout = DockLayout {
            edge: DockEdge::Right,
            ..MAGNIFIED
        };
        let size = layout.size(4);
        let insets = layout.pass_through();
        let input = Rect::from_origin_size(Point::ZERO, size) - insets;
        let rest = layout.rest(4);
        let along = rest.shelf;
        assert!(
            close(input.y0, along.0) && close(input.y1, along.1),
            "{input:?}"
        );
        assert!(layout.is_over(4, input.center()));
        assert!(!layout.is_over(4, Point::new(input.center().x, input.y0 - 1.0)));
        assert!(!layout.is_over(4, Point::new(input.x0 - 1.0, input.center().y)));
    }

    #[test]
    fn sizes_are_brought_into_range() {
        let layout = DockLayout::new(DockEdge::Bottom, 4.0, Some(10.0));
        assert_eq!(layout.icon, ICON_RANGE.0);
        assert_eq!(layout.magnified, None);
        let layout = DockLayout::new(DockEdge::Bottom, 64.0, Some(1000.0));
        assert_eq!(layout.magnified, Some(MAGNIFIED_MAX));
        assert_eq!(DockLayout::new(DockEdge::Left, f64::NAN, None).icon, ICON);
    }
}
