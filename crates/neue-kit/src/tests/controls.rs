//! The 2.12 controls (checkbox, radio button, bevel and recessed buttons, segmented
//! control) end to end on the fake host: messages, keyboard, assistive technology, and
//! changes from the app.

use masonry_core::accesskit::{Action, Toggled};
use masonry_core::core::WidgetId;
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::kurbo::Point;
use masonry_testing::TestHarness;

use super::*;
use crate::widgets::StackWidget;
use crate::{BevelButton, CheckState, Checkbox, Radio, Segmented};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Check(bool),
    Select(&'static str),
    Press,
    Bevel(bool),
    Segment(usize),
    /// Script: the app changes controls itself.
    Set,
    /// Script: the app disables controls.
    Disable,
}

#[derive(Default)]
struct Controls {
    checkbox: Handle<Checkbox<Msg>>,
    mixed: Handle<Checkbox<Msg>>,
    radio_a: Handle<Radio<Msg>>,
    radio_b: Handle<Radio<Msg>>,
    momentary: Handle<BevelButton<Msg>>,
    toggle: Handle<BevelButton<Msg>>,
    segments: Handle<Segmented<Msg>>,
    log: Vec<Msg>,
}

impl Application for Controls {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .with(
                Checkbox::new("Check")
                    .on_toggle(Msg::Check)
                    .bind(&mut self.checkbox),
            )
            .with(
                Checkbox::new("Some")
                    .state(CheckState::Mixed)
                    .on_toggle(Msg::Check)
                    .bind(&mut self.mixed),
            )
            .with(
                Radio::new("A")
                    .selected(true)
                    .on_select(Msg::Select("A"))
                    .bind(&mut self.radio_a),
            )
            .with(
                Radio::new("B")
                    .on_select(Msg::Select("B"))
                    .bind(&mut self.radio_b),
            )
            .with(
                BevelButton::recessed("Press")
                    .on_press(Msg::Press)
                    .bind(&mut self.momentary),
            )
            .with(
                BevelButton::new("Bold")
                    .on_toggle(Msg::Bevel)
                    .bind(&mut self.toggle),
            )
            .with(
                Segmented::new(["One", "Two", "Three"])
                    .on_select(Msg::Segment)
                    .bind(&mut self.segments),
            );
        cx.open_window(Window::new("Controls", (300, 300), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            // The app owns the radio choice: turn the other button off.
            Msg::Select("A") => self.radio_b.set_selected(cx, false),
            Msg::Select(_) => self.radio_a.set_selected(cx, false),
            Msg::Set => {
                self.checkbox.set_checked(cx, true);
                self.radio_b.set_selected(cx, true);
                self.toggle.set_on(cx, true);
                self.segments.set_selected(cx, 2);
                return;
            }
            Msg::Disable => {
                self.checkbox.set_enabled(cx, false);
                self.segments.set_enabled(cx, false);
                return;
            }
            _ => {}
        }
        self.log.push(message);
    }
}

fn setup() -> (Runner<Controls>, FakeHost) {
    let (runner, mut host, _) = start(Controls::default());
    // Lay out and render once so nodes and positions exist.
    let _ = host.only().render();
    (runner, host)
}

fn click(runner: &mut Runner<Controls>, host: &mut FakeHost, widget: WidgetId) {
    host.only().mouse_click_on(widget, None);
    deliver(runner, host);
}

fn key(runner: &mut Runner<Controls>, host: &mut FakeHost, widget: WidgetId, key: Key) {
    let harness = host.only();
    harness.focus_on(Some(widget));
    tap(harness, &key);
    deliver(runner, host);
}

fn toggled(harness: &mut TestHarness<StackWidget>, widget: WidgetId) -> Option<Toggled> {
    let _ = harness.render();
    harness.access_node(widget)?.toggled()
}

#[test]
fn a_checkbox_toggles_on_click_and_space_and_reports_its_state() {
    let (mut runner, mut host) = setup();
    let checkbox = id(&runner.app().checkbox);
    assert_eq!(toggled(host.only(), checkbox), Some(Toggled::False));

    click(&mut runner, &mut host, checkbox);
    assert_eq!(runner.app().log, [Msg::Check(true)]);
    assert_eq!(toggled(host.only(), checkbox), Some(Toggled::True));

    key(&mut runner, &mut host, checkbox, SPACE());
    assert_eq!(runner.app().log, [Msg::Check(true), Msg::Check(false)]);
    assert_eq!(toggled(host.only(), checkbox), Some(Toggled::False));
}

#[test]
fn a_mixed_checkbox_turns_on_when_clicked() {
    let (mut runner, mut host) = setup();
    let mixed = id(&runner.app().mixed);
    assert_eq!(toggled(host.only(), mixed), Some(Toggled::Mixed));
    click(&mut runner, &mut host, mixed);
    assert_eq!(runner.app().log, [Msg::Check(true)]);
    assert_eq!(toggled(host.only(), mixed), Some(Toggled::True));
}

#[test]
fn assistive_technology_toggles_a_checkbox() {
    let (mut runner, mut host) = setup();
    let checkbox = id(&runner.app().checkbox);
    let _ = host.only().render();
    let node = host.only().access_node(checkbox).expect("node");
    assert_eq!(node.role(), masonry_core::accesskit::Role::CheckBox);
    assert_eq!(node.label().as_deref(), Some("Check"));
    assert!(node.data().supports_action(Action::Click));
    a11y_click(host.only(), checkbox);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Check(true)]);
}

