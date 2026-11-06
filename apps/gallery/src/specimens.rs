//! The widgets the goldens render, shared with the gallery's pages.

use neue_kit::tokens::Color;
use neue_kit::{
    Align, BevelButton, Button, CheckState, Checkbox, Collection, ColorWell, Column, ComboBox,
    ContextMenu, Date, DatePicker, Entry, Glyph, Icon, IconImage, IconPaint, Item, Label,
    MaterialTile, Menu, MenuItem, Outline, PathControl, PopUpButton, ProgressBar, PullDownButton,
    Radio, ScrollAxes, ScrollView, Segmented, Shortcut, Slider, SortOrder, SourceList, Specimen,
    Spinner, Stack, Stepper, Table, TextField, TextView, VectorIcon, VirtualGrid, VirtualList,
};
use neue_kit::{DisplayMode, Toolbar, ToolbarButton};
use neue_kit::{DividerStyle, Pane, SplitView, TabView, Window};
use neue_kit::{Edge, Popover, Sheet, TextStyle};

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

/// A virtual grid of `count` numbered items in 40 × 24 cells: the columns follow the width.
pub fn virtual_grid<M: 'static>(count: usize) -> VirtualGrid<M> {
    VirtualGrid::new(count, 40.0, 24.0, |i| {
        Stack::row()
            .padding(neue_kit::tokens::metrics::spacing::XXS)
            .with(Label::new((i + 1).to_string()))
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

/// Golden window size for a source list: 176 wide and ten rows high, with the fixture's
/// margin.
pub const SIDEBAR_SIZE: (u32, u32) = (192, 240);

/// A node of the sample sidebar: id, parent (0 for a group), label, icon, badge.
pub type SampleItem = (u32, u32, &'static str, Option<Glyph>, Option<&'static str>);

/// The sidebars' sample groups (Favorites, Devices) and items.
pub const ITEMS: [SampleItem; 10] = [
    (1, 0, "Favorites", None, None),
    (11, 1, "Documents", Some(Glyph::Document), None),
    (12, 1, "Pictures", Some(Glyph::Picture), None),
    (13, 1, "Music", Some(Glyph::Music), None),
    (131, 13, "Albums", None, None),
    (132, 13, "Artists", None, None),
    (14, 1, "Starred", Some(Glyph::Star), Some("12")),
    (2, 0, "Devices", None, None),
    (21, 2, "Disk", Some(Glyph::Folder), None),
    (22, 2, "Recent", Some(Glyph::Clock), Some("3")),
];

/// The ids of the sidebar nodes inside `parent` (`None`: the groups), in order.
pub fn item_children(parent: Option<&u32>) -> Vec<u32> {
    let parent = parent.copied().unwrap_or(0);
    ITEMS
        .iter()
        .filter(|n| n.1 == parent)
        .map(|n| n.0)
        .collect()
}

/// The label of sidebar node `id`.
pub fn item_label(id: u32) -> &'static str {
    ITEMS.iter().find(|n| n.0 == id).map_or("", |n| n.2)
}

/// What a sidebar shows for node `id`.
pub fn item_entry(id: u32) -> Entry {
    let Some(&(_, _, label, icon, badge)) = ITEMS.iter().find(|n| n.0 == id) else {
        return Entry::new("");
    };
    let mut entry = Entry::new(label);
    if let Some(icon) = icon {
        entry = entry.icon(icon);
    }
    if let Some(badge) = badge {
        entry = entry.badge(badge);
    }
    entry
}

/// A source list of the sample groups, nothing selected.
pub fn source_list<M: 'static>() -> SourceList<M, u32> {
    SourceList::new(item_children, |&id| item_entry(id))
        .width(176.0)
        .height(224.0)
}

/// The sample source list with Starred selected (its badge turns white).
pub fn source_list_selected<M: 'static>() -> SourceList<M, u32> {
    source_list().selected(14)
}

