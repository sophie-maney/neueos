//! A civil date (task 2.24c, ADR 0062): a day in the proleptic Gregorian calendar, without
//! a time or a time zone. The date picker's value; just enough calendar maths for it, so
//! the kit takes no date-time dependency.

use std::fmt;

/// The first year a [`Date`] can have.
pub const MIN_YEAR: i32 = 1;
/// The last year a [`Date`] can have (the picker's year segment has four digits).
pub const MAX_YEAR: i32 = 9999;

/// English month names, January first (translations come with locale support).
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

/// A day in the proleptic Gregorian calendar, years 1 to 9999. Dates compare in calendar
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

/// Whether `year` is a leap year.
pub const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// The number of days in `month` (1 to 12) of `year`; 0 for a month that doesn't exist.
pub const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

impl Date {
    /// The date `year`-`month`-`day`, or `None` if there is no such day (or the year is out
    /// of range).
    pub const fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if year < MIN_YEAR || year > MAX_YEAR || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// The year, 1 to 9999.
    pub const fn year(self) -> i32 {
        self.year
    }

    /// The month, 1 (January) to 12.
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The day of the month, from 1.
    pub const fn day(self) -> u8 {
        self.day
    }

    /// Today in UTC. (The local time zone needs a library the kit doesn't have yet; apps
    /// that know better pass their own with `DatePicker::today`.)
    pub fn today() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let days = i64::try_from(secs / 86_400).unwrap_or(0);
        Self::from_days(days).unwrap_or(Self {
            year: 1970,
            month: 1,
            day: 1,
        })
    }

    /// Days since 1970-01-01 (negative before it), by Howard Hinnant's `days_from_civil`.
    pub const fn days(self) -> i64 {
        let y = self.year as i64 - if self.month <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = self.month as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` days after 1970-01-01, or `None` out of the supported years.
    pub fn from_days(days: i64) -> Option<Self> {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = u8::try_from(doy - (153 * mp + 2) / 5 + 1).ok()?;
        let month = u8::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).ok()?;
        let year = i32::try_from(yoe + era * 400 + i64::from(month <= 2)).ok()?;
        Self::new(year, month, day)
    }

    /// The day of the week, 0 for Monday to 6 for Sunday.
    pub const fn weekday(self) -> u8 {
        // 1970-01-01 was a Thursday (3).
        (self.days() + 3).rem_euclid(7) as u8
    }

    /// The date `delta` days later (earlier if negative), or `None` out of range.
    pub fn add_days(self, delta: i64) -> Option<Self> {
        Self::from_days(self.days() + delta)
    }

    /// The same day `delta` months later (earlier if negative); a day the month doesn't
    /// have (the 31st into April) becomes the month's last. `None` out of range.
    pub fn add_months(self, delta: i32) -> Option<Self> {
        let index = self.year * 12 + i32::from(self.month) - 1 + delta;
        let (year, month) = (
            index.div_euclid(12),
            u8::try_from(index.rem_euclid(12)).ok()? + 1,
        );
        self.with_month(year, month)
    }

    /// The day of `year` and `month` that is nearest this date's day number: the same
    /// number, or the month's last if it has fewer days. `None` if `year` is out of range
    /// or `month` isn't 1 to 12.
    pub fn with_month(self, year: i32, month: u8) -> Option<Self> {
        let last = days_in_month(year, month);
        Self::new(year, month, self.day.min(last))
    }

    /// The date with its day replaced, `None` if the month hasn't that many days.
    pub const fn with_day(self, day: u8) -> Option<Self> {
        Self::new(self.year, self.month, day)
    }

    /// The month's English name.
    pub fn month_name(self) -> &'static str {
        MONTHS[usize::from(self.month) - 1]
    }

    /// The English name of `month` (1 to 12), or an empty string.
    pub fn name_of_month(month: u8) -> &'static str {
        MONTHS
            .get(usize::from(month).wrapping_sub(1))
            .copied()
            .unwrap_or("")
    }

    /// "5 November 2026", for assistive technology and tooltips.
    pub fn long(self) -> String {
        format!("{} {} {}", self.day, self.month_name(), self.year)
    }

    /// The 42 days of a calendar page for `month` of `year` (a grid of six weeks, Monday
    /// first): the month, padded with the end of the last month and the start of the next.
    /// `None` for a cell outside the supported years. Empty for a month that doesn't exist.
    pub fn month_grid(year: i32, month: u8) -> Vec<Option<Date>> {
        let Some(first) = Self::new(year, month, 1) else {
            return Vec::new();
        };
        let start = first.days() - i64::from(first.weekday());
        (0..42).map(|i| Self::from_days(start + i)).collect()
    }
}

impl fmt::Display for Date {
    /// ISO 8601: `2026-11-05`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).expect("a real date")
    }

    #[test]
    fn months_have_their_lengths() {
        let lengths: Vec<u8> = (1..=12).map(|m| days_in_month(2026, m)).collect();
        assert_eq!(lengths, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
        assert_eq!(days_in_month(2026, 0), 0);
        assert_eq!(days_in_month(2026, 13), 0);
    }

    #[test]
    fn leap_years_follow_the_gregorian_rule() {
        assert!(is_leap_year(2024));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
    }

    #[test]
    fn impossible_dates_are_refused() {
        assert!(Date::new(2026, 2, 29).is_none());
        assert!(Date::new(2024, 2, 29).is_some());
        assert!(Date::new(2026, 4, 31).is_none());
        assert!(Date::new(2026, 0, 1).is_none());
        assert!(Date::new(2026, 1, 0).is_none());
        assert!(Date::new(0, 1, 1).is_none());
        assert!(Date::new(10_000, 1, 1).is_none());
    }

    #[test]
    fn days_since_the_epoch_round_trip() {
        assert_eq!(d(1970, 1, 1).days(), 0);
        assert_eq!(d(1970, 1, 2).days(), 1);
        assert_eq!(d(1969, 12, 31).days(), -1);
        assert_eq!(d(2000, 3, 1).days(), 11_017);
        for days in (-100_000..100_000).step_by(37) {
            assert_eq!(Date::from_days(days).map(Date::days), Some(days));
        }
        assert_eq!(Date::from_days(-1_000_000_000), None);
    }

    #[test]
    fn weekdays_count_from_monday() {
        assert_eq!(d(2026, 11, 5).weekday(), 3); // a Thursday
        assert_eq!(d(1970, 1, 1).weekday(), 3);
        assert_eq!(d(2000, 1, 1).weekday(), 5); // a Saturday
        assert_eq!(d(2024, 2, 29).weekday(), 3);
        assert_eq!(d(1, 1, 1).weekday(), 0); // the proleptic calendar's Monday
        assert_eq!(d(2026, 11, 8).weekday(), 6);
    }

    #[test]
    fn adding_days_crosses_months_and_years() {
        assert_eq!(d(2026, 11, 30).add_days(1), Some(d(2026, 12, 1)));
        assert_eq!(d(2026, 12, 31).add_days(1), Some(d(2027, 1, 1)));
        assert_eq!(d(2024, 2, 28).add_days(1), Some(d(2024, 2, 29)));
        assert_eq!(d(2026, 3, 1).add_days(-1), Some(d(2026, 2, 28)));
        assert_eq!(d(9999, 12, 31).add_days(1), None);
        assert_eq!(d(1, 1, 1).add_days(-1), None);
    }

    #[test]
    fn adding_months_keeps_the_day_or_the_last_of_the_month() {
        assert_eq!(d(2026, 1, 31).add_months(1), Some(d(2026, 2, 28)));
        assert_eq!(d(2024, 1, 31).add_months(1), Some(d(2024, 2, 29)));
        assert_eq!(d(2026, 11, 5).add_months(2), Some(d(2027, 1, 5)));
        assert_eq!(d(2026, 1, 15).add_months(-1), Some(d(2025, 12, 15)));
        assert_eq!(d(2026, 3, 31).add_months(-12), Some(d(2025, 3, 31)));
        assert_eq!(d(9999, 12, 1).add_months(1), None);
        assert_eq!(d(1, 1, 1).add_months(-1), None);
    }

    #[test]
    fn a_month_page_is_six_weeks_starting_on_monday() {
        // November 2026 starts on a Sunday: the page starts the Monday before.
        let grid = Date::month_grid(2026, 11);
        assert_eq!(grid.len(), 42);
        assert_eq!(grid[0], Some(d(2026, 10, 26)));
        assert_eq!(grid[6], Some(d(2026, 11, 1)));
        assert_eq!(grid[35], Some(d(2026, 11, 30)));
        assert_eq!(grid[41], Some(d(2026, 12, 6)));
        for (i, cell) in grid.iter().enumerate() {
            assert_eq!(cell.map(|c| usize::from(c.weekday())), Some(i % 7));
        }
        // A month that starts on a Monday has no days from the month before.
        assert_eq!(Date::month_grid(2026, 6)[0], Some(d(2026, 6, 1)));
    }

    #[test]
    fn a_page_at_the_edge_of_time_has_gaps() {
        let grid = Date::month_grid(1, 1);
        assert_eq!(grid[0], Some(d(1, 1, 1)));
        let end = Date::month_grid(9999, 12);
        assert_eq!(end[41], None);
        assert!(Date::month_grid(2026, 13).is_empty());
    }

    #[test]
    fn dates_order_and_print() {
        assert!(d(2026, 11, 5) < d(2026, 11, 6));
        assert!(d(2026, 12, 1) > d(2026, 11, 30));
        assert!(d(2027, 1, 1) > d(2026, 12, 31));
        assert_eq!(d(2026, 11, 5).to_string(), "2026-11-05");
        assert_eq!(d(26, 1, 2).to_string(), "0026-01-02");
        assert_eq!(d(2026, 11, 5).long(), "5 November 2026");
    }
}
