//! Notification banners and alerts (task 3.15b): a rounded panel with a soft shadow, the
//! app's icon (or an image, the icon then a badge on it) on the left, a bold title and up
//! to two lines of body, and for an alert a column of action buttons on the right. A close
//! button stands out over the top left corner while the pointer is over the panel.
//!
//! The text is the caller's (it is laid out by the toolkit), in [`Palette::title`] and
//! [`Palette::body`] colours, inside [`Parts::text`]. Colours and metrics come from
//! [`neue_theme::materials::notification`].

use neue_theme::materials::notification::{
    ACTIONS_WIDTH, BADGE_SIZE, CLOSE_INSET, CLOSE_SIZE, ICON_GAP, ICON_SIZE, IMAGE_RADIUS,
    MAX_ACTIONS, MIN_HEIGHT, PADDING, RADIUS, RIM, SHADOW_BLUR, SHADOW_OFFSET,
};
use vello_common::kurbo::{Cap, Circle, Line, Point, Rect, RoundedRect, Shape, Stroke, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::icon::{Icon, icon};
use crate::materials::desktop::Picture;
use crate::scene::Scene;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::notification::{MARGIN, Palette, WIDTH};

/// Where the parts of a panel are, in the same coordinates as the panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Parts {
    /// The panel itself.
    pub panel: Rect,
    /// The icon's (or image's) square; `None` when there is neither.
    pub icon: Option<Rect>,
    /// The column the title and the body are laid out in: its left and right edges, and
    /// the title's top. The text runs down from there.
    pub text: Rect,
    /// The close button's square.
    pub close: Rect,
    /// An alert's action buttons, top to bottom (at most [`MAX_ACTIONS`]).
    pub actions: Vec<Rect>,
}

impl Parts {
    /// Lays out a panel filling `panel`, with an icon or not, and `actions` buttons (more
    /// than [`MAX_ACTIONS`] are left out).
    pub fn new(panel: Rect, has_icon: bool, actions: usize) -> Self {
        let actions = actions.min(MAX_ACTIONS);
        let icon = has_icon.then(|| {
            Rect::from_origin_size(
                (panel.x0 + PADDING, panel.y0 + PADDING),
                (ICON_SIZE, ICON_SIZE),
            )
        });
        let text_x0 = icon.map_or(panel.x0 + PADDING, |icon| icon.x1 + ICON_GAP);
        let content_x1 = if actions > 0 {
            panel.x1 - ACTIONS_WIDTH
        } else {
            panel.x1
        };
        let text = Rect::new(
            text_x0,
            panel.y0 + PADDING,
            (content_x1 - PADDING).max(text_x0),
            panel.y1 - PADDING,
        );
        let corner = Point::new(panel.x0 + CLOSE_INSET, panel.y0 + CLOSE_INSET);
        let close = Rect::from_center_size(corner, (CLOSE_SIZE, CLOSE_SIZE));
        let actions = (0..actions)
            .map(|i| {
                let h = panel.height() / actions as f64;
                let y0 = panel.y0 + h * i as f64;
                Rect::new(content_x1, y0, panel.x1, y0 + h)
            })
            .collect();
        Self {
            panel,
            icon,
            text,
            close,
            actions,
        }
    }

    /// The width left for the title and the body.
    pub fn text_width(has_icon: bool, actions: usize) -> f64 {
        Self::new(Rect::new(0.0, 0.0, WIDTH, MIN_HEIGHT), has_icon, actions)
            .text
            .width()
    }
}

/// The panel's height for text `text_height` tall: the text and the padding around it, at
/// least [`MIN_HEIGHT`].
pub fn panel_height(text_height: f64) -> f64 {
    (text_height + 2.0 * PADDING).ceil().max(MIN_HEIGHT)
}

/// The outline of a panel filling `rect`.
pub fn outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.width().min(rect.height()) / 2.0))
}

fn vertical(rect: Rect, top: Color, bottom: Color) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, top), (1.0, bottom)])
}

/// Draw the panel of `parts`: its shadow (reaching [`MARGIN`] around it), body and rim,
/// and the lines around the action buttons.
pub fn panel(scene: &mut Scene, parts: &Parts, variant: impl Into<Variant>, backdrop: Backdrop) {
    let p = Palette::new(variant.into(), backdrop);
    let rect = parts.panel;
    let outline = outline(rect);

    // The shadow falls outside the panel only, so it doesn't darken a translucent body.
    let mut frame = rect.inflate(MARGIN, MARGIN).to_path(0.1);
    frame.extend(outline.to_path(0.1).reverse_subpaths());
    scene.push_clip(&frame);
    scene.blurred_rounded_rect(
        rect + Vec2::new(0.0, SHADOW_OFFSET),
        outline.radii().top_left,
        SHADOW_BLUR,
        p.shadow,
    );
    scene.pop_layer();

    scene.fill(&outline, vertical(rect, p.fill[0], p.fill[1]));

    scene.push_clip(&outline);
    // The top highlight, one pixel inside the rim.
    let y = rect.y0 + RIM + 0.5;
    scene.stroke(
        &Line::new((rect.x0 + RADIUS, y), (rect.x1 - RADIUS, y)),
        &Stroke::new(1.0),
        p.highlight,
    );
    for (i, button) in parts.actions.iter().enumerate() {
        let x = button.x0 + 0.5;
        if i == 0 {
            scene.stroke(
                &Line::new((x, rect.y0), (x, rect.y1)),
                &Stroke::new(1.0),
                p.divider,
            );
        } else {
            let y = button.y0.round() - 0.5;
            scene.stroke(
                &Line::new((x, y), (rect.x1, y)),
                &Stroke::new(1.0),
                p.divider,
            );
        }
    }
    scene.pop_layer();

    let rim = RoundedRect::from_rect(
        rect.inset(-RIM / 2.0),
        (outline.radii().top_left - RIM / 2.0).max(0.0),
    );
    scene.stroke(&rim, &Stroke::new(RIM), p.rim);
}

