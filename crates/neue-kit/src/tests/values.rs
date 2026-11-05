//! The 2.13 value controls (slider, stepper, progress bar, spinner) end to end on the fake
//! host: messages, pointer, keyboard, assistive technology, changes from the app, and the
//! ambient animation.

use masonry_core::accesskit::{Action, ActionData, ActionRequest, NodeId, Role, TreeId};
use masonry_core::core::WidgetId;
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::kurbo::Point;
use std::sync::Arc;

use super::*;
use crate::{ProgressBar, Slider, Spinner, Stepper};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Slide(f64),
    Step(f64),
    /// Script: the app moves controls itself.
    Set,
    /// Script: the app changes the progress bar and the spinner.
    Progress(Option<f64>),
    Stop,
    Disable,
}

#[derive(Default)]
struct Values {
    slider: Handle<Slider<Msg>>,
    stepper: Handle<Stepper<Msg>>,
    bar: Handle<ProgressBar>,
    busy: Handle<ProgressBar>,
    spinner: Handle<Spinner>,
    log: Vec<Msg>,
}

impl Application for Values {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .with(
                Slider::new()
                    .range(0.0..=100.0)
                    .step(5.0)
                    .value(50.0)
                    .width(100.0)
                    .on_change(Msg::Slide)
                    .bind(&mut self.slider),
            )
            .with(
                Stepper::new()
                    .range(0.0..=3.0)
                    .value(1.0)
                    .on_change(Msg::Step)
                    .bind(&mut self.stepper),
            )
            .with(
                ProgressBar::new()
                    .value(0.25)
                    .label("Copying")
                    .bind(&mut self.bar),
            )
            .with(ProgressBar::new().bind(&mut self.busy))
            .with(Spinner::new().label("Loading").bind(&mut self.spinner));
        cx.open_window(Window::new("Values", (300, 300), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Set => {
                self.slider.set_value(cx, 80.0);
                self.stepper.set_value(cx, 3.0);
                return;
            }
            Msg::Progress(value) => {
                self.bar.set_value(cx, value);
                return;
            }
            Msg::Stop => {
                self.spinner.set_spinning(cx, false);
                return;
            }
            Msg::Disable => {
                self.slider.set_enabled(cx, false);
                self.stepper.set_enabled(cx, false);
                return;
            }
            _ => {}
        }
        self.log.push(message);
    }
}

fn setup() -> (Runner<Values>, FakeHost) {
    let (runner, mut host, _) = start(Values::default());
    let _ = host.only().render();
    (runner, host)
}

fn key(runner: &mut Runner<Values>, host: &mut FakeHost, widget: WidgetId, key: Key) {
    let harness = host.only();
    harness.focus_on(Some(widget));
    tap(harness, &key);
    deliver(runner, host);
}

fn named(key: NamedKey) -> Key {
    Key::Named(key)
}

/// The window position `(fx, fy)` of the way across `widget`'s box.
fn point_in(host: &mut FakeHost, widget: WidgetId, fx: f64, fy: f64) -> Point {
    let widget = host.only().get_widget_with_id(widget);
    let ctx = widget.ctx();
    let size = ctx.border_box().size();
    ctx.to_window(Point::new(size.width * fx, size.height * fy))
}

fn numeric(host: &mut FakeHost, widget: WidgetId) -> Option<f64> {
    let _ = host.only().render();
    host.only().access_node(widget)?.numeric_value()
}

fn access(
    runner: &mut Runner<Values>,
    host: &mut FakeHost,
    widget: WidgetId,
    action: Action,
    data: Option<ActionData>,
) {
    host.only().process_access_event(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: NodeId(widget.to_raw()),
        data,
    });
    deliver(runner, host);
}

#[test]
fn clicking_the_slider_jumps_to_that_value_on_a_step() {
    let (mut runner, mut host) = setup();
    let slider = id(&runner.app().slider);
    // The knob's centre travels from 7.5 to 92.5 of 100 px: 25 px in is about a fifth.
    let at = point_in(&mut host, slider, 0.25, 0.5);
    host.only().mouse_move(at);
    host.only().mouse_button_press(None);
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Slide(20.0)]);
    assert_eq!(numeric(&mut host, slider), Some(20.0));
}

