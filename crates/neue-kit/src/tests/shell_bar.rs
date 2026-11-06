//! The shell's menu bar (3.2b) on the fake host: titles on the left, extras on the right,
//! a title's menu opening below it, choosing, the keyboard, and changing a title; the
//! active app's menus being replaced, and their open and close messages (3.7a); moving
//! between menus by pointing and with Left and Right, and opening the first menu from the
//! keyboard (3.7b); icon extras that change and hide (3.11b); and the tray's status icons
//! that come and go, read their tooltip and give way to the titles (3.13b).

use masonry_core::accesskit::{Action, Role};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::kurbo::Rect;
use neue_theme::materials::menu_bar::{EDGE_INSET, HEIGHT};

use super::menus::{a11y, click_item, find, focused, menus_open, named};
use super::*;
use crate::desktop::{Anchor, ExtraIcon, LayerSpec, MenuBar, MenuTitle, NetworkIcon};
use crate::{Menu, MenuItem};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    About,
    Quit,
    /// Script: the app renames its title.
    Rename(&'static str),
    /// Script: the app's menus become these titles, each with one item named after it.
    SetMenus(Vec<&'static str>),
    /// Script: the shell was asked to focus the menu bar (Ctrl+F2).
    FocusBar,
    Opened(&'static str),
    Closed(&'static str),
    Item(&'static str),
    /// Script: the battery extra shows this icon, named so.
    SetBattery(ExtraIcon, &'static str),
    /// Script: the network extra shows this icon, named so.
    SetNetwork(ExtraIcon, &'static str),
    /// Script: the Bluetooth extra hides or shows.
    HideBluetooth(bool),
    /// Script: the Bluetooth extra's menu becomes one item, `Replaced`.
    ReplaceBluetoothMenu,
    /// Script: the tray's icons become these (name, tooltip, picture side in pixels).
    SetTray(Vec<TrayItem>),
    /// Script: the tray's icons become two that act on a press: `Mail` with a menu and
    /// `Bare` without.
    SetPressTray,
    /// A pressable tray icon was pressed.
    Pressed(&'static str),
    /// A pressable tray icon without a menu got a secondary press.
    Context(&'static str),
    /// Script: tray icon `n` hides or shows.
    HideTray(usize, bool),
    /// Script: tray icon `n` changes its name and its tooltip.
    RenameTray(usize, &'static str, Option<String>),
}

/// A status icon's name, tooltip and picture side.
type TrayItem = (&'static str, &'static str, u32);

/// A tray icon named `name` with `tooltip`, a grey square of `side` pixels, and a menu with
/// one item, `name item`.
fn tray_item(name: &'static str, tooltip: &'static str, side: u32) -> MenuTitle<Msg> {
    let item: &'static str = Box::leak(format!("{name} item").into_boxed_str());
    let rgba = vec![160; (side * side * 4) as usize];
    let icon = ExtraIcon::pixmap(side, side, &rgba).expect("a valid picture");
    MenuTitle::icon(
        icon,
        name,
        Menu::new().with(MenuItem::new(item).on_choose(Msg::Item(item))),
    )
    .description(tooltip)
}

/// An app menu titled `name`, with an item `name item`, reporting opening and closing.
fn app_menu(name: &'static str) -> MenuTitle<Msg> {
    let item: &'static str = Box::leak(format!("{name} item").into_boxed_str());
    MenuTitle::new(
        name,
        Menu::new().with(MenuItem::new(item).on_choose(Msg::Item(item))),
    )
    .on_open(Msg::Opened(name))
    .on_close(Msg::Closed(name))
}

const WIDTH: u32 = 500;

#[derive(Default)]
struct Bar {
    bar: Handle<MenuBar<Msg>>,
    logo: Handle<MenuTitle<Msg>>,
    app: Handle<MenuTitle<Msg>>,
    clock: Handle<MenuTitle<Msg>>,
    battery: Handle<MenuTitle<Msg>>,
    network: Handle<MenuTitle<Msg>>,
    bluetooth: Handle<MenuTitle<Msg>>,
    tray: Vec<Handle<MenuTitle<Msg>>>,
    log: Vec<Msg>,
}

const HALF: ExtraIcon = ExtraIcon::Battery {
    level: 0.5,
    charging: false,
};
const WIFI: ExtraIcon = ExtraIcon::Network(NetworkIcon::Wifi { level: 3 });
const BLUETOOTH: ExtraIcon = ExtraIcon::Bluetooth {
    powered: true,
    connected: false,
};

impl Application for Bar {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let bar = MenuBar::new()
            .with(
                MenuTitle::logo(
                    "System",
                    Menu::new().with(MenuItem::new("About").on_choose(Msg::About)),
                )
                .bind(&mut self.logo),
            )
            .with(
                MenuTitle::new(
                    "Files",
                    Menu::new().with(MenuItem::new("Quit Files").on_choose(Msg::Quit)),
                )
                .bold(true)
                .bind(&mut self.app),
            )
            .with_menu(app_menu("Edit"))
            .with_extra(MenuTitle::new("10:09", Menu::new()).bind(&mut self.clock))
            .with_extra(
                MenuTitle::icon(
                    HALF,
                    "Battery, 50 percent",
                    Menu::new().with(MenuItem::new("Power").on_choose(Msg::Item("Power"))),
                )
                .bind(&mut self.battery),
            )
            .with_extra(
                MenuTitle::icon(
                    WIFI,
                    "Network, Wi-Fi, Home",
                    Menu::new()
                        .with(
                            MenuItem::new("Home")
                                .checked(true)
                                .signal(3, true)
                                .on_choose(Msg::Item("Home")),
                        )
                        .with(
                            MenuItem::new("Cafe")
                                .signal(2, false)
                                .on_choose(Msg::Item("Cafe")),
                        ),
                )
                .bind(&mut self.network),
            )
            .with_extra(
                MenuTitle::icon(
                    BLUETOOTH,
                    "Bluetooth, on",
                    Menu::new().with(MenuItem::new("Switch").on_choose(Msg::Item("Switch"))),
                )
                .on_open(Msg::Opened("Bluetooth"))
                .on_close(Msg::Closed("Bluetooth"))
                .hidden(true)
                .bind(&mut self.bluetooth),
            );
        // Two status icons, of other sizes than the tray's: the first is rightmost.
        self.tray = vec![Handle::default(), Handle::default()];
        let [first, second] = &mut self.tray[..] else {
            unreachable!("two handles");
        };
        let bar = bar
            .with_tray(tray_item("Chat", "Chat: 2 unread", 24).bind(first))
            .with_tray(tray_item("Sync", "Sync: idle", 48).bind(second))
            .bind(&mut self.bar);
        // The bar is the surface's root, as in the shell; the window is taller so that the
        // menus fit in the fake host.
        let layer = LayerSpec::new("test-bar", Anchor::TOP_EDGE);
        cx.open_window(Window::new("Bar", (WIDTH, 300), bar).layer(layer));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Rename(name) => self.app.set_text(cx, name),
            Msg::FocusBar => self.bar.open_first_menu(cx),
            Msg::SetBattery(icon, name) => self.battery.set_icon(cx, icon, name),
            Msg::SetNetwork(icon, name) => self.network.set_icon(cx, icon, name),
            Msg::HideBluetooth(hidden) => self.bluetooth.set_hidden(cx, hidden),
            Msg::ReplaceBluetoothMenu => self.bluetooth.set_menu(
                cx,
                Menu::new().with(MenuItem::new("Replaced").on_choose(Msg::Item("Replaced"))),
            ),
            Msg::SetTray(items) => {
                self.tray = vec![Handle::default(); items.len()];
                let titles = items
                    .into_iter()
                    .zip(&mut self.tray)
                    .map(|((name, tooltip, side), handle)| {
                        tray_item(name, tooltip, side).bind(handle)
                    })
                    .collect();
                self.bar.set_tray(cx, titles);
            }
            Msg::SetPressTray => {
                self.tray = vec![Handle::default(); 2];
                let [mail, bare] = &mut self.tray[..] else {
                    unreachable!("two handles");
                };
                let titles = vec![
                    tray_item("Mail", "", 16)
                        .on_press(Msg::Pressed("Mail"))
                        .on_context(Msg::Context("Mail"))
                        .bind(mail),
                    MenuTitle::icon(
                        ExtraIcon::pixmap(16, 16, &[160; 16 * 16 * 4]).expect("a valid picture"),
                        "Bare",
                        Menu::new(),
                    )
                    .on_press(Msg::Pressed("Bare"))
                    .on_context(Msg::Context("Bare"))
                    .bind(bare),
                ];
                self.bar.set_tray(cx, titles);
            }
            Msg::HideTray(n, hidden) => self.tray[n].set_hidden(cx, hidden),
            Msg::RenameTray(n, name, tooltip) => {
                let icon = ExtraIcon::pixmap(1, 1, &[0, 0, 0, 255]).expect("a valid picture");
                self.tray[n].set_icon(cx, icon, name);
                self.tray[n].set_description(cx, tooltip);
            }
            Msg::SetMenus(names) => {
                self.bar
                    .set_menus(cx, names.into_iter().map(app_menu).collect());
            }
            message => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Bar>, FakeHost) {
    let (runner, mut host, _) = start(Bar::default());
    let _ = host.only().render();
    (runner, host)
}

fn bounds_of(host: &mut FakeHost, id: WidgetId) -> Rect {
    let widget = host.only().get_widget_with_id(id);
    widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box())
}

fn click_widget(runner: &mut Runner<Bar>, host: &mut FakeHost, id: WidgetId) {
    let at = bounds_of(host, id).center();
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

/// Whether the title labelled `label` says its menu is up.
fn expanded(host: &mut FakeHost, label: &str) -> Option<bool> {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == Role::MenuItem && node.label().as_deref() == Some(label) {
            return node.data().is_expanded();
        }
        stack.extend(node.children());
    }
    None
}

#[test]
fn titles_sit_left_and_extras_right_at_the_bar_height() {
    let (runner, mut host) = setup();
    let logo = bounds_of(&mut host, id(&runner.app().logo));
    let app = bounds_of(&mut host, id(&runner.app().app));
    let clock = bounds_of(&mut host, id(&runner.app().clock));
    assert!((logo.x0 - EDGE_INSET).abs() < 0.5, "{logo:?}");
    assert!((app.x0 - logo.x1).abs() < 0.5, "titles abut: {app:?}");
    let bar = find(&mut host, Role::MenuBar, None)[0].bounds;
    assert!((bar.width() - f64::from(WIDTH)).abs() < 0.5, "{bar:?}");
    assert!(
        (clock.x1 - (f64::from(WIDTH) - EDGE_INSET)).abs() < 0.5,
        "{clock:?}"
    );
    for rect in [logo, app, clock] {
        assert!((rect.height() - HEIGHT).abs() < 0.5);
        assert!(rect.y0.abs() < 0.5);
    }
    assert_eq!(find(&mut host, Role::MenuBar, None).len(), 1);
}

#[test]
fn a_title_opens_its_menu_below_itself_and_choosing_sends_the_message() {
    let (mut runner, mut host) = setup();
    let app = id(&runner.app().app);
    let title = bounds_of(&mut host, app);
    click_widget(&mut runner, &mut host, app);
    assert_eq!(menus_open(&mut host), 1);
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    assert!((menu.y0 - title.y1).abs() < 1.5, "right below the bar");
    assert!((menu.x0 - title.x0).abs() < 1.5, "level with the title");
    assert_eq!(expanded(&mut host, "Files"), Some(true));

    click_item(&mut runner, &mut host, "Quit Files");
    assert_eq!(runner.app().log, [Msg::Quit]);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(expanded(&mut host, "Files"), Some(false));
}

#[test]
fn the_keyboard_and_assistive_technologies_open_a_title() {
    let (mut runner, mut host) = setup();
    let logo = id(&runner.app().logo);
    host.only().focus_on(Some(logo));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::About]);

    let node = find(&mut host, Role::MenuItem, Some("System"))[0].node;
    a11y(&mut host, node, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
}

#[test]
fn a_title_with_no_menu_opens_nothing_and_text_can_change() {
    let (mut runner, mut host) = setup();
    let clock = id(&runner.app().clock);
    click_widget(&mut runner, &mut host, clock);
    assert_eq!(menus_open(&mut host), 0);

    let before = bounds_of(&mut host, id(&runner.app().app));
    runner.send(&mut host, Msg::Rename("A Much Longer Name"));
    let _ = host.only().render();
    let after = bounds_of(&mut host, id(&runner.app().app));
    assert!(after.width() > before.width());
    assert_eq!(
        find(&mut host, Role::MenuItem, Some("A Much Longer Name")).len(),
        1
    );
}

/// The bounds of the title labelled `label`, if it is in the bar.
fn title(host: &mut FakeHost, label: &str) -> Option<Rect> {
    find(host, Role::MenuItem, Some(label))
        .first()
        .map(|n| n.bounds)
}

#[test]
fn app_menus_follow_the_fixed_titles_and_report_opening_and_closing() {
    let (mut runner, mut host) = setup();
    let files = bounds_of(&mut host, id(&runner.app().app));
    let edit = title(&mut host, "Edit").unwrap();
    assert!(
        (edit.x0 - files.x1).abs() < 0.5,
        "after the app's name: {edit:?}"
    );

    let at = edit.center();
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(runner.app().log, [Msg::Opened("Edit")]);
    click_item(&mut runner, &mut host, "Edit item");
    assert_eq!(
        runner.app().log,
        [
            Msg::Opened("Edit"),
            Msg::Item("Edit item"),
            Msg::Closed("Edit")
        ]
    );
}

#[test]
fn app_menus_are_replaced_whole() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::SetMenus(vec!["Window", "Help"]));
    let _ = host.only().render();
    assert!(title(&mut host, "Edit").is_none());
    let files = bounds_of(&mut host, id(&runner.app().app));
    let window = title(&mut host, "Window").unwrap();
    let help = title(&mut host, "Help").unwrap();
    assert!((window.x0 - files.x1).abs() < 0.5, "{window:?}");
    assert!((help.x0 - window.x1).abs() < 0.5, "{help:?}");

    // The new titles' items act.
    let harness = host.only();
    harness.mouse_move(help.center());
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    click_item(&mut runner, &mut host, "Help item");
    assert!(runner.app().log.contains(&Msg::Item("Help item")));

    runner.send(&mut host, Msg::SetMenus(Vec::new()));
    let _ = host.only().render();
    assert!(title(&mut host, "Window").is_none());
    assert_eq!(
        find(&mut host, Role::MenuItem, None).len(),
        8,
        "logo, app, clock, battery, network, the hidden Bluetooth and the two tray icons"
    );
}

/// A bar that is translucent over a blur, as the shell's.
struct BlurBar;

impl Application for BlurBar {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let bar = MenuBar::new()
            .with(MenuTitle::new("Files", Menu::new()))
            .backdrop(cx.backdrop());
        let layer = LayerSpec::new("test-bar", Anchor::TOP_EDGE);
        cx.open_window(
            Window::new("Bar", (WIDTH, HEIGHT as u32), bar)
                .layer(layer)
                .blur_behind(),
        );
    }

    fn update(&mut self, _cx: &mut Cx<'_, Msg>, _message: Msg) {}
}

fn blur_bar(blur: bool) -> FakeHost {
    let mut runner = Runner::new(BlurBar, Settings::default());
    let mut host = FakeHost {
        blur,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    host
}

#[test]
fn the_bar_asks_for_a_plain_blur_and_turns_translucent_when_it_is_offered() {
    let mut host = blur_bar(true);
    let blur = host.blurs.values().next().copied();
    assert_eq!(blur.map(|b| b.corner_radius), Some(0.0));
    let translucent = host.only().render().get_pixel(250, 10).0;

    let mut host = blur_bar(false);
    assert!(host.blurs.is_empty());
    let opaque = host.only().render().get_pixel(250, 10).0;

    // The harness paints over a backdrop of its own, so the two tints differ in colour.
    assert_ne!(translucent, opaque, "the bar draws the translucent tint");
}

/// Moves the pointer to the centre of the title labelled `label`.
fn point_at(runner: &mut Runner<Bar>, host: &mut FakeHost, label: &str) {
    let at = title(host, label).unwrap().center();
    host.only().mouse_move(at);
    deliver(runner, host);
}

#[test]
fn pointing_at_another_title_while_a_menu_is_up_switches_menus() {
    let (mut runner, mut host) = setup();
    // Nothing is up: pointing opens nothing.
    point_at(&mut runner, &mut host, "Edit");
    assert_eq!(menus_open(&mut host), 0);

    let files = id(&runner.app().app);
    click_widget(&mut runner, &mut host, files);
    assert_eq!(expanded(&mut host, "Files"), Some(true));
    point_at(&mut runner, &mut host, "Edit");
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(expanded(&mut host, "Files"), Some(false));
    assert_eq!(expanded(&mut host, "Edit"), Some(true));
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    let edit = title(&mut host, "Edit").unwrap();
    assert!((menu.x0 - edit.x0).abs() < 1.5, "below Edit: {menu:?}");
    assert_eq!(runner.app().log, [Msg::Opened("Edit")]);

    // The extras on the right take no part.
    point_at(&mut runner, &mut host, "10:09");
    assert_eq!(expanded(&mut host, "Edit"), Some(true));

    // Back to the logo; the new menu works like one opened by a click.
    point_at(&mut runner, &mut host, "System");
    assert_eq!(expanded(&mut host, "System"), Some(true));
    assert_eq!(menus_open(&mut host), 1);
    click_item(&mut runner, &mut host, "About");
    assert_eq!(
        runner.app().log,
        [Msg::Opened("Edit"), Msg::Closed("Edit"), Msg::About]
    );
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn left_and_right_walk_the_menus_and_wrap() {
    let (mut runner, mut host) = setup();
    let logo = id(&runner.app().logo);
    host.only().focus_on(Some(logo));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));

    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("Quit Files"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("Edit item"));
    assert_eq!(menus_open(&mut host), 1);
    // Past the last app menu: back to the logo (the clock extra is skipped).
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut host).as_deref(), Some("Edit item"));
    assert_eq!(
        runner.app().log,
        [
            Msg::Opened("Edit"),
            Msg::Closed("Edit"),
            Msg::Opened("Edit")
        ]
    );

    // Escape closes the menu, and focus goes back to where it was before the first one.
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(focused(&mut host).as_deref(), Some("System"));

    // Between focused titles, Left and Right move focus; Escape leaves the bar.
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut host).as_deref(), Some("Edit"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("System"));
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_ne!(focused(&mut host).as_deref(), Some("System"));
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn the_shortcut_opens_the_first_menu_from_the_keyboard() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::FocusBar);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(expanded(&mut host, "System"), Some(true));
    assert_eq!(focused(&mut host).as_deref(), Some("About"));
    // Again while it is up: nothing changes.
    runner.send(&mut host, Msg::FocusBar);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));

    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("Quit Files"));
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
}