/// The sample source list with Music open and the nested item Artists selected.
pub fn source_list_nested<M: 'static>() -> SourceList<M, u32> {
    source_list().expanded([13]).selected(132)
}

/// The sample source list with the Devices group closed.
pub fn source_list_collapsed<M: 'static>() -> SourceList<M, u32> {
    source_list().collapsed([2]).selected(12)
}

/// Golden window size for an open context menu (logical pixels).
pub const MENU_SIZE: (u32, u32) = (250, 190);

/// Golden window size for a context menu with its submenu open.
pub const SUBMENU_SIZE: (u32, u32) = (370, 190);

/// The command-key shortcut for `key`, as the gallery shows it: Super (⌘) on every host.
pub fn command(key: &'static str) -> Shortcut {
    Shortcut::command(key)
}

/// The sample menu: Cut and Copy with shortcuts, a disabled Paste, a separator, a checked
/// item, a submenu with a radio group, and Delete. `send(label)` is each item's message;
/// `shortcut` makes the command shortcuts ([`command`]).
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

/// Golden window size for a row of icons.
pub const ICONS_SIZE: (u32, u32) = (260, 64);

/// Every named glyph, 32 logical pixels square, in a row.
pub fn icon_glyphs<M: 'static>() -> Stack<M> {
    [
        Glyph::Folder,
        Glyph::Document,
        Glyph::Picture,
        Glyph::Music,
        Glyph::Star,
        Glyph::Clock,
    ]
    .into_iter()
    .fold(Stack::row().spacing(8.0), |row, glyph| {
        row.with(IconImage::new(glyph).size(32.0))
    })
}

/// The sample vector icon: a disc with its own colours for light and dark, and a tick in
/// the text colour. Built from SVG path data, as an app's own icon would be.
pub fn sample_icon() -> Icon {
    let disc = IconPaint::Fixed {
        light: Color::from_rgb8(0x4a, 0x90, 0xd9),
        dark: Color::from_rgb8(0x6c, 0xaa, 0xe8),
    };
    VectorIcon::new(16.0, 16.0)
        .and_then(|icon| icon.shape("M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1z", disc))
        .and_then(|icon| {
            icon.shape(
                "M4.6 8.1l1.2-1.2 1.7 1.7 3.2-3.4 1.2 1.2-4.4 4.6z",
                IconPaint::Current { alpha: 0.9 },
            )
        })
        .map(Icon::from)
        .unwrap_or_else(|_| Icon::from(Glyph::Star))
}

/// The sample vector icon at 16, 32 and 48 logical pixels.
pub fn icon_vector<M: 'static>() -> Stack<M> {
    [16.0, 32.0, 48.0]
        .into_iter()
        .fold(Stack::row().spacing(8.0), |row, size| {
            row.with(IconImage::new(sample_icon()).size(size).label("Done"))
        })
}

/// Golden window size for a collection of six items in three columns and two rows, with
/// the fixture's margin.
pub const COLLECTION_SIZE: (u32, u32) = (240, 184);

/// The sample collection's items: id, label, glyph. The last label is too long for its cell.
pub const COLLECTION_ITEMS: [(u32, &str, Glyph); 6] = [
    (1, "Documents", Glyph::Document),
    (2, "Pictures", Glyph::Picture),
    (3, "Music", Glyph::Music),
    (4, "Starred", Glyph::Star),
    (5, "Recent", Glyph::Clock),
    (6, "Quarterly report, final", Glyph::Folder),
];

/// What the sample collection shows for item `id`: `COLLECTION_ITEMS` for 1 to 6, then
/// numbered copies of them.
pub fn collection_item(id: u32) -> Item {
    let (_, label, glyph) = COLLECTION_ITEMS[(id as usize - 1) % COLLECTION_ITEMS.len()];
    if id as usize <= COLLECTION_ITEMS.len() {
        Item::new(label).icon(glyph)
    } else {
        Item::new(format!("{label} {id}")).icon(glyph)
    }
}

