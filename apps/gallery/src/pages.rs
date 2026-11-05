//! The gallery's pages, one per widget (or family of widgets).

use neue_kit::tokens::metrics::spacing;
use std::cell::RefCell;
use std::rc::Rc;

use neue_kit::{
    Align, BevelButton, Button, Cell, Checkbox, Column, ContextMenu, DividerStyle, Element, Handle,
    Label, MaterialTile, Pane, PopUpButton, ProgressBar, Radio, Segmented, Slider, SortOrder,
    Spinner, SplitView, Stack, Stepper, TabView, Table, TextField, TextStyle, Toolbar,
};
use neue_kit::{DisplayMode, Edge, Popover};

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
    /// The Tables page: the files table and the files it shows, in its order.
    pub files: Rc<RefCell<Vec<specimens::SampleFile>>>,
    pub files_table: Handle<Table<Message>>,
    /// The Toolbar page: the toolbar whose display mode the segmented control changes.
    pub toolbar: Handle<Toolbar<Message>>,
    /// The Popovers page: the buttons that show one on each side, and which one has its
    /// popover open.
    pub popover_anchors: Vec<Handle<Button<Message>>>,
    pub popover_open: Option<usize>,
}

impl PageHandles {
    /// Sorts the Tables page's files by `column` and shows them again.
    pub fn sort_files(&self, cx: &mut neue_kit::Cx<'_, Message>, column: usize, order: SortOrder) {
        let mut files = self.files.borrow_mut();
        files.sort_by(|a, b| match column {
            0 => a.0.to_lowercase().cmp(&b.0.to_lowercase()),
            1 => a.1.cmp(&b.1),
            _ => a.2.cmp(b.2).then_with(|| a.0.cmp(b.0)),
        });
        if order == SortOrder::Descending {
            files.reverse();
        }
        drop(files);
        // The rows moved: the old selection would point at other files.
        self.files_table.set_selection(cx, []);
        self.files_table.reload(cx);
    }

    /// Shows the Popovers page's popover on the side of button `n`.
    pub fn show_popover(&mut self, cx: &mut neue_kit::Cx<'_, Message>, n: usize) {
        let (Some(anchor), Some(&(_, edge))) = (self.popover_anchors.get(n), POPOVER_SIDES.get(n))
        else {
            return;
        };
        let content = specimens::popover_content(Some(Message::PopoverDone));
        cx.show_popover(
            anchor,
            Popover::new(content)
                .edge(edge)
                .on_close(Message::PopoverClosed),
        );
        self.popover_open = Some(n);
    }

    /// Closes the Popovers page's open popover.
    pub fn close_popover(&mut self, cx: &mut neue_kit::Cx<'_, Message>) {
        if let Some(anchor) = self
            .popover_open
            .take()
            .and_then(|n| self.popover_anchors.get(n))
        {
            cx.close_popover(anchor);
        }
    }

    /// The name of the file in row `row` of the Tables page.
    pub fn file_name(&self, row: usize) -> Option<&'static str> {
        self.files.borrow().get(row).map(|f| f.0)
    }
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
        title: "Text View",
        build: text_view,
    },
    Page {
        title: "Scrolling",
        build: scrolling,
    },
    Page {
        title: "Menus",
        build: menus,
    },
    Page {
        title: "Pop-ups",
        build: pop_ups,
    },
    Page {
        title: "Tables",
        build: tables,
    },
    Page {
        title: "Sidebar",
        build: sidebar,
    },
    Page {
        title: "Icons",
        build: icons,
    },
    Page {
        title: "Panes",
        build: panes,
    },
    Page {
        title: "Toolbar",
        build: toolbar,
    },
    Page {
        title: "Popovers",
        build: popovers,
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

fn text_view(_: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Text View",
            "Return starts a line · Up and Down move between lines · Page Up and Page Down \
             scroll · the caret stays in view while you type · the status line shows what \
             the editable view sends",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Editable",
                    Stack::column().with(
                        specimens::text_view_filled().on_change(|t| Message::Field("Notes", t)),
                    ),
                ))
                .with(section(
                    "Read only (select and copy)",
                    Stack::column().with(specimens::text_view_read_only()),
                ))
                .with(section(
                    "Disabled",
                    Stack::column().with(specimens::text_view_filled().disabled(true)),
                )),
        )
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
                ))
                .with(section(
                    "Virtual grid (1,000,000 items)",
                    Stack::column().with(specimens::virtual_grid(1_000_000)),
                )),
        )
        .into()
}

