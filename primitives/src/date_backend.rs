//! Date arithmetic backend used by [`crate::calendar`] and
//! [`crate::date_picker`].
//!
//! The [`DateBackend`] trait and the generic component implementations
//! in [`crate::calendar::generic`] / [`crate::date_picker::generic`]
//! are always compiled, regardless of which cargo features are active —
//! so downstream crates can implement [`DateBackend`] for their own
//! date library (e.g. `chrono`) without depending on a feature of
//! this crate.
//!
//! Two backends ship with the crate behind cargo features:
//!
//! - [`TimeBackend`] (feature `time`, on by default) wraps the
//!   [`time`] crate.
//! - [`JiffBackend`] (feature `jiff`) wraps [`jiff`].
//!
//! When at least one of those features is enabled, the corresponding
//! `time::` / `jiff::` sub-modules of [`crate::calendar`] /
//! [`crate::date_picker`] expose concrete, non-generic versions of the
//! components, and [`DefaultDateBackend`] resolves to whichever backend
//! is the "preferred" default (`TimeBackend` if `time` is enabled,
//! otherwise `JiffBackend`).

use std::fmt::{Debug, Display};

/// Abstraction over a calendar-date crate. Implementations are
/// provided for [`time`] (behind the `time` feature) and
/// [`jiff`] (behind the `jiff` feature).
pub trait DateBackend: Copy + Clone + PartialEq + Eq + Debug + 'static {
    /// Concrete calendar-date type.
    type Date: Copy + Clone + PartialEq + Eq + PartialOrd + Ord + Debug + Display + 'static;
    /// Concrete month enum.
    type Month: Copy + Clone + PartialEq + Eq + PartialOrd + Ord + Debug + Display + 'static;
    /// Concrete weekday enum.
    type Weekday: Copy + Clone + PartialEq + Eq + Debug + 'static;

    /// Today's date, in the local time zone where possible. Falls back
    /// to UTC if the runtime can't resolve the local offset.
    fn today() -> Self::Date;

    /// Construct a date from `(year, month, day)`. Returns `None` for
    /// invalid combinations (e.g. Feb 30).
    fn from_ymd(year: i32, month: Self::Month, day: u8) -> Option<Self::Date>;

    /// Year component of `date`.
    fn year(date: Self::Date) -> i32;
    /// Month component of `date`.
    fn month(date: Self::Date) -> Self::Month;
    /// Day-of-month component of `date` (1..=31).
    fn day(date: Self::Date) -> u8;
    /// Weekday component of `date`.
    fn weekday(date: Self::Date) -> Self::Weekday;

    /// Successor day. `None` at the type's upper bound.
    fn next_day(date: Self::Date) -> Option<Self::Date>;
    /// Predecessor day. `None` at the type's lower bound.
    fn previous_day(date: Self::Date) -> Option<Self::Date>;

    /// Replace the day-of-month component. `None` if the resulting
    /// date is invalid (e.g. Feb 30).
    fn replace_day(date: Self::Date, day: u8) -> Option<Self::Date>;

    /// Replace the month component, clamping the day if the new
    /// month has fewer days. Always succeeds.
    fn replace_month(date: Self::Date, month: Self::Month) -> Self::Date {
        let year = Self::year(date);
        let num_days = Self::month_length(month, year);
        Self::from_ymd(year, month, Self::day(date).min(num_days))
            .expect("from_ymd with clamped day must succeed")
    }

    /// Replace the year component. `None` if the resulting date is
    /// invalid (e.g. Feb 29 on a non-leap year).
    fn replace_year(date: Self::Date, year: i32) -> Option<Self::Date> {
        Self::from_ymd(year, Self::month(date), Self::day(date))
    }

    /// Saturating day arithmetic — clamps at the backend's bounds
    /// rather than panicking or returning `None`.
    fn saturating_add_days(date: Self::Date, days: i64) -> Self::Date;
    /// Saturating day arithmetic — clamps at the backend's bounds
    /// rather than panicking or returning `None`.
    fn saturating_sub_days(date: Self::Date, days: i64) -> Self::Date;

    /// Convert a month to its 1..=12 number.
    fn month_to_number(m: Self::Month) -> u8;
    /// Convert a 1..=12 number to a month. `None` for any other value.
    fn month_from_number(n: u8) -> Option<Self::Month>;

    /// Length of a month in days, year-aware (handles leap February).
    fn month_length(m: Self::Month, year: i32) -> u8;

    /// Next month, wrapping from December to January.
    fn next_month(m: Self::Month) -> Self::Month;
    /// Previous month, wrapping from January to December.
    fn previous_month(m: Self::Month) -> Self::Month;
    /// `n`-th month after `m`, wrapping at year boundaries.
    fn nth_next_month(m: Self::Month, n: u8) -> Self::Month;
    /// `n`-th month before `m`, wrapping at year boundaries.
    fn nth_prev_month(m: Self::Month, n: u8) -> Self::Month;

    /// Convert a weekday to its 0..=6 number (0 = Monday in ISO 8601 order).
    fn weekday_to_number(w: Self::Weekday) -> u8;
    /// Convert a 0..=6 number to a weekday. `None` for any other value.
    fn weekday_from_number(n: u8) -> Option<Self::Weekday>;

    /// `n`-th weekday after `w`, wrapping at the week boundary.
    fn nth_next_weekday(w: Self::Weekday, n: u8) -> Self::Weekday;

    /// January.
    const JANUARY: Self::Month;
    /// February.
    const FEBRUARY: Self::Month;
    /// March.
    const MARCH: Self::Month;
    /// April.
    const APRIL: Self::Month;
    /// May.
    const MAY: Self::Month;
    /// June.
    const JUNE: Self::Month;
    /// July.
    const JULY: Self::Month;
    /// August.
    const AUGUST: Self::Month;
    /// September.
    const SEPTEMBER: Self::Month;
    /// October.
    const OCTOBER: Self::Month;
    /// November.
    const NOVEMBER: Self::Month;
    /// December.
    const DECEMBER: Self::Month;

    /// Monday.
    const MONDAY: Self::Weekday;
    /// Tuesday.
    const TUESDAY: Self::Weekday;
    /// Wednesday.
    const WEDNESDAY: Self::Weekday;
    /// Thursday.
    const THURSDAY: Self::Weekday;
    /// Friday.
    const FRIDAY: Self::Weekday;
    /// Saturday.
    const SATURDAY: Self::Weekday;
    /// Sunday.
    const SUNDAY: Self::Weekday;
}

