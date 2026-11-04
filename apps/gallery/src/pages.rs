//! The gallery's pages, one per widget (or family of widgets).

use neue_kit::tokens::metrics::spacing;
use neue_kit::{Button, Element, Label, MaterialTile, Stack, TextStyle};

use crate::Message;
use crate::specimens;

/// A gallery page: its title in the page list, and its content.
#[derive(Debug)]
pub struct Page {
    pub title: &'static str,
    pub build: fn() -> Element<Message>,
}

/// Every page, in page-list order.
pub const PAGES: &[Page] = &[
    Page {
        title: "Push Button",
        build: push_buttons,
    },
    Page {
        title: "Text",
        build: text,
    },
    Page {
        title: "Materials",
        build: materials,
    },
];

fn heading(title: &str, blurb: &str) -> Stack<Message> {
    Stack::column()
        .spacing(spacing::XS)
        .with(Label::new(title).style(TextStyle::Headline))
        .with(Label::new(blurb).style(TextStyle::Small).secondary())
}

fn section(title: &str, row: Stack<Message>) -> Stack<Message> {
    Stack::column()
        .spacing(spacing::S)
        .with(Label::new(title).style(TextStyle::Small).secondary())
        .with(row.spacing(spacing::S))
}

fn push_buttons() -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Push Button",
            "Tab moves keyboard focus · Space presses the focused button · Return presses \
             the default (pulsing) button",
        ))
        .with(section(
            "Dialog",
            Stack::row()
                .with(Button::new("Cancel").on_press(Message::Pressed("Cancel")))
                .with(specimens::default_button().on_press(Message::Pressed("OK"))),
        ))
        .with(section(
            "States",
            Stack::row()
                .with(specimens::push_button().on_press(Message::Pressed("Button")))
                .with(Button::new("Disabled").disabled(true)),
        ))
        .with(section(
            "Titles",
            Stack::row()
                .with(Button::new("A").on_press(Message::Pressed("A")))
                .with(
                    Button::new("A longer title widens the button")
                        .on_press(Message::Pressed("A longer title")),
                ),
        ))
        .into()
}

fn text() -> Element<Message> {
    let styles = [
        (TextStyle::Display, "Display"),
        (TextStyle::Title, "Title"),
        (TextStyle::Headline, "Headline"),
        (
            TextStyle::Body,
            "Body: the quick brown fox jumps over the lazy dog",
        ),
        (
            TextStyle::Small,
            "Small: captions and secondary information",
        ),
    ];
    let mut page = Stack::column().spacing(spacing::L).with(heading(
        "Text",
        "The type scale; the UI font is chosen in task 2.10",
    ));
    let mut list = Stack::column().spacing(spacing::S);
    for (style, sample) in styles {
        list = list.with(Label::new(sample).style(style));
    }
    page = page.with(list.with(Label::new("Secondary label").secondary()));
    page.into()
}

fn materials() -> Element<Message> {
    let column = |translucent: bool| {
        let tile = |tile| specimens::material(tile, translucent);
        Stack::column()
            .spacing(spacing::M)
            .with(section(
                "Menu bar",
                Stack::row().with(tile(MaterialTile::MenuBar)),
            ))
            .with(section(
                "Dock shelf",
                Stack::row().with(tile(MaterialTile::DockShelf)),
            ))
            .with(section(
                "Dock shelf, flat (left and right edges)",
                Stack::row().with(tile(MaterialTile::DockShelfFlat)),
            ))
            .with(section(
                "HUD panel",
                Stack::row().with(tile(MaterialTile::Hud)),
            ))
    };
    Stack::column()
        .spacing(spacing::M)
        .with(heading(
            "Materials",
            "Left: opaque (no blur behind). Right: translucent (blurred backdrop, Phase 3).",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(column(false))
                .with(column(true)),
        )
        .into()
}