/// Extra width of a title around its content: the padding on both sides.
const PADDING: f64 = 2.0 * neue_theme::materials::menu_bar::TITLE_PADDING;

#[test]
fn icon_extras_are_sized_by_their_icon_and_sit_left_of_the_clock() {
    use neue_theme::materials::menu_bar::{BATTERY_WIDTH, BLUETOOTH_WIDTH};
    let (mut runner, mut host) = setup();
    let clock = bounds_of(&mut host, id(&runner.app().clock));
    let battery = bounds_of(&mut host, id(&runner.app().battery));
    assert!((battery.x1 - clock.x0).abs() < 0.5, "{battery:?} {clock:?}");
    assert!((battery.width() - (BATTERY_WIDTH + PADDING).ceil()).abs() < 0.5);
    assert!((battery.height() - HEIGHT).abs() < 0.5);
    // Read as a title: its name is the accessible label, and it has a menu.
    let found = find(&mut host, Role::MenuItem, Some("Battery, 50 percent"));
    assert_eq!(found.len(), 1);

    // Another kind of icon is another width.
    runner.send(&mut host, Msg::SetBattery(BLUETOOTH, "Bluetooth, on"));
    let _ = host.only().render();
    let now = bounds_of(&mut host, id(&runner.app().battery));
    assert!((now.width() - (BLUETOOTH_WIDTH + PADDING).ceil()).abs() < 0.5);
    assert!((now.x1 - clock.x0).abs() < 0.5);
}