// ── Default backend selection ────────────────────────────────────────────

/// The backend used when calendar / date-picker components are
/// instantiated without an explicit type parameter. Resolves to
/// [`TimeBackend`] when the `time` feature is enabled (the crate
/// default) and otherwise to [`JiffBackend`].
#[cfg(feature = "time")]
pub type DefaultDateBackend = TimeBackend;

/// `jiff`-only fallback for [`DefaultDateBackend`] when the `time`
/// feature is disabled.
#[cfg(all(not(feature = "time"), feature = "jiff"))]
pub type DefaultDateBackend = JiffBackend;

// When neither `time` nor `jiff` is enabled, the [`DateBackend`] trait
// and its generic consumers in [`crate::calendar::generic`] /
// [`crate::date_picker::generic`] are still available — downstream
// crates can implement [`DateBackend`] for their own date library
// (e.g. `chrono`) and use the generic components directly.

// ── Time backend (behind `time` feature) ─────────────────────────────────

/// [`DateBackend`] using the [`time`] crate. Enabled by the
/// `time` cargo feature.
#[cfg(feature = "time")]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TimeBackend;

#[cfg(feature = "time")]
impl DateBackend for TimeBackend {
    type Date = time::Date;
    type Month = time::Month;
    type Weekday = time::Weekday;

    fn today() -> Self::Date {
        <time::OffsetDateTime as crate::LocalDateExt>::now_local_date()
    }

    fn from_ymd(year: i32, month: Self::Month, day: u8) -> Option<Self::Date> {
        time::Date::from_calendar_date(year, month, day).ok()
    }

