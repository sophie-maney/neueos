//! The gallery's pages, one per widget (or family of widgets).

use neue_kit::tokens::metrics::spacing;
use neue_kit::{
    BevelButton, Button, Checkbox, Element, Handle, Label, MaterialTile, Radio, Segmented, Stack,
    TextStyle,
};

use crate::Message;
use crate::specimens;

/// Handles to widgets of the current page that the gallery's `update` changes: the app
/// owns a radio group's choice, so it turns the other buttons off.
#[derive(Debug, Default)]
pub struct PageHandles {
    pub radios: Vec<Handle<Radio<Message>>>,
}

/// A gallery page: its title in the page list, and its content.
#[derive(Debug)]
pub struct Page {
    pub title: &'static str,
    pub build: fn(&mut PageHandles) -> Element<Message>,
}

/// Every page, in page-list order.
pub const PAGES: &[Page] = &[
    Page {
        title: "Push Button",
        build: push_buttons,
    },
    Page {
        title: "Buttons",
        build: buttons,
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

fn push_buttons(_: &mut PageHandles) -> Element<Message> {
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

/// The names the radio group's buttons report.
pub const RADIO_NAMES: [&str; 3] = ["Radio: Aqua", "Radio: Graphite", "Radio: Linen"];

/// The segmented control's segments report these.
const SEGMENT_NAMES: [&str; 3] = ["Segment One", "Segment Two", "Segment Three"];

fn buttons(handles: &mut PageHandles) -> Element<Message> {
    handles.radios = vec![Handle::default(); RADIO_NAMES.len()];
    let mut radios = Stack::column().spacing(spacing::XS);
    for (n, (name, handle)) in ["Aqua", "Graphite", "Linen"]
        .into_iter()
        .zip(&mut handles.radios)
        .enumerate()
    {
        radios = radios.with(
            Radio::new(name)
                .selected(n == 0)
                .on_select(Message::Radio(n))
                .bind(handle),
        );
    }
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Buttons",
            "Tab moves keyboard focus · Space toggles the focused control · Left and Right \
             move the segmented selection",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(
                    Stack::column()
                        .spacing(spacing::L)
                        .with(section(
                            "Checkbox",
                            Stack::column()
                                .spacing(spacing::XS)
                                .with(Checkbox::new("Off").on_toggle(|on| {
                                    Message::Pressed(["Check off", "Check on"][usize::from(on)])
                                }))
                                .with(specimens::checkbox_on().on_toggle(|on| {
                                    Message::Pressed(["Check off", "Check on"][usize::from(on)])
                                }))
                                .with(specimens::checkbox_mixed().on_toggle(|on| {
                                    Message::Pressed(["Check off", "Check on"][usize::from(on)])
                                }))
                                .with(Checkbox::<Message>::new("Disabled").disabled(true)),
                        ))
                        .with(section(
                            "Radio buttons (the app owns the choice)",
                            radios.with(Radio::<Message>::new("Disabled").disabled(true)),
                        )),
                )
                .with(
                    Stack::column()
                        .spacing(spacing::L)
                        .with(section(
                            "Bevel",
                            Stack::row()
                                .with(BevelButton::new("Bevel").on_press(Message::Pressed("Bevel")))
                                .with(
                                    BevelButton::new("Toggle")
                                        .on_toggle(|_| Message::Pressed("Bevel toggle")),
                                )
                                .with(
                                    specimens::bevel_button_on()
                                        .on_toggle(|_| Message::Pressed("Bevel toggle")),
                                )
                                .with(BevelButton::<Message>::new("Disabled").disabled(true)),
                        ))
                        .with(section(
                            "Recessed",
                            Stack::row()
                                .with(
                                    BevelButton::recessed("Recessed")
                                        .on_press(Message::Pressed("Recessed")),
                                )
                                .with(
                                    BevelButton::recessed("Toggle")
                                        .on_toggle(|_| Message::Pressed("Recessed toggle")),
                                )
                                .with(
                                    specimens::recessed_button_on()
                                        .on_toggle(|_| Message::Pressed("Recessed toggle")),
                                )
                                .with(BevelButton::<Message>::recessed("Disabled").disabled(true)),
                        ))
                        .with(section(
                            "Segmented control",
                            Stack::row()
                                .with(
                                    specimens::segmented(0)
                                        .on_select(|i| Message::Pressed(SEGMENT_NAMES[i])),
                                )
                                .with(
                                    Segmented::<Message>::new(["Disabled", "Off"]).disabled(true),
                                ),
                        )),
                ),
        )
        .into()
}

fn text(_: &mut PageHandles) -> Element<Message> {
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

fn materials(_: &mut PageHandles) -> Element<Message> {
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
