//! The 2.22c toolbar items end to end on the fake host: layout (spacing, flexible and fixed
//! spaces, separators, the bar's height), the display modes, presses by pointer, key and
//! assistive technology, Tab through the items, and what assistive technology sees.

use masonry_core::accesskit::Role;
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::kurbo::{Point, Rect};
use neue_paint::icon::Glyph;
use neue_theme::materials::title_bar::TOOLBAR_INSET;
use neue_theme::materials::toolbar::{ITEM_SPACING, SEPARATOR_WIDTH, SPACE_WIDTH};

use super::table::click_with;
use super::*;
use crate::widgets::ToolbarButtonWidget;
use crate::{DisplayMode, Segmented, TextField, Toolbar, ToolbarButton};

const WIDTH: f64 = 500.0;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Back,
    Forward,
    View(usize),
    /// Test commands, handled with a `Cx`.
    SetMode(DisplayMode),
    Edit,
}

#[derive(Default)]
struct Tools {
    mode: DisplayMode,
    toolbar: Handle<Toolbar<Msg>>,
    back: Handle<ToolbarButton<Msg>>,
    forward: Handle<ToolbarButton<Msg>>,
    view: Handle<Segmented<Msg>>,
    search: Handle<TextField<Msg>>,
    log: Vec<Msg>,
}

impl Application for Tools {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let toolbar = Toolbar::new()
            .with(
                ToolbarButton::new("Back", Glyph::Folder)
                    .on_press(Msg::Back)
                    .bind(&mut self.back),
            )
            .with(
                ToolbarButton::new("Forward", Glyph::Document)
                    .on_press(Msg::Forward)
                    .disabled(true)
                    .bind(&mut self.forward),
            )
            .space()
            .separator()
            .with(
                Segmented::new(["List", "Icons"])
                    .on_select(Msg::View)
                    .bind(&mut self.view),
            )
            .flexible_space()
            .with(TextField::search().width(120.0).bind(&mut self.search))
            .display_mode(self.mode)
            .bind(&mut self.toolbar);
        let content = Label::new("Content");
        cx.open_window(Window::new("Tools", (WIDTH as u32, 200), content).toolbar(toolbar));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::SetMode(mode) => self.toolbar.set_display_mode(cx, mode),
            Msg::Edit => {
                self.back.set_label(cx, "Previous");
                self.back.set_icon(cx, Glyph::Star);
                self.forward.set_enabled(cx, true);
            }
            message => self.log.push(message),
        }
    }
}

fn setup_with(mode: DisplayMode) -> (Runner<Tools>, FakeHost) {
    let (runner, mut host, _) = start(Tools {
        mode,
        ..Tools::default()
    });
    let _ = host.only().render();
    (runner, host)
}

fn setup() -> (Runner<Tools>, FakeHost) {
    setup_with(DisplayMode::IconAndText)
}

/// A widget's rectangle in window coordinates.
fn rect<W>(host: &mut FakeHost, handle: &Handle<W>) -> Rect {
    let _ = host.only().render();
    let widget = host.only().get_widget_with_id(id(handle));
    let origin = widget.ctx().to_window(Point::ORIGIN);
    let size = widget.ctx().border_box().size();
    Rect::from_origin_size(origin, size)
}

