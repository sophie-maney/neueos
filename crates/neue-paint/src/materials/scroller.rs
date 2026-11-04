//! Overlay scrollers: a thin translucent knob along a scroll view's edge, drawn over the
//! content. While the pointer is over it the knob widens and a track appears behind it.
//!
//! Colours and metrics come from [`neue_theme::materials::scroller`].

use neue_theme::materials::scroller::{
    KNOB_INSET, KNOB_MIN_LENGTH, KNOB_THIN, KNOB_WIDE, Palette, TRACK_EDGE,
};
use vello_common::kurbo::{Rect, RoundedRect};

use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a scroller looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroller {
    /// Along a vertical edge (scrolls up and down) or a horizontal one.
    pub vertical: bool,
    /// How much of the content the view shows, from 0 to 1: the knob's share of the track.
    pub visible: f64,
    /// How far the view has scrolled, from 0 (start) to 1 (end).
    pub position: f64,
    /// From 0 (thin knob, no track) to 1 (wide knob on a track): the pointer is over it.
    pub expanded: f32,
    /// From 0 (hidden) to 1: scrollers fade out when scrolling stops.
    pub opacity: f32,
}

impl Default for Scroller {
    fn default() -> Self {
        Self {
            vertical: true,
            visible: 1.0,
            position: 0.0,
            expanded: 0.0,
            opacity: 1.0,
        }
    }
}

/// The knob's length and the track length it moves along, for a scroller filling `rect`.
fn along(rect: Rect, scroller: &Scroller) -> (f64, f64) {
    let length = if scroller.vertical {
        rect.height()
    } else {
        rect.width()
    };
    let track = (length - 2.0 * KNOB_INSET).max(0.0);
    let knob = (track * scroller.visible.clamp(0.0, 1.0))
        .max(KNOB_MIN_LENGTH)
        .min(track);
    (knob, track)
}

/// How far the knob of a scroller filling `rect` travels from start to end, in logical
/// pixels: dragging it this far scrolls the whole content.
pub fn scroller_travel(rect: Rect, scroller: &Scroller) -> f64 {
    let (knob, track) = along(rect, scroller);
    track - knob
}

/// The knob of a scroller filling `rect` (logical pixels): where it's drawn and where a
/// press grabs it.
pub fn scroller_knob(rect: Rect, scroller: &Scroller) -> RoundedRect {
    let (knob, track) = along(rect, scroller);
    let width = KNOB_THIN + (KNOB_WIDE - KNOB_THIN) * f64::from(scroller.expanded.clamp(0.0, 1.0));
    let start = KNOB_INSET + scroller.position.clamp(0.0, 1.0) * (track - knob);
    let knob_rect = if scroller.vertical {
        let x1 = rect.x1 - KNOB_INSET;
        Rect::new(x1 - width, rect.y0 + start, x1, rect.y0 + start + knob)
    } else {
        let y1 = rect.y1 - KNOB_INSET;
        Rect::new(rect.x0 + start, y1 - width, rect.x0 + start + knob, y1)
    };
    RoundedRect::from_rect(knob_rect, width / 2.0)
}

/// Draw a scroller filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::SCROLLER_WIDTH`] across is the regular size).
pub fn scroller(scene: &mut Scene, rect: Rect, scroller: Scroller, variant: impl Into<Variant>) {
    let opacity = scroller.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let p = Palette::new(variant.into());
    let layered = opacity < 1.0;
    if layered {
        scene.push_opacity(opacity);
    }
    let expanded = scroller.expanded.clamp(0.0, 1.0);
    if expanded > 0.0 {
        scene.fill(&rect, p.track.multiply_alpha(expanded));
        let edge = if scroller.vertical {
            Rect::new(rect.x0, rect.y0, rect.x0 + TRACK_EDGE, rect.y1)
        } else {
            Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + TRACK_EDGE)
        };
        scene.fill(&edge, p.track_edge.multiply_alpha(expanded));
    }
    scene.fill(&scroller_knob(rect, &scroller), p.knob);
    if layered {
        scene.pop_layer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    const STRIP: Rect = Rect::new(0.0, 0.0, 15.0, 104.0);

    fn at(visible: f64, position: f64) -> Scroller {
        Scroller {
            visible,
            position,
            ..Scroller::default()
        }
    }

    #[test]
    fn the_knob_shows_the_visible_share_at_the_position() {
        // 100 px of track: half the content visible, scrolled to the middle.
        let knob = scroller_knob(STRIP, &at(0.5, 0.5)).rect();
        assert_eq!((knob.y0, knob.y1), (27.0, 77.0));
        assert_eq!((knob.x0, knob.x1), (6.0, 13.0), "thin, at the outer edge");
        assert_eq!(scroller_travel(STRIP, &at(0.5, 0.0)), 50.0);
        let end = scroller_knob(STRIP, &at(0.5, 1.0)).rect();
        assert_eq!(end.y1, 102.0);
    }

    #[test]
    fn long_content_keeps_a_minimum_knob() {
        let knob = scroller_knob(STRIP, &at(0.001, 0.0)).rect();
        assert_eq!(knob.height(), KNOB_MIN_LENGTH);
        assert_eq!(
            scroller_travel(STRIP, &at(0.001, 0.0)),
            100.0 - KNOB_MIN_LENGTH
        );
    }

    #[test]
    fn expanding_widens_inward_and_horizontal_mirrors() {
        let wide = Scroller {
            expanded: 1.0,
            ..at(0.5, 0.0)
        };
        let knob = scroller_knob(STRIP, &wide).rect();
        assert_eq!((knob.x0, knob.x1), (2.0, 13.0));
        let strip = Rect::new(0.0, 0.0, 104.0, 15.0);
        let flat = Scroller {
            vertical: false,
            ..at(0.5, 0.0)
        };
        let knob = scroller_knob(strip, &flat).rect();
        assert_eq!((knob.x0, knob.x1, knob.y0, knob.y1), (2.0, 52.0, 6.0, 13.0));
    }

    #[test]
    fn hidden_draws_nothing_and_layers_balance() {
        let draw = |s: Scroller| {
            let mut scene = Scene::new(15.0, 104.0);
            scroller(&mut scene, STRIP, s, Appearance::Dark);
            assert_eq!(scene.open_layers(), 0);
            scene.commands().len()
        };
        let hidden = Scroller {
            opacity: 0.0,
            ..Scroller::default()
        };
        assert_eq!(draw(hidden), 0);
        assert_eq!(draw(Scroller::default()), 1, "just the knob");
        let fading = Scroller {
            opacity: 0.5,
            expanded: 1.0,
            ..Scroller::default()
        };
        assert_eq!(draw(fading), 5, "layer, track, edge, knob, pop");
    }
}
