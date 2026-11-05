//! The date picker: the textual style (a text field's well holding day, month and year
//! segments, with a stepper beside it) and the graphical calendar (a framed month grid
//! with a header of the month and its previous, today and next buttons). The text is the
//! caller's.
//!
//! Colours and metrics come from [`neue_theme::materials::date_picker`].

use neue_theme::materials::date_picker::{
    ARROW_HEIGHT, ARROW_STROKE, ARROW_WIDTH, BORDER, BUTTON_SIZE, CELL_HEIGHT, CELL_RADIUS,
    CELL_WIDTH, DISABLED_OPACITY, HEADER_HEIGHT, PADDING, RADIUS, SEGMENT_RADIUS, TODAY_RADIUS,
    TODAY_RING, WEEKDAY_HEIGHT, arrow, background, border, hover_fill, pressed_fill, rule,
    selection, today_ring,
};
use vello_common::kurbo::{BezPath, Cap, Circle, Join, Point, Rect, RoundedRect, Size, Stroke};

use super::focus_ring;
use super::stepper::{Stepper, StepperPart, stepper};
use super::text_field::{FieldShape, TextField, text_field, text_field_inside, text_field_outline};
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a textual date picker looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateField<'a> {
    /// The segments' rectangles, left to right.
    pub segments: &'a [Rect],
    /// The segment being edited: drawn selected, as long as the field has the focus.
    pub active: Option<usize>,
    /// The stepper half held down by the pointer.
    pub pressed: Option<StepperPart>,
    /// The field has the keyboard focus (the ring is drawn around it).
    pub focused: bool,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
}

/// Draw the textual style: the field's well filling `field` and the stepper filling
/// `stepper_rect` (logical pixels). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside the field.
pub fn date_field(
    scene: &mut Scene,
    field: Rect,
    stepper_rect: Rect,
    spec: DateField<'_>,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    let well = TextField {
        shape: FieldShape::Square,
        focused: spec.focused,
        disabled: spec.disabled,
    };
    text_field(scene, field, well, variant);

    if spec.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    if spec.focused
        && !spec.disabled
        && let Some(&segment) = spec.active.and_then(|i| spec.segments.get(i))
    {
        scene.push_clip(&text_field_inside(field, FieldShape::Square));
        scene.fill(
            &RoundedRect::from_rect(segment, SEGMENT_RADIUS),
            selection(variant),
        );
        scene.pop_layer();
    }
    if spec.disabled {
        scene.pop_layer();
    }

    stepper(
        scene,
        stepper_rect,
        Stepper {
            pressed: spec.pressed,
            disabled: spec.disabled,
            focused: false,
        },
        variant,
    );
}

/// The outline of the textual style's field: the shape its focus ring follows.
pub fn date_field_outline(field: Rect) -> RoundedRect {
    text_field_outline(field, FieldShape::Square)
}

/// A part of the calendar that takes the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalendarPart {
    /// The button for the month before.
    Previous,
    /// The button that goes to today.
    Today,
    /// The button for the month after.
    Next,
    /// A day cell: its index in the grid, 0 to 41, row by row from the top left.
    Day(usize),
}

/// Where the parts of a calendar sit. Cells are indexed 0 to 41, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarLayout {
    /// The whole calendar.
    pub frame: Rect,
    /// Where the month and year are written, left of the buttons.
    pub title: Rect,
    /// The previous button.
    pub previous: Rect,
    /// The today button.
    pub today: Rect,
    /// The next button.
    pub next: Rect,
    /// The weekday names' cells, Monday first.
    pub weekdays: Vec<Rect>,
    /// The day cells.
    pub cells: Vec<Rect>,
}

/// The size of a calendar: seven columns and six rows of days under a header and the
/// weekday names.
pub fn calendar_size() -> Size {
    Size::new(
        7.0 * CELL_WIDTH + 2.0 * PADDING,
        2.0 * PADDING + HEADER_HEIGHT + WEEKDAY_HEIGHT + 6.0 * CELL_HEIGHT,
    )
}