#[test]
fn an_icon_extra_opens_its_menu_and_changes_its_name() {
    let (mut runner, mut host) = setup();
    let battery = id(&runner.app().battery);
    click_widget(&mut runner, &mut host, battery);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(expanded(&mut host, "Battery, 50 percent"), Some(true));
    click_item(&mut runner, &mut host, "Power");
    assert_eq!(runner.app().log, [Msg::Item("Power")]);

    let full = ExtraIcon::Battery {
        level: 1.0,
        charging: true,
    };
    runner.send(
        &mut host,
        Msg::SetBattery(full, "Battery, 100 percent, charging"),
    );
    let _ = host.only().render();
    assert!(title(&mut host, "Battery, 50 percent").is_none());
    assert!(title(&mut host, "Battery, 100 percent, charging").is_some());
}

#[test]
fn a_hidden_extra_takes_no_room_and_opens_nothing_until_shown() {
    use neue_theme::materials::menu_bar::BLUETOOTH_WIDTH;
    let (mut runner, mut host) = setup();
    let (bluetooth, network) = (id(&runner.app().bluetooth), id(&runner.app().network));
    let width = bounds_of(&mut host, bluetooth).width();
    assert!(width < 0.5, "hidden at the start: {width}");
    // Nothing takes it from the keyboard either.
    host.only().focus_on(Some(bluetooth));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(menus_open(&mut host), 0);

    runner.send(&mut host, Msg::HideBluetooth(false));
    let _ = host.only().render();
    let shown = bounds_of(&mut host, bluetooth);
    assert!((shown.width() - (BLUETOOTH_WIDTH + PADDING).ceil()).abs() < 0.5);
    assert!((shown.x1 - bounds_of(&mut host, network).x0).abs() < 0.5);
    click_widget(&mut runner, &mut host, bluetooth);
    assert_eq!(menus_open(&mut host), 1);

    // Hiding it while its menu is up takes the menu down too.
    runner.send(&mut host, Msg::HideBluetooth(true));
    deliver(&mut runner, &mut host);
    let _ = host.only().render();
    assert_eq!(menus_open(&mut host), 0);
    assert!(bounds_of(&mut host, bluetooth).width() < 0.5);
}

