//! The colour well (2.24b) end to end on the fake host: opening and closing its panel, the
//! area, the sliders and the hex field changing the colour (and each other's view of it),
//! the messages that go out while dragging, and what assistive technologies see.

use masonry_core::accesskit::{Action, Role};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{PointerButton, Widget, WidgetRef};
use masonry_core::kurbo::{Point, Rect};
use neue_paint::materials::{Hsb, color_area_knob, color_hex};

use super::menus::{a11y, find, focused, named, press, typed};
use super::*;
use crate::widgets::FieldWidget;
use crate::widgets::popover::shown;
use crate::{ColorWell, tokens::Color};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Color(Color),
    /// Script: the app shows another colour.
    Set(Color),
    /// Script: the app disables the well.
    Disable,
}

struct Wells {
    initial: Color,
    well: Handle<ColorWell<Msg>>,
    /// The colours the app was sent, in order.
    log: Vec<Color>,
}

impl Application for Wells {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column().padding(200.0).with(
            ColorWell::new(self.initial)
                .label("Background")
                .on_change(Msg::Color)
                .bind(&mut self.well),
        );
        cx.open_window(Window::new("Colours", (560, 640), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Color(color) => self.log.push(color),
            Msg::Set(color) => self.well.set_color(cx, color),
            Msg::Disable => self.well.set_enabled(cx, false),
        }
    }
}

const RED: Color = Color::new([1.0, 0.0, 0.0, 1.0]);

fn setup_with(initial: Color) -> (Runner<Wells>, FakeHost) {
    let (runner, mut host, _) = start(Wells {
        initial,
        well: Handle::default(),
        log: Vec::new(),
    });
    let _ = host.only().render();
    (runner, host)
}

fn setup() -> (Runner<Wells>, FakeHost) {
    setup_with(RED)
}

fn close(a: Color, b: Color) -> bool {
    a.components
        .iter()
        .zip(b.components)
        .all(|(x, y)| (x - y).abs() < 1e-4)
}

fn well_bounds(host: &mut FakeHost) -> Rect {
    find(host, Role::ColorWell, Some("Background"))[0].bounds
}

fn click_at(runner: &mut Runner<Wells>, host: &mut FakeHost, at: Point) {
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn click_well(runner: &mut Runner<Wells>, host: &mut FakeHost) {
    let at = well_bounds(host).center();
    click_at(runner, host, at);
}

fn open(runner: &mut Runner<Wells>, host: &mut FakeHost) {
    click_well(runner, host);
    assert_eq!(find(host, Role::Dialog, None).len(), 1, "one panel");
}

fn is_open(host: &mut FakeHost) -> bool {
    !find(host, Role::Dialog, None).is_empty()
}

fn area(host: &mut FakeHost) -> Rect {
    find(host, Role::Slider, Some("Hue and saturation"))[0].bounds
}

fn slider(host: &mut FakeHost, label: &str) -> Rect {
    find(host, Role::Slider, Some(label))[0].bounds
}

/// The text value and the numeric value of the first node with `role` and `label`.
fn read(host: &mut FakeHost, role: Role, label: &str) -> (Option<String>, Option<f64>) {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == role && node.label().as_deref() == Some(label) {
            return (node.value(), node.numeric_value());
        }
        stack.extend(node.children());
    }
    panic!("no {role:?} labelled {label:?}");
}

/// A slider's value, from its accessibility node.
fn slider_value(host: &mut FakeHost, label: &str) -> f64 {
    read(host, Role::Slider, label)
        .1
        .expect("a slider with a value")
}

/// The first widget of type `W` under `widget`.
fn find_widget<'a, W: Widget + 'static>(
    widget: WidgetRef<'a, dyn Widget>,
) -> Option<WidgetRef<'a, W>> {
    if let Some(found) = widget.downcast::<W>() {
        return Some(found);
    }
    widget.children().into_iter().find_map(find_widget::<W>)
}

/// The panel's hex field: its bounds and text.
fn hex_field(runner: &Runner<Wells>, host: &mut FakeHost) -> (Rect, String) {
    let bounds = find(host, Role::TextInput, Some("Hex colour"))[0].bounds;
    let well = id(&runner.app().well);
    let pop = host
        .only()
        .render_root()
        .edit_widget(well, |w| shown(&w))
        .expect("the panel is open");
    let text = find_widget::<FieldWidget>(host.only().get_widget_with_id(pop))
        .expect("a hex field")
        .inner()
        .text();
    (bounds, text)
}

fn hex_text(runner: &Runner<Wells>, host: &mut FakeHost) -> String {
    hex_field(runner, host).1
}

fn command() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    }
}

/// Clicks in the hex field, selects all of its text and types `text`.
fn type_hex(runner: &mut Runner<Wells>, host: &mut FakeHost, text: &str) {
    let (bounds, _) = hex_field(runner, host);
    click_at(runner, host, bounds.center());
    press(runner, host, Key::Character("a".into()), command());
    typed(runner, host, text);
}