fn tables(handles: &mut PageHandles) -> Element<Message> {
    *handles.files.borrow_mut() = specimens::FILES.to_vec();
    let files = Rc::clone(&handles.files);
    let table = Table::new(
        specimens::file_columns(),
        specimens::FILES.len(),
        move |row, column| {
            files
                .borrow()
                .get(row)
                .map(|file| specimens::file_cell(file, column))
                .unwrap_or_default()
        },
    )
    .multiple(true)
    .alternating_rows(true)
    .sorted(0, SortOrder::Ascending)
    .on_select(|rows| Message::TableSelection(rows.len()))
    .on_activate(Message::OpenFile)
    .on_sort(Message::SortFiles)
    .width(300.0)
    .height(200.0)
    .bind(&mut handles.files_table);
    let columns = [
        Column::new("#").width(60.0).align(Align::End),
        Column::new("Task").width(140.0),
        Column::new("Done").width(60.0),
    ];
    let tasks = Table::new(columns, 10_000, |row, column| -> Cell<Message> {
        match column {
            0 => format!("{}", row + 1).into(),
            1 => format!("Task number {}", row + 1).into(),
            _ => Cell::element(Checkbox::new("").checked(row % 3 == 0)),
        }
    })
    .on_select(|rows| Message::TableSelection(rows.len()))
    .width(300.0)
    .height(200.0);
    let folders = specimens::outline::<Message>()
        .multiple(true)
        .alternating_rows(true)
        .expanded([1])
        .on_select(|ids| Message::TableSelection(ids.len()))
        .width(300.0)
        .height(200.0);
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Tables",
            "Click a row · Shift-click or Shift-arrows extend the selection, Cmd/Ctrl-click \
             adds a row · type a name to jump · Return or double-click opens · click a title \
             to sort, again to reverse · drag a divider to resize a column · in the outline, \
             click a triangle (Option-click: every level below) or press Right and Left to \
             open and close folders",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Files (sortable, multiple selection)",
                    Stack::column().with(table),
                ))
                .with(section(
                    "10,000 rows, element cells",
                    Stack::column().with(tasks),
                )),
        )
        .with(section("Outline (folders)", Stack::column().with(folders)))
        .into()
}

fn sidebar(_: &mut PageHandles) -> Element<Message> {
    let list = specimens::source_list::<Message>()
        .selected(11)
        .on_select(|id| Message::SidebarSelected(id.copied()))
        .on_activate(|&id| Message::SidebarOpened(id))
        .width(200.0)
        .height(260.0);
    let nested = specimens::source_list::<Message>()
        .expanded([13])
        .collapsed([2])
        .on_select(|id| Message::SidebarSelected(id.copied()))
        .width(200.0)
        .height(260.0);
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Sidebar",
            "Click an item · Up and Down skip the group headers · Right and Left open and \
             close an item with items · type a name to jump · Return or double-click opens · \
             point at a group header and click Hide or Show · the selection is blue in the \
             focused list and grey otherwise",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section("Source list", Stack::column().with(list)))
                .with(section(
                    "Nested items, Devices closed",
                    Stack::column().with(nested),
                )),
        )
        .into()
}

fn icons(_: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Icons",
            "Glyphs are drawn in the text colour at any size · a vector icon comes from SVG \
             path data, with colours that follow the text or have their own for light and dark \
             · switch the appearance and the scale to see them stay sharp · in a collection \
             click, Shift-click and Cmd- or Ctrl-click select · the arrow keys move over the \
             grid · type a name to jump · Return or double-click opens · resize the window to \
             change the columns",
        ))
        .with(section(
            "Glyphs, 32 px",
            Stack::column().with(specimens::icon_glyphs()),
        ))
        .with(section(
            "A vector icon at 16, 32 and 48 px",
            Stack::column().with(specimens::icon_vector()),
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Collection, 32 px icons, 1,000 items",
                    Stack::column().with(
                        specimens::collection_many(1000, 32.0)
                            .on_select(|ids| Message::CollectionSelected(ids.to_vec()))
                            .on_activate(|&id| Message::CollectionOpened(id))
                            .width(330.0)
                            .height(250.0),
                    ),
                ))
                .with(section(
                    "64 px icons",
                    Stack::column().with(
                        specimens::collection_many(12, 64.0)
                            .on_select(|ids| Message::CollectionSelected(ids.to_vec()))
                            .on_activate(|&id| Message::CollectionOpened(id))
                            .width(330.0)
                            .height(250.0),
                    ),
                )),
        )
        .into()
}

