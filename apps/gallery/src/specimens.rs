//! The widgets the goldens render, shared with the gallery's pages.

use neue_kit::{
    Align, BevelButton, Button, CheckState, Checkbox, Column, ComboBox, ContextMenu, Label,
    MaterialTile, Menu, MenuItem, Outline, PopUpButton, ProgressBar, PullDownButton, Radio,
    ScrollAxes, ScrollView, Segmented, Shortcut, Slider, SortOrder, Specimen, Spinner, Stack,
    Stepper, Table, TextField, TextView, VirtualList,
};

/// Golden window size for one push button (logical pixels): the button with the fixture's
/// margin, room for its focus ring.
pub const PUSH_BUTTON_SIZE: (u32, u32) = (90, 40);

/// An ordinary push button.
pub fn push_button<M: 'static>() -> Button<M> {
    Button::new("Button")
}

/// The window's default button (Return presses it).
pub fn default_button<M: 'static>() -> Button<M> {
    Button::new("OK").as_default()
}

/// Golden window size for one checkbox or radio button (logical pixels).
pub const TOGGLE_SIZE: (u32, u32) = (120, 40);

/// Golden window size for one bevel or recessed button.
pub const BEVEL_SIZE: (u32, u32) = (110, 40);

/// Golden window size for the segmented control.
pub const SEGMENTED_SIZE: (u32, u32) = (190, 40);

/// An unchecked checkbox.
pub fn checkbox<M: 'static>() -> Checkbox<M> {
    Checkbox::new("Checkbox")
}

/// A checked checkbox.
pub fn checkbox_on<M: 'static>() -> Checkbox<M> {
    checkbox().checked(true)
}

/// A checkbox in the mixed state.
pub fn checkbox_mixed<M: 'static>() -> Checkbox<M> {
    checkbox().state(CheckState::Mixed)
}

/// An unselected radio button.
pub fn radio<M: 'static>() -> Radio<M> {
    Radio::new("Radio")
}

/// A selected radio button.
pub fn radio_on<M: 'static>() -> Radio<M> {
    radio().selected(true)
}

/// A bevel button that is off.
pub fn bevel_button<M: 'static>() -> BevelButton<M> {
    BevelButton::new("Bevel")
}

/// A bevel button toggled on.
pub fn bevel_button_on<M: 'static>() -> BevelButton<M> {
    bevel_button().on(true)
}

/// A recessed button that is off.
pub fn recessed_button<M: 'static>() -> BevelButton<M> {
    BevelButton::recessed("Recessed")
}

/// A recessed button toggled on.
pub fn recessed_button_on<M: 'static>() -> BevelButton<M> {
    recessed_button().on(true)
}

/// A segmented control with `selected` chosen.
pub fn segmented<M: 'static>(selected: usize) -> Segmented<M> {
    Segmented::new(["One", "Two", "Three"]).selected(selected)
}

/// Golden window size for the slider (the default width plus the fixture's margin).
pub const SLIDER_SIZE: (u32, u32) = (150, 40);

/// Golden window size for the stepper.
pub const STEPPER_SIZE: (u32, u32) = (50, 50);

/// Golden window size for the progress bar.
pub const PROGRESS_SIZE: (u32, u32) = (180, 40);

/// Golden window size for the spinner.
pub const SPINNER_SIZE: (u32, u32) = (50, 40);

/// A slider over 0 to 1, at `value`.
pub fn slider<M: 'static>(value: f64) -> Slider<M> {
    Slider::new().value(value)
}

/// A stepper from 0 to 10, at 5.
pub fn stepper<M: 'static>() -> Stepper<M> {
    Stepper::new().range(0.0..=10.0).value(5.0)
}

/// A progress bar filled to `value` (0 to 1).
pub fn progress_bar(value: f64) -> ProgressBar {
    ProgressBar::new().value(value)
}

/// A progress bar of unknown length: the barber pole.
pub fn barber_pole() -> ProgressBar {
    ProgressBar::new()
}

/// A turning spinner.
pub fn spinner() -> Spinner {
    Spinner::new()
}

/// Golden window size for one text, search or secure field (the default width plus the
/// fixture's margin, room for the focus ring).
pub const FIELD_SIZE: (u32, u32) = (220, 40);

/// An empty text field with a placeholder.
pub fn text_field<M: 'static>() -> TextField<M> {
    TextField::new().placeholder("Name")
}

