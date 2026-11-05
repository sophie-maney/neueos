//! The 2.22b window chrome end to end on the fake host: client-side decorations, layout
//! under a toolbar, the traffic lights (hover, press, act on release), dragging and
//! double-clicking the bar, the inactive look, and what assistive technology sees.

use masonry_core::accesskit::Role;
use masonry_core::core::TextEvent;
use masonry_core::core::keyboard::Modifiers;
use masonry_core::kurbo::Point;
use neue_paint::materials::title_bar::{TITLE_BAR_HEIGHT, TitleBarStyle, TrafficLight};
use neue_theme::materials::title_bar::TOOLBAR_BOTTOM;

use super::table::{click_with, pointer};
use super::*;
use crate::widgets::{RootWidget, WindowRequest};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Act,
}

#[derive(Default)]
struct Chromed {
    /// A toolbar under the title row.
    toolbar: bool,
    /// No chrome at all: the compositor's bar.
    plain: bool,
    tools: Handle<Label>,
    act: Handle<Button<Msg>>,
    content: Handle<Label>,
    log: Vec<Msg>,
}

impl Application for Chromed {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let content = Label::new("Content").bind(&mut self.content);
        let mut window = Window::new("Chrome", (400, 200), content);
        if self.toolbar {
            let toolbar = Stack::row()
                .with(Label::new("Tools").bind(&mut self.tools))
                .with(Button::new("Act").on_press(Msg::Act).bind(&mut self.act));
            window = window.toolbar(toolbar);
        } else if !self.plain {
            window = window.chrome();
        }
        cx.open_window(window);
    }

    fn update(&mut self, _cx: &mut Cx<'_, Msg>, message: Msg) {
        self.log.push(message);
    }
}

