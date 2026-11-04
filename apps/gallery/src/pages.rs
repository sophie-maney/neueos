//! The gallery's pages, one per widget (or family of widgets).

use neue_kit::tokens::metrics::spacing;
use neue_kit::{Button, Element, Label, Stack, TextStyle};

use crate::Message;
use crate::fonts::CANDIDATES;
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
        title: "Fonts",
        build: fonts,
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
    let mut page = Stack::column()
        .spacing(spacing::L)
        .with(heading("Text", "The type scale in the UI font"));
    let mut list = Stack::column().spacing(spacing::S);
    for (style, sample) in styles {
        list = list.with(Label::new(sample).style(style));
    }
    page = page.with(list.with(Label::new("Secondary label").secondary()));
    page.into()
}

/// The same specimen in each UI font candidate (task 2.10, PLAN open question 2).
fn fonts() -> Element<Message> {
    let specimen = |family: &'static str| {
        Stack::column()
            .spacing(spacing::XS)
            .with(Label::new(family).style(TextStyle::Small).secondary())
            .with(
                Label::new("Display 0123456789")
                    .family(family)
                    .style(TextStyle::Display),
            )
            .with(
                Label::new("Title: Hamburgefonstiv")
                    .family(family)
                    .style(TextStyle::Title),
            )
            .with(
                Label::new("Headline: Sharing & Privacy")
                    .family(family)
                    .style(TextStyle::Headline),
            )
            .with(
                Label::new("Body: Quick brown fox, Il1 O0 rn m g y ? ! @ &")
                    .family(family)
                    .style(TextStyle::Body),
            )
            .with(
                Label::new("Small: 3 items, 12.4 GB available — 100%")
                    .family(family)
                    .style(TextStyle::Small),
            )
    };
    let mut grid = Stack::column().spacing(spacing::M);
    for pair in CANDIDATES.chunks(2) {
        let mut row = Stack::row().spacing(spacing::L);
        for candidate in pair {
            row = row.with(specimen(candidate.family));
        }
        grid = grid.with(row);
    }
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Fonts",
            "UI font candidates at the type scale (Regular only): the owner picks (task 2.10)",
        ))
        .with(grid)
        .into()
}