/// A text field holding text.
pub fn text_field_filled<M: 'static>() -> TextField<M> {
    text_field().text("Mountain pass")
}

/// An empty search field (placeholder "Search").
pub fn search_field<M: 'static>() -> TextField<M> {
    TextField::search()
}

/// A search field holding text: the cancel button shows.
pub fn search_field_filled<M: 'static>() -> TextField<M> {
    search_field().text("granite")
}

/// An empty secure field with a placeholder.
pub fn secure_field<M: 'static>() -> TextField<M> {
    TextField::secure().placeholder("Password")
}

/// Golden window size for a text view (the default 300 × 120 plus the fixture's margin,
/// room for the focus ring).
pub const TEXT_VIEW_SIZE: (u32, u32) = (320, 140);

/// A paragraph that wraps and, in a text view of the default size, runs past the bottom.
const PARAGRAPHS: &str = "Mountain passes were closed from November to April, so the \
    post came in by sled.\n\nThe second paragraph starts here. It is long enough to wrap \
    onto a second and third line, and the third paragraph below makes the text taller \
    than the view.\n\nThird paragraph.\nFourth line.\nFifth line.";

/// An empty, editable text view.
pub fn text_view<M: 'static>() -> TextView<M> {
    TextView::new()
}

/// An editable text view holding more text than it shows.
pub fn text_view_filled<M: 'static>() -> TextView<M> {
    TextView::new().text(PARAGRAPHS)
}

/// A read-only text view: its text can be selected and copied.
pub fn text_view_read_only<M: 'static>() -> TextView<M> {
    TextView::new().editable(false).text(PARAGRAPHS)
}

/// A material over a stand-in for a blurred desktop: opaque, or translucent if
/// `translucent`. Sized by the tile.
pub fn material(tile: MaterialTile, translucent: bool) -> Specimen {
    let specimen = Specimen::new(tile);
    if translucent {
        specimen.translucent()
    } else {
        specimen
    }
}

/// Golden window size for `tile`: the tile with the fixture's margin.
pub fn material_window_size(tile: MaterialTile) -> (u32, u32) {
    let (width, height) = tile.size();
    // Tile sizes are small positive whole numbers.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "tile sizes are small"
    )]
    let size = ((width.ceil() as u32) + 16, (height.ceil() as u32) + 16);
    size
}

/// Golden window size for a scroll view or a virtual list: 160 × 100 with the fixture's
/// margin, room for the focus ring.
pub const SCROLL_SIZE: (u32, u32) = (176, 116);

/// A scroll view over a column of `lines` numbered lines.
pub fn scroll_view<M: 'static>(lines: usize) -> ScrollView<M> {
    let mut column = Stack::column().padding(neue_kit::tokens::metrics::spacing::XS);
    for i in 1..=lines {
        column = column.with(Label::new(format!("Line {i} of the scroll view")));
    }
    ScrollView::new(column).width(160.0).height(100.0)
}

/// A scroll view over a grid wider and taller than itself, scrolling both ways.
pub fn scroll_view_both<M: 'static>() -> ScrollView<M> {
    let mut grid = Stack::column().padding(neue_kit::tokens::metrics::spacing::XS);
    for row in 1..=12 {
        let mut line = Stack::row().spacing(neue_kit::tokens::metrics::spacing::L);
        for column in 1..=6 {
            line = line.with(Label::new(format!("R{row}C{column}")));
        }
        grid = grid.with(line);
    }
    ScrollView::new(grid)
        .axes(ScrollAxes::Both)
        .width(160.0)
        .height(100.0)
}

/// A virtual list of `count` numbered rows, 20 px high.
pub fn virtual_list<M: 'static>(count: usize) -> VirtualList<M> {
    VirtualList::new(count, 20.0, |i| {
        Stack::row()
            .padding(neue_kit::tokens::metrics::spacing::XXS)
            .with(Label::new(format!("Row {}", i + 1)))
    })
    .width(160.0)
    .height(100.0)
}

/// Golden window size for a table: 240 × 120 with the fixture's margin, room for the
/// focus ring.
pub const TABLE_SIZE: (u32, u32) = (256, 136);

/// A file in the tables' sample data: name, size in kilobytes, kind.
pub type SampleFile = (&'static str, u32, &'static str);

