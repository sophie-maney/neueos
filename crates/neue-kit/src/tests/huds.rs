//! HUD windows (2.23d) end to end on the fake host: client-side decorations over a
//! transparent surface, the dark panel under the content, the close button (pointer and
//! assistive technology), dragging the small title bar, and a parent that keeps its
//! focused look while its HUD window has the keyboard.

use masonry_core::accesskit::Role;
use masonry_core::core::{PointerButton, TextEvent};
use masonry_core::kurbo::Point;
use neue_paint::materials::hud::{HUD_TITLE_HEIGHT, close_rect};

use super::*;
use crate::widgets::{Chrome, RootWidget, WindowRequest};

const PARENT: (u32, u32) = (400, 240);
const HUD: (u32, u32) = (200, 120);

#[derive(Default)]
struct Huds {
    /// Open the HUD window with the parent window as its parent.
    with_parent: bool,
    parent: Option<WindowId>,
    hud: Option<WindowId>,
}

impl Application for Huds {
    type Message = ();

    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let parent =
            cx.open_window(Window::new("Document", PARENT, Label::new("Content")).chrome());
        self.parent = Some(parent);
        let mut hud = Window::new("Inspector", HUD, Label::new("Size").on_hud()).hud();
        if self.with_parent {
            hud = hud.parent(parent);
        }
        self.hud = Some(cx.open_window(hud));
    }

    fn update(&mut self, _cx: &mut Cx<'_, ()>, _message: ()) {}
}

fn setup(with_parent: bool) -> (Runner<Huds>, FakeHost) {
    setup_with(with_parent, false)
}

