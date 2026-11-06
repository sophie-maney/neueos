//! Shell surfaces (3.2a) on the fake host: layer windows, output events opening and
//! closing a surface per output, and the compositor's window list reaching the app.

use std::collections::BTreeMap;

use neue_platform::WindowKind;

use super::*;
use crate::desktop::{
    Anchor, LayerSpec, OutputEvent, OutputId, OutputInfo, ShellEvent, ShellRequest, Toplevel,
    ToplevelId,
};

/// A miniature shell: a bar per output, and the active window's title.
#[derive(Default)]
struct MiniShell {
    bars: BTreeMap<OutputId, WindowId>,
    windows: BTreeMap<ToplevelId, Toplevel>,
    active: Option<ToplevelId>,
}

impl MiniShell {
    fn active_title(&self) -> Option<&str> {
        self.active
            .and_then(|id| self.windows.get(&id))
            .map(|t| t.title.as_str())
    }
}

impl Application for MiniShell {
    type Message = ();

    fn start(&mut self, _cx: &mut Cx<'_, ()>) {}

    fn update(&mut self, _cx: &mut Cx<'_, ()>, (): ()) {}

    fn output(&mut self, cx: &mut Cx<'_, ()>, event: OutputEvent) {
        match event {
            OutputEvent::Added(info) => {
                let layer = LayerSpec {
                    exclusive_zone: 22,
                    output: Some(info.id),
                    ..LayerSpec::new("neue-menubar", Anchor::TOP_EDGE)
                };
                let bar = Window::new("Menu bar", (0, 22), Label::new("bar")).layer(layer);
                self.bars.insert(info.id, cx.open_window(bar));
            }
            OutputEvent::Changed(_) => {}
            OutputEvent::Removed(id) => {
                if let Some(bar) = self.bars.remove(&id) {
                    cx.close_window(bar);
                }
            }
        }
    }

    fn shell(&mut self, cx: &mut Cx<'_, ()>, event: ShellEvent) {
        match event {
            ShellEvent::Toplevel(id, info) => {
                // A window asking for attention gets it (a stand-in for a dock click).
                if info.attention {
                    cx.shell(ShellRequest::Activate(id));
                }
                self.windows.insert(id, info);
            }
            ShellEvent::Closed(id) => {
                self.windows.remove(&id);
            }
            ShellEvent::Active(id) => self.active = id,
            ShellEvent::ActionFailed { .. }
            | ShellEvent::FocusMenuBar { .. }
            | ShellEvent::SystemKey { .. }
            | ShellEvent::Thumbnail(..) => {}
        }
    }
}

fn output(raw: u64) -> OutputInfo {
    OutputInfo {
        id: OutputId::from_raw(raw),
        name: format!("HEADLESS-{raw}"),
        description: String::new(),
        logical_size: (1280, 800),
        scale: 1,
    }
}

#[test]
fn a_layer_surface_opens_per_output_and_closes_with_it() {
    let (mut runner, mut host, _) = start(MiniShell::default());
    runner.output(&mut host, OutputEvent::Added(output(0)));
    runner.output(&mut host, OutputEvent::Added(output(1)));
    assert_eq!(host.windows.len(), 2);

    let bar = runner.app().bars[&OutputId::from_raw(1)];
    let WindowKind::Layer(spec) = &host.kinds[&bar] else {
        panic!("the bar is a layer surface: {:?}", host.kinds[&bar]);
    };
    assert_eq!(spec.output, Some(OutputId::from_raw(1)));
    assert_eq!(spec.exclusive_zone, 22);
    let (_, decorations, background) = host.specs[&bar];
    assert_eq!(
        decorations,
        Decorations::Server,
        "layer surfaces have no chrome"
    );
    assert_eq!(
        background,
        Color::TRANSPARENT,
        "the root paints the whole surface"
    );

    runner.output(&mut host, OutputEvent::Removed(OutputId::from_raw(0)));
    assert_eq!(host.windows.len(), 1);
    assert!(host.windows.contains_key(&bar));
}

#[test]
fn a_layer_surface_can_follow_its_content() {
    /// Opens a corner panel sized by its content, and a bar that isn't.
    struct Corner;
    impl Application for Corner {
        type Message = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            let corner = LayerSpec::new("neue-notification", Anchor::TOP_RIGHT);
            cx.open_window(
                Window::new("Banners", (1, 1), Label::new("hi"))
                    .layer(corner)
                    .fit_content(),
            );
            let bar = LayerSpec::new("neue-menubar", Anchor::TOP_EDGE);
            cx.open_window(Window::new("Bar", (0, 22), Label::new("bar")).layer(bar));
            // A toplevel has no use for it and opens as usual.
            cx.open_window(Window::new("Doc", (300, 200), Label::new("doc")).fit_content());
        }
        fn update(&mut self, _: &mut Cx<'_, ()>, (): ()) {}
    }
    let (_runner, host, _) = start(Corner);
    let fits: Vec<Option<bool>> = host
        .kinds
        .values()
        .map(|kind| match kind {
            WindowKind::Layer(spec) => Some(spec.fit_content),
            WindowKind::Toplevel => None,
        })
        .collect();
    assert_eq!(fits, [Some(true), Some(false), None]);
}

#[test]
fn the_window_list_reaches_the_app_and_requests_reach_the_compositor() {
    let (mut runner, mut host, _) = start(MiniShell::default());
    host.shell = true;
    let (a, b) = (ToplevelId::from_raw(0), ToplevelId::from_raw(1));
    let toplevel = |title: &str| Toplevel {
        identifier: format!("window-{title}"),
        title: title.into(),
        ..Toplevel::default()
    };
    runner.shell(&mut host, ShellEvent::Toplevel(a, toplevel("Notes")));
    runner.shell(&mut host, ShellEvent::Toplevel(b, toplevel("Terminal")));
    runner.shell(&mut host, ShellEvent::Active(Some(b)));
    assert_eq!(runner.app().active_title(), Some("Terminal"));

    runner.shell(&mut host, ShellEvent::Closed(b));
    runner.shell(&mut host, ShellEvent::Active(None));
    assert_eq!(runner.app().active_title(), None);
    assert_eq!(runner.app().windows.len(), 1);

    // The app asks the compositor to act through `Cx::shell`; without the protocol the
    // request is dropped.
    let attention = Toplevel {
        attention: true,
        ..toplevel("Notes")
    };
    runner.shell(&mut host, ShellEvent::Toplevel(a, attention.clone()));
    assert_eq!(host.shell_requests, [ShellRequest::Activate(a)]);
    host.shell = false;
    runner.shell(&mut host, ShellEvent::Toplevel(a, attention));
    assert_eq!(host.shell_requests.len(), 1);
}

#[test]
fn cx_reports_outputs_and_whether_it_is_the_shell() {
    struct Probe(Vec<(usize, bool)>);
    impl Application for Probe {
        type Message = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            self.0.push((cx.outputs().len(), cx.has_shell()));
        }
        fn update(&mut self, _: &mut Cx<'_, ()>, (): ()) {}
    }
    let (runner, ..) = start(Probe(Vec::new()));
    assert_eq!(runner.app().0, [(0, false)]);
}

#[test]
fn activation_tokens_come_back_as_messages_once() {
    /// Asks for two tokens for its banner (as two clicks would) and keeps the answers.
    #[derive(Default)]
    struct Clicker {
        window: Option<WindowId>,
        tokens: Vec<Option<String>>,
    }
    impl Application for Clicker {
        type Message = Option<String>;
        fn start(&mut self, cx: &mut Cx<'_, Option<String>>) {
            let banner = Window::new("Banner", (100, 40), Label::new("hi"))
                .layer(LayerSpec::new("neue-notification", Anchor::TOP_EDGE));
            let window = cx.open_window(banner);
            self.window = Some(window);
            cx.activation_token(window, |token| token);
            cx.activation_token(window, |token| token);
        }
        fn update(&mut self, _: &mut Cx<'_, Option<String>>, token: Option<String>) {
            self.tokens.push(token);
        }
    }
    let (mut runner, mut host, _) = start(Clicker::default());
    let window = runner.app().window.expect("opened in start");
    assert_eq!(host.token_requests.len(), 2);
    assert!(host.token_requests.iter().all(|(_, w)| *w == window));
    let (first, second) = (host.token_requests[0].0, host.token_requests[1].0);
    assert!(runner.app().tokens.is_empty());

    // Answers may come in any order; each reaches `update` once, as its own message.
    runner.activation_token(&mut host, second, None);
    runner.activation_token(&mut host, first, Some("tok".into()));
    runner.activation_token(&mut host, first, Some("again".into()));
    assert_eq!(runner.app().tokens, [None, Some("tok".to_owned())]);
}
