//! The 2.16 text view end to end on the fake host: typing and line breaks, wrapping,
//! keeping the caret in view, the clipboard, read-only views, changes from the app and
//! what assistive technology sees.

use masonry_core::accesskit::Role;
use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use masonry_core::core::{Ime, TextEvent, Widget, WidgetRef};
use masonry_core::kurbo::Vec2;

use super::*;
use crate::TextView;
use crate::widgets::{ScrollWidget, TextViewWidget};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Notes(String),
    Default,
    /// Script: the app sets the notes and reads them back.
    Set,
    Read,
}

#[derive(Default)]
struct Notes {
    notes: Handle<TextView<Msg>>,
    licence: Handle<TextView<Msg>>,
    read: Option<String>,
    log: Vec<Msg>,
}

impl Application for Notes {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .with(
                TextView::new()
                    .width(150.0)
                    .height(60.0)
                    .on_change(Msg::Notes)
                    .bind(&mut self.notes),
            )
            .with(
                TextView::new()
                    .editable(false)
                    .text("Read only")
                    .width(150.0)
                    .height(60.0)
                    .on_change(Msg::Notes)
                    .bind(&mut self.licence),
            )
            .with(Button::new("OK").on_press(Msg::Default).as_default());
        cx.open_window(Window::new("Notes", (200, 220), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Set => self.notes.set_text(cx, "one\ntwo"),
            Msg::Read => self.read = self.notes.text(cx),
            message => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Notes>, FakeHost) {
    let (runner, mut host, _) = start(Notes::default());
    let _ = host.only().render();
    (runner, host)
}

fn press(host: &mut FakeHost, key: Key, modifiers: Modifiers) {
    for state in [KeyState::Down, KeyState::Up] {
        host.only()
            .process_text_event(TextEvent::Keyboard(KeyboardEvent {
                state,
                key: key.clone(),
                modifiers,
                ..KeyboardEvent::default()
            }));
    }
}

fn type_text(runner: &mut Runner<Notes>, host: &mut FakeHost, text: &str) {
    for c in text.chars() {
        press(host, Key::Character(c.to_string()), Modifiers::empty());
    }
    deliver(runner, host);
}

fn tap(runner: &mut Runner<Notes>, host: &mut FakeHost, key: NamedKey) {
    press(host, Key::Named(key), Modifiers::empty());
    deliver(runner, host);
}

/// The platform's command key: Cmd on macOS, Ctrl elsewhere.
fn command() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    }
}

fn focus(host: &mut FakeHost, view: &Handle<TextView<Msg>>) {
    host.only().focus_on(Some(id(view)));
}

/// The scroll view around a text view: the first `ScrollWidget` above it in the tree.
fn scroller_of(host: &mut FakeHost, view: &Handle<TextView<Msg>>) -> Vec2 {
    fn find(
        widget: WidgetRef<'_, dyn Widget>,
        target: masonry_core::core::WidgetId,
    ) -> Option<Vec2> {
        if let Some(scroll) = widget.downcast::<ScrollWidget>()
            && widget.children().iter().any(|c| c.id() == target)
        {
            return Some(scroll.inner().offset());
        }
        widget.children().into_iter().find_map(|c| find(c, target))
    }
    let target = id(view);
    find(host.only().root_widget().as_dyn(), target).expect("the text view's scroll view")
}

fn text_of(host: &mut FakeHost, view: &Handle<TextView<Msg>>) -> String {
    host.only()
        .get_widget_with_id(id(view))
        .downcast::<TextViewWidget>()
        .expect("a text view")
        .inner()
        .text()
}

#[test]
fn return_adds_a_line_and_leaves_the_default_button_alone() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().notes);
    type_text(&mut runner, &mut host, "a");
    tap(&mut runner, &mut host, NamedKey::Enter);
    type_text(&mut runner, &mut host, "b");
    assert_eq!(
        runner.app().log,
        [
            Msg::Notes("a".into()),
            Msg::Notes("a\n".into()),
            Msg::Notes("a\nb".into()),
        ],
        "Return in a text view inserts a line break"
    );
}

#[test]
fn up_and_down_move_between_lines() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Set);
    focus(&mut host, &runner.app().notes);
    let _ = host.only().render();
    // The caret starts at the beginning; Down goes to line two, where typing lands.
    tap(&mut runner, &mut host, NamedKey::ArrowDown);
    type_text(&mut runner, &mut host, "!");
    assert_eq!(
        runner.app().log.last(),
        Some(&Msg::Notes("one\n!two".into()))
    );
    tap(&mut runner, &mut host, NamedKey::ArrowUp);
    type_text(&mut runner, &mut host, "?");
    assert_eq!(
        runner.app().log.last(),
        Some(&Msg::Notes("?one\n!two".into()))
    );
}

