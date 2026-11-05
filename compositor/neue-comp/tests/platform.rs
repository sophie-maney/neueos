//! neue-platform's Wayland driver against the headless compositor (task 2.5, ADR 0033): a
//! Masonry window maps at its size, draws its background through wgpu, and closes when
//! the compositor asks. Needs Mesa's software EGL (llvmpipe) for wgpu's GL backend and
//! the compositor's renderer; CI installs it.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)] // test code

mod harness;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use harness::Server;
use masonry::core::{ErasedAction, NewWidget, WidgetId};
use masonry::peniko::{Blob, Color};
use masonry::widgets::Label;
use neue_comp::{HEADLESS_OUTPUT_NAME, WindowInfo};
use neue_platform::{App, Blur, Ctx, Options, WindowId, WindowSpec};
use smithay::wayland::background_effect::BackgroundEffectSurfaceCachedState;
use smithay::wayland::compositor::RectangleKind;
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

/// Opens a window that asks for blur with 8 px rounded corners, and records whether the
/// compositor offered blur when the app started.
struct BlurApp(Arc<AtomicBool>);

impl App for BlurApp {
    fn on_start(&mut self, cx: &mut Ctx<'_>) {
        self.0.store(cx.has_blur(), Ordering::Relaxed);
        let root = NewWidget::new(Label::new("neue-platform")).erased();
        cx.open_window(WindowSpec {
            app_id: APP_ID.into(),
            background: Color::TRANSPARENT,
            blur: Some(Blur { corner_radius: 8.0 }),
            ..WindowSpec::new("Blur test", (320, 200), root)
        });
    }

    fn on_action(&mut self, _: &mut Ctx<'_>, _: WindowId, _: WidgetId, _: ErasedAction) {}
}

/// The window's blur region as the compositor holds it: whether each of the given points
/// (surface-local) is inside, and the covered area.
fn blur_region(
    server: &Server,
    info: &WindowInfo,
    probes: [(i32, i32); 3],
) -> Option<([bool; 3], i32)> {
    let id = info.id;
    server.with_state(move |state| {
        let window = state.window_by_id(id)?;
        let mut found = None;
        window.with_surfaces(|_, data| {
            let mut cached = data
                .cached_state
                .get::<BackgroundEffectSurfaceCachedState>();
            let Some(region) = cached.current().blur_region.clone() else {
                return;
            };
            // Only additions: the driver sends no subtractions.
            let inside = |x: i32, y: i32| {
                region
                    .rects
                    .iter()
                    .any(|(kind, r)| matches!(kind, RectangleKind::Add) && r.contains((x, y)))
            };
            let area = region.rects.iter().map(|(_, r)| r.size.w * r.size.h).sum();
            found = Some((probes.map(|(x, y)| inside(x, y)), area));
        });
        found
    })
}

#[test]
fn driver_blurs_behind_a_window_that_asks_inside_its_rounded_corners() {
    let server = Server::start_rendering();
    let stream = server.connect_raw();
    let offered = Arc::new(AtomicBool::new(false));
    let seen = offered.clone();
    std::thread::spawn(move || {
        if let Ok(conn) = Connection::from_socket(stream) {
            let _ =
                neue_platform::wayland::run_with_connection(conn, &mut BlurApp(seen), options());
        }
    });

    let window = wait_for("the window to map", || our_window(&server));
    assert!(
        offered.load(Ordering::Relaxed),
        "the compositor offers blur when the app starts"
    );
    // The middle and the corner (0, 0): out; the left edge's middle: in.
    let (inside, area) = wait_for("the blur region", || {
        blur_region(&server, &window, [(160, 100), (0, 0), (0, 100)])
    });
    assert_eq!(inside, [true, false, true]);
    assert!(area > 0 && area < 320 * 200, "rounded: {area} px");
}
