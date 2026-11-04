//! `xdg_popup` lifecycle (bug B5, task 1.21).
//!
//! One test per binary: it installs its own global tracing subscriber to count the
//! compositor's ERROR events.

// Test code: a failed step should fail the test with a message, like in `#[test]` fns.
#![allow(clippy::unwrap_used)]

mod harness;

use std::fmt::Write as _;
use std::sync::Mutex;

use harness::Server;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// Messages of every ERROR event logged in this test binary.
static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct RecordErrors;

impl<S: Subscriber> Layer<S> for RecordErrors {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if *event.metadata().level() != Level::ERROR {
            return;
        }
        let mut message = format!("{}: ", event.metadata().target());
        event.record(&mut MessageVisitor(&mut message));
        ERRORS.lock().unwrap().push(message);
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

/// B5: GTK closes a menu by destroying its `xdg_popup` and `xdg_surface` and then
/// committing the bare `wl_surface`, which it keeps for the next menu. That is valid, and
/// must not log `surface missing from known popups`.
#[test]
fn committing_a_closed_popups_surface_logs_no_error() {
    let subscriber = tracing_subscriber::registry().with(RecordErrors);
    tracing::subscriber::set_global_default(subscriber).expect("first subscriber");

    let server = Server::start();
    let mut client = server.connect();
    let window = client.create_toplevel("Menus", "org.example.Menus", (300, 200));
    let menu = client.create_popup(&window, (120, 80));
    let surface = client.close_popup_keep_surface(menu);
    // A later commit too, e.g. a frame callback request before the surface is reused.
    surface.commit();
    client.roundtrip();

    let errors = ERRORS.lock().unwrap().clone();
    assert!(
        errors.is_empty(),
        "the compositor logged errors: {errors:#?}"
    );
}