#[test]
fn replacing_a_menu_keeps_the_open_and_close_messages() {
    let (mut runner, mut host) = setup();
    let bluetooth = id(&runner.app().bluetooth);
    runner.send(&mut host, Msg::HideBluetooth(false));
    let _ = host.only().render();
    for _ in 0..2 {
        runner.send(&mut host, Msg::ReplaceBluetoothMenu);
    }
    click_widget(&mut runner, &mut host, bluetooth);
    assert_eq!(runner.app().log, [Msg::Opened("Bluetooth")]);
    click_item(&mut runner, &mut host, "Replaced");
    assert_eq!(
        runner.app().log,
        [
            Msg::Opened("Bluetooth"),
            Msg::Item("Replaced"),
            Msg::Closed("Bluetooth")
        ],
        "the new item's message, and the close one the title was built with"
    );
}

#[test]
fn the_network_extra_keeps_its_size_in_every_state_and_sits_left_of_the_battery() {
    use neue_theme::materials::menu_bar::NETWORK_WIDTH;
    let (mut runner, mut host) = setup();
    let battery = bounds_of(&mut host, id(&runner.app().battery));
    let network = bounds_of(&mut host, id(&runner.app().network));
    assert!(
        (network.x1 - battery.x0).abs() < 0.5,
        "{network:?} {battery:?}"
    );
    assert!((network.width() - (NETWORK_WIDTH + PADDING).ceil()).abs() < 0.5);

    // Switching state moves nothing: every network icon is one size.
    for (icon, name) in [
        (ExtraIcon::Network(NetworkIcon::Wired), "Network, wired"),
        (
            ExtraIcon::Network(NetworkIcon::WifiOff),
            "Network, Wi-Fi off",
        ),
        (
            ExtraIcon::Network(NetworkIcon::Disconnected),
            "Network, disconnected",
        ),
        (
            ExtraIcon::Network(NetworkIcon::Wifi { level: 0 }),
            "Network, Wi-Fi on",
        ),
    ] {
        runner.send(&mut host, Msg::SetNetwork(icon, name));
        let _ = host.only().render();
        let now = bounds_of(&mut host, id(&runner.app().network));
        assert_eq!(now, network, "{name}");
        assert_eq!(find(&mut host, Role::MenuItem, Some(name)).len(), 1);
    }
}