    fn year(date: Self::Date) -> i32 {
        date.year()
    }
    fn month(date: Self::Date) -> Self::Month {
        date.month()
    }
    fn day(date: Self::Date) -> u8 {
        date.day()
    }
    fn weekday(date: Self::Date) -> Self::Weekday {
        date.weekday()
    }

    fn next_day(date: Self::Date) -> Option<Self::Date> {
        date.next_day()
    }
    fn previous_day(date: Self::Date) -> Option<Self::Date> {
        date.previous_day()
    }

    fn replace_day(date: Self::Date, day: u8) -> Option<Self::Date> {
        date.replace_day(day).ok()
    }

    fn saturating_add_days(date: Self::Date, days: i64) -> Self::Date {
        use time::ext::NumericalDuration;
        date.saturating_add(days.days())
    }
    fn saturating_sub_days(date: Self::Date, days: i64) -> Self::Date {
        use time::ext::NumericalDuration;
        date.saturating_sub(days.days())
    }

    fn month_to_number(m: Self::Month) -> u8 {
        u8::from(m)
    }
    fn month_from_number(n: u8) -> Option<Self::Month> {
        time::Month::try_from(n).ok()
    }
    fn month_length(m: Self::Month, year: i32) -> u8 {
        m.length(year)
    }
    fn next_month(m: Self::Month) -> Self::Month {
        m.next()
    }
    fn previous_month(m: Self::Month) -> Self::Month {
        m.previous()
    }
    fn nth_next_month(m: Self::Month, n: u8) -> Self::Month {
        m.nth_next(n)
    }
    fn nth_prev_month(m: Self::Month, n: u8) -> Self::Month {
        m.nth_prev(n)
    }

    fn weekday_to_number(w: Self::Weekday) -> u8 {
        w.number_days_from_monday()
    }
    fn weekday_from_number(n: u8) -> Option<Self::Weekday> {
        match n {
            0 => Some(time::Weekday::Monday),
            1 => Some(time::Weekday::Tuesday),
            2 => Some(time::Weekday::Wednesday),
            3 => Some(time::Weekday::Thursday),
            4 => Some(time::Weekday::Friday),
            5 => Some(time::Weekday::Saturday),
            6 => Some(time::Weekday::Sunday),
            _ => None,
        }
    }
    fn nth_next_weekday(w: Self::Weekday, n: u8) -> Self::Weekday {
        w.nth_next(n)
    }

    const JANUARY: Self::Month = time::Month::January;
    const FEBRUARY: Self::Month = time::Month::February;
    const MARCH: Self::Month = time::Month::March;
    const APRIL: Self::Month = time::Month::April;
    const MAY: Self::Month = time::Month::May;
    const JUNE: Self::Month = time::Month::June;
    const JULY: Self::Month = time::Month::July;
    const AUGUST: Self::Month = time::Month::August;
    const SEPTEMBER: Self::Month = time::Month::September;
    const OCTOBER: Self::Month = time::Month::October;
    const NOVEMBER: Self::Month = time::Month::November;
    const DECEMBER: Self::Month = time::Month::December;

    const MONDAY: Self::Weekday = time::Weekday::Monday;
    const TUESDAY: Self::Weekday = time::Weekday::Tuesday;
    const WEDNESDAY: Self::Weekday = time::Weekday::Wednesday;
    const THURSDAY: Self::Weekday = time::Weekday::Thursday;
    const FRIDAY: Self::Weekday = time::Weekday::Friday;
    const SATURDAY: Self::Weekday = time::Weekday::Saturday;
    const SUNDAY: Self::Weekday = time::Weekday::Sunday;
}

// ── Jiff backend (behind `jiff` feature) ─────────────────────────────────

#[cfg(feature = "jiff")]
mod jiff_backend {
    use super::DateBackend;
    use jiff::civil::{Date, Weekday};
    use jiff::ToSpan;

    /// [`DateBackend`] using the [`jiff`] crate. Enabled by
    /// the `jiff` cargo feature.
    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    pub struct JiffBackend;

    impl DateBackend for JiffBackend {
        type Date = Date;
        type Month = JiffMonth;
        type Weekday = Weekday;

