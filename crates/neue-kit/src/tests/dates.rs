//! The date picker (2.24c) end to end on the fake host: segment editing in the textual
//! style (arrows, digits, the stepper), month navigation and day selection in the
//! calendar, the keys, changes from the app, and what assistive technologies see.

use masonry_core::accesskit::{Action, NodeId, Role};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::kurbo::Point;
use neue_paint::materials::CalendarPart;

use super::menus::{a11y, named, typed};
use super::*;
use crate::widgets::{CalendarWidget, DateFieldWidget};
use crate::{CalendarPicker, Date, DatePicker};

fn d(y: i32, m: u8, day: u8) -> Date {
    Date::new(y, m, day).expect("a real date")
}

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Field(Date),
    Calendar(Date),
    /// Script: the app shows another date in both.
    Set(Date),
    /// Script: the app disables both.
    Disable,
}

struct Dates {
    start: Date,
    field: Handle<DatePicker<Msg>>,
    calendar: Handle<CalendarPicker<Msg>>,
    log: Vec<Msg>,
}

impl Application for Dates {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .padding(20.0)
            .spacing(40.0)
            .with(
                DatePicker::new(self.start)
                    .label("Due")
                    .on_change(Msg::Field)
                    .bind(&mut self.field),
            )
            .with(
                DatePicker::calendar(self.start)
                    .today(d(2026, 11, 20))
                    .label("Calendar")
                    .on_change(Msg::Calendar)
                    .bind(&mut self.calendar),
            );
        cx.open_window(Window::new("Dates", (400, 500), root).chrome());
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Set(date) => {
                self.field.set_date(cx, date);
                self.calendar.set_date(cx, date);
            }
            Msg::Disable => {
                self.field.set_enabled(cx, false);
                self.calendar.set_enabled(cx, false);
            }
            _ => self.log.push(message),
        }
    }
}

fn setup_at(start: Date) -> (Runner<Dates>, FakeHost) {
    let (runner, mut host, _) = start_app(Dates {
        start,
        field: Handle::default(),
        calendar: Handle::default(),
        log: Vec::new(),
    });
    let _ = host.only().render();
    (runner, host)
}

fn start_app(app: Dates) -> (Runner<Dates>, FakeHost, Arc<AtomicUsize>) {
    start(app)
}

fn setup() -> (Runner<Dates>, FakeHost) {
    setup_at(d(2026, 11, 5))
}

fn click_at(runner: &mut Runner<Dates>, host: &mut FakeHost, at: Point) {
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn focus(host: &mut FakeHost, widget: WidgetId) {
    host.only().focus_on(Some(widget));
}

/// The dates the app was sent by the textual picker, in order.
fn field_log(runner: &Runner<Dates>) -> Vec<Date> {
    runner
        .app()
        .log
        .iter()
        .filter_map(|m| match m {
            Msg::Field(d) => Some(*d),
            _ => None,
        })
        .collect()
}

/// The dates the app was sent by the calendar, in order.
fn calendar_log(runner: &Runner<Dates>) -> Vec<Date> {
    runner
        .app()
        .log
        .iter()
        .filter_map(|m| match m {
            Msg::Calendar(d) => Some(*d),
            _ => None,
        })
        .collect()
}

/// The centres of the segments, then the stepper's upper and lower halves, in window space.
fn field_points(host: &mut FakeHost, widget: WidgetId) -> ([Point; 3], Point, Point) {
    let _ = host.only().render();
    let w = host.only().get_widget_with_id(widget);
    let ctx = w.ctx();
    let (segments, stepper) = w
        .downcast::<DateFieldWidget>()
        .expect("a date field")
        .inner()
        .parts();
    let at = |p: Point| ctx.to_window(p);
    (
        segments.map(|r| at(r.center())),
        at(Point::new(
            stepper.center().x,
            stepper.y0 + stepper.height() / 4.0,
        )),
        at(Point::new(
            stepper.center().x,
            stepper.y1 - stepper.height() / 4.0,
        )),
    )
}

/// Which segment has the focus: 0 day, 1 month, 2 year.
fn focused_segment(host: &mut FakeHost, widget: WidgetId) -> usize {
    let w = host.only().get_widget_with_id(widget);
    w.downcast::<DateFieldWidget>()
        .expect("a date field")
        .inner()
        .focused_segment()
}

/// Reads the calendar widget: the month on show and the window position of a day's cell.
fn shown(host: &mut FakeHost, widget: WidgetId) -> (i32, u8) {
    let _ = host.only().render();
    let w = host.only().get_widget_with_id(widget);
    w.downcast::<CalendarWidget>()
        .expect("a calendar")
        .inner()
        .shown()
}

fn cell(host: &mut FakeHost, widget: WidgetId, date: Date) -> Point {
    let _ = host.only().render();
    let w = host.only().get_widget_with_id(widget);
    let ctx = w.ctx();
    let local = w
        .downcast::<CalendarWidget>()
        .expect("a calendar")
        .inner()
        .cell_centre(date)
        .expect("the day is on show");
    ctx.to_window(local)
}

fn button(host: &mut FakeHost, widget: WidgetId, part: CalendarPart) -> Point {
    let _ = host.only().render();
    let w = host.only().get_widget_with_id(widget);
    let ctx = w.ctx();
    let local = w
        .downcast::<CalendarWidget>()
        .expect("a calendar")
        .inner()
        .button_centre(part);
    ctx.to_window(local)
}

/// What assistive technology reads as the widget's value.
fn a11y_value(host: &mut FakeHost, widget: WidgetId) -> Option<String> {
    let harness = host.only();
    let _ = harness.render();
    harness.access_node(widget).expect("a node").value()
}

// ---- Textual style ---------------------------------------------------------------------

#[test]
fn up_and_down_change_the_selected_segment_and_send_the_date() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(
        field_log(&runner),
        [d(2026, 11, 6), d(2026, 11, 5), d(2026, 11, 4)]
    );
}