fn setup(app: Chromed) -> (Runner<Chromed>, FakeHost) {
    let (mut runner, mut host, _) = start(app);
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn with_toolbar() -> (Runner<Chromed>, FakeHost) {
    setup(Chromed {
        toolbar: true,
        ..Chromed::default()
    })
}

fn root_id(host: &FakeHost) -> WidgetId {
    let (root, ..) = host.specs.values().next().expect("a window");
    root.expect("a focus fallback")
}

/// Runs `f` on the window root's chrome.
fn chrome<R>(host: &mut FakeHost, f: impl FnOnce(&crate::widgets::Chrome) -> R) -> R {
    let root = root_id(host);
    let harness = host.only();
    let widget = harness.get_widget_with_id(root);
    let root = widget.downcast::<RootWidget>().expect("the window root");
    f(root.inner().chrome().expect("chrome"))
}

fn requests(host: &mut FakeHost) -> Vec<WindowRequest> {
    chrome(host, |c| c.requests.clone())
}

fn style(host: &mut FakeHost) -> TitleBarStyle {
    chrome(host, |c| c.style(Theme::default(), true))
}

fn origin(host: &mut FakeHost, id: WidgetId) -> Point {
    let _ = host.only().render();
    host.only()
        .get_widget_with_id(id)
        .ctx()
        .to_window(Point::ORIGIN)
}

fn centre(light: TrafficLight) -> Point {
    light.rect().center()
}

fn click(runner: &mut Runner<Chromed>, host: &mut FakeHost, at: Point, count: u8) {
    click_with(runner, host, at, Modifiers::empty(), count);
}

#[test]
fn chrome_asks_for_client_side_decorations_over_a_transparent_background() {
    let (_, host) = with_toolbar();
    let (_, decorations, background) = host.specs.values().next().copied().unwrap();
    assert_eq!(decorations, Decorations::Client);
    assert_eq!(background, Color::TRANSPARENT);

    let (_, host) = setup(Chromed {
        plain: true,
        ..Chromed::default()
    });
    let (_, decorations, background) = host.specs.values().next().copied().unwrap();
    assert_eq!(decorations, Decorations::Server);
    assert_ne!(background, Color::TRANSPARENT);
}

#[test]
fn the_driver_learns_where_the_chrome_ends() {
    let (_, mut host) = with_toolbar();
    let height = chrome(&mut host, crate::widgets::Chrome::height);
    let shared = host
        .chrome_heights
        .values()
        .next()
        .expect("a chrome height");
    assert!(height > TITLE_BAR_HEIGHT, "{height}");
    assert_eq!(f64::from(shared.get()), height.ceil());

    let (_, host) = setup(Chromed::default());
    let shared = host
        .chrome_heights
        .values()
        .next()
        .expect("a chrome height");
    assert_eq!(f64::from(shared.get()), TITLE_BAR_HEIGHT);

    let (_, host) = setup(Chromed {
        plain: true,
        ..Chromed::default()
    });
    assert!(
        host.chrome_heights.is_empty(),
        "no chrome, nothing to announce"
    );
}

#[test]
fn the_toolbar_sits_under_the_title_row_and_the_content_under_the_chrome() {
    let (runner, mut host) = with_toolbar();
    let (tools, content) = (id(&runner.app().tools), id(&runner.app().content));
    let height = chrome(&mut host, crate::widgets::Chrome::height);
    let toolbar = origin(&mut host, tools);
    assert_eq!(toolbar.y, TITLE_BAR_HEIGHT);
    assert!(toolbar.x > 0.0, "inset from the window's edge: {toolbar:?}");
    assert!(
        height > TITLE_BAR_HEIGHT + TOOLBAR_BOTTOM,
        "the toolbar adds to the title row: {height}"
    );
    assert_eq!(origin(&mut host, content), Point::new(0.0, height));

    let (runner, mut host) = setup(Chromed::default());
    let content = id(&runner.app().content);
    assert_eq!(
        chrome(&mut host, crate::widgets::Chrome::height),
        TITLE_BAR_HEIGHT
    );
    assert_eq!(
        origin(&mut host, content),
        Point::new(0.0, TITLE_BAR_HEIGHT)
    );
}

#[test]
fn traffic_lights_act_when_released_over_the_button_they_were_pressed_on() {
    let (mut runner, mut host) = with_toolbar();
    click(&mut runner, &mut host, centre(TrafficLight::Minimize), 1);
    click(&mut runner, &mut host, centre(TrafficLight::Zoom), 1);
    assert_eq!(
        requests(&mut host),
        [WindowRequest::Minimize, WindowRequest::Zoom]
    );

    // Pressed on close, released over minimise: nothing happens.
    pointer(
        &mut host,
        centre(TrafficLight::Close),
        Modifiers::empty(),
        1,
        true,
    );
    pointer(
        &mut host,
        centre(TrafficLight::Minimize),
        Modifiers::empty(),
        1,
        false,
    );
    deliver(&mut runner, &mut host);
    assert_eq!(requests(&mut host).len(), 2);
    assert_eq!(host.windows.len(), 1);

    // Close goes through the app's close request, which closes by default.
    click(&mut runner, &mut host, centre(TrafficLight::Close), 1);
    assert!(host.windows.is_empty(), "the window closed");
}

#[test]
fn hovering_the_lights_shows_their_glyphs_and_pressing_darkens_one() {
    let (_, mut host) = with_toolbar();
    assert!(!style(&mut host).hover);
    host.only().mouse_move(centre(TrafficLight::Zoom));
    assert!(style(&mut host).hover);
    // Between two lights is still over the group.
    host.only()
        .mouse_move(Point::new(TrafficLight::Close.rect().x1 + 2.0, 11.0));
    assert!(style(&mut host).hover);
    host.only().mouse_move(Point::new(200.0, 11.0));
    assert!(!style(&mut host).hover);

    pointer(
        &mut host,
        centre(TrafficLight::Close),
        Modifiers::empty(),
        1,
        true,
    );
    assert_eq!(style(&mut host).pressed, Some(TrafficLight::Close));
    // Dragged off the button: it lets go of its pressed look, but stays the one pressed.
    host.only().mouse_move(Point::new(200.0, 11.0));
    let s = style(&mut host);
    assert_eq!(s.pressed, None);
    assert!(s.hover, "the glyphs stay while a light is held");
    host.only().mouse_move(centre(TrafficLight::Close));
    assert_eq!(style(&mut host).pressed, Some(TrafficLight::Close));
}

#[test]
fn dragging_the_bar_moves_the_window_and_double_clicking_zooms() {
    let (mut runner, mut host) = with_toolbar();
    let (tools, act, content) = {
        let app = runner.app();
        (id(&app.tools), id(&app.act), id(&app.content))
    };
    pointer(
        &mut host,
        Point::new(200.0, 11.0),
        Modifiers::empty(),
        1,
        true,
    );
    assert_eq!(requests(&mut host), [WindowRequest::Move]);
    pointer(
        &mut host,
        Point::new(200.0, 11.0),
        Modifiers::empty(),
        1,
        false,
    );

    // A bare part of the toolbar (a label) moves the window too.
    let label = origin(&mut host, tools) + (2.0, 2.0);
    pointer(&mut host, label, Modifiers::empty(), 1, true);
    pointer(&mut host, label, Modifiers::empty(), 1, false);
    assert_eq!(requests(&mut host).len(), 2);

    // The second click of a double-click zooms instead.
    click(&mut runner, &mut host, Point::new(200.0, 11.0), 2);
    assert_eq!(requests(&mut host)[2..], [WindowRequest::Zoom]);

    // A toolbar button takes its own clicks; the content isn't part of the bar.
    let button = origin(&mut host, act) + (4.0, 4.0);
    click(&mut runner, &mut host, button, 1);
    assert_eq!(runner.app().log, [Msg::Act]);
    let inside = origin(&mut host, content) + (2.0, 2.0);
    click(&mut runner, &mut host, inside, 1);
    assert_eq!(requests(&mut host).len(), 3);
}

/// The window's pixel at `(x, y)`, freshly rendered.
fn pixel(host: &mut FakeHost, x: u32, y: u32) -> [u8; 4] {
    host.only().render().get_pixel(x, y).0
}

#[test]
fn the_bar_takes_its_inactive_look_when_the_window_loses_focus() {
    let (_, mut host) = setup(Chromed::default());
    host.only()
        .process_text_event(TextEvent::WindowFocusChange(true));
    let focused = pixel(&mut host, 200, 3);
    host.only()
        .process_text_event(TextEvent::WindowFocusChange(false));
    let unfocused = pixel(&mut host, 200, 3);
    assert_ne!(focused, unfocused, "the bar repainted");
    assert!(
        unfocused[0] > focused[0],
        "lighter while inactive: {focused:?} → {unfocused:?}"
    );
}

#[test]
fn assistive_technology_sees_and_clicks_the_traffic_lights() {
    let (mut runner, mut host) = with_toolbar();
    let lights: Vec<WidgetId> = chrome(&mut host, |c| c.children_ids().take(3).collect());
    let labels: Vec<(Role, String)> = lights
        .iter()
        .map(|&light| {
            let node = host.only().access_node(light).expect("a node");
            (node.role(), node.label().unwrap_or_default().to_string())
        })
        .collect();
    assert_eq!(
        labels,
        [
            (Role::Button, "Close".to_owned()),
            (Role::Button, "Minimise".to_owned()),
            (Role::Button, "Zoom".to_owned()),
        ]
    );
    let root = root_id(&host);
    let window = host.only().access_node(root).expect("a node");
    assert_eq!(window.label().as_deref(), Some("Chrome"));

    a11y_click(host.only(), lights[2]);
    deliver(&mut runner, &mut host);
    assert_eq!(requests(&mut host), [WindowRequest::Zoom]);
    a11y_click(host.only(), lights[0]);
    deliver(&mut runner, &mut host);
    assert!(host.windows.is_empty(), "the window closed");
}