/// The label of the sample collection's item `id`.
pub fn collection_label(id: u32) -> String {
    let (_, label, _) = COLLECTION_ITEMS[(id as usize - 1) % COLLECTION_ITEMS.len()];
    if id as usize <= COLLECTION_ITEMS.len() {
        label.to_owned()
    } else {
        format!("{label} {id}")
    }
}

/// A collection of the six sample items, nothing selected.
pub fn collection<M: 'static>() -> Collection<M, u32> {
    Collection::new(1..=6, |&id| collection_item(id))
        .icon_size(32.0)
        .width(224.0)
        .height(168.0)
}

/// The sample collection with Pictures and the item with the long label selected (grey at
/// rest, the accent with focus).
pub fn collection_selected<M: 'static>() -> Collection<M, u32> {
    collection().multiple(true).selected([2, 6])
}

/// A collection of `count` items with icons `icon_size` wide, for the gallery.
pub fn collection_many<M: 'static>(count: u32, icon_size: f64) -> Collection<M, u32> {
    Collection::new(1..=count, |&id| collection_item(id))
        .icon_size(icon_size)
        .multiple(true)
}

/// Golden window size for a 200 × 80 split view with the fixture's margin.
pub const SPLIT_SIZE: (u32, u32) = (216, 96);

/// A pane's content: a label with some room around it.
fn pane_label<M: 'static>(text: &str) -> Stack<M> {
    Stack::column().padding(8.0).with(Label::new(text))
}

/// A 200 × 80 split view of a sidebar (70 long, between 50 and 120, holding its size,
/// collapsible) and the content, with `style` dividers. `vertical` stacks the panes.
pub fn split_view<M: 'static>(style: DividerStyle, vertical: bool) -> SplitView<M> {
    let view = if vertical {
        SplitView::vertical()
    } else {
        SplitView::horizontal()
    };
    let sidebar = Pane::new()
        .size(if vertical { 36.0 } else { 70.0 })
        .min(if vertical { 24.0 } else { 50.0 })
        .max(120.0)
        .holds_size()
        .collapsible();
    view.divider(style)
        .with_pane(pane_label("Sidebar"), sidebar)
        .with_pane(pane_label("Content"), Pane::new().min(24.0))
        .width(200.0)
        .height(80.0)
}

/// The same split view with its sidebar collapsed.
pub fn split_view_collapsed<M: 'static>(style: DividerStyle) -> SplitView<M> {
    SplitView::horizontal()
        .divider(style)
        .with_pane(
            pane_label("Sidebar"),
            Pane::new()
                .size(70.0)
                .min(50.0)
                .collapsible()
                .collapsed(true),
        )
        .with_pane(pane_label("Content"), Pane::new().min(24.0))
        .width(200.0)
        .height(80.0)
}

/// Golden window size for a 260 × 90 tab view with the fixture's margin.
pub const TABS_SIZE: (u32, u32) = (276, 106);

/// A 260 × 90 tab view of "General", "Advanced" and a disabled "Hidden" tab, tab `selected`
/// shown.
pub fn tab_view<M: 'static>(selected: usize) -> TabView<M> {
    TabView::new()
        .with_tab("General", pane_label("General settings"))
        .with_tab("Advanced", pane_label("Advanced settings"))
        .with_disabled_tab("Hidden", pane_label("Hidden"))
        .selected(selected)
        .width(260.0)
        .height(90.0)
}

/// Golden window size for a window with its own title bar.
pub const CHROME_SIZE: (u32, u32) = (240, 60);
/// Golden window size for a window with a unified toolbar.
pub const TOOLBAR_SIZE: (u32, u32) = (300, 90);

/// A window that draws its own title bar over some content.
pub fn window_chrome<M: 'static>() -> Window<M> {
    Window::new("Untitled", CHROME_SIZE, pane_label("Content")).chrome()
}

