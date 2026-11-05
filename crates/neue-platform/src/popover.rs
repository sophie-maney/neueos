//! What a toolkit and a driver tell each other about a popover (task 2.23a, ADR 0056).
//!
//! A popover is a Masonry layer that points at a control. The toolkit puts a
//! [`PopoverAnchor`] property on the layer's root widget; a driver that places layers
//! itself (Wayland: an xdg-popup with an `xdg_positioner`) reads it and asks the
//! compositor for that side of the anchor, letting it flip the popover to the other side
//! or slide it along at the screen's edges. Once the compositor has placed it, the driver
//! moves the Masonry layer there and sets [`PopoverLanded`] on the root, so the popover can
//! point its arrow back at the anchor. A driver that paints layers into the window
//! (winit) does neither: the popover stays where the toolkit put it.

use masonry_core::core::Property;
use masonry_core::kurbo::{Point, Rect};

/// A side of a rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// What a popover asks of the driver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverRequest {
    /// What the popover points at, in window coordinates, grown by the space to leave
    /// between it and the arrow's tip: the popover's box goes flush against it.
    pub anchor: Rect,
    /// The side of `anchor` the popover goes on if there is room.
    pub side: Side,
    /// How far the popover's paint reaches beyond its box (its shadow), the same on every
    /// side; Masonry's layer bounding box is the box grown by this.
    pub reach: f64,
}

/// On a layer's root: the layer is a popover. `None` (the default): it isn't.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopoverAnchor(pub Option<PopoverRequest>);

impl Property for PopoverAnchor {
    fn static_default() -> &'static Self {
        static DEFAULT: PopoverAnchor = PopoverAnchor(None);
        &DEFAULT
    }
}

/// On a popover's root, set by the driver: where the compositor put the layer (its
/// position in window coordinates, as Masonry's layer origin). `None` until then, and
/// always on drivers that don't place layers themselves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopoverLanded(pub Option<Point>);

impl Property for PopoverLanded {
    fn static_default() -> &'static Self {
        static DEFAULT: PopoverLanded = PopoverLanded(None);
        &DEFAULT
    }
}

/// What a popup layer (a menu, a popover) asks of the driver for the blur behind it
/// (task 3.5b, ADR 0073).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupBlurRequest {
    /// Corner radius of the panel.
    pub radius: f64,
    /// Depth of the arrow strip along the panel's side facing the anchor, which stays out
    /// of the blurred region. Zero for a menu.
    pub arrow: f64,
}

/// On a popup layer's root: the panel's material is translucent when the compositor blurs
/// behind it. The blurred region is the layer's box (its border box, not the shadow around
/// it, less the arrow strip). `None` (the default): no blur is asked for. Ignored by
/// drivers without blur.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopupBlur(pub Option<PopupBlurRequest>);

impl Property for PopupBlur {
    fn static_default() -> &'static Self {
        static DEFAULT: PopupBlur = PopupBlur(None);
        &DEFAULT
    }
}