#[test]
fn network_items_show_and_read_their_signal() {
    let (mut runner, mut host) = setup();
    let network = id(&runner.app().network);
    click_widget(&mut runner, &mut host, network);
    assert_eq!(menus_open(&mut host), 1);
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let described = |label: &str| {
        let mut stack = vec![state.root()];
        while let Some(node) = stack.pop() {
            if node.label().as_deref() == Some(label) {
                return node.data().description().map(str::to_owned);
            }
            stack.extend(node.children());
        }
        panic!("no node {label:?}");
    };
    assert_eq!(
        described("Home").as_deref(),
        Some("strong signal, password needed")
    );
    assert_eq!(described("Cafe").as_deref(), Some("good signal"));
    click_item(&mut runner, &mut host, "Cafe");
    assert_eq!(runner.app().log, [Msg::Item("Cafe")]);
}

/// The description of the node labelled `label`.
fn description(host: &mut FakeHost, label: &str) -> Option<String> {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == Role::MenuItem && node.label().as_deref() == Some(label) {
            return node.data().description().map(str::to_owned);
        }
        stack.extend(node.children());
    }
    panic!("no title {label:?}");
}

#[test]
fn tray_icons_sit_left_of_the_extras_one_size_whatever_their_picture() {
    use neue_theme::materials::menu_bar::TRAY_ICON_SIZE;
    let (runner, mut host) = setup();
    let network = bounds_of(&mut host, id(&runner.app().network));
    let chat = bounds_of(&mut host, id(&runner.app().tray[0]));
    let sync = bounds_of(&mut host, id(&runner.app().tray[1]));
    // The first icon is rightmost, left of the Bluetooth extra (hidden here) and the
    // network one; they abut.
    assert!((chat.x1 - network.x0).abs() < 0.5, "{chat:?} {network:?}");
    assert!((sync.x1 - chat.x0).abs() < 0.5, "{sync:?} {chat:?}");
    // A 24 px and a 48 px picture are fitted to the tray's size.
    for icon in [chat, sync] {
        assert!((icon.width() - (TRAY_ICON_SIZE + PADDING).ceil()).abs() < 0.5);
        assert!((icon.height() - HEIGHT).abs() < 0.5);
    }
}