/// A window with a unified title bar and toolbar: two buttons and a label.
pub fn window_toolbar<M: 'static>() -> Window<M> {
    let toolbar = Stack::row()
        .spacing(8.0)
        .align(Align::Center)
        .with(Button::new("Back"))
        .with(Button::new("Forward"))
        .with(Label::new("Documents"));
    Window::new("Documents", TOOLBAR_SIZE, pane_label("Content")).toolbar(toolbar)
}

/// Golden window size for a window whose toolbar holds every kind of item.
pub const TOOLBAR_ITEMS_SIZE: (u32, u32) = (440, 96);

/// Where the first toolbar button's middle is in a [`TOOLBAR_ITEMS_SIZE`] window (logical
/// pixels), for pressing it.
pub const TOOLBAR_FIRST_BUTTON: (f64, f64) = (34.0, 46.0);

/// A toolbar with every kind of item: three buttons (the last disabled), a separator, a
/// segmented control, a flexible space and a search field.
pub fn toolbar_items<M: Clone + 'static>(mode: DisplayMode) -> Toolbar<M> {
    Toolbar::new()
        .with(ToolbarButton::new("Open", Glyph::Folder))
        .with(ToolbarButton::new("Favourite", Glyph::Star))
        .with(ToolbarButton::new("Share", Glyph::Document).disabled(true))
        .separator()
        .with(Segmented::new(["List", "Icons"]))
        .flexible_space()
        .with(TextField::search().width(120.0))
        .display_mode(mode)
}

/// A window with a unified title bar and a toolbar of every kind of item.
pub fn window_toolbar_items<M: Clone + 'static>(mode: DisplayMode) -> Window<M> {
    Window::new("Documents", TOOLBAR_ITEMS_SIZE, pane_label("Content")).toolbar(toolbar_items(mode))
}

/// Room around [`popover_anchor`]'s button: enough for a popover on any side.
const POPOVER_ROOM: f64 = 150.0;

/// Golden window size for a button with a popover on one of its sides: the button in the
/// middle, with [`POPOVER_ROOM`] around it.
pub const POPOVER_SIZE: (u32, u32) = (386, 338);

/// What the gallery's popovers show: a title, a line of text and a button sending `done`.
pub fn popover_content<M: Clone + 'static>(done: Option<M>) -> Stack<M> {
    let button = Button::new("Done");
    let button = match done {
        Some(message) => button.on_press(message),
        None => button,
    };
    Stack::column()
        .spacing(6.0)
        .with(Label::new("Popover").style(TextStyle::Headline))
        .with(
            Label::new("Points at its button")
                .style(TextStyle::Small)
                .secondary(),
        )
        .with(button)
}

/// A popover of [`popover_content`] on side `edge` of its anchor.
pub fn popover<M: Clone + 'static>(edge: Edge) -> Popover<M> {
    Popover::new(popover_content(None)).edge(edge)
}

/// A button with room around it for a popover on any side (in a [`POPOVER_SIZE`]
/// window).
pub fn popover_anchor<M: Clone + 'static>() -> Stack<M> {
    Stack::column()
        .padding(POPOVER_ROOM)
        .with(Button::new("Show"))
}

/// Size of the gallery's sheet.
pub const SHEET_SIZE: (u32, u32) = (320, 128);

/// Golden window size for a window with the gallery's sheet drawn in it, under its title
/// bar.
pub const SHEET_PARENT_SIZE: (u32, u32) = (420, 220);

/// The gallery's sheet: a question and a line of text, with Save (Return) and Cancel
/// (Escape).
pub fn sheet<M: Clone + 'static>(save: M, cancel: M) -> Sheet<M> {
    let content = Stack::column()
        .spacing(6.0)
        .with(Label::new("Save changes to “Notes”?").style(TextStyle::Headline))
        .with(
            Label::new("Your changes are lost if you don’t save them.")
                .style(TextStyle::Small)
                .secondary(),
        );
    Sheet::new(SHEET_SIZE, content)
        .title("Save")
        .default_button("Save", save)
        .cancel_button("Cancel", cancel)
}