fn last(runner: &Runner<Wells>) -> Color {
    *runner.app().log.last().expect("a colour was sent")
}

#[test]
fn a_press_opens_the_panel_under_the_well() {
    let (mut runner, mut host) = setup();
    let well = well_bounds(&mut host);
    open(&mut runner, &mut host);
    let panel = find(&mut host, Role::Dialog, None)[0].bounds;
    assert!(panel.y0 >= well.y1, "{panel:?} under {well:?}");
    // The area, both sliders and the hex field are in it.
    for bounds in [
        area(&mut host),
        slider(&mut host, "Brightness"),
        slider(&mut host, "Opacity"),
        hex_field(&runner, &mut host).0,
    ] {
        assert!(panel.contains_rect(bounds), "{bounds:?} in {panel:?}");
    }
    assert!(runner.app().log.is_empty(), "opening sends nothing");
}

#[test]
fn the_panel_starts_from_the_wells_colour() {
    let (mut runner, mut host) = setup_with(Color::new([0.2, 0.4, 0.6, 0.5]));
    open(&mut runner, &mut host);
    assert!((slider_value(&mut host, "Brightness") - 0.6).abs() < 1e-4);
    assert!((slider_value(&mut host, "Opacity") - 0.5).abs() < 1e-4);
    assert_eq!(hex_text(&runner, &mut host), "#33669980");
}

#[test]
fn space_and_return_open_it_and_escape_closes_it_and_gives_focus_back() {
    for key in [Key::Character(" ".into()), Key::Named(NamedKey::Enter)] {
        let (mut runner, mut host) = setup();
        let well = id(&runner.app().well);
        host.only().focus_on(Some(well));
        press(&mut runner, &mut host, key, Modifiers::empty());
        assert!(is_open(&mut host));
        named(&mut runner, &mut host, NamedKey::Escape);
        assert!(!is_open(&mut host));
        assert_eq!(host.only().focused_widget_id(), Some(well));
        assert!(runner.app().log.is_empty());
    }
}

#[test]
fn escape_closes_it_from_inside_a_control_too() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    let (bounds, _) = hex_field(&runner, &mut host);
    click_at(&mut runner, &mut host, bounds.center());
    named(&mut runner, &mut host, NamedKey::Escape);
    assert!(!is_open(&mut host));
}

#[test]
fn pressing_the_well_again_closes_it() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    click_well(&mut runner, &mut host);
    assert!(!is_open(&mut host));
    // And the next press opens it again.
    open(&mut runner, &mut host);
}

#[test]
fn a_press_outside_closes_it() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    click_at(&mut runner, &mut host, Point::new(10.0, 10.0));
    assert!(!is_open(&mut host));
}

#[test]
fn dragging_in_the_area_sends_each_colour_on_the_way() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    let rect = area(&mut host);
    let at =
        |fx: f64, fy: f64| Point::new(rect.x0 + rect.width() * fx, rect.y0 + rect.height() * fy);

    host.only().mouse_move(at(0.25, 0.25));
    host.only().mouse_button_press(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    for (fx, fy) in [(0.4, 0.4), (0.5, 0.5)] {
        host.only().mouse_move(at(fx, fy));
        deliver(&mut runner, &mut host);
    }
    host.only()
        .mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);

    assert_eq!(runner.app().log.len(), 3, "one colour per position");
    // Hue 180 at the middle, saturation 0.5 (the top is fully saturated), brightness and
    // opacity untouched.
    let expected = Hsb {
        hue: 180.0,
        saturation: 0.5,
        brightness: 1.0,
        alpha: 1.0,
    };
    assert!(
        close(last(&runner), expected.to_color()),
        "{:?}",
        last(&runner)
    );
    // The other controls follow.
    assert_eq!(hex_text(&runner, &mut host), color_hex(expected.to_color()));
    assert!((slider_value(&mut host, "Brightness") - 1.0).abs() < 1e-4);
}