#[test]
fn the_view_scrolls_to_keep_the_caret_in_sight() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().notes);
    assert_eq!(scroller_of(&mut host, &runner.app().notes), Vec2::ZERO);
    // Ten lines don't fit in 60 px.
    for _ in 0..10 {
        type_text(&mut runner, &mut host, "x");
        tap(&mut runner, &mut host, NamedKey::Enter);
    }
    let _ = host.only().render();
    let down = scroller_of(&mut host, &runner.app().notes);
    assert!(down.y > 80.0, "the view followed the caret: {down:?}");
    // Back to the top with Cmd-Up (macOS) or Ctrl-Home.
    let (key, mods) = if cfg!(target_os = "macos") {
        (NamedKey::ArrowUp, Modifiers::META)
    } else {
        (NamedKey::Home, Modifiers::CONTROL)
    };
    press(&mut host, Key::Named(key), mods);
    deliver(&mut runner, &mut host);
    let _ = host.only().render();
    assert_eq!(scroller_of(&mut host, &runner.app().notes), Vec2::ZERO);
}

#[test]
fn long_lines_wrap_to_the_width() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().notes);
    type_text(
        &mut runner,
        &mut host,
        "wrapping words wrapping words wrapping words wrapping words wrapping words",
    );
    let _ = host.only().render();
    let scroll = scroller_of(&mut host, &runner.app().notes);
    assert_eq!(scroll.x, 0.0, "no sideways scrolling");
    assert!(
        scroll.y > 0.0,
        "wrapped into more lines than fit: {scroll:?}"
    );
}

#[test]
fn copy_cut_and_paste_keep_line_breaks() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().notes);
    host.only()
        .process_text_event(TextEvent::ClipboardPaste("one\r\ntwo".into()));
    deliver(&mut runner, &mut host);
    assert_eq!(
        runner.app().log.last(),
        Some(&Msg::Notes("one\ntwo".into()))
    );
    press(&mut host, Key::Character("a".into()), command());
    press(&mut host, Key::Character("x".into()), command());
    deliver(&mut runner, &mut host);
    assert_eq!(host.only().clipboard_contents(), "one\ntwo");
    assert_eq!(runner.app().log.last(), Some(&Msg::Notes(String::new())));
}

#[test]
fn composition_is_not_a_change_until_committed() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().notes);
    type_text(&mut runner, &mut host, "a");
    let ime = |host: &mut FakeHost, ime: Ime| host.only().process_text_event(TextEvent::Ime(ime));
    assert!(host.only().has_ime_session(), "a text view asks for an IME");
    ime(&mut host, Ime::Enabled);
    ime(&mut host, Ime::Preedit("ni".into(), Some((2, 2))));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Notes("a".into())]);
    ime(&mut host, Ime::Preedit(String::new(), None));
    ime(&mut host, Ime::Commit("你".into()));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log.last(), Some(&Msg::Notes("a你".into())));
}

#[test]
fn a_read_only_view_selects_and_copies_but_never_changes() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().licence);
    assert!(
        !host.only().has_ime_session(),
        "a read-only view takes no input method"
    );
    type_text(&mut runner, &mut host, "x");
    tap(&mut runner, &mut host, NamedKey::Backspace);
    host.only()
        .process_text_event(TextEvent::ClipboardPaste("pasted".into()));
    press(&mut host, Key::Character("a".into()), command());
    press(&mut host, Key::Character("x".into()), command());
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty(), "{:?}", runner.app().log);
    // Return has nothing to insert, so it goes on to the default button.
    tap(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Default]);
    assert_eq!(text_of(&mut host, &runner.app().licence), "Read only");
    press(&mut host, Key::Character("c".into()), command());
    assert_eq!(host.only().clipboard_contents(), "Read only");
}

#[test]
fn the_app_sets_and_reads_text_without_a_message() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Set);
    runner.send(&mut host, Msg::Read);
    assert_eq!(runner.app().read.as_deref(), Some("one\ntwo"));
    assert!(runner.app().log.is_empty(), "{:?}", runner.app().log);
}

#[test]
fn assistive_technology_sees_multiline_text_and_can_set_it() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Set);
    let notes = id(&runner.app().notes);
    let licence = id(&runner.app().licence);
    let _ = host.only().render();
    let node = host.only().access_node(notes).expect("a node");
    assert_eq!(node.role(), Role::MultilineTextInput);
    assert!(!node.is_read_only());
    assert!(
        host.only()
            .access_node(licence)
            .expect("a node")
            .is_read_only()
    );
    let tree = format!("{:?}", host.only().access_tree());
    assert!(tree.contains("one"), "{tree}");

    use masonry_core::accesskit::{Action, ActionData};
    host.only()
        .process_access_event(masonry_core::accesskit::ActionRequest {
            action: Action::SetValue,
            target_tree: masonry_core::accesskit::TreeId::ROOT,
            target_node: masonry_core::accesskit::NodeId(notes.to_raw()),
            data: Some(ActionData::Value("set\nby AT".into())),
        });
    deliver(&mut runner, &mut host);
    assert_eq!(
        runner.app().log.last(),
        Some(&Msg::Notes("set\nby AT".into()))
    );
}

#[test]
fn the_text_view_is_one_tab_stop_inside_its_scroll_view() {
    let (mut runner, mut host) = setup();
    // Tab goes straight to the text view, not to a scroll view around it.
    press(&mut host, Key::Named(NamedKey::Tab), Modifiers::empty());
    deliver(&mut runner, &mut host);
    assert_eq!(
        host.only().focused_widget_id(),
        Some(id(&runner.app().notes))
    );
}
