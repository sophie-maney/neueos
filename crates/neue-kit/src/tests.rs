//! The app model end to end, without a platform: a fake [`Host`] keeps each window's tree
//! in a `masonry_testing` harness, and the tests feed the harness's actions to the
//! [`Runner`] as a driver would.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use masonry_core::accesskit::{Action, ActionRequest, NodeId, TreeId};
use masonry_core::app::RenderRoot;
use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};
use masonry_core::core::{NewWidget, TextEvent, WidgetId};
use masonry_core::kurbo::Axis;
use masonry_core::peniko::Color;
use masonry_testing::{TestHarness, TestHarnessParams};
use neue_platform::{Decorations, MenuAddress, WindowId, WindowSpec};
use neue_theme::Theme;

mod chrome;
mod collection;
mod colors;
mod controls;
mod dates;
mod editmenu;
mod fields;
mod grid;
mod huds;
mod icons;
#[cfg(target_os = "linux")]
mod menubar;
mod menus;
mod outline;
mod paths;
mod popovers;
mod popups;
mod scroll;
mod sheets;
mod source_list;
mod split;
mod table;
mod tabs;
mod textview;
mod toolbar;
mod values;

use crate::app::{Host, Runner};
use crate::theme::default_properties;
use crate::widgets::{ButtonWidget, LabelWidget, StackWidget};
use crate::{Application, Button, Cx, Handle, Label, Settings, Stack, Task, Window, Zoom};

/// A window's parent, whether it is modal, and its size.
type DialogSpec = (Option<WindowId>, bool, (u32, u32));

/// Windows as harnesses; the root of each is a stack holding the window's root widget.
#[derive(Default)]
struct FakeHost {
    windows: BTreeMap<WindowId, TestHarness<StackWidget>>,
    next: u64,
    exited: bool,
    /// The theme windows open with.
    theme: Theme,
    /// Each window's menu-bar address, as last set.
    menu_addresses: BTreeMap<WindowId, MenuAddress>,
    /// Each window's root widget (its focus fallback), decorations and background.
    specs: BTreeMap<WindowId, (Option<WidgetId>, Decorations, Color)>,
    /// Each window's parent and whether it is modal, and its size.
    dialogs: BTreeMap<WindowId, DialogSpec>,
    /// Sheets open as windows of their own, as on Wayland.
    modal_windows: bool,
}

impl Host for FakeHost {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId {
        let id = WindowId::from_raw(self.next);
        self.next += 1;
        let mut root = StackWidget::new(Axis::Vertical, 0.0);
        // A chrome spans the window, as a driver lays the root out. (Other roots keep their
        // content's width, which older tests rely on.)
        if spec.decorations == Decorations::Client {
            root.align = crate::Align::Fill;
        }
        root.push(spec.root);
        let mut harness = TestHarness::create_with(
            default_properties(self.theme),
            NewWidget::new(root),
            TestHarnessParams::default().with_size((spec.width, spec.height)),
        );
        harness.set_focus_fallback(spec.focus_fallback);
        self.windows.insert(id, harness);
        self.specs
            .insert(id, (spec.focus_fallback, spec.decorations, spec.background));
        self.dialogs
            .insert(id, (spec.parent, spec.modal, (spec.width, spec.height)));
        id
    }
    fn close_window(&mut self, window: WindowId) {
        self.windows.remove(&window);
    }
    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot> {
        self.windows.get_mut(&window).map(TestHarness::render_root)
    }
    fn windows(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }
    fn exit(&mut self) {
        self.exited = true;
    }
    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>) {
        match address {
            Some(address) => self.menu_addresses.insert(window, address),
            None => self.menu_addresses.remove(&window),
        };
    }
    fn has_modal_windows(&self) -> bool {
        self.modal_windows
    }
}

impl FakeHost {
    fn only(&mut self) -> &mut TestHarness<StackWidget> {
        assert_eq!(self.windows.len(), 1, "one window open");
        self.windows.values_mut().next().unwrap()
    }
}

/// Starts `app` on a fake host. Wakes are counted.
fn start<A: Application>(app: A) -> (Runner<A>, FakeHost, Arc<AtomicUsize>) {
    let mut runner = Runner::new(app, Settings::default());
    let mut host = FakeHost::default();
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = wakes.clone();
    runner.start(
        &mut host,
        Arc::new(move || {
            w.fetch_add(1, Ordering::Relaxed);
        }),
    );
    (runner, host, wakes)
}