fn panes(_: &mut PageHandles) -> Element<Message> {
    let sized = |axis_vertical: bool, style| {
        specimens::split_view::<Message>(style, axis_vertical)
            .on_resize(|lengths| Message::SplitResized(lengths.to_vec()))
    };
    let three = SplitView::horizontal()
        .divider(DividerStyle::Thick)
        .with_pane(
            Label::new("Sidebar"),
            Pane::new().size(90.0).min(60.0).holds_size().collapsible(),
        )
        .with(Label::new("Content"))
        .with_pane(
            Label::new("Inspector"),
            Pane::new().size(90.0).min(60.0).holds_size().collapsible(),
        )
        .on_resize(|lengths| Message::SplitResized(lengths.to_vec()))
        .width(420.0)
        .height(120.0);
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Panes",
            "Drag a divider to resize the panes next to it · a sidebar stops at its limits, \
             and collapses when dragged well below its minimum · double-click a divider, or \
             press Return on it, to collapse or bring back a pane · Tab to a divider: the \
             arrow keys along the split move it, the ones across pick another divider, Home \
             and End go to either end",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Thin divider",
                    Stack::column().with(sized(false, DividerStyle::Thin)),
                ))
                .with(section(
                    "Thick divider",
                    Stack::column().with(sized(false, DividerStyle::Thick)),
                ))
                .with(section(
                    "Stacked",
                    Stack::column().with(sized(true, DividerStyle::Thick)),
                )),
        )
        .with(section(
            "Three panes, the outer two collapsible",
            Stack::column().with(three),
        ))
        .with(section(
            "Tab view · Left and Right on the strip, Ctrl-Tab from anywhere inside",
            Stack::row()
                .spacing(spacing::L)
                .with(
                    specimens::tab_view::<Message>(0)
                        .on_select(Message::TabSelected)
                        .width(260.0)
                        .height(110.0),
                )
                .with(
                    TabView::new()
                        .with_tab(
                            "Fields",
                            Stack::column()
                                .spacing(spacing::S)
                                .with(TextField::new().placeholder("Name"))
                                .with(Checkbox::new("Remember me")),
                        )
                        .with_tab("About", Label::new("A tab holds any content"))
                        .on_select(Message::TabSelected)
                        .width(260.0)
                        .height(110.0),
                ),
        ))
        .into()
}

/// What the Toolbar page's segmented control offers, in its order.
pub const DISPLAY_MODES: [DisplayMode; 3] = [
    DisplayMode::IconAndText,
    DisplayMode::IconOnly,
    DisplayMode::TextOnly,
];

fn toolbar(handles: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Toolbar",
            "The items of a window's toolbar (this gallery's own, at the top, holds its \
             switches) · press a button by click, or Tab to it and press Space · the \
             segmented control changes what the first toolbar's buttons show · a flexible \
             space pushes the search field to the right · the disabled button ignores input",
        ))
        .with(section(
            "Display mode",
            Stack::row().with(
                Segmented::new(["Icon and text", "Icon only", "Text only"])
                    .on_select(Message::ToolbarMode),
            ),
        ))
        .with(
            Stack::column()
                .spacing(spacing::M)
                .with(
                    specimens::toolbar_items::<Message>(DisplayMode::IconAndText)
                        .bind(&mut handles.toolbar),
                )
                .with(specimens::toolbar_items::<Message>(DisplayMode::IconOnly))
                .with(specimens::toolbar_items::<Message>(DisplayMode::TextOnly)),
        )
        .into()
}

/// The Popovers page's buttons: their labels and the side their popover asks for.
pub const POPOVER_SIDES: [(&str, Edge); 4] = [
    ("Below", Edge::Bottom),
    ("Above", Edge::Top),
    ("Left", Edge::Left),
    ("Right", Edge::Right),
];