#[test]
fn changes_from_the_app_send_no_message() {
    let (mut runner, mut host) = setup();
    let checkbox = runner.app().checkbox;
    let radio_b = runner.app().radio_b;
    let toggle = runner.app().toggle;
    let segments = runner.app().segments;
    runner.send(&mut host, Msg::Set);
    assert!(runner.app().log.is_empty());
    assert_eq!(toggled(host.only(), id(&checkbox)), Some(Toggled::True));
    assert_eq!(toggled(host.only(), id(&radio_b)), Some(Toggled::True));
    assert_eq!(toggled(host.only(), id(&toggle)), Some(Toggled::True));
    let node = host.only().access_node(id(&segments)).expect("node");
    assert_eq!(node.value().as_deref(), Some("Three"));
}

#[test]
fn a_radio_button_only_turns_on_and_the_app_turns_the_others_off() {
    let (mut runner, mut host) = setup();
    let (a, b) = (id(&runner.app().radio_a), id(&runner.app().radio_b));
    assert_eq!(toggled(host.only(), a), Some(Toggled::True));

    // Clicking the selected one changes nothing.
    click(&mut runner, &mut host, a);
    assert!(runner.app().log.is_empty());

    click(&mut runner, &mut host, b);
    assert_eq!(runner.app().log, [Msg::Select("B")]);
    assert_eq!(toggled(host.only(), b), Some(Toggled::True));
    assert_eq!(toggled(host.only(), a), Some(Toggled::False));
    let node = host.only().access_node(b).expect("node");
    assert_eq!(node.role(), masonry_core::accesskit::Role::RadioButton);
}

#[test]
fn bevel_buttons_press_or_toggle() {
    let (mut runner, mut host) = setup();
    let (momentary, toggle) = (id(&runner.app().momentary), id(&runner.app().toggle));
    click(&mut runner, &mut host, momentary);
    click(&mut runner, &mut host, momentary);
    assert_eq!(runner.app().log, [Msg::Press, Msg::Press]);
    assert_eq!(toggled(host.only(), momentary), None, "not a toggle");

    click(&mut runner, &mut host, toggle);
    key(&mut runner, &mut host, toggle, SPACE());
    assert_eq!(runner.app().log[2..], [Msg::Bevel(true), Msg::Bevel(false)]);
    assert_eq!(toggled(host.only(), toggle), Some(Toggled::False));
}

#[test]
fn disabled_controls_ignore_input() {
    let (mut runner, mut host) = setup();
    let checkbox = runner.app().checkbox;
    let segments = runner.app().segments;
    runner.send(&mut host, Msg::Disable);
    click(&mut runner, &mut host, id(&checkbox));
    click(&mut runner, &mut host, id(&segments));
    assert!(runner.app().log.is_empty());
}

/// Clicks `index` of the three segments by pointing at its centre.
fn click_segment(runner: &mut Runner<Controls>, host: &mut FakeHost, index: usize) {
    let segments = id(&runner.app().segments);
    let harness = host.only();
    let at = {
        let widget = harness.get_widget_with_id(segments);
        let ctx = widget.ctx();
        let size = ctx.border_box().size();
        // Equal thirds: every label is shorter than the minimum width.
        let local = Point::new(size.width * (index as f64 + 0.5) / 3.0, size.height / 2.0);
        ctx.to_window(local)
    };
    harness.mouse_move(at);
    harness.mouse_button_press(None);
    harness.mouse_button_release(None);
    deliver(runner, host);
}

#[test]
fn clicking_a_segment_selects_it() {
    let (mut runner, mut host) = setup();
    click_segment(&mut runner, &mut host, 2);
    click_segment(&mut runner, &mut host, 2);
    click_segment(&mut runner, &mut host, 0);
    assert_eq!(runner.app().log, [Msg::Segment(2), Msg::Segment(0)]);
}

#[test]
fn arrow_keys_move_the_selection_and_stop_at_the_ends() {
    let (mut runner, mut host) = setup();
    let segments = id(&runner.app().segments);
    let left = || Key::Named(NamedKey::ArrowLeft);
    let right = || Key::Named(NamedKey::ArrowRight);
    key(&mut runner, &mut host, segments, left());
    assert!(runner.app().log.is_empty(), "already at the first segment");
    key(&mut runner, &mut host, segments, right());
    key(&mut runner, &mut host, segments, right());
    key(&mut runner, &mut host, segments, right());
    key(&mut runner, &mut host, segments, left());
    assert_eq!(
        runner.app().log,
        [Msg::Segment(1), Msg::Segment(2), Msg::Segment(1)]
    );
    let _ = host.only().render();
    let node = host.only().access_node(segments).expect("node");
    assert_eq!(node.value().as_deref(), Some("Two"));
}

#[test]
fn assistive_technology_steps_through_the_segments() {
    let (mut runner, mut host) = setup();
    let segments = id(&runner.app().segments);
    let _ = host.only().render();
    let node = host.only().access_node(segments).expect("node");
    assert_eq!(node.role(), masonry_core::accesskit::Role::RadioGroup);
    assert!(node.data().supports_action(Action::Increment));
    host.only()
        .process_access_event(masonry_core::accesskit::ActionRequest {
            action: Action::Increment,
            target_tree: masonry_core::accesskit::TreeId::ROOT,
            target_node: masonry_core::accesskit::NodeId(segments.to_raw()),
            data: None,
        });
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Segment(1)]);
}

#[test]
fn the_segment_highlight_springs_to_the_new_segment() {
    let (mut runner, mut host) = setup();
    let segments = id(&runner.app().segments);
    key(
        &mut runner,
        &mut host,
        segments,
        Key::Named(NamedKey::ArrowRight),
    );
    let first = host.only().render();
    host.only().animate_ms(20);
    let mid = host.only().render();
    assert_ne!(first, mid, "the highlight moved");
    while host.only().render_root().needs_anim() {
        host.only().animate_ms(16);
    }
    let rest = host.only().render();
    assert_ne!(mid, rest);
    assert!(!host.only().render_root().needs_anim());
}