#[test]
fn dragging_the_slider_follows_the_pointer_and_stops_at_the_ends() {
    let (mut runner, mut host) = setup();
    let slider = id(&runner.app().slider);
    let start = point_in(&mut host, slider, 0.5, 0.5);
    host.only().mouse_move(start);
    host.only().mouse_button_press(None);
    // Pressing on the knob's centre changes nothing.
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty());
    for fx in [0.75, 1.5, -0.5] {
        let to = point_in(&mut host, slider, fx, 0.5);
        host.only().mouse_move(to);
        deliver(&mut runner, &mut host);
    }
    host.only().mouse_button_release(None);
    let log = &runner.app().log;
    assert_eq!(log.last(), Some(&Msg::Slide(0.0)));
    assert!(log.contains(&Msg::Slide(100.0)), "the right end: {log:?}");
    let values: Vec<f64> = log
        .iter()
        .filter_map(|m| {
            if let Msg::Slide(v) = m {
                Some(*v)
            } else {
                None
            }
        })
        .collect();
    assert!(values.windows(2).all(|w| w[0] != w[1]), "no repeats");
    // Moving the pointer without the button changes nothing.
    let before = runner.app().log.len();
    let away = point_in(&mut host, slider, 0.3, 0.5);
    host.only().mouse_move(away);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log.len(), before);
}

#[test]
fn slider_keys_move_by_step_page_and_to_the_ends() {
    let (mut runner, mut host) = setup();
    let slider = id(&runner.app().slider);
    key(&mut runner, &mut host, slider, named(NamedKey::ArrowRight));
    key(&mut runner, &mut host, slider, named(NamedKey::ArrowUp));
    key(&mut runner, &mut host, slider, named(NamedKey::ArrowLeft));
    key(&mut runner, &mut host, slider, named(NamedKey::PageUp));
    key(&mut runner, &mut host, slider, named(NamedKey::End));
    key(&mut runner, &mut host, slider, named(NamedKey::End));
    key(&mut runner, &mut host, slider, named(NamedKey::Home));
    key(&mut runner, &mut host, slider, named(NamedKey::PageDown));
    assert_eq!(
        runner.app().log,
        [
            Msg::Slide(55.0),
            Msg::Slide(60.0),
            Msg::Slide(55.0),
            Msg::Slide(65.0),
            Msg::Slide(100.0),
            Msg::Slide(0.0),
        ]
    );
}

#[test]
fn assistive_technology_reads_and_sets_the_slider() {
    let (mut runner, mut host) = setup();
    let slider = id(&runner.app().slider);
    let _ = host.only().render();
    let node = host.only().access_node(slider).expect("node");
    assert_eq!(node.role(), Role::Slider);
    assert_eq!(node.numeric_value(), Some(50.0));
    assert_eq!(node.min_numeric_value(), Some(0.0));
    assert_eq!(node.max_numeric_value(), Some(100.0));
    assert_eq!(node.numeric_value_step(), Some(5.0));
    assert!(node.data().supports_action(Action::SetValue));

    access(&mut runner, &mut host, slider, Action::Increment, None);
    access(
        &mut runner,
        &mut host,
        slider,
        Action::SetValue,
        Some(ActionData::NumericValue(31.0)),
    );
    // 31 snaps to 30; a value that is already current sends nothing.
    access(
        &mut runner,
        &mut host,
        slider,
        Action::SetValue,
        Some(ActionData::NumericValue(30.0)),
    );
    assert_eq!(runner.app().log, [Msg::Slide(55.0), Msg::Slide(30.0)]);
}

#[test]
fn changes_from_the_app_send_no_message() {
    let (mut runner, mut host) = setup();
    let (slider, stepper) = (id(&runner.app().slider), id(&runner.app().stepper));
    runner.send(&mut host, Msg::Set);
    assert!(runner.app().log.is_empty());
    assert_eq!(numeric(&mut host, slider), Some(80.0));
    assert_eq!(numeric(&mut host, stepper), Some(3.0));
}

#[test]
fn the_stepper_halves_add_and_subtract_and_stop_at_the_ends() {
    let (mut runner, mut host) = setup();
    let stepper = id(&runner.app().stepper);
    let click = |runner: &mut Runner<Values>, host: &mut FakeHost, fy: f64| {
        let at = point_in(host, stepper, 0.5, fy);
        host.only().mouse_move(at);
        host.only().mouse_button_press(None);
        host.only().mouse_button_release(None);
        deliver(runner, host);
    };
    click(&mut runner, &mut host, 0.2);
    click(&mut runner, &mut host, 0.2);
    click(&mut runner, &mut host, 0.2); // at 3: no message
    click(&mut runner, &mut host, 0.8);
    assert_eq!(
        runner.app().log,
        [Msg::Step(2.0), Msg::Step(3.0), Msg::Step(2.0)]
    );
    assert_eq!(numeric(&mut host, stepper), Some(2.0));
}

