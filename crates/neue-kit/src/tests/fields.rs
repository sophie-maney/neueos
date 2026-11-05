//! The 2.14 text, search and secure fields end to end on the fake host: typing, Return and
//! the default button, input methods, the clipboard, the search field's Escape and cancel
//! button, what a secure field hides, and changes from the app.

use masonry_core::accesskit::Role;
use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use masonry_core::core::{Ime, TextEvent};
use masonry_core::kurbo::Point;

use super::*;
use crate::TextField;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Name(String),
    NameSubmitted(String),
    Note(String),
    Search(String),
    Password(String),
    Default,
    /// Script: the app sets the name and disables the note.
    Set,
    Read,
}

#[derive(Default)]
struct Form {
    name: Handle<TextField<Msg>>,
    note: Handle<TextField<Msg>>,
    search: Handle<TextField<Msg>>,
    password: Handle<TextField<Msg>>,
    read: Option<String>,
    log: Vec<Msg>,
}

impl Application for Form {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .with(
                TextField::new()
                    .on_change(Msg::Name)
                    .on_submit(Msg::NameSubmitted)
                    .bind(&mut self.name),
            )
            // No submit handler: Return goes on to the default button.
            .with(TextField::new().on_change(Msg::Note).bind(&mut self.note))
            .with(
                TextField::search()
                    .on_change(Msg::Search)
                    .bind(&mut self.search),
            )
            .with(
                TextField::secure()
                    .on_change(Msg::Password)
                    .bind(&mut self.password),
            )
            .with(Button::new("OK").on_press(Msg::Default).as_default());
        cx.open_window(Window::new("Form", (300, 200), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Set => {
                self.name.set_text(cx, "Set by the app");
                self.note.set_enabled(cx, false);
            }
            Msg::Read => self.read = self.password.text(cx),
            message => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Form>, FakeHost) {
    let (runner, mut host, _) = start(Form::default());
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

fn type_text(runner: &mut Runner<Form>, host: &mut FakeHost, text: &str) {
    for c in text.chars() {
        press(host, Key::Character(c.to_string()), Modifiers::empty());
    }
    deliver(runner, host);
}

fn tap_named(runner: &mut Runner<Form>, host: &mut FakeHost, key: NamedKey) {
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

fn focus(host: &mut FakeHost, field: &Handle<TextField<Msg>>) {
    host.only().focus_on(Some(id(field)));
}

#[test]
fn typing_sends_the_whole_text_and_return_submits() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "Ab");
    tap_named(&mut runner, &mut host, NamedKey::Backspace);
    tap_named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        runner.app().log,
        [
            Msg::Name("A".into()),
            Msg::Name("Ab".into()),
            Msg::Name("A".into()),
            Msg::NameSubmitted("A".into()),
        ],
        "Return in a field with a submit handler doesn't reach the default button"
    );
}

#[test]
fn return_in_a_field_without_submit_presses_the_default_button() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().note);
    type_text(&mut runner, &mut host, "x");
    tap_named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Note("x".into()), Msg::Default]);
}

#[test]
fn tab_selects_the_text_and_typing_replaces_it() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Set);
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "B");
    assert_eq!(runner.app().log, [Msg::Name("B".into())]);
}

#[test]
fn composition_is_not_a_change_until_committed() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "a");
    let ime = |host: &mut FakeHost, ime: Ime| host.only().process_text_event(TextEvent::Ime(ime));
    ime(&mut host, Ime::Enabled);
    ime(&mut host, Ime::Preedit("ni".into(), Some((2, 2))));
    ime(&mut host, Ime::Preedit("nih".into(), Some((3, 3))));
    // Keys while composing belong to the input method.
    press(
        &mut host,
        Key::Named(NamedKey::Backspace),
        Modifiers::empty(),
    );
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Name("a".into())]);
    assert!(
        host.only().has_ime_session(),
        "a text field asks for an IME"
    );
    ime(&mut host, Ime::Preedit(String::new(), None));
    ime(&mut host, Ime::Commit("你".into()));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log.last(), Some(&Msg::Name("a你".into())));
}

#[test]
fn copy_cut_and_paste() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "hello");
    press(&mut host, Key::Character("a".into()), command());
    press(&mut host, Key::Character("x".into()), command());
    deliver(&mut runner, &mut host);
    assert_eq!(host.only().clipboard_contents(), "hello");
    assert_eq!(runner.app().log.last(), Some(&Msg::Name(String::new())));
    // Pasted line breaks become spaces.
    host.only()
        .process_text_event(TextEvent::ClipboardPaste("one\ntwo".into()));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log.last(), Some(&Msg::Name("one two".into())));
}

/// The window point `fx` of the way across `field`'s width, at its middle height.
fn field_point(host: &mut FakeHost, field: &Handle<TextField<Msg>>, fx: f64) -> Point {
    let widget = host.only().get_widget_with_id(id(field));
    let ctx = widget.ctx();
    let size = ctx.border_box().size();
    ctx.to_window(Point::new(size.width * fx, size.height / 2.0))
}