        fn today() -> Self::Date {
            // Prefer the local time zone, fall back to UTC if tzdb is
            // unavailable so we don't panic the way `Zoned::now()` will.
            jiff::tz::TimeZone::try_system()
                .map(|tz| jiff::Timestamp::now().to_zoned(tz))
                .unwrap_or_else(|_| jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC))
                .date()
        }

        fn from_ymd(year: i32, month: Self::Month, day: u8) -> Option<Self::Date> {
            let year: i16 = year.try_into().ok()?;
            Date::new(year, month.0, day as i8).ok()
        }

        fn year(date: Self::Date) -> i32 {
            date.year() as i32
        }
        fn month(date: Self::Date) -> Self::Month {
            JiffMonth(date.month())
        }
        fn day(date: Self::Date) -> u8 {
            date.day() as u8
        }
        fn weekday(date: Self::Date) -> Self::Weekday {
            date.weekday()
        }

        fn next_day(date: Self::Date) -> Option<Self::Date> {
            date.checked_add(1.day()).ok()
        }
        fn previous_day(date: Self::Date) -> Option<Self::Date> {
            date.checked_sub(1.day()).ok()
        }

        fn replace_day(date: Self::Date, day: u8) -> Option<Self::Date> {
            Date::new(date.year(), date.month(), day as i8).ok()
        }

        fn saturating_add_days(date: Self::Date, days: i64) -> Self::Date {
            date.saturating_add(days.days())
        }
        fn saturating_sub_days(date: Self::Date, days: i64) -> Self::Date {
            date.saturating_sub(days.days())
        }

        fn month_to_number(m: Self::Month) -> u8 {
            m.0 as u8
        }
        fn month_from_number(n: u8) -> Option<Self::Month> {
            if (1..=12).contains(&n) {
                Some(JiffMonth(n as i8))
            } else {
                None
            }
        }
        fn month_length(m: Self::Month, year: i32) -> u8 {
            // Delegate to jiff; out-of-range year (or invalid month) yields
            // 0 the same way the trait's contract treats invalid input.
            let Ok(year): Result<i16, _> = year.try_into() else {
                return 0;
            };
            Date::new(year, m.0, 1)
                .map(|d| d.days_in_month() as u8)
                .unwrap_or(0)
        }
        fn next_month(m: Self::Month) -> Self::Month {
            JiffMonth(if m.0 == 12 { 1 } else { m.0 + 1 })
        }
        fn previous_month(m: Self::Month) -> Self::Month {
            JiffMonth(if m.0 == 1 { 12 } else { m.0 - 1 })
        }
        fn nth_next_month(m: Self::Month, n: u8) -> Self::Month {
            // Promote to i32 so any u8 value handles correctly; rem_euclid
            // normalizes the sum.
            let shifted = (m.0 as i32 - 1 + n as i32).rem_euclid(12);
            JiffMonth((shifted + 1) as i8)
        }
        fn nth_prev_month(m: Self::Month, n: u8) -> Self::Month {
            let shifted = (m.0 as i32 - 1 - n as i32).rem_euclid(12);
            JiffMonth((shifted + 1) as i8)
        }

        fn weekday_to_number(w: Self::Weekday) -> u8 {
            // jiff: Monday=1..Sunday=7. Match the trait contract
            // (0..=6 starting Monday).
            (w.to_monday_one_offset() - 1) as u8
        }
        fn weekday_from_number(n: u8) -> Option<Self::Weekday> {
            Weekday::from_monday_one_offset((n as i8) + 1).ok()
        }
        fn nth_next_weekday(w: Self::Weekday, n: u8) -> Self::Weekday {
            w.wrapping_add(n as i8)
        }

        const JANUARY: Self::Month = JiffMonth(1);
        const FEBRUARY: Self::Month = JiffMonth(2);
        const MARCH: Self::Month = JiffMonth(3);
        const APRIL: Self::Month = JiffMonth(4);
        const MAY: Self::Month = JiffMonth(5);
        const JUNE: Self::Month = JiffMonth(6);
        const JULY: Self::Month = JiffMonth(7);
        const AUGUST: Self::Month = JiffMonth(8);
        const SEPTEMBER: Self::Month = JiffMonth(9);
        const OCTOBER: Self::Month = JiffMonth(10);
        const NOVEMBER: Self::Month = JiffMonth(11);
        const DECEMBER: Self::Month = JiffMonth(12);