#[test]
fn stepper_keys_and_assistive_technology_step() {
    let (mut runner, mut host) = setup();
    let stepper = id(&runner.app().stepper);
    key(&mut runner, &mut host, stepper, named(NamedKey::ArrowDown));
    key(&mut runner, &mut host, stepper, named(NamedKey::ArrowDown));
    access(&mut runner, &mut host, stepper, Action::Increment, None);
    assert_eq!(runner.app().log, [Msg::Step(0.0), Msg::Step(1.0)]);
    let node = host.only().access_node(stepper).expect("node");
    assert_eq!(node.role(), Role::SpinButton);
    assert!(node.data().supports_action(Action::Decrement));
}

#[test]
fn disabled_controls_ignore_input() {
    let (mut runner, mut host) = setup();
    let (slider, stepper) = (id(&runner.app().slider), id(&runner.app().stepper));
    runner.send(&mut host, Msg::Disable);
    let at = point_in(&mut host, slider, 0.9, 0.5);
    host.only().mouse_move(at);
    host.only().mouse_button_press(None);
    host.only().mouse_button_release(None);
    let at = point_in(&mut host, stepper, 0.5, 0.2);
    host.only().mouse_move(at);
    host.only().mouse_button_press(None);
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty());
}

#[test]
fn a_progress_bar_reports_its_value_and_an_indeterminate_one_has_none() {
    let (mut runner, mut host) = setup();
    let (bar, busy) = (id(&runner.app().bar), id(&runner.app().busy));
    let _ = host.only().render();
    let node = host.only().access_node(bar).expect("node");
    assert_eq!(node.role(), Role::ProgressIndicator);
    assert_eq!(node.label().as_deref(), Some("Copying"));
    assert_eq!(node.numeric_value(), Some(0.25));
    assert_eq!(numeric(&mut host, busy), None);

    runner.send(&mut host, Msg::Progress(Some(7.0)));
    assert_eq!(numeric(&mut host, bar), Some(1.0), "kept inside 0..1");
    runner.send(&mut host, Msg::Progress(None));
    assert_eq!(numeric(&mut host, bar), None);
}

#[test]
fn the_fill_springs_to_a_new_value_and_comes_to_rest() {
    let (mut runner, mut host) = setup();
    // The barber pole and the spinner keep the window animating; stop them to see the
    // fill alone.
    runner.send(&mut host, Msg::Stop);
    let first = host.only().render();
    runner.send(&mut host, Msg::Progress(Some(0.9)));
    host.only().animate_ms(30);
    assert_ne!(first, host.only().render(), "the fill moved");
}

#[test]
fn the_barber_pole_and_the_spinner_animate_until_stopped() {
    let (mut runner, mut host) = setup();
    let first = host.only().render();
    assert!(host.only().render_root().needs_anim());
    host.only().animate_ms(150);
    assert_ne!(first, host.only().render(), "the stripes moved");

    runner.send(&mut host, Msg::Stop);
    host.only().animate_ms(16);
    host.only().animate_ms(16);
    // Only the barber pole is left.
    assert!(host.only().render_root().needs_anim());
    let busy = id(&runner.app().busy);
    let node = host.only().access_node(busy).expect("node");
    assert_eq!(node.role(), Role::ProgressIndicator);
}

#[test]
fn reduced_motion_stops_the_ambient_animation() {
    let theme = Theme {
        reduced_motion: true,
        ..Theme::default()
    };
    let mut runner = Runner::new(
        Values::default(),
        Settings {
            theme,
            ..Settings::default()
        },
    );
    let mut host = FakeHost {
        theme: Theme {
            reduced_motion: true,
            ..Theme::default()
        },
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    let _ = host.only().render();
    // The first frame notices reduced motion and stops asking for more.
    host.only().animate_ms(16);
    let still = host.only().render();
    assert!(!host.only().render_root().needs_anim());
    host.only().animate_ms(300);
    assert_eq!(still, host.only().render(), "nothing moved");
}