/// `blur`: the compositor offers backdrop blur.
fn setup_with(with_parent: bool, blur: bool) -> (Runner<Huds>, FakeHost) {
    let mut runner = Runner::new(
        Huds {
            with_parent,
            ..Huds::default()
        },
        Settings::default(),
    );
    let mut host = FakeHost {
        blur,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    for harness in host.windows.values_mut() {
        let _ = harness.render();
    }
    (runner, host)
}

fn hud(runner: &Runner<Huds>) -> WindowId {
    runner.app().hud.expect("the HUD window")
}

fn parent(runner: &Runner<Huds>) -> WindowId {
    runner.app().parent.expect("the parent window")
}

fn root_of(host: &FakeHost, window: WindowId) -> WidgetId {
    host.specs[&window].0.expect("a focus fallback")
}

/// Runs `f` on `window`'s chrome.
fn chrome<R>(host: &mut FakeHost, window: WindowId, f: impl FnOnce(&Chrome) -> R) -> R {
    let root = root_of(host, window);
    let harness = host.windows.get_mut(&window).expect("an open window");
    let widget = harness.get_widget_with_id(root);
    let root = widget.downcast::<RootWidget>().expect("the window root");
    f(root.inner().chrome().expect("chrome"))
}

fn pixel(host: &mut FakeHost, window: WindowId, x: u32, y: u32) -> [u8; 4] {
    let harness = host.windows.get_mut(&window).expect("an open window");
    harness.render().get_pixel(x, y).0
}

fn click(runner: &mut Runner<Huds>, host: &mut FakeHost, window: WindowId, at: Point, count: u8) {
    let harness = host.windows.get_mut(&window).expect("an open window");
    harness.mouse_move(at);
    for _ in 0..count {
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
    }
    deliver(runner, host);
}

fn focus(host: &mut FakeHost, window: WindowId, focused: bool) {
    host.windows
        .get_mut(&window)
        .expect("an open window")
        .process_text_event(TextEvent::WindowFocusChange(focused));
}

#[test]
fn a_hud_window_has_client_side_decorations_over_a_transparent_surface() {
    let (runner, host) = setup(false);
    let (_, decorations, background) = host.specs[&hud(&runner)];
    assert_eq!(decorations, Decorations::Client);
    assert_eq!(background, Color::TRANSPARENT);
}

#[test]
fn a_hud_window_is_a_child_of_its_parent() {
    let (runner, host) = setup(true);
    assert_eq!(host.dialogs[&hud(&runner)].0, Some(parent(&runner)));
}

#[test]
fn the_title_bar_is_small() {
    let (runner, mut host) = setup(false);
    let window = hud(&runner);
    assert_eq!(chrome(&mut host, window, Chrome::height), HUD_TITLE_HEIGHT);
}

#[test]
fn the_panel_is_dark_under_the_title_bar_and_the_content() {
    let (runner, mut host) = setup(false);
    let window = hud(&runner);
    // Light theme, light window: the HUD is dark anyway.
    let body = pixel(&mut host, window, 100, 100);
    assert!(
        body[..3].iter().all(|&c| c < 90),
        "a dark panel in the light theme: {body:?}"
    );
    let bar = pixel(&mut host, window, 100, 3);
    assert!(bar[..3].iter().all(|&c| c < 140), "a dark bar: {bar:?}");
}

#[test]
fn the_close_button_is_the_only_window_control() {
    let (runner, mut host) = setup(false);
    let window = hud(&runner);
    let controls: Vec<WidgetId> = chrome(&mut host, window, |c| c.children_ids().collect());
    assert_eq!(controls.len(), 1);
    let node = host
        .windows
        .get_mut(&window)
        .unwrap()
        .access_node(controls[0]);
    let node = node.expect("a node");
    assert_eq!(node.role(), Role::Button);
    assert_eq!(node.label().as_deref(), Some("Close"));
}

#[test]
fn clicking_the_close_button_asks_the_app_to_close_the_window() {
    let (mut runner, mut host) = setup(false);
    let window = hud(&runner);
    // Clicking the rest of the bar does not.
    click(&mut runner, &mut host, window, Point::new(120.0, 9.0), 1);
    assert!(host.windows.contains_key(&window));
    click(&mut runner, &mut host, window, close_rect().center(), 1);
    assert!(!host.windows.contains_key(&window), "the HUD closed");
    assert!(host.windows.contains_key(&parent(&runner)));
}

#[test]
fn the_close_button_acts_for_assistive_technology() {
    let (mut runner, mut host) = setup(false);
    let window = hud(&runner);
    let button = chrome(&mut host, window, |c| c.children_ids().next().unwrap());
    a11y_click(host.windows.get_mut(&window).unwrap(), button);
    deliver(&mut runner, &mut host);
    assert!(!host.windows.contains_key(&window), "the HUD closed");
}

#[test]
fn the_cross_shows_while_the_pointer_is_over_the_button() {
    let (_, mut host) = setup(false);
    let window = *host.windows.keys().nth(1).unwrap();
    let c = close_rect().center();
    let (x, y) = (c.x as u32, c.y as u32);
    let rest = pixel(&mut host, window, x, y);
    host.windows.get_mut(&window).unwrap().mouse_move(c);
    let over = pixel(&mut host, window, x, y);
    assert_ne!(rest, over, "the cross appears on the disc");
    host.windows
        .get_mut(&window)
        .unwrap()
        .mouse_move(Point::new(150.0, 60.0));
    assert_eq!(pixel(&mut host, window, x, y), rest);
}

#[test]
fn dragging_the_bar_moves_the_window_and_double_clicking_does_not_zoom() {
    let (mut runner, mut host) = setup(false);
    let window = hud(&runner);
    click(&mut runner, &mut host, window, Point::new(120.0, 9.0), 1);
    assert_eq!(
        chrome(&mut host, window, |c| c.requests.clone()),
        [WindowRequest::Move]
    );
    click(&mut runner, &mut host, window, Point::new(120.0, 9.0), 2);
    let requests = chrome(&mut host, window, |c| c.requests.clone());
    assert!(
        !requests.contains(&WindowRequest::Zoom),
        "a panel doesn't zoom: {requests:?}"
    );
}

#[test]
fn the_parent_keeps_its_focused_look_while_its_hud_window_has_the_keyboard() {
    let (mut runner, mut host) = setup(true);
    let (parent, hud) = (parent(&runner), hud(&runner));
    focus(&mut host, parent, true);
    let active = pixel(&mut host, parent, 200, 3);
    // The keyboard goes from the parent to the HUD window.
    focus(&mut host, parent, false);
    let inactive = pixel(&mut host, parent, 200, 3);
    assert_ne!(active, inactive, "the bar does take an inactive look");
    focus(&mut host, hud, true);
    deliver(&mut runner, &mut host);
    assert_eq!(pixel(&mut host, parent, 200, 3), active);

    // The keyboard leaves for another app: both are inactive.
    focus(&mut host, hud, false);
    deliver(&mut runner, &mut host);
    assert_eq!(pixel(&mut host, parent, 200, 3), inactive);
}

#[test]
fn a_hud_window_without_a_parent_changes_no_look() {
    let (mut runner, mut host) = setup(false);
    let (parent, hud) = (parent(&runner), hud(&runner));
    focus(&mut host, parent, false);
    let inactive = pixel(&mut host, parent, 200, 3);
    focus(&mut host, hud, true);
    deliver(&mut runner, &mut host);
    assert_eq!(pixel(&mut host, parent, 200, 3), inactive);
}

#[test]
fn closing_the_hud_window_gives_the_parent_its_own_look_back() {
    let (mut runner, mut host) = setup(true);
    let (parent, hud) = (parent(&runner), hud(&runner));
    focus(&mut host, parent, false);
    let inactive = pixel(&mut host, parent, 200, 3);
    focus(&mut host, hud, true);
    deliver(&mut runner, &mut host);
    assert_ne!(pixel(&mut host, parent, 200, 3), inactive);

    click(&mut runner, &mut host, hud, close_rect().center(), 1);
    assert!(!host.windows.contains_key(&hud));
    assert_eq!(pixel(&mut host, parent, 200, 3), inactive);
}

#[test]
fn closing_the_parent_closes_its_hud_windows() {
    let (mut runner, mut host) = setup(true);
    let (parent, hud) = (parent(&runner), hud(&runner));
    runner.close_requested(&mut host, parent);
    assert!(!host.windows.contains_key(&parent));
    assert!(!host.windows.contains_key(&hud), "the HUD went with it");
}

#[test]
fn a_hud_window_without_a_parent_stays_when_the_other_window_closes() {
    let (mut runner, mut host) = setup(false);
    let (parent, hud) = (parent(&runner), hud(&runner));
    runner.close_requested(&mut host, parent);
    assert!(host.windows.contains_key(&hud));
}

#[test]
fn a_hud_window_asks_for_blur_with_rounded_corners_when_the_compositor_offers_it() {
    let (runner, host) = setup_with(false, true);
    let blur = host.blurs[&hud(&runner)];
    assert_eq!(blur.corner_radius, neue_theme::materials::hud::RADIUS);
    // Only the HUD's material is translucent.
    assert_eq!(host.blurs.len(), 1);
}

#[test]
fn nothing_asks_for_blur_without_the_compositor_offering_it() {
    let (_, host) = setup_with(false, false);
    assert!(host.blurs.is_empty());
}

#[test]
fn the_hud_panel_is_translucent_over_blur_and_opaque_without() {
    let (runner, mut host) = setup_with(false, true);
    let blurred = pixel(&mut host, hud(&runner), 100, 3);
    let (runner, mut host) = setup_with(false, false);
    let plain = pixel(&mut host, hud(&runner), 100, 3);
    // In the title bar; the harness paints over a backdrop of its own, so the tints differ.
    assert_ne!(blurred, plain, "the HUD draws the translucent tint");
}