/// The tables' sample files, by name.
pub const FILES: [SampleFile; 10] = [
    ("Annual Report.pdf", 2_340, "PDF document"),
    ("Beach Photo.jpg", 1_820, "JPEG image"),
    ("Budget 2026.ods", 48, "Spreadsheet"),
    ("Field Notes.txt", 4, "Plain text"),
    ("Holiday Video.mp4", 812_000, "MPEG-4 video"),
    ("Invoice 0412.pdf", 96, "PDF document"),
    ("Letter to Sam.odt", 22, "Text document"),
    ("Mixtape.flac", 41_200, "FLAC audio"),
    ("Recipes.md", 9, "Markdown"),
    ("Screenshot.png", 640, "PNG image"),
];

/// The columns of a table of [`SampleFile`]s: name, size (right-aligned), kind; all
/// sortable.
pub fn file_columns() -> Vec<Column> {
    vec![
        Column::new("Name")
            .width(100.0)
            .min_width(60.0)
            .sortable(true),
        Column::new("Size")
            .width(70.0)
            .min_width(40.0)
            .align(Align::End)
            .sortable(true),
        Column::new("Kind").width(60.0).sortable(true),
    ]
}

/// A file size for people: kilobytes, megabytes or gigabytes.
pub fn file_size(kb: u32) -> String {
    let kb = f64::from(kb);
    if kb < 1_000.0 {
        format!("{kb} KB")
    } else if kb < 1_000_000.0 {
        format!("{:.1} MB", kb / 1_000.0)
    } else {
        format!("{:.1} GB", kb / 1_000_000.0)
    }
}

/// The text of `file`'s cell in `column` of [`file_columns`].
pub fn file_cell(file: &SampleFile, column: usize) -> String {
    match column {
        0 => file.0.to_owned(),
        1 => file_size(file.1),
        _ => file.2.to_owned(),
    }
}

/// A table of the sample files: striped, sorted by name, the second row selected.
pub fn table<M: 'static>() -> Table<M> {
    Table::new(file_columns(), FILES.len(), |row, column| {
        file_cell(&FILES[row], column)
    })
    .alternating_rows(true)
    .sorted(0, SortOrder::Ascending)
    .selected([1])
    .width(240.0)
    .height(120.0)
}

/// Golden window size for an outline: the table's width, room for six rows.
pub const OUTLINE_SIZE: (u32, u32) = (256, 156);

/// A node of the outline's sample folders: id, parent (0 for the top level), name, kind.
pub type SampleNode = (u32, u32, &'static str, &'static str);

/// The outlines' sample folders and files. Ids are stable; a node's children are those
/// naming it as parent.
pub const NODES: [SampleNode; 13] = [
    (1, 0, "Documents", "Folder"),
    (11, 1, "Work", "Folder"),
    (111, 11, "Annual Report.pdf", "PDF document"),
    (112, 11, "Budget 2026.ods", "Spreadsheet"),
    (12, 1, "Letters", "Folder"),
    (121, 12, "Letter to Sam.odt", "Text document"),
    (2, 0, "Pictures", "Folder"),
    (21, 2, "Beach Photo.jpg", "JPEG image"),
    (22, 2, "Screenshot.png", "PNG image"),
    (3, 0, "Music", "Folder"),
    (31, 3, "Mixtape.flac", "FLAC audio"),
    (4, 0, "Field Notes.txt", "Plain text"),
    (5, 0, "Recipes.md", "Markdown"),
];

/// The ids of the nodes inside `parent` (`None`: the top level), in order.
pub fn node_children(parent: Option<&u32>) -> Vec<u32> {
    let parent = parent.copied().unwrap_or(0);
    NODES
        .iter()
        .filter(|n| n.1 == parent)
        .map(|n| n.0)
        .collect()
}

/// The text of node `id`'s cell in `column` of [`node_columns`].
pub fn node_cell(id: u32, column: usize) -> String {
    NODES
        .iter()
        .find(|n| n.0 == id)
        .map(|n| if column == 0 { n.2 } else { n.3 })
        .unwrap_or_default()
        .to_owned()
}

/// The columns of an outline of [`NODES`]: name and kind.
pub fn node_columns() -> Vec<Column> {
    vec![
        Column::new("Name").width(150.0).min_width(80.0),
        Column::new("Kind").width(80.0),
    ]
}

/// An outline of the sample folders, all closed.
pub fn outline<M: 'static>() -> Outline<M, u32> {
    Outline::new(node_columns(), node_children, |&id, column| {
        node_cell(id, column)
    })
    .width(240.0)
    .height(140.0)
}

/// The sample outline with Documents open.
pub fn outline_expanded<M: 'static>() -> Outline<M, u32> {
    outline().expanded([1])
}

