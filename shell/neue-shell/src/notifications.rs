//! The open notifications and their clocks (task 3.15a, ADR 0089): how long each stays up,
//! replacement in place, pausing while the pointer is over a banner, and expiry. The
//! process feeds it from `neue_dbus::notifications` and tells the server what closed; the
//! store itself knows nothing about D-Bus, so it is tested on every host. The current time
//! is always passed in.

use std::time::{Duration, Instant};

/// How long a notification stays up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    /// Until the user dismisses it: shown as an alert.
    Never,
    /// This long, then it expires: shown as a banner.
    After(Duration),
}

/// The lifetime for a notification: `requested` is what the app asked for (`None` when it
/// left it to us), `default` the configured banner time. Critical notifications never
/// expire, whatever the app asked (the spec's advice; GNOME and KDE do the same).
pub fn lifetime(requested: Option<Lifetime>, critical: bool, default: Duration) -> Lifetime {
    if critical {
        return Lifetime::Never;
    }
    requested.unwrap_or(Lifetime::After(default))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clock {
    /// It never expires.
    Stopped,
    Running {
        deadline: Instant,
    },
    /// The pointer is over it; this much is left.
    Paused {
        left: Duration,
    },
}

impl Clock {
    fn start(lifetime: Lifetime, now: Instant, paused: bool) -> Self {
        match lifetime {
            Lifetime::Never => Self::Stopped,
            Lifetime::After(left) if paused => Self::Paused { left },
            Lifetime::After(d) => Self::Running { deadline: now + d },
        }
    }
}

#[derive(Debug)]
struct Entry<T> {
    id: u32,
    item: T,
    lifetime: Lifetime,
    clock: Clock,
}

/// What [`Store::insert`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Inserted<T> {
    /// A new notification, at the end.
    Added,
    /// It took the place of the one with its id, which is returned.
    Replaced(T),
    /// A new notification, and the oldest one was dropped to make room. Tell its app it
    /// closed (reason "undefined").
    Evicted(u32, T),
}

/// The open notifications, oldest first.
#[derive(Debug)]
pub struct Store<T> {
    entries: Vec<Entry<T>>,
    capacity: usize,
}

