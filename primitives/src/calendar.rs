//! Defines the [`Calendar`] component and its sub-components, which provide a calendar interface with date selection and navigation.
//!
//! The backend-generic implementations of every component, prop, and context
//! live in [`generic`] and are always available — downstream crates can
//! implement [`crate::date_backend::DateBackend`] for their own date library
//! and use them directly.
//!
//! When the `time` or `jiff` cargo features are enabled, the [`time`] / [`jiff`]
//! sub-modules expose concrete, non-generic versions of the same components
//! pinned to those backends. With the default `time` feature on, those items
//! are also re-exported at the root of this module so existing call sites need
//! no turbofish. Builds with `time` disabled (`--no-default-features --features
//! jiff`) have no root-level surface; reach for [`jiff::Calendar`] etc.
//! directly. This keeps feature combinations additive — enabling `time` on
//! top of `jiff` never changes which backend the root resolves to.

/// The backend-generic implementations of every calendar component, prop, and
/// context. Most users should reach for the backend-pinned re-exports in
/// [`crate::calendar`] (or [`crate::calendar::time`] / [`crate::calendar::jiff`])
/// instead.
pub mod generic {
    use dioxus::{
        core::{current_scope_id, ScopeId},
        prelude::*,
    };
    use std::{
        collections::HashSet,
        fmt::{self, Display},
        rc::Rc,
    };

    use crate::date_backend::DateBackend;
    use crate::{date_picker::generic::DefaultCalendarProps, use_effect_cleanup};

    // A collection of weekdays stored as a single u8 bitmask. Bits 0..=6
    // correspond to Monday..Sunday in ISO 8601 order. The bitmask itself
    // is backend-agnostic; conversions to a typed `B::Weekday` happen
    // at the fn level via `B::weekday_from_number` / `B::weekday_to_number`.
    #[derive(Clone, Copy)]
    struct WeekdaySet(u8); // the 8-th bit is always 0

    impl WeekdaySet {
        fn iter<B: DateBackend>(self, start: B::Weekday) -> WeekdaySetIter<B> {
            WeekdaySetIter {
                days: self,
                start: B::weekday_to_number(start),
                _phantom: std::marker::PhantomData,
            }
        }

        const fn is_empty(self) -> bool {
            self.0 == 0
        }

        fn first_number(self) -> Option<u8> {
            if self.is_empty() {
                None
            } else {
                Some(self.0.trailing_zeros() as u8)
            }
        }

        fn split_at_number(self, n: u8) -> (Self, Self) {
            let days_after = 0b1000_0000 - (1 << n);
            let days_before = days_after ^ 0b0111_1111;
            (Self(self.0 & days_before), Self(self.0 & days_after))
        }
    }

    // `single`, `contains`, and `remove` are only exercised from the test
    // module; gating them with cfg(test) avoids the dead-code warning
    // while keeping the production lib slim.
    #[cfg(test)]
    impl WeekdaySet {
        fn single<B: DateBackend>(weekday: B::Weekday) -> Self {
            Self(1 << B::weekday_to_number(weekday))
        }

        fn contains<B: DateBackend>(self, day: B::Weekday) -> bool {
            self.0 & Self::single::<B>(day).0 != 0
        }

        fn remove<B: DateBackend>(&mut self, day: B::Weekday) -> bool {
            if self.contains::<B>(day) {
                self.0 &= !Self::single::<B>(day).0;
                return true;
            }
            false
        }
    }

    // An iterator over a WeekdaySet starting from a given weekday. Carries
    // `PhantomData<B>` so it can implement `Iterator<Item = B::Weekday>`
    // directly, restoring idiomatic `for weekday in iter` usage at call sites.
    struct WeekdaySetIter<B: DateBackend> {
        days: WeekdaySet,
        start: u8,
        _phantom: std::marker::PhantomData<B>,
    }

    impl<B: DateBackend> Iterator for WeekdaySetIter<B> {
        type Item = B::Weekday;

        fn next(&mut self) -> Option<Self::Item> {
            if self.days.is_empty() {
                return None;
            }
            let (before, after) = self.days.split_at_number(self.start);
            let days = if after.is_empty() { before } else { after };
            let next = days.first_number().expect("non-empty");
            self.days.0 &= !(1u8 << next);
            B::weekday_from_number(next)
        }
    }

