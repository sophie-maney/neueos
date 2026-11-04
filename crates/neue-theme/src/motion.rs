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
}
