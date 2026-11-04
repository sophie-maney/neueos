//! The widget gallery (task 2.9): a page per widget, with switches for appearance, accent,
//! contrast, reduced motion and zoom. Run it with `cargo xtask gallery`.
//!
//! [`specimens`] are the exact widgets the golden tests (`tests/golden.rs`) render, so what
//! the gallery shows and what the goldens pin down are the same thing. Every widget task
//! adds a page here and its specimens' goldens (ADR 0037).

pub mod pages;
pub mod specimens;

use neue_kit::tokens::metrics::spacing;
use neue_kit::{
    Accent, Align, Appearance, Application, Button, Contrast, Cx, Handle, Label, Stack, TextStyle,
    Theme, Window, Zoom,
};

/// Zoom factors the scale switch cycles through.
const ZOOMS: [f64; 3] = [1.0, 1.5, 2.0];

/// The gallery's model.
#[derive(Debug, Default)]
pub struct Gallery {
    page: usize,
    zoom: usize,
    page_buttons: Vec<Handle<Button<Message>>>,
    handles: pages::PageHandles,
    content: Handle<Stack<Message>>,
    zoomed: Handle<Zoom<Message>>,
    status: Handle<Label>,
    appearance: Handle<Button<Message>>,
    accent: Handle<Button<Message>>,
    contrast: Handle<Button<Message>>,
    motion: Handle<Button<Message>>,
    scale: Handle<Button<Message>>,
}

/// Everything that can happen in the gallery.
#[derive(Debug, Clone)]
pub enum Message {
    /// Show page `n` of [`pages::PAGES`].
    Page(usize),
    ToggleAppearance,
    ToggleAccent,
    ToggleContrast,
    ToggleMotion,
    CycleZoom,
    /// A specimen was pressed.
    Pressed(&'static str),
    /// Radio button `n` of the Buttons page was selected.
    Radio(usize),
    /// The Values page's slider moved to this value (0 to 1).
    Slider(f64),
    /// The Values page's stepper changed to this value.
    Stepper(f64),
}

fn appearance_label(theme: Theme) -> String {
    let name = match theme.variant.appearance {
        Appearance::Light => "Light",
        Appearance::Dark => "Dark",
    };
    format!("Appearance: {name}")
}

fn accent_label(theme: Theme) -> String {
    let name = match theme.variant.accent {
        Accent::Blue => "Blue",
        Accent::Graphite => "Graphite",
    };
    format!("Accent: {name}")
}

fn contrast_label(theme: Theme) -> String {
    let name = match theme.variant.contrast {
        Contrast::Standard => "Standard",
        Contrast::Increased => "Increased",
    };
    format!("Contrast: {name}")
}

fn motion_label(theme: Theme) -> String {
    let name = if theme.reduced_motion {
        "Reduced"
    } else {
        "Full"
    };
    format!("Motion: {name}")
}

fn zoom_label(zoom: f64) -> String {
    format!("Zoom: {zoom}×")
}

fn page_label(title: &str, current: bool) -> String {
    if current {
        format!("› {title}")
    } else {
        title.to_owned()
    }
}

impl Gallery {
    /// A gallery that opens on page `n` (clamped), for screenshots.
    pub fn on_page(n: usize) -> Self {
        Self {
            page: n.min(pages::PAGES.len() - 1),
            ..Self::default()
        }
    }
}

impl Application for Gallery {
    type Message = Message;