#[test]
fn left_and_right_move_between_day_month_and_year() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    focus(&mut host, field);
    assert_eq!(focused_segment(&mut host, field), 0);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    // The year is the last segment: Right stops there, Left goes back.
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(focused_segment(&mut host, field), 2);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(focused_segment(&mut host, field), 0);
    assert_eq!(
        field_log(&runner),
        [d(2026, 12, 5), d(2027, 12, 5), d(2028, 12, 5)]
    );
}

#[test]
fn the_day_wraps_round_the_month_and_follows_its_length() {
    let (mut runner, mut host) = setup_at(d(2026, 11, 29));
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(
        field_log(&runner),
        [d(2026, 11, 30), d(2026, 11, 1), d(2026, 11, 30)]
    );

    // Changing the month shortens the day to fit: 31 January to 28 February, and back up
    // the 28th stays.
    let (mut runner, mut host) = setup_at(d(2026, 1, 31));
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(field_log(&runner), [d(2026, 2, 28)]);
}

#[test]
fn a_leap_day_goes_when_the_year_is_not_a_leap_year() {
    let (mut runner, mut host) = setup_at(d(2024, 2, 29));
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(field_log(&runner), [d(2025, 2, 28)]);
}

#[test]
fn clicking_a_segment_selects_it() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    let (segments, _, _) = field_points(&mut host, field);
    click_at(&mut runner, &mut host, segments[1]);
    assert_eq!(host.only().focused_widget_id(), Some(field));
    assert_eq!(focused_segment(&mut host, field), 1);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    click_at(&mut runner, &mut host, segments[2]);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(field_log(&runner), [d(2026, 10, 5), d(2027, 10, 5)]);
}

#[test]
fn the_stepper_changes_the_selected_segment() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    let (segments, up, down) = field_points(&mut host, field);
    click_at(&mut runner, &mut host, up);
    click_at(&mut runner, &mut host, up);
    click_at(&mut runner, &mut host, down);
    // The stepper works on the segment that was selected: select the month, step again.
    click_at(&mut runner, &mut host, segments[1]);
    click_at(&mut runner, &mut host, down);
    assert_eq!(
        field_log(&runner),
        [
            d(2026, 11, 6),
            d(2026, 11, 7),
            d(2026, 11, 6),
            d(2026, 10, 6)
        ]
    );
}

#[test]
fn a_press_on_the_stepper_that_leaves_it_does_nothing() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    let (segments, up, _) = field_points(&mut host, field);
    let harness = host.only();
    harness.mouse_move(up);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(segments[0]);
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty());
}

