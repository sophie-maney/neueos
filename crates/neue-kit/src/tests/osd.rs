//! The on-screen display (3.14a) on the fake host: it fades in once on screen, shows
//! what it's told, fades out, snaps under reduced motion, and says what it shows to
//! assistive technology.

use masonry_core::accesskit::Role;
use neue_theme::materials::osd::{RADIUS, SIZE};

use super::*;
use crate::desktop::{Anchor, LayerSpec, Osd, OsdGlyph};
use crate::widgets::OsdWidget;

#[derive(Default)]
struct Display {
    osd: Handle<Osd>,
    window: Option<WindowId>,
}

/// `Some`: show this; `None`: hide.
type Show = Option<(OsdGlyph, f64)>;

impl Application for Display {
    type Message = Show;

    fn start(&mut self, cx: &mut Cx<'_, Show>) {
        let osd = Osd::new(OsdGlyph::Volume, 0.5)
            .fade_in()
            .backdrop(cx.backdrop())
            .bind(&mut self.osd);
        #[expect(clippy::cast_possible_truncation, reason = "the OSD's 200 px side")]
        let side = SIZE as u32;
        let layer = LayerSpec::new("neue-osd", Anchor::BOTTOM);
        let window = Window::new("Volume", (side, side), osd)
            .layer(layer)
            .blur_behind_rounded(RADIUS);
        self.window = Some(cx.open_window(window));
    }

    fn update(&mut self, cx: &mut Cx<'_, Show>, message: Show) {
        match message {
            Some((glyph, level)) => self.osd.show(cx, glyph, level),
            None => self.osd.hide(cx),
        }
    }
}

fn setup(theme: Theme, blur: bool) -> (Runner<Display>, FakeHost) {
    let mut runner = Runner::new(
        Display::default(),
        Settings {
            theme,
            ..Settings::default()
        },
    );
    let mut host = FakeHost {
        theme,
        blur,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    let _ = host.only().render();
    (runner, host)
}

fn opacity(runner: &Runner<Display>, host: &mut FakeHost) -> f64 {
    host.only()
        .get_widget_with_id(id(&runner.app().osd))
        .downcast::<OsdWidget>()
        .expect("the OSD")
        .inner()
        .opacity()
}

/// Steps animation frames until nothing moves; returns how many it took.
fn settle(host: &mut FakeHost) -> u32 {
    let mut frames = 0;
    while host.only().render_root().needs_anim() {
        host.only().animate_ms(16);
        frames += 1;
        assert!(frames < 200, "the fade never ended");
    }
    frames
}

#[test]
fn it_fades_in_then_out() {
    let (mut runner, mut host) = setup(Theme::default(), false);
    assert!(opacity(&runner, &mut host) < 1.0, "starts transparent");
    assert!(settle(&mut host) > 3, "fades in over several frames");
    assert!((opacity(&runner, &mut host) - 1.0).abs() < 1e-6);

    runner.send(&mut host, None);
    host.only().animate_ms(16);
    let mid = opacity(&runner, &mut host);
    assert!(mid > 0.0 && mid < 1.0, "fading: {mid}");
    settle(&mut host);
    assert!(opacity(&runner, &mut host).abs() < 1e-6);

    // Shown again before the surface closes: back in.
    runner.send(&mut host, Some((OsdGlyph::Brightness, 0.25)));
    settle(&mut host);
    assert!((opacity(&runner, &mut host) - 1.0).abs() < 1e-6);
}

#[test]
fn reduced_motion_shows_and_hides_without_a_fade() {
    let theme = Theme {
        reduced_motion: true,
        ..Theme::default()
    };
    let (mut runner, mut host) = setup(theme, false);
    assert!((opacity(&runner, &mut host) - 1.0).abs() < 1e-6);
    runner.send(&mut host, None);
    assert!(opacity(&runner, &mut host).abs() < 1e-6);
    assert!(!host.only().render_root().needs_anim());
}

#[test]
fn it_tells_assistive_technology_what_it_shows() {
    let (mut runner, mut host) = setup(Theme::default(), false);
    let osd = id(&runner.app().osd);
    let node = host.only().access_node(osd).expect("a node");
    assert_eq!(node.role(), Role::Meter);
    assert_eq!(node.label().as_deref(), Some("Volume"));
    assert_eq!(node.numeric_value(), Some(50.0));

    runner.send(&mut host, Some((OsdGlyph::Muted, 0.5)));
    let _ = host.only().render();
    let node = host.only().access_node(osd).expect("a node");
    assert_eq!(node.label().as_deref(), Some("Volume muted"));
    assert_eq!(node.numeric_value(), None);

    runner.send(&mut host, None);
    let _ = host.only().render();
    let node = host.only().access_node(osd).expect("a node");
    assert!(node.is_hidden());
}

#[test]
fn it_asks_for_blur_cut_to_its_corners() {
    let (runner, host) = setup(Theme::default(), true);
    let window = runner.app().window.expect("open");
    assert!((host.blurs[&window].corner_radius - RADIUS).abs() < f64::EPSILON);
}

#[test]
fn its_corners_are_clear_on_a_layer_surface() {
    let (runner, mut host) = setup(Theme::default(), false);
    settle(&mut host);
    let window = runner.app().window.expect("open");
    let harness = host.windows.get_mut(&window).expect("an open window");
    let image = harness.render();
    // The harness paints its own backdrop where the window leaves pixels clear.
    assert_eq!(
        image.get_pixel(1, 1).0,
        [0x29, 0x29, 0x29, 0xff],
        "the corner shows what is below"
    );
    assert_ne!(
        image.get_pixel(100, 30).0,
        [0x29, 0x29, 0x29, 0xff],
        "the square"
    );
}