    pub(crate) fn weekday_abbreviation<B: DateBackend>(weekday: B::Weekday) -> &'static str {
        match B::weekday_to_number(weekday) {
            0 => "Mo",
            1 => "Tu",
            2 => "We",
            3 => "Th",
            4 => "Fr",
            5 => "Sa",
            6 => "Su",
            _ => unreachable!("weekday_to_number is contracted to return 0..=6"),
        }
    }

    // The number of days since the first weekday of current date
    fn days_since<B: DateBackend>(date: B::Date, weekday: B::Weekday) -> i64 {
        let first_of_month = B::replace_day(date, 1).unwrap();
        let lhs = B::weekday_to_number(B::weekday(first_of_month)) as i64;
        let rhs = B::weekday_to_number(weekday) as i64;
        if lhs < rhs {
            7 + lhs - rhs
        } else {
            lhs - rhs
        }
    }

    fn next_month<B: DateBackend>(date: B::Date) -> Option<B::Date> {
        let next_month = B::next_month(B::month(date));
        let last_day = B::month_length(next_month, B::year(date));
        let current_day = B::day(date);
        let new_day = current_day.min(last_day);
        B::from_ymd(
            B::year(date) + if next_month == B::JANUARY { 1 } else { 0 },
            next_month,
            new_day,
        )
    }

    fn previous_month<B: DateBackend>(date: B::Date) -> Option<B::Date> {
        let previous_month = B::previous_month(B::month(date));
        let last_day = B::month_length(previous_month, B::year(date));
        let current_day = B::day(date);
        let new_day = current_day.min(last_day);
        B::from_ymd(
            B::year(date) + if previous_month == B::DECEMBER { -1 } else { 0 },
            previous_month,
            new_day,
        )
    }

    fn replace_month<B: DateBackend>(date: B::Date, month: B::Month) -> B::Date {
        let year = B::year(date);
        let num_days = B::month_length(month, year);
        B::from_ymd(year, month, std::cmp::min(B::day(date), num_days))
            .expect("invalid or out-of-range date")
    }

    /// Move forward n months from the given date, handling year transitions
    fn nth_month_next<B: DateBackend>(date: B::Date, n: u8) -> Option<B::Date> {
        match n {
            0 => Some(date),
            n => {
                let month = B::month(date);
                let nth_month = B::nth_next_month(month, n);
                let year = B::year(date) + if month > nth_month { 1 } else { 0 };
                let max_day = B::month_length(nth_month, year);
                B::from_ymd(year, nth_month, B::day(date).min(max_day))
            }
        }
    }

    /// Move backward n months from the given date, handling year transitions
    fn nth_month_previous<B: DateBackend>(date: B::Date, n: u8) -> Option<B::Date> {
        match n {
            0 => Some(date),
            n => {
                let month = B::month(date);
                let nth_month = B::nth_prev_month(month, n);
                let year = B::year(date) - if month < nth_month { 1 } else { 0 };
                let max_day = B::month_length(nth_month, year);
                B::from_ymd(year, nth_month, B::day(date).min(max_day))
            }
        }
    }

    /// Calendar date range
    #[derive(Copy, Clone, PartialEq, Debug)]
    pub struct DateRange<B: DateBackend> {
        /// The start date of the range
        start: B::Date,
        /// The end date of the range
        end: B::Date,
    }

    impl<B: DateBackend> DateRange<B> {
        /// Create a new date range
        pub fn new(start: B::Date, end: B::Date) -> Self {
            if start <= end {
                Self { start, end }
            } else {
                Self {
                    start: end,
                    end: start,
                }
            }
        }

        /// Returns true if date is contained in the range.
        pub fn contains(&self, date: B::Date) -> bool {
            self.start <= date && date <= self.end
        }

        fn contained_in_interval(&self, date: B::Date) -> bool {
            self.start < date && date < self.end
        }

        fn clamp(&self, date: B::Date) -> B::Date {
            date.clamp(self.start, self.end)
        }

        /// Get the start of the range
        pub fn start(&self) -> B::Date {
            self.start
        }

        /// Get the end of the range
        pub fn end(&self) -> B::Date {
            self.end
        }
    }

    impl<B: DateBackend> Display for DateRange<B> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{} - {}", self.start, self.end)
        }
    }

    /// Calendar available dates
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct AvailableRanges<B: DateBackend> {
        /// A sorted list of dates. Values after an odd number of elements are disabled.
        changes: Vec<B::Date>,
    }

    impl<B: DateBackend> AvailableRanges<B> {
        /// Create a new available dates
        pub fn new(disabled_ranges: &[DateRange<B>]) -> Self {
            let mut sorted_range: Vec<_> = disabled_ranges
                .iter()
                .enumerate()
                .flat_map(|(index, date)| [(index, date.start), (index, date.end)])
                .collect();

            sorted_range.sort_by_key(|(_, date)| *date);

            // Merge any overlapping ranges
            let mut open_ranges = HashSet::new();
            let mut deduped_ranges = Vec::with_capacity(sorted_range.len());

            for (index, date) in sorted_range {
                let end_of_range = open_ranges.remove(&index);
                if open_ranges.is_empty() {
                    deduped_ranges.push(date);
                }
                if !end_of_range {
                    open_ranges.insert(index);
                }
            }

            Self {
                changes: deduped_ranges,
            }
        }

        /// Check the availability of given date
        pub fn valid_interval(&self, date: B::Date) -> bool {
            match self.changes.binary_search(&date) {
                Ok(_) => false,
                Err(index) => index % 2 == 0,
            }
        }

        /// Get the available range of given date
        pub fn available_range(
            &self,
            date: B::Date,
            date_range: DateRange<B>,
        ) -> Option<DateRange<B>> {
            let date_index = self.changes.binary_search(&date).err()?;
            let min_date = date_range.start();
            let max_date = date_range.end();

            let valid = date_index % 2 == 0;
            if !valid {
                return None;
            }

            let start = date_index
                .checked_sub(1)
                .and_then(|index| self.changes.get(index).copied())
                .map(|start| B::next_day(start).unwrap_or(start))
                .unwrap_or(min_date);
            let end = self
                .changes
                .get(date_index)
                .copied()
                .map(|end| B::previous_day(end).unwrap_or(end))
                .unwrap_or(max_date);

            Some(DateRange::<B>::new(start, end))
        }

        /// Get disabled ranges
        pub fn to_disabled_ranges(&self) -> Vec<DateRange<B>> {
            self.changes
                .chunks(2)
                .map(|d| DateRange::<B>::new(d[0], d[1]))
                .collect()
        }
    }

    /// The base context provided by the [`Calendar`] and the [`RangeCalendar`] component to its children.
    #[derive(Copy, Clone)]
    pub struct BaseCalendarContext<B: DateBackend> {
        // State
        focused_date: Signal<Option<B::Date>>,
        view_date: ReadSignal<B::Date>,
        available_ranges: Memo<AvailableRanges<B>>,
        set_view_date: Callback<B::Date>,
        format_weekday: Callback<B::Weekday, String>,
        format_month: Callback<B::Month, String>,

        // Configuration
        disabled: ReadSignal<bool>,
        today: B::Date,
        first_day_of_week: B::Weekday,
        enabled_date_range: DateRange<B>,
        view_registrations: Signal<Vec<CalendarViewRegistration>>,
    }

    #[derive(Clone, Copy, PartialEq)]
    struct CalendarViewRegistration {
        id: ScopeId,
        offset: Option<u8>,
    }

    impl<B: DateBackend> BaseCalendarContext<B> {
        /// Get the currently focused date
        pub fn focused_date(&self) -> Option<B::Date> {
            self.focused_date.cloned()
        }

        /// Set the focused date
        pub fn set_focused_date(&mut self, date: Option<B::Date>) {
            self.focused_date.set(date);
        }

        /// Get the current view date
        pub fn view_date(&self) -> B::Date {
            self.view_date.cloned()
        }

        /// Set the view date
        pub fn set_view_date(&self, date: B::Date) {
            (self.set_view_date)(self.enabled_date_range.clamp(date));
        }

        /// Check if the calendar is disabled
        pub fn is_disabled(&self) -> bool {
            self.disabled.cloned()
        }

        /// Check if the selected date is unavailable
        pub fn is_unavailable(&self, date: B::Date) -> bool {
            !self.available_ranges.read().valid_interval(date)
        }

        /// Check if a date is focused
        pub fn is_focused(&self, date: B::Date) -> bool {
            self.focused_date().is_some_and(|d| d == date)
        }

        /// Return available date range by given date
        pub fn available_range(&self) -> Option<DateRange<B>> {
            try_consume_context::<RangeCalendarContext<B>>().and_then(|ctx| {
                ctx.anchor_date.cloned().and_then(|date| {
                    self.available_ranges
                        .read()
                        .available_range(date, self.enabled_date_range)
                })
            })
        }

        fn visible_month_count(&self) -> u8 {
            self.view_registrations
                .read()
                .iter()
                .enumerate()
                .map(|(index, view)| {
                    view.offset
                        .unwrap_or_else(|| u8::try_from(index).unwrap_or(u8::MAX))
                        .saturating_add(1)
                })
                .max()
                .unwrap_or(1)
        }

        fn calendar_view_offset(&self, id: ScopeId, offset: Option<u8>) -> u8 {
            offset.unwrap_or_else(|| {
                self.view_registrations
                    .read()
                    .iter()
                    .position(|view| view.id == id)
                    .and_then(|index| u8::try_from(index).ok())
                    .unwrap_or_default()
            })
        }

        fn register_calendar_view(&self, id: ScopeId, offset: Option<u8>) {
            if self
                .view_registrations
                .read()
                .iter()
                .any(|view| view.id == id && view.offset == offset)
            {
                return;
            }

            let mut view_registrations_signal = self.view_registrations;
            let mut view_registrations = view_registrations_signal.write();
            if let Some(view) = view_registrations.iter_mut().find(|view| view.id == id) {
                view.offset = offset;
            } else {
                view_registrations.push(CalendarViewRegistration { id, offset });
            }
        }

        fn unregister_calendar_view(&self, id: ScopeId) {
            if !self
                .view_registrations
                .read()
                .iter()
                .any(|view| view.id == id)
            {
                return;
            }

            let mut view_registrations = self.view_registrations;
            view_registrations.write().retain(|view| view.id != id);
        }
    }

    /// The context provided by the [`Calendar`] component to its children.
    #[derive(Copy, Clone)]
    pub struct CalendarContext<B: DateBackend> {
        selected_date: ReadSignal<Option<B::Date>>,
        set_selected_date: Callback<Option<B::Date>>,
    }

    impl<B: DateBackend> CalendarContext<B> {
        /// Get the currently selected date
        pub fn selected_date(&self) -> Option<B::Date> {
            self.selected_date.cloned()
        }

        /// Set the selected date
        pub fn set_selected_date(&self, date: Option<B::Date>) {
            (self.set_selected_date)(date);
        }
    }

    /// The props for the [`Calendar`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarProps<B: DateBackend> {
        /// The selected date
        #[props(default)]
        pub selected_date: ReadSignal<Option<B::Date>>,

        /// Callback when selected date changes
        #[props(default)]
        pub on_date_change: Callback<Option<B::Date>>,

        /// Callback when display weekday
        #[props(default = Callback::new(|weekday: B::Weekday| weekday_abbreviation::<B>(weekday).to_string()))]
        pub on_format_weekday: Callback<B::Weekday, String>,

        /// Callback when display month
        #[props(default = Callback::new(|month: B::Month| month.to_string()))]
        pub on_format_month: Callback<B::Month, String>,

        /// The month being viewed
        #[props(default = ReadSignal::new(Signal::new(B::today())))]
        pub view_date: ReadSignal<B::Date>,

        /// The current date (used for highlighting today)
        #[props(default = B::today())]
        pub today: B::Date,

        /// Callback when view date changes
        #[props(default)]
        pub on_view_change: Callback<B::Date>,

        /// Whether the calendar is disabled
        #[props(default)]
        pub disabled: ReadSignal<bool>,

        /// First day of the week
        #[props(default = B::SUNDAY)]
        pub first_day_of_week: B::Weekday,

        /// Lower limit of the range of available dates
        #[props(default = B::from_ymd(1925, B::JANUARY, 1).unwrap())]
        pub min_date: B::Date,

        /// Upper limit of the range of available dates
        #[props(default = B::from_ymd(2050, B::DECEMBER, 31).unwrap())]
        pub max_date: B::Date,

        /// Unavailable dates
        #[props(default)]
        pub disabled_ranges: ReadSignal<Vec<DateRange<B>>>,

        /// Additional attributes to extend the calendar element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the calendar element
        pub children: Element,
    }

    impl<B: DateBackend> DefaultCalendarProps for CalendarProps<B> {
        fn default_calendar(self) -> Element {
            Calendar(self)
        }
    }

    /// # Calendar
    ///
    /// The [`Calendar`] component provides an accessible calendar interface with arrow key navigation, month switching, and date selection.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Styling
    ///
    /// The [`Calendar`] component defines the following data attributes you can use to control styling:
    /// - `data-disabled`: Indicates if the calendar is disabled. Possible values are `true` or `false`.
    #[component]
    pub fn Calendar<B: DateBackend>(props: CalendarProps<B>) -> Element {
        let available_ranges =
            use_memo(move || AvailableRanges::<B>::new(&props.disabled_ranges.read()));
        let view_registrations = use_signal(Vec::new);

        // Create base context provider for child components
        let mut base_ctx = use_context_provider(|| BaseCalendarContext::<B> {
            focused_date: Signal::new(None),
            view_date: props.view_date,
            set_view_date: props.on_view_change,
            available_ranges,
            format_weekday: props.on_format_weekday,
            format_month: props.on_format_month,
            disabled: props.disabled,
            today: props.today,
            first_day_of_week: props.first_day_of_week,
            enabled_date_range: DateRange::<B>::new(props.min_date, props.max_date),
            view_registrations,
        });
        // Create Calendar context provider for child components
        use_context_provider(|| CalendarContext::<B> {
            selected_date: props.selected_date,
            set_selected_date: props.on_date_change,
        });

        rsx! {
            div {
                role: "application",
                aria_label: "Calendar",
                "data-disabled": (props.disabled)(),
                onkeydown: move |e| {
                    let Some(focused_date) = (base_ctx.focused_date)() else {
                        return;
                    };
                    let mut set_focused_date = |new_date: Option<B::Date>| {
                        if let Some(date) = new_date {
                            let min_date = B::replace_day((base_ctx.view_date)(), 1).unwrap();
                            if date < min_date {
                                let view_date = previous_month::<B>(min_date).unwrap_or(min_date);
                                (base_ctx.set_view_date)(view_date);
                            } else {
                                let max_date = nth_month_next::<B>(min_date, base_ctx.visible_month_count())
                                    .unwrap_or(min_date);
                                if date >= max_date {
                                    let view_date = next_month::<B>(min_date).unwrap_or(min_date);
                                    (base_ctx.set_view_date)(view_date);
                                }
                            }
                        }
                        match new_date {
                            Some(date) => {
                                if base_ctx.enabled_date_range.contains(date) {
                                    base_ctx.focused_date.set(new_date);
                                }
                            }
                            None => base_ctx.focused_date.set(None),
                        }
                    };
                    match e.key() {
                        Key::ArrowLeft => {
                            e.prevent_default();
                            set_focused_date(B::previous_day(focused_date));
                        }
                        Key::ArrowRight => {
                            e.prevent_default();
                            set_focused_date(B::next_day(focused_date));
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            if e.modifiers().shift() {
                                if let Some(date) = previous_month::<B>(focused_date) {
                                    set_focused_date(Some(date));
                                }
                            } else {
                                set_focused_date(Some(B::saturating_sub_days(focused_date, 7)));
                            }
                        }
                        Key::ArrowDown => {
                            e.prevent_default();
                            if e.modifiers().shift() {
                                if let Some(date) = next_month::<B>(focused_date) {
                                    set_focused_date(Some(date));
                                }
                            } else {
                                set_focused_date(Some(B::saturating_add_days(focused_date, 7)));
                            }
                        }
                        _ => {}
                    }
                },
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The context provided by the [`RangeCalendar`] component to its children.
    #[derive(Copy, Clone)]
    pub struct RangeCalendarContext<B: DateBackend> {
        // The date that the user clicked on to begin range selection
        anchor_date: Signal<Option<B::Date>>,
        // Currently highlighted date range
        highlighted_range: Signal<Option<DateRange<B>>>,
        set_selected_range: Callback<Option<DateRange<B>>>,
    }

    impl<B: DateBackend> RangeCalendarContext<B> {
        /// Set the selected date
        pub fn set_selected_date(&mut self, date: Option<B::Date>) {
            match (self.anchor_date)() {
                Some(anchor) => {
                    if let Some(date) = date {
                        self.anchor_date.set(None);

                        let range = DateRange::<B>::new(date, anchor);
                        self.set_selected_range.call(Some(range));
                        self.highlighted_range.set(Some(range));
                    }
                }
                None => {
                    self.anchor_date.set(date);

                    let range = date.map(|d| DateRange::<B>::new(d, d));
                    self.highlighted_range.set(range);
                }
            }
        }

        /// Set the selected date range by hovered date
        pub fn set_hovered_date(&mut self, date: B::Date) {
            if let Some(anchor) = (self.anchor_date)() {
                let range = DateRange::<B>::new(anchor, date);
                self.highlighted_range.set(Some(range));
            }
        }

        /// Set previous selected range
        pub fn reset_selection(&mut self, range: Option<DateRange<B>>) {
            self.anchor_date.set(None);
            self.highlighted_range.set(range);
        }
    }

    /// The props for the [`RangeCalendar`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct RangeCalendarProps<B: DateBackend> {
        /// The selected range
        #[props(default)]
        pub selected_range: ReadSignal<Option<DateRange<B>>>,

        /// Callback when selected date range changes
        #[props(default)]
        pub on_range_change: Callback<Option<DateRange<B>>>,

        /// Callback when display weekday
        #[props(default = Callback::new(|weekday: B::Weekday| weekday_abbreviation::<B>(weekday).to_string()))]
        pub on_format_weekday: Callback<B::Weekday, String>,

        /// Callback when display month
        #[props(default = Callback::new(|month: B::Month| month.to_string()))]
        pub on_format_month: Callback<B::Month, String>,

        /// The month being viewed
        #[props(default = ReadSignal::new(Signal::new(B::today())))]
        pub view_date: ReadSignal<B::Date>,

        /// The current date (used for highlighting today)
        #[props(default = B::today())]
        pub today: B::Date,

        /// Callback when view date changes
        #[props(default)]
        pub on_view_change: Callback<B::Date>,

        /// Whether the calendar is disabled
        #[props(default)]
        pub disabled: ReadSignal<bool>,

        /// First day of the week
        #[props(default = B::SUNDAY)]
        pub first_day_of_week: B::Weekday,

        /// Lower limit of the range of available dates
        #[props(default = B::from_ymd(1925, B::JANUARY, 1).unwrap())]
        pub min_date: B::Date,

        /// Upper limit of the range of available dates
        #[props(default = B::from_ymd(2050, B::DECEMBER, 31).unwrap())]
        pub max_date: B::Date,

        /// Unavailable dates
        #[props(default)]
        pub disabled_ranges: ReadSignal<Vec<DateRange<B>>>,

        /// Additional attributes to extend the calendar element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the calendar element
        pub children: Element,
    }

    impl<B: DateBackend> DefaultCalendarProps for RangeCalendarProps<B> {
        fn default_calendar(self) -> Element {
            RangeCalendar(self)
        }
    }

    /// # RangeCalendar
    ///
    /// The [`RangeCalendar`] component provides an accessible calendar interface with arrow key navigation, month switching, and date selection.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::*;
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_range = use_signal(|| None::<DateRange>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         RangeCalendar {
    ///             selected_range: selected_range(),
    ///             on_range_change: move |range| {
    ///                 tracing::info!("Selected range: {:?}", range);
    ///                 selected_range.set(range);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Styling
    ///
    /// The [`RangeCalendar`] component defines the following data attributes you can use to control styling:
    /// - `data-disabled`: Indicates if the calendar is disabled. Possible values are `true` or `false`.
    #[component]
    pub fn RangeCalendar<B: DateBackend>(props: RangeCalendarProps<B>) -> Element {
        let focused_date = use_signal(|| {
            let range = (props.selected_range)();
            range.map(|r| r.end)
        });
        let anchor_date = use_signal(|| None::<B::Date>);
        let highlighted_range = use_signal(|| (props.selected_range)());
        let available_ranges =
            use_memo(move || AvailableRanges::<B>::new(&props.disabled_ranges.read()));
        let view_registrations = use_signal(Vec::new);

        // Create base context provider for child components
        let mut base_ctx = use_context_provider(|| BaseCalendarContext::<B> {
            focused_date,
            view_date: props.view_date,
            set_view_date: props.on_view_change,
            available_ranges,
            format_weekday: props.on_format_weekday,
            format_month: props.on_format_month,
            disabled: props.disabled,
            today: props.today,
            first_day_of_week: props.first_day_of_week,
            enabled_date_range: DateRange::<B>::new(props.min_date, props.max_date),
            view_registrations,
        });

        // Create RangeCalendar context provider for child components
        let mut ctx = use_context_provider(|| RangeCalendarContext::<B> {
            anchor_date,
            highlighted_range,
            set_selected_range: props.on_range_change,
        });

        rsx! {
            div {
                role: "application",
                aria_label: "Calendar",
                "data-disabled": (props.disabled)(),
                onkeydown: move |e| {
                    let Some(mut focused_date) = (base_ctx.focused_date)() else {
                        return;
                    };
                    if let (Some(range), Some(date)) = (
                        (ctx.highlighted_range)(),
                        (ctx.anchor_date)(),
                    ) {
                        if date != range.start {
                            focused_date = range.start
                        } else {
                            focused_date = range.end
                        }
                    }
                    let mut set_focused_date = |new_date: Option<B::Date>| {
                        if let Some(date) = new_date {
                            let min_date = B::replace_day((base_ctx.view_date)(), 1).unwrap();
                            if date < min_date {
                                let view_date = previous_month::<B>(min_date).unwrap_or(min_date);
                                (base_ctx.set_view_date)(view_date);
                            } else {
                                let max_date = nth_month_next::<B>(min_date, base_ctx.visible_month_count())
                                    .unwrap_or(min_date);
                                if date >= max_date {
                                    let view_date = next_month::<B>(min_date).unwrap_or(min_date);
                                    (base_ctx.set_view_date)(view_date);
                                }
                            }
                        }
                        match new_date {
                            Some(date) => {
                                if base_ctx.enabled_date_range.contains(date) {
                                    base_ctx.focused_date.set(new_date);
                                    let date = match base_ctx.available_range() {
                                        Some(range) => range.clamp(date),
                                        None => date,
                                    };
                                    ctx.set_hovered_date(date);
                                }
                            }
                            None => base_ctx.focused_date.set(None),
                        }
                    };
                    match e.key() {
                        Key::ArrowLeft => {
                            e.prevent_default();
                            set_focused_date(B::previous_day(focused_date));
                        }
                        Key::ArrowRight => {
                            e.prevent_default();
                            set_focused_date(B::next_day(focused_date));
                        }
                        Key::ArrowUp => {
                            e.prevent_default();
                            if e.modifiers().shift() {
                                if let Some(date) = previous_month::<B>(focused_date) {
                                    set_focused_date(Some(date));
                                }
                            } else {
                                set_focused_date(Some(B::saturating_sub_days(focused_date, 7)));
                            }
                        }
                        Key::ArrowDown => {
                            e.prevent_default();
                            if e.modifiers().shift() {
                                if let Some(date) = next_month::<B>(focused_date) {
                                    set_focused_date(Some(date));
                                }
                            } else {
                                set_focused_date(Some(B::saturating_add_days(focused_date, 7)));
                            }
                        }
                        Key::Escape => {
                            ctx.reset_selection((props.selected_range)());
                        }
                        _ => {}
                    }
                },
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The props for the [`CalendarView`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarViewProps<B: DateBackend> {
        /// An offset from the beginning of the view date that this should display
        #[props(default)]
        pub offset: Option<u8>,

        /// Additional attributes to apply to the view element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the calendar element
        pub children: Element,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    #[derive(Copy, Clone, PartialEq)]
    struct CalendarViewContext {
        offset: u8,
    }

    impl CalendarViewContext {
        fn offset_view_date<B: DateBackend>(&self) -> B::Date {
            let base_ctx: BaseCalendarContext<B> = consume_context();
            let view_date = (base_ctx.view_date)();

            nth_month_next::<B>(view_date, self.offset).unwrap_or(view_date)
        }

        fn set_offset_view_date<B: DateBackend>(&self, date: B::Date) {
            let base_ctx: BaseCalendarContext<B> = consume_context();
            let view_date = base_ctx.view_date();
            // The date is currently relative to the offset, so we need to adjust it back
            let date = nth_month_previous::<B>(date, self.offset).unwrap_or(view_date);
            base_ctx.set_view_date(date);
        }
    }

    /// A calendar view for one visible month.
    ///
    /// Render one [`CalendarView`] for each month you want visible. The calendar derives the
    /// visible month count from the registered views and uses each view's render order as
    /// its month offset unless `offset` is provided.
    #[component]
    pub fn CalendarView<B: DateBackend>(props: CalendarViewProps<B>) -> Element {
        let base_ctx: BaseCalendarContext<B> = use_context();
        let view_id = current_scope_id();

        use_hook(move || {
            base_ctx.register_calendar_view(view_id, props.offset);
        });

        use_effect(move || {
            base_ctx.register_calendar_view(view_id, props.offset);
        });

        use_effect_cleanup(move || {
            base_ctx.unregister_calendar_view(view_id);
        });

        let offset = base_ctx.calendar_view_offset(view_id, props.offset);

        use_context_provider(|| CalendarViewContext { offset });

        rsx! {
            div { ..props.attributes,
                {props.children}
            }
        }
    }

    /// The props for the [`CalendarHeader`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarHeaderProps {
        /// Optional ID for the header
        #[props(default)]
        pub id: Option<String>,

        /// Additional attributes to extend the header element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the header element
        pub children: Element,
    }

    /// # CalendarHeader
    ///
    /// The [`CalendarHeader`] component displays the header for the calendar. It typically contains the [`CalendarNavigation`] component
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarHeader(props: CalendarHeaderProps) -> Element {
        rsx! {
            div {
                role: "heading",
                "aria-level": "2",
                id: props.id,
                ..props.attributes,

                {props.children}
            }
        }
    }

    /// The props for the [`CalendarNavigation`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarNavigationProps {
        /// Optional ID for the navigation
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the navigation element
        #[props(default)]
        pub children: Element,
    }

    /// # CalendarNavigation
    ///
    /// The [`CalendarNavigation`] component provides a container for navigation buttons in the calendar header.
    /// It typically contains the [`CalendarPreviousMonthButton`], [`CalendarNextMonthButton`], and [`CalendarMonthTitle`] components.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarNavigation(props: CalendarNavigationProps) -> Element {
        rsx! {
            div { ..props.attributes,
                {props.children}
            }
        }
    }

    /// The props for the [`CalendarPreviousMonthButton`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarPreviousMonthButtonProps<B: DateBackend> {
        /// Additional attributes to apply to the button
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the button element
        pub children: Element,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # CalendarPreviousMonthButton
    ///
    /// The [`CalendarPreviousMonthButton`] component provides a button to navigate to the previous month in the calendar.
    ///
    /// This must be used inside a [`Calendar`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarPreviousMonthButton<B: DateBackend>(
        props: CalendarPreviousMonthButtonProps<B>,
    ) -> Element {
        let ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();

        // disable previous button when we reach the limit
        let button_disabled = use_memo(move || {
            // Get the current view date from context
            let view_date = view_ctx.offset_view_date::<B>();
            match previous_month::<B>(view_date) {
                Some(date) => B::replace_day(ctx.enabled_date_range.start, 1).unwrap() > date,
                None => true,
            }
        });
        // disable previous button when the current selection range does not include the previous month
        let navigate_disabled = use_memo(move || {
            // Get the current view date from context
            let view_date = view_ctx.offset_view_date::<B>();
            ctx.available_range()
                .is_some_and(|range| B::month(range.start) == B::month(view_date))
        });

        // Handle navigation to previous month
        let handle_prev_month = move |e: Event<MouseData>| {
            e.prevent_default();
            let current_view = (ctx.view_date)();
            if let Some(date) = previous_month::<B>(current_view) {
                ctx.set_view_date.call(date)
            }
        };

        rsx! {
            button {
                aria_label: "Previous month",
                type: "button",
                onclick: handle_prev_month,
                disabled: (ctx.disabled)() || button_disabled() || navigate_disabled(),
                ..props.attributes,

                {props.children}
            }
        }
    }

    /// The props for the [`CalendarNextMonthButton`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarNextMonthButtonProps<B: DateBackend> {
        /// Additional attributes to apply to the button
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the button element
        pub children: Element,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # CalendarNextMonthButton
    ///
    /// The [`CalendarNextMonthButton`] component provides a button to navigate to the next month in the calendar.
    ///
    /// This must be used inside a [`Calendar`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarNextMonthButton<B: DateBackend>(
        props: CalendarNextMonthButtonProps<B>,
    ) -> Element {
        let ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();

        // disable next button when we reach the limit
        let button_disabled = use_memo(move || {
            // Get the current view date from context
            let view_date = view_ctx.offset_view_date::<B>();
            match next_month::<B>(view_date) {
                Some(date) => {
                    let max = ctx.enabled_date_range.end();
                    let last_day = B::month_length(B::month(max), B::year(max));
                    B::replace_day(max, last_day).unwrap() < date
                }
                None => true,
            }
        });
        // disable next button when the current selection range does not include the next month
        let navigate_disabled = use_memo(move || {
            // Get the current view date from context
            let view_date = view_ctx.offset_view_date::<B>();
            ctx.available_range()
                .is_some_and(|range| B::month(range.end) == B::month(view_date))
        });

        // Handle navigation to next month
        let handle_next_month = move |e: Event<MouseData>| {
            e.prevent_default();
            let current_view = (ctx.view_date)();
            if let Some(date) = next_month::<B>(current_view) {
                ctx.set_view_date.call(date)
            }
        };

        rsx! {
            button {
                aria_label: "Next month",
                type: "button",
                onclick: handle_next_month,
                disabled: (ctx.disabled)() || button_disabled() || navigate_disabled(),
                ..props.attributes,

                {props.children}
            }
        }
    }

    /// The props for the [`CalendarMonthTitle`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarMonthTitleProps<B: DateBackend> {
        /// Additional attributes to apply to the title element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # CalendarMonthTitle
    ///
    /// The [`CalendarMonthTitle`] component displays the title of the current month in the calendar. It will contain
    /// the month and year information as text in the children.
    ///
    /// This must be used inside a [`Calendar`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarMonthTitle<B: DateBackend>(props: CalendarMonthTitleProps<B>) -> Element {
        let view_ctx: CalendarViewContext = use_context();
        // Format the current month and year
        let month_year = use_memo(move || {
            let view_date = view_ctx.offset_view_date::<B>();
            format!("{} {}", B::month(view_date), B::year(view_date))
        });

        rsx! {
            div {
                ..props.attributes,

                {month_year}
            }
        }
    }

    /// The props for the [`CalendarGrid`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridProps<B: DateBackend> {
        /// Optional ID for the grid
        #[props(default)]
        pub id: Option<String>,

        /// Additional attributes to apply to the grid element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`CalendarGridRoot`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridRootProps {
        /// Optional ID for the grid
        #[props(default)]
        pub id: Option<String>,

        /// Additional attributes to apply to the grid element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the grid element
        pub children: Element,
    }

    /// The props for the [`CalendarGridHead`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridHeadProps {
        /// Additional attributes to apply to the grid head element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the grid head element
        pub children: Element,
    }

    /// The props for the [`CalendarGridHeaderRow`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridHeaderRowProps {
        /// Additional attributes to apply to the grid header row element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the grid header row element
        pub children: Element,
    }

    /// The props for the [`CalendarGridDayHeader`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridDayHeaderProps<B: DateBackend> {
        /// The weekday represented by this header.
        pub weekday: B::Weekday,

        /// Additional attributes to apply to the weekday header element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the weekday header element
        #[props(default)]
        pub children: Option<Element>,
    }

    /// The props for the [`CalendarGridBody`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridBodyProps {
        /// Additional attributes to apply to the grid body element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the grid body element
        pub children: Element,
    }

    /// The props for the [`CalendarGridWeek`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridWeekProps {
        /// Additional attributes to apply to the week row element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the week row element
        pub children: Element,
    }

    /// The props for the [`CalendarGridCell`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarGridCellProps {
        /// Additional attributes to apply to the day cell element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the day cell element
        pub children: Element,
    }

    /// Data for a weekday header in a calendar grid.
    #[derive(Clone, PartialEq)]
    pub struct CalendarGridWeekday<B: DateBackend> {
        weekday: B::Weekday,
        label: String,
    }

    impl<B: DateBackend> CalendarGridWeekday<B> {
        /// The weekday represented by this header.
        pub fn weekday(&self) -> B::Weekday {
            self.weekday
        }

        /// The formatted weekday label.
        pub fn label(&self) -> &str {
            &self.label
        }
    }

    /// Data returned by [`use_calendar_grid`].
    #[derive(Clone, PartialEq)]
    pub struct CalendarGridData<B: DateBackend> {
        view_date: B::Date,
        weekdays: Vec<CalendarGridWeekday<B>>,
        weeks: Vec<Vec<B::Date>>,
    }

    impl<B: DateBackend> CalendarGridData<B> {
        /// The first date in the month currently displayed by the grid.
        pub fn view_date(&self) -> B::Date {
            self.view_date
        }

        /// Weekday headers in display order.
        pub fn weekdays(&self) -> &[CalendarGridWeekday<B>] {
            &self.weekdays
        }

        /// Weeks in the displayed month, each containing seven dates.
        pub fn weeks(&self) -> &[Vec<B::Date>] {
            &self.weeks
        }
    }

    /// Return the weekday headers and week rows for the current calendar grid view.
    pub fn use_calendar_grid<B: DateBackend>() -> CalendarGridData<B> {
        let ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();

        use_memo(move || {
            let view_date = view_ctx.offset_view_date::<B>();
            CalendarGridData {
                view_date,
                weekdays: calendar_grid_weekdays::<B>(ctx.first_day_of_week, ctx.format_weekday),
                weeks: calendar_grid_weeks::<B>(view_date, ctx.first_day_of_week),
            }
        })()
    }

    fn calendar_grid_weekdays<B: DateBackend>(
        first_day_of_week: B::Weekday,
        format_weekday: Callback<B::Weekday, String>,
    ) -> Vec<CalendarGridWeekday<B>> {
        WeekdaySet(0b111_1111)
            .iter::<B>(first_day_of_week)
            .map(|weekday| CalendarGridWeekday {
                weekday,
                label: format_weekday.call(weekday),
            })
            .collect()
    }

    fn calendar_grid_weeks<B: DateBackend>(
        view_date: B::Date,
        first_day_of_week: B::Weekday,
    ) -> Vec<Vec<B::Date>> {
        let mut grid = Vec::new();

        let previous_month = B::replace_day(view_date, 1).expect("invalid or out-of-range date");
        let num_days = days_since::<B>(view_date, first_day_of_week);
        let mut date = B::saturating_sub_days(previous_month, num_days);
        for _ in 1..=num_days {
            grid.push(date);
            date = B::next_day(date).expect("invalid or out-of-range date");
        }

        let mut date = view_date;
        let num_days_in_month = B::month_length(B::month(view_date), B::year(view_date));
        for day in 1..=num_days_in_month {
            date = B::replace_day(view_date, day).expect("invalid or out-of-range date");
            grid.push(date);
        }

        let remainder = grid.len() % 7;
        if remainder > 0 {
            for _ in 1..=(7 - remainder) {
                date = B::next_day(date).expect("invalid or out-of-range date");
                grid.push(date);
            }
        }

        grid.chunks(7).map(|chunk| chunk.to_vec()).collect()
    }

    /// # CalendarGrid
    ///
    /// The [`CalendarGrid`] component displays the grid of days for the current month in the calendar.
    ///
    /// This must be used inside a [`Calendar`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarMonthTitle, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// ## Styling
    ///
    /// The [`CalendarGrid`] component renders days in a grid that can be styled using CSS. They define the following data attributes:
    /// - `data-today`: If the date is today. Possible values are `true` or `false`
    /// - `data-selected`: If the date is selected. Possible values are `true` or `false`
    /// - `data-month`: The relative month of the date. Possible values are `last`, `current`, or `next`
    #[component]
    pub fn CalendarGrid<B: DateBackend>(props: CalendarGridProps<B>) -> Element {
        let grid = use_calendar_grid::<B>();

        rsx! {
            CalendarGridRoot {
                id: props.id,
                attributes: props.attributes,
                CalendarGridHead {
                    CalendarGridHeaderRow {
                        for weekday in grid.weekdays().iter().cloned() {
                            CalendarGridDayHeader::<B> {
                                key: "{weekday.label()}",
                                weekday: weekday.weekday(),
                                {weekday.label().to_string()}
                            }
                        }
                    }
                }
                CalendarGridBody {
                    for week in grid.weeks() {
                        CalendarGridWeek {
                            for date in week.iter().copied() {
                                CalendarGridCell {
                                    key: "{date}",
                                    CalendarDay::<B> { date }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The root table element for a calendar grid.
    #[component]
    pub fn CalendarGridRoot(props: CalendarGridRootProps) -> Element {
        rsx! {
            table {
                role: "grid",
                id: props.id,
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The header section of a calendar grid.
    #[component]
    pub fn CalendarGridHead(props: CalendarGridHeadProps) -> Element {
        rsx! {
            thead {
                aria_hidden: "true",
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The row that contains weekday header cells.
    #[component]
    pub fn CalendarGridHeaderRow(props: CalendarGridHeaderRowProps) -> Element {
        rsx! {
            tr {
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// A weekday header cell in a calendar grid.
    #[component]
    pub fn CalendarGridDayHeader<B: DateBackend>(props: CalendarGridDayHeaderProps<B>) -> Element {
        let ctx: BaseCalendarContext<B> = use_context();
        let children = props.children.unwrap_or_else(|| {
            let label = ctx.format_weekday.call(props.weekday);
            rsx! { {label} }
        });

        rsx! {
            th {
                ..props.attributes,
                {children}
            }
        }
    }

    /// The body section of a calendar grid.
    #[component]
    pub fn CalendarGridBody(props: CalendarGridBodyProps) -> Element {
        rsx! {
            tbody {
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// A week row in a calendar grid.
    #[component]
    pub fn CalendarGridWeek(props: CalendarGridWeekProps) -> Element {
        rsx! {
            tr {
                role: "row",
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// A day cell in a calendar grid.
    #[component]
    pub fn CalendarGridCell(props: CalendarGridCellProps) -> Element {
        rsx! {
            td {
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The props for the [`CalendarSelectMonth`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectMonthProps {
        /// Additional attributes to apply to the month select container element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the month select container element.
        #[props(default)]
        pub children: Element,
    }

    /// The props for the [`CalendarSelectMonthSelect`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectMonthSelectProps<B: DateBackend> {
        /// Additional attributes to apply to the native month select element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`CalendarSelectMonthOption`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectMonthOptionProps<B: DateBackend> {
        /// The month represented by this option.
        pub month: B::Month,

        /// Additional attributes to apply to the month option element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the month option element.
        #[props(default)]
        pub children: Option<Element>,
    }

    /// The props for the [`CalendarSelectMonthValue`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectMonthValueProps<B: DateBackend> {
        /// Additional attributes to apply to the displayed month value element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the displayed month value element.
        #[props(default)]
        pub children: Element,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # CalendarSelectMonth
    ///
    /// The [`CalendarSelectMonth`] component provides a container for the month select controls.
    ///
    /// This must be used inside a [`Calendar`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton,
    ///     CalendarSelectMonth, CalendarSelectMonthSelect, CalendarSelectMonthValue
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarSelectMonth {
    ///                         CalendarSelectMonthSelect {}
    ///                         CalendarSelectMonthValue {}
    ///                     }
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarSelectMonth(props: CalendarSelectMonthProps) -> Element {
        rsx! {
            span {
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The native select element for choosing the visible month.
    #[component]
    pub fn CalendarSelectMonthSelect<B: DateBackend>(
        props: CalendarSelectMonthSelectProps<B>,
    ) -> Element {
        let base_ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();

        let months = use_memo(move || {
            // Get the current view date from context
            let view_date = view_ctx.offset_view_date::<B>();
            let min_date = base_ctx.enabled_date_range.start();
            let max_date = base_ctx.enabled_date_range.end();
            let mut min_month = B::JANUARY;
            if replace_month::<B>(view_date, min_month) < min_date {
                min_month = B::month(min_date);
            }
            let mut max_month = B::DECEMBER;
            if replace_month::<B>(view_date, max_month) > max_date {
                max_month = B::month(max_date);
            }

            let mut month = min_month;
            let mut months = Vec::new();
            loop {
                months.push(month);

                if month == max_month {
                    return months;
                }
                month = B::next_month(month);
            }
        });

        rsx! {
            select {
                aria_label: "Month",
                onchange: move |e| {
                    let mut view_date = view_ctx.offset_view_date::<B>();
                    let number = e.value().parse().unwrap_or(B::month_to_number(B::month(view_date)));
                    let cur_month = B::month_from_number(number).expect("month out of range");
                    view_date = B::replace_month(view_date, cur_month);
                    view_ctx.set_offset_view_date::<B>(view_date);
                },
                ..props.attributes,
                for month in months() {
                    CalendarSelectMonthOption::<B> { key: "{month:?}", month }
                }
            }
        }
    }

    /// An option in the native month select element.
    #[component]
    pub fn CalendarSelectMonthOption<B: DateBackend>(
        props: CalendarSelectMonthOptionProps<B>,
    ) -> Element {
        let base_ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();
        let children = props
            .children
            .unwrap_or_else(|| rsx! { {base_ctx.format_month.call(props.month)} });

        rsx! {
            option {
                value: B::month_to_number(props.month),
                selected: B::month(view_ctx.offset_view_date::<B>()) == props.month,
                ..props.attributes,
                {children}
            }
        }
    }

    /// The displayed month value.
    #[component]
    pub fn CalendarSelectMonthValue<B: DateBackend>(
        props: CalendarSelectMonthValueProps<B>,
    ) -> Element {
        let base_ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();
        let month = B::month(view_ctx.offset_view_date::<B>());

        rsx! {
            span {
                ..props.attributes,
                {base_ctx.format_month.call(month)}
                {props.children}
            }
        }
    }

    /// The props for the [`CalendarSelectYear`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectYearProps {
        /// Additional attributes to apply to the year select container element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the year select container element.
        #[props(default)]
        pub children: Element,
    }

    /// The props for the [`CalendarSelectYearSelect`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectYearSelectProps<B: DateBackend> {
        /// Additional attributes to apply to the native year select element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`CalendarSelectYearOption`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectYearOptionProps<B: DateBackend> {
        /// The year represented by this option.
        pub year: i32,

        /// Additional attributes to apply to the year option element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the year option element.
        #[props(default)]
        pub children: Option<Element>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`CalendarSelectYearValue`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarSelectYearValueProps<B: DateBackend> {
        /// Additional attributes to apply to the displayed year value element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the displayed year value element.
        #[props(default)]
        pub children: Element,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # CalendarSelectYear
    ///
    /// The [`CalendarSelectYear`] component provides a container for the year select controls.
    ///
    /// This must be used inside a [`Calendar`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::{
    ///     Calendar, CalendarGrid, CalendarHeader, CalendarNavigation, CalendarNextMonthButton, CalendarPreviousMonthButton,
    ///     CalendarSelectYear, CalendarSelectYearSelect, CalendarSelectYearValue
    /// };
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_date = use_signal(|| None::<Date>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         Calendar {
    ///             selected_date: selected_date(),
    ///             on_date_change: move |date| {
    ///                 tracing::info!("Selected date: {:?}", date);
    ///                 selected_date.set(date);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarSelectYear {
    ///                         CalendarSelectYearSelect {}
    ///                         CalendarSelectYearValue {}
    ///                     }
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    #[component]
    pub fn CalendarSelectYear(props: CalendarSelectYearProps) -> Element {
        rsx! {
            span {
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The native select element for choosing the visible year.
    #[component]
    pub fn CalendarSelectYearSelect<B: DateBackend>(
        props: CalendarSelectYearSelectProps<B>,
    ) -> Element {
        let base_ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();

        let years = use_memo(move || {
            // Get the current view date from context
            let view_date = view_ctx.offset_view_date::<B>();
            let min_date = base_ctx.enabled_date_range.start();
            let max_date = base_ctx.enabled_date_range.end();
            let month = B::month(view_date);
            let mut min_year = B::year(min_date);
            if replace_month::<B>(min_date, month) < min_date {
                min_year += 1;
            }
            let mut max_year = B::year(max_date);
            if replace_month::<B>(max_date, month) > max_date {
                max_year -= 1;
            }

            min_year..=max_year
        });

        rsx! {
            select {
                aria_label: "Year",
                onchange: move |e| {
                    let mut view_date = view_ctx.offset_view_date::<B>();
                    let year = e.value().parse().unwrap_or(B::year(view_date));
                    view_date = B::replace_year(view_date, year).unwrap_or(view_date);
                    view_ctx.set_offset_view_date::<B>(view_date);
                },
                ..props.attributes,
                for year in years() {
                    CalendarSelectYearOption::<B> { key: "{year}", year }
                }
            }
        }
    }

    /// An option in the native year select element.
    #[component]
    pub fn CalendarSelectYearOption<B: DateBackend>(
        props: CalendarSelectYearOptionProps<B>,
    ) -> Element {
        let view_ctx: CalendarViewContext = use_context();
        let children = props.children.unwrap_or_else(|| {
            let year = props.year;
            rsx! { "{year}" }
        });

        rsx! {
            option {
                value: props.year,
                selected: B::year(view_ctx.offset_view_date::<B>()) == props.year,
                ..props.attributes,
                {children}
            }
        }
    }

    /// The displayed year value.
    #[component]
    pub fn CalendarSelectYearValue<B: DateBackend>(
        props: CalendarSelectYearValueProps<B>,
    ) -> Element {
        let view_ctx: CalendarViewContext = use_context();
        let year = B::year(view_ctx.offset_view_date::<B>());

        rsx! {
            span {
                ..props.attributes,
                "{year}"
                {props.children}
            }
        }
    }

    #[derive(Copy, Clone, Debug, PartialEq)]
    enum RelativeMonth {
        Last,
        Current,
        Next,
    }

    impl RelativeMonth {
        fn current_month(&self) -> bool {
            *self == RelativeMonth::Current
        }
    }

    impl Display for RelativeMonth {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                RelativeMonth::Last => write!(f, "last"),
                RelativeMonth::Current => write!(f, "current"),
                RelativeMonth::Next => write!(f, "next"),
            }
        }
    }

    /// Get a human-readable ARIA label for input date
    fn weekday_full_name<B: DateBackend>(weekday: B::Weekday) -> &'static str {
        match B::weekday_to_number(weekday) {
            0 => "Monday",
            1 => "Tuesday",
            2 => "Wednesday",
            3 => "Thursday",
            4 => "Friday",
            5 => "Saturday",
            6 => "Sunday",
            _ => unreachable!("weekday_to_number is contracted to return 0..=6"),
        }
    }

    fn aria_label<B: DateBackend>(date: &B::Date) -> String {
        format!(
            "{}, {} {}, {}",
            weekday_full_name::<B>(B::weekday(*date)),
            B::month(*date),
            B::day(*date),
            B::year(*date)
        )
    }

    /// The props for the [`CalendarDay`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct CalendarDayProps<B: DateBackend> {
        /// The date for this day cell.
        pub date: B::Date,
        /// Additional attributes to extend the calendar day element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,
        /// The children of the calendar day element
        #[props(default)]
        pub children: Option<Element>,
    }

    /// # CalendarDay
    ///
    /// The [`CalendarDay`] component provides an accessible calendar interface for a date
    ///
    /// This must be used inside a [`CalendarGrid`] component.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::calendar::*;
    /// use ::time::{Date, Month, UtcDateTime};
    /// #[component]
    /// fn Demo() -> Element {
    ///     let mut selected_range = use_signal(|| None::<DateRange>);
    ///     let mut view_date = use_signal(|| UtcDateTime::now().date());
    ///     rsx! {
    ///         RangeCalendar {
    ///             selected_range: selected_range(),
    ///             on_range_change: move |range| {
    ///                 tracing::info!("Selected range: {:?}", range);
    ///                 selected_range.set(range);
    ///             },
    ///             view_date: view_date(),
    ///             on_view_change: move |new_view: Date| {
    ///                 tracing::info!("View changed to: {}-{}", new_view.year(), new_view.month());
    ///                 view_date.set(new_view);
    ///             },
    ///             CalendarHeader {
    ///                 CalendarNavigation {
    ///                     CalendarPreviousMonthButton {
    ///                         "<"
    ///                     }
    ///                     CalendarMonthTitle {}
    ///                     CalendarNextMonthButton {
    ///                         ">"
    ///                     }
    ///                 }
    ///             }
    ///             CalendarGrid {}
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Styling
    ///
    /// The [`CalendarDay`] component defines the following data attributes you can use to control styling:
    /// - `data-disabled`: Indicates if the calendar is disabled. Possible values are `true` or `false`.
    /// - `data-unavailable`: Indicates if the date is unavailable. Possible values are `true` or `false`.
    /// - `data-today`: Indicates if the cell is today. Possible values are `true` or `false`.
    /// - `data-month`: The relative month of the date. Possible values are `last`,
    /// - `data-selected`: Indicates if the cell is selected. Possible values are `true` or `false`.
    /// - `data-selection-start`: Indicates if cell is the first date in a range selection. Possible values are `true` or `false`.
    /// - `data-selection-between`: Indicates if a date interval contains a cell. Possible values are `true` or `false`.
    /// - `data-selection-end`: Indicates if cell is the last date in a range selection. Possible values are `true` or `false`.
    #[component]
    pub fn CalendarDay<B: DateBackend>(props: CalendarDayProps<B>) -> Element {
        let single_context = try_use_context::<CalendarContext<B>>().is_some();
        let CalendarDayProps {
            date,
            attributes,
            children,
        } = props;

        if single_context {
            rsx! {
                SingleCalendarDay::<B> { date, attributes: attributes.clone(), children: children.clone() }
            }
        } else {
            rsx! {
                RangeCalendarDay::<B> { date, attributes, children }
            }
        }
    }

    fn relative_calendar_month<B: DateBackend>(
        date: B::Date,
        base_ctx: &BaseCalendarContext<B>,
        current_month: B::Month,
    ) -> RelativeMonth {
        if date < base_ctx.enabled_date_range.start {
            RelativeMonth::Last
        } else if date > base_ctx.enabled_date_range.end {
            RelativeMonth::Next
        } else {
            match B::month(date).cmp(&current_month) {
                std::cmp::Ordering::Less => RelativeMonth::Last,
                std::cmp::Ordering::Equal => RelativeMonth::Current,
                std::cmp::Ordering::Greater => RelativeMonth::Next,
            }
        }
    }

    fn is_between<B: DateBackend>(date: B::Date, range: Option<DateRange<B>>) -> bool {
        range.is_some_and(|r| r.contained_in_interval(date))
    }

    fn is_start<B: DateBackend>(date: B::Date, range: Option<DateRange<B>>) -> bool {
        range.is_some_and(|r| r.start == date && date != r.end)
    }

    fn is_end<B: DateBackend>(date: B::Date, range: Option<DateRange<B>>) -> bool {
        range.is_some_and(|r| r.end == date && date != r.start)
    }

    fn use_day_mounted_ref(
        mut is_focused: impl FnMut() -> bool + 'static,
    ) -> impl FnMut(MountedEvent) + 'static {
        let mut day_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
        use_effect(move || {
            if let Some(day) = day_ref() {
                if is_focused() {
                    spawn(async move {
                        _ = day.set_focus(true).await;
                    });
                }
            }
        });
        move |e| day_ref.set(Some(e.data()))
    }

    #[component]
    fn SingleCalendarDay<B: DateBackend>(props: CalendarDayProps<B>) -> Element {
        let CalendarDayProps {
            date,
            attributes,
            children,
        } = props;
        let mut base_ctx: BaseCalendarContext<B> = use_context();
        let view_ctx: CalendarViewContext = use_context();
        let day = B::day(date);
        let content = children.unwrap_or_else(|| rsx! { {day.to_string()} });
        let view_date = view_ctx.offset_view_date::<B>();
        let month = relative_calendar_month::<B>(date, &base_ctx, B::month(view_date));
        let in_current_month = month.current_month();
        let is_focused = move || {
            base_ctx
                .focused_date()
                .is_some_and(|d| d == date && B::month(d) == B::month(view_date))
        };
        let is_today = date == base_ctx.today;
        let is_unavailable = base_ctx.is_unavailable(date);

        let is_disabled = move || {
            if (base_ctx.disabled)() {
                return true;
            }

            is_unavailable
        };
        let onmounted = use_day_mounted_ref(is_focused);

        let ctx: CalendarContext<B> = use_context();
        let is_selected = move || (ctx.selected_date)().is_some_and(|d| d == date);

        // Handle day selection
        let mut handle_day_select = move |day: u8| {
            if (base_ctx.disabled)() || is_unavailable {
                return;
            }
            let view_date = view_ctx.offset_view_date::<B>();
            let date = B::replace_day(view_date, day).unwrap();
            ctx.set_selected_date.call((!is_selected()).then_some(date));
            base_ctx.focused_date.set(Some(date));
        };

        let focusable_date = (base_ctx.focused_date)()
            .filter(|d| B::month(*d) == B::month(view_date))
            .or_else(|| {
                ctx.selected_date
                    .cloned()
                    .filter(|d| B::month(*d) == B::month(view_date))
            })
            .unwrap_or(view_date);

        rsx! {
            button {
                type: "button",
                tabindex: if date == focusable_date {
                    "0"
                } else {
                    "-1"
                },
                aria_label: aria_label::<B>(&date),
                "data-today": if is_today { true },
                "data-selected": is_selected(),
                "data-unavailable": if is_unavailable { true },
                "data-disabled": is_disabled(),
                "data-month": "{month}",
                onclick: move |e| {
                    e.prevent_default();
                    if in_current_month {
                        handle_day_select(day);
                    }
                },
                onfocus: move |_| {
                    if in_current_month {
                        base_ctx.focused_date.set(Some(date));
                    }
                },
                onmounted,
                ..attributes,
                {content}
            }
        }
    }

    #[component]
    fn RangeCalendarDay<B: DateBackend>(props: CalendarDayProps<B>) -> Element {
        let CalendarDayProps {
            date,
            attributes,
            children,
        } = props;
        let mut base_ctx: BaseCalendarContext<B> = use_context();
        let day = B::day(date);
        let content = children.unwrap_or_else(|| rsx! { {day.to_string()} });
        let view_ctx: CalendarViewContext = use_context();
        let view_date = view_ctx.offset_view_date::<B>();
        let month = relative_calendar_month::<B>(date, &base_ctx, B::month(view_date));
        let in_current_month = month.current_month();
        let is_focused = move || {
            base_ctx
                .focused_date()
                .is_some_and(|d| d == date && B::month(d) == B::month(view_date))
        };
        let is_today = date == base_ctx.today;
        let is_unavailable = base_ctx.is_unavailable(date);

        let is_disabled = move || {
            if (base_ctx.disabled)() {
                return true;
            }

            is_unavailable
        };
        let onmounted = use_day_mounted_ref(is_focused);

        let mut ctx: RangeCalendarContext<B> = use_context();
        let is_selected = move || (ctx.highlighted_range)().is_some_and(|r| r.contains(date));
        let is_between = move || is_between(date, ctx.highlighted_range.cloned());
        let is_start = move || is_start(date, ctx.highlighted_range.cloned());
        let is_end = move || is_end(date, ctx.highlighted_range.cloned());

        let clamp_date_to_available_range = move |date| {
            let available_range = base_ctx.available_range();
            available_range.map_or(date, |range| range.clamp(date))
        };

        // Handle day selection
        let mut handle_day_select = move |day: u8| {
            if is_disabled() || is_unavailable {
                return;
            }

            let view_date = view_ctx.offset_view_date::<B>();
            let date = B::replace_day(view_date, day).map(clamp_date_to_available_range);
            ctx.set_selected_date(date);
            base_ctx.focused_date.set(date);
        };

        let focusable_date = (base_ctx.focused_date)()
            .filter(|d| B::month(*d) == B::month(view_date))
            .or_else(|| {
                ctx.anchor_date
                    .cloned()
                    .filter(|d| B::month(*d) == B::month(view_date))
            })
            .unwrap_or(view_date);

        rsx! {
            button {
                type: "button",
                tabindex: if date == focusable_date {
                    "0"
                } else {
                    "-1"
                },
                aria_label: aria_label::<B>(&date),
                "data-disabled": is_disabled(),
                "data-today": if is_today { true },
                "data-selected": is_selected(),
                "data-unavailable": if is_unavailable { true },
                "data-selection-start": if is_start() { true },
                "data-selection-between": if is_between() { true },
                "data-selection-end": if is_end() { true },
                "data-month": "{month}",
                onclick: move |e| {
                    e.prevent_default();
                    if in_current_month {
                        handle_day_select(day);
                    }
                },
                onfocus: move |_| {
                    if in_current_month {
                        base_ctx.focused_date.set(Some(date));
                    }
                },
                onmouseover: move |_| {
                    if in_current_month {
                        ctx.set_hovered_date(clamp_date_to_available_range(date));
                    }
                },
                onmounted,
                ..attributes,
                {content}
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        pub(super) fn weekday_set_body<B: DateBackend>() {
            let mut weekdays = WeekdaySet::single::<B>(B::MONDAY);
            assert!(weekdays.contains::<B>(B::MONDAY));
            assert!(!weekdays.contains::<B>(B::TUESDAY));

            assert!(weekdays.remove::<B>(B::MONDAY));
            assert!(!weekdays.contains::<B>(B::MONDAY));
            assert!(!weekdays.remove::<B>(B::MONDAY));

            let all_days = WeekdaySet(0b111_1111);
            let empty_set = WeekdaySet(0b000_0000);
            let single_set = WeekdaySet::single::<B>(B::FRIDAY);
            let part_size_set = WeekdaySet(0b010_1010); // Tu, Th, Sa

            let days: Vec<B::Weekday> = all_days.iter::<B>(B::SUNDAY).collect();
            assert_eq!(days.len(), 7);
            assert_eq!(days[0], B::SUNDAY);

            let mut iter = all_days.iter::<B>(B::WEDNESDAY);
            assert_eq!(iter.next(), Some(B::WEDNESDAY));
            assert_eq!(iter.next(), Some(B::THURSDAY));

            assert!(empty_set.is_empty());
            assert!(!part_size_set.is_empty());
            assert!(!single_set.is_empty());
            assert!(!all_days.is_empty());
        }

        pub(super) fn days_since_body<B: DateBackend>() {
            let date = B::from_ymd(2024, B::JANUARY, 1).unwrap(); // Monday
            assert_eq!(days_since::<B>(date, B::MONDAY), 0);
            assert_eq!(days_since::<B>(date, B::SUNDAY), 1);
            assert_eq!(days_since::<B>(date, B::TUESDAY), 6);
        }

        pub(super) fn month_navigation_body<B: DateBackend>() {
            let date = B::from_ymd(2024, B::JANUARY, 15).unwrap();

            let next = next_month::<B>(date).expect("next month");
            assert_eq!(B::month(next), B::FEBRUARY);
            assert_eq!(B::year(next), 2024);
            assert_eq!(B::day(next), 15);

            let prev = previous_month::<B>(date).expect("previous month");
            assert_eq!(B::month(prev), B::DECEMBER);
            assert_eq!(B::year(prev), 2023);
            assert_eq!(B::day(prev), 15);
        }

        pub(super) fn calendar_grid_weeks_body<B: DateBackend>() {
            fn generate_test_grid<B: DateBackend>(
                view_date: B::Date,
                first_day_of_week: B::Weekday,
            ) -> Vec<Vec<B::Date>> {
                let mut grid = Vec::new();
                let first_of_month = B::replace_day(view_date, 1).unwrap();
                let start_offset = days_since::<B>(view_date, first_day_of_week) as usize;

                if start_offset > 0 {
                    if let Some(mut date) = B::previous_day(first_of_month) {
                        for _ in 1..start_offset {
                            date = B::previous_day(date).unwrap_or(date);
                        }
                        for _ in 0..start_offset {
                            grid.push(date);
                            date = B::next_day(date).unwrap_or(date);
                        }
                    }
                }

                let days_in_month = B::month_length(B::month(view_date), B::year(view_date));
                for day in 1..=days_in_month {
                    if let Some(date) = B::replace_day(view_date, day) {
                        grid.push(date);
                    }
                }

                let remainder = grid.len() % 7;
                if remainder > 0 {
                    if let Some(last_day) = B::replace_day(view_date, days_in_month) {
                        if let Some(mut date) = B::next_day(last_day) {
                            for _ in 0..(7 - remainder) {
                                grid.push(date);
                                date = B::next_day(date).unwrap_or(date);
                            }
                        }
                    }
                }

                grid.chunks(7).map(|week| week.to_vec()).collect()
            }

            // February 2021: starts on Monday, 28 days; with first_day Monday, fits in 4 weeks.
            let feb_2021 = B::from_ymd(2021, B::FEBRUARY, 15).unwrap();
            let feb_grid = generate_test_grid::<B>(feb_2021, B::MONDAY);
            assert_eq!(
                feb_grid.len(),
                4,
                "February 2021 should have exactly 4 weeks"
            );
            let feb_days: Vec<_> = feb_grid
                .iter()
                .flatten()
                .filter(|d| B::month(**d) == B::FEBRUARY && B::year(**d) == 2021)
                .collect();
            assert_eq!(
                feb_days.len(),
                28,
                "Should have all 28 days of February 2021"
            );

            // May 2024: starts on Wednesday, 31 days; with first_day Sunday, fits in 5 weeks.
            let may_2024 = B::from_ymd(2024, B::MAY, 15).unwrap();
            let may_grid = generate_test_grid::<B>(may_2024, B::SUNDAY);
            assert_eq!(
                may_grid.len(),
                5,
                "May 2024 should have exactly 5 weeks when starting from Sunday"
            );
            let may_days: Vec<_> = may_grid
                .iter()
                .flatten()
                .filter(|d| B::month(**d) == B::MAY && B::year(**d) == 2024)
                .collect();
            assert_eq!(may_days.len(), 31, "Should have all 31 days of May 2024");

            // December 2018: starts on Saturday, 31 days, first_day Sunday — needs 6 weeks.
            let dec_2018 = B::from_ymd(2018, B::DECEMBER, 15).unwrap();
            let dec_grid = generate_test_grid::<B>(dec_2018, B::SUNDAY);
            assert_eq!(
                dec_grid.len(),
                6,
                "December 2018 should have exactly 6 weeks"
            );
            for week in &dec_grid {
                assert!(!week.is_empty(), "No week should be empty");
                assert_eq!(week.len(), 7, "Each week should have exactly 7 days");
            }
        }

        #[cfg(feature = "time")]
        mod time_tests {
            use super::*;
            use crate::date_backend::TimeBackend;

            #[component]
            fn ConsecutiveCalendarViews() -> Element {
                rsx! {
                    Calendar::<TimeBackend> {
                        view_date: TimeBackend::from_ymd(2026, TimeBackend::MAY, 15).unwrap(),
                        CalendarView::<TimeBackend> { CalendarMonthTitle::<TimeBackend> {} }
                        CalendarView::<TimeBackend> { CalendarMonthTitle::<TimeBackend> {} }
                        CalendarView::<TimeBackend> { CalendarMonthTitle::<TimeBackend> {} }
                    }
                }
            }

            #[component]
            fn CalendarDayWithCustomChild() -> Element {
                rsx! {
                    Calendar::<TimeBackend> {
                        view_date: TimeBackend::from_ymd(2026, TimeBackend::MAY, 15).unwrap(),
                        CalendarView::<TimeBackend> {
                            CalendarDay::<TimeBackend> {
                                date: TimeBackend::from_ymd(2026, TimeBackend::MAY, 15).unwrap(),
                                "Custom day"
                            }
                        }
                    }
                }
            }

            #[component]
            fn RangeCalendarDayWithCustomChild() -> Element {
                rsx! {
                    RangeCalendar::<TimeBackend> {
                        view_date: TimeBackend::from_ymd(2026, TimeBackend::MAY, 15).unwrap(),
                        CalendarView::<TimeBackend> {
                            CalendarDay::<TimeBackend> {
                                date: TimeBackend::from_ymd(2026, TimeBackend::MAY, 15).unwrap(),
                                "Custom range day"
                            }
                        }
                    }
                }
            }

            #[test]
            fn weekday_set() {
                super::weekday_set_body::<TimeBackend>()
            }
            #[test]
            fn days_since_test() {
                super::days_since_body::<TimeBackend>()
            }
            #[test]
            fn month_navigation() {
                super::month_navigation_body::<TimeBackend>()
            }
            #[test]
            fn calendar_grid_weeks_test() {
                super::calendar_grid_weeks_body::<TimeBackend>()
            }

            #[test]
            fn implicit_calendar_views_render_consecutive_months_on_first_render() {
                let mut dom = VirtualDom::new(ConsecutiveCalendarViews);
                dom.rebuild_in_place();
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("May 2026"));
                assert!(html.contains("June 2026"));
                assert!(html.contains("July 2026"));
            }

            #[test]
            fn calendar_day_forwards_custom_children() {
                let mut dom = VirtualDom::new(CalendarDayWithCustomChild);
                dom.rebuild_in_place();
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Custom day"));
                assert!(!html.contains(">15</button>"));
            }

            #[test]
            fn range_calendar_day_forwards_custom_children() {
                let mut dom = VirtualDom::new(RangeCalendarDayWithCustomChild);
                dom.rebuild_in_place();
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Custom range day"));
                assert!(!html.contains(">15</button>"));
            }
        }

        #[cfg(feature = "jiff")]
        mod jiff_tests {
            use crate::date_backend::JiffBackend;

            #[test]
            fn weekday_set() {
                super::weekday_set_body::<JiffBackend>()
            }
            #[test]
            fn days_since_test() {
                super::days_since_body::<JiffBackend>()
            }
            #[test]
            fn month_navigation() {
                super::month_navigation_body::<JiffBackend>()
            }
            #[test]
            fn calendar_grid_weeks_test() {
                super::calendar_grid_weeks_body::<JiffBackend>()
            }
        }
    }
} // close pub mod generic

// ── Backend-pinned wrapper modules ──────────────────────────────────────
//
// Each backend module re-exports the non-generic items from `generic`,
// provides type aliases for the generic structs/contexts pinned to its
// backend, and emits thin function wrappers for each generic component
// so call sites can drop the `::<Backend>` turbofish.

// The aliases and wrappers below mirror items documented in
// `super::generic`; modules invoking `backend_module_body!` carry an
// outer `#[allow(missing_docs)]` so they don't drown the lint output.
#[cfg(any(feature = "time", feature = "jiff"))]
macro_rules! backend_module_body {
    ($backend:ty) => {
        use dioxus::prelude::*;

        // Non-generic items pass through unchanged.
        pub use super::generic::{
            CalendarGridBody, CalendarGridBodyProps, CalendarGridCell, CalendarGridCellProps,
            CalendarGridHead, CalendarGridHeadProps, CalendarGridHeaderRow,
            CalendarGridHeaderRowProps, CalendarGridRoot, CalendarGridRootProps, CalendarGridWeek,
            CalendarGridWeekProps, CalendarHeader, CalendarHeaderProps, CalendarNavigation,
            CalendarNavigationProps, CalendarSelectMonth, CalendarSelectMonthProps,
            CalendarSelectYear, CalendarSelectYearProps,
        };

        // Generic structs/contexts become concrete aliases.
        pub type DateRange = super::generic::DateRange<$backend>;
        pub type BaseCalendarContext = super::generic::BaseCalendarContext<$backend>;
        pub type CalendarContext = super::generic::CalendarContext<$backend>;
        pub type RangeCalendarContext = super::generic::RangeCalendarContext<$backend>;
        pub type CalendarGridData = super::generic::CalendarGridData<$backend>;
        pub type CalendarGridWeekday = super::generic::CalendarGridWeekday<$backend>;
        pub type CalendarProps = super::generic::CalendarProps<$backend>;
        pub type RangeCalendarProps = super::generic::RangeCalendarProps<$backend>;
        pub type CalendarDayProps = super::generic::CalendarDayProps<$backend>;
        pub type CalendarSelectMonthOptionProps =
            super::generic::CalendarSelectMonthOptionProps<$backend>;
        pub type CalendarGridDayHeaderProps = super::generic::CalendarGridDayHeaderProps<$backend>;
        pub type CalendarViewProps = super::generic::CalendarViewProps<$backend>;
        pub type CalendarPreviousMonthButtonProps =
            super::generic::CalendarPreviousMonthButtonProps<$backend>;
        pub type CalendarNextMonthButtonProps =
            super::generic::CalendarNextMonthButtonProps<$backend>;
        pub type CalendarMonthTitleProps = super::generic::CalendarMonthTitleProps<$backend>;
        pub type CalendarGridProps = super::generic::CalendarGridProps<$backend>;
        pub type CalendarSelectMonthSelectProps =
            super::generic::CalendarSelectMonthSelectProps<$backend>;
        pub type CalendarSelectMonthValueProps =
            super::generic::CalendarSelectMonthValueProps<$backend>;
        pub type CalendarSelectYearSelectProps =
            super::generic::CalendarSelectYearSelectProps<$backend>;
        pub type CalendarSelectYearOptionProps =
            super::generic::CalendarSelectYearOptionProps<$backend>;
        pub type CalendarSelectYearValueProps =
            super::generic::CalendarSelectYearValueProps<$backend>;

        // Generic components become non-generic wrappers.
        #[component]
        pub fn Calendar(props: CalendarProps) -> Element {
            super::generic::Calendar::<$backend>(props)
        }
        #[component]
        pub fn RangeCalendar(props: RangeCalendarProps) -> Element {
            super::generic::RangeCalendar::<$backend>(props)
        }
        #[component]
        pub fn CalendarView(props: CalendarViewProps) -> Element {
            super::generic::CalendarView::<$backend>(props)
        }
        #[component]
        pub fn CalendarPreviousMonthButton(props: CalendarPreviousMonthButtonProps) -> Element {
            super::generic::CalendarPreviousMonthButton::<$backend>(props)
        }
        #[component]
        pub fn CalendarNextMonthButton(props: CalendarNextMonthButtonProps) -> Element {
            super::generic::CalendarNextMonthButton::<$backend>(props)
        }
        #[component]
        pub fn CalendarMonthTitle(props: CalendarMonthTitleProps) -> Element {
            super::generic::CalendarMonthTitle::<$backend>(props)
        }
        #[component]
        pub fn CalendarGrid(props: CalendarGridProps) -> Element {
            super::generic::CalendarGrid::<$backend>(props)
        }
        #[component]
        pub fn CalendarGridDayHeader(props: CalendarGridDayHeaderProps) -> Element {
            super::generic::CalendarGridDayHeader::<$backend>(props)
        }
        #[component]
        pub fn CalendarSelectMonthSelect(props: CalendarSelectMonthSelectProps) -> Element {
            super::generic::CalendarSelectMonthSelect::<$backend>(props)
        }
        #[component]
        pub fn CalendarSelectMonthOption(props: CalendarSelectMonthOptionProps) -> Element {
            super::generic::CalendarSelectMonthOption::<$backend>(props)
        }
        #[component]
        pub fn CalendarSelectMonthValue(props: CalendarSelectMonthValueProps) -> Element {
            super::generic::CalendarSelectMonthValue::<$backend>(props)
        }
        #[component]
        pub fn CalendarSelectYearSelect(props: CalendarSelectYearSelectProps) -> Element {
            super::generic::CalendarSelectYearSelect::<$backend>(props)
        }
        #[component]
        pub fn CalendarSelectYearOption(props: CalendarSelectYearOptionProps) -> Element {
            super::generic::CalendarSelectYearOption::<$backend>(props)
        }
        #[component]
        pub fn CalendarSelectYearValue(props: CalendarSelectYearValueProps) -> Element {
            super::generic::CalendarSelectYearValue::<$backend>(props)
        }
        #[component]
        pub fn CalendarDay(props: CalendarDayProps) -> Element {
            super::generic::CalendarDay::<$backend>(props)
        }

        // Hooks pinned to the backend.
        pub fn use_calendar_grid() -> CalendarGridData {
            super::generic::use_calendar_grid::<$backend>()
        }
    };
}

/// Calendar items pinned to [`crate::date_backend::TimeBackend`].
#[cfg(feature = "time")]
pub mod time {
    #![allow(missing_docs)]
    use crate::date_backend::TimeBackend;
    backend_module_body!(TimeBackend);
}

/// Calendar items pinned to [`crate::date_backend::JiffBackend`].
#[cfg(feature = "jiff")]
pub mod jiff {
    #![allow(missing_docs)]
    use crate::date_backend::JiffBackend;
    backend_module_body!(JiffBackend);
}

// Default surface: re-export the `time` backend's items at the module
// root so existing call sites compile unchanged. Jiff-only users must
// use `calendar::jiff::*` explicitly; this keeps the feature set
// additive (enabling `time` on top of `jiff` never changes which
// backend the root surface resolves to).
#[cfg(feature = "time")]
pub use time::*;
