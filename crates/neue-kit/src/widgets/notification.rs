//! Notification banners and alerts (task 3.15b): [`crate::desktop::Notification`] paints
//! the notification material (`neue_paint::materials::notification`) with the app's icon
//! or an image, a bold title and up to two lines of body. An alert adds a column of action
//! buttons. A close button shows over the top left corner while the pointer is over the
//! panel, or while it has keyboard focus.
//!
//! The close and action buttons are child widgets, so each is its own accessibility node
//! and Tab reaches it. The panel itself takes focus too: Space or Return activates it and
//! Escape closes it.

use std::any::TypeId;
use std::borrow::Cow;

use masonry_core::accesskit::{Action, Live, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut, WidgetPod, render_text,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{
    Affine, Axis, Insets, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Size,
};
use masonry_core::layout::{LenReq, Length};
use masonry_core::parley::{
    FontContext, FontFamily, FontWeight, Layout, LayoutContext, StyleProperty,
};
use masonry_core::peniko::Color;
use neue_paint::icon::Icon;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::desktop::Picture;
use neue_paint::materials::focus_ring;
use neue_paint::materials::notification::{self as material, MARGIN, Palette, Parts, WIDTH};
use neue_paint::{Backdrop, Scene};
use neue_theme::materials::notification::{
    ACTION_PADDING, BODY_LINES, MAX_ACTIONS, RADIUS, TITLE_GAP,
};
use neue_theme::metrics::control::FOCUS_RING;
use neue_theme::{Theme, typography};

use crate::theme::ThemeProp;

/// What a notification shows. Actions past
/// `tokens::materials::notification::MAX_ACTIONS` are left out; with none it is a banner,
/// with some an alert.
#[derive(Clone, Debug, Default)]
pub struct NotificationContent {
    /// The summary, in bold on one line.
    pub title: String,
    /// The body, cut to two lines.
    pub body: String,
    /// The app's icon.
    pub app_icon: Option<Icon>,
    /// An image shown in the icon's place; the icon becomes a badge on it.
    pub image: Option<Picture>,
    /// The action buttons' labels, top to bottom.
    pub actions: Vec<String>,
}

/// What the user did to a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationEvent {
    /// Clicked the panel (or pressed Space or Return on it): its default action.
    Activated,
    /// Pressed the close button, or Escape on the panel.
    Closed,
    /// Pressed the action button at this index.
    Action(usize),
    /// The pointer came over the panel (true) or left it (false): pause its timeout.
    Hovered(bool),
}

/// What the panel widget emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PanelEvent(pub(crate) NotificationEvent);

/// What a close or action button emits when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ButtonPressed;

/// Text cut to a number of lines, the last ending in an ellipsis when something was left
/// out.
struct Clamped {
    text: String,
    bold: bool,
    lines: usize,
    width: f64,
    layout: Layout<BrushIndex>,
    stale: bool,
}

impl std::fmt::Debug for Clamped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clamped")
            .field("text", &self.text)
            .field("lines", &self.lines)
            .finish_non_exhaustive()
    }
}

impl Clamped {
    fn new(text: String, bold: bool, lines: usize) -> Self {
        Self {
            text,
            bold,
            lines,
            width: 0.0,
            layout: Layout::new(),
            stale: true,
        }
    }

    fn set_text(&mut self, text: String) {
        if self.text != text {
            self.text = text;
            self.stale = true;
        }
    }