fn popovers(handles: &mut PageHandles) -> Element<Message> {
    handles.popover_anchors = vec![Handle::default(); POPOVER_SIDES.len()];
    handles.popover_open = None;
    let mut row = Stack::row();
    for (n, ((label, _), handle)) in POPOVER_SIDES
        .iter()
        .zip(&mut handles.popover_anchors)
        .enumerate()
    {
        row = row.with(
            Button::new(*label)
                .on_press(Message::ShowPopover(n))
                .bind(handle),
        );
    }
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Popovers",
            "Press a button to show a popover on that side of it (the compositor may flip it \
             to the other side near a screen edge; its arrow follows) · Tab moves into it · \
             Escape, a press outside it, or its button again closes it · Done closes it from \
             the app",
        ))
        .with(section("Show a popover", row))
        .with(heading(
            "Sheets",
            "A modal panel that comes out from under the window's title bar: the window takes \
             no clicks while it is open · Return presses Save, Escape Cancel · Tab stays \
             inside it",
        ))
        .with(section(
            "Show a sheet",
            Stack::row().with(Button::new("Save…").on_press(Message::ShowSheet)),
        ))
        .with(heading(
            "HUD panel",
            "A small dark window for tools: a close button and a title, nothing else · it \
             stays above the gallery window, which keeps its focused look while the panel \
             has the keyboard · drag its bar to move it",
        ))
        .with(section(
            "Show a HUD window",
            Stack::row().with(Button::new("Inspector").on_press(Message::ShowHud)),
        ))
        .into()
}

fn menus(_: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Menus",
            "Right-click an area, or Tab to the field and press the Menu key or Shift-F10 · \
             Up and Down move · Right opens a submenu, Left closes it · Return chooses · \
             Escape closes · type to jump to an item · the status line shows what you chose",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Context menu",
                    Stack::column().with(specimens::context_menu(
                        Message::Pressed,
                        specimens::command,
                    )),
                ))
                .with(section(
                    "On a text field",
                    Stack::column().with(ContextMenu::new(
                        TextField::new().placeholder("Right-click or Shift-F10"),
                        specimens::sample_menu(Message::Pressed, specimens::command),
                    )),
                )),
        )
        .into()
}

fn pop_ups(_: &mut PageHandles) -> Element<Message> {
    Stack::column()
        .spacing(spacing::L)
        .with(heading(
            "Pop-ups",
            "Press a button to open its menu · Space, Return or Down open it from the keyboard · \
             type to choose in a closed pop-up · the combo box takes your own text too · \
             right-click a field for Cut, Copy and Select All · click a path segment, or use \
             Left, Right and Return · the status line shows what you chose",
        ))
        .with(
            Stack::row()
                .spacing(spacing::L)
                .with(section(
                    "Pop-up button",
                    Stack::column().with(
                        PopUpButton::new(specimens::SIZES)
                            .selected(1)
                            .on_select(|i| Message::Field("Size", specimens::SIZES[i].into())),
                    ),
                ))
                .with(section(
                    "Pull-down button",
                    Stack::column().with(specimens::pull_down_button(Message::Pressed)),
                ))
                .with(section(
                    "Disabled",
                    Stack::column()
                        .with(PopUpButton::new(specimens::SIZES).disabled(true))
                        .with(specimens::pull_down_button(Message::Pressed).disabled(true)),
                )),
        )
        .with(section(
            "Path control (standard and popup styles)",
            Stack::row()
                .spacing(spacing::L)
                .with(
                    specimens::path_control()
                        .on_select(|i| Message::Field("Path", specimens::PATH[i].into())),
                )
                .with(neue_kit::PathControl::popup(specimens::PATH, |i| {
                    Message::Field("Path", specimens::PATH[i].into())
                })),
        ))
        .with(section(
            "Combo box",
            Stack::column()
                .with(
                    specimens::combo_box()
                        .on_change(|t| Message::Field("Combo", t))
                        .on_select(|_, t| Message::Field("Combo", t)),
                )
                .with(specimens::combo_box_filled().disabled(true)),
        ))
        .with(section(
            "Edit menu (text field and text view)",
            Stack::row()
                .spacing(spacing::L)
                .with(specimens::edit_menu_field())
                .with(specimens::text_view_filled()),
        ))
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
