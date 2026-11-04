//! Tables: the header bar of column titles (a light gradient with dividers, a darker
//! pressed title, a sort triangle) and the scroll view's hairline border. The caller lays
//! out the columns and draws the titles; rows, stripes and the selection are flat fills in
//! the semantic colours.
//!
//! Colours and metrics come from [`neue_theme::materials::table`].

use neue_theme::materials::table::{
    BORDER, DIVIDER, HEADER_BORDER, Palette, SORT_ARROW_HEIGHT, SORT_ARROW_INSET, SORT_ARROW_WIDTH,
};
use vello_common::kurbo::{BezPath, Point, Rect, Stroke};
use vello_common::peniko::{Color, Gradient};

use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a table header looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableHeader<'a> {
    /// Column widths, left to right; space after the last column is header too.
    pub widths: &'a [f64],
    /// The column whose title is held down.
    pub pressed: Option<usize>,
    /// The sorted column, and whether it sorts ascending (the triangle points up).
    pub sorted: Option<(usize, bool)>,
}

fn vertical(rect: Rect, [top, bottom]: [Color; 2]) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, top), (1.0, bottom)])
}

/// The left and right edge of each column in a header or row filling `rect`.
pub fn column_edges(rect: Rect, widths: &[f64]) -> Vec<(f64, f64)> {
    let mut x = rect.x0;
    widths
        .iter()
        .map(|w| {
            let edges = (x, x + w.max(0.0));
            x = edges.1;
            edges
        })
        .collect()
}

/// Where the sort triangle of a column spanning `x0..x1` in a header filling `rect` goes.
pub fn sort_arrow_rect(rect: Rect, (_, x1): (f64, f64)) -> Rect {
    let body = rect.y1 - HEADER_BORDER;
    let y0 = (rect.y0 + (body - rect.y0 - SORT_ARROW_HEIGHT) / 2.0).round();
    let x0 = x1 - SORT_ARROW_INSET - SORT_ARROW_WIDTH;
    Rect::new(x0, y0, x0 + SORT_ARROW_WIDTH, y0 + SORT_ARROW_HEIGHT)
}

/// Draw a table header filling `rect` (logical pixels;
/// [`neue_theme::materials::table::HEADER_HEIGHT`] tall is the regular size). The titles
/// are the caller's.
pub fn table_header(
    scene: &mut Scene,
    rect: Rect,
    header: TableHeader<'_>,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into());
    let body = Rect::new(rect.x0, rect.y0, rect.x1, rect.y1 - HEADER_BORDER);
    scene.fill(&body, vertical(body, p.header));
    let edges = column_edges(rect, header.widths);
    if let Some(&(x0, x1)) = header.pressed.and_then(|i| edges.get(i)) {
        let cell = Rect::new(x0, body.y0, x1, body.y1);
        scene.fill(&cell, vertical(cell, p.header_pressed));
    }
    for &(_, x1) in &edges {
        // A last column that reaches the edge (stretched to fill) has no divider.
        if x1 >= rect.x1 {
            continue;
        }
        // The divider sits in the last pixel of its column; the highlight in the next one.
        let line = Rect::new(x1 - DIVIDER, body.y0 + 2.0, x1, body.y1 - 2.0);
        scene.fill(&line, p.divider);
        scene.fill(&line.with_origin((x1, line.y0)), p.divider_highlight);
    }
    scene.fill(
        &Rect::new(rect.x0, body.y1, rect.x1, rect.y1),
        p.header_border,
    );
    if let Some((i, ascending)) = header.sorted
        && let Some(&column) = edges.get(i)
    {
        let r = sort_arrow_rect(rect, column);
        let mut path = BezPath::new();
        let (tip, base) = if ascending {
            (r.y0, r.y1)
        } else {
            (r.y1, r.y0)
        };
        path.move_to(Point::new(r.center().x, tip));
        path.line_to(Point::new(r.x1, base));
        path.line_to(Point::new(r.x0, base));
        path.close_path();
        scene.fill(&path, p.sort_arrow);
    }
}

/// Draw the hairline border of a table's scroll view filling `rect`, inside its edge.
pub fn table_border(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let half = BORDER / 2.0;
    scene.stroke(&rect.inset(-half), &Stroke::new(BORDER), p.border);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_follow_each_other_from_the_left_edge() {
        let edges = column_edges(Rect::new(10.0, 0.0, 300.0, 17.0), &[50.0, 0.0, 30.0]);
        assert_eq!(edges, [(10.0, 60.0), (60.0, 60.0), (60.0, 90.0)]);
    }

    #[test]
    fn the_sort_arrow_sits_inside_the_right_end_of_its_column() {
        let r = sort_arrow_rect(Rect::new(0.0, 0.0, 200.0, 17.0), (0.0, 100.0));
        assert_eq!(r.x1, 100.0 - SORT_ARROW_INSET);
        assert!(r.y0 >= 0.0 && r.y1 <= 16.0, "{r:?}");
    }
}
