//! Time for ambient animation that never settles: the barber pole's stripes and the
//! spinner. Unlike a spring (`crate::anim`), it just keeps counting while it is wanted.

use std::time::Duration;

use masonry_core::core::UpdateCtx;

use crate::anim::AnimCtx;

/// Elapsed time of a looping animation, and whether animation frames are coming.
#[derive(Debug, Default)]
pub(super) struct Ticker {
    elapsed: Duration,
    running: bool,
}

impl Ticker {
    /// How far into its `period` the loop is, from 0 up to (not including) 1.
    pub(super) fn amount(&self, period: Duration) -> f64 {
        (self.elapsed.as_secs_f64() / period.as_secs_f64()).fract()
    }

    /// Asks for animation frames if `wanted` and none are coming; [`Self::frame`] decides
    /// whether the loop actually runs.
    pub(super) fn start(&mut self, ctx: &mut impl AnimCtx, wanted: bool) {
        if wanted && !self.running {
            self.running = true;
            ctx.request_anim_frame();
        }
    }

    /// Forward from `Widget::on_anim_frame`: counts `interval` (nanoseconds) and asks for
    /// the next frame while `wanted`. Otherwise it stops, where it is, until
    /// [`Self::start`] runs again.
    pub(super) fn frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        wanted: bool,
        period: Duration,
        interval: u64,
    ) {
        if wanted {
            let nanos = (self.elapsed.as_nanos() + u128::from(interval)) % period.as_nanos();
            // The remainder is below the period, which fits in a u64 of nanoseconds.
            self.elapsed = Duration::from_nanos(u64::try_from(nanos).unwrap_or(0));
            ctx.request_anim_frame();
        } else {
            self.running = false;
        }
        ctx.request_paint_only();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_wraps_at_the_period() {
        let mut t = Ticker::default();
        let period = Duration::from_millis(600);
        assert_eq!(t.amount(period), 0.0);
        t.elapsed = Duration::from_millis(150);
        assert!((t.amount(period) - 0.25).abs() < 1e-12);
        t.elapsed = Duration::from_millis(599);
        assert!(t.amount(period) < 1.0);
    }
}