#[test]
fn typing_digits_fills_the_segment_and_moves_on() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    focus(&mut host, field);
    // Day 23: the 2 is a day already, the 3 completes it and the month is next.
    typed(&mut runner, &mut host, "23");
    assert_eq!(field_log(&runner), [d(2026, 11, 2), d(2026, 11, 23)]);
    assert_eq!(focused_segment(&mut host, field), 1);
    // Month 3: no month starts with 3, so it is done at once and the year is next.
    typed(&mut runner, &mut host, "3");
    assert_eq!(field_log(&runner).last(), Some(&d(2026, 3, 23)));
    assert_eq!(focused_segment(&mut host, field), 2);
}

#[test]
fn a_year_takes_effect_with_its_fourth_digit() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    typed(&mut runner, &mut host, "203");
    assert!(field_log(&runner).is_empty(), "{:?}", field_log(&runner));
    typed(&mut runner, &mut host, "0");
    assert_eq!(field_log(&runner), [d(2030, 11, 5)]);
    // The year is the last segment: the focus stays.
    assert_eq!(focused_segment(&mut host, field), 2);
}

#[test]
fn return_confirms_a_short_year_and_escape_drops_it() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    typed(&mut runner, &mut host, "99");
    named(&mut runner, &mut host, NamedKey::Escape);
    assert!(field_log(&runner).is_empty());
    typed(&mut runner, &mut host, "99");
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(field_log(&runner), [d(99, 11, 5)]);
}

#[test]
fn a_digit_too_big_for_the_day_starts_again() {
    let (mut runner, mut host) = setup_at(d(2026, 2, 1));
    let field = id(&runner.app().field);
    focus(&mut host, field);
    // February has 28 days: 3 is a day and done (30 would not exist), so the 1 is the month.
    typed(&mut runner, &mut host, "31");
    assert_eq!(field_log(&runner), [d(2026, 2, 3), d(2026, 1, 3)]);
}

#[test]
fn a_half_typed_year_is_dropped_when_the_focus_leaves() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    typed(&mut runner, &mut host, "20");
    host.only().focus_on(None);
    assert!(field_log(&runner).is_empty());
    assert_eq!(
        a11y_value(&mut host, field).as_deref(),
        Some("5 November 2026")
    );
}

#[test]
fn a_disabled_picker_ignores_the_pointer_and_the_keys() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    let (segments, up, _) = field_points(&mut host, field);
    runner.send(&mut host, Msg::Disable);
    click_at(&mut runner, &mut host, up);
    click_at(&mut runner, &mut host, segments[1]);
    assert_ne!(host.only().focused_widget_id(), Some(field));
    assert!(runner.app().log.is_empty());
}

#[test]
fn the_app_can_show_another_date_without_a_message() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    runner.send(&mut host, Msg::Set(d(2024, 2, 29)));
    assert!(runner.app().log.is_empty());
    assert_eq!(
        a11y_value(&mut host, field).as_deref(),
        Some("29 February 2024")
    );
    focus(&mut host, field);
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(field_log(&runner), [d(2024, 2, 1)]);
}

#[test]
fn the_field_tells_assistive_technology_its_segments() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    let harness = host.only();
    let _ = harness.render();
    let node = harness.access_node(field).expect("a node");
    assert_eq!(node.role(), Role::Group);
    assert_eq!(node.label().as_deref(), Some("Due"));
    assert_eq!(node.value().as_deref(), Some("5 November 2026"));
    let segments: Vec<_> = node
        .children()
        .map(|c| {
            (
                c.role(),
                c.label(),
                c.numeric_value(),
                c.min_numeric_value(),
                c.max_numeric_value(),
            )
        })
        .collect();
    let spin = Role::SpinButton;
    assert_eq!(
        segments,
        [
            (spin, Some("Day".into()), Some(5.0), Some(1.0), Some(30.0)),
            (
                spin,
                Some("Month".into()),
                Some(11.0),
                Some(1.0),
                Some(12.0)
            ),
            (
                spin,
                Some("Year".into()),
                Some(2026.0),
                Some(1.0),
                Some(9999.0)
            ),
        ]
    );

    // Increment and Decrement change the selected segment.
    a11y(&mut host, NodeId(field.to_raw()), Action::Increment);
    deliver(&mut runner, &mut host);
    a11y(&mut host, NodeId(field.to_raw()), Action::Decrement);
    a11y(&mut host, NodeId(field.to_raw()), Action::Decrement);
    deliver(&mut runner, &mut host);
    assert_eq!(
        field_log(&runner),
        [d(2026, 11, 6), d(2026, 11, 5), d(2026, 11, 4)]
    );
}