#[test]
fn the_arrow_keys_move_the_knob() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    // Tab from the panel goes to the area first.
    named(&mut runner, &mut host, NamedKey::Tab);
    assert_eq!(focused(&mut host).as_deref(), Some("Hue and saturation"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    let expected = Hsb {
        hue: 7.2,
        saturation: 0.98,
        brightness: 1.0,
        alpha: 1.0,
    };
    assert_eq!(runner.app().log.len(), 2);
    assert!(close(last(&runner), expected.to_color()));
    // Saturation stops at the top and hue at the left.
    for _ in 0..3 {
        named(&mut runner, &mut host, NamedKey::ArrowUp);
        named(&mut runner, &mut host, NamedKey::ArrowLeft);
    }
    assert!(close(last(&runner), RED));
}

#[test]
fn the_sliders_set_brightness_and_opacity_and_brightness_keeps_the_hue() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    let brightness = slider(&mut host, "Brightness");
    let left = Point::new(brightness.x0 + 1.0, brightness.center().y);
    let right = Point::new(brightness.x1 - 1.0, brightness.center().y);

    click_at(&mut runner, &mut host, left);
    assert!(close(last(&runner), Color::new([0.0, 0.0, 0.0, 1.0])));
    assert_eq!(hex_text(&runner, &mut host), "#000000");
    // Black has no hue of its own: going back up gives the red it came from.
    click_at(&mut runner, &mut host, right);
    assert!(close(last(&runner), RED));

    let opacity = slider(&mut host, "Opacity");
    click_at(
        &mut runner,
        &mut host,
        Point::new(opacity.x0 + 1.0, opacity.center().y),
    );
    assert!(close(last(&runner), Color::new([1.0, 0.0, 0.0, 0.0])));
    assert_eq!(hex_text(&runner, &mut host), "#FF000000");
}

#[test]
fn a_hex_value_sets_the_colour_and_the_other_controls() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    type_hex(&mut runner, &mut host, "#336699");
    let expected = Color::new([0.2, 0.4, 0.6, 1.0]);
    assert!(close(last(&runner), expected), "{:?}", last(&runner));
    assert!((slider_value(&mut host, "Brightness") - 0.6).abs() < 1e-4);
    // What was typed stays as it was typed.
    assert_eq!(hex_text(&runner, &mut host), "#336699");
}

#[test]
fn eight_digits_set_the_opacity_and_six_keep_it() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    type_hex(&mut runner, &mut host, "#11223380");
    assert!((last(&runner).components[3] - 128.0 / 255.0).abs() < 1e-4);
    assert!((slider_value(&mut host, "Opacity") - 128.0 / 255.0).abs() < 1e-4);
    type_hex(&mut runner, &mut host, "#445566");
    assert!((last(&runner).components[3] - 128.0 / 255.0).abs() < 1e-4);
}

#[test]
fn a_bad_hex_sends_nothing_and_return_puts_the_colour_back() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    type_hex(&mut runner, &mut host, "zzz");
    assert!(runner.app().log.is_empty());
    assert_eq!(hex_text(&runner, &mut host), "zzz");
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(hex_text(&runner, &mut host), "#FF0000");
    assert!(runner.app().log.is_empty());
    // Return on a good one tidies it: three digits become six.
    type_hex(&mut runner, &mut host, "#0f0");
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(hex_text(&runner, &mut host), "#00FF00");
    assert!(is_open(&mut host), "Return doesn't close the panel");
}

#[test]
fn the_well_shows_the_colour_and_assistive_technologies_read_it_as_hex() {
    let (mut runner, mut host) = setup_with(Color::new([0.2, 0.4, 0.6, 1.0]));
    let read = |host: &mut FakeHost| read(host, Role::ColorWell, "Background").0;
    assert_eq!(read(&mut host).as_deref(), Some("#336699"));
    open(&mut runner, &mut host);
    type_hex(&mut runner, &mut host, "#FF000080");
    assert_eq!(read(&mut host).as_deref(), Some("#FF000080"));
}

#[test]
fn assistive_technologies_can_open_it() {
    let (mut runner, mut host) = setup();
    let node = find(&mut host, Role::ColorWell, None)[0].node;
    a11y(&mut host, node, Action::Click);
    deliver(&mut runner, &mut host);
    assert!(is_open(&mut host));
}

#[test]
fn the_app_can_show_another_colour_without_a_message() {
    let (mut runner, mut host) = setup();
    let blue = Color::new([0.0, 0.0, 1.0, 1.0]);
    runner.send(&mut host, Msg::Set(blue));
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty());
    // A panel opened now starts from it.
    open(&mut runner, &mut host);
    assert_eq!(hex_text(&runner, &mut host), "#0000FF");
}

#[test]
fn a_disabled_well_ignores_clicks_and_keys() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Disable);
    deliver(&mut runner, &mut host);
    click_well(&mut runner, &mut host);
    assert!(!is_open(&mut host));
    assert!(runner.app().log.is_empty());
}

#[test]
fn the_knob_sits_where_the_colour_is() {
    // The material's knob position and the widget's pointer mapping agree: pressing on the
    // knob leaves the colour as it was.
    let initial = Color::new([0.2, 0.4, 0.6, 1.0]);
    let (mut runner, mut host) = setup_with(initial);
    open(&mut runner, &mut host);
    let rect = area(&mut host);
    let knob = color_area_knob(rect, Hsb::from_color(initial, Hsb::default()));
    click_at(&mut runner, &mut host, knob);
    for sent in &runner.app().log {
        assert!(
            sent.components
                .iter()
                .zip(initial.components)
                .all(|(a, b)| (a - b).abs() < 0.02),
            "{sent:?}"
        );
    }
}
