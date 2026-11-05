//! neue-platform's Wayland driver against the headless compositor (task 2.5, ADR 0033): a
//! Masonry window maps at its size, draws its background through wgpu, and closes when
//! the compositor asks. Needs Mesa's software EGL (llvmpipe) for wgpu's GL backend and
//! the compositor's renderer; CI installs it.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use harness::Server;
use masonry::core::{ErasedAction, NewWidget, WidgetId};
use masonry::peniko::{Blob, Color};
use masonry::widgets::Label;
use neue_comp::{HEADLESS_OUTPUT_NAME, WindowInfo};
use neue_platform::{App, Ctx, Options, WindowId, WindowSpec};
use wayland_client::Connection;

const APP_ID: &str = "org.neueos.PlatformTest";
/// The window's background, as 8-bit ARGB.
const BACKGROUND: u32 = 0xff2040c0;

struct TestApp;

impl App for TestApp {
    fn on_start(&mut self, cx: &mut Ctx<'_>) {
        let root = NewWidget::new(Label::new("neue-platform")).erased();
        cx.open_window(WindowSpec {
            app_id: APP_ID.into(),
            background: Color::from_rgb8(0x20, 0x40, 0xc0),
            ..WindowSpec::new("Platform test", (320, 200), root)
        });
    }

    fn on_action(&mut self, _: &mut Ctx<'_>, _: WindowId, _: WidgetId, _: ErasedAction) {}
}

fn options() -> Options {
    static FONT: &[u8] = include_bytes!("../../../crates/neue-paint/fonts/DejaVuSans.ttf");
    Options {
        default_properties: Arc::new(masonry::theme::default_property_set()),
        font: Some(Blob::new(Arc::new(FONT))),
        extra_fonts: Vec::new(),
    }
}

/// Polls `f` until it returns something, failing the test after 30 s (llvmpipe is slow
/// to compile vello's shaders).
fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn our_window(server: &Server) -> Option<WindowInfo> {
    server
        .with_state(|state| state.windows())
        .into_iter()
        .find(|w| w.app_id.as_deref() == Some(APP_ID))
}

#[test]
fn driver_maps_draws_and_closes_a_window() {
    let server = Server::start_rendering();
    let stream = server.connect_raw();
    let (done_tx, done) = mpsc::channel();
    std::thread::spawn(move || {
        let result = Connection::from_socket(stream)
            .map_err(|err| err.to_string())
            .and_then(|conn| {
                neue_platform::wayland::run_with_connection(conn, &mut TestApp, options())
                    .map_err(|err| err.to_string())
            });
        let _ = done_tx.send(result);
    });

    let window = wait_for("the window to map", || our_window(&server));
    assert_eq!(window.title.as_deref(), Some("Platform test"));
    assert_eq!((window.width, window.height), (320, 200), "{window:#?}");

    // The headless output is at scale 1, so logical pixels are output pixels. The label
    // sits top-left; the bottom-right corner shows the background wgpu cleared to.
    let (x, y) = (window.x + 310, window.y + 190);
    wait_for("the window's background", || {
        let pixels = server
            .with_state(|state| state.headless_render(HEADLESS_OUTPUT_NAME))
            .expect("rendering the headless output");
        (pixels.pixel(x, y) == BACKGROUND).then_some(())
    });

    // The close button: the app's default `on_close_requested` closes the window, and the
    // loop ends with its last window.
    server
        .ipc()
        .action("close-window", Some(window.id))
        .expect("asking the window to close");
    let result = done
        .recv_timeout(Duration::from_secs(10))
        .expect("the driver to return after its last window closed");
    assert_eq!(result, Ok(()));
    wait_for("the window to unmap", || {
        our_window(&server).is_none().then_some(())
    });
}