/// Size of the gallery's HUD window.
pub const HUD_SIZE: (u32, u32) = (240, 120);

/// The gallery's HUD window: an inspector with two lines of light text on the dark panel.
/// Open it with a parent to make it a utility window.
pub fn hud_window<M: 'static>() -> Window<M> {
    let content = Stack::column()
        .padding(neue_kit::tokens::metrics::spacing::M)
        .spacing(neue_kit::tokens::metrics::spacing::XS)
        .with(Label::new("Width  240").on_hud())
        .with(Label::new("Height  120").on_hud())
        .with(
            Label::new("Drag the bar to move it")
                .style(TextStyle::Small)
                .on_hud()
                .secondary(),
        );
    Window::new("Inspector", HUD_SIZE, content).hud()
}

/// A window with a title bar for [`sheet`] to hang from.
pub fn sheet_parent<M: 'static>() -> Window<M> {
    Window::new("Notes", SHEET_PARENT_SIZE, pane_label("Content")).chrome()
}

/// Golden window size for the path control.
pub const PATH_SIZE: (u32, u32) = (250, 40);

/// The levels of the path control's specimen, root first.
pub const PATH: [&str; 3] = ["Macintosh HD", "Users", "sophie"];

/// A path control of [`PATH`].
pub fn path_control<M: 'static>() -> PathControl<M> {
    PathControl::new(PATH)
}

/// Golden window size for the colour well.
pub const COLOR_WELL_SIZE: (u32, u32) = (90, 40);

/// The colour of the gallery's wells: a translucent orange, so the checkerboard shows.
pub const WELL_COLOR: Color = Color::new([1.0, 0.55, 0.1, 0.6]);

/// A colour well showing [`WELL_COLOR`].
pub fn color_well<M: 'static>() -> ColorWell<M> {
    ColorWell::new(WELL_COLOR).label("Colour")
}

/// `color` as hex: `#RRGGBB`, or `#RRGGBBAA` when it isn't opaque.
pub fn hex(color: Color) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel clamped to 0..255 and rounded"
    )]
    let [r, g, b, a] = color
        .components
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// Room around [`color_well_anchor`]'s well: enough for its panel below it.
const COLOR_PANEL_ROOM: f64 = 130.0;

/// Golden window size for a colour well with its panel open below it.
pub const COLOR_PANEL_SIZE: (u32, u32) = (306, 440);

/// A colour well with room around it for its panel (in a [`COLOR_PANEL_SIZE`] window).
pub fn color_well_anchor<M: 'static>() -> Stack<M> {
    Stack::column().padding(COLOR_PANEL_ROOM).with(color_well())
}

/// Golden window size for the textual date picker.
pub const DATE_SIZE: (u32, u32) = (150, 40);

/// Golden window size for the calendar.
pub const CALENDAR_SIZE: (u32, u32) = (220, 216);

/// The date the specimens show.
pub const DATE: Date = match Date::new(2026, 11, 5) {
    Some(date) => date,
    None => panic!("a real date"),
};

/// The day the calendar specimens call today (not the clock's, so goldens stay the same).
pub const TODAY: Date = match Date::new(2026, 11, 20) {
    Some(date) => date,
    None => panic!("a real date"),
};

/// A textual date picker showing [`DATE`].
pub fn date_picker<M: 'static>() -> DatePicker<M> {
    DatePicker::new(DATE).label("Date")
}

/// A calendar with [`DATE`] selected and [`TODAY`] ringed.
pub fn calendar<M: 'static>() -> neue_kit::CalendarPicker<M> {
    DatePicker::calendar(DATE).today(TODAY).label("Calendar")
}

/// Golden window size for the shell's menu bar, with room for its logo menu.
pub const SHELL_BAR_SIZE: (u32, u32) = (460, 240);

