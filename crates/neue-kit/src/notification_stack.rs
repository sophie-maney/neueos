//! Where notification panels sit in their stack and how they move (task 3.15b): the
//! newest on top, sliding in from the right while the others move down to make room; a
//! dismissed one slides back out to the right, and once it is gone the ones below close
//! up. Under reduced motion everything jumps to its place.
//!
//! Plain data stepped by the caller's clock, so it is tested on every host. Coordinates
//! are the stack's own: y runs down from the top of the stack (the first panel's top),
//! x is how far right of its place a panel is drawn. Place the stack [`SCREEN_GAP`] under
//! the menu bar and left of the screen's right edge.

use std::time::Duration;

use neue_theme::materials::notification::{MARGIN, SHIFT, SLIDE, SPACING, WIDTH};
use neue_theme::{Motion, SpringValue};

pub use neue_theme::materials::notification::SCREEN_GAP;

/// How far a panel slides: its width and room for its shadow and the gap to the screen
/// edge, so it starts and ends wholly off screen.
pub const SLIDE_DISTANCE: f64 = WIDTH + MARGIN + SCREEN_GAP;

/// Moves smaller than this (logical pixels) count as at rest.
const REST: f64 = 0.25;

#[derive(Debug, Clone)]
struct Entry {
    id: u32,
    height: f64,
    /// 0: in place; 1: a whole [`SLIDE_DISTANCE`] to the right.
    slide: SpringValue,
    y: SpringValue,
    leaving: bool,
}

/// Where one panel is drawn now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The notification's id.
    pub id: u32,
    /// How far right of its place, in logical pixels.
    pub x: f64,
    /// Its top, from the top of the stack.
    pub y: f64,
    /// Its height.
    pub height: f64,
    /// Whether it is sliding out after [`NotificationStack::remove`].
    pub leaving: bool,
}

/// The stack of notification panels, top to bottom.
#[derive(Debug, Clone, Default)]
pub struct NotificationStack {
    entries: Vec<Entry>,
}

