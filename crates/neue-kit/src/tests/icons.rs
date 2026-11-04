//! The 2.20a icon image end to end on the fake host: what assistive technology is told, its
//! size, and a change of icon from the app.

use masonry_core::accesskit::Role;
use masonry_core::core::WidgetId;

use super::*;
use crate::{Glyph, Icon, IconImage, IconPaint, VectorIcon};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// Script: the app swaps the first icon.
    Swap,
}

#[derive(Default)]
struct Icons {
    named: Handle<IconImage>,
    labelled: Handle<IconImage>,
    decorative: Handle<IconImage>,
    big: Handle<IconImage>,
}

fn vector() -> Icon {
    VectorIcon::new(8.0, 8.0)
        .and_then(|v| v.shape("M0 0H8V8H0z", IconPaint::CURRENT))
        .map(Icon::from)
        .expect("valid path data")
}

impl Application for Icons {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .with(IconImage::new(Glyph::Folder).bind(&mut self.named))
            .with(
                IconImage::new(Glyph::Star)
                    .label("Favourite")
                    .bind(&mut self.labelled),
            )
            .with(IconImage::new(vector()).bind(&mut self.decorative))
            .with(IconImage::new(vector()).size(48.0).bind(&mut self.big));
        cx.open_window(Window::new("Icons", (200, 200), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Swap => self.named.set_icon(cx, Glyph::Music),
        }
    }
}

fn id<T>(handle: &Handle<T>) -> WidgetId {
    handle.id().expect("bound")
}

#[test]
fn icons_are_images_named_by_their_label_or_glyph() {
    let (runner, mut host, _) = start(Icons::default());
    let _ = host.only().render();
    let app = runner.app();
    for handle in [&app.named, &app.labelled, &app.decorative] {
        let node = host.only().access_node(id(handle)).expect("node");
        assert_eq!(node.role(), Role::Image);
    }
    let label = |host: &mut FakeHost, h: &Handle<IconImage>| {
        host.only()
            .access_node(id(h))
            .and_then(|n| n.label().map(|l| l.to_string()))
    };
    assert_eq!(label(&mut host, &app.named).as_deref(), Some("Folder"));
    assert_eq!(
        label(&mut host, &app.labelled).as_deref(),
        Some("Favourite")
    );
    assert_eq!(label(&mut host, &app.decorative), None, "decorative");
}

#[test]
fn an_icon_is_as_large_as_asked() {
    let (runner, mut host, _) = start(Icons::default());
    let _ = host.only().render();
    let app = runner.app();
    let size = |host: &mut FakeHost, h: &Handle<IconImage>| {
        let widget = host.only().get_widget_with_id(id(h));
        widget.ctx().border_box().size()
    };
    assert_eq!(size(&mut host, &app.named), (16.0, 16.0).into());
    assert_eq!(size(&mut host, &app.big), (48.0, 48.0).into());
}

#[test]
fn the_app_can_swap_the_icon_and_its_name_follows() {
    let (mut runner, mut host, _) = start(Icons::default());
    runner.send(&mut host, Msg::Swap);
    let _ = host.only().render();
    let node = host
        .only()
        .access_node(id(&runner.app().named))
        .expect("node");
    assert_eq!(node.label().as_deref(), Some("Music"));
}
