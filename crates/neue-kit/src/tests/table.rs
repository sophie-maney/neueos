//! The 2.19a table end to end on the fake host: selecting by click, Shift- and
//! Cmd-click, drag and keys, type-select, activating, sorting by a title, resizing a
//! column, what assistive technology sees, and changes from the app.

use masonry_core::accesskit::{Role, SortDirection};
use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use masonry_core::core::{
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, TextEvent, WidgetId,
};
use masonry_core::dpi::LogicalPosition;
use masonry_core::kurbo::Point;
use masonry_testing::PRIMARY_MOUSE;
use neue_theme::materials::table::{HEADER_HEIGHT, ROW_HEIGHT};

use super::*;
use crate::widgets::{CellWidget, HeaderWidget, RowsWidget, ScrollWidget, TableRowWidget};
use crate::{Cell, Column, SortOrder, Table};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Selected(Vec<usize>),
    Activated(usize),
    Sorted(usize, SortOrder),
    Default,
    /// Script: the app changes the table.
    Select(Vec<usize>),
    Count(usize),
    Sort(Option<(usize, SortOrder)>),
}

#[derive(Default)]
struct Tabling {
    table: Handle<Table<Msg>>,
    single: Handle<Table<Msg>>,
    log: Vec<Msg>,
}

const NAMES: [&str; 6] = ["Apple", "Banana", "Cherry", "Damson", "Elder", "Fig"];

fn name(row: usize) -> String {
    format!("{} {row}", NAMES[row % NAMES.len()])
}

impl Application for Tabling {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let columns = [
            Column::new("Name").width(80.0).sortable(true),
            Column::new("Size")
                .width(50.0)
                .min_width(30.0)
                .max_width(90.0),
            Column::new("Kind").width(60.0).sortable(true),
        ];
        let table = Table::new(columns.clone(), 100, |row, column| -> Cell<Msg> {
            match column {
                0 => name(row).into(),
                1 => format!("{} KB", row * 3).into(),
                _ => "Fruit".into(),
            }
        })
        .multiple(true)
        .on_select(|rows| Msg::Selected(rows.to_vec()))
        .on_activate(Msg::Activated)
        .on_sort(Msg::Sorted)
        .width(200.0)
        .height(120.0)
        .bind(&mut self.table);
        // Single selection, no activation: Return reaches the default button.
        let single = Table::new(columns, 5, |row, _| Cell::<Msg>::text(name(row)))
            .on_select(|rows| Msg::Selected(rows.to_vec()))
            .width(200.0)
            .height(130.0)
            .bind(&mut self.single);
        let root = Stack::column()
            .with(table)
            .with(single)
            .with(Button::new("Default").on_press(Msg::Default).as_default());
        cx.open_window(Window::new("Tables", (220, 300), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Select(rows) => self.table.set_selection(cx, rows),
            Msg::Count(n) => self.table.set_count(cx, n),
            Msg::Sort(sorted) => self.table.set_sorted(cx, sorted),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Tabling>, FakeHost) {
    let (mut runner, mut host, _) = start(Tabling::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn scroll(host: &mut FakeHost, table: WidgetId) -> &ScrollWidget {
    host.only()
        .get_widget_with_id(table)
        .downcast::<ScrollWidget>()
        .expect("a table's scroll view")
        .inner()
}

fn body_id(host: &mut FakeHost, table: WidgetId) -> WidgetId {
    scroll(host, table).content_id()
}

fn header_id(host: &mut FakeHost, table: WidgetId) -> WidgetId {
    scroll(host, table).header_id().expect("a header")
}

fn body(host: &mut FakeHost, table: WidgetId) -> &RowsWidget {
    let id = body_id(host, table);
    host.only()
        .get_widget_with_id(id)
        .downcast::<RowsWidget>()
        .expect("the body")
        .inner()
}

fn selected(host: &mut FakeHost, table: WidgetId) -> Vec<usize> {
    let body = body(host, table);
    body.table
        .as_ref()
        .expect("a table")
        .selection()
        .rows
        .into_iter()
        .collect()
}

/// The window point at `(x, y)` in the body (content coordinates, before scrolling).
fn in_body(host: &mut FakeHost, table: WidgetId, x: f64, y: f64) -> Point {
    let id = body_id(host, table);
    host.only()
        .get_widget_with_id(id)
        .ctx()
        .to_window(Point::new(x, y))
}

/// The window point in the middle of row `row`.
fn row_point(host: &mut FakeHost, table: WidgetId, row: usize) -> Point {
    #[expect(clippy::cast_precision_loss, reason = "small rows")]
    let y = (row as f64 + 0.5) * ROW_HEIGHT;
    in_body(host, table, 20.0, y)
}

fn pointer(host: &mut FakeHost, at: Point, mods: Modifiers, count: u8, down: bool) {
    let harness = host.only();
    harness.mouse_move(at);
    let scale = 1.0;
    let mut state = masonry_core::core::PointerState {
        position: LogicalPosition::new(at.x, at.y).to_physical(scale),
        modifiers: mods,
        count,
        scale_factor: scale,
        ..Default::default()
    };
    if down {
        state.buttons.insert(PointerButton::Primary);
    }
    let event = PointerButtonEvent {
        pointer: PRIMARY_MOUSE,
        button: Some(PointerButton::Primary),
        state,
    };
    harness.process_pointer_event(if down {
        PointerEvent::Down(event)
    } else {
        PointerEvent::Up(event)
    });
}

fn click_with(
    runner: &mut Runner<Tabling>,
    host: &mut FakeHost,
    at: Point,
    mods: Modifiers,
    count: u8,
) {
    pointer(host, at, mods, count, true);
    pointer(host, at, mods, count, false);
    deliver(runner, host);
}

fn command() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    }
}

fn key_with(runner: &mut Runner<Tabling>, host: &mut FakeHost, key: Key, mods: Modifiers) {
    for state in [KeyState::Down, KeyState::Up] {
        host.only()
            .process_text_event(TextEvent::Keyboard(KeyboardEvent {
                state,
                key: key.clone(),
                modifiers: mods,
                ..KeyboardEvent::default()
            }));
    }
    deliver(runner, host);
}

fn key(runner: &mut Runner<Tabling>, host: &mut FakeHost, named: NamedKey) {
    key_with(runner, host, Key::Named(named), Modifiers::empty());
}

fn take_log(runner: &mut Runner<Tabling>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

#[test]
fn clicks_select_and_shift_and_command_extend_the_selection() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let at = row_point(&mut host, table, 2);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![2])]);
    assert_eq!(
        host.only().focused_widget_id(),
        Some(body_id(&mut host, table)),
        "a click focuses the table"
    );
    let at = row_point(&mut host, table, 4);
    click_with(&mut runner, &mut host, at, Modifiers::SHIFT, 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![2, 3, 4])]);
    let at = row_point(&mut host, table, 0);
    click_with(&mut runner, &mut host, at, command(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![0, 2, 3, 4])]);
    click_with(&mut runner, &mut host, at, command(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![2, 3, 4])]);
    // The same click again changes nothing, and sends nothing.
    let at = row_point(&mut host, table, 3);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![3])]);
}

