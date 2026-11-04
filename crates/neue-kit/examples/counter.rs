//! neue-kit's counter: the Elm-style app model in its smallest form (task 2.7).
//!
//! ```sh
//! cargo run -p neue-kit --example counter
//! ```
//!
//! Uses only neue-kit's API, as an app crate would (no `masonry_*`).

use neue_kit::tokens::metrics::spacing;
use neue_kit::{Align, Application, Button, Cx, Handle, Label, Settings, Stack, TextStyle, Window};

/// The model.
#[derive(Debug, Default)]
struct Counter {
    count: i64,
    value: Handle<Label>,
    decrement: Handle<Button<Message>>,
}

/// Everything that can happen.
#[derive(Debug, Clone)]
enum Message {
    Increment,
    Decrement,
    Reset,
}

impl Application for Counter {
    type Message = Message;

    fn start(&mut self, cx: &mut Cx<'_, Message>) {
        let buttons = Stack::row()
            .with(
                Button::new("−")
                    .on_press(Message::Decrement)
                    .disabled(true)
                    .bind(&mut self.decrement),
            )
            .with(Button::new("+").on_press(Message::Increment))
            .with(Button::new("Reset").on_press(Message::Reset));
        let root = Stack::column()
            .padding(spacing::L)
            .spacing(spacing::M)
            .align(Align::Center)
            .with(
                Label::new("0")
                    .style(TextStyle::Display)
                    .bind(&mut self.value),
            )
            .with(buttons)
            .with(Label::new("The counter doesn't go below zero.").secondary());
        cx.open_window(Window::new("Counter", (320, 160), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        match message {
            Message::Increment => self.count += 1,
            Message::Decrement => self.count = (self.count - 1).max(0),
            Message::Reset => self.count = 0,
        }
        self.value.set_text(cx, self.count.to_string());
        self.decrement.set_enabled(cx, self.count > 0);
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let settings = Settings {
        app_id: "org.neueos.examples.Counter".into(),
        ..Settings::default()
    };
    neue_kit::run(Counter::default(), settings)?;
    Ok(())
}
