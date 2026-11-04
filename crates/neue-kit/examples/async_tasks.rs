//! neue-kit's background tasks (task 2.7): blocking work with progress and cancelling, a
//! future, and a thread that sends messages on its own.
//!
//! ```sh
//! cargo run -p neue-kit --example async_tasks
//! ```
//!
//! Uses only neue-kit's API, as an app crate would (no `masonry_*`).

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use neue_kit::tokens::metrics::spacing;
use neue_kit::{Application, Button, Cx, Handle, Label, Settings, Stack, Task, Window};

/// Primes are counted below this.
const LIMIT: u64 = 3_000_000;

#[derive(Debug, Default)]
struct Tasks {
    job: Option<Task>,
    started: Option<Instant>,
    status: Handle<Label>,
    clock: Handle<Label>,
    count: Handle<Button<Message>>,
    cancel: Handle<Button<Message>>,
    log: Handle<Stack<Message>>,
}

#[derive(Debug, Clone)]
enum Message {
    /// Start counting primes on a background thread.
    Count,
    /// Progress of the count, in percent.
    Progress(u64),
    /// The count's result.
    Counted(u64),
    Cancel,
    /// Start a future that resolves after a delay.
    Wait,
    Waited(Duration),
    /// Sent every second by a thread started in `start`.
    Tick(u64),
    ClearLog,
}

impl Tasks {
    fn log(&self, cx: &mut Cx<'_, Message>, line: String) {
        self.log.push(cx, Label::new(line).secondary());
    }

    fn set_busy(&self, cx: &mut Cx<'_, Message>, busy: bool) {
        self.count.set_enabled(cx, !busy);
        self.cancel.set_enabled(cx, busy);
    }
}

impl Application for Tasks {
    type Message = Message;

    fn start(&mut self, cx: &mut Cx<'_, Message>) {
        // A thread of our own, sending through the app's sender.
        let tx = cx.sender();
        std::thread::spawn(move || {
            for seconds in 1.. {
                std::thread::sleep(Duration::from_secs(1));
                if !tx.send(Message::Tick(seconds)) {
                    break;
                }
            }
        });

        let buttons = Stack::row()
            .with(
                Button::new("Count primes")
                    .on_press(Message::Count)
                    .bind(&mut self.count),
            )
            .with(
                Button::new("Cancel")
                    .on_press(Message::Cancel)
                    .disabled(true)
                    .bind(&mut self.cancel),
            )
            .with(Button::new("Wait 1 s").on_press(Message::Wait))
            .with(Button::new("Clear log").on_press(Message::ClearLog));
        let root = Stack::column()
            .padding(spacing::L)
            .spacing(spacing::M)
            .with(Label::new("Running for 0 s").bind(&mut self.clock))
            .with(buttons)
            .with(Label::new("Idle").bind(&mut self.status))
            .with(Stack::column().spacing(spacing::XS).bind(&mut self.log));
        cx.open_window(Window::new("Background tasks", (460, 360), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
        match message {
            Message::Count => {
                self.started = Some(Instant::now());
                self.job = Some(cx.spawn(|tx| Message::Counted(count_primes(LIMIT, tx))));
                self.status.set_text(cx, "Counting…");
                self.set_busy(cx, true);
            }
            Message::Progress(percent) => {
                self.status.set_text(cx, format!("Counting… {percent} %"));
            }
            Message::Counted(primes) => {
                let ms = self.started.map_or(0, |t| t.elapsed().as_millis());
                self.job = None;
                self.status.set_text(cx, "Idle");
                self.set_busy(cx, false);
                self.log(cx, format!("{primes} primes below {LIMIT}, in {ms} ms"));
            }
            Message::Cancel => {
                if let Some(job) = self.job.take() {
                    job.cancel();
                }
                self.status.set_text(cx, "Idle");
                self.set_busy(cx, false);
                self.log(cx, "Count cancelled".into());
            }
            Message::Wait => {
                cx.spawn_future(async {
                    let started = Instant::now();
                    Delay::new(Duration::from_secs(1)).await;
                    Message::Waited(started.elapsed())
                });
            }
            Message::Waited(elapsed) => {
                self.log(
                    cx,
                    format!("A future resolved after {} ms", elapsed.as_millis()),
                );
            }
            Message::Tick(seconds) => self.clock.set_text(cx, format!("Running for {seconds} s")),
            Message::ClearLog => self.log.clear(cx),
        }
    }
}

/// Counts primes below `limit` by trial division (slow on purpose), reporting progress
/// every 10 % and stopping early when cancelled.
fn count_primes(limit: u64, tx: &neue_kit::Sender<Message>) -> u64 {
    let step = limit / 10;
    let mut count = 0;
    for n in 2..limit {
        if n % step == 0 {
            if tx.is_cancelled() {
                return count;
            }
            tx.send(Message::Progress(n * 100 / limit));
        }
        if (2..).take_while(|d| d * d <= n).all(|d| n % d != 0) {
            count += 1;
        }
    }
    count
}

/// A future that resolves after `duration`, woken by a timer thread. Any executor can
/// poll it, including neue-kit's.
struct Delay {
    state: Arc<Mutex<(bool, Option<std::task::Waker>)>>,
}

impl Delay {
    fn new(duration: Duration) -> Self {
        let state = Arc::new(Mutex::new((false, None::<std::task::Waker>)));
        let timer = state.clone();
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            if let Ok(mut state) = timer.lock() {
                state.0 = true;
                if let Some(waker) = state.1.take() {
                    waker.wake();
                }
            }
        });
        Self { state }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let Ok(mut state) = self.state.lock() else {
            return Poll::Ready(());
        };
        if state.0 {
            Poll::Ready(())
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let settings = Settings {
        app_id: "org.neueos.examples.Tasks".into(),
        ..Settings::default()
    };
    neue_kit::run(Tasks::default(), settings)?;
    Ok(())
}
