# 0062: Date picker: a kit civil-date type and two leaf widgets, one per style

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.24c. Mountain Lion's date picker has a textual style (segments with a stepper) and a
graphical calendar. The kit needs both, with the keyboard and AccessKit, and a date value
for apps, without a date-time dependency (decided in the 2.24 plan).

## Decision
1. **`neue_kit::Date` is a civil date**: year 1 to 9999, month, day, no time and no zone.
   It has the maths the picker needs (`days_in_month`, `is_leap_year`, `weekday` from
   Monday, `add_days`, `add_months` clamping to the month's end, a 42-cell `month_grid`),
   done with Howard Hinnant's day-count algorithms, and `Display` as ISO 8601. `Date::today`
   is the system date in **UTC**, because the local zone needs a library; apps that know
   better pass their own to `CalendarPicker::today`.
2. **Two builders, one per style, because each is its own widget.** `DatePicker::new(date)`
   is the textual style; `DatePicker::calendar(date)` returns a `CalendarPicker`. Each has
   `on_change(Date)`, `label`, `disabled`, `bind`, and a `Handle` with `set_date` and
   `set_enabled`; the same split as `PathControl::popup` (ADR 0060), so a `Handle` stays
   typed to the one widget it can edit. Both send only changes the user makes, and only
   when the date differs.
3. **Both are leaf widgets** that draw and hit test themselves, like the path control and
   the segmented control: the segments and the 42 day cells are text and a fill, not child
   widgets. The look is `neue_paint::materials::date_picker` (`date_field`, `calendar`,
   `calendar_layout`), tokens in `neue_theme::materials::date_picker`. The field reuses the
   text field's well and the stepper material.
4. **Textual style.** Segments in the order day, month, year (a constant, `ORDER`, for
   locales later), each laid out at the width of `00` or `0000` so they don't move while
   digits change. One focus stop; Left and Right choose the segment, Up and Down or the
   stepper's halves change it. Day and month wrap, the year stops at the ends, and the day
   is shortened to fit when the month or year changes. Digits are typed into the segment:
   the day and month take effect with each digit, the year with its fourth (or Return), and
   the focus moves on when no further digit could belong. Escape or Backspace drop what was
   typed; leaving the field does too.
5. **Calendar.** A header (month and year, previous, today and next buttons) over the weekday
   names and six rows of days, Monday first, with days of the neighbouring months dimmed.
   Clicking a day selects it and, if it belongs to another month, turns the page. The
   arrows browse without changing the date; the circle selects today. Keys: arrows by a day
   or a week, Page Up and Page Down by a month, Home to today; the shown month follows the
   selection.
6. **AccessKit.** Textual: a `Group` with the long date ("5 November 2026") as its value and
   three `SpinButton` children (Day, Month, Year, with value, minimum and maximum);
   Increment and Decrement on the group change the selected segment. Calendar: a `Group`
   with three `Button`s and a `Grid` of seven `Row`s, the first of `ColumnHeader`s and six of
   `GridCell`s labelled with the long date, the selected one flagged and today described as
   "Today"; Increment and Decrement move the selection a day. As in ADR 0060, Masonry
   doesn't say which synthetic child an action was for, so cells and buttons can be read but
   not clicked individually.

## Consequences
- English month and weekday names, day-month-year order and a Monday start are fixed until
  there is a locale story.
- No minimum and maximum date, no time of day, no date ranges, and no clock style.
- "Today" can be a day off in the evening or morning, away from UTC.
- The calendar's selection is drawn in the focused selection colour even when the window is
  inactive, and a held stepper doesn't repeat.
