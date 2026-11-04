//! The gallery's pages, one per widget (or family of widgets).

use neue_kit::tokens::metrics::spacing;
use neue_kit::{
    BevelButton, Button, Checkbox, Element, Handle, Label, MaterialTile, ProgressBar, Radio,
    Segmented, Slider, Spinner, Stack, Stepper, TextField, TextStyle,
};

use crate::Message;
use crate::specimens;

/// Handles to widgets of the current page that the gallery's `update` changes: the app
/// owns a radio group's choice, so it turns the other buttons off.
#[derive(Debug, Default)]
pub struct PageHandles {
    pub radios: Vec<Handle<Radio<Message>>>,
    /// The Values page: what the slider and the stepper drive.
    pub slider_value: Handle<Label>,
    pub slider_progress: Handle<ProgressBar>,
    pub stepper_value: Handle<Label>,
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
        title: "Values",
        build: values,
    },
    Page {
        title: "Fields",
        build: fields,
    },
    Page {
        title: "Scrolling",
        build: scrolling,
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

fn values(handles: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Values",
            "Drag the slider or use its arrow keys, Home, End and Page Up/Down · Up and Down              step the stepper · stripes and spokes stop under reduced motion",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(
                    Stack::column()
                        .spacing(spacing::L)
                        .with(section(
                            "Slider (drives the bar below)",
                            Stack::column()
                                .with(specimens::slider(0.4).on_change(Message::Slider))
                                .with(Slider::<Message>::new().value(0.7).disabled(true)),
                        ))
                        .with(section(
                            "Stepper",
                            Stack::row()
                                .with(specimens::stepper().on_change(Message::Stepper))
                                .with(Label::new("5").bind(&mut handles.stepper_value))
                                .with(Stepper::<Message>::new().disabled(true)),
                        )),
                )
                .with(
                    Stack::column()
                        .spacing(spacing::L)
                        .with(section(
                            "Progress",
                            Stack::column()
                                .with(specimens::progress_bar(0.4).bind(&mut handles.slider_progress))
                                .with(Label::new("40%").bind(&mut handles.slider_value))
                                .with(specimens::progress_bar(1.0))
                                .with(ProgressBar::new().value(0.5).disabled(true)),
                        ))
                        .with(section(
                            "Indeterminate (barber pole)",
                            Stack::column()
                                .with(specimens::barber_pole())
                                .with(ProgressBar::new().disabled(true)),
                        ))
                        .with(section(
                            "Spinner",
                            Stack::row()
                                .with(specimens::spinner())
                                .with(Spinner::new().label("Stopped").spinning(false)),
                        )),
                ),
        )
        .into()
}

fn fields(_: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Fields",
            "Return submits the first field · Escape or × clears the search field · \
             the status line shows what each field sends",
        ))
        .with(section(
            "Text field",
            Stack::column()
                .with(
                    specimens::text_field()
                        .on_change(|t| Message::Field("Name", t))
                        .on_submit(Message::Submitted),
                )
                .with(specimens::text_field_filled().disabled(true)),
        ))
        .with(section(
            "Search field",
            Stack::column()
                .with(specimens::search_field().on_change(|t| Message::Field("Search", t))),
        ))
        .with(section(
            "Secure field",
            Stack::column()
                .with(specimens::secure_field().on_change(|t| Message::Field("Password", t))),
        ))
        .with(section(
            "Narrow (scrolls to keep the caret in view)",
            Stack::column().with(TextField::new().width(90.0).text("Longer than the field")),
        ))
        .into()
}

fn scrolling(_: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Scrolling",
            "Wheel or swipe over a view · the scrollers fade a second after scrolling stops \
             · hover one to widen it, drag its knob or press beside it · Tab to a view for \
             the arrow keys, Page Up/Down, Space, Home and End",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Scroll view",
                    Stack::column().with(specimens::scroll_view(40)),
                ))
                .with(section(
                    "Both directions",
                    Stack::column().with(specimens::scroll_view_both()),
                ))
                .with(section(
                    "Virtual list (1,000,000 rows)",
                    Stack::column().with(specimens::virtual_list(1_000_000)),
                )),
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