// ---- Graphical style -------------------------------------------------------------------

#[test]
fn clicking_a_day_selects_it_and_sends_the_date() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    let at = cell(&mut host, calendar, d(2026, 11, 17));
    click_at(&mut runner, &mut host, at);
    assert_eq!(calendar_log(&runner), [d(2026, 11, 17)]);
    // The same day again is not a change.
    click_at(&mut runner, &mut host, at);
    assert_eq!(calendar_log(&runner), [d(2026, 11, 17)]);
    assert_eq!(host.only().focused_widget_id(), Some(calendar));
}

#[test]
fn a_day_of_the_next_month_on_the_page_selects_it_and_turns_the_page() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    // November 2026 starts on a Sunday: the page opens with 26 October and ends on 6 December.
    let at = cell(&mut host, calendar, d(2026, 10, 26));
    click_at(&mut runner, &mut host, at);
    assert_eq!(calendar_log(&runner), [d(2026, 10, 26)]);
    assert_eq!(shown(&mut host, calendar), (2026, 10));
    let at = cell(&mut host, calendar, d(2026, 11, 1));
    click_at(&mut runner, &mut host, at);
    assert_eq!(shown(&mut host, calendar), (2026, 11));
}

#[test]
fn the_arrows_browse_months_without_changing_the_date() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    let next = button(&mut host, calendar, CalendarPart::Next);
    let previous = button(&mut host, calendar, CalendarPart::Previous);
    click_at(&mut runner, &mut host, next);
    click_at(&mut runner, &mut host, next);
    assert_eq!(shown(&mut host, calendar), (2027, 1));
    click_at(&mut runner, &mut host, previous);
    assert_eq!(shown(&mut host, calendar), (2026, 12));
    assert!(runner.app().log.is_empty());
    // A day on the page being browsed is chosen as usual.
    let at = cell(&mut host, calendar, d(2026, 12, 25));
    click_at(&mut runner, &mut host, at);
    assert_eq!(calendar_log(&runner), [d(2026, 12, 25)]);
}

#[test]
fn the_today_button_selects_today_and_turns_to_its_page() {
    let (mut runner, mut host) = setup_at(d(2027, 3, 14));
    let calendar = id(&runner.app().calendar);
    let today = button(&mut host, calendar, CalendarPart::Today);
    click_at(&mut runner, &mut host, today);
    assert_eq!(calendar_log(&runner), [d(2026, 11, 20)]);
    assert_eq!(shown(&mut host, calendar), (2026, 11));
    // Already there: nothing more to send.
    click_at(&mut runner, &mut host, today);
    assert_eq!(calendar_log(&runner), [d(2026, 11, 20)]);
}

#[test]
fn a_press_that_leaves_the_day_selects_nothing() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    let on = cell(&mut host, calendar, d(2026, 11, 10));
    let off = cell(&mut host, calendar, d(2026, 11, 11));
    let harness = host.only();
    harness.mouse_move(on);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(off);
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty());
}

#[test]
fn the_arrow_keys_move_the_selection_by_days_and_weeks() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    focus(&mut host, calendar);
    for key in [
        NamedKey::ArrowRight,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowUp,
    ] {
        named(&mut runner, &mut host, key);
    }
    assert_eq!(
        calendar_log(&runner),
        [
            d(2026, 11, 6),
            d(2026, 11, 13),
            d(2026, 11, 12),
            d(2026, 11, 5)
        ]
    );
}

#[test]
fn the_keys_carry_the_selection_over_month_ends_and_the_page_follows() {
    let (mut runner, mut host) = setup_at(d(2026, 11, 30));
    let calendar = id(&runner.app().calendar);
    focus(&mut host, calendar);
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(shown(&mut host, calendar), (2026, 12));
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(shown(&mut host, calendar), (2026, 11));
    assert_eq!(calendar_log(&runner), [d(2026, 12, 1), d(2026, 11, 24)]);
}