#[test]
fn single_selection_ignores_shift_and_a_click_below_the_rows_deselects() {
    let (mut runner, mut host) = setup();
    let single = id(&runner.app().single);
    let at = row_point(&mut host, single, 1);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    let at = row_point(&mut host, single, 3);
    click_with(&mut runner, &mut host, at, Modifiers::SHIFT, 1);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![1]), Msg::Selected(vec![3])]
    );
    // Five rows of 19 px fill less than the view (111 px): a click under them deselects.
    let at = in_body(&mut host, single, 20.0, 5.0 * ROW_HEIGHT + 5.0);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![])]);
}

#[test]
fn dragging_extends_the_selection() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let start = row_point(&mut host, table, 1);
    pointer(&mut host, start, Modifiers::empty(), 1, true);
    let end = row_point(&mut host, table, 3);
    let mut state = masonry_core::core::PointerState {
        position: LogicalPosition::new(end.x, end.y).to_physical(1.0),
        scale_factor: 1.0,
        ..Default::default()
    };
    state.buttons.insert(PointerButton::Primary);
    host.only()
        .process_pointer_event(PointerEvent::Move(PointerUpdate {
            pointer: PRIMARY_MOUSE,
            current: state,
            coalesced: vec![],
            predicted: vec![],
        }));
    pointer(&mut host, end, Modifiers::empty(), 1, false);
    deliver(&mut runner, &mut host);
    assert_eq!(selected(&mut host, table), [1, 2, 3]);
}

#[test]
fn keys_move_extend_and_jump_and_keep_the_row_in_view() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let body = body_id(&mut host, table);
    host.only().focus_on(Some(body));
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(
        selected(&mut host, table),
        [0],
        "nothing selected: Down takes the first"
    );
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    key_with(
        &mut runner,
        &mut host,
        Key::Named(NamedKey::ArrowDown),
        Modifiers::SHIFT,
    );
    assert_eq!(selected(&mut host, table), [1, 2]);
    key(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(selected(&mut host, table), [1]);
    key(&mut runner, &mut host, NamedKey::End);
    assert_eq!(selected(&mut host, table), [99]);
    let _ = host.only().render();
    let offset = scroll(&mut host, table).offset().y;
    // 100 rows in a 120 px view less the header and the borders.
    let view = 120.0 - HEADER_HEIGHT - 2.0;
    assert_eq!(
        offset,
        100.0 * ROW_HEIGHT - view,
        "scrolled to the last row"
    );
    key(&mut runner, &mut host, NamedKey::PageUp);
    let page = (view / ROW_HEIGHT).floor() as usize - 1;
    assert_eq!(selected(&mut host, table), [99 - page]);
    key(&mut runner, &mut host, NamedKey::Home);
    assert_eq!(selected(&mut host, table), [0]);
    let _ = host.only().render();
    assert_eq!(scroll(&mut host, table).offset().y, 0.0);
    key_with(
        &mut runner,
        &mut host,
        Key::Character("a".into()),
        command(),
    );
    assert_eq!(selected(&mut host, table).len(), 100, "select all");
    let log = take_log(&mut runner);
    assert_eq!(log.len(), 8, "one message per change: {log:?}");
}