impl NotificationStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a panel `height` tall on top, sliding in from the right; the others move down.
    /// An id already in the stack is resized in place instead (a replacement), and one
    /// that was leaving slides back in.
    pub fn push(&mut self, id: u32, height: f64, motion: Motion) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.height = height;
            if entry.leaving {
                entry.leaving = false;
                entry.slide.animate_to(0.0, motion.spring(SLIDE));
            }
        } else {
            let mut slide = SpringValue::new(1.0);
            slide.animate_to(0.0, motion.spring(SLIDE));
            self.entries.insert(
                0,
                Entry {
                    id,
                    height,
                    slide,
                    y: SpringValue::new(0.0).with_rest_threshold(REST),
                    leaving: false,
                },
            );
        }
        self.retarget(motion);
    }

    /// Changes the height of panel `id`; the ones below move.
    pub fn set_height(&mut self, id: u32, height: f64, motion: Motion) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.height = height;
            self.retarget(motion);
        }
    }

    /// Starts panel `id` sliding out to the right. It keeps its place until it is gone,
    /// then [`step`](Self::step) drops it and the ones below close up.
    pub fn remove(&mut self, id: u32, motion: Motion) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.leaving = true;
            entry.slide.animate_to(1.0, motion.spring(SLIDE));
        }
        self.drop_gone(motion);
    }

    /// Moves everything on by `dt`. Returns the ids of the panels that finished sliding
    /// out and left the stack.
    pub fn step(&mut self, dt: Duration, motion: Motion) -> Vec<u32> {
        for entry in &mut self.entries {
            for value in [&mut entry.slide, &mut entry.y] {
                if motion.reduced {
                    value.finish();
                } else {
                    value.step(dt);
                }
            }
        }
        self.drop_gone(motion)
    }

    /// Whether anything is still moving.
    pub fn is_animating(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.slide.is_animating() || e.y.is_animating())
    }

    /// Whether panel `id` is in the stack (also while it slides out).
    pub fn contains(&self, id: u32) -> bool {
        self.entries.iter().any(|e| e.id == id)
    }

    /// Whether the stack is empty: nothing shown and nothing sliding out.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Where each panel is drawn now, top to bottom.
    pub fn placements(&self) -> impl Iterator<Item = Placement> + '_ {
        self.entries.iter().map(|e| Placement {
            id: e.id,
            x: (e.slide.value() * SLIDE_DISTANCE).round(),
            y: e.y.value().round(),
            height: e.height,
            leaving: e.leaving,
        })
    }

    /// How tall the stack is now and will be at rest, whichever is more: the height its
    /// surface needs.
    pub fn extent(&self) -> f64 {
        self.entries
            .iter()
            .map(|e| e.y.value().max(e.y.target()) + e.height)
            .fold(0.0, f64::max)
    }

    fn drop_gone(&mut self, motion: Motion) -> Vec<u32> {
        let mut gone = Vec::new();
        self.entries.retain(|e| {
            let done = e.leaving && !e.slide.is_animating();
            if done {
                gone.push(e.id);
            }
            !done
        });
        if !gone.is_empty() {
            self.retarget(motion);
        }
        gone
    }

    /// Sends every panel towards its place under the ones above it.
    fn retarget(&mut self, motion: Motion) {
        let mut y = 0.0;
        for entry in &mut self.entries {
            entry.y.animate_to(y, motion.spring(SHIFT));
            y += entry.height + SPACING;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOVING: Motion = Motion { reduced: false };
    const REDUCED: Motion = Motion { reduced: true };
    const FRAME: Duration = Duration::from_millis(16);

    /// Steps until nothing moves; returns every id that left.
    fn settle(stack: &mut NotificationStack) -> Vec<u32> {
        let mut gone = Vec::new();
        for _ in 0..500 {
            if !stack.is_animating() {
                return gone;
            }
            gone.extend(stack.step(FRAME, MOVING));
        }
        panic!("the stack never came to rest");
    }

    fn at_rest(stack: &NotificationStack) -> Vec<(u32, f64, f64)> {
        stack.placements().map(|p| (p.id, p.x, p.y)).collect()
    }

    #[test]
    fn a_new_panel_slides_in_on_top_and_pushes_the_others_down() {
        let mut stack = NotificationStack::new();
        stack.push(1, 60.0, MOVING);
        let first = stack.placements().next().expect("one");
        assert_eq!(
            (first.x, first.y),
            (SLIDE_DISTANCE, 0.0),
            "starts off screen"
        );
        stack.step(FRAME * 4, MOVING);
        let x = stack.placements().next().expect("one").x;
        assert!(x > 0.0 && x < SLIDE_DISTANCE, "sliding: {x}");
        settle(&mut stack);
        assert_eq!(at_rest(&stack), [(1, 0.0, 0.0)]);

        stack.push(2, 80.0, MOVING);
        stack.step(FRAME * 4, MOVING);
        let below = stack.placements().nth(1).expect("two");
        assert_eq!(below.id, 1);
        assert!(
            below.y > 0.0 && below.y < 80.0 + SPACING,
            "moving down: {}",
            below.y
        );
        settle(&mut stack);
        assert_eq!(at_rest(&stack), [(2, 0.0, 0.0), (1, 0.0, 80.0 + SPACING)]);
        assert_eq!(stack.extent(), 80.0 + SPACING + 60.0);
    }

    #[test]
    fn a_removed_panel_slides_out_then_the_rest_close_up() {
        let mut stack = NotificationStack::new();
        for id in [1, 2, 3] {
            stack.push(id, 50.0, MOVING);
        }
        settle(&mut stack);
        // 3 on top, then 2, then 1.
        stack.remove(2, MOVING);
        stack.step(FRAME * 4, MOVING);
        let placements: Vec<_> = stack.placements().collect();
        assert!(placements[1].leaving && placements[1].x > 0.0);
        assert_eq!(
            placements[2].y,
            2.0 * (50.0 + SPACING),
            "1 waits for 2 to go"
        );

        let gone = settle(&mut stack);
        assert_eq!(gone, [2]);
        assert!(!stack.contains(2));
        assert_eq!(at_rest(&stack), [(3, 0.0, 0.0), (1, 0.0, 50.0 + SPACING)]);
    }

    #[test]
    fn the_extent_covers_a_panel_still_moving_up() {
        let mut stack = NotificationStack::new();
        stack.push(1, 50.0, MOVING);
        stack.push(2, 50.0, MOVING);
        settle(&mut stack);
        stack.remove(2, MOVING);
        // Step until 2 is gone but 1 has not finished moving up.
        while stack.contains(2) {
            stack.step(FRAME, MOVING);
        }
        stack.step(FRAME, MOVING);
        let y = stack.placements().next().expect("one").y;
        assert!(y > 0.0, "still moving up: {y}");
        assert!(stack.extent() >= y + 50.0);
        settle(&mut stack);
        assert_eq!(stack.extent(), 50.0);
    }

    #[test]
    fn a_replacement_resizes_in_place() {
        let mut stack = NotificationStack::new();
        stack.push(1, 50.0, MOVING);
        stack.push(2, 50.0, MOVING);
        settle(&mut stack);
        stack.push(1, 50.0, MOVING);
        stack.push(2, 90.0, MOVING);
        settle(&mut stack);
        assert_eq!(at_rest(&stack), [(2, 0.0, 0.0), (1, 0.0, 90.0 + SPACING)]);

        stack.set_height(2, 40.0, MOVING);
        settle(&mut stack);
        assert_eq!(at_rest(&stack), [(2, 0.0, 0.0), (1, 0.0, 40.0 + SPACING)]);
    }

    #[test]
    fn a_leaving_panel_pushed_again_slides_back() {
        let mut stack = NotificationStack::new();
        stack.push(1, 50.0, MOVING);
        settle(&mut stack);
        stack.remove(1, MOVING);
        stack.step(FRAME * 3, MOVING);
        stack.push(1, 50.0, MOVING);
        assert!(settle(&mut stack).is_empty());
        assert_eq!(at_rest(&stack), [(1, 0.0, 0.0)]);
    }

    #[test]
    fn reduced_motion_jumps_without_sliding() {
        let mut stack = NotificationStack::new();
        stack.push(1, 50.0, REDUCED);
        stack.push(2, 50.0, REDUCED);
        assert!(!stack.is_animating());
        assert_eq!(at_rest(&stack), [(2, 0.0, 0.0), (1, 0.0, 50.0 + SPACING)]);

        stack.remove(2, REDUCED);
        assert!(!stack.contains(2), "gone at once");
        assert_eq!(at_rest(&stack), [(1, 0.0, 0.0)]);
    }

    #[test]
    fn switching_to_reduced_motion_finishes_moves_on_the_next_step() {
        let mut stack = NotificationStack::new();
        stack.push(1, 50.0, MOVING);
        stack.push(2, 50.0, MOVING);
        stack.step(FRAME * 4, MOVING);
        stack.remove(1, MOVING);
        assert!(stack.contains(1));
        assert_eq!(stack.step(FRAME, REDUCED), [1]);
        assert!(!stack.is_animating());
        assert_eq!(at_rest(&stack), [(2, 0.0, 0.0)]);
    }
}
