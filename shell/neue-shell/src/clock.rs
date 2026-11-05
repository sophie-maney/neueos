//! The clock extra's text (task 3.2b): the weekday and the time, 24-hour, in the local
//! time zone. Locale and 12-hour settings come with Preferences (task 3.24).

use std::time::Duration;

use jiff::Zoned;
use jiff::civil::{DateTime, Weekday};

/// Longest wait between two clock updates: after a suspend the bar catches up within this.
pub const MAX_TICK: Duration = Duration::from_secs(5);

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS_LONG: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn weekday_index(day: Weekday) -> usize {
    usize::from(day.to_monday_zero_offset().unsigned_abs())
}

/// The clock extra's text for `now`: `Thu 10:09`.
pub fn time_text(now: DateTime) -> String {
    format!(
        "{} {:02}:{:02}",
        WEEKDAYS[weekday_index(now.weekday())],
        now.hour(),
        now.minute()
    )
}

/// The clock menu's date for `now`: `Thursday, 5 November 2026`.
pub fn full_date_text(now: DateTime) -> String {
    let month = usize::from(now.month().unsigned_abs()).saturating_sub(1);
    format!(
        "{}, {} {} {}",
        WEEKDAYS_LONG[weekday_index(now.weekday())],
        now.day(),
        MONTHS[month],
        now.year()
    )
}

/// The local date and time now. Falls back to UTC if the time zone can't be read.
pub fn now() -> DateTime {
    Zoned::now().datetime()
}

/// How long to wait before the next update: to the start of the next minute (plus a
/// little, so the update lands in it), but never longer than [`MAX_TICK`].
pub fn next_tick(now: DateTime) -> Duration {
    let into_minute = Duration::from_secs(now.second().unsigned_abs().into())
        + Duration::from_nanos(now.subsec_nanosecond().unsigned_abs().into());
    let to_next = Duration::from_secs(60).saturating_sub(into_minute) + Duration::from_millis(20);
    to_next.min(MAX_TICK)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn formats_the_bar_and_the_menu() {
        let now = date(2026, 11, 5).at(9, 7, 3, 0);
        assert_eq!(time_text(now), "Thu 09:07");
        assert_eq!(full_date_text(now), "Thursday, 5 November 2026");
        let sunday = date(2026, 11, 8).at(23, 59, 0, 0);
        assert_eq!(time_text(sunday), "Sun 23:59");
    }

    #[test]
    fn ticks_at_the_next_minute_or_sooner() {
        let near_end = date(2026, 1, 1).at(0, 0, 58, 500_000_000);
        assert_eq!(next_tick(near_end), Duration::from_millis(1_520));
        let start = date(2026, 1, 1).at(0, 0, 0, 0);
        assert_eq!(next_tick(start), MAX_TICK);
    }
}