fn take_log(runner: &mut Runner<Tools>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

fn space() -> Key {
    Key::Character(" ".into())
}

#[test]
fn items_sit_left_to_right_with_the_item_spacing_and_flexible_space_pushes_the_rest_right() {
    let (mut runner, mut host) = setup();
    let app = runner.app();
    let (back, forward) = (rect(&mut host, &app.back), rect(&mut host, &app.forward));
    let (view, search) = (rect(&mut host, &app.view), rect(&mut host, &app.search));

    assert_eq!(back.x0, TOOLBAR_INSET, "the first item starts at the inset");
    assert_eq!(
        forward.x0 - back.x1,
        ITEM_SPACING,
        "neighbours are one spacing apart"
    );
    // Fixed space, separator and the gaps around them sit between "Forward" and the segments.
    let between = view.x0 - forward.x1;
    let expected = ITEM_SPACING + SPACE_WIDTH + ITEM_SPACING + SEPARATOR_WIDTH + ITEM_SPACING;
    assert!((between - expected).abs() <= 1.0, "{between} vs {expected}");
    // The flexible space takes everything else: the search field ends at the right inset.
    assert!(
        (search.x1 - (WIDTH - TOOLBAR_INSET)).abs() <= 1.0,
        "search ends at {}",
        search.x1
    );
    assert!(search.x0 > view.x1 + ITEM_SPACING, "there is room between");
    let _ = &mut runner;
}

#[test]
fn items_are_centred_in_the_bars_height() {
    let (runner, mut host) = setup();
    let app = runner.app();
    let back = rect(&mut host, &app.back);
    let view = rect(&mut host, &app.view);
    // The button is the tallest item, so the 22 px segmented control is centred on it.
    assert!(back.height() > view.height());
    assert!(((back.center().y) - view.center().y).abs() <= 1.0);
}

#[test]
fn display_modes_change_the_buttons_size_and_the_bars_height() {
    let height = |mode| {
        let (runner, mut host) = setup_with(mode);
        let back = rect(&mut host, &runner.app().back);
        (back.width(), back.height())
    };
    let (both_w, both_h) = height(DisplayMode::IconAndText);
    let (icon_w, icon_h) = height(DisplayMode::IconOnly);
    let (text_w, text_h) = height(DisplayMode::TextOnly);
    assert!(icon_h < both_h, "no label under the icon");
    assert!(text_h < icon_h, "text is shorter than an icon");
    assert!(icon_w <= both_w);
    assert!(text_w > 0.0);
}

#[test]
fn the_display_mode_can_change_while_the_window_is_open() {
    let (mut runner, mut host) = setup();
    let before = rect(&mut host, &runner.app().back).height();
    runner.send(&mut host, Msg::SetMode(DisplayMode::IconOnly));
    let after = rect(&mut host, &runner.app().back).height();
    assert!(after < before, "{after} < {before}");
}

#[test]
fn a_click_presses_a_button_and_a_disabled_one_sends_nothing() {
    let (mut runner, mut host) = setup();
    let back = rect(&mut host, &runner.app().back).center();
    click_with(&mut runner, &mut host, back, Modifiers::empty(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Back]);

    let forward = rect(&mut host, &runner.app().forward).center();
    click_with(&mut runner, &mut host, forward, Modifiers::empty(), 1);
    assert!(take_log(&mut runner).is_empty());
}

#[test]
fn pressing_a_button_does_not_move_the_window() {
    let (mut runner, mut host) = setup();
    let back = rect(&mut host, &runner.app().back).center();
    click_with(&mut runner, &mut host, back, Modifiers::empty(), 1);
    let root = host
        .specs
        .values()
        .next()
        .and_then(|(root, ..)| *root)
        .unwrap();
    let harness = host.only();
    let widget = harness.get_widget_with_id(root);
    let root = widget.downcast::<crate::widgets::RootWidget>().unwrap();
    assert!(
        root.inner().chrome().unwrap().requests.is_empty(),
        "a press on an item is not a press on the bar"
    );
}

#[test]
fn space_presses_a_focused_button_and_tab_skips_disabled_items_and_spaces() {
    let (mut runner, mut host) = setup();
    host.only().focus_on(Some(id(&runner.app().back)));
    super::tap(host.only(), &space());
    deliver(&mut runner, &mut host);
    assert_eq!(take_log(&mut runner), [Msg::Back]);

    // Tab goes on to the segmented control (the disabled "Forward" button, the space and
    // the separator take no focus), then to the search field.
    super::tap(host.only(), &Key::Named(NamedKey::Tab));
    assert_eq!(
        host.only().focused_widget_id(),
        Some(id(&runner.app().view))
    );
    super::tap(host.only(), &Key::Named(NamedKey::Tab));
    assert_eq!(
        host.only().focused_widget_id(),
        Some(id(&runner.app().search))
    );
}

#[test]
fn items_other_than_buttons_keep_working_inside_the_toolbar() {
    let (mut runner, mut host) = setup();
    let view = rect(&mut host, &runner.app().view);
    click_with(
        &mut runner,
        &mut host,
        Point::new(view.x1 - 5.0, view.center().y),
        Modifiers::empty(),
        1,
    );
    assert_eq!(take_log(&mut runner), [Msg::View(1)]);
}

#[test]
fn assistive_technology_sees_a_toolbar_of_labelled_buttons_in_any_display_mode() {
    let (mut runner, mut host) = setup_with(DisplayMode::IconOnly);
    let toolbar = id(&runner.app().toolbar);
    let node = host.only().access_node(toolbar).expect("a node");
    assert_eq!(node.role(), Role::Toolbar);

    let back = id(&runner.app().back);
    let node = host.only().access_node(back).expect("a node");
    assert_eq!(node.role(), Role::Button);
    assert_eq!(
        node.label().as_deref(),
        Some("Back"),
        "icon-only still has a name"
    );

    a11y_click(host.only(), back);
    deliver(&mut runner, &mut host);
    assert_eq!(take_log(&mut runner), [Msg::Back]);
}

#[test]
fn a_button_can_change_its_label_icon_and_enabled_state() {
    let (mut runner, mut host) = setup();
    let back = runner.app().back;
    runner.send(&mut host, Msg::Edit);
    let _ = host.only().render();
    let node = host.only().access_node(id(&back)).expect("a node");
    assert_eq!(node.label().as_deref(), Some("Previous"));

    let forward = rect(&mut host, &runner.app().forward).center();
    click_with(&mut runner, &mut host, forward, Modifiers::empty(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Forward]);
    let widget = host.only().get_widget_with_id(id(&back));
    assert!(widget.downcast::<ToolbarButtonWidget>().is_some());
}