/// Hands every action the harnesses emitted to the runner, as the drivers' `pump` does.
fn deliver<A: Application>(runner: &mut Runner<A>, host: &mut FakeHost) {
    loop {
        // Signals from the app's edits (a popover's new layer), as a driver handles them
        // after each callback.
        for harness in host.windows.values_mut() {
            harness.edit_root_widget(|_| ());
        }
        let action = host
            .windows
            .iter_mut()
            .find_map(|(&window, harness)| Some((window, harness.pop_action_erased()?)));
        let Some((window, (action, widget))) = action else {
            break;
        };
        runner.action(host, window, widget, action);
    }
}

/// An assistive technology's click. (The harness's `accessibility_click_on` doesn't
/// collect the actions it causes.)
fn a11y_click(harness: &mut TestHarness<StackWidget>, widget: WidgetId) {
    a11y_action(harness, widget, Action::Click);
}

/// An assistive technology's request for `action` on `widget`.
fn a11y_action(harness: &mut TestHarness<StackWidget>, widget: WidgetId, action: Action) {
    harness.process_access_event(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: NodeId(widget.to_raw()),
        data: None,
    });
}

fn id<W>(handle: &Handle<W>) -> WidgetId {
    handle.id().expect("bound handle")
}

fn label_text(host: &mut FakeHost, handle: &Handle<Label>) -> String {
    let harness = host.only();
    let widget = harness.get_widget_with_id(id(handle));
    widget
        .downcast::<LabelWidget>()
        .expect("a label")
        .inner()
        .text()
        .to_owned()
}

#[derive(Default)]
struct Counter {
    count: i64,
    value: Handle<Label>,
    decrement: Handle<Button<Msg>>,
    increment: Handle<Button<Msg>>,
}

#[derive(Debug, Clone)]
enum Msg {
    Inc,
    Dec,
}

impl Application for Counter {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .with(Label::new("0").bind(&mut self.value))
            .with(
                Button::new("−")
                    .on_press(Msg::Dec)
                    .disabled(true)
                    .bind(&mut self.decrement),
            )
            .with(
                Button::new("+")
                    .on_press(Msg::Inc)
                    .bind(&mut self.increment),
            );
        cx.open_window(Window::new("Counter", (200, 120), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Inc => self.count += 1,
            Msg::Dec => self.count -= 1,
        }
        self.value.set_text(cx, self.count.to_string());
        self.decrement.set_enabled(cx, self.count > 0);
    }
}

#[test]
fn clicks_become_messages_and_update_widgets() {
    let (mut runner, mut host, _) = start(Counter::default());
    let (inc, dec, value) = {
        let app = runner.app();
        (app.increment, app.decrement, app.value)
    };
    for _ in 0..2 {
        host.only().mouse_click_on(id(&inc), None);
        deliver(&mut runner, &mut host);
    }
    assert_eq!(label_text(&mut host, &value), "2");

    host.only().mouse_click_on(id(&dec), None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().count, 1);
    assert_eq!(label_text(&mut host, &value), "1");
}

/// A button that enables another, and nothing else changes on screen.
#[derive(Default)]
struct Toggle {
    toggle: Handle<Button<bool>>,
    target: Handle<Button<bool>>,
    enabled: bool,
    target_pressed: bool,
}

impl Application for Toggle {
    type Message = bool;