#[test]
fn typing_selects_the_first_row_starting_with_what_was_typed() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let body = body_id(&mut host, table);
    host.only().focus_on(Some(body));
    key_with(
        &mut runner,
        &mut host,
        Key::Character("c".into()),
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, table), [2], "Cherry 2");
    key_with(
        &mut runner,
        &mut host,
        Key::Character("h".into()),
        Modifiers::empty(),
    );
    key_with(
        &mut runner,
        &mut host,
        Key::Character("e".into()),
        Modifiers::empty(),
    );
    key_with(
        &mut runner,
        &mut host,
        Key::Character("r".into()),
        Modifiers::empty(),
    );
    key_with(
        &mut runner,
        &mut host,
        Key::Character("r".into()),
        Modifiers::empty(),
    );
    key_with(
        &mut runner,
        &mut host,
        Key::Character("y".into()),
        Modifiers::empty(),
    );
    key_with(
        &mut runner,
        &mut host,
        Key::Character(" ".into()),
        Modifiers::empty(),
    );
    key_with(
        &mut runner,
        &mut host,
        Key::Character("8".into()),
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, table), [8], "Cherry 8");
}

#[test]
fn return_and_double_click_activate_and_without_a_handler_return_goes_on() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let at = row_point(&mut host, table, 4);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    key(&mut runner, &mut host, NamedKey::Enter);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 2);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![4]), Msg::Activated(4), Msg::Activated(4)]
    );
    let single = id(&runner.app().single);
    let at = row_point(&mut host, single, 0);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    key(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![0]), Msg::Default]
    );
}

/// The window point `x` along the header, in its middle.
fn header_point(host: &mut FakeHost, table: WidgetId, x: f64) -> Point {
    let header = header_id(host, table);
    host.only()
        .get_widget_with_id(header)
        .ctx()
        .to_window(Point::new(x, HEADER_HEIGHT / 2.0))
}

#[test]
fn a_click_on_a_sortable_title_sorts_and_again_reverses() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let name = header_point(&mut host, table, 30.0);
    click_with(&mut runner, &mut host, name, Modifiers::empty(), 1);
    click_with(&mut runner, &mut host, name, Modifiers::empty(), 1);
    let size = header_point(&mut host, table, 100.0);
    click_with(&mut runner, &mut host, size, Modifiers::empty(), 1);
    let kind = header_point(&mut host, table, 160.0);
    click_with(&mut runner, &mut host, kind, Modifiers::empty(), 1);
    assert_eq!(
        take_log(&mut runner),
        [
            Msg::Sorted(0, SortOrder::Ascending),
            Msg::Sorted(0, SortOrder::Descending),
            Msg::Sorted(2, SortOrder::Ascending),
        ],
        "Size isn't sortable"
    );
    let _ = host.only().render();
    let header = header_id(&mut host, table);
    let titles: Vec<WidgetId> = host
        .only()
        .get_widget_with_id(header)
        .children()
        .iter()
        .map(|c| c.id())
        .collect();
    let direction = |host: &mut FakeHost, i: usize| {
        host.only()
            .access_node(titles[i])
            .and_then(|n| n.sort_direction())
    };
    assert_eq!(direction(&mut host, 0), None);
    assert_eq!(direction(&mut host, 2), Some(SortDirection::Ascending));
    // The app moves the indicator; nothing is sent.
    runner.app_mut().log.clear();
    runner.send(&mut host, Msg::Sort(Some((0, SortOrder::Descending))));
    let _ = host.only().render();
    assert_eq!(direction(&mut host, 0), Some(SortDirection::Descending));
    assert_eq!(direction(&mut host, 2), None);
    assert!(take_log(&mut runner).is_empty());
}

fn cell_width(host: &mut FakeHost, table: WidgetId, row: usize, column: usize) -> f64 {
    let rows = body(host, table);
    let row = rows.row_id(row).expect("row built");
    let cells = host
        .only()
        .get_widget_with_id(row)
        .downcast::<TableRowWidget>()
        .expect("a row")
        .inner()
        .cell_ids();
    host.only()
        .get_widget_with_id(cells[column])
        .ctx()
        .border_box()
        .width()
}