#[test]
fn tray_icons_come_and_go_without_the_bar_being_rebuilt() {
    let (mut runner, mut host) = setup();
    let bar = id(&runner.app().bar);
    let app = bounds_of(&mut host, id(&runner.app().app));
    runner.send(
        &mut host,
        Msg::SetTray(vec![("Mail", "", 16), ("Chat", "", 16), ("VPN", "", 16)]),
    );
    let _ = host.only().render();
    assert_eq!(id(&runner.app().bar), bar);
    assert_eq!(find(&mut host, Role::MenuBar, None).len(), 1);
    assert!(title(&mut host, "Sync").is_none(), "the old icons are gone");
    let (mail, chat, vpn) = (
        title(&mut host, "Mail").unwrap(),
        title(&mut host, "Chat").unwrap(),
        title(&mut host, "VPN").unwrap(),
    );
    assert!(
        mail.x0 > chat.x0 && chat.x0 > vpn.x0,
        "the first is rightmost"
    );
    // The rest of the bar stayed where it was.
    assert_eq!(bounds_of(&mut host, id(&runner.app().app)), app);

    // A new icon's menu acts.
    let vpn_id = id(&runner.app().tray[2]);
    click_widget(&mut runner, &mut host, vpn_id);
    assert_eq!(menus_open(&mut host), 1);
    click_item(&mut runner, &mut host, "VPN item");
    assert_eq!(runner.app().log, [Msg::Item("VPN item")]);

    runner.send(&mut host, Msg::SetTray(Vec::new()));
    let _ = host.only().render();
    assert!(title(&mut host, "Mail").is_none() && title(&mut host, "VPN").is_none());
}