        const MONDAY: Self::Weekday = Weekday::Monday;
        const TUESDAY: Self::Weekday = Weekday::Tuesday;
        const WEDNESDAY: Self::Weekday = Weekday::Wednesday;
        const THURSDAY: Self::Weekday = Weekday::Thursday;
        const FRIDAY: Self::Weekday = Weekday::Friday;
        const SATURDAY: Self::Weekday = Weekday::Saturday;
        const SUNDAY: Self::Weekday = Weekday::Sunday;
    }

    /// Wrapper around `jiff::civil`'s month-as-`i8` representation.
    /// Carries the same range invariant (1..=12) as `time::Month` and
    /// implements `Display` (`January`..`December`) so the trait
    /// bounds line up.
    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
    pub struct JiffMonth(pub i8);

    impl std::fmt::Display for JiffMonth {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let name = match self.0 {
                1 => "January",
                2 => "February",
                3 => "March",
                4 => "April",
                5 => "May",
                6 => "June",
                7 => "July",
                8 => "August",
                9 => "September",
                10 => "October",
                11 => "November",
                12 => "December",
                _ => "Invalid",
            };
            f.write_str(name)
        }
    }
}

#[cfg(feature = "jiff")]
pub use jiff_backend::{JiffBackend, JiffMonth};

#[cfg(test)]
mod tests {
    use super::*;

    // `DefaultDateBackend` should be `TimeBackend` whenever the `time`
    // feature is enabled (whether `jiff` is also enabled or not). When
    // only `jiff` is on, it falls back to `JiffBackend`. We assert
    // identity via the trait's associated `Date` type, which uniquely
    // identifies a backend.
    #[cfg(feature = "time")]
    #[test]
    fn default_backend_resolves_to_time_when_feature_enabled() {
        fn assert_same<A: 'static, B: 'static>() {
            assert_eq!(std::any::TypeId::of::<A>(), std::any::TypeId::of::<B>());
        }
        assert_same::<<DefaultDateBackend as DateBackend>::Date, time::Date>();
    }

    #[cfg(all(not(feature = "time"), feature = "jiff"))]
    #[test]
    fn default_backend_resolves_to_jiff_when_only_jiff_enabled() {
        fn assert_same<A: 'static, B: 'static>() {
            assert_eq!(std::any::TypeId::of::<A>(), std::any::TypeId::of::<B>());
        }
        assert_same::<<DefaultDateBackend as DateBackend>::Date, jiff::civil::Date>();
    }

    #[cfg(feature = "jiff")]
    mod jiff_backend_tests {
        use super::*;

        // `JiffBackend::today()` must not panic in environments without a
        // platform tzdb. We can't easily *force* tzdb to be unavailable in
        // unit tests, but we can at least assert the function returns a
        // reasonable date when run in a normal environment.
        #[test]
        fn today_returns_a_plausible_date() {
            let date = JiffBackend::today();
            // Years are 16-bit in jiff but easily within i32 range; check
            // the value is plausibly within the supported window
            // (jiff covers years -9999..=9999).
            let year = JiffBackend::year(date);
            assert!(year > 1900 && year < 3000, "today's year was {year}");
            // Day and month are within their natural ranges.
            assert!(JiffBackend::day(date) >= 1);
            assert!(JiffBackend::day(date) <= 31);
            let month_num = JiffBackend::month_to_number(JiffBackend::month(date));
            assert!((1..=12).contains(&month_num));
        }

        // Leap-year month-length math: February has 29 days in years
        // divisible by 4 but not 100 (unless also divisible by 400).
        #[test]
        fn month_length_handles_leap_february() {
            // Standard 4-year leap.
            assert_eq!(JiffBackend::month_length(JiffBackend::FEBRUARY, 2024), 29);
            // Non-leap regular year.
            assert_eq!(JiffBackend::month_length(JiffBackend::FEBRUARY, 2023), 28);
            // Century non-leap.
            assert_eq!(JiffBackend::month_length(JiffBackend::FEBRUARY, 1900), 28);
            // 400-divisible leap.
            assert_eq!(JiffBackend::month_length(JiffBackend::FEBRUARY, 2000), 29);
            // Boundary years.
            assert_eq!(JiffBackend::month_length(JiffBackend::FEBRUARY, 2100), 28);
        }

