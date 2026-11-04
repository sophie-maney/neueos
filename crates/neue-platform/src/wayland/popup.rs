//! Where a Masonry layer's xdg-popup goes (task 2.6, ADR 0034 §1).
//!
//! Masonry only gives a layer kind and a point in window coordinates. The driver turns
//! that into an `xdg_positioner`, so the compositor can flip or slide the popup at the
//! screen edges:
//! - **menus** (`Selector`, `Other`) opened under a widget (the selector puts its menu at
//!   its own bottom-left corner) anchor to that widget's border box and flip above it;
//!   opened anywhere else (context menus) they anchor to the point and flip on both axes.
//!   Menus grab the seat: keyboard focus, and the compositor dismisses them on an outside
//!   click;
//! - **tooltips** anchor to the point, flip above it, never grab and let the pointer
//!   through.

use masonry_core::core::{LayerType, Widget, WidgetRef};
use masonry_core::kurbo::{Point, Rect, Vec2};
use smithay_client_toolkit::error::GlobalError;
use smithay_client_toolkit::shell::xdg::{XdgPositioner, XdgShell};
use wayland_client::Proxy as _;
use wayland_protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity,
};

/// A rectangle in whole logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IRect {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

impl IRect {
    /// The smallest whole-pixel rectangle covering `r`, at least 1×1.
    fn covering(r: Rect) -> Self {
        let r = r.expand();
        Self {
            x: r.x0 as i32,
            y: r.y0 as i32,
            width: (r.width() as i32).max(1),
            height: (r.height() as i32).max(1),
        }
    }

    fn point(p: Point) -> Self {
        Self {
            x: p.x.floor() as i32,
            y: p.y.floor() as i32,
            width: 1,
            height: 1,
        }
    }

    pub(super) fn translate(self, by: Vec2) -> Self {
        Self {
            x: self.x + by.x.round() as i32,
            y: self.y + by.y.round() as i32,
            ..self
        }
    }
}

/// An xdg-popup's positioner rules, in the parent window's logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Placement {
    pub(super) anchor_rect: IRect,
    pub(super) anchor: Anchor,
    pub(super) gravity: Gravity,
    pub(super) constraints: ConstraintAdjustment,
    /// The popup's top-left relative to the anchor point.
    pub(super) offset: (i32, i32),
    pub(super) size: (i32, i32),
    /// Takes an explicit grab (menus). Tooltips don't, and get an empty input region.
    pub(super) grab: bool,
}

impl Placement {
    /// Placement for a layer of `kind` that Masonry put at `pos`, painting `bbox` (both in
    /// window coordinates). `owner` is the border box of the widget whose bottom-left
    /// corner is `pos`, if there is one.
    pub(super) fn new(kind: &LayerType, pos: Point, bbox: Rect, owner: Option<Rect>) -> Self {
        let size = IRect::covering(bbox);
        let size = (size.width, size.height);
        let offset_from = |anchor: (i32, i32)| {
            (
                (bbox.x0 - f64::from(anchor.0)).round() as i32,
                (bbox.y0 - f64::from(anchor.1)).round() as i32,
            )
        };
        match (kind, owner) {
            (LayerType::Tooltip(_), _) => {
                let anchor_rect = IRect::point(pos);
                Self {
                    anchor_rect,
                    anchor: Anchor::TopLeft,
                    gravity: Gravity::BottomRight,
                    constraints: ConstraintAdjustment::FlipY
                        | ConstraintAdjustment::SlideX
                        | ConstraintAdjustment::SlideY,
                    offset: offset_from((anchor_rect.x, anchor_rect.y)),
                    size,
                    grab: false,
                }
            }
            (_, Some(owner)) => {
                let anchor_rect = IRect::covering(owner);
                let corner = (anchor_rect.x, anchor_rect.y + anchor_rect.height);
                Self {
                    anchor_rect,
                    anchor: Anchor::BottomLeft,
                    gravity: Gravity::BottomRight,
                    constraints: ConstraintAdjustment::FlipY
                        | ConstraintAdjustment::SlideX
                        | ConstraintAdjustment::SlideY,
                    offset: offset_from(corner),
                    size,
                    grab: true,
                }
            }
            (_, None) => {
                let anchor_rect = IRect::point(pos);
                Self {
                    anchor_rect,
                    anchor: Anchor::TopLeft,
                    gravity: Gravity::BottomRight,
                    constraints: ConstraintAdjustment::FlipX
                        | ConstraintAdjustment::FlipY
                        | ConstraintAdjustment::SlideX
                        | ConstraintAdjustment::SlideY,
                    offset: offset_from((anchor_rect.x, anchor_rect.y)),
                    size,
                    grab: true,
                }
            }
        }
    }

    /// The same placement for a layer Masonry moved by `delta`.
    pub(super) fn moved(self, delta: Vec2) -> Self {
        Self {
            anchor_rect: self.anchor_rect.translate(delta),
            ..self
        }
    }

