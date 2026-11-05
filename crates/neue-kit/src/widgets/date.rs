//! The date picker (task 2.24c, ADR 0062), in two styles. The textual style is a field of
//! day, month and year segments with a stepper beside it: Left and Right move between the
//! segments, Up and Down (or the stepper) change the one that is selected, digits type into
//! it. The graphical style is a month calendar: a header with the month and its previous,
//! today and next buttons, and a grid of days, Monday first. Both hold a [`Date`] and send
//! it as `on_change` whenever the user changes it.
//!
//! Each style is one leaf Masonry widget that draws itself with
//! `neue_paint::materials::date_picker` and does its own hit testing, like the path control.

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, NodeId, Orientation, Role};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    Calendar, CalendarLayout, CalendarPart, DateField, StepperPart, calendar, calendar_layout,
    calendar_size, date_field, stepper_part_at,
};
use neue_theme::materials::date_picker::{
    FIELD_PADDING, SEGMENT_INSET, SEGMENT_PADDING, STEPPER_GAP,
};
use neue_theme::metrics::control;

use super::text::{TextLine, TextStyle};
use crate::date::{MAX_YEAR, MIN_YEAR, days_in_month};
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;
use crate::{Cx, Date};

/// What a date picker emits when the user changes its date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Changed(pub(crate) Date);

fn accesskit_rect(r: Rect) -> masonry_core::accesskit::Rect {
    masonry_core::accesskit::Rect {
        x0: r.x0,
        y0: r.y0,
        x1: r.x1,
        y1: r.y1,
    }
}

// ---------------------------------------------------------------------------------------
// The textual style
// ---------------------------------------------------------------------------------------

/// A segment of the textual style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Day,
    Month,
    Year,
}

/// The order of the segments, left to right (day, month, year; locales come later).
const ORDER: [Part; 3] = [Part::Day, Part::Month, Part::Year];

impl Part {
    fn digits(self) -> usize {
        match self {
            Self::Day | Self::Month => 2,
            Self::Year => 4,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Month => "Month",
            Self::Year => "Year",
        }
    }

    fn value(self, date: Date) -> i32 {
        match self {
            Self::Day => i32::from(date.day()),
            Self::Month => i32::from(date.month()),
            Self::Year => date.year(),
        }
    }

    /// The smallest and largest value of the segment in `date`'s month.
    fn range(self, date: Date) -> (i32, i32) {
        match self {
            Self::Day => (1, i32::from(days_in_month(date.year(), date.month()))),
            Self::Month => (1, 12),
            Self::Year => (MIN_YEAR, MAX_YEAR),
        }
    }

    /// `date` with this segment set to `value`, kept in range (a day the new month hasn't
    /// is the month's last).
    fn set(self, date: Date, value: i32) -> Date {
        let (lo, hi) = self.range(date);
        let v = value.clamp(lo, hi);
        let next = match self {
            Self::Day => u8::try_from(v).ok().and_then(|d| date.with_day(d)),
            Self::Month => u8::try_from(v)
                .ok()
                .and_then(|m| date.with_month(date.year(), m)),
            Self::Year => date.with_month(v, date.month()),
        };
        next.unwrap_or(date)
    }

    /// `date` with this segment moved by `delta`: days and months wrap round, years stop at
    /// the ends.
    fn step(self, date: Date, delta: i32) -> Date {
        let (lo, hi) = self.range(date);
        let v = self.value(date) + delta;
        match self {
            Self::Day | Self::Month => self.set(date, (v - lo).rem_euclid(hi - lo + 1) + lo),
            Self::Year => self.set(date, v),
        }
    }
}

/// The Masonry widget behind [`crate::DatePicker`].
#[derive(Debug)]
pub(crate) struct DateFieldWidget {
    date: Date,
    label: Option<String>,
    /// The segment that Up, Down and typing change (an index into [`ORDER`]).
    focus: usize,
    /// Digits typed into the focused segment and not yet applied (the year waits for all
    /// four).
    typing: String,
    lines: [TextLine; 3],
    slash: TextLine,
    /// "00" and "0000": the widths the segments are laid out with, so that they don't
    /// move while the digits change.
    sizing: [TextLine; 2],
    replayer: ImagingReplayer,
    field: Rect,
    stepper: Rect,
    rects: [Rect; 3],
    /// The centre of each slash, between two segments.
    slashes: [f64; 2],
    /// The stepper half the pointer went down on.
    pressed: Option<StepperPart>,
    node_ids: Vec<NodeId>,
}

