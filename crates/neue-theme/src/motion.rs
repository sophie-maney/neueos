//! Motion: fixed durations, spring presets and the reduced-motion switch.
//!
//! Timings follow the feel of Mountain Lion: controls react in about a tenth of a second,
//! sheets and popovers settle in about a third, and whole-screen transitions (Overview,
//! Spaces) in under half a second. Springs are specified by response and damping ratio
//! (as in most modern toolkits); [`Spring::stiffness`] and [`Spring::damping`] convert
//! them for a unit mass.
//!
//! Under reduced motion, [`Motion`] snaps: durations become zero and springs are `None`,
//! so the caller jumps to the end value.
//!
//! [`SpringValue`] runs a spring: it is solved in closed form, so the result depends only
//! on the time elapsed, never on how that time is split into frames (ADR 0036).

use std::f32::consts::TAU;
use std::time::Duration;

/// Highlight and hover fades.
pub const FAST: Duration = Duration::from_millis(100);
/// Most fades and small moves.
pub const NORMAL: Duration = Duration::from_millis(200);
/// Sheets, popovers and notification banners when not sprung.
pub const SLOW: Duration = Duration::from_millis(350);
/// Period of the default push button's pulse.
pub const DEFAULT_BUTTON_PULSE: Duration = Duration::from_millis(1400);
/// Time the barber pole's stripes take to move one stripe period (indeterminate progress
/// bars).
pub const BARBER_POLE_PERIOD: Duration = Duration::from_millis(600);
/// Time the spinner takes for one full turn.
pub const SPINNER_PERIOD: Duration = Duration::from_millis(1000);
/// Delay before a tooltip appears.
pub const TOOLTIP_DELAY: Duration = Duration::from_millis(1000);

/// A damped spring, described by how fast it settles and how much it overshoots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Period of the undamped oscillation, in seconds. Smaller is faster.
    pub response: f32,
    /// 1 is critically damped (no overshoot); below 1 bounces.
    pub damping_ratio: f32,
}

impl Spring {
    /// Controls: switches, sliders snapping, segmented selection.
    pub const SNAPPY: Self = Self {
        response: 0.15,
        damping_ratio: 1.0,
    };
    /// Sheets, popovers, menus and window open/close.
    pub const STANDARD: Self = Self {
        response: 0.3,
        damping_ratio: 1.0,
    };
    /// Whole-screen transitions: Overview, Spaces, Notification Center.
    pub const GENTLE: Self = Self {
        response: 0.45,
        damping_ratio: 0.95,
    };
    /// Playful moves with a small overshoot: dock icons, a shaken login field.
    pub const BOUNCY: Self = Self {
        response: 0.35,
        damping_ratio: 0.7,
    };

    /// Spring constant k for a unit mass: (2π / response)².
    pub fn stiffness(self) -> f32 {
        let omega = TAU / self.response;
        omega * omega
    }

    /// Damping coefficient c for a unit mass: 4π ζ / response.
    pub fn damping(self) -> f32 {
        2.0 * TAU * self.damping_ratio / self.response
    }
}

/// Motion tokens as the user wants them: full, or reduced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Motion {
    /// The reduced-motion preference.
    pub reduced: bool,
}

impl Motion {
    /// `duration`, or zero under reduced motion.
    pub fn duration(self, duration: Duration) -> Duration {
        if self.reduced {
            Duration::ZERO
        } else {
            duration
        }
    }

    /// `spring`, or `None` under reduced motion: jump straight to the end value.
    pub fn spring(self, spring: Spring) -> Option<Spring> {
        (!self.reduced).then_some(spring)
    }

    /// Whether ambient animation (the default button's pulse, spinners' easing) runs.
    pub fn ambient(self) -> bool {
        !self.reduced
    }
}

/// A value moving toward a target on a [`Spring`], stepped by elapsed time.
///
/// Toolkit widgets step it from Masonry's `on_anim_frame`; the compositor can step it from
/// its own frame clock. Each step solves the damped oscillator exactly, so ten 12 ms steps
/// land where one 120 ms step does: tests are deterministic and the motion is the same at
/// 60 Hz and 120 Hz. Retargeting mid-flight keeps the current velocity, so an interrupted
/// animation turns smoothly instead of jumping.
///
/// ```
/// use std::time::Duration;
/// use neue_theme::{Motion, Spring, motion::SpringValue};
///
/// let mut x = SpringValue::new(0.0);
/// x.animate_to(1.0, Motion::default().spring(Spring::SNAPPY));
/// while x.step(Duration::from_millis(8)) {}
/// assert_eq!(x.value(), 1.0);
///
/// // Under reduced motion the spring is `None`: the value snaps.
/// x.animate_to(0.0, Motion { reduced: true }.spring(Spring::SNAPPY));
/// assert_eq!(x.value(), 0.0);
/// assert!(!x.is_animating());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringValue {
    value: f64,
    velocity: f64,
    target: f64,
    /// The running spring; `None` when at rest.
    spring: Option<Spring>,
    rest_threshold: f64,
}