/// What the specimen bar shows: a fixed app name and time, so goldens don't change; a
/// battery on its way up, Wi-Fi joined to Home (with Cafe open and Library protected in
/// range, and an Ethernet port with a cable) and a connected Bluetooth mouse.
pub fn shell_bar_content(app_name: &str) -> neue_shell::bar::BarContent {
    use neue_shell::bar::{
        BatteryView, BluetoothView, DeviceView, NetworkView, WifiNetworkView, WifiView, WiredView,
    };
    let network = |ssid: &str, strength, secured, saved, active| WifiNetworkView {
        ssid: ssid.to_owned(),
        strength,
        secured,
        saved,
        active,
    };
    let device = |name: &str, connected| DeviceView {
        name: name.to_owned(),
        connected,
    };
    neue_shell::bar::BarContent {
        app_name: app_name.to_owned(),
        time: "Thu 09:41".to_owned(),
        full_date: "Thursday, 5 November 2026".to_owned(),
        battery: Some(BatteryView {
            percent: 73,
            charging: true,
            on_ac: true,
            time_left: Some(std::time::Duration::from_secs(45 * 60)),
        }),
        network: Some(NetworkView {
            wifi_enabled: true,
            wired: vec![WiredView {
                name: "enp0s1".to_owned(),
                carrier: true,
                connected: false,
                connecting: false,
            }],
            wifi: Some(WifiView {
                connected: true,
                connecting: false,
                networks: vec![
                    network("Home", 82, true, true, true),
                    network("Cafe", 55, false, false, false),
                    network("Library", 20, true, false, false),
                ],
            }),
        }),
        bluetooth: Some(BluetoothView {
            powered: true,
            devices: vec![device("Trackpad", true), device("Headphones", false)],
        }),
        tray: Vec::new(),
    }
}

/// Golden window size for the menu extras' icons in each state.
pub const SHELL_EXTRAS_SIZE: (u32, u32) = (300, 90);

/// Three bars of menu extras: batteries (full, charging, half, low, empty), network icons
/// (wired, disconnected, Wi-Fi with three, two, one and no arcs lit, Wi-Fi off) and
/// Bluetooth icons (off, on, connected).
pub fn shell_extras<M: 'static>() -> Stack<M> {
    use neue_kit::desktop::{Backdrop, ExtraIcon, MenuBar, MenuTitle, NetworkIcon};
    let network =
        |icon| MenuTitle::icon(ExtraIcon::Network(icon), "Network", neue_kit::Menu::new());
    let battery = |level, charging| {
        MenuTitle::icon(
            ExtraIcon::Battery { level, charging },
            "Battery",
            neue_kit::Menu::new(),
        )
    };
    let bluetooth = |powered, connected| {
        MenuTitle::icon(
            ExtraIcon::Bluetooth { powered, connected },
            "Bluetooth",
            neue_kit::Menu::new(),
        )
    };
    Stack::column()
        .align(Align::Fill)
        .with(
            MenuBar::new()
                .backdrop(Backdrop::Opaque)
                .with_extra(battery(1.0, false))
                .with_extra(battery(0.7, true))
                .with_extra(battery(0.5, false))
                .with_extra(battery(0.15, false))
                .with_extra(battery(0.0, false)),
        )
        .with(
            MenuBar::new()
                .backdrop(Backdrop::Opaque)
                .with_extra(network(NetworkIcon::Wired))
                .with_extra(network(NetworkIcon::Disconnected))
                .with_extra(network(NetworkIcon::Wifi { level: 3 }))
                .with_extra(network(NetworkIcon::Wifi { level: 2 }))
                .with_extra(network(NetworkIcon::Wifi { level: 1 }))
                .with_extra(network(NetworkIcon::Wifi { level: 0 }))
                .with_extra(network(NetworkIcon::WifiOff)),
        )
        .with(
            MenuBar::new()
                .backdrop(Backdrop::Opaque)
                .with_extra(bluetooth(false, false))
                .with_extra(bluetooth(true, false))
                .with_extra(bluetooth(true, true)),
        )
}