impl DateFieldWidget {
    fn new(date: Date) -> Self {
        let line = |s: String| TextLine::new(s, TextStyle::Body);
        let mut widget = Self {
            date,
            label: None,
            focus: 0,
            typing: String::new(),
            lines: [
                line(String::new()),
                line(String::new()),
                line(String::new()),
            ],
            slash: line("/".to_owned()),
            sizing: [line("00".to_owned()), line("0000".to_owned())],
            replayer: ImagingReplayer::new(),
            field: Rect::ZERO,
            stepper: Rect::ZERO,
            rects: [Rect::ZERO; 3],
            slashes: [0.0; 2],
            pressed: None,
            node_ids: Vec::new(),
        };
        widget.refresh();
        widget
    }

    /// The text each segment shows: the date, except the year while its digits are typed.
    fn refresh(&mut self) {
        for (i, part) in ORDER.iter().enumerate() {
            let text = if part == &Part::Year && i == self.focus && !self.typing.is_empty() {
                format!("{:0>4}", self.typing)
            } else {
                format!("{:0width$}", part.value(self.date), width = part.digits())
            };
            self.lines[i].set_text(text);
        }
    }

    /// Shows `date` from the app: no message goes back.
    pub(crate) fn edit_date(this: &mut WidgetMut<'_, Self>, date: Date) {
        this.widget.date = date;
        this.widget.typing.clear();
        this.widget.refresh();
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// The segment widths: the text and its padding.
    fn segment_widths(
        &mut self,
        text: (
            &mut masonry_core::parley::FontContext,
            &mut masonry_core::parley::LayoutContext<masonry_core::core::BrushIndex>,
        ),
    ) -> ([f64; 3], f64) {
        let (fonts, layouts) = text;
        let two = self.sizing[0]
            .size((&mut *fonts, &mut *layouts))
            .width
            .ceil();
        let four = self.sizing[1]
            .size((&mut *fonts, &mut *layouts))
            .width
            .ceil();
        let slash = self.slash.size((&mut *fonts, &mut *layouts)).width.ceil();
        let pad = 2.0 * SEGMENT_PADDING;
        let widths = ORDER.map(|p| (if p.digits() == 2 { two } else { four }) + pad);
        (widths, slash)
    }

    /// The date changed from the user's input: show it and tell the app.
    fn set_date(&mut self, ctx: &mut EventCtx<'_>, date: Date) {
        if date != self.date {
            self.date = date;
            ctx.submit_action::<Changed>(Changed(date));
        }
        self.refresh();
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    /// Applies what was typed into the year and ends typing.
    fn commit(&mut self, ctx: &mut EventCtx<'_>) {
        if self.typing.is_empty() {
            return;
        }
        let part = ORDER[self.focus];
        let typed = self.typing.parse::<i32>().ok();
        self.typing.clear();
        let date = match typed {
            Some(v) if part == Part::Year => part.set(self.date, v),
            _ => self.date,
        };
        self.set_date(ctx, date);
    }

    fn step(&mut self, ctx: &mut EventCtx<'_>, delta: i32) {
        self.commit(ctx);
        let date = ORDER[self.focus].step(self.date, delta);
        self.set_date(ctx, date);
    }

    fn focus_part(&mut self, ctx: &mut EventCtx<'_>, index: usize) {
        self.commit(ctx);
        self.focus = index.min(ORDER.len() - 1);
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    /// A digit typed into the focused segment. The day and month take effect with each
    /// digit; the year when its four digits are in. When no further digit could belong to
    /// the segment, the focus moves on.
    fn type_digit(&mut self, ctx: &mut EventCtx<'_>, digit: char) {
        let part = ORDER[self.focus];
        let (lo, hi) = part.range(self.date);
        let mut buffer = if self.typing.len() >= part.digits() {
            String::new()
        } else {
            std::mem::take(&mut self.typing)
        };
        buffer.push(digit);
        let mut value: i32 = buffer.parse().unwrap_or(0);
        if part != Part::Year && value > hi {
            // 4 then 5 in a day is not 45: start again with the new digit.
            buffer = digit.to_string();
            value = i32::from(digit as u8 - b'0');
        }
        let complete = buffer.len() >= part.digits() || (part != Part::Year && value * 10 > hi);
        self.typing = buffer;
        if part != Part::Year && value >= lo {
            let date = part.set(self.date, value);
            self.set_date(ctx, date);
        } else if part == Part::Year && complete {
            self.commit(ctx);
        } else {
            self.refresh();
            ctx.request_paint_only();
        }
        if complete {
            self.typing.clear();
            self.refresh();
            if self.focus + 1 < ORDER.len() {
                self.focus += 1;
            }
            ctx.request_paint_only();
            ctx.request_accessibility_update();
        }
    }

    /// The segment nearest `x`.
    fn segment_at(&self, x: f64) -> usize {
        (0..ORDER.len())
            .min_by(|&a, &b| {
                let dist = |i: usize| {
                    let r = self.rects[i];
                    (r.x0 - x).max(x - r.x1).max(0.0)
                };
                dist(a).total_cmp(&dist(b))
            })
            .unwrap_or(0)
    }

    /// Where the segments and the stepper sit (tests click on them).
    #[cfg(test)]
    pub(crate) fn parts(&self) -> ([Rect; 3], Rect) {
        (self.rects, self.stepper)
    }

    /// The focused segment's index in day, month, year order (tests).
    #[cfg(test)]
    pub(crate) fn focused_segment(&self) -> usize {
        self.focus
    }
}

impl UsesProperty<ThemeProp> for DateFieldWidget {}

impl Widget for DateFieldWidget {
    type Action = Changed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let at = ctx.local_position(state.position);
                if self.stepper.contains(at) {
                    self.pressed = Some(stepper_part_at(self.stepper, at.y));
                } else if self.field.contains(at) {
                    let index = self.segment_at(at.x);
                    self.focus_part(ctx, index);
                }
                ctx.capture_pointer();
                ctx.request_focus();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let at = ctx.local_position(state.position);
                if let Some(part) = self.pressed.take()
                    && ctx.is_active()
                    && self.stepper.contains(at)
                    && stepper_part_at(self.stepper, at.y) == part
                {
                    self.step(ctx, if part == StepperPart::Up { 1 } else { -1 });
                }
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) => {
                self.pressed = None;
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
                match &key.key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        self.focus_part(ctx, self.focus.saturating_sub(1));
                    }
                    Key::Named(NamedKey::ArrowRight) => self.focus_part(ctx, self.focus + 1),
                    Key::Named(NamedKey::ArrowUp) => self.step(ctx, 1),
                    Key::Named(NamedKey::ArrowDown) => self.step(ctx, -1),
                    Key::Named(NamedKey::Enter) if !self.typing.is_empty() => self.commit(ctx),
                    Key::Named(NamedKey::Escape | NamedKey::Backspace | NamedKey::Delete)
                        if !self.typing.is_empty() =>
                    {
                        self.typing.clear();
                        self.refresh();
                        ctx.request_paint_only();
                    }
                    Key::Character(c)
                        if !key
                            .modifiers
                            .intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT)
                            && c.len() == 1
                            && c.starts_with(|d: char| d.is_ascii_digit()) =>
                    {
                        if let Some(d) = c.chars().next() {
                            self.type_digit(ctx, d);
                        }
                    }
                    _ => return,
                }
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Increment => self.step(ctx, 1),
            Action::Decrement => self.step(ctx, -1),
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::FocusChanged(false) => {
                // What was half typed is dropped: leaving the field is no way to confirm.
                self.typing.clear();
                self.refresh();
                ctx.request_paint_only();
            }
            Update::DisabledChanged(_) | Update::ActiveChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            Update::FontsChanged => {
                for line in &mut self.lines {
                    line.invalidate();
                }
                self.slash.invalidate();
                for line in &mut self.sizing {
                    line.invalidate();
                }
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
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(match axis {
            Axis::Horizontal => {
                let (widths, slash) = self.segment_widths(ctx.text_contexts());
                widths.iter().sum::<f64>()
                    + 2.0 * slash
                    + 2.0 * FIELD_PADDING
                    + STEPPER_GAP
                    + control::STEPPER_WIDTH
            }
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let (widths, slash) = self.segment_widths(ctx.text_contexts());
        let field_width = widths.iter().sum::<f64>() + 2.0 * slash + 2.0 * FIELD_PADDING;
        self.field = Rect::new(0.0, 0.0, field_width, size.height);
        let stepper_y = ((size.height - control::STEPPER_HEIGHT) / 2.0).round();
        self.stepper = Rect::from_origin_size(
            (field_width + STEPPER_GAP, stepper_y),
            (control::STEPPER_WIDTH, control::STEPPER_HEIGHT),
        );
        let mut x = FIELD_PADDING;
        for (i, w) in widths.iter().enumerate() {
            self.rects[i] = Rect::new(x, SEGMENT_INSET, x + w, size.height - SEGMENT_INSET);
            x += w;
            if i < 2 {
                self.slashes[i] = x + slash / 2.0;
                x += slash;
            }
        }
        let text_height = self.lines[0].size(ctx.text_contexts()).height;
        let baseline = (size.height - text_height) / 2.0 + self.lines[0].baseline();
        ctx.set_baselines(baseline, baseline);
        ctx.set_paint_insets(Insets::uniform(control::FOCUS_RING));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let disabled = ctx.is_disabled();
        let focused = ctx.is_focus_target() && ctx.is_window_focused();
        let spec = DateField {
            segments: &self.rects,
            active: Some(self.focus),
            pressed: self.pressed.filter(|_| ctx.is_active()),
            focused,
            disabled,
        };
        let mut scene = Scene::new(self.stepper.x1.max(size.width), size.height);
        date_field(&mut scene, self.field, self.stepper, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let colors = theme.colors();
        let plain = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        for i in 0..ORDER.len() {
            let colour = if focused && !disabled && i == self.focus {
                colors.selected_label
            } else {
                plain
            };
            let rect = self.rects[i];
            let text = self.lines[i].size(ctx.text_contexts());
            let origin = Vec2::new(
                (rect.x0 + (rect.width() - text.width) / 2.0).round(),
                ((size.height - text.height) / 2.0).round(),
            );
            self.lines[i].paint(painter, Affine::translate(origin), colour);
        }
        for &x in &self.slashes {
            let text = self.slash.size(ctx.text_contexts());
            let origin = Vec2::new(
                (x - text.width / 2.0).round(),
                ((size.height - text.height) / 2.0).round(),
            );
            self.slash.paint(painter, Affine::translate(origin), plain);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        while self.node_ids.len() < ORDER.len() {
            self.node_ids.push(AccessCtx::next_node_id());
        }
        if let Some(label) = &self.label {
            node.set_label(label.clone());
        }
        node.set_orientation(Orientation::Horizontal);
        node.set_value(self.date.long());
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
        let mut children = Vec::new();
        for (i, part) in ORDER.iter().enumerate() {
            let id = self.node_ids[i];
            let (lo, hi) = part.range(self.date);
            let mut segment = Node::new(Role::SpinButton);
            segment.set_label(part.label());
            segment.set_numeric_value(f64::from(part.value(self.date)));
            segment.set_min_numeric_value(f64::from(lo));
            segment.set_max_numeric_value(f64::from(hi));
            segment.set_numeric_value_step(1.0);
            segment.set_bounds(accesskit_rect(self.rects[i]));
            segment.set_selected(i == self.focus);
            children.push(id);
            ctx.tree_update().nodes.push((id, segment));
        }
        node.set_children(children);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.date.to_string())
    }
}

// ---------------------------------------------------------------------------------------
// The graphical style
// ---------------------------------------------------------------------------------------

/// The weekday names, Monday first.
const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// The Masonry widget behind [`crate::CalendarPicker`].
#[derive(Debug)]
pub(crate) struct CalendarWidget {
    date: Date,
    today: Date,
    label: Option<String>,
    /// The month on show (it follows the selection, and the buttons move it alone).
    shown: (i32, u8),
    /// The 42 days of the month on show.
    grid: Vec<Option<Date>>,
    title: TextLine,
    weekdays: Vec<TextLine>,
    /// The numerals 1 to 31, for the cells.
    days: Vec<TextLine>,
    layout: CalendarLayout,
    hovered: Option<CalendarPart>,
    pressed: Option<CalendarPart>,
    replayer: ImagingReplayer,
    node_ids: Vec<NodeId>,
}

impl CalendarWidget {
    fn new(date: Date, today: Date) -> Self {
        let mut widget = Self {
            date,
            today,
            label: None,
            shown: (date.year(), date.month()),
            grid: Vec::new(),
            title: TextLine::new(String::new(), TextStyle::Body),
            weekdays: WEEKDAYS
                .iter()
                .map(|d| TextLine::new((*d).to_owned(), TextStyle::Small))
                .collect(),
            days: (1..=31)
                .map(|d| TextLine::new(d.to_string(), TextStyle::Body))
                .collect(),
            layout: calendar_layout(Rect::from_origin_size((0.0, 0.0), calendar_size())),
            hovered: None,
            pressed: None,
            replayer: ImagingReplayer::new(),
            node_ids: Vec::new(),
        };
        widget.show(date.year(), date.month());
        widget
    }

    /// Puts `month` of `year` on show.
    fn show(&mut self, year: i32, month: u8) {
        self.shown = (year, month);
        self.grid = Date::month_grid(year, month);
        self.title
            .set_text(format!("{} {year}", Date::name_of_month(month)));
    }

    /// Shows `date` from the app: no message goes back.
    pub(crate) fn edit_date(this: &mut WidgetMut<'_, Self>, date: Date) {
        this.widget.date = date;
        this.widget.show(date.year(), date.month());
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Selects `date` for the user: the month follows, the app hears if it is a new date.
    fn select(&mut self, ctx: &mut EventCtx<'_>, date: Date) {
        self.show(date.year(), date.month());
        if date != self.date {
            self.date = date;
            ctx.submit_action::<Changed>(Changed(date));
        }
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    /// The month on show, `delta` months on; the selection stays.
    fn browse(&mut self, ctx: &mut EventCtx<'_>, delta: i32) {
        let moved = Date::new(self.shown.0, self.shown.1, 1).and_then(|d| d.add_months(delta));
        if let Some(first) = moved {
            self.show(first.year(), first.month());
            ctx.request_paint_only();
            ctx.request_accessibility_update();
        }
    }

    fn activate(&mut self, ctx: &mut EventCtx<'_>, part: CalendarPart) {
        match part {
            CalendarPart::Previous => self.browse(ctx, -1),
            CalendarPart::Next => self.browse(ctx, 1),
            CalendarPart::Today => self.select(ctx, self.today),
            CalendarPart::Day(i) => {
                if let Some(Some(date)) = self.grid.get(i).copied() {
                    self.select(ctx, date);
                }
            }
        }
    }

    fn move_by_days(&mut self, ctx: &mut EventCtx<'_>, delta: i64) {
        if let Some(date) = self.date.add_days(delta) {
            self.select(ctx, date);
        }
    }

    fn move_by_months(&mut self, ctx: &mut EventCtx<'_>, delta: i32) {
        if let Some(date) = self.date.add_months(delta) {
            self.select(ctx, date);
        }
    }

    fn cell_of(&self, date: Date) -> Option<usize> {
        self.grid.iter().position(|c| *c == Some(date))
    }

    fn set_hover(&mut self, ctx: &mut EventCtx<'_>, hovered: Option<CalendarPart>) {
        // A day the grid has no date for (the edge of time) does nothing.
        let hovered = hovered.filter(|p| match p {
            CalendarPart::Day(i) => self.grid.get(*i).copied().flatten().is_some(),
            _ => true,
        });
        if self.hovered != hovered {
            self.hovered = hovered;
            ctx.request_paint_only();
        }
    }

    /// The month on show (tests).
    #[cfg(test)]
    pub(crate) fn shown(&self) -> (i32, u8) {
        self.shown
    }

    /// The window-independent centre of a day's cell, if it is on show (tests).
    #[cfg(test)]
    pub(crate) fn cell_centre(&self, date: Date) -> Option<masonry_core::kurbo::Point> {
        Some(self.layout.cells[self.cell_of(date)?].center())
    }

    /// The centre of one of the header's buttons (tests).
    #[cfg(test)]
    pub(crate) fn button_centre(&self, part: CalendarPart) -> masonry_core::kurbo::Point {
        self.layout.rect_of(part).center()
    }
}

impl UsesProperty<ThemeProp> for CalendarWidget {}

impl Widget for CalendarWidget {
    type Action = Changed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let part = self.layout.part_at(ctx.local_position(state.position));
                self.set_hover(ctx, part);
                self.pressed = self.hovered;
                ctx.capture_pointer();
                ctx.request_focus();
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let part = self.layout.part_at(ctx.local_position(current.position));
                self.set_hover(ctx, part);
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let part = self.layout.part_at(ctx.local_position(state.position));
                let pressed = self.pressed.take();
                if ctx.is_active()
                    && let Some(p) = pressed.filter(|&p| part == Some(p))
                {
                    self.activate(ctx, p);
                }
                self.set_hover(ctx, part);
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) | PointerEvent::Leave(..) => {
                self.pressed = None;
                self.set_hover(ctx, None);
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
                match &key.key {
                    Key::Named(NamedKey::ArrowLeft) => self.move_by_days(ctx, -1),
                    Key::Named(NamedKey::ArrowRight) => self.move_by_days(ctx, 1),
                    Key::Named(NamedKey::ArrowUp) => self.move_by_days(ctx, -7),
                    Key::Named(NamedKey::ArrowDown) => self.move_by_days(ctx, 7),
                    Key::Named(NamedKey::PageUp) => self.move_by_months(ctx, -1),
                    Key::Named(NamedKey::PageDown) => self.move_by_months(ctx, 1),
                    Key::Named(NamedKey::Home) => self.select(ctx, self.today),
                    _ => return,
                }
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Increment => self.move_by_days(ctx, 1),
            Action::Decrement => self.move_by_days(ctx, -1),
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) | Update::ActiveChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            Update::HoveredChanged(false) => {
                self.hovered = None;
                ctx.request_paint_only();
            }
            Update::FontsChanged => {
                self.title.invalidate();
                for line in self.weekdays.iter_mut().chain(&mut self.days) {
                    line.invalidate();
                }
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
        let size = calendar_size();
        Length::px(match axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.layout = calendar_layout(Rect::from_origin_size((0.0, 0.0), size));
        ctx.clear_baselines();
        ctx.set_paint_insets(Insets::uniform(control::FOCUS_RING));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let disabled = ctx.is_disabled();
        let selected = self.cell_of(self.date);
        let held = ctx.is_active();
        let spec = Calendar {
            selected,
            today: self.cell_of(self.today),
            focused: selected.filter(|_| ctx.is_focus_target() && ctx.is_window_focused()),
            hovered: self.hovered.filter(|_| !disabled),
            pressed: self.pressed.filter(|_| held),
            disabled,
        };
        let mut scene = Scene::new(size.width, size.height);
        calendar(&mut scene, &self.layout, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let colors = theme.colors();
        let tone = |c| if disabled { colors.disabled_label } else { c };

        // The month, left of the buttons.
        let text = self.title.size(ctx.text_contexts());
        let title = self.layout.title;
        let origin = Vec2::new(
            title.x0 + 2.0,
            (title.center().y - text.height / 2.0).round(),
        );
        self.title
            .paint(painter, Affine::translate(origin), tone(colors.label));

        for (line, cell) in self.weekdays.iter_mut().zip(&self.layout.weekdays) {
            let text = line.size(ctx.text_contexts());
            let origin = Vec2::new(
                (cell.center().x - text.width / 2.0).round(),
                (cell.center().y - text.height / 2.0).round(),
            );
            line.paint(
                painter,
                Affine::translate(origin),
                tone(colors.secondary_label),
            );
        }

        for (i, cell) in self.layout.cells.iter().enumerate() {
            let Some(Some(date)) = self.grid.get(i).copied() else {
                continue;
            };
            let colour = if Some(i) == selected {
                tone(colors.selected_label)
            } else if (date.year(), date.month()) == self.shown {
                tone(colors.label)
            } else {
                tone(colors.tertiary_label)
            };
            let line = &mut self.days[usize::from(date.day()) - 1];
            let text = line.size(ctx.text_contexts());
            let origin = Vec2::new(
                (cell.center().x - text.width / 2.0).round(),
                (cell.center().y - text.height / 2.0).round(),
            );
            line.paint(painter, Affine::translate(origin), colour);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        // 1 grid, 7 rows (the weekday names, then six weeks), 7 headers, 42 days, 3 buttons.
        while self.node_ids.len() < 1 + 7 + 7 + 42 + 3 {
            self.node_ids.push(AccessCtx::next_node_id());
        }
        let ids = self.node_ids.clone();
        let (grid_id, row_ids, header_ids, cell_ids, button_ids) =
            (ids[0], &ids[1..8], &ids[8..15], &ids[15..57], &ids[57..60]);

        if let Some(label) = &self.label {
            node.set_label(label.clone());
        }
        node.set_value(self.date.long());
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);

        let mut group = Vec::new();
        for (id, (part, label)) in button_ids.iter().zip([
            (CalendarPart::Previous, "Previous month"),
            (CalendarPart::Today, "Today"),
            (CalendarPart::Next, "Next month"),
        ]) {
            let mut button = Node::new(Role::Button);
            button.set_label(label);
            button.set_bounds(accesskit_rect(self.layout.rect_of(part)));
            group.push(*id);
            ctx.tree_update().nodes.push((*id, button));
        }

        let mut rows = Vec::new();
        let mut header = Node::new(Role::Row);
        let mut header_cells = Vec::new();
        for (c, id) in header_ids.iter().enumerate() {
            let mut name = Node::new(Role::ColumnHeader);
            name.set_label(WEEKDAYS[c]);
            name.set_bounds(accesskit_rect(self.layout.weekdays[c]));
            header_cells.push(*id);
            ctx.tree_update().nodes.push((*id, name));
        }
        header.set_children(header_cells);
        rows.push(row_ids[0]);
        ctx.tree_update().nodes.push((row_ids[0], header));

        for week in 0..6 {
            let mut row = Node::new(Role::Row);
            let mut cells = Vec::new();
            for col in 0..7 {
                let i = week * 7 + col;
                let Some(Some(date)) = self.grid.get(i).copied() else {
                    continue;
                };
                let mut cell = Node::new(Role::GridCell);
                cell.set_label(date.long());
                cell.set_bounds(accesskit_rect(self.layout.cells[i]));
                cell.set_selected(date == self.date);
                if date == self.today {
                    cell.set_description("Today");
                }
                cells.push(cell_ids[i]);
                ctx.tree_update().nodes.push((cell_ids[i], cell));
            }
            row.set_children(cells);
            rows.push(row_ids[week + 1]);
            ctx.tree_update().nodes.push((row_ids[week + 1], row));
        }

        let mut grid = Node::new(Role::Grid);
        grid.set_label(self.title.text().to_owned());
        grid.set_row_count(7);
        grid.set_column_count(7);
        grid.set_children(rows);
        group.push(grid_id);
        ctx.tree_update().nodes.push((grid_id, grid));
        node.set_children(group);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.date.to_string())
    }
}

// ---------------------------------------------------------------------------------------
// The builders
// ---------------------------------------------------------------------------------------

/// A date picker, textual style: a field of day, month and year segments with a stepper.
/// Left and Right select a segment, Up and Down (or the stepper's halves) change it, and
/// typed digits replace it; the day and month wrap round, the day follows the month's
/// length. The graphical style is [`DatePicker::calendar`].
///
/// ```no_run
/// # use neue_kit::{Date, DatePicker};
/// # #[derive(Clone)] enum Msg { Due(Date) }
/// let picker = DatePicker::new(Date::new(2026, 11, 5).expect("a real date"))
///     .label("Due date")
///     .on_change(Msg::Due);
/// ```
pub struct DatePicker<M> {
    inner: NewWidget<DateFieldWidget>,
    on_change: Option<Box<dyn FnMut(Date) -> M>>,
}

impl<M> std::fmt::Debug for DatePicker<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatePicker")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .finish()
    }
}

impl<M: 'static> DatePicker<M> {
    /// A textual picker showing `date`.
    pub fn new(date: Date) -> Self {
        Self {
            inner: NewWidget::new(DateFieldWidget::new(date)),
            on_change: None,
        }
    }

    /// The graphical style: a calendar of the month with `date` selected, today being the
    /// system's date (UTC; see [`CalendarPicker::today`]).
    pub fn calendar(date: Date) -> CalendarPicker<M> {
        CalendarPicker::new(date)
    }

    /// What assistive technology announces for the picker.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Sends the message `make` returns, given the new date, each time the user changes it.
    /// Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_change(mut self, make: impl FnMut(Date) -> M + 'static) -> Self {
        self.on_change = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this picker.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<DatePicker<M>> for Element<M> {
    fn from(picker: DatePicker<M>) -> Self {
        let id = picker.inner.id();
        let mut element = Element::new(picker.inner.erased());
        if let Some(mut make) = picker.on_change {
            element.on_action(id, move |action| {
                let Changed(date) = *action.downcast_ref::<Changed>()?;
                Some(make(date))
            });
        }
        element
    }
}

impl<M> Handle<DatePicker<M>> {
    /// Shows `date`. Digits half typed are dropped.
    pub fn set_date<N>(&self, cx: &mut Cx<'_, N>, date: Date) {
        cx.edit::<DateFieldWidget, _>(self, |mut w| DateFieldWidget::edit_date(&mut w, date));
    }

    /// Enables or disables the picker.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<DateFieldWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

/// A date picker, graphical style: a month calendar, Monday first. Clicking a day selects
/// it; the arrows in the header page through months without changing the date, and the
/// circle between them selects today. With the keyboard, Left, Right, Up and Down move the
/// selection by a day or a week, Page Up and Page Down by a month and Home to today.
///
/// ```no_run
/// # use neue_kit::{Date, DatePicker};
/// # #[derive(Clone)] enum Msg { Due(Date) }
/// let calendar = DatePicker::calendar(Date::new(2026, 11, 5).expect("a real date"))
///     .on_change(Msg::Due);
/// ```
pub struct CalendarPicker<M> {
    inner: NewWidget<CalendarWidget>,
    on_change: Option<Box<dyn FnMut(Date) -> M>>,
}

impl<M> std::fmt::Debug for CalendarPicker<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalendarPicker")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .finish()
    }
}

impl<M: 'static> CalendarPicker<M> {
    fn new(date: Date) -> Self {
        Self {
            inner: NewWidget::new(CalendarWidget::new(date, Date::today())),
            on_change: None,
        }
    }

    /// Which day is today (ringed, and what the circle button selects). The default is the
    /// system's date in UTC, which is a day off for some of the local day.
    #[must_use]
    pub fn today(mut self, today: Date) -> Self {
        self.inner.widget.today = today;
        self
    }

    /// What assistive technology announces for the calendar.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Sends the message `make` returns, given the new date, each time the user selects a
    /// different one. Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_change(mut self, make: impl FnMut(Date) -> M + 'static) -> Self {
        self.on_change = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this calendar.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<CalendarPicker<M>> for Element<M> {
    fn from(picker: CalendarPicker<M>) -> Self {
        let id = picker.inner.id();
        let mut element = Element::new(picker.inner.erased());
        if let Some(mut make) = picker.on_change {
            element.on_action(id, move |action| {
                let Changed(date) = *action.downcast_ref::<Changed>()?;
                Some(make(date))
            });
        }
        element
    }
}

impl<M> Handle<CalendarPicker<M>> {
    /// Selects `date` and shows its month.
    pub fn set_date<N>(&self, cx: &mut Cx<'_, N>, date: Date) {
        cx.edit::<CalendarWidget, _>(self, |mut w| CalendarWidget::edit_date(&mut w, date));
    }

    /// Enables or disables the calendar.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<CalendarWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).expect("a real date")
    }

    #[test]
    fn days_and_months_wrap_and_years_stop() {
        assert_eq!(Part::Day.step(d(2026, 11, 30), 1), d(2026, 11, 1));
        assert_eq!(Part::Day.step(d(2026, 11, 1), -1), d(2026, 11, 30));
        assert_eq!(Part::Day.step(d(2024, 2, 29), 1), d(2024, 2, 1));
        assert_eq!(Part::Month.step(d(2026, 12, 5), 1), d(2026, 1, 5));
        assert_eq!(Part::Month.step(d(2026, 1, 5), -1), d(2026, 12, 5));
        assert_eq!(Part::Year.step(d(9999, 5, 5), 1), d(9999, 5, 5));
        assert_eq!(Part::Year.step(d(1, 5, 5), -1), d(1, 5, 5));
        assert_eq!(Part::Year.step(d(2026, 5, 5), 1), d(2027, 5, 5));
    }

    #[test]
    fn changing_the_month_or_year_keeps_the_day_in_range() {
        assert_eq!(Part::Month.step(d(2026, 1, 31), 1), d(2026, 2, 28));
        assert_eq!(Part::Month.set(d(2026, 1, 31), 4), d(2026, 4, 30));
        assert_eq!(Part::Year.step(d(2024, 2, 29), 1), d(2025, 2, 28));
        assert_eq!(Part::Year.set(d(2024, 2, 29), 2028), d(2028, 2, 29));
    }

    #[test]
    fn setting_a_segment_clamps_to_its_range() {
        assert_eq!(Part::Day.set(d(2026, 2, 10), 45), d(2026, 2, 28));
        assert_eq!(Part::Day.set(d(2026, 2, 10), 0), d(2026, 2, 1));
        assert_eq!(Part::Month.set(d(2026, 2, 10), 13), d(2026, 12, 10));
        assert_eq!(Part::Year.set(d(2026, 2, 10), 0), d(1, 2, 10));
        assert_eq!(Part::Year.set(d(2026, 2, 10), 20_000), d(9999, 2, 10));
    }
}