#[test]
fn dragging_a_divider_resizes_the_column_within_its_limits() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    assert_eq!(cell_width(&mut host, table, 0, 1), 50.0);
    // Size's right edge is at 80 + 50.
    let grab = header_point(&mut host, table, 130.0);
    host.only().mouse_move(grab);
    host.only().mouse_button_press(None);
    let to = header_point(&mut host, table, 150.0);
    host.only().mouse_move(to);
    let _ = host.only().render();
    assert_eq!(cell_width(&mut host, table, 0, 1), 70.0);
    let far = header_point(&mut host, table, 400.0);
    host.only().mouse_move(far);
    host.only().mouse_button_release(None);
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    assert_eq!(
        cell_width(&mut host, table, 0, 1),
        90.0,
        "the column's maximum"
    );
    assert!(take_log(&mut runner).is_empty(), "resizing doesn't sort");
    let header = header_id(&mut host, table);
    let widths = host
        .only()
        .get_widget_with_id(header)
        .downcast::<HeaderWidget>()
        .expect("the header")
        .inner()
        .widths()
        .to_vec();
    assert_eq!(widths, [80.0, 90.0, 60.0]);
    // Wider than the view now: it scrolls sideways, and the header follows.
    let range = host
        .only()
        .access_node(table)
        .and_then(|n| n.scroll_x_max());
    assert_eq!(range, Some(80.0 + 90.0 + 60.0 - 198.0));
}

#[test]
fn assistive_technology_sees_a_table_of_rows_and_cells_and_can_select() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    let body_node = body_id(&mut host, table);
    let _ = host.only().render();
    let node = host.only().access_node(body_node).expect("the body's node");
    assert_eq!(node.role(), Role::Table);
    assert_eq!(node.data().row_count(), Some(100));
    assert_eq!(node.data().column_count(), Some(3));
    assert!(node.is_multiselectable());
    let row = body(&mut host, table).row_id(3).expect("row 3 built");
    let node = host.only().access_node(row).expect("a row");
    assert_eq!(node.role(), Role::Row);
    assert_eq!(node.data().row_index(), Some(3));
    let cell = node.children().next().expect("a cell");
    assert_eq!(cell.role(), Role::Cell);
    assert_eq!(cell.label().as_deref(), Some("Damson 3"));
    a11y_click(host.only(), row);
    deliver(&mut runner, &mut host);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![3])]);
    let _ = host.only().render();
    assert_eq!(
        host.only().access_node(row).unwrap().is_selected(),
        Some(true)
    );
    let header = header_id(&mut host, table);
    let node = host.only().access_node(header).expect("the header row");
    let titles: Vec<_> = node
        .children()
        .map(|t| (t.role(), t.label().unwrap_or_default()))
        .collect();
    assert_eq!(
        titles,
        [
            (Role::ColumnHeader, "Name".to_owned()),
            (Role::ColumnHeader, "Size".to_owned()),
            (Role::ColumnHeader, "Kind".to_owned()),
        ]
    );
}

fn look_of(host: &mut FakeHost, table: WidgetId, row: usize) -> String {
    let rows = body(host, table);
    let row = rows.row_id(row).expect("row built");
    let cell = host
        .only()
        .get_widget_with_id(row)
        .downcast::<TableRowWidget>()
        .expect("a row")
        .inner()
        .cell_ids()[0];
    let cell = host
        .only()
        .get_widget_with_id(cell)
        .downcast::<CellWidget>()
        .expect("a cell")
        .inner();
    format!("{cell:?}")
}

#[test]
fn the_app_sets_the_selection_silently_and_rows_built_later_show_it() {
    let (mut runner, mut host) = setup();
    let table = id(&runner.app().table);
    runner.send(&mut host, Msg::Select(vec![1, 60, 500]));
    let _ = host.only().render();
    assert_eq!(selected(&mut host, table), [1, 60], "500 is past the end");
    assert!(look_of(&mut host, table, 1).contains("selected: true"));
    assert!(look_of(&mut host, table, 2).contains("selected: false"));
    // Row 60 isn't built until it scrolls into view.
    assert_eq!(body(&mut host, table).row_id(60), None);
    let body = body_id(&mut host, table);
    host.only().focus_on(Some(body));
    let _ = host.only().render();
    assert!(look_of(&mut host, table, 1).contains("emphasized: true"));
    runner.send(&mut host, Msg::Count(50));
    let _ = host.only().render();
    assert_eq!(selected(&mut host, table), [1], "60 went with the rows");
    assert!(take_log(&mut runner).is_empty());
}