    /// A positioner for this placement, relative to a parent surface at `parent_origin`
    /// in window coordinates (the window itself, or the menu a submenu opens from).
    pub(super) fn positioner(
        &self,
        xdg: &XdgShell,
        parent_origin: Vec2,
    ) -> Result<XdgPositioner, GlobalError> {
        let positioner = XdgPositioner::new(xdg)?;
        let r = self.anchor_rect.translate(-parent_origin);
        positioner.set_size(self.size.0, self.size.1);
        positioner.set_anchor_rect(r.x, r.y, r.width, r.height);
        positioner.set_anchor(self.anchor);
        positioner.set_gravity(self.gravity);
        positioner.set_constraint_adjustment(self.constraints);
        positioner.set_offset(self.offset.0, self.offset.1);
        if positioner.version() >= 3 {
            // Placed again when the parent moves or resizes.
            positioner.set_reactive();
        }
        Ok(positioner)
    }
}

/// The border box, in window coordinates, of the widget whose bottom-left corner is at
/// `pos` (within a pixel): the widget a menu opened under. The outermost match wins.
pub(super) fn owner_rect(widget: WidgetRef<'_, dyn Widget>, pos: Point) -> Option<Rect> {
    // Just inside the owner's bottom-left corner.
    let probe = Point::new(pos.x + 0.5, pos.y - 0.5);
    let ctx = widget.ctx();
    if !ctx.bounding_box().contains(probe) {
        return None;
    }
    let border = ctx.window_transform().transform_rect_bbox(ctx.border_box());
    if touches_bottom_left(border, pos) {
        return Some(border);
    }
    widget
        .children()
        .into_iter()
        .find_map(|child| owner_rect(child, pos))
}

fn touches_bottom_left(r: Rect, pos: Point) -> bool {
    (r.x0 - pos.x).abs() < 1.0 && (r.y1 - pos.y).abs() < 1.0 && r.width() > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENU: LayerType = LayerType::Other;

    #[test]
    fn menu_under_a_widget_anchors_to_it_and_flips_above() {
        let owner = Rect::new(150.0, 160.0, 240.0, 184.0);
        let pos = Point::new(150.0, 184.0);
        let bbox = Rect::new(148.0, 184.0, 238.5, 274.0);
        let p = Placement::new(&MENU, pos, bbox, Some(owner));
        assert_eq!(
            p.anchor_rect,
            IRect {
                x: 150,
                y: 160,
                width: 90,
                height: 24
            }
        );
        assert_eq!(
            (p.anchor, p.gravity),
            (Anchor::BottomLeft, Gravity::BottomRight)
        );
        assert!(p.constraints.contains(ConstraintAdjustment::FlipY));
        assert!(!p.constraints.contains(ConstraintAdjustment::FlipX));
        // The shadow inset to the left survives as an offset; the size covers the bbox.
        assert_eq!(p.offset, (-2, 0));
        assert_eq!(p.size, (91, 90));
        assert!(p.grab);
    }

    #[test]
    fn menu_at_a_point_flips_on_both_axes() {
        let pos = Point::new(20.4, 30.6);
        let bbox = Rect::from_origin_size(pos, (100.0, 50.0));
        let p = Placement::new(&MENU, pos, bbox, None);
        assert_eq!(p.anchor_rect, IRect::point(pos));
        assert_eq!((p.anchor_rect.x, p.anchor_rect.y), (20, 30));
        assert!(
            p.constraints
                .contains(ConstraintAdjustment::FlipX | ConstraintAdjustment::FlipY)
        );
        assert_eq!(p.offset, (0, 1));
        assert!(p.grab);
    }

    #[test]
    fn tooltips_never_grab() {
        let pos = Point::new(10.0, 10.0);
        let bbox = Rect::from_origin_size(pos, (60.0, 20.0));
        let owner = Some(Rect::new(10.0, 0.0, 50.0, 10.0));
        let p = Placement::new(&LayerType::Tooltip("hi".into()), pos, bbox, owner);
        assert!(!p.grab);
        assert_eq!(p.anchor, Anchor::TopLeft);
        assert_eq!(p.anchor_rect.width, 1);
    }

    #[test]
    fn moving_keeps_everything_but_the_anchor() {
        let pos = Point::new(5.0, 5.0);
        let p = Placement::new(&MENU, pos, Rect::from_origin_size(pos, (10.0, 10.0)), None);
        let moved = p.moved(Vec2::new(30.0, -4.0));
        assert_eq!((moved.anchor_rect.x, moved.anchor_rect.y), (35, 1));
        assert_eq!(
            Placement {
                anchor_rect: p.anchor_rect,
                ..moved
            },
            p
        );
    }

    #[test]
    fn bottom_left_match_allows_subpixel_error() {
        let r = Rect::new(10.0, 0.0, 50.0, 20.0);
        assert!(touches_bottom_left(r, Point::new(10.4, 20.3)));
        assert!(!touches_bottom_left(r, Point::new(12.0, 20.0)));
        assert!(!touches_bottom_left(r, Point::new(10.0, 0.0)));
    }
}