/// The shell's menu bar across the top, naming `app_name`; the logo menu sends
/// `send(command)`, the network menu `network(command)` and the Bluetooth menu
/// `bluetooth(command)`.
pub fn shell_bar<M: Clone + 'static>(
    app_name: &str,
    send: impl Fn(neue_shell::bar::SystemCommand) -> M + Clone + 'static,
    network: impl Fn(neue_shell::bar::NetworkCommand) -> M + Clone + 'static,
    bluetooth: impl Fn(neue_shell::bar::BluetoothCommand) -> M + Clone + 'static,
) -> Stack<M> {
    let mut handles = neue_shell::bar::BarHandles::default();
    // The app's own menus, as the global menu shows them (task 3.7a).
    let menus = ["File", "Edit", "View"]
        .into_iter()
        .map(|title| {
            neue_kit::desktop::MenuTitle::new(
                title,
                neue_kit::Menu::new().with(neue_kit::MenuItem::new("Close").disabled(true)),
            )
        })
        .collect();
    Stack::column()
        .align(Align::Fill)
        .with(neue_shell::bar::menu_bar(
            &shell_bar_content(app_name),
            menus,
            send,
            network,
            bluetooth,
            |_| neue_shell::bar::TrayActions::menu(neue_kit::Menu::new()),
            &mut handles,
            // Inside a gallery window, nothing is blurred behind the bar.
            neue_kit::desktop::Backdrop::Opaque,
        ))
}

/// Golden window size for the tray specimens.
pub const SHELL_TRAY_SIZE: (u32, u32) = (420, 100);

/// A round app icon of `rgb`, `side` pixels wide, with a soft edge and a lighter top: made
/// by code, so the goldens depend on no file. Drawn at 24 px it is resampled down to the
/// tray's 16, as an app's picture is.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "every value is clamped to 0..=255 first"
)]
fn tray_picture(rgb: [u8; 3], side: u32) -> neue_kit::Icon {
    let mut rgba = Vec::new();
    let r = f64::from(side) / 2.0;
    for y in 0..side {
        for x in 0..side {
            let (dx, dy) = (f64::from(x) + 0.5 - r, f64::from(y) + 0.5 - r);
            let coverage = (r - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            let lift = (1.0 - f64::from(y) / f64::from(side)) * 0.35;
            for c in rgb {
                rgba.push((f64::from(c) + (255.0 - f64::from(c)) * lift).clamp(0.0, 255.0) as u8);
            }
            rgba.push((coverage * 255.0).round() as u8);
        }
    }
    match neue_kit::desktop::ExtraIcon::pixmap(side, side, &rgba) {
        Some(neue_kit::desktop::ExtraIcon::Image(icon)) => icon,
        _ => unreachable!("a square of side pixels is a valid picture"),
    }
}

/// The tray's items in each state: a chat app (active), a sync app (needs attention: its
/// attention icon shows), an idle one (passive: hidden), one whose icon couldn't be found
/// (a placeholder shows) and a monochrome glyph, each with a tooltip.
pub fn shell_tray_items() -> Vec<neue_shell::bar::TrayItemView> {
    use neue_shell::bar::{TrayItemView, TrayStatus};
    let item = |id: &str, status, icon: Option<neue_kit::Icon>, attention| TrayItemView {
        id: id.to_owned(),
        title: id.to_owned(),
        status,
        icon,
        attention_icon: attention,
        tooltip: format!("{id}: all good"),
    };
    vec![
        item(
            "Chat",
            TrayStatus::Active,
            Some(tray_picture([52, 120, 246], 24)),
            None,
        ),
        item(
            "Sync",
            TrayStatus::NeedsAttention,
            Some(tray_picture([60, 170, 90], 24)),
            Some(tray_picture([240, 140, 30], 24)),
        ),
        item(
            "Idle",
            TrayStatus::Passive,
            Some(tray_picture([150, 150, 150], 24)),
            None,
        ),
        item("Lost", TrayStatus::Active, None, None),
        item(
            "Glyph",
            TrayStatus::Active,
            Some(neue_kit::Icon::from(neue_kit::Glyph::Folder)),
            None,
        ),
    ]
}

/// Two menu bars with the tray: the items above, left of the clock (each with a menu of
/// one item), and the same on a bar too narrow for all of them, where those that don't
/// fit before the titles on the left are left out. No menu item sends anything but
/// `idle`.
pub fn shell_tray<M: Clone + 'static>(idle: M) -> Stack<M> {
    use neue_shell::bar::{BarContent, BarHandles};
    let bar =
        |app_name: &str, titles: &[&str], tray| {
            let content = BarContent {
                app_name: app_name.to_owned(),
                time: "09:41".to_owned(),
                full_date: String::new(),
                battery: None,
                network: None,
                bluetooth: None,
                tray,
            };
            let menus = titles
                .iter()
                .map(|t| neue_kit::desktop::MenuTitle::new(*t, neue_kit::Menu::new()))
                .collect();
            neue_shell::bar::menu_bar(
                &content,
                menus,
                {
                    let idle = idle.clone();
                    move |_| idle.clone()
                },
                {
                    let idle = idle.clone();
                    move |_| idle.clone()
                },
                {
                    let idle = idle.clone();
                    move |_| idle.clone()
                },
                |item| {
                    neue_shell::bar::TrayActions::menu(neue_kit::Menu::new().with(
                        neue_kit::MenuItem::new(format!("Open {}", item.title)).disabled(true),
                    ))
                },
                &mut BarHandles::default(),
                neue_kit::desktop::Backdrop::Opaque,
            )
            // Room for the tray: a shell's bar is as wide as its output.
            .min_width(380.0)
        };
    let mut crowded = shell_tray_items();
    crowded.extend(shell_tray_items());
    Stack::column()
        .align(Align::Fill)
        .with(bar("Gallery", &[], shell_tray_items()))
        .with(bar("Gallery", &["File", "Edit", "View"], crowded))
}