#[test]
fn a_pointer_selection_becomes_the_primary_selection_when_it_ends() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "hello world");
    let (from, to) = (
        field_point(&mut host, &runner.app().name, 0.0),
        field_point(&mut host, &runner.app().name, 1.0),
    );
    host.only().mouse_move(from);
    host.only().mouse_button_press(None);
    host.only().mouse_move(to);
    deliver(&mut runner, &mut host);
    assert_eq!(host.primary_selection, None, "still dragging");
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert_eq!(host.primary_selection.as_deref(), Some("hello world"));
}

#[test]
fn a_click_without_a_selection_leaves_the_primary_selection_alone() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "hello");
    let at = field_point(&mut host, &runner.app().name, 0.5);
    host.only().mouse_move(at);
    host.only().mouse_button_press(None);
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert_eq!(host.primary_selection, None);
}

#[test]
fn shift_arrows_select_into_the_primary_selection() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().name);
    type_text(&mut runner, &mut host, "abc");
    press(&mut host, Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
    press(&mut host, Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
    deliver(&mut runner, &mut host);
    assert_eq!(host.primary_selection.as_deref(), Some("bc"));
}

#[test]
fn a_secure_field_never_sets_the_primary_selection() {
    let (mut runner, mut host) = setup();
    focus(&mut host, &runner.app().password);
    type_text(&mut runner, &mut host, "s3cret");
    press(&mut host, Key::Named(NamedKey::ArrowLeft), Modifiers::SHIFT);
    let (from, to) = (
        field_point(&mut host, &runner.app().password, 0.0),
        field_point(&mut host, &runner.app().password, 1.0),
    );
    host.only().mouse_move(from);
    host.only().mouse_button_press(None);
    host.only().mouse_move(to);
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert_eq!(host.primary_selection, None);
}

#[test]
fn escape_and_the_cancel_button_clear_a_search_field() {
    let (mut runner, mut host) = setup();
    let search = id(&runner.app().search);
    focus(&mut host, &runner.app().search);
    type_text(&mut runner, &mut host, "q");
    tap_named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(
        runner.app().log,
        [Msg::Search("q".into()), Msg::Search(String::new())]
    );

    type_text(&mut runner, &mut host, "r");
    // The cancel button sits in the round right end.
    let at = {
        let widget = host.only().get_widget_with_id(search);
        let ctx = widget.ctx();
        let size = ctx.border_box().size();
        ctx.to_window(Point::new(
            size.width - size.height / 2.0,
            size.height / 2.0,
        ))
    };
    host.only().mouse_move(at);
    host.only().mouse_button_press(None);
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log.last(), Some(&Msg::Search(String::new())));
}

#[test]
fn a_secure_field_hides_its_text_from_everyone_but_the_app() {
    let (mut runner, mut host) = setup();
    let password = id(&runner.app().password);
    focus(&mut host, &runner.app().password);
    assert!(
        !host.only().has_ime_session(),
        "secure input bypasses the IME"
    );
    type_text(&mut runner, &mut host, "s3cret");
    tap_named(&mut runner, &mut host, NamedKey::Backspace);
    tap_named(&mut runner, &mut host, NamedKey::ArrowLeft);
    tap_named(&mut runner, &mut host, NamedKey::Delete);
    assert_eq!(runner.app().log.last(), Some(&Msg::Password("s3cr".into())));
    runner.send(&mut host, Msg::Read);
    assert_eq!(runner.app().read.as_deref(), Some("s3cr"));

    // No copying out of it.
    press(&mut host, Key::Character("a".into()), command());
    press(&mut host, Key::Character("c".into()), command());
    assert_eq!(host.only().clipboard_contents(), "");

    // Assistive technology sees a password field of bullets.
    let _ = host.only().render();
    let node = host.only().access_node(password).expect("a node");
    assert_eq!(node.role(), Role::PasswordInput);
    let tree = format!("{:?}", host.only().access_tree());
    assert!(!tree.contains("s3cr"), "the real text leaked: {tree}");
    assert!(tree.contains("••••"), "bullets for the text: {tree}");
}

#[test]
fn the_app_sets_text_without_a_message_and_disabled_fields_ignore_typing() {
    let (mut runner, mut host) = setup();
    // Disabling the focused field stops its typing.
    focus(&mut host, &runner.app().note);
    runner.send(&mut host, Msg::Set);
    type_text(&mut runner, &mut host, "x");
    assert!(runner.app().log.is_empty(), "{:?}", runner.app().log);

    let name = id(&runner.app().name);
    let _ = host.only().render();
    let tree = format!("{:?}", host.only().access_tree());
    assert!(tree.contains("Set by the app"), "{tree}");
    assert_eq!(
        host.only().access_node(name).map(|n| n.role()),
        Some(Role::TextInput)
    );
}