    fn start(&mut self, cx: &mut Cx<'_, bool>) {
        let root = Stack::row()
            .with(Button::new("Toggle").on_press(false).bind(&mut self.toggle))
            .with(
                Button::new("Target")
                    .on_press(true)
                    .disabled(true)
                    .bind(&mut self.target),
            );
        cx.open_window(Window::new("Toggle", (300, 60), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, bool>, target: bool) {
        if target {
            self.target_pressed = true;
        } else {
            self.enabled = !self.enabled;
            self.target.set_enabled(cx, self.enabled);
        }
    }
}

#[test]
fn disabled_buttons_send_nothing_and_enabling_repaints() {
    let (mut runner, mut host, _) = start(Toggle::default());
    let (toggle, target) = (runner.app().toggle, runner.app().target);
    let harness = host.only();
    a11y_click(harness, id(&target));
    deliver(&mut runner, &mut host);
    assert!(!runner.app().target_pressed, "disabled");

    let harness = host.only();
    harness.mouse_move_to(id(&toggle));
    let disabled = harness.render();
    a11y_click(harness, id(&toggle));
    deliver(&mut runner, &mut host);
    let enabled = host.only().render();
    // Masonry's disabled pass doesn't repaint; the button must ask for it.
    assert_ne!(disabled, enabled, "enabling repainted the button");

    a11y_click(host.only(), id(&target));
    deliver(&mut runner, &mut host);
    assert!(runner.app().target_pressed, "enabled");
}

fn tap(harness: &mut TestHarness<StackWidget>, key: &Key) {
    for state in [KeyState::Down, KeyState::Up] {
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state,
            key: key.clone(),
            ..KeyboardEvent::default()
        }));
    }
}

const SPACE: fn() -> Key = || Key::Character(" ".into());
const RETURN: Key = Key::Named(NamedKey::Enter);

#[test]
fn space_presses_a_focused_button_and_return_does_not() {
    let (mut runner, mut host, _) = start(Counter::default());
    let inc = runner.app().increment;
    let harness = host.only();
    harness.focus_on(Some(id(&inc)));
    tap(harness, &SPACE());
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().count, 1);
    // Return belongs to the default button, and this window has none.
    tap(host.only(), &RETURN);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().count, 1);
}

/// A dialog: "Cancel", and "Save" as the default button.
#[derive(Default)]
struct Dialog {
    cancel: Handle<Button<&'static str>>,
    save: Handle<Button<&'static str>>,
    pressed: Vec<&'static str>,
}

impl Application for Dialog {
    type Message = &'static str;

    fn start(&mut self, cx: &mut Cx<'_, &'static str>) {
        let root = Stack::row()
            .with(
                Button::new("Cancel")
                    .on_press("cancel")
                    .bind(&mut self.cancel),
            )
            .with(
                Button::new("Save")
                    .on_press("save")
                    .as_default()
                    .bind(&mut self.save),
            );
        cx.open_window(Window::new("Dialog", (300, 60), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, &'static str>, message: &'static str) {
        match message {
            "disable" => self.save.set_enabled(cx, false),
            pressed => self.pressed.push(pressed),
        }
    }
}

#[test]
fn return_presses_the_default_button_from_anywhere_in_the_window() {
    let (mut runner, mut host, _) = start(Dialog::default());
    let cancel = runner.app().cancel;
    // Nothing focused: the window root (the focus fallback) takes Return.
    tap(host.only(), &RETURN);
    deliver(&mut runner, &mut host);
    // Another button focused: Return bubbles past it; Space presses it.
    host.only().focus_on(Some(id(&cancel)));
    tap(host.only(), &RETURN);
    deliver(&mut runner, &mut host);
    tap(host.only(), &SPACE());
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().pressed, ["save", "save", "cancel"]);

    // A disabled default button ignores Return.
    host.only().focus_on(None);
    runner.send(&mut host, "disable");
    tap(host.only(), &RETURN);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().pressed.len(), 3);
}

#[test]
fn the_default_button_tells_assistive_technology_its_shortcut() {
    let (runner, mut host, _) = start(Dialog::default());
    let (cancel, save) = (runner.app().cancel, runner.app().save);
    let harness = host.only();
    let _ = harness.render();
    let shortcut = |id| {
        harness
            .access_node(id)
            .expect("node")
            .data()
            .keyboard_shortcut()
            .map(str::to_owned)
    };
    assert_eq!(shortcut(id(&save)).as_deref(), Some("Return"));
    assert_eq!(shortcut(id(&cancel)), None);
}

fn pulse_of(host: &mut FakeHost, button: WidgetId) -> f64 {
    host.only()
        .get_widget_with_id(button)
        .downcast::<ButtonWidget>()
        .expect("a button")
        .inner()
        .pulse()
}

