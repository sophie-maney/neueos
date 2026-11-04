//! The 2.20b virtual grid end to end on the fake host: columns from the width (and on
//! resize), which items exist, scrolling to an item, and handlers of items built and
//! dropped as it scrolls.

use masonry_core::core::WidgetId;
use masonry_core::core::WindowEvent;
use masonry_core::dpi::PhysicalSize;
use masonry_core::kurbo::{Point, Vec2};

use super::*;
use crate::VirtualGrid;
use crate::widgets::{RowsWidget, ScrollWidget};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Item(usize),
    Count(usize),
    Reload,
    ToItem(usize),
}

#[derive(Default)]
struct Gridded {
    grid: Handle<VirtualGrid<Msg>>,
    log: Vec<Msg>,
}

impl Application for Gridded {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        // 40 × 20 cells, the grid filling a 200 × 100 window: 5 columns.
        let grid = VirtualGrid::new(1000, 40.0, 20.0, |i| {
            Button::new(format!("{i}")).on_press(Msg::Item(i))
        })
        .height(100.0)
        .bind(&mut self.grid);
        cx.open_window(Window::new("Grid", (200, 100), grid));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Count(n) => self.grid.set_count(cx, n),
            Msg::Reload => self.grid.reload(cx),
            Msg::ToItem(i) => self.grid.scroll_to_item(cx, i),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Gridded>, FakeHost) {
    let (mut runner, mut host, _) = start(Gridded::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn scroll_widget(host: &mut FakeHost, grid: WidgetId) -> &ScrollWidget {
    host.only()
        .get_widget_with_id(grid)
        .downcast::<ScrollWidget>()
        .expect("a scroll view")
        .inner()
}

fn rows(host: &mut FakeHost, grid: WidgetId) -> &RowsWidget {
    let content = scroll_widget(host, grid).content_id();
    host.only()
        .get_widget_with_id(content)
        .downcast::<RowsWidget>()
        .expect("rows")
        .inner()
}

/// The items that exist, as a range (they are contiguous), so failures stay short.
fn built(host: &mut FakeHost, grid: WidgetId) -> std::ops::Range<usize> {
    let built = rows(host, grid).built();
    assert!(
        built.windows(2).all(|w| w[1] == w[0] + 1),
        "contiguous: {built:?}"
    );
    built.first().copied().unwrap_or(0)..built.last().map_or(0, |last| last + 1)
}

fn resize(runner: &mut Runner<Gridded>, host: &mut FakeHost, width: u32, height: u32) {
    host.only()
        .render_root()
        .handle_window_event(WindowEvent::Resize(PhysicalSize::new(width, height)));
    let _ = host.only().render();
    deliver(runner, host);
}

fn wheel(runner: &mut Runner<Gridded>, host: &mut FakeHost, grid: WidgetId, dy: f64) {
    let widget = host.only().get_widget_with_id(grid);
    let ctx = widget.ctx();
    let at = ctx.to_window(Point::new(20.0, 20.0));
    host.only().mouse_move(at);
    host.only().mouse_wheel(Vec2::new(0.0, dy));
    deliver(runner, host);
    let _ = host.only().render();
}

#[test]
fn the_columns_fill_the_width_and_only_items_near_the_screen_exist() {
    let (runner, mut host) = setup();
    let grid = id(&runner.app().grid);
    assert_eq!(rows(&mut host, grid).columns, 5);
    // 100 px of 20 px rows: rows 0..5 show, half a screen more is built: rows 0..8.
    assert_eq!(built(&mut host, grid), 0..40);
    let _ = host.only().render();
    assert_eq!(
        host.only().access_node(grid).unwrap().scroll_y_max(),
        Some(200.0 * 20.0 - 100.0),
        "1000 items in 5 columns are 200 rows"
    );
}

#[test]
fn items_sit_in_their_cells() {
    let (runner, mut host) = setup();
    let grid = id(&runner.app().grid);
    let item = |host: &mut FakeHost, i| {
        let id = rows(host, grid).row_id(i).expect("built");
        host.only().get_widget_with_id(id).ctx().border_box()
    };
    let origin = |host: &mut FakeHost, i| {
        let id = rows(host, grid).row_id(i).unwrap();
        host.only()
            .get_widget_with_id(id)
            .ctx()
            .to_window(Point::ORIGIN)
    };
    let (a, b, c) = (
        origin(&mut host, 0),
        origin(&mut host, 1),
        origin(&mut host, 5),
    );
    assert_eq!(b.x - a.x, 40.0, "next column");
    assert_eq!(b.y, a.y);
    assert_eq!(c.x, a.x, "the sixth item starts the second row");
    assert_eq!(c.y - a.y, 20.0);
    assert_eq!(item(&mut host, 0).size(), (40.0, 20.0).into());
}

#[test]
fn resizing_changes_the_columns_and_the_items_that_exist() {
    let (mut runner, mut host) = setup();
    let grid = id(&runner.app().grid);
    resize(&mut runner, &mut host, 90, 100);
    assert_eq!(rows(&mut host, grid).columns, 2);
    assert_eq!(built(&mut host, grid), 0..16);
    let _ = host.only().render();
    assert_eq!(
        host.only().access_node(grid).unwrap().scroll_y_max(),
        Some(500.0 * 20.0 - 100.0),
        "1000 items in 2 columns are 500 rows"
    );
    resize(&mut runner, &mut host, 20, 100);
    assert_eq!(rows(&mut host, grid).columns, 1, "narrower than a cell");
    resize(&mut runner, &mut host, 400, 100);
    assert_eq!(rows(&mut host, grid).columns, 10);
    assert_eq!(built(&mut host, grid), 0..80);
}

#[test]
fn scrolling_builds_whole_rows_and_drops_the_rest() {
    let (mut runner, mut host) = setup();
    let grid = id(&runner.app().grid);
    wheel(&mut runner, &mut host, grid, -200.0);
    // Window 200..300 (rows 10..15), 50 px either side: rows 7..18, items 35..90.
    assert_eq!(built(&mut host, grid), 35..90);
}

#[test]
fn the_app_scrolls_to_an_item_changes_the_count_and_reloads() {
    let (mut runner, mut host) = setup();
    let grid = id(&runner.app().grid);
    runner.send(&mut host, Msg::ToItem(502));
    let _ = host.only().render();
    assert_eq!(
        scroll_widget(&mut host, grid).offset().y,
        101.0 * 20.0 - 100.0,
        "item 502 is in row 100, at the bottom"
    );
    assert!(rows(&mut host, grid).built().contains(&502));
    runner.send(&mut host, Msg::Count(7));
    let _ = host.only().render();
    assert_eq!(scroll_widget(&mut host, grid).offset().y, 0.0);
    assert_eq!(built(&mut host, grid), 0..7);
    let before = rows(&mut host, grid).row_id(0);
    runner.send(&mut host, Msg::Reload);
    let _ = host.only().render();
    let after = rows(&mut host, grid).row_id(0);
    assert!(after.is_some() && after != before, "rebuilt");
}

#[test]
fn items_send_messages_and_dropped_items_forget_their_handlers() {
    let (mut runner, mut host) = setup();
    let grid = id(&runner.app().grid);
    let handlers = runner.handler_count();
    let item = rows(&mut host, grid).row_id(7).expect("item 7 is built");
    host.only().mouse_click_on(item, None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Item(7)]);
    for _ in 0..50 {
        wheel(&mut runner, &mut host, grid, -400.0);
        deliver(&mut runner, &mut host);
    }
    assert!(
        runner.handler_count() <= handlers + 40,
        "handlers of dropped items are gone: {} vs {handlers}",
        runner.handler_count()
    );
}
