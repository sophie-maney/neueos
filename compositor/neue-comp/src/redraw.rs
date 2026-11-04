//! When an output is drawn: only after something changed, at most once per refresh.
//!
//! The tty backend keeps one [`Redraw`] per output and drives it from its event sources;
//! this module is the bookkeeping only, so it is tested without a GPU (ADR 0019).
//!
//! An output is idle until [`Redraw::queue`] marks it: client traffic, a commit, input, a
//! `neuectl` request, a config change, a hotplug or an animated cursor's next frame. Then
//! one render runs. A render with damage queues a frame and waits for its vblank; a render
//! without damage queues nothing and waits for an *estimated* vblank instead, a timer one
//! refresh after the last real one, which is when the frame callbacks of that empty frame
//! go out. Either wait ends in a new render if anything was queued meanwhile, else in
//! idle. So a busy client cannot make an output render faster than its refresh rate, and
//! an idle desktop renders nothing.

use std::time::Duration;

/// Where an output is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Nothing scheduled; the next [`Redraw::queue`] schedules a render.
    Idle,
    /// A render is scheduled and has not run yet.
    Scheduled,
    /// A frame was queued; its vblank has not arrived.
    WaitingForVblank,
    /// The last render had no damage; the estimated-vblank timer is armed.
    WaitingForEstimatedVblank,
}

/// The redraw state of one output.
#[derive(Debug)]
pub struct Redraw {
    phase: Phase,
    /// Something changed since the last render started.
    needed: bool,
    /// When the last frame reached the screen (`CLOCK_MONOTONIC`), to time estimated
    /// vblanks against.
    last_vblank: Option<Duration>,
    /// Renders run since the output was lit, with damage or not.
    renders: u64,
    /// Of those, the ones that queued a frame.
    frames: u64,
}

impl Default for Redraw {
    fn default() -> Self {
        Self::new()
    }
}