/// Draw the app's icon in `rect`, or `image` with the icon as a badge on its bottom right
/// corner.
pub fn picture(
    scene: &mut Scene,
    rect: Rect,
    app_icon: Option<&Icon>,
    image: Option<&Picture>,
    variant: impl Into<Variant>,
    backdrop: Backdrop,
) {
    let variant = variant.into();
    let p = Palette::new(variant, backdrop);
    match image {
        Some(image) => {
            let (w, h) = (f64::from(image.width()), f64::from(image.height()));
            let k = (rect.width() / w).min(rect.height() / h);
            let placed = Rect::from_center_size(rect.center(), ((w * k).round(), (h * k).round()));
            scene.push_clip(&RoundedRect::from_rect(placed, IMAGE_RADIUS));
            icon(scene, rect, &Icon::raster(image.clone()), p.title, variant);
            scene.pop_layer();
            if let Some(app_icon) = app_icon {
                let badge = Rect::new(
                    rect.x1 - BADGE_SIZE + 4.0,
                    rect.y1 - BADGE_SIZE + 4.0,
                    rect.x1 + 4.0,
                    rect.y1 + 4.0,
                );
                icon(scene, badge, app_icon, p.title, variant);
            }
        }
        None => {
            if let Some(app_icon) = app_icon {
                icon(scene, rect, app_icon, p.title, variant);
            }
        }
    }
}

/// Draw the close button in `rect` (from [`Parts::close`]): a disc with a light ring and
/// a cross, darker while `hot` (the pointer is over it or holds it).
pub fn close_button(scene: &mut Scene, rect: Rect, hot: bool, variant: impl Into<Variant>) {
    // Only the panel depends on the backdrop.
    let p = Palette::new(variant.into(), Backdrop::Opaque);
    let disc = Circle::new(rect.center(), rect.width().min(rect.height()) / 2.0 - 0.75);
    scene.fill(&disc, p.close[usize::from(hot)]);
    scene.stroke(&disc, &Stroke::new(1.5), p.close_ring);
    let arm = rect.width() * 0.17;
    let c = rect.center();
    let stroke = Stroke::new(1.75).with_caps(Cap::Round);
    for (from, to) in [((-arm, -arm), (arm, arm)), ((-arm, arm), (arm, -arm))] {
        scene.stroke(
            &Line::new(c + Vec2::from(from), c + Vec2::from(to)),
            &stroke,
            p.close_cross,
        );
    }
}

#[cfg(test)]
mod tests {
    use neue_theme::materials::notification::{CLOSE_SIZE, MARGIN};

    use super::*;
    use crate::Appearance;

    fn panel_rect() -> Rect {
        Rect::new(MARGIN, MARGIN, MARGIN + WIDTH, MARGIN + 70.0)
    }

    #[test]
    fn a_banner_gives_the_text_everything_right_of_the_icon() {
        let parts = Parts::new(panel_rect(), true, 0);
        let icon = parts.icon.expect("an icon");
        assert_eq!(parts.text.x0, icon.x1 + ICON_GAP);
        assert_eq!(parts.text.x1, parts.panel.x1 - PADDING);
        assert!(parts.actions.is_empty());
        let bare = Parts::new(panel_rect(), false, 0);
        assert_eq!(bare.text.x0, bare.panel.x0 + PADDING);
    }

    #[test]
    fn an_alert_splits_its_button_column() {
        let parts = Parts::new(panel_rect(), true, 3);
        assert_eq!(parts.actions.len(), MAX_ACTIONS);
        let [a, b] = [parts.actions[0], parts.actions[1]];
        assert_eq!(a.x1, parts.panel.x1);
        assert_eq!(a.width(), ACTIONS_WIDTH);
        assert_eq!(a.y1, b.y0);
        assert_eq!((a.y0, b.y1), (parts.panel.y0, parts.panel.y1));
        assert_eq!(parts.text.x1, a.x0 - PADDING);
    }

    #[test]
    fn the_close_button_fits_in_the_margin() {
        let parts = Parts::new(panel_rect(), true, 0);
        assert_eq!(parts.close.width(), CLOSE_SIZE);
        assert!(parts.close.x0 >= 0.0 && parts.close.y0 >= 0.0);
    }

    #[test]
    fn short_text_keeps_the_icon_height() {
        assert_eq!(panel_height(10.0), MIN_HEIGHT);
        assert_eq!(panel_height(60.0), 80.0);
    }

    #[test]
    fn layers_are_balanced() {
        let parts = Parts::new(panel_rect(), true, 2);
        let mut scene = Scene::new(400.0, 120.0);
        panel(&mut scene, &parts, Appearance::Dark, Backdrop::Translucent);
        close_button(&mut scene, parts.close, true, Appearance::Light);
        let image = Picture::from_rgba8(2, 1, &[255; 8]).expect("a picture");
        picture(
            &mut scene,
            parts.icon.expect("an icon"),
            None,
            Some(&image),
            Appearance::Light,
            Backdrop::Opaque,
        );
        assert_eq!(scene.open_layers(), 0);
    }
}