impl SpringValue {
    /// How close to the target (in the value's own units, and in units per response
    /// period for velocity) counts as arrived. A thousandth suits both opacities (0–1)
    /// and logical pixels.
    pub const DEFAULT_REST_THRESHOLD: f64 = 1e-3;

    /// A value at rest.
    pub fn new(value: f64) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            spring: None,
            rest_threshold: Self::DEFAULT_REST_THRESHOLD,
        }
    }

    /// Uses `threshold` instead of [`Self::DEFAULT_REST_THRESHOLD`], for values with
    /// larger or smaller units.
    #[must_use]
    pub fn with_rest_threshold(mut self, threshold: f64) -> Self {
        self.rest_threshold = threshold.abs();
        self
    }

    /// The current value.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// The current velocity, in units per second.
    pub fn velocity(&self) -> f64 {
        self.velocity
    }

    /// Where the value is heading (or is, at rest).
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Whether the value still moves, so the caller needs another frame.
    pub fn is_animating(&self) -> bool {
        self.spring.is_some()
    }

    /// Heads for `target` on `spring`, keeping the current velocity. `None` (what
    /// [`Motion::spring`] returns under reduced motion) jumps straight there. Returns
    /// whether the value now animates, so the caller knows to request frames.
    pub fn animate_to(&mut self, target: f64, spring: Option<Spring>) -> bool {
        self.target = target;
        match spring.filter(|s| s.response.is_finite() && s.response > 0.0) {
            Some(spring) if !self.at_rest(spring) => self.spring = Some(spring),
            _ => self.finish(),
        }
        self.is_animating()
    }

    /// Jumps to `value` and stops.
    pub fn snap_to(&mut self, value: f64) {
        self.target = value;
        self.finish();
    }

    /// Jumps to the target and stops (for example when reduced motion is switched on
    /// mid-flight).
    pub fn finish(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
        self.spring = None;
    }

    /// Advances by `dt`. Returns whether the value still animates; once it is within the
    /// rest threshold it lands exactly on the target.
    pub fn step(&mut self, dt: Duration) -> bool {
        let Some(spring) = self.spring else {
            return false;
        };
        let t = dt.as_secs_f64();
        let (x, v) = solve(spring, self.value - self.target, self.velocity, t);
        self.value = self.target + x;
        self.velocity = v;
        if self.at_rest(spring) {
            self.finish();
        }
        self.is_animating()
    }

    fn at_rest(&self, spring: Spring) -> bool {
        let omega = TAU as f64 / f64::from(spring.response);
        (self.value - self.target).abs() <= self.rest_threshold
            && (self.velocity / omega).abs() <= self.rest_threshold
    }
}

