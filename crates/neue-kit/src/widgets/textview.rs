//! The text view: multi-line, word-wrapped text in a scroll view, in the well of a text
//! field (ADR 0044).
//!
//! The editing code is the text field's (ADR 0042) without the one-line limits: the
//! editor wraps at the view's width, Return inserts a line break, Up and Down move between
//! lines, and the widget is the scroll view's *content*. It is as tall as its text and
//! asks the scroll view to reveal the caret after every change; the scroll view draws the
//! well and the focus ring.

use std::any::TypeId;

use masonry_core::accesskit::{Action, ActionData, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, CursorIcon, EventCtx, Ime, LayoutCtx,
    MeasureCtx, MutateCtx, NewWidget, PaintCtx, PlainEditorAccessibility, PointerButton,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, QueryCtx,
    RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut,
    render_text, set_accesskit_brush_properties,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Point, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use masonry_core::parley::editing::{Generation, PlainEditorDriver};
use masonry_core::parley::{FontFamily, PlainEditor, StyleProperty};
use masonry_core::util::bounding_box_to_rect;
use neue_theme::materials::text_field::CARET_WIDTH;
use neue_theme::metrics::control;
use neue_theme::typography;

use super::edit_menu::{self, EditCommand};
use super::field::Blink;
use super::menu::{self, Anchor};
use super::scroll::ScrollView;
use super::text::TextStyle;
use crate::Cx;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// What a text view tells the app.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewAction {
    /// The user changed the text (not the IME's uncommitted preedit).
    Changed(String),
}

/// `text` for a text view: line breaks are `\n`, tabs stay, other control characters go.
fn multi_line(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\r' => Some('\n'),
            '\n' | '\t' => Some(c),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// The edit menu closed (`chosen`: the item, if one was chosen).
fn edit_closed(mut owner: WidgetMut<'_, dyn Widget>, chosen: Option<usize>) {
    let Some(command) = chosen.and_then(EditCommand::from_id) else {
        return;
    };
    let mut this = owner.downcast::<TextViewWidget>();
    let editable = this.widget.editable;
    let changed = edit_menu::run(command, &mut this.widget.editor, &mut this.ctx, editable);
    this.ctx.request_layout();
    this.ctx.request_render();
    if changed {
        let text = this.widget.value();
        this.ctx
            .submit_action::<ViewAction>(ViewAction::Changed(text));
    }
}

/// The Masonry widget behind [`TextView`].
pub(crate) struct TextViewWidget {
    editor: PlainEditor<BrushIndex>,
    access: PlainEditorAccessibility,
    /// Typing, cutting and pasting change the text; a read-only view only selects and
    /// copies.
    editable: bool,
    /// The wrap width the editor last laid out for.
    wrap: Option<f32>,
    rendered: Generation,
    focus_by_click: bool,
    /// The text was replaced: put the caret at its start at the next layout.
    caret_to_start: bool,
    focused: bool,
    blink: Blink,
    /// What had focus before the edit menu opened.
    edit_return_focus: Option<WidgetId>,
}

impl std::fmt::Debug for TextViewWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextViewWidget")
            .field("editable", &self.editable)
            .finish_non_exhaustive()
    }
}

impl TextViewWidget {
    fn new() -> Self {
        let mut editor = PlainEditor::new(TextStyle::Body.size());
        let family = format!("\"{}\", sans-serif", typography::UI_FAMILY);
        editor
            .edit_styles()
            .insert(StyleProperty::FontFamily(FontFamily::Source(family.into())));
        Self {
            editor,
            access: PlainEditorAccessibility::default(),
            editable: true,
            wrap: None,
            rendered: Generation::default(),
            focus_by_click: false,
            caret_to_start: false,
            focused: false,
            blink: Blink::default(),
            edit_return_focus: None,
        }
    }

    fn value(&self) -> String {
        self.editor.text().into_iter().collect()
    }

    /// Replaces the text, caret at the start, without telling the app.
    fn set_value(&mut self, text: &str) {
        self.editor.set_text(&multi_line(text));
        self.caret_to_start = true;
    }

    /// Where the text starts, inside the view.
    fn origin() -> Vec2 {
        Vec2::new(control::TEXT_VIEW_INSET, control::TEXT_VIEW_INSET)
    }