impl Redraw {
    pub const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            needed: false,
            last_vblank: None,
            renders: 0,
            frames: 0,
        }
    }

    /// Something on the output changed. `true` means the caller must schedule a render
    /// (the output was idle); otherwise the running wait picks the change up.
    pub fn queue(&mut self) -> bool {
        self.needed = true;
        if self.phase == Phase::Idle {
            self.phase = Phase::Scheduled;
            true
        } else {
            false
        }
    }

    /// The scheduled render is about to run. `false` means there is nothing to do (it is
    /// not the render this state scheduled).
    pub fn start_render(&mut self) -> bool {
        if self.phase != Phase::Scheduled {
            return false;
        }
        self.needed = false;
        true
    }

    /// The scheduled render could not run (the session is inactive, or the output is not
    /// in the layout yet). The change stays queued for [`Redraw::reset`].
    pub fn skip_render(&mut self) {
        if self.phase == Phase::Scheduled {
            self.phase = Phase::Idle;
        }
    }

    /// The render finished. With `queued` a frame is on its way to the screen; without,
    /// the caller arms the estimated-vblank timer for [`Redraw::estimated_vblank`].
    pub fn rendered(&mut self, queued: bool) {
        self.renders += 1;
        if queued {
            self.frames += 1;
            self.phase = Phase::WaitingForVblank;
        } else {
            self.phase = Phase::WaitingForEstimatedVblank;
        }
    }

    /// The queued frame reached the screen at `time`. `true` means render again.
    pub fn vblank(&mut self, time: Duration) -> bool {
        if self.phase != Phase::WaitingForVblank {
            return false;
        }
        self.last_vblank = Some(time);
        self.end_wait()
    }

    /// The estimated-vblank timer fired. `true` means render again. The caller sends the
    /// frame callbacks of the empty frame either way.
    pub fn estimated_vblank_passed(&mut self) -> bool {
        if self.phase != Phase::WaitingForEstimatedVblank {
            return false;
        }
        self.end_wait()
    }

    fn end_wait(&mut self) -> bool {
        if self.needed {
            self.phase = Phase::Scheduled;
            true
        } else {
            self.phase = Phase::Idle;
            false
        }
    }

    /// Forget any wait (the session came back: vblanks of frames queued before the
    /// switch never arrive). `true` means the caller must schedule a render: always,
    /// since another DRM master may have left anything on screen.
    pub fn reset(&mut self) -> bool {
        self.phase = Phase::Idle;
        self.queue()
    }

    /// How long from `now` until the estimated next vblank, for an output refreshing every
    /// `refresh`: the first refresh boundary after `now`, counted from the last real vblank.
    /// Without a vblank yet (or an unknown refresh rate) it is one refresh from now.
    pub fn until_estimated_vblank(&self, now: Duration, refresh: Duration) -> Duration {
        let refresh = refresh.max(Duration::from_millis(1));
        let Some(last) = self.last_vblank.filter(|last| *last <= now) else {
            return refresh;
        };
        let since = (now - last).as_nanos();
        let period = refresh.as_nanos();
        let into_period = since % period;
        // `into_period < period`, which fits in u64 nanoseconds (period is a refresh).
        Duration::from_nanos((period - into_period) as u64)
    }

    /// Renders since the output was lit (the idle check of task 1.15b reads this).
    pub fn renders(&self) -> u64 {
        self.renders
    }

    /// Frames queued since the output was lit.
    pub fn frames(&self) -> u64 {
        self.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFRESH: Duration = Duration::from_micros(16_667);

    #[test]
    fn idle_until_queued() {
        let mut r = Redraw::new();
        assert!(!r.start_render(), "nothing scheduled, nothing to render");
        assert!(r.queue());
        assert!(!r.queue(), "a second change joins the scheduled render");
        assert!(r.start_render());
        r.rendered(true);
        assert!(
            !r.vblank(Duration::from_secs(1)),
            "nothing changed meanwhile"
        );
        assert!(!r.start_render(), "idle again");
        assert_eq!((r.renders(), r.frames()), (1, 1));
    }

    #[test]
    fn change_during_vblank_wait_renders_after_it() {
        let mut r = Redraw::new();
        r.queue();
        r.start_render();
        r.rendered(true);
        assert!(!r.queue(), "waits for the vblank");
        assert!(r.vblank(Duration::from_secs(1)));
        assert!(r.start_render());
    }

    #[test]
    fn empty_render_waits_for_the_estimate() {
        let mut r = Redraw::new();
        r.queue();
        r.start_render();
        r.rendered(false);
        assert!(!r.vblank(Duration::from_secs(1)), "no frame was queued");
        assert!(!r.queue(), "capped at the refresh rate");
        assert!(r.estimated_vblank_passed());
        assert!(r.start_render());
        r.rendered(false);
        assert!(!r.estimated_vblank_passed(), "nothing changed: idle");
        assert_eq!((r.renders(), r.frames()), (2, 0));
    }

    #[test]
    fn skipped_render_keeps_the_change_for_reset() {
        let mut r = Redraw::new();
        r.queue();
        r.skip_render();
        assert!(!r.start_render());
        assert!(r.queue(), "idle again, so a new change schedules");
        r.start_render();
        r.rendered(true);
        assert!(r.reset(), "a lost vblank does not leave the output stuck");
        assert!(r.start_render());
    }

    #[test]
    fn estimate_follows_the_last_vblank() {
        let mut r = Redraw::new();
        let now = Duration::from_secs(10);
        assert_eq!(r.until_estimated_vblank(now, REFRESH), REFRESH);

        r.queue();
        r.start_render();
        r.rendered(true);
        r.vblank(now);
        let later = now + Duration::from_millis(5);
        assert_eq!(
            r.until_estimated_vblank(later, REFRESH),
            REFRESH - Duration::from_millis(5)
        );
        // Several refreshes later: the next boundary, not one refresh from now.
        let much_later = now + REFRESH * 3 + Duration::from_millis(2);
        assert_eq!(
            r.until_estimated_vblank(much_later, REFRESH),
            REFRESH - Duration::from_millis(2)
        );
        // Exactly on a boundary: the next one.
        assert_eq!(r.until_estimated_vblank(now + REFRESH, REFRESH), REFRESH);
    }
}