/// Displacement and velocity after `t` seconds of x'' + c x' + k x = 0 (unit mass),
/// starting from displacement `x0` and velocity `v0`.
fn solve(spring: Spring, x0: f64, v0: f64, t: f64) -> (f64, f64) {
    let omega = TAU as f64 / f64::from(spring.response);
    let zeta = f64::from(spring.damping_ratio).max(0.0);
    if (zeta - 1.0).abs() < 1e-6 {
        // Critically damped: x = e^(-ωt) (a + b t).
        let b = v0 + omega * x0;
        let e = (-omega * t).exp();
        let x = e * (x0 + b * t);
        (x, e * b - omega * x)
    } else if zeta < 1.0 {
        // Underdamped: x = e^(-ζωt) (a cos ω_d t + b sin ω_d t).
        let wd = omega * (1.0 - zeta * zeta).sqrt();
        let decay = zeta * omega;
        let b = (v0 + decay * x0) / wd;
        let e = (-decay * t).exp();
        let (sin, cos) = (wd * t).sin_cos();
        let x = e * (x0 * cos + b * sin);
        let v = e * ((b * wd - decay * x0) * cos - (x0 * wd + decay * b) * sin);
        (x, v)
    } else {
        // Overdamped: x = c1 e^(r1 t) + c2 e^(r2 t).
        let root = (zeta * zeta - 1.0).sqrt();
        let r1 = -omega * (zeta - root);
        let r2 = -omega * (zeta + root);
        let c2 = (v0 - r1 * x0) / (r2 - r1);
        let c1 = x0 - c2;
        let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
        (c1 * e1 + c2 * e2, r1 * c1 * e1 + r2 * c2 * e2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_snaps() {
        let reduced = Motion { reduced: true };
        assert_eq!(reduced.duration(SLOW), Duration::ZERO);
        assert_eq!(reduced.spring(Spring::STANDARD), None);
        assert!(!reduced.ambient());
        let full = Motion::default();
        assert_eq!(full.duration(SLOW), SLOW);
        assert_eq!(full.spring(Spring::STANDARD), Some(Spring::STANDARD));
    }

    #[test]
    fn spring_constants_match_response_and_ratio() {
        let s = Spring::STANDARD;
        let omega = s.stiffness().sqrt();
        assert!((omega - TAU / 0.3).abs() < 1e-3);
        // Critical damping: c = 2 √k for a unit mass.
        assert!((s.damping() - 2.0 * omega).abs() < 1e-3);
    }

    const FRAME_120: Duration = Duration::from_nanos(8_333_333);

    /// Steps `x` at `dt` until it rests; returns the number of steps.
    fn run(x: &mut SpringValue, dt: Duration) -> usize {
        let mut n = 0;
        while x.step(dt) {
            n += 1;
            assert!(n < 10_000, "spring never settled");
        }
        n
    }

    #[test]
    fn steps_do_not_depend_on_frame_split() {
        for spring in [
            Spring::SNAPPY,
            Spring::STANDARD,
            Spring::GENTLE,
            Spring::BOUNCY,
        ] {
            let over = Spring {
                damping_ratio: 1.4,
                ..spring
            };
            for spring in [spring, over] {
                let mut one = SpringValue::new(0.0).with_rest_threshold(0.0);
                let mut many = one;
                one.animate_to(100.0, Some(spring));
                many.animate_to(100.0, Some(spring));
                one.step(Duration::from_millis(120));
                for _ in 0..10 {
                    many.step(Duration::from_millis(12));
                }
                assert!((one.value() - many.value()).abs() < 1e-9, "{spring:?}");
                assert!(
                    (one.velocity() - many.velocity()).abs() < 1e-7,
                    "{spring:?}"
                );
            }
        }
    }

    #[test]
    fn same_inputs_same_frames() {
        let trace = || {
            let mut x = SpringValue::new(0.0);
            x.animate_to(1.0, Some(Spring::BOUNCY));
            let mut values = vec![];
            while x.step(FRAME_120) {
                values.push(x.value());
            }
            values
        };
        assert_eq!(trace(), trace());
    }

    #[test]
    fn settles_exactly_on_target_in_about_the_response_time() {
        let mut x = SpringValue::new(0.0);
        assert!(x.animate_to(1.0, Some(Spring::STANDARD)));
        let frames = run(&mut x, FRAME_120);
        assert_eq!(x.value(), 1.0);
        assert_eq!(x.velocity(), 0.0);
        assert!(!x.is_animating());
        // Critically damped with a 0.3 s response: settled within half a second.
        let secs = frames as f64 * FRAME_120.as_secs_f64();
        assert!((0.2..0.5).contains(&secs), "settled after {secs} s");
    }

    #[test]
    fn critical_damping_never_overshoots_and_bouncy_does() {
        let mut x = SpringValue::new(0.0);
        x.animate_to(1.0, Some(Spring::SNAPPY));
        let mut max: f64 = 0.0;
        while x.step(FRAME_120) {
            max = max.max(x.value());
        }
        assert!(max <= 1.0 + 1e-9);

        let mut x = SpringValue::new(0.0);
        x.animate_to(1.0, Some(Spring::BOUNCY));
        let mut max: f64 = 0.0;
        while x.step(FRAME_120) {
            max = max.max(x.value());
        }
        assert!(max > 1.01, "overshoot {max}");
    }

    #[test]
    fn reduced_motion_snaps_to_the_end_value() {
        let reduced = Motion { reduced: true };
        let mut x = SpringValue::new(0.0);
        assert!(!x.animate_to(40.0, reduced.spring(Spring::GENTLE)));
        assert_eq!(x.value(), 40.0);
        assert!(!x.step(FRAME_120));

        // Switched on mid-flight: the caller finishes the animation.
        x.animate_to(0.0, Motion::default().spring(Spring::GENTLE));
        x.step(FRAME_120);
        assert!(x.is_animating());
        x.finish();
        assert_eq!((x.value(), x.velocity()), (0.0, 0.0));
    }

    #[test]
    fn retargeting_keeps_velocity_and_zero_dt_is_a_no_op() {
        let mut x = SpringValue::new(0.0);
        x.animate_to(1.0, Some(Spring::STANDARD));
        let before = x;
        x.step(Duration::ZERO);
        assert_eq!(x, before);
        x.step(Duration::from_millis(50));
        let v = x.velocity();
        assert!(v > 0.0);
        x.animate_to(-1.0, Some(Spring::STANDARD));
        assert_eq!(x.velocity(), v);
        run(&mut x, FRAME_120);
        assert_eq!(x.value(), -1.0);
    }

    #[test]
    fn already_there_or_bad_spring_does_not_animate() {
        let mut x = SpringValue::new(3.0);
        assert!(!x.animate_to(3.0, Some(Spring::STANDARD)));
        let broken = Spring {
            response: 0.0,
            damping_ratio: 1.0,
        };
        assert!(!x.animate_to(5.0, Some(broken)));
        assert_eq!(x.value(), 5.0);
    }
}