    fn build(
        &mut self,
        (fonts, layouts): (&mut FontContext, &mut LayoutContext<BrushIndex>),
        text: &str,
    ) {
        let family = format!("\"{}\", sans-serif", typography::UI_FAMILY);
        let mut builder = layouts.ranged_builder(fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(typography::BODY));
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(Cow::Owned(
            family,
        ))));
        if self.bold {
            builder.push_default(StyleProperty::FontWeight(FontWeight::BOLD));
        }
        builder.build_into(&mut self.layout, text);
        #[expect(clippy::cast_possible_truncation, reason = "a width in logical pixels")]
        self.layout.break_all_lines(Some(self.width as f32));
    }

    /// Lays the text out `width` wide if anything changed; returns its height.
    fn height(
        &mut self,
        (fonts, layouts): (&mut FontContext, &mut LayoutContext<BrushIndex>),
        width: f64,
    ) -> f64 {
        if self.text.is_empty() {
            return 0.0;
        }
        if self.stale || self.width != width {
            self.width = width;
            let text = self.text.clone();
            self.build((fonts, layouts), &text);
            if self.layout.len() > self.lines {
                // Keep what fits on the allowed lines, then shorten it until an ellipsis
                // fits after it.
                let end = self
                    .layout
                    .get(self.lines - 1)
                    .map_or(text.len(), |line| line.text_range().end);
                let mut kept = trim_end(&text[..end]).to_owned();
                loop {
                    let candidate = format!("{kept}\u{2026}");
                    self.build((fonts, layouts), &candidate);
                    if self.layout.len() <= self.lines || kept.is_empty() {
                        break;
                    }
                    kept.pop();
                    kept.truncate(trim_end(&kept).len());
                }
            }
            self.stale = false;
        }
        f64::from(self.layout.height())
    }

    fn invalidate(&mut self) {
        self.stale = true;
    }

    fn paint(&self, painter: &mut Painter<'_>, origin: Point, color: Color) {
        if !self.text.is_empty() {
            render_text(
                painter,
                Affine::translate(origin.to_vec2()),
                &self.layout,
                &[color.into()],
                true,
            );
        }
    }
}

/// `text` without the spaces and punctuation at its end, which an ellipsis replaces.
fn trim_end(text: &str) -> &str {
    text.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':'))
}

/// Which of a notification's buttons a [`NoteButtonWidget`] is.
#[derive(Debug)]
enum ButtonKind {
    /// The close button over the top left corner.
    Close,
    /// An action button, and which of its right-hand corners are rounded (the panel's).
    Action {
        label: Box<Clamped>,
        round_top: bool,
        round_bottom: bool,
    },
}

/// A notification's close button or one of its action buttons.
#[derive(Debug)]
pub(crate) struct NoteButtonWidget {
    kind: ButtonKind,
    /// The close button shows while the pointer is over the panel.
    shown: bool,
    key_down: bool,
    replayer: ImagingReplayer,
}

impl NoteButtonWidget {
    fn close() -> Self {
        Self {
            kind: ButtonKind::Close,
            shown: false,
            key_down: false,
            replayer: ImagingReplayer::new(),
        }
    }

    fn action(label: String) -> Self {
        Self {
            kind: ButtonKind::Action {
                label: Box::new(Clamped::new(label, false, 1)),
                round_top: false,
                round_bottom: false,
            },
            shown: true,
            key_down: false,
            replayer: ImagingReplayer::new(),
        }
    }

    fn label(&self) -> &str {
        match &self.kind {
            ButtonKind::Close => "Close",
            ButtonKind::Action { label, .. } => &label.text,
        }
    }

    fn visible(&self, ctx: &PaintCtx<'_>) -> bool {
        self.shown || ctx.is_focus_target() || ctx.is_hovered()
    }
}

impl UsesProperty<ThemeProp> for NoteButtonWidget {}