/// The sample outline with Documents and Work open: two levels deep.
pub fn outline_nested<M: 'static>() -> Outline<M, u32> {
    outline().expanded([1, 11])
}

/// The nested outline, striped, with the open folder Work selected (its triangle turns
/// white with focus).
pub fn outline_selected<M: 'static>() -> Outline<M, u32> {
    outline_nested().alternating_rows(true).selected([11])
}

/// Golden window size for an open context menu (logical pixels).
pub const MENU_SIZE: (u32, u32) = (250, 190);

/// Golden window size for a context menu with its submenu open.
pub const SUBMENU_SIZE: (u32, u32) = (370, 190);

/// The command-key shortcut for `key`, as the gallery shows it: Cmd on macOS hosts, Ctrl
/// elsewhere.
pub fn command(key: &'static str) -> Shortcut {
    Shortcut::command(key)
}

/// Ctrl and `key`: the same on every host, so goldens match on macOS and Linux.
pub fn ctrl(key: &'static str) -> Shortcut {
    Shortcut::new(key).ctrl()
}

/// The sample menu: Cut and Copy with shortcuts, a disabled Paste, a separator, a checked
/// item, a submenu with a radio group, and Delete. `send(label)` is each item's message;
/// `shortcut` makes the command shortcuts ([`command`] or [`ctrl`]).
pub fn sample_menu<M: Clone + 'static>(
    send: impl Fn(&'static str) -> M,
    shortcut: fn(&'static str) -> Shortcut,
) -> Menu<M> {
    let item = |label: &'static str| MenuItem::new(label).on_choose(send(label));
    Menu::new()
        .with(item("Cut").shortcut(shortcut("X")))
        .with(item("Copy").shortcut(shortcut("C")))
        .with(item("Paste").shortcut(shortcut("V")).disabled(true))
        .separator()
        .with(item("Wrap Lines").checked(true))
        .with(MenuItem::submenu(
            "Text Size",
            Menu::new()
                .with(item("Small").radio(false))
                .with(item("Regular").radio(true))
                .with(item("Large").radio(false)),
        ))
        .with(item("Delete").shortcut(Shortcut::new("⌫")))
}

/// A label with the sample menu as its context menu.
pub fn context_menu<M: Clone + 'static>(
    send: impl Fn(&'static str) -> M,
    shortcut: fn(&'static str) -> Shortcut,
) -> ContextMenu<M> {
    ContextMenu::new(
        Label::new("Right-click here").secondary(),
        sample_menu(send, shortcut),
    )
}

/// The sizes the pop-up button's menu is chosen from.
pub const SIZES: [&str; 3] = ["Small", "Medium", "Large"];

/// Golden window size for a pop-up button with room around it for its open menu, which
/// sits over the button (logical pixels).
pub const POPUP_SIZE: (u32, u32) = (180, 130);

/// A pop-up button with [`SIZES`], "Medium" chosen, padded so its open menu fits in the
/// window.
pub fn pop_up_button<M: 'static>() -> Stack<M> {
    Stack::column()
        .padding(40.0)
        .with(PopUpButton::new(SIZES).selected(1))
}

/// Golden window size for a pull-down button and its open menu below it.
pub const PULL_DOWN_SIZE: (u32, u32) = (180, 130);

/// A pull-down button of commands. `send(label)` is each item's message.
pub fn pull_down_button<M: Clone + 'static>(send: impl Fn(&'static str) -> M) -> PullDownButton<M> {
    let item = |label: &'static str| MenuItem::new(label).on_choose(send(label));
    PullDownButton::new(
        "Actions",
        Menu::new()
            .with(item("Rename"))
            .with(item("Duplicate"))
            .separator()
            .with(item("Delete")),
    )
}

/// Golden window size for a combo box and its open menu below it.
pub const COMBO_SIZE: (u32, u32) = (220, 120);

/// An empty combo box suggesting [`SIZES`].
pub fn combo_box<M: 'static>() -> ComboBox<M> {
    ComboBox::new(SIZES).placeholder("Size")
}

/// A combo box holding "Medium".
pub fn combo_box_filled<M: 'static>() -> ComboBox<M> {
    ComboBox::new(SIZES).text("Medium")
}

/// A text field with a few words to cut, copy and select, for the edit menu.
pub fn edit_menu_field<M: 'static>() -> TextField<M> {
    TextField::new().text("Edit me")
}
