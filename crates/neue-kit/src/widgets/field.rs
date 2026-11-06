//! Text, search and secure fields: one line of editable text on parley's `PlainEditor`, in
//! the Aqua well from `neue_paint::materials::text_field`.
//!
//! The editing code follows Masonry's `TextArea` (MIT OR Apache-2.0), which neue-kit
//! can't use (ADR 0002 §4), cut down to a single line that scrolls sideways to keep the
//! caret in view. A secure field keeps the real text to itself and gives the editor one
//! bullet per character, so only bullets are laid out, drawn and shown to assistive
//! technology (ADR 0042).

use std::any::TypeId;
use std::time::Duration;

use masonry_core::accesskit::{Action, ActionData, HasPopup, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, CursorIcon, EventCtx, Ime, LayoutCtx,
    MeasureCtx, MutateCtx, NewWidget, PaintCtx, PlainEditorAccessibility, PointerButton,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, QueryCtx,
    RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut,
    render_text, set_accesskit_brush_properties,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Point, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use masonry_core::parley::editing::{Generation, PlainEditorDriver};
use masonry_core::parley::{FontFamily, PlainEditor, StyleProperty};
use masonry_core::util::bounding_box_to_rect;
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    FieldShape, TextField as FieldSpec, cancel_button, cancel_button_rect, combo_arrow,
    combo_arrow_rect, magnifier, text_field, text_field_inside,
};
use neue_theme::materials::text_field::CARET_WIDTH;
use neue_theme::metrics::control;
use neue_theme::motion::{CARET_BLINK, CARET_BLINK_TIMEOUT};
use neue_theme::typography;

use super::edit_menu::{self, EditCommand};
use super::menu::{self, Anchor, MenuWidget};
use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::anim::AnimCtx;
use crate::element::{Element, Handle};
use crate::menu::MenuSpec;
use crate::theme::ThemeProp;

mod combo;
pub use combo::ComboBox;

/// What a secure field shows for each character.
const BULLET: &str = "\u{2022}";

/// The three kinds of field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Text,
    Search,
    Secure,
}

/// What a field tells the app.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FieldAction {
    /// The user changed the text (not the IME's uncommitted preedit).
    Changed(String),
    /// Return, in a field with a submit handler.
    Submitted(String),
    /// A combo box's suggestion was chosen: its index.
    Chosen(usize),
    /// The user finished selecting text with the pointer or Shift: the runner makes it the
    /// primary selection. Never sent by a secure field.
    Selected(String),
}

/// The caret's blink: shown for the first half of each [`CARET_BLINK`], solid again
/// [`CARET_BLINK_TIMEOUT`] after the last keystroke.
#[derive(Debug, Default)]
pub(super) struct Blink {
    since: Duration,
    running: bool,
}

impl Blink {
    pub(super) fn visible(&self) -> bool {
        self.since >= CARET_BLINK_TIMEOUT
            || self.since.as_nanos() % CARET_BLINK.as_nanos() < CARET_BLINK.as_nanos() / 2
    }

    /// Shows the caret and starts blinking again, if `wanted`.
    pub(super) fn restart(&mut self, ctx: &mut impl AnimCtx, wanted: bool) {
        self.since = Duration::ZERO;
        ctx.request_paint_only();
        if wanted && !self.running {
            self.running = true;
            ctx.request_anim_frame();
        }
    }

    pub(super) fn frame(&mut self, ctx: &mut UpdateCtx<'_>, wanted: bool, interval: u64) {
        let was = self.visible();
        if wanted && self.since < CARET_BLINK_TIMEOUT {
            self.since += Duration::from_nanos(interval);
            ctx.request_anim_frame();
        } else {
            self.running = false;
            self.since = CARET_BLINK_TIMEOUT;
        }
        if self.visible() != was {
            ctx.request_paint_only();
        }
    }
}

/// The Masonry widget behind [`TextField`].
pub(crate) struct FieldWidget {
    kind: Kind,
    editor: PlainEditor<BrushIndex>,
    /// Secure fields: the real text, one entry per bullet in the editor.
    secret: Vec<char>,
    placeholder: TextLine,
    access: PlainEditorAccessibility,
    replayer: ImagingReplayer,
    width: f64,
    /// Return sends [`FieldAction::Submitted`]; otherwise it goes on to the window (and
    /// its default button).
    submits: bool,
    /// How far the text is scrolled left to keep the caret in view.
    scroll: f64,
    size: Size,
    rendered: Generation,
    /// Focus came from a click, which places the caret, rather than from Tab, which
    /// selects everything.
    focus_by_click: bool,
    /// The search field's cancel button is held down.
    cancel_held: bool,
    /// What assistive technology announces.
    label: Option<String>,
    /// The text was replaced: put the caret at its end at the next layout.
    caret_to_end: bool,
    /// Has keyboard focus. Without it the text shows from its start.
    focused: bool,
    blink: Blink,
    /// A combo box's menu and state (`None`: not a combo box).
    combo: Option<Combo>,
    /// What had focus before the edit menu opened.
    edit_return_focus: Option<WidgetId>,
    /// A menu took focus away and gives it back: the text stays as it was, not selected
    /// as Tab would.
    keep_selection: bool,
    /// A primary-button press began a selection that its release ends.
    selecting: bool,
}