    fn start(&mut self, cx: &mut Cx<'_, Message>) {
        let theme = cx.theme();
        let mut sidebar = Stack::column()
            .padding(spacing::L)
            .spacing(spacing::S)
            .align(Align::Fill)
            .with(Label::new("Widgets").style(TextStyle::Headline));
        self.page_buttons = vec![Handle::default(); pages::PAGES.len()];
        for (n, (page, handle)) in pages::PAGES.iter().zip(&mut self.page_buttons).enumerate() {
            sidebar = sidebar.with(
                Button::new(page_label(page.title, n == self.page))
                    .on_press(Message::Page(n))
                    .bind(handle),
            );
        }

        let switches = Stack::row()
            .spacing(spacing::S)
            .with(
                Button::new(appearance_label(theme))
                    .on_press(Message::ToggleAppearance)
                    .bind(&mut self.appearance),
            )
            .with(
                Button::new(accent_label(theme))
                    .on_press(Message::ToggleAccent)
                    .bind(&mut self.accent),
            )
            .with(
                Button::new(contrast_label(theme))
                    .on_press(Message::ToggleContrast)
                    .bind(&mut self.contrast),
            )
            .with(
                Button::new(motion_label(theme))
                    .on_press(Message::ToggleMotion)
                    .bind(&mut self.motion),
            )
            .with(
                Button::new(zoom_label(ZOOMS[self.zoom]))
                    .on_press(Message::CycleZoom)
                    .bind(&mut self.scale),
            );
        let content = Stack::column()
            .with((pages::PAGES[self.page].build)(&mut self.handles))
            .bind(&mut self.content);
        let main = Stack::column()
            .padding(spacing::L)
            .spacing(spacing::M)
            .with(switches)
            .with(
                Label::new("Press a specimen…")
                    .secondary()
                    .bind(&mut self.status),
            )
            .with(Zoom::new(ZOOMS[self.zoom], content).bind(&mut self.zoomed));
        let root = Stack::row().with(sidebar).with(main);
        cx.open_window(Window::new("Widget Gallery", (860, 560), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        let mut theme = cx.theme();
        match message {
            Message::Page(n) if n != self.page && n < pages::PAGES.len() => {
                for (i, handle) in self.page_buttons.iter().enumerate() {
                    handle.set_label(cx, page_label(pages::PAGES[i].title, i == n));
                }
                self.page = n;
                self.content.clear(cx);
                self.content
                    .push(cx, (pages::PAGES[n].build)(&mut self.handles));
            }
            Message::Page(_) => {}
            Message::ToggleAppearance => {
                theme.variant.appearance = match theme.variant.appearance {
                    Appearance::Light => Appearance::Dark,
                    Appearance::Dark => Appearance::Light,
                };
                self.appearance.set_label(cx, appearance_label(theme));
            }
            Message::ToggleAccent => {
                theme.variant.accent = match theme.variant.accent {
                    Accent::Blue => Accent::Graphite,
                    Accent::Graphite => Accent::Blue,
                };
                self.accent.set_label(cx, accent_label(theme));
            }
            Message::ToggleContrast => {
                theme.variant.contrast = match theme.variant.contrast {
                    Contrast::Standard => Contrast::Increased,
                    Contrast::Increased => Contrast::Standard,
                };
                self.contrast.set_label(cx, contrast_label(theme));
            }
            Message::ToggleMotion => {
                theme.reduced_motion = !theme.reduced_motion;
                self.motion.set_label(cx, motion_label(theme));
            }
            Message::CycleZoom => {
                self.zoom = (self.zoom + 1) % ZOOMS.len();
                let zoom = ZOOMS[self.zoom];
                self.zoomed.set_factor(cx, zoom);
                self.scale.set_label(cx, zoom_label(zoom));
            }
            Message::Pressed(what) => self.status.set_text(cx, format!("Pressed “{what}”")),
            Message::Slider(value) => {
                self.handles.slider_progress.set_value(cx, Some(value));
                self.handles
                    .slider_value
                    .set_text(cx, format!("{:.0}%", value * 100.0));
            }
            Message::Stepper(value) => {
                self.handles.stepper_value.set_text(cx, format!("{value}"));
            }
            Message::Radio(n) => {
                // The app owns the choice: the others turn off.
                for (i, radio) in self.handles.radios.iter().enumerate() {
                    radio.set_selected(cx, i == n);
                }
                if let Some(name) = pages::RADIO_NAMES.get(n) {
                    self.status.set_text(cx, format!("Pressed “{name}”"));
                }
            }
        }
        cx.set_theme(theme);
    }
}