impl<T> Store<T> {
    /// An empty store that keeps at most `capacity` notifications (at least one).
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    fn position(&self, id: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    /// Adds `item` as `id`, or replaces the open one with that id in place. Either way its
    /// clock starts again from `lifetime`; a replaced one the pointer is over stays paused.
    pub fn insert(&mut self, id: u32, item: T, lifetime: Lifetime, now: Instant) -> Inserted<T> {
        if let Some(i) = self.position(id) {
            let entry = &mut self.entries[i];
            let paused = matches!(entry.clock, Clock::Paused { .. });
            entry.lifetime = lifetime;
            entry.clock = Clock::start(lifetime, now, paused);
            return Inserted::Replaced(std::mem::replace(&mut entry.item, item));
        }
        let evicted = (self.entries.len() >= self.capacity).then(|| self.entries.remove(0));
        self.entries.push(Entry {
            id,
            item,
            lifetime,
            clock: Clock::start(lifetime, now, false),
        });
        match evicted {
            Some(old) => Inserted::Evicted(old.id, old.item),
            None => Inserted::Added,
        }
    }

    /// Takes `id` out (dismissed, closed by its app, or an action picked).
    pub fn remove(&mut self, id: u32) -> Option<T> {
        self.position(id).map(|i| self.entries.remove(i).item)
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.position(id).map(|i| &self.entries[i].item)
    }

    /// The lifetime `id` was given.
    pub fn lifetime(&self, id: u32) -> Option<Lifetime> {
        self.position(id).map(|i| self.entries[i].lifetime)
    }

    /// Ids and notifications, oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (u32, &T)> {
        self.entries.iter().map(|e| (e.id, &e.item))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Stops `id`'s clock (the pointer entered it). `false` if it isn't open.
    pub fn pause(&mut self, id: u32, now: Instant) -> bool {
        let Some(i) = self.position(id) else {
            return false;
        };
        let clock = &mut self.entries[i].clock;
        if let Clock::Running { deadline } = *clock {
            *clock = Clock::Paused {
                left: deadline.saturating_duration_since(now),
            };
        }
        true
    }

    /// Starts `id`'s clock again with the time it had left (the pointer left it). `false`
    /// if it isn't open.
    pub fn resume(&mut self, id: u32, now: Instant) -> bool {
        let Some(i) = self.position(id) else {
            return false;
        };
        let clock = &mut self.entries[i].clock;
        if let Clock::Paused { left } = *clock {
            *clock = Clock::Running {
                deadline: now + left,
            };
        }
        true
    }

    /// When the next running clock runs out: wake up then and call [`Store::expire`].
    pub fn next_deadline(&self) -> Option<Instant> {
        self.entries
            .iter()
            .filter_map(|e| match e.clock {
                Clock::Running { deadline } => Some(deadline),
                _ => None,
            })
            .min()
    }

    /// Takes out every notification whose time ran out by `now`, oldest first.
    pub fn expire(&mut self, now: Instant) -> Vec<(u32, T)> {
        let (gone, kept) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|e| matches!(e.clock, Clock::Running { deadline } if deadline <= now));
        self.entries = kept;
        gone.into_iter().map(|e: Entry<T>| (e.id, e.item)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    fn ids<T>(store: &Store<T>) -> Vec<u32> {
        store.iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn lifetimes_follow_the_request_and_urgency() {
        let five = 5 * S;
        assert_eq!(lifetime(None, false, five), Lifetime::After(five));
        assert_eq!(
            lifetime(Some(Lifetime::After(2 * S)), false, five),
            Lifetime::After(2 * S)
        );
        assert_eq!(
            lifetime(Some(Lifetime::Never), false, five),
            Lifetime::Never
        );
        assert_eq!(
            lifetime(Some(Lifetime::After(2 * S)), true, five),
            Lifetime::Never,
            "critical stays"
        );
    }

    #[test]
    fn notifications_expire_in_order() {
        let t = Instant::now();
        let mut store = Store::new(10);
        assert_eq!(
            store.insert(1, "a", Lifetime::After(5 * S), t),
            Inserted::Added
        );
        store.insert(2, "b", Lifetime::After(2 * S), t);
        store.insert(3, "c", Lifetime::Never, t);
        assert_eq!(store.next_deadline(), Some(t + 2 * S));
        assert!(store.expire(t + S).is_empty());
        assert_eq!(store.expire(t + 2 * S), [(2, "b")]);
        assert_eq!(store.next_deadline(), Some(t + 5 * S));
        assert_eq!(store.expire(t + 60 * S), [(1, "a")]);
        assert_eq!(store.next_deadline(), None, "an alert never expires");
        assert_eq!(ids(&store), [3]);
    }

    #[test]
    fn replacing_keeps_the_place_and_restarts_the_clock() {
        let t = Instant::now();
        let mut store = Store::new(10);
        store.insert(1, "a", Lifetime::After(5 * S), t);
        store.insert(2, "b", Lifetime::After(5 * S), t);
        assert_eq!(
            store.insert(1, "a2", Lifetime::After(5 * S), t + 4 * S),
            Inserted::Replaced("a")
        );
        assert_eq!(ids(&store), [1, 2]);
        assert_eq!(store.get(1), Some(&"a2"));
        assert_eq!(store.expire(t + 5 * S), [(2, "b")]);
        assert_eq!(store.expire(t + 9 * S), [(1, "a2")]);
    }

    #[test]
    fn a_replacement_can_turn_a_banner_into_an_alert() {
        let t = Instant::now();
        let mut store = Store::new(10);
        store.insert(1, "a", Lifetime::After(S), t);
        store.insert(1, "a", Lifetime::Never, t);
        assert_eq!(store.lifetime(1), Some(Lifetime::Never));
        assert!(store.expire(t + 60 * S).is_empty());
    }

    #[test]
    fn hovering_pauses_the_clock() {
        let t = Instant::now();
        let mut store = Store::new(10);
        store.insert(1, "a", Lifetime::After(5 * S), t);
        assert!(store.pause(1, t + 3 * S));
        assert_eq!(store.next_deadline(), None);
        assert!(store.expire(t + 60 * S).is_empty());
        // A replacement while hovered stays paused, with its full time.
        store.insert(1, "a2", Lifetime::After(5 * S), t + 61 * S);
        assert!(store.expire(t + 120 * S).is_empty());
        assert!(store.resume(1, t + 120 * S));
        assert_eq!(store.next_deadline(), Some(t + 125 * S));
        assert!(!store.pause(9, t), "not open");
    }

    #[test]
    fn resuming_gives_back_the_time_left() {
        let t = Instant::now();
        let mut store = Store::new(10);
        store.insert(1, "a", Lifetime::After(5 * S), t);
        store.pause(1, t + 3 * S);
        store.resume(1, t + 10 * S);
        assert_eq!(store.next_deadline(), Some(t + 12 * S));
        // Resuming a running clock changes nothing.
        store.resume(1, t + 11 * S);
        assert_eq!(store.next_deadline(), Some(t + 12 * S));
    }

    #[test]
    fn a_full_store_drops_the_oldest() {
        let t = Instant::now();
        let mut store = Store::new(2);
        store.insert(1, "a", Lifetime::Never, t);
        store.insert(2, "b", Lifetime::Never, t);
        assert_eq!(
            store.insert(3, "c", Lifetime::Never, t),
            Inserted::Evicted(1, "a")
        );
        assert_eq!(
            store.insert(2, "b2", Lifetime::Never, t),
            Inserted::Replaced("b"),
            "replacing needs no room"
        );
        assert_eq!(ids(&store), [2, 3]);
        assert_eq!(store.remove(2), Some("b2"));
        assert_eq!(store.remove(2), None);
        assert_eq!(store.len(), 1);
    }
}