    fn set_wrap(&mut self, wrap: Option<f32>) {
        if self.wrap != wrap {
            self.wrap = wrap;
            self.editor.set_width(wrap);
        }
    }

    /// Wrapped at `advance`: the size the text needs, plus the inset. Measuring
    /// shouldn't disturb the layout paint will use, so the old width is put back.
    fn measure_wrapped(&mut self, ctx: &mut MeasureCtx<'_>, advance: Option<f32>) -> Size {
        let old = self.wrap;
        self.set_wrap(advance);
        let (fonts, layouts) = ctx.text_contexts();
        let layout = self.editor.layout(fonts, layouts);
        let width = advance.unwrap_or_else(|| layout.full_width());
        let size = Size::new(f64::from(width), f64::from(layout.height()));
        self.set_wrap(old);
        self.editor.refresh_layout(fonts, layouts);
        size + Size::new(
            2.0 * control::TEXT_VIEW_INSET,
            2.0 * control::TEXT_VIEW_INSET,
        )
    }

    /// Inserts typed, pasted or committed text over the selection.
    fn insert(&mut self, ctx: &mut EventCtx<'_>, text: &str) {
        let text = multi_line(text);
        if text.is_empty() || !self.editable {
            return;
        }
        let (fonts, layouts) = ctx.text_contexts();
        self.editor
            .driver(fonts, layouts)
            .insert_or_replace_selection(&text);
    }