        // Non-February months have fixed lengths regardless of year.
        #[test]
        fn month_length_for_31_day_months() {
            for m in [
                JiffBackend::JANUARY,
                JiffBackend::MARCH,
                JiffBackend::MAY,
                JiffBackend::JULY,
                JiffBackend::AUGUST,
                JiffBackend::OCTOBER,
                JiffBackend::DECEMBER,
            ] {
                assert_eq!(JiffBackend::month_length(m, 2024), 31);
            }
        }

        #[test]
        fn month_length_for_30_day_months() {
            for m in [
                JiffBackend::APRIL,
                JiffBackend::JUNE,
                JiffBackend::SEPTEMBER,
                JiffBackend::NOVEMBER,
            ] {
                assert_eq!(JiffBackend::month_length(m, 2024), 30);
            }
        }

        // `nth_next_month` / `nth_prev_month` must handle the full u8 range
        // without wrapping into negatives (the previous `n as i8` cast was
        // buggy for n >= 128).
        #[test]
        fn nth_next_month_handles_large_offsets() {
            // Equivalent classes mod 12.
            assert_eq!(
                JiffBackend::nth_next_month(JiffBackend::JANUARY, 0),
                JiffBackend::JANUARY
            );
            assert_eq!(
                JiffBackend::nth_next_month(JiffBackend::JANUARY, 12),
                JiffBackend::JANUARY
            );
            assert_eq!(
                JiffBackend::nth_next_month(JiffBackend::JANUARY, 13),
                JiffBackend::FEBRUARY
            );
            // Large u8 (formerly wrapped to negative under `as i8`).
            assert_eq!(
                JiffBackend::nth_next_month(JiffBackend::JANUARY, 200),
                // 200 % 12 = 8; January + 8 months → September
                JiffBackend::SEPTEMBER
            );
            assert_eq!(
                JiffBackend::nth_next_month(JiffBackend::MARCH, 255),
                // 255 % 12 = 3 → March + 3 = June
                JiffBackend::JUNE
            );
        }

        #[test]
        fn nth_prev_month_handles_large_offsets() {
            assert_eq!(
                JiffBackend::nth_prev_month(JiffBackend::JANUARY, 1),
                JiffBackend::DECEMBER
            );
            assert_eq!(
                JiffBackend::nth_prev_month(JiffBackend::JANUARY, 13),
                JiffBackend::DECEMBER
            );
            assert_eq!(
                JiffBackend::nth_prev_month(JiffBackend::JANUARY, 200),
                // 200 % 12 = 8; January back 8 → May
                JiffBackend::MAY
            );
        }
    }

    #[cfg(feature = "time")]
    mod time_backend_tests {
        use super::*;

        #[test]
        fn today_returns_a_plausible_date() {
            let date = TimeBackend::today();
            let year = TimeBackend::year(date);
            assert!(year > 1900 && year < 3000, "today's year was {year}");
            assert!(TimeBackend::day(date) >= 1);
            assert!(TimeBackend::day(date) <= 31);
            let month_num = TimeBackend::month_to_number(TimeBackend::month(date));
            assert!((1..=12).contains(&month_num));
        }

        // Cross-check that `time::Month::length` (which `TimeBackend`
        // delegates to) agrees with our jiff-side leap-year arithmetic
        // for the edge cases.
        #[test]
        fn month_length_handles_leap_february() {
            assert_eq!(TimeBackend::month_length(TimeBackend::FEBRUARY, 2024), 29);
            assert_eq!(TimeBackend::month_length(TimeBackend::FEBRUARY, 2023), 28);
            assert_eq!(TimeBackend::month_length(TimeBackend::FEBRUARY, 1900), 28);
            assert_eq!(TimeBackend::month_length(TimeBackend::FEBRUARY, 2000), 29);
            assert_eq!(TimeBackend::month_length(TimeBackend::FEBRUARY, 2100), 28);
        }
    }
}