#[test]
fn the_default_button_pulses_only_in_an_active_window() {
    let (runner, mut host, _) = start(Dialog::default());
    let (cancel, save) = (id(&runner.app().cancel), id(&runner.app().save));
    let half = u64::try_from(neue_theme::motion::DEFAULT_BUTTON_PULSE.as_millis() / 2).unwrap();
    for _ in 0..2 {
        host.only().animate_ms(half / 2);
    }
    assert!(
        (pulse_of(&mut host, save) - 1.0).abs() < 1e-9,
        "peak at half a period"
    );
    assert_eq!(
        pulse_of(&mut host, cancel),
        0.0,
        "only the default button pulses"
    );

    // Inactive window: the pulse rests (and stops asking for frames).
    host.only()
        .process_text_event(TextEvent::WindowFocusChange(false));
    host.only().animate_ms(16);
    assert_eq!(pulse_of(&mut host, save), 0.0);
}

#[test]
fn reduced_motion_keeps_the_default_button_still() {
    let mut runner = Runner::new(
        Dialog::default(),
        Settings {
            theme: Theme {
                reduced_motion: true,
                ..Theme::default()
            },
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
    let save = id(&runner.app().save);
    host.only().animate_ms(300);
    host.only().animate_ms(300);
    assert_eq!(pulse_of(&mut host, save), 0.0);
}

#[test]
fn button_access_node_has_its_label_and_click() {
    let (runner, mut host, _) = start(Counter::default());
    let inc = runner.app().increment;
    let harness = host.only();
    let _ = harness.render();
    let node = harness.access_node(id(&inc)).expect("node");
    assert_eq!(node.role(), masonry_core::accesskit::Role::Button);
    assert_eq!(node.label().as_deref(), Some("+"));
    assert!(node.data().supports_action(Action::Click));
}

/// Adds a button per message into a stack, and clears it.
#[derive(Default)]
struct Dynamic {
    list: Handle<Stack<DynMsg>>,
    add: Handle<Button<DynMsg>>,
    pressed: Vec<u32>,
}

#[derive(Debug, Clone)]
enum DynMsg {
    Add,
    Clear,
    Pressed(u32),
}

impl Application for Dynamic {
    type Message = DynMsg;

    fn start(&mut self, cx: &mut Cx<'_, DynMsg>) {
        let root = Stack::column()
            .with(Button::new("Add").on_press(DynMsg::Add).bind(&mut self.add))
            .with(Stack::column().bind(&mut self.list));
        cx.open_window(Window::new("Dynamic", (200, 300), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, DynMsg>, message: DynMsg) {
        match message {
            DynMsg::Add => {
                let n = u32::try_from(self.pressed.len()).unwrap_or(0);
                self.list.push(
                    cx,
                    Button::new(format!("Item {n}")).on_press(DynMsg::Pressed(n)),
                );
            }
            DynMsg::Clear => self.list.clear(cx),
            DynMsg::Pressed(n) => self.pressed.push(n),
        }
    }
}

#[test]
fn pushed_children_send_messages_and_clearing_drops_their_handlers() {
    let (mut runner, mut host, _) = start(Dynamic::default());
    let add = runner.app().add;
    host.only().mouse_click_on(id(&add), None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.handler_count(), 2, "Add and the new item");

    // The new item is the list's only child.
    let list = runner.app().list;
    let item = {
        let harness = host.only();
        let list = harness.get_widget_with_id(id(&list));
        list.children()[0].id()
    };
    host.only().mouse_click_on(item, None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().pressed, [0]);

    runner.send(&mut host, DynMsg::Clear);
    assert_eq!(runner.handler_count(), 1, "only Add is left");
    assert!(!host.only().render_root().has_widget(item));
}

/// Spawns work, optionally cancelling it, and records results.
#[derive(Default)]
struct Worker {
    results: Vec<u32>,
    task: Option<Task>,
}

#[derive(Debug)]
enum WorkMsg {
    Start(u32),
    Cancel,
    Done(u32),
}

impl Application for Worker {
    type Message = WorkMsg;

    fn start(&mut self, cx: &mut Cx<'_, WorkMsg>) {
        cx.open_window(Window::new("Worker", (100, 100), Label::new("")));
    }

    fn update(&mut self, cx: &mut Cx<'_, WorkMsg>, message: WorkMsg) {
        match message {
            WorkMsg::Start(n) => {
                self.task = Some(cx.spawn(move |_| {
                    std::thread::sleep(Duration::from_millis(20));
                    WorkMsg::Done(n)
                }));
            }
            WorkMsg::Cancel => {
                if let Some(task) = self.task.take() {
                    task.cancel();
                }
            }
            WorkMsg::Done(n) => self.results.push(n),
        }
    }
}

/// Drains the mailbox, as `on_wake` does, until `done` or a timeout.
fn wait_for<A: Application>(
    runner: &mut Runner<A>,
    host: &mut FakeHost,
    done: impl Fn(&A) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        runner.drain(host);
        if done(runner.app()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

#[test]
fn background_results_arrive_through_the_mailbox_after_a_wake() {
    let (mut runner, mut host, wakes) = start(Worker::default());
    runner.send(&mut host, WorkMsg::Start(5));
    assert!(wait_for(&mut runner, &mut host, |app| app.results == [5]));
    assert!(wakes.load(Ordering::Relaxed) >= 1, "the loop was woken");
}

#[test]
fn cancelled_tasks_never_reach_update() {
    let (mut runner, mut host, _) = start(Worker::default());
    runner.send(&mut host, WorkMsg::Start(1));
    runner.send(&mut host, WorkMsg::Cancel);
    runner.send(&mut host, WorkMsg::Start(2));
    assert!(wait_for(&mut runner, &mut host, |app| !app
        .results
        .is_empty()));
    std::thread::sleep(Duration::from_millis(50));
    runner.drain(&mut host);
    assert_eq!(runner.app().results, [2]);
}

#[test]
fn closing_a_window_drops_its_handlers() {
    let (mut runner, mut host, _) = start(Counter::default());
    assert_eq!(runner.handler_count(), 2);
    let window = host.windows()[0];
    runner.close_requested(&mut host, window);
    assert!(host.windows.is_empty());
    assert_eq!(runner.handler_count(), 0);
}

/// A button inside a 2× zoom, and a theme switch.
#[derive(Default)]
struct Zoomed {
    zoom: Handle<Zoom<ZoomMsg>>,
    button: Handle<Button<ZoomMsg>>,
    pressed: usize,
}

#[derive(Debug, Clone)]
enum ZoomMsg {
    Pressed,
    Dark,
}

impl Application for Zoomed {
    type Message = ZoomMsg;

    fn start(&mut self, cx: &mut Cx<'_, ZoomMsg>) {
        let button = Button::new("Big")
            .on_press(ZoomMsg::Pressed)
            .bind(&mut self.button);
        let root = Stack::column().with(Zoom::new(2.0, button).bind(&mut self.zoom));
        cx.open_window(Window::new("Zoom", (300, 100), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, ZoomMsg>, message: ZoomMsg) {
        match message {
            ZoomMsg::Pressed => self.pressed += 1,
            ZoomMsg::Dark => cx.set_theme(Theme {
                variant: neue_theme::Variant::new(neue_theme::Appearance::Dark),
                ..cx.theme()
            }),
        }
    }
}

#[test]
fn zoom_scales_its_child_and_clicks_still_land() {
    let (mut runner, mut host, _) = start(Zoomed::default());
    let (zoom, button) = (id(&runner.app().zoom), id(&runner.app().button));
    let harness = host.only();
    let outer = harness.get_widget_with_id(zoom).ctx().border_box().size();
    let inner = harness.get_widget_with_id(button).ctx().border_box().size();
    assert_eq!(outer, inner * 2.0);
    assert_eq!(inner.height, neue_theme::metrics::control::REGULAR_HEIGHT);

    harness.mouse_click_on(button, None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().pressed, 1);
}

#[test]
fn set_theme_repaints_open_windows() {
    let (mut runner, mut host, _) = start(Zoomed::default());
    let corner = |host: &mut FakeHost| *host.only().render().get_pixel(1, 1);
    let light = corner(&mut host);
    runner.send(&mut host, ZoomMsg::Dark);
    let dark = corner(&mut host);
    assert!(dark.0[0] < light.0[0], "{light:?} → {dark:?}");
    assert_eq!(
        runner.app().pressed,
        0,
        "switching themes sends no other messages"
    );
}