/// Lays a calendar out in `frame`, from its top left corner.
pub fn calendar_layout(frame: Rect) -> CalendarLayout {
    let inner = Rect::new(
        frame.x0 + PADDING,
        frame.y0 + PADDING,
        frame.x1 - PADDING,
        frame.y1 - PADDING,
    );
    let header = Rect::new(inner.x0, inner.y0, inner.x1, inner.y0 + HEADER_HEIGHT);
    let button = |from_right: f64| {
        let x1 = header.x1 - from_right * BUTTON_SIZE;
        let y = header.center().y - BUTTON_SIZE / 2.0;
        Rect::new(x1 - BUTTON_SIZE, y, x1, y + BUTTON_SIZE)
    };
    let (next, today, previous) = (button(0.0), button(1.0), button(2.0));
    let title = Rect::new(header.x0, header.y0, previous.x0, header.y1);

    let weekday_y = header.y1;
    let grid_y = weekday_y + WEEKDAY_HEIGHT;
    let weekdays = (0..7)
        .map(|c| {
            let x = inner.x0 + f64::from(c) * CELL_WIDTH;
            Rect::new(x, weekday_y, x + CELL_WIDTH, grid_y)
        })
        .collect();
    let cells = (0..42)
        .map(|i| {
            let (row, col) = (f64::from(i / 7), f64::from(i % 7));
            let x = inner.x0 + col * CELL_WIDTH;
            let y = grid_y + row * CELL_HEIGHT;
            Rect::new(x, y, x + CELL_WIDTH, y + CELL_HEIGHT)
        })
        .collect();
    CalendarLayout {
        frame,
        title,
        previous,
        today,
        next,
        weekdays,
        cells,
    }
}

impl CalendarLayout {
    /// The part under `point`, if any.
    pub fn part_at(&self, point: Point) -> Option<CalendarPart> {
        let inside = |r: &Rect| r.contains(point);
        if inside(&self.previous) {
            Some(CalendarPart::Previous)
        } else if inside(&self.today) {
            Some(CalendarPart::Today)
        } else if inside(&self.next) {
            Some(CalendarPart::Next)
        } else {
            self.cells.iter().position(inside).map(CalendarPart::Day)
        }
    }

    /// The rectangle of `part`.
    pub fn rect_of(&self, part: CalendarPart) -> Rect {
        match part {
            CalendarPart::Previous => self.previous,
            CalendarPart::Today => self.today,
            CalendarPart::Next => self.next,
            CalendarPart::Day(i) => self.cells.get(i).copied().unwrap_or(Rect::ZERO),
        }
    }
}

/// Everything that changes how a calendar looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calendar {
    /// The selected day's cell: drawn in the selection colour.
    pub selected: Option<usize>,
    /// Today's cell: ringed, unless it is the selected one.
    pub today: Option<usize>,
    /// The cell with the keyboard focus ring.
    pub focused: Option<usize>,
    /// The part under the pointer.
    pub hovered: Option<CalendarPart>,
    /// The part held down by the pointer.
    pub pressed: Option<CalendarPart>,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
}

/// The outline of a day cell's fill and focus ring.
pub fn calendar_cell_outline(cell: Rect) -> RoundedRect {
    RoundedRect::from_rect(cell.inset(-1.0), CELL_RADIUS)
}

/// The outline of one of the header's buttons.
fn button_outline(button: Rect) -> RoundedRect {
    RoundedRect::from_rect(button, CELL_RADIUS)
}

/// An arrow pointing left or right, centred in `button`.
fn arrow_path(button: Rect, left: bool) -> BezPath {
    let c = button.center();
    let (tip, base) = if left {
        (c.x - ARROW_WIDTH / 2.0, c.x + ARROW_WIDTH / 2.0)
    } else {
        (c.x + ARROW_WIDTH / 2.0, c.x - ARROW_WIDTH / 2.0)
    };
    let mut path = BezPath::new();
    path.move_to(Point::new(base, c.y - ARROW_HEIGHT / 2.0));
    path.line_to(Point::new(tip, c.y));
    path.line_to(Point::new(base, c.y + ARROW_HEIGHT / 2.0));
    path
}