#[test]
fn the_tooltip_is_the_description_and_icons_change_in_place() {
    let (mut runner, mut host) = setup();
    assert_eq!(
        description(&mut host, "Chat").as_deref(),
        Some("Chat: 2 unread")
    );
    // The name and the tooltip change; the icon stays where it is.
    let before = bounds_of(&mut host, id(&runner.app().tray[0]));
    runner.send(
        &mut host,
        Msg::RenameTray(0, "Chat, away", Some("Chat: away".to_owned())),
    );
    let _ = host.only().render();
    assert!(title(&mut host, "Chat").is_none());
    assert_eq!(
        description(&mut host, "Chat, away").as_deref(),
        Some("Chat: away")
    );
    assert_eq!(bounds_of(&mut host, id(&runner.app().tray[0])), before);
    runner.send(&mut host, Msg::RenameTray(0, "Chat, away", None));
    let _ = host.only().render();
    assert_eq!(description(&mut host, "Chat, away"), None);
}

/// A press of `button` on widget `id`.
fn press_widget(
    runner: &mut Runner<Bar>,
    host: &mut FakeHost,
    id: WidgetId,
    button: PointerButton,
) {
    let at = bounds_of(host, id).center();
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(button));
    harness.mouse_button_release(Some(button));
    deliver(runner, host);
}

#[test]
fn a_pressable_tray_icon_sends_its_message_and_opens_its_menu_on_a_secondary_press() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::SetPressTray);
    let _ = host.only().render();
    let (mail, bare) = (id(&runner.app().tray[0]), id(&runner.app().tray[1]));

    press_widget(&mut runner, &mut host, mail, PointerButton::Primary);
    assert_eq!(runner.app().log, [Msg::Pressed("Mail")]);
    assert_eq!(menus_open(&mut host), 0, "a primary press opens no menu");

    press_widget(&mut runner, &mut host, mail, PointerButton::Secondary);
    assert_eq!(menus_open(&mut host), 1);
    click_item(&mut runner, &mut host, "Mail item");
    assert_eq!(
        runner.app().log,
        [Msg::Pressed("Mail"), Msg::Item("Mail item")],
        "the menu's items still send their messages, and a secondary press sends no Context"
    );

    // Without a menu a secondary press is the context message, a primary one the press.
    press_widget(&mut runner, &mut host, bare, PointerButton::Secondary);
    press_widget(&mut runner, &mut host, bare, PointerButton::Primary);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(
        runner.app().log[2..],
        [Msg::Context("Bare"), Msg::Pressed("Bare")]
    );
}