    fn drive(
        &mut self,
        ctx: &mut EventCtx<'_>,
        op: impl FnOnce(&mut PlainEditorDriver<'_, BrushIndex>),
    ) {
        let (fonts, layouts) = ctx.text_contexts();
        op(&mut self.editor.driver(fonts, layouts));
    }

    /// Opens the edit menu after the event that asked for it, unless a context menu
    /// around the view opened one meanwhile (it gets the event after the view).
    fn defer_edit_menu(&self, ctx: &mut EventCtx<'_>, at: Point, keyboard: bool) {
        if ctx.is_disabled() {
            return;
        }
        let epoch = menu::opened();
        let id = ctx.widget_id();
        ctx.mutate_later(id, move |mut view| {
            if menu::opened() != epoch {
                return;
            }
            let mut this = view.downcast::<TextViewWidget>();
            this.widget.open_edit_menu(&mut this.ctx, at, keyboard);
        });
    }

    /// Where a keyboard-opened edit menu goes: below the caret.
    fn caret_window_point(&self, ctx: &EventCtx<'_>) -> Point {
        let caret = self
            .editor
            .cursor_geometry(1.0)
            .map(bounding_box_to_rect)
            .map_or(Point::ORIGIN, |r| Point::new(r.x0, r.y1))
            + Self::origin();
        ctx.to_window(caret)
    }

    fn open_edit_menu(&mut self, ctx: &mut MutateCtx<'_>, at: Point, keyboard: bool) {
        if ctx.is_disabled() {
            return;
        }
        let spec = edit_menu::spec(
            self.editable,
            !self.editor.raw_selection().is_collapsed(),
            !self.editor.raw_text().is_empty(),
        );
        let anchor = Anchor {
            on_closed: Some(edit_closed),
            refocus: Some(ctx.widget_id()),
            ..Anchor::at(at)
        };
        menu::open(ctx, spec, anchor, keyboard, &mut self.edit_return_focus);
    }

    /// After an event: lay out again if the editor changed, ask the scroll view to keep
    /// the caret in sight, and tell the app if the text changed.
    fn after_edit(&mut self, ctx: &mut EventCtx<'_>, changed: bool) {
        let (fonts, layouts) = ctx.text_contexts();
        self.editor.refresh_layout(fonts, layouts);
        if self.editor.generation() != self.rendered {
            self.rendered = self.editor.generation();
            ctx.request_layout();
            ctx.request_render();
        }
        if let Some(caret) = self.editor.cursor_geometry(CARET_WIDTH as f32) {
            // With the inset above and below, so the first and last lines scroll flush.
            let inset = control::TEXT_VIEW_INSET;
            let rect = (bounding_box_to_rect(caret) + Self::origin()).inflate(0.0, inset);
            ctx.request_scroll_to(rect);
        }
        if changed {
            ctx.submit_action::<ViewAction>(ViewAction::Changed(self.value()));
        }
        let wanted = !ctx.is_disabled();
        self.blink.restart(ctx, wanted);
    }

    /// A key while focused; `None` if the view doesn't use it, else whether the text
    /// changed.
    fn on_key(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &Key,
        mods: masonry_core::core::Modifiers,
    ) -> Option<bool> {
        let mac = cfg!(target_os = "macos");
        let shift = mods.shift();
        // The command key acts on the clipboard and, on macOS, jumps to the ends of the
        // line (Left, Right) or the text (Up, Down); elsewhere Ctrl with Home and End goes
        // to the ends of the text, and with the arrows moves by words.
        let command = if mac { mods.meta() } else { mods.ctrl() };
        let word = if mac { mods.alt() } else { mods.ctrl() };
        let editable = self.editable;
        let collapsed = self.editor.raw_selection().is_collapsed();
        let mut changed = false;
        match key {
            // The Menu key and Shift-F10: the edit menu, unless a context menu around the
            // view takes the key (it is not handled here, so it bubbles up to one).
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
            Key::Character(c) if command && c.eq_ignore_ascii_case("a") => {
                self.drive(ctx, |d| d.select_all());
            }
            Key::Character(c) if command && c.eq_ignore_ascii_case("c") => {
                if let Some(text) = self.editor.selected_text() {
                    ctx.set_clipboard(text.to_owned());
                }
            }
            Key::Character(c) if command && c.eq_ignore_ascii_case("x") => {
                if let (true, Some(text)) = (editable, self.editor.selected_text()) {
                    ctx.set_clipboard(text.to_owned());
                    self.drive(ctx, |d| d.delete_selection());
                    changed = true;
                }
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.drive(ctx, |d| match (mac && command, word, shift) {
                    (true, _, false) => d.move_to_line_start(),
                    (true, _, true) => d.select_to_line_start(),
                    (_, true, false) => d.move_word_left(),
                    (_, true, true) => d.select_word_left(),
                    (_, _, false) => d.move_left(),
                    (_, _, true) => d.select_left(),
                })
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.drive(ctx, |d| match (mac && command, word, shift) {
                    (true, _, false) => d.move_to_line_end(),
                    (true, _, true) => d.select_to_line_end(),
                    (_, true, false) => d.move_word_right(),
                    (_, true, true) => d.select_word_right(),
                    (_, _, false) => d.move_right(),
                    (_, _, true) => d.select_right(),
                })
            }
            Key::Named(NamedKey::ArrowUp) => self.drive(ctx, |d| match (mac && command, shift) {
                (true, false) => d.move_to_text_start(),
                (true, true) => d.select_to_text_start(),
                (_, false) => d.move_up(),
                (_, true) => d.select_up(),
            }),
            Key::Named(NamedKey::ArrowDown) => self.drive(ctx, |d| match (mac && command, shift) {
                (true, false) => d.move_to_text_end(),
                (true, true) => d.select_to_text_end(),
                (_, false) => d.move_down(),
                (_, true) => d.select_down(),
            }),
            Key::Named(NamedKey::Home) => self.drive(ctx, |d| match (command, shift) {
                (true, false) => d.move_to_text_start(),
                (true, true) => d.select_to_text_start(),
                (false, false) => d.move_to_line_start(),
                (false, true) => d.select_to_line_start(),
            }),
            Key::Named(NamedKey::End) => self.drive(ctx, |d| match (command, shift) {
                (true, false) => d.move_to_text_end(),
                (true, true) => d.select_to_text_end(),
                (false, false) => d.move_to_line_end(),
                (false, true) => d.select_to_line_end(),
            }),
            Key::Named(NamedKey::Backspace) if editable => {
                let to_start = mac && command && collapsed;
                self.drive(ctx, |d| match (to_start, word) {
                    (true, _) => {
                        d.select_to_line_start();
                        d.delete_selection();
                    }
                    (false, true) => d.backdelete_word(),
                    (false, false) => d.backdelete(),
                });
                changed = true;
            }
            Key::Named(NamedKey::Delete) if editable => {
                let to_end = mac && command && collapsed;
                self.drive(ctx, |d| match (to_end, word) {
                    (true, _) => {
                        d.select_to_line_end();
                        d.delete_selection();
                    }
                    (false, true) => d.delete_word(),
                    (false, false) => d.delete(),
                });
                changed = true;
            }
            Key::Named(NamedKey::Enter) if editable => {
                self.insert(ctx, "\n");
                changed = true;
            }
            Key::Character(text) if editable && !mods.ctrl() && !mods.meta() => {
                self.insert(ctx, text);
                changed = true;
            }
            // Page Up and Down, Tab and the rest are the scroll view's and the window's.
            _ => return None,
        }
        Some(changed)
    }
}

impl UsesProperty<ThemeProp> for TextViewWidget {}

impl Widget for TextViewWidget {
    type Action = ViewAction;

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
            && self.editable
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
                    let p = local - Self::origin();
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
                if self.editor.is_composing() {
                    return;
                }
                let p = ctx.local_position(state.position) - Self::origin();
                let (x, y) = (p.x as f32, p.y as f32);
                let (fonts, layouts) = ctx.text_contexts();
                let mut drv = self.editor.driver(fonts, layouts);
                match state.count {
                    2 => drv.select_word_at_point(x, y),
                    3.. => drv.select_hard_line_at_point(x, y),
                    _ if state.modifiers.shift() => drv.shift_click_extension(x, y),
                    _ => drv.move_to_point(x, y),
                }
                if !ctx.is_focus_target() {
                    self.focus_by_click = true;
                    ctx.request_focus();
                }
                ctx.capture_pointer();
                self.after_edit(ctx, false);
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                let p = ctx.local_position(current.position) - Self::origin();
                let (fonts, layouts) = ctx.text_contexts();
                self.editor
                    .driver(fonts, layouts)
                    .extend_selection_to_point(p.x as f32, p.y as f32);
                self.after_edit(ctx, false);
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
            }
            TextEvent::Ime(ime) if self.editable => {
                let (fonts, layouts) = ctx.text_contexts();
                let mut drv = self.editor.driver(fonts, layouts);
                let changed = match ime {
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
            TextEvent::ClipboardPaste(text) if self.editable => {
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
            _ => {}
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
            (Action::SetValue, Some(ActionData::Value(value))) if self.editable => {
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
                self.focus_by_click = false;
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
            Update::FontsChanged => {
                let _ = self.editor.edit_styles();
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
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let inset = 2.0 * control::TEXT_VIEW_INSET;
        let advance = |space: f64| Some((space - inset).max(0.0) as f32);
        let wrap = match axis {
            // The width: the longest line unwrapped, as much as is offered, or the
            // longest word.
            Axis::Horizontal => match len_req {
                LenReq::MaxContent => None,
                LenReq::FitContent(space) => advance(space.get()),
                LenReq::MinContent => Some(0.0),
            },
            // The height depends on the width it is given.
            Axis::Vertical => cross_length.and_then(|width| advance(width.get())),
        };
        let size = self.measure_wrapped(ctx, wrap);
        Length::px(match axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let inset = 2.0 * control::TEXT_VIEW_INSET;
        self.set_wrap(Some((size.width - inset).max(0.0) as f32));
        let (fonts, layouts) = ctx.text_contexts();
        if std::mem::take(&mut self.caret_to_start) {
            self.editor.driver(fonts, layouts).move_to_text_start();
        } else {
            self.editor.refresh_layout(fonts, layouts);
        }
        let origin = Self::origin();
        if let Some(line) = self.editor.try_layout().and_then(|l| l.get(0)) {
            let baseline = origin.y + f64::from(line.metrics().baseline);
            ctx.set_baselines(baseline, baseline);
        }
        let ime = bounding_box_to_rect(self.editor.ime_cursor_area()) + origin;
        ctx.set_ime_area(ime);
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
        let focus = ctx.is_focus_target() && !disabled;
        let active = ctx.is_window_focused();
        let Some(layout) = self.editor.try_layout() else {
            return;
        };
        let origin = Self::origin();
        let colour = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        let p = painter;
        if focus {
            let selection = if active {
                colors.text_selection
            } else {
                colors.unfocused_selection
            };
            for (bb, _) in self.editor.selection_geometry() {
                p.fill(bounding_box_to_rect(bb) + origin, selection).draw();
            }
        }
        render_text(p, Affine::translate(origin), layout, &[colour.into()], true);
        let collapsed = self.editor.raw_selection().is_collapsed();
        let caret_shown = self.blink.visible() || theme.reduced_motion;
        if focus
            && active
            && self.editable
            && collapsed
            && caret_shown
            && let Some(caret) = self.editor.cursor_geometry(CARET_WIDTH as f32)
        {
            p.fill(bounding_box_to_rect(caret) + origin, colors.label)
                .draw();
        }
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
        if ctx.is_disabled() {
            CursorIcon::Default
        } else {
            CursorIcon::Text
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::MultilineTextInput
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        if self.editable {
            node.add_action(Action::SetValue);
        } else {
            node.set_read_only();
        }
        node.add_action(Action::ShowContextMenu);
        let origin = Self::origin();
        let colour = theme.colors().label;
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
            tracing::debug!("text view accessibility before layout");
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        self.editable
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.value())
    }
}

impl TextViewWidget {
    pub(crate) fn edit_text(this: &mut WidgetMut<'_, Self>, text: &str) {
        this.widget.set_value(text);
        this.ctx.request_layout();
        this.ctx.request_render();
    }

    pub(crate) fn text(&self) -> String {
        self.value()
    }
}

/// Multi-line text in a scrolling, word-wrapped view with a text field's well.
///
/// Return starts a new line, Up and Down move between lines, Page Up and Page Down
/// scroll; Tab moves focus on (it doesn't insert a tab). Click to place the caret, drag
/// to select, double-click for a word, triple-click for a paragraph. Sends the whole text
/// on every change. [`editable(false)`](Self::editable) makes a view for reading: its
/// text can be selected and copied, not changed.
///
/// ```no_run
/// # use neue_kit::{Handle, TextView};
/// # #[derive(Clone)] enum Msg { Notes(String) }
/// let notes = TextView::new().text("Dear diary,").on_change(Msg::Notes);
/// let licence = TextView::<Msg>::new().editable(false).text("GPL-3.0-or-later").height(200.0);
/// ```
pub struct TextView<M> {
    inner: NewWidget<TextViewWidget>,
    width: f64,
    height: f64,
    on_change: Option<Box<dyn FnMut(String) -> M>>,
}

impl<M> std::fmt::Debug for TextView<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextView")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .finish_non_exhaustive()
    }
}

impl<M: 'static> Default for TextView<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: 'static> TextView<M> {
    /// An empty, editable text view.
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(TextViewWidget::new()),
            width: control::TEXT_VIEW_WIDTH,
            height: control::TEXT_VIEW_HEIGHT,
            on_change: None,
        }
    }

    /// Starts with `text`, the caret at its start.
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self.inner.widget.set_value(text);
        self
    }