/// What makes a text field a combo box: an arrow button inside its right end that opens a
/// menu of suggestions as wide as the field (ADR 0046).
#[derive(Debug)]
struct Combo {
    spec: MenuSpec,
    /// The title of each item: what choosing it puts in the field.
    labels: Vec<String>,
    /// Its menu is up.
    open: bool,
    return_focus: Option<WidgetId>,
}

impl std::fmt::Debug for FieldWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldWidget")
            .field("kind", &self.kind)
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

/// `text` as one line: line breaks and tabs become spaces, other control characters go.
fn one_line(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

impl FieldWidget {
    fn new(kind: Kind) -> Self {
        let mut editor = PlainEditor::new(TextStyle::Body.size());
        let family = format!("\"{}\", sans-serif", typography::UI_FAMILY);
        let styles = editor.edit_styles();
        styles.insert(StyleProperty::FontFamily(FontFamily::Source(family.into())));
        let placeholder = match kind {
            Kind::Search => "Search",
            Kind::Text | Kind::Secure => "",
        };
        Self {
            kind,
            editor,
            secret: Vec::new(),
            placeholder: TextLine::new(placeholder.to_owned(), TextStyle::Body),
            access: PlainEditorAccessibility::default(),
            replayer: ImagingReplayer::new(),
            width: control::TEXT_FIELD_WIDTH,
            submits: false,
            scroll: 0.0,
            size: Size::ZERO,
            rendered: Generation::default(),
            focus_by_click: false,
            cancel_held: false,
            label: None,
            caret_to_end: false,
            focused: false,
            blink: Blink::default(),
            combo: None,
            edit_return_focus: None,
            keep_selection: false,
            selecting: false,
        }
    }

    fn shape(&self) -> FieldShape {
        match self.kind {
            Kind::Search => FieldShape::Round,
            Kind::Text | Kind::Secure => FieldShape::Square,
        }
    }

    /// The field's text (for a secure field, the real one).
    fn value(&self) -> String {
        match self.kind {
            Kind::Secure => self.secret.iter().collect(),
            Kind::Text | Kind::Search => self.editor.text().into_iter().collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.editor.raw_text().is_empty()
    }

    /// Replaces the text, caret at the end, without telling the app.
    fn set_value(&mut self, text: &str) {
        let text = one_line(text);
        if self.kind == Kind::Secure {
            self.secret = text.chars().collect();
            self.editor.set_text(&BULLET.repeat(self.secret.len()));
        } else {
            self.editor.set_text(&text);
        }
        self.caret_to_end = true;
    }

    fn rect(&self) -> Rect {
        Rect::from_origin_size(Point::ORIGIN, self.size)
    }

    /// Where text goes: inside the well, clear of a search field's glyphs.
    fn text_rect(&self) -> Rect {
        let inset = match self.kind {
            Kind::Search => control::SEARCH_FIELD_INSET,
            Kind::Text | Kind::Secure => control::TEXT_FIELD_INSET,
        };
        let arrow = if self.combo.is_some() {
            control::COMBO_ARROW_WIDTH
        } else {
            0.0
        };
        text_field_inside(self.rect(), self.shape())
            .rect()
            .inset(Insets::new(-inset, 0.0, -inset - arrow, 0.0))
    }

    /// Whether `at` (local) is on a combo box's arrow button.
    fn over_arrow(&self, at: Point) -> bool {
        self.combo.is_some() && combo_arrow_rect(self.rect()).contains(at)
    }

    /// Opens a combo box's menu right below the field, as wide as the field.
    fn open_menu(&mut self, ctx: &mut EventCtx<'_>, keyboard: bool, pointer: Option<Point>) {
        let Some(combo) = &mut self.combo else {
            return;
        };
        if ctx.is_disabled() {
            return;
        }
        let rect = ctx.border_box();
        let origin = ctx.to_window(rect.origin());
        let at = Point::new(origin.x, origin.y + rect.height());
        let anchor = Anchor {
            at,
            pointer: pointer.unwrap_or(at),
            min_width: rect.width(),
            highlight: None,
            on_closed: Some(combo::closed),
            refocus: None,
            on_step: None,
            on_change: None,
            return_focus: None,
            beside: None,
        };
        menu::open(
            ctx,
            combo.spec.clone(),
            anchor,
            keyboard,
            &mut combo.return_focus,
        );
        combo.open = true;
        self.keep_selection = true;
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    /// Opens the edit menu after the event that asked for it, unless a context menu
    /// around the field opened one meanwhile (it gets the event after the field).
    fn defer_edit_menu(&self, ctx: &mut EventCtx<'_>, at: Point, keyboard: bool) {
        if self.kind == Kind::Secure || ctx.is_disabled() {
            return;
        }
        let epoch = menu::opened();
        let id = ctx.widget_id();
        ctx.mutate_later(id, move |mut field| {
            if menu::opened() != epoch {
                return;
            }
            let mut this = field.downcast::<FieldWidget>();
            this.widget.open_edit_menu(&mut this.ctx, at, keyboard);
        });
    }

    /// Where a keyboard-opened edit menu goes: below the caret.
    fn caret_window_point(&self, ctx: &EventCtx<'_>) -> Point {
        let x = self
            .editor
            .cursor_geometry(1.0)
            .map_or(0.0, |caret| caret.x0 + self.text_origin().x);
        ctx.to_window(Point::new(x.clamp(0.0, self.size.width), self.size.height))
    }

    fn open_edit_menu(&mut self, ctx: &mut MutateCtx<'_>, at: Point, keyboard: bool) {
        if ctx.is_disabled() {
            return;
        }
        let spec = edit_menu::spec(
            true,
            !self.editor.raw_selection().is_collapsed(),
            !self.is_empty(),
        );
        let anchor = Anchor {
            on_closed: Some(edit_closed),
            refocus: Some(ctx.widget_id()),
            on_step: None,
            on_change: None,
            return_focus: None,
            ..Anchor::at(at)
        };
        menu::open(ctx, spec, anchor, keyboard, &mut self.edit_return_focus);
        self.keep_selection = true;
    }
    /// The top left of the laid-out text, scrolled.
    fn text_origin(&self) -> Point {
        let height = self
            .editor
            .try_layout()
            .map_or(0.0, |l| f64::from(l.height()));
        let y = ((self.size.height - height) / 2.0).round();
        Point::new(self.text_rect().x0 - self.scroll, y)
    }

    fn shows_cancel(&self) -> bool {
        self.kind == Kind::Search && !self.is_empty()
    }

    fn over_cancel(&self, at: Point) -> bool {
        self.shows_cancel() && cancel_button_rect(self.rect()).contains(at)
    }

    /// Scrolls so the caret is inside the text rect, and no further than the text needs;
    /// back to the start while the field isn't being edited.
    fn scroll_to_caret(&mut self) {
        let Some(layout) = self.editor.try_layout() else {
            return;
        };
        if !self.focused {
            self.scroll = 0.0;
            return;
        }
        let visible = self.text_rect().width() - CARET_WIDTH;
        let full = f64::from(layout.full_width());
        if let Some(caret) = self.editor.cursor_geometry(1.0) {
            let x = caret.x0;
            if x - self.scroll > visible {
                self.scroll = x - visible;
            } else if x < self.scroll {
                self.scroll = x;
            }
        }
        self.scroll = self.scroll.clamp(0.0, (full - visible).max(0.0));
    }

    /// Inserts typed, pasted or committed text over the selection.
    fn insert(&mut self, ctx: &mut EventCtx<'_>, text: &str) {
        let text = one_line(text);
        if text.is_empty() {
            return;
        }
        let (fonts, layouts) = ctx.text_contexts();
        let mut driver = self.editor.driver(fonts, layouts);
        if self.kind == Kind::Secure {
            let range = driver.editor.raw_selection().text_range();
            let (start, end) = (range.start / BULLET.len(), range.end / BULLET.len());
            let end = end.min(self.secret.len());
            let start = start.min(end);
            self.secret.splice(start..end, text.chars());
            driver.insert_or_replace_selection(&BULLET.repeat(text.chars().count()));
        } else {
            driver.insert_or_replace_selection(&text);
        }
    }

    /// Runs a deleting editor operation, and in a secure field removes the same characters
    /// from the real text: the editor holds only bullets, so whatever it deleted ends at
    /// the new caret.
    fn delete(
        &mut self,
        ctx: &mut EventCtx<'_>,
        op: impl FnOnce(&mut PlainEditorDriver<'_, BrushIndex>),
    ) {
        let before = self.editor.raw_text().len();
        let (fonts, layouts) = ctx.text_contexts();
        op(&mut self.editor.driver(fonts, layouts));
        if self.kind == Kind::Secure {
            let removed = before.saturating_sub(self.editor.raw_text().len()) / BULLET.len();
            let at = (self.editor.raw_selection().text_range().start / BULLET.len())
                .min(self.secret.len());
            let end = (at + removed).min(self.secret.len());
            self.secret.drain(at..end);
        }
    }

    /// Runs an editor operation that doesn't change the text.
    fn drive(
        &mut self,
        ctx: &mut EventCtx<'_>,
        op: impl FnOnce(&mut PlainEditorDriver<'_, BrushIndex>),
    ) {
        let (fonts, layouts) = ctx.text_contexts();
        op(&mut self.editor.driver(fonts, layouts));
    }

    fn clear(&mut self) {
        self.secret.clear();
        self.editor.set_text("");
    }

    /// After an event: lay out again if the editor changed (the caret may need scrolling
    /// into view), and tell the app if the text did.
    fn after_edit(&mut self, ctx: &mut EventCtx<'_>, changed: bool) {
        if self.editor.generation() != self.rendered {
            self.rendered = self.editor.generation();
            ctx.request_layout();
            ctx.request_render();
        }
        if changed {
            ctx.submit_action::<FieldAction>(FieldAction::Changed(self.value()));
        }
        let wanted = !ctx.is_disabled();
        self.blink.restart(ctx, wanted);
    }

    /// Offers the selected text as the primary selection (a middle click elsewhere pastes
    /// it). Called when a pointer or Shift selection ends. Never for a secure field, nor
    /// for an empty selection (that leaves the primary selection as it is).
    fn publish_selection(&self, ctx: &mut EventCtx<'_>) {
        if self.kind == Kind::Secure {
            return;
        }
        if let Some(text) = self.editor.selected_text().filter(|t| !t.is_empty()) {
            ctx.submit_action::<FieldAction>(FieldAction::Selected(text.to_owned()));
        }
    }

    /// A key while focused; false if the field doesn't use it.
    fn on_key(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &Key,
        mods: masonry_core::core::Modifiers,
    ) -> Option<bool> {
        let mac = cfg!(target_os = "macos");
        let shift = mods.shift();
        // The command key (Super, ⌘; ADR 0080) acts on the clipboard and jumps to the ends
        // of the line; the word key (Option on macOS, Ctrl elsewhere) moves by words. A
        // secure field has no words: its word moves go to the ends.
        let command = mods.meta();
        let secure = self.kind == Kind::Secure;
        let word = if mac { mods.alt() } else { mods.ctrl() };
        let to_end = command || (word && secure);
        let word = word && !secure;
        let collapsed = self.editor.raw_selection().is_collapsed();
        let mut changed = false;
        match key {
            Key::Character(c) if command && c.eq_ignore_ascii_case("a") => {
                self.drive(ctx, |d| d.select_all());
            }
            Key::Character(c) if command && c.eq_ignore_ascii_case("c") => {
                if let (false, Some(text)) = (secure, self.editor.selected_text()) {
                    ctx.set_clipboard(text.to_owned());
                }
            }
            Key::Character(c) if command && c.eq_ignore_ascii_case("x") => {
                if let (false, Some(text)) = (secure, self.editor.selected_text()) {
                    ctx.set_clipboard(text.to_owned());
                    self.delete(ctx, |d| d.delete_selection());
                    changed = true;
                }
            }
            Key::Named(NamedKey::ArrowLeft) => self.drive(ctx, |d| match (to_end, word, shift) {
                (true, _, false) => d.move_to_text_start(),
                (true, _, true) => d.select_to_text_start(),
                (_, true, false) => d.move_word_left(),
                (_, true, true) => d.select_word_left(),
                (_, _, false) => d.move_left(),
                (_, _, true) => d.select_left(),
            }),
            Key::Named(NamedKey::ArrowRight) => self.drive(ctx, |d| match (to_end, word, shift) {
                (true, _, false) => d.move_to_text_end(),
                (true, _, true) => d.select_to_text_end(),
                (_, true, false) => d.move_word_right(),
                (_, true, true) => d.select_word_right(),
                (_, _, false) => d.move_right(),
                (_, _, true) => d.select_right(),
            }),
            // One line: Up and Home go to its start, Down and End to its end.
            Key::Named(NamedKey::ArrowUp | NamedKey::Home) => self.drive(ctx, |d| {
                if shift {
                    d.select_to_text_start();
                } else {
                    d.move_to_text_start();
                }
            }),
            // The Menu key and Shift-F10: the edit menu, unless a context menu around the
            // field takes the key (it is not handled here, so it bubbles up to one).
            Key::Named(NamedKey::ContextMenu) => {
                let at = self.caret_window_point(ctx);
                self.defer_edit_menu(ctx, at, true);
                return None;
            }
            Key::Named(NamedKey::F10) if shift && !mods.ctrl() && !mods.alt() && !mods.meta() => {
                let at = self.caret_window_point(ctx);
                self.defer_edit_menu(ctx, at, true);
                return None;
            }
            Key::Named(NamedKey::ArrowDown) if self.combo.is_some() && !shift => {
                self.open_menu(ctx, true, None);
            }
            Key::Named(NamedKey::ArrowDown | NamedKey::End) => self.drive(ctx, |d| {
                if shift {
                    d.select_to_text_end();
                } else {
                    d.move_to_text_end();
                }
            }),
            Key::Named(NamedKey::Backspace) => {
                self.delete(ctx, |d| match (collapsed && to_end, word) {
                    (true, _) => {
                        d.select_to_text_start();
                        d.delete_selection();
                    }
                    (false, true) => d.backdelete_word(),
                    (false, false) => d.backdelete(),
                });
                changed = true;
            }
            Key::Named(NamedKey::Delete) => {
                self.delete(ctx, |d| match (collapsed && to_end, word) {
                    (true, _) => {
                        d.select_to_text_end();
                        d.delete_selection();
                    }
                    (false, true) => d.delete_word(),
                    (false, false) => d.delete(),
                });
                changed = true;
            }
            Key::Named(NamedKey::Enter) if self.submits => {
                ctx.submit_action::<FieldAction>(FieldAction::Submitted(self.value()));
            }
            Key::Named(NamedKey::Escape) if self.shows_cancel() => {
                self.clear();
                changed = true;
            }
            Key::Character(text) if !mods.ctrl() && !mods.meta() => {
                self.insert(ctx, text);
                changed = true;
            }
            _ => return None,
        }
        Some(changed)
    }
}

impl UsesProperty<ThemeProp> for FieldWidget {}

impl Widget for FieldWidget {
    type Action = FieldAction;

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let wanted = ctx.is_focus_target()
            && ctx.is_window_focused()
            && !ctx.is_disabled()
            && !theme.reduced_motion;
        self.blink.frame(ctx, wanted, interval);
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Secondary),
                state,
                ..
            }) => {
                if self.editor.is_composing() {
                    return;
                }
                let local = ctx.local_position(state.position);
                if self.editor.raw_selection().is_collapsed() {
                    let p = local - self.text_origin().to_vec2();
                    let (fonts, layouts) = ctx.text_contexts();
                    self.editor
                        .driver(fonts, layouts)
                        .move_to_point(p.x as f32, p.y as f32);
                }
                if !ctx.is_focus_target() {
                    self.focus_by_click = true;
                    ctx.request_focus();
                }
                self.defer_edit_menu(ctx, ctx.to_window(local), false);
                self.after_edit(ctx, false);
            }
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let local = ctx.local_position(state.position);
                if self.over_arrow(local) {
                    // While its menu is up the menu took this press as an outside one and
                    // is closing: pressing the arrow again closes it.
                    if ctx.get_attached_layer::<MenuWidget>().is_none() {
                        let at = ctx.to_window(local);
                        self.open_menu(ctx, false, Some(at));
                    }
                    return;
                }
                if self.over_cancel(local) {
                    self.cancel_held = true;
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                    return;
                }
                if self.editor.is_composing() {
                    return;
                }
                let p = local - self.text_origin().to_vec2();
                let (x, y) = (p.x as f32, p.y as f32);
                let secure = self.kind == Kind::Secure;
                let (fonts, layouts) = ctx.text_contexts();
                let mut drv = self.editor.driver(fonts, layouts);
                match state.count {
                    2 if !secure => drv.select_word_at_point(x, y),
                    2.. => drv.select_all(),
                    _ if state.modifiers.shift() => drv.shift_click_extension(x, y),
                    _ => drv.move_to_point(x, y),
                }
                if !ctx.is_focus_target() {
                    self.focus_by_click = true;
                    ctx.request_focus();
                }
                ctx.capture_pointer();
                self.selecting = true;
                self.after_edit(ctx, false);
            }
            PointerEvent::Up(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                ..
            }) if self.selecting => {
                self.selecting = false;
                self.publish_selection(ctx);
            }
            PointerEvent::Move(PointerUpdate { current, .. })
                if ctx.is_active() && !self.cancel_held =>
            {
                let p = ctx.local_position(current.position) - self.text_origin().to_vec2();
                let (fonts, layouts) = ctx.text_contexts();
                self.editor
                    .driver(fonts, layouts)
                    .extend_selection_to_point(p.x as f32, p.y as f32);
                self.after_edit(ctx, false);
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) if self.cancel_held => {
                self.cancel_held = false;
                let local = ctx.local_position(state.position);
                if self.over_cancel(local) {
                    self.clear();
                    self.after_edit(ctx, true);
                }
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) => {
                self.cancel_held = false;
                self.selecting = false;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if key.state.is_down() => {
                if self.editor.is_composing() {
                    return;
                }
                let Some(changed) = self.on_key(ctx, &key.key, key.modifiers) else {
                    return;
                };
                ctx.set_handled();
                self.after_edit(ctx, changed);
                // Shift with a caret move ends one selecting step: the selection is the
                // primary selection from now on.
                if key.modifiers.shift()
                    && matches!(
                        key.key,
                        Key::Named(
                            NamedKey::ArrowLeft
                                | NamedKey::ArrowRight
                                | NamedKey::ArrowUp
                                | NamedKey::ArrowDown
                                | NamedKey::Home
                                | NamedKey::End
                        )
                    )
                {
                    self.publish_selection(ctx);
                }
            }
            TextEvent::Ime(ime) => {
                let (fonts, layouts) = ctx.text_contexts();
                let mut drv = self.editor.driver(fonts, layouts);
                let changed = match ime {
                    // A secure field has no IME; a platform that commits anyway types.
                    Ime::Preedit(..) if self.kind == Kind::Secure => false,
                    Ime::Disabled => {
                        drv.clear_compose();
                        false
                    }
                    Ime::Preedit(text, cursor) => {
                        if text.is_empty() {
                            drv.clear_compose();
                        } else {
                            drv.set_compose(text, *cursor);
                        }
                        false
                    }
                    Ime::Commit(text) => {
                        drv.clear_compose();
                        self.insert(ctx, text);
                        true
                    }
                    Ime::Enabled => false,
                };
                ctx.set_handled();
                self.after_edit(ctx, changed);
            }
            TextEvent::ClipboardPaste(text) => {
                if self.editor.is_composing() {
                    return;
                }
                self.insert(ctx, text);
                ctx.set_handled();
                self.after_edit(ctx, true);
            }
            TextEvent::WindowFocusChange(_) => {
                let wanted = !ctx.is_disabled();
                self.blink.restart(ctx, wanted);
                ctx.request_render();
            }
            TextEvent::Keyboard(_) => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match (&event.action, &event.data) {
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                if self.editor.is_composing() {
                    return;
                }
                let (fonts, layouts) = ctx.text_contexts();
                self.access.select_from_access_selection(
                    &mut self.editor.driver(fonts, layouts),
                    selection,
                );
                self.after_edit(ctx, false);
            }
            (Action::SetValue, Some(ActionData::Value(value))) => {
                self.set_value(value);
                self.after_edit(ctx, true);
            }
            (Action::ShowContextMenu, _) => {
                let at = self.caret_window_point(ctx);
                self.defer_edit_menu(ctx, at, true);
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::FocusChanged(focused) => {
                if *focused && !self.focus_by_click && !self.keep_selection {
                    // Tab into a field selects its text.
                    let (fonts, layouts) = ctx.text_contexts();
                    self.editor.driver(fonts, layouts).select_all();
                }
                self.focus_by_click = false;
                if *focused {
                    self.keep_selection = false;
                }
                self.focused = *focused;
                if !*focused && self.editor.is_composing() {
                    let (fonts, layouts) = ctx.text_contexts();
                    self.editor.driver(fonts, layouts).clear_compose();
                }
                self.blink.restart(ctx, *focused);
                ctx.request_layout();
                ctx.request_render();
            }
            Update::DisabledChanged(_) => ctx.request_render(),
            // A compositor can take a combo box's menu away without telling us.
            Update::HoveredChanged(_) => {
                let up = ctx.get_attached_layer::<MenuWidget>().is_some();
                if let Some(combo) = &mut self.combo
                    && combo.open != up
                {
                    combo.open = up;
                    ctx.request_paint_only();
                    ctx.request_accessibility_update();
                }
            }
            Update::FontsChanged => {
                let _ = self.editor.edit_styles();
                self.placeholder.invalidate();
                ctx.request_layout();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_paint_insets(Insets::uniform(control::FOCUS_RING));
        let (fonts, layouts) = ctx.text_contexts();
        if std::mem::take(&mut self.caret_to_end) {
            self.editor.driver(fonts, layouts).move_to_text_end();
        } else {
            self.editor.refresh_layout(fonts, layouts);
        }
        self.placeholder.size(ctx.text_contexts());
        self.scroll_to_caret();
        let origin = self.text_origin();
        if let Some(line) = self.editor.try_layout().and_then(|l| l.get(0)) {
            let baseline = origin.y + f64::from(line.metrics().baseline);
            ctx.set_baselines(baseline, baseline);
        }
        let ime = bounding_box_to_rect(self.editor.ime_cursor_area()) + origin.to_vec2();
        ctx.set_ime_area(ime.intersect(self.rect()));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let colors = theme.colors();
        let disabled = ctx.is_disabled();
        let editing = ctx.is_focus_target() && !disabled;
        let active = ctx.is_window_focused();
        let rect = self.rect();

        let mut scene = Scene::new(rect.width(), rect.height());
        let spec = FieldSpec {
            shape: self.shape(),
            focused: editing && active,
            disabled,
        };
        text_field(&mut scene, rect, spec, theme.variant);
        if let Some(combo) = &self.combo {
            combo_arrow(
                &mut scene,
                combo_arrow_rect(rect),
                combo.open,
                disabled,
                theme.variant,
            );
        }
        if self.kind == Kind::Search {
            let h = rect.height();
            magnifier(
                &mut scene,
                Point::new(rect.x0 + h / 2.0 + 1.0, rect.center().y),
                theme.variant,
            );
            if self.shows_cancel() && !disabled {
                let held = self.cancel_held && ctx.is_active();
                cancel_button(&mut scene, cancel_button_rect(rect), held, theme.variant);
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let Some(layout) = self.editor.try_layout() else {
            return;
        };
        let text = self.text_rect();
        let inside = text_field_inside(rect, self.shape()).rect();
        let clip = Rect::new(
            text.x0 - CARET_WIDTH,
            inside.y0,
            text.x1 + CARET_WIDTH,
            inside.y1,
        );
        let origin = self.text_origin().to_vec2();
        let colour = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        let p = painter;
        p.push_fill_clip(clip);
        if editing {
            let selection = if active {
                colors.text_selection
            } else {
                colors.unfocused_selection
            };
            for (bb, _) in self.editor.selection_geometry() {
                p.fill(bounding_box_to_rect(bb) + origin, selection).draw();
            }
        }
        if self.is_empty() {
            let at = Affine::translate(Vec2::new(text.x0, origin.y));
            self.placeholder.paint(p, at, colors.tertiary_label);
        }
        render_text(p, Affine::translate(origin), layout, &[colour.into()], true);
        let collapsed = self.editor.raw_selection().is_collapsed();
        let caret_shown = self.blink.visible() || theme.reduced_motion;
        if editing
            && active
            && collapsed
            && caret_shown
            && let Some(caret) = self.editor.cursor_geometry(CARET_WIDTH as f32)
        {
            p.fill(bounding_box_to_rect(caret) + origin, colors.label)
                .draw();
        }
        p.pop_clip();
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let local = ctx.to_local(pos);
        if ctx.is_disabled() || self.over_cancel(local) || self.over_arrow(local) {
            CursorIcon::Default
        } else {
            CursorIcon::Text
        }
    }

    fn accessibility_role(&self) -> Role {
        match self.kind {
            Kind::Text if self.combo.is_some() => Role::ComboBox,
            Kind::Text => Role::TextInput,
            Kind::Search => Role::SearchInput,
            Kind::Secure => Role::PasswordInput,
        }
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        if let Some(label) = &self.label {
            node.set_label(label.clone());
        }
        if !self.placeholder.text().is_empty() {
            node.set_placeholder(self.placeholder.text().to_owned());
        }
        node.add_action(Action::SetValue);
        if self.kind != Kind::Secure {
            node.add_action(Action::ShowContextMenu);
        }
        if let Some(combo) = &self.combo {
            node.set_has_popup(HasPopup::Menu);
            node.set_expanded(combo.open);
        }
        let origin = self.text_origin();
        let colour = theme.colors().label;
        // Spans of a secure field hold bullets: the editor never sees the real text.
        let built = self.access.build_nodes(
            &self.editor,
            ctx.tree_update(),
            node,
            AccessCtx::next_node_id,
            origin.x,
            origin.y,
            |node, style| set_accesskit_brush_properties(node, style, &[colour.into()]),
        );
        if built.is_none() {
            tracing::debug!("text field accessibility before layout");
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        // Secure input bypasses input methods, as on the desktop it imitates.
        self.kind != Kind::Secure
    }

    fn get_debug_text(&self) -> Option<String> {
        match self.kind {
            Kind::Secure => Some(BULLET.repeat(self.secret.len())),
            Kind::Text | Kind::Search => Some(self.value()),
        }
    }
}

impl FieldWidget {
    pub(crate) fn edit_text(this: &mut WidgetMut<'_, Self>, text: &str) {
        this.widget.set_value(text);
        this.ctx.request_layout();
        this.ctx.request_render();
    }

    pub(crate) fn edit_placeholder(this: &mut WidgetMut<'_, Self>, text: String) {
        this.widget.placeholder.set_text(text);
        this.ctx.request_layout();
        this.ctx.request_render();
    }

    pub(crate) fn text(&self) -> String {
        self.value()
    }
}

/// The edit menu closed (`chosen`: the item, if one was chosen).
fn edit_closed(mut owner: WidgetMut<'_, dyn Widget>, chosen: Option<usize>) {
    let Some(command) = chosen.and_then(EditCommand::from_id) else {
        return;
    };
    let mut this = owner.downcast::<FieldWidget>();
    let changed = edit_menu::run(command, &mut this.widget.editor, &mut this.ctx, true);
    this.ctx.request_layout();
    this.ctx.request_render();
    if changed {
        let text = this.widget.value();
        this.ctx
            .submit_action::<FieldAction>(FieldAction::Changed(text));
    }
}

/// A one-line text field: a text field (`new`), a search field (`search`: rounded, with
/// a magnifier, and a cancel button that clears it), or a secure field (`secure`: shows a
/// bullet for each character, can't be copied from, and bypasses input methods).
///
/// Click to place the caret, drag to select, double-click for a word; Tab into it selects
/// everything. Sends the text on every change, and on Return if it has a submit handler;
/// without one, Return goes on to the window's default button. Escape clears a search
/// field.
///
/// ```no_run
/// # use neue_kit::{Handle, TextField};
/// # #[derive(Clone)] enum Msg { Name(String), Search(String) }
/// let name = TextField::new().placeholder("Name").on_change(Msg::Name);
/// let search = TextField::search().on_submit(Msg::Search);
/// ```
pub struct TextField<M> {
    inner: NewWidget<FieldWidget>,
    on_change: Option<Box<dyn FnMut(String) -> M>>,
    on_submit: Option<Box<dyn FnMut(String) -> M>>,
}

impl<M> std::fmt::Debug for TextField<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextField")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .field("on_submit", &self.on_submit.is_some())
            .finish()
    }
}

impl<M: 'static> Default for TextField<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: 'static> TextField<M> {
    fn with_kind(kind: Kind) -> Self {
        Self {
            inner: NewWidget::new(FieldWidget::new(kind)),
            on_change: None,
            on_submit: None,
        }
    }

    /// A plain text field.
    pub fn new() -> Self {
        Self::with_kind(Kind::Text)
    }

    /// A search field, with the placeholder "Search" until set otherwise.
    pub fn search() -> Self {
        Self::with_kind(Kind::Search)
    }

    /// A secure field, for passwords.
    pub fn secure() -> Self {
        Self::with_kind(Kind::Secure)
    }

    /// Starts with `text`, the caret at its end.
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self.inner.widget.set_value(text);
        self
    }

    /// Grey text shown while the field is empty.
    #[must_use]
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.inner.widget.placeholder.set_text(text.into());
        self
    }

    /// What assistive technology announces for the field.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Width in logical pixels ([`neue_theme::metrics::control::TEXT_FIELD_WIDTH`] by
    /// default).
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.inner.widget.width = width;
        self
    }

    /// Sends the message `make` returns, given the whole text, each time the user changes
    /// it. Text an input method is still composing doesn't count until it is committed;
    /// changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_change(mut self, make: impl FnMut(String) -> M + 'static) -> Self {
        self.on_change = Some(Box::new(make));
        self
    }

    /// Sends the message `make` returns, given the text, when the user presses Return.
    #[must_use]
    pub fn on_submit(mut self, make: impl FnMut(String) -> M + 'static) -> Self {
        self.inner.widget.submits = true;
        self.on_submit = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this field.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<TextField<M>> for Element<M> {
    fn from(field: TextField<M>) -> Self {
        let id = field.inner.id();
        let mut element = Element::new(field.inner.erased());
        let (mut on_change, mut on_submit) = (field.on_change, field.on_submit);
        if on_change.is_some() || on_submit.is_some() {
            element.on_action(id, move |action| {
                match action.downcast_ref::<FieldAction>()? {
                    FieldAction::Changed(text) => on_change.as_mut().map(|f| f(text.clone())),
                    FieldAction::Submitted(text) => on_submit.as_mut().map(|f| f(text.clone())),
                    FieldAction::Chosen(_) | FieldAction::Selected(_) => None,
                }
            });
        }
        element
    }
}

impl<M> Handle<TextField<M>> {
    /// Replaces the text, caret at the end. Sends no message.
    pub fn set_text<N>(&self, cx: &mut Cx<'_, N>, text: &str) {
        cx.edit::<FieldWidget, _>(self, |mut w| FieldWidget::edit_text(&mut w, text));
    }

    /// The field's text, if it's in an open window.
    pub fn text<N>(&self, cx: &mut Cx<'_, N>) -> Option<String> {
        cx.edit::<FieldWidget, _>(self, |w| w.widget.text())
    }

    /// Replaces the placeholder.
    pub fn set_placeholder<N>(&self, cx: &mut Cx<'_, N>, text: impl Into<String>) {
        let text = text.into();
        cx.edit::<FieldWidget, _>(self, |mut w| FieldWidget::edit_placeholder(&mut w, text));
    }

    /// Enables or disables the field.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<FieldWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_flattens_breaks_and_drops_controls() {
        assert_eq!(one_line("a\nb\r\tc\u{7}d"), "a b  cd");
    }

    #[test]
    fn the_caret_blinks_then_rests() {
        let mut blink = Blink::default();
        assert!(blink.visible());
        blink.since = CARET_BLINK * 3 / 4;
        assert!(!blink.visible());
        blink.since = CARET_BLINK_TIMEOUT;
        assert!(blink.visible());
    }
}