/// Draw a calendar laid out as `layout` (logical pixels). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside the focused cell.
pub fn calendar(
    scene: &mut Scene,
    layout: &CalendarLayout,
    calendar: Calendar,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    if calendar.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }

    // The paper and its frame.
    let frame = RoundedRect::from_rect(layout.frame, RADIUS);
    scene.fill(&frame, border(variant));
    scene.fill(
        &RoundedRect::from_rect(layout.frame.inset(-BORDER), (RADIUS - BORDER).max(0.0)),
        background(variant),
    );

    // The hairline under the weekday names.
    let y = layout.cells.first().map_or(layout.frame.y0, |c| c.y0);
    scene.fill(
        &Rect::new(
            layout.frame.x0 + PADDING,
            y - 0.5,
            layout.frame.x1 - PADDING,
            y + 0.5,
        ),
        rule(variant),
    );

    // The header's buttons.
    for part in [
        CalendarPart::Previous,
        CalendarPart::Today,
        CalendarPart::Next,
    ] {
        let outline = button_outline(layout.rect_of(part));
        if calendar.pressed == Some(part) {
            scene.fill(&outline, pressed_fill(variant));
        } else if calendar.hovered == Some(part) {
            scene.fill(&outline, hover_fill(variant));
        }
    }
    let stroke = Stroke::new(ARROW_STROKE)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    scene.stroke(&arrow_path(layout.previous, true), &stroke, arrow(variant));
    scene.stroke(&arrow_path(layout.next, false), &stroke, arrow(variant));
    scene.stroke(
        &Circle::new(layout.today.center(), TODAY_RADIUS),
        &stroke,
        arrow(variant),
    );

    // The days.
    for (i, &cell) in layout.cells.iter().enumerate() {
        let outline = calendar_cell_outline(cell);
        if calendar.selected == Some(i) {
            scene.fill(&outline, selection(variant));
        } else {
            if calendar.today == Some(i) {
                scene.stroke(&outline, &Stroke::new(TODAY_RING), today_ring(variant));
            }
            if calendar.pressed == Some(CalendarPart::Day(i)) {
                scene.fill(&outline, pressed_fill(variant));
            } else if calendar.hovered == Some(CalendarPart::Day(i)) {
                scene.fill(&outline, hover_fill(variant));
            }
        }
    }

    if calendar.disabled {
        scene.pop_layer();
    } else if let Some(&cell) = calendar.focused.and_then(|i| layout.cells.get(i)) {
        focus_ring(scene, calendar_cell_outline(cell), variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn layout() -> CalendarLayout {
        let size = calendar_size();
        calendar_layout(Rect::from_origin_size((0.0, 0.0), size))
    }

    #[test]
    fn the_grid_has_seven_columns_and_six_rows_inside_the_frame() {
        let l = layout();
        assert_eq!(l.cells.len(), 42);
        assert_eq!(l.weekdays.len(), 7);
        assert_eq!(l.cells[0].width(), CELL_WIDTH);
        assert_eq!(l.cells[7].y0, l.cells[0].y1);
        assert_eq!(l.cells[6].x0, l.weekdays[6].x0);
        for cell in &l.cells {
            assert!(l.frame.contains_rect(*cell), "{cell:?}");
        }
        assert!(l.frame.contains_rect(l.next));
        assert!(l.previous.x1 <= l.today.x0 && l.today.x1 <= l.next.x0);
        assert!(l.title.x1 <= l.previous.x0);
    }

    #[test]
    fn points_find_their_part() {
        let l = layout();
        assert_eq!(l.part_at(l.cells[10].center()), Some(CalendarPart::Day(10)));
        assert_eq!(l.part_at(l.previous.center()), Some(CalendarPart::Previous));
        assert_eq!(l.part_at(l.today.center()), Some(CalendarPart::Today));
        assert_eq!(l.part_at(l.next.center()), Some(CalendarPart::Next));
        assert_eq!(l.part_at(l.title.center()), None);
        assert_eq!(l.part_at(l.weekdays[2].center()), None);
        assert_eq!(l.part_at(Point::new(-5.0, -5.0)), None);
        assert_eq!(l.rect_of(CalendarPart::Day(99)), Rect::ZERO);
    }

    #[test]
    fn layers_are_balanced() {
        let l = layout();
        for disabled in [false, true] {
            let mut scene = Scene::new(l.frame.width(), l.frame.height());
            let spec = Calendar {
                selected: Some(9),
                today: Some(12),
                focused: Some(9),
                hovered: Some(CalendarPart::Day(3)),
                pressed: Some(CalendarPart::Next),
                disabled,
            };
            calendar(&mut scene, &l, spec, Appearance::Dark);
            assert_eq!(scene.open_layers(), 0);
        }
        for (focused, disabled) in [(false, false), (true, false), (true, true)] {
            let segments = [Rect::new(4.0, 2.0, 24.0, 20.0)];
            let mut scene = Scene::new(120.0, 24.0);
            let spec = DateField {
                segments: &segments,
                active: Some(0),
                pressed: Some(StepperPart::Up),
                focused,
                disabled,
            };
            date_field(
                &mut scene,
                Rect::new(0.0, 0.0, 100.0, 22.0),
                Rect::new(104.0, 0.0, 117.0, 22.0),
                spec,
                Appearance::Light,
            );
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn the_selected_day_is_not_also_ringed() {
        let l = layout();
        let mut plain = Scene::new(l.frame.width(), l.frame.height());
        let spec = |today| Calendar {
            selected: Some(5),
            today,
            focused: None,
            hovered: None,
            pressed: None,
            disabled: false,
        };
        calendar(&mut plain, &l, spec(None), Appearance::Light);
        let mut same = Scene::new(l.frame.width(), l.frame.height());
        calendar(&mut same, &l, spec(Some(5)), Appearance::Light);
        assert_eq!(plain.commands().len(), same.commands().len());
    }
}