#[test]
fn page_up_and_page_down_move_a_month_and_home_goes_to_today() {
    let (mut runner, mut host) = setup_at(d(2026, 3, 31));
    let calendar = id(&runner.app().calendar);
    focus(&mut host, calendar);
    named(&mut runner, &mut host, NamedKey::PageUp);
    named(&mut runner, &mut host, NamedKey::PageDown);
    named(&mut runner, &mut host, NamedKey::PageDown);
    named(&mut runner, &mut host, NamedKey::Home);
    // 31 March back a month is the end of February (the 28th), and on from there.
    assert_eq!(
        calendar_log(&runner),
        [
            d(2026, 2, 28),
            d(2026, 3, 28),
            d(2026, 4, 28),
            d(2026, 11, 20)
        ]
    );
    named(&mut runner, &mut host, NamedKey::Home);
    assert_eq!(calendar_log(&runner).len(), 4, "today again is no change");
}

#[test]
fn a_disabled_calendar_ignores_clicks_and_keys() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    let at = cell(&mut host, calendar, d(2026, 11, 17));
    let next = button(&mut host, calendar, CalendarPart::Next);
    runner.send(&mut host, Msg::Disable);
    click_at(&mut runner, &mut host, at);
    click_at(&mut runner, &mut host, next);
    assert_ne!(host.only().focused_widget_id(), Some(calendar));
    assert_eq!(shown(&mut host, calendar), (2026, 11));
    assert!(runner.app().log.is_empty());
}

#[test]
fn the_app_can_select_another_date_without_a_message() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    runner.send(&mut host, Msg::Set(d(2025, 2, 14)));
    assert!(runner.app().log.is_empty());
    assert_eq!(shown(&mut host, calendar), (2025, 2));
    assert_eq!(
        a11y_value(&mut host, calendar).as_deref(),
        Some("14 February 2025")
    );
}

#[test]
fn the_calendar_is_a_grid_of_weeks_for_assistive_technology() {
    let (mut runner, mut host) = setup();
    let calendar = id(&runner.app().calendar);
    let harness = host.only();
    let _ = harness.render();
    let node = harness.access_node(calendar).expect("a node");
    assert_eq!(node.role(), Role::Group);
    assert_eq!(node.label().as_deref(), Some("Calendar"));
    assert_eq!(node.value().as_deref(), Some("5 November 2026"));

    let buttons: Vec<_> = node
        .children()
        .filter(|c| c.role() == Role::Button)
        .filter_map(|c| c.label())
        .collect();
    assert_eq!(buttons, ["Previous month", "Today", "Next month"]);

    let grid = node
        .children()
        .find(|c| c.role() == Role::Grid)
        .expect("a grid");
    assert_eq!(grid.label().as_deref(), Some("November 2026"));
    let rows: Vec<_> = grid.children().collect();
    assert_eq!(rows.len(), 7, "the weekday names and six weeks");
    assert!(rows.iter().all(|r| r.role() == Role::Row));
    let names: Vec<_> = rows[0].children().filter_map(|c| c.label()).collect();
    assert_eq!(names, ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]);
    assert!(rows[0].children().all(|c| c.role() == Role::ColumnHeader));

    let cells: Vec<_> = rows[1..].iter().flat_map(|r| r.children()).collect();
    assert_eq!(cells.len(), 42);
    assert!(cells.iter().all(|c| c.role() == Role::GridCell));
    assert_eq!(cells[0].label().as_deref(), Some("26 October 2026"));
    assert_eq!(cells[41].label().as_deref(), Some("6 December 2026"));
    let selected: Vec<_> = cells
        .iter()
        .filter(|c| c.is_selected() == Some(true))
        .filter_map(|c| c.label())
        .collect();
    assert_eq!(selected, ["5 November 2026"]);
    let today: Vec<_> = cells
        .iter()
        .filter(|c| c.description().as_deref() == Some("Today"))
        .filter_map(|c| c.label())
        .collect();
    assert_eq!(today, ["20 November 2026"]);

    // Increment and Decrement move the selection a day.
    a11y(&mut host, NodeId(calendar.to_raw()), Action::Increment);
    deliver(&mut runner, &mut host);
    assert_eq!(calendar_log(&runner), [d(2026, 11, 6)]);
}