impl Widget for NoteButtonWidget {
    type Action = ButtonPressed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => {
                ctx.capture_pointer();
                ctx.request_paint_only();
                ctx.set_handled();
            }
            PointerEvent::Up(..) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<ButtonPressed>(ButtonPressed);
                }
                ctx.request_paint_only();
                ctx.set_handled();
            }
            PointerEvent::Cancel(..) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(key) = event
            && is_press_key(&key.key)
        {
            if key.state.is_down() {
                self.key_down = true;
            } else if std::mem::take(&mut self.key_down) {
                ctx.submit_action::<ButtonPressed>(ButtonPressed);
            }
            ctx.request_paint_only();
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<ButtonPressed>(ButtonPressed);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::FocusChanged(focused) => {
                if !focused {
                    self.key_down = false;
                }
                ctx.request_paint_only();
            }
            Update::HoveredChanged(_) | Update::ActiveChanged(_) => ctx.request_paint_only(),
            Update::FontsChanged => {
                if let ButtonKind::Action { label, .. } = &mut self.kind {
                    label.invalidate();
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
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The panel lays its buttons out at the sizes it computed.
        Length::px(0.0)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if let ButtonKind::Action { label, .. } = &mut self.kind {
            label.height(ctx.text_contexts(), size.width - 2.0 * ACTION_PADDING);
        }
        ctx.set_paint_insets(Insets::uniform(FOCUS_RING));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let rect = Rect::from_origin_size((0.0, 0.0), size);
        let hot = (ctx.is_active() && ctx.is_hovered()) || self.key_down;
        let focused = ctx.is_focus_target() && ctx.is_window_focused();
        let mut scene = Scene::new(size.width, size.height);
        let p = Palette::new(theme.variant, Backdrop::Opaque);
        match &self.kind {
            ButtonKind::Close => {
                if self.visible(ctx) {
                    material::close_button(
                        &mut scene,
                        rect,
                        hot || ctx.is_hovered(),
                        theme.variant,
                    );
                    if focused {
                        let r = size.width / 2.0;
                        focus_ring(&mut scene, RoundedRect::from_rect(rect, r), theme.variant);
                    }
                }
            }
            ButtonKind::Action {
                round_top,
                round_bottom,
                ..
            } => {
                let r = |round: bool| if round { RADIUS } else { 0.0 };
                let shape = RoundedRect::from_rect(
                    rect,
                    RoundedRectRadii::new(0.0, r(*round_top), r(*round_bottom), 0.0),
                );
                if hot {
                    scene.fill(&shape, p.action_pressed);
                }
                if focused {
                    // Inside the button: outside it is the panel's edge or its neighbour.
                    let inner = RoundedRect::from_rect(
                        rect.inset(-(FOCUS_RING + 1.0)),
                        RoundedRectRadii::new(
                            2.0,
                            r(*round_top) / 2.0 + 2.0,
                            r(*round_bottom) / 2.0 + 2.0,
                            2.0,
                        ),
                    );
                    focus_ring(&mut scene, inner, theme.variant);
                }
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
        if let ButtonKind::Action { label, .. } = &self.kind {
            let width = f64::from(label.layout.width());
            let height = f64::from(label.layout.height());
            let origin = Point::new(
                ((size.width - width) / 2.0).round(),
                ((size.height - height) / 2.0).round(),
            );
            label.paint(painter, origin, p.action);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.label());
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.label().to_owned())
    }
}

fn is_press_key(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ") || *key == Key::Named(NamedKey::Enter)
}

/// The Masonry widget behind [`crate::desktop::Notification`].
#[derive(Debug)]
pub(crate) struct NotificationWidget {
    title: Clamped,
    body: Clamped,
    app_icon: Option<Icon>,
    image: Option<Picture>,
    /// Action buttons shown (at most `MAX_ACTIONS`).
    shown_actions: usize,
    pub(super) backdrop: Backdrop,
    close: WidgetPod<NoteButtonWidget>,
    actions: Vec<WidgetPod<NoteButtonWidget>>,
    /// The pointer is over the panel or one of its buttons.
    hovered: bool,
    key_down: bool,
    replayer: ImagingReplayer,
}

impl NotificationWidget {
    pub(crate) fn new(content: NotificationContent) -> Self {
        let shown_actions = content.actions.len().min(MAX_ACTIONS);
        let mut labels = content.actions.into_iter();
        Self {
            title: Clamped::new(content.title, true, 1),
            body: Clamped::new(content.body, false, BODY_LINES),
            app_icon: content.app_icon,
            image: content.image,
            shown_actions,
            backdrop: Backdrop::Opaque,
            close: NewWidget::new(NoteButtonWidget::close()).to_pod(),
            actions: (0..MAX_ACTIONS)
                .map(|_| {
                    let label = labels.next().unwrap_or_default();
                    NewWidget::new(NoteButtonWidget::action(label)).to_pod()
                })
                .collect(),
            hovered: false,
            key_down: false,
            replayer: ImagingReplayer::new(),
        }
    }

    /// The close button's id, for its message handler.
    pub(crate) fn close_id(&self) -> WidgetId {
        self.close.id()
    }

    /// The action buttons' ids, top to bottom, for their message handlers.
    pub(crate) fn action_ids(&self) -> Vec<WidgetId> {
        self.actions.iter().map(WidgetPod::id).collect()
    }

    /// Shows `content` in place (a replacement).
    pub(crate) fn edit_content(this: &mut WidgetMut<'_, Self>, content: NotificationContent) {
        let widget = &mut *this.widget;
        widget.title.set_text(content.title);
        widget.body.set_text(content.body);
        widget.app_icon = content.app_icon;
        widget.image = content.image;
        widget.shown_actions = content.actions.len().min(MAX_ACTIONS);
        let mut labels = content.actions.into_iter();
        for i in 0..MAX_ACTIONS {
            let text = labels.next().unwrap_or_default();
            let mut button = this.ctx.get_mut(&mut this.widget.actions[i]);
            if let ButtonKind::Action { label, .. } = &mut button.widget.kind {
                label.set_text(text);
            }
            button.ctx.request_layout();
            button.ctx.request_accessibility_update();
        }
        this.ctx.request_layout();
        this.ctx.request_accessibility_update();
    }

    fn has_picture(&self) -> bool {
        self.app_icon.is_some() || self.image.is_some()
    }

    /// The panel's rect in the widget, `height` tall.
    fn panel(height: f64) -> Rect {
        Rect::new(MARGIN, MARGIN, MARGIN + WIDTH, MARGIN + height)
    }

    /// Lays the text out; returns the panel's height.
    fn panel_height(
        &mut self,
        contexts: (&mut FontContext, &mut LayoutContext<BrushIndex>),
    ) -> f64 {
        let (fonts, layouts) = contexts;
        let width = Parts::text_width(self.has_picture(), self.shown_actions);
        let title = self.title.height((fonts, layouts), width);
        let body = self.body.height((fonts, layouts), width);
        let gap = if title > 0.0 && body > 0.0 {
            TITLE_GAP
        } else {
            0.0
        };
        material::panel_height(title + gap + body)
    }

    fn parts(&self, size: Size) -> Parts {
        Parts::new(
            Self::panel(size.height - 2.0 * MARGIN),
            self.has_picture(),
            self.shown_actions,
        )
    }

    fn emit(ctx: &mut EventCtx<'_>, event: NotificationEvent) {
        ctx.submit_action::<PanelEvent>(PanelEvent(event));
    }
}

impl UsesProperty<ThemeProp> for NotificationWidget {}

impl Widget for NotificationWidget {
    type Action = PanelEvent;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let on_panel = |ctx: &EventCtx<'_>, position| {
            let size = ctx.content_box().size();
            let at = ctx.local_position(position);
            material::outline(Self::panel(size.height - 2.0 * MARGIN)).contains(at)
        };
        match event {
            PointerEvent::Down(down) if on_panel(ctx, down.state.position) => {
                ctx.capture_pointer();
                ctx.set_handled();
            }
            PointerEvent::Up(up) => {
                if ctx.is_active() && on_panel(ctx, up.state.position) {
                    Self::emit(ctx, NotificationEvent::Activated);
                }
                ctx.set_handled();
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
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if !ctx.is_focus_target() {
            return;
        }
        if is_press_key(&key.key) {
            if key.state.is_down() {
                self.key_down = true;
            } else if std::mem::take(&mut self.key_down) {
                Self::emit(ctx, NotificationEvent::Activated);
            }
            ctx.set_handled();
        } else if key.key == Key::Named(NamedKey::Escape) && key.state.is_down() {
            Self::emit(ctx, NotificationEvent::Closed);
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        // A click on a button inside bubbles up here too.
        if event.action == Action::Click && ctx.target() == ctx.widget_id() {
            Self::emit(ctx, NotificationEvent::Activated);
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.close);
        for action in &mut self.actions {
            ctx.register_child(action);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::ChildHoveredChanged(hovered) => {
                let hovered = *hovered;
                if self.hovered != hovered {
                    self.hovered = hovered;
                    ctx.mutate_child_later(&mut self.close, move |mut close| {
                        close.widget.shown = hovered;
                        close.ctx.request_paint_only();
                    });
                    ctx.submit_action::<PanelEvent>(PanelEvent(NotificationEvent::Hovered(
                        hovered,
                    )));
                }
            }
            Update::FocusChanged(focused) => {
                if !focused {
                    self.key_down = false;
                }
                ctx.request_paint_only();
            }
            Update::FontsChanged => {
                self.title.invalidate();
                self.body.invalidate();
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
            Axis::Horizontal => WIDTH + 2.0 * MARGIN,
            Axis::Vertical => self.panel_height(ctx.text_contexts()) + 2.0 * MARGIN,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.panel_height(ctx.text_contexts());
        let parts = self.parts(size);
        ctx.run_layout(&mut self.close, parts.close.size());
        ctx.place_child(&mut self.close, parts.close.origin());
        let last = parts.actions.len().saturating_sub(1);
        for (i, pod) in self.actions.iter_mut().enumerate() {
            let rect = parts.actions.get(i).copied();
            ctx.set_stashed(pod, rect.is_none());
            if let Some(rect) = rect {
                ctx.mutate_child_later(pod, move |mut button| {
                    if let ButtonKind::Action {
                        round_top,
                        round_bottom,
                        ..
                    } = &mut button.widget.kind
                        && (*round_top != (i == 0) || *round_bottom != (i == last))
                    {
                        *round_top = i == 0;
                        *round_bottom = i == last;
                        button.ctx.request_paint_only();
                    }
                });
                ctx.run_layout(pod, rect.size());
                ctx.place_child(pod, rect.origin());
            }
        }
        ctx.set_paint_insets(Insets::uniform(FOCUS_RING));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let parts = self.parts(size);
        let mut scene = Scene::new(size.width, size.height);
        material::panel(&mut scene, &parts, theme.variant, self.backdrop);
        if let Some(icon) = parts.icon {
            material::picture(
                &mut scene,
                icon,
                self.app_icon.as_ref(),
                self.image.as_ref(),
                theme.variant,
                self.backdrop,
            );
        }
        if ctx.is_focus_target() && ctx.is_window_focused() {
            focus_ring(&mut scene, material::outline(parts.panel), theme.variant);
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let p = palette(&theme, self.backdrop);
        let mut y = parts.text.y0;
        let title = f64::from(self.title.layout.height());
        if !self.title.text.is_empty() {
            self.title
                .paint(painter, Point::new(parts.text.x0, y), p.title);
            y += title + TITLE_GAP;
        }
        self.body
            .paint(painter, Point::new(parts.text.x0, y), p.body);
    }

    fn accessibility_role(&self) -> Role {
        if self.shown_actions > 0 {
            Role::Alert
        } else {
            Role::Group
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.title.text.clone());
        if !self.body.text.is_empty() {
            node.set_description(self.body.text.clone());
        }
        node.set_live(Live::Polite);
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        std::iter::once(self.close.id())
            .chain(self.actions.iter().map(WidgetPod::id))
            .collect()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.title.text.clone())
    }
}

fn palette(theme: &Theme, backdrop: Backdrop) -> Palette {
    Palette::new(theme.variant, backdrop)
}