/// The desktop background specimens' size: a 16:10 output, small.
const DESKTOP_SPECIMEN: (f64, f64) = (320.0, 200.0);

/// Golden window size for them: the specimen and the window's padding.
pub const SHELL_DESKTOP_SIZE: (u32, u32) = (336, 216);

/// The built-in desktop picture.
pub fn shell_desktop() -> neue_kit::desktop::Desktop {
    neue_kit::desktop::Desktop::new().size(DESKTOP_SPECIMEN.0, DESKTOP_SPECIMEN.1)
}

/// A stand-in for a photo: 96 × 48 pixels, a blue-to-orange diagonal with a white border,
/// so every edge and the proportions are visible under each [`Fit`].
#[allow(
    clippy::expect_used,
    reason = "the sides are the constants the pixels were generated for"
)]
pub fn demo_picture() -> neue_kit::desktop::Picture {
    let (w, h) = (96_u32, 48_u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let t = f64::from(x + y) / f64::from(w + h - 2);
            let border = x < 2 || y < 2 || x >= w - 2 || y >= h - 2;
            let channel = |from: f64, to: f64| (255.0 * (from + (to - from) * t)).round() as u8;
            rgba.extend(if border {
                [255, 255, 255, 255]
            } else {
                [
                    channel(0.1, 0.95),
                    channel(0.3, 0.55),
                    channel(0.8, 0.15),
                    255,
                ]
            });
        }
    }
    neue_kit::desktop::Picture::from_rgba8(w, h, &rgba).expect("the sides match the pixels")
}

/// The desktop background showing [`demo_picture`] under `fit`.
pub fn shell_desktop_photo(fit: neue_kit::desktop::Fit) -> neue_kit::desktop::Desktop {
    shell_desktop().photo(demo_picture(), fit)
}