    /// Whether the user can change the text (yes by default).
    #[must_use]
    pub fn editable(mut self, editable: bool) -> Self {
        self.inner.widget.editable = editable;
        self
    }

    /// Width in logical pixels ([`neue_theme::metrics::control::TEXT_VIEW_WIDTH`] by
    /// default).
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.width = width.max(0.0);
        self
    }

    /// Height in logical pixels ([`neue_theme::metrics::control::TEXT_VIEW_HEIGHT`] by
    /// default); longer text scrolls.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.height = height.max(0.0);
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

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this text view.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<TextView<M>> for Element<M> {
    fn from(view: TextView<M>) -> Self {
        let TextView {
            inner,
            width,
            height,
            on_change,
        } = view;
        let id = inner.id();
        let mut content = Element::new(inner.erased());
        if let Some(mut make) = on_change {
            content.on_action(id, move |action| {
                match action.downcast_ref::<ViewAction>()? {
                    ViewAction::Changed(text) => Some(make(text.clone())),
                }
            });
        }
        ScrollView::new(content)
            .framed()
            .width(width)
            .height(height)
            .into()
    }
}

impl<M> Handle<TextView<M>> {
    /// Replaces the text, caret at the start. Sends no message.
    pub fn set_text<N>(&self, cx: &mut Cx<'_, N>, text: &str) {
        cx.edit::<TextViewWidget, _>(self, |mut w| TextViewWidget::edit_text(&mut w, text));
    }

    /// The text, if the view is in an open window.
    pub fn text<N>(&self, cx: &mut Cx<'_, N>) -> Option<String> {
        cx.edit::<TextViewWidget, _>(self, |w| w.widget.text())
    }

    /// Enables or disables the view.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<TextViewWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_line_keeps_breaks_and_tabs_and_drops_other_controls() {
        assert_eq!(multi_line("a\r\nb\rc\td\u{7}e\nf"), "a\nb\nc\tde\nf");
    }
}