#[test]
fn a_pressable_tray_icon_presses_on_return_and_opens_its_menu_on_down() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::SetPressTray);
    let _ = host.only().render();
    let mail = id(&runner.app().tray[0]);
    host.only().focus_on(Some(mail));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Pressed("Mail")]);
    assert_eq!(menus_open(&mut host), 0);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(menus_open(&mut host), 1);

    // Assistive technologies click it.
    host.only().focus_on(None);
    let node = find(&mut host, Role::MenuItem, Some("Mail"))[0].node;
    a11y(&mut host, node, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(
        runner.app().log,
        [Msg::Pressed("Mail"), Msg::Pressed("Mail")]
    );
}

#[test]
fn a_title_that_does_not_press_ignores_a_secondary_press() {
    let (mut runner, mut host) = setup();
    let app = id(&runner.app().app);
    press_widget(&mut runner, &mut host, app, PointerButton::Secondary);
    assert_eq!(menus_open(&mut host), 0);
    assert!(runner.app().log.is_empty());
}

#[test]
fn a_hidden_tray_icon_takes_no_room_until_shown() {
    let (mut runner, mut host) = setup();
    let chat = bounds_of(&mut host, id(&runner.app().tray[0]));
    let sync = bounds_of(&mut host, id(&runner.app().tray[1]));
    runner.send(&mut host, Msg::HideTray(0, true));
    let _ = host.only().render();
    assert!(bounds_of(&mut host, id(&runner.app().tray[0])).width() < 0.5);
    // The next one moves right to take its place.
    let moved = bounds_of(&mut host, id(&runner.app().tray[1]));
    assert!((moved.x1 - chat.x1).abs() < 0.5, "{moved:?} {chat:?}");
    runner.send(&mut host, Msg::HideTray(0, false));
    let _ = host.only().render();
    assert_eq!(bounds_of(&mut host, id(&runner.app().tray[1])), sync);
}

const MANY: [&str; 16] = [
    "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10", "t11", "t12", "t13", "t14",
    "t15",
];

#[test]
fn tray_icons_that_do_not_fit_before_the_titles_are_left_out_last_first() {
    use neue_theme::materials::menu_bar::TRAY_OVERFLOW_GAP;
    let (mut runner, mut host) = setup();
    let files = bounds_of(&mut host, id(&runner.app().app));
    runner.send(
        &mut host,
        Msg::SetTray(MANY.iter().map(|n| (*n, "", 16)).collect()),
    );
    let _ = host.only().render();
    let shown: Vec<_> = MANY
        .iter()
        .filter_map(|n| title(&mut host, n).map(|r| (*n, r)))
        .collect();
    assert!(
        !shown.is_empty() && shown.len() < MANY.len(),
        "{} of {} shown",
        shown.len(),
        MANY.len()
    );
    // The first ones are kept, and the last one shown keeps clear of the titles.
    for (i, (name, _)) in shown.iter().enumerate() {
        assert_eq!(*name, MANY[i]);
    }
    let edge = shown.last().unwrap().1.x0;
    let titles_end = title(&mut host, "Edit").unwrap().x1;
    assert!(
        edge >= titles_end + TRAY_OVERFLOW_GAP - 0.5,
        "{edge} {titles_end}"
    );
    assert!(files.x0 < edge);
    // A stashed icon isn't in the accessibility tree, so nothing can open it.
    let nodes = find(&mut host, Role::MenuItem, None).len();
    // Logo, app, Edit, clock, battery, network and the hidden Bluetooth, and the icons shown.
    assert_eq!(nodes, 7 + shown.len());

    // Fewer icons: all of them show again.
    runner.send(
        &mut host,
        Msg::SetTray(MANY[..3].iter().map(|n| (*n, "", 16)).collect()),
    );
    let _ = host.only().render();
    assert!(MANY[..3].iter().all(|n| title(&mut host, n).is_some()));
}

#[test]
fn a_pixmap_icon_needs_exactly_its_pixels() {
    assert!(matches!(
        ExtraIcon::pixmap(2, 2, &[255; 16]),
        Some(ExtraIcon::Image(_))
    ));
    assert!(ExtraIcon::pixmap(2, 2, &[255; 15]).is_none());
    assert!(ExtraIcon::pixmap(0, 2, &[]).is_none());
}
