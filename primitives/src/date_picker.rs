//! Defines the [`DatePicker`] and [`DateRangePicker`] components and its subcomponents, which allowing users to enter or select a date value.
//!
//! The backend-generic implementations live in [`generic`] and are always
//! available. The [`time`] and [`jiff`] sub-modules expose concrete versions
//! pinned to those backends when their cargo features are enabled, and with
//! the default `time` feature on those items are re-exported at the module
//! root for backwards-compatible usage. Builds with `time` disabled
//! (`--no-default-features --features jiff`) have no root-level surface;
//! reach for [`jiff::DatePicker`] etc. directly so enabling `time` on top of
//! `jiff` never changes which backend the root resolves to.

/// The backend-generic implementations of every date-picker component, prop,
/// and context. Most users should reach for the backend-pinned re-exports in
/// [`crate::date_picker`] (or [`crate::date_picker::time`] /
/// [`crate::date_picker::jiff`]) instead.
pub mod generic {
    use crate::{
        calendar::generic::{
            weekday_abbreviation, AvailableRanges, CalendarProps, DateRange, RangeCalendarProps,
        },
        dioxus_core::Properties,
        focus::{use_focus_controlled_item_disabled, use_focus_provider, FocusState},
        popover::*,
        use_unique_id,
    };

    use crate::date_backend::DateBackend;
    use dioxus::prelude::*;
    use num_integer::Integer;
    use std::{fmt::Display, str::FromStr};

    /// The context provided by the [`DatePicker`] component to its children.
    #[derive(Copy, Clone)]
    struct BaseDatePickerContext<B: DateBackend> {
        // State
        open: Signal<bool>,
        read_only: ReadSignal<bool>,

        // Configuration
        disabled: ReadSignal<bool>,
        focus: FocusState,
        enabled_date_range: DateRange<B>,
        available_ranges: Memo<AvailableRanges<B>>,
    }

    /// The context provided by the [`DatePicker`] component to its children.
    #[derive(Copy, Clone)]
    struct DatePickerContext<B: DateBackend> {
        on_value_change: Callback<Option<B::Date>>,
        selected_date: ReadSignal<Option<B::Date>>,
    }

    impl<B: DateBackend> DatePickerContext<B> {
        fn set_date(&mut self, date: Option<B::Date>) {
            let value = { self.selected_date.peek().cloned() };
            if value != date {
                self.on_value_change.call(date);
            }
        }
    }

    /// The props for the [`DatePicker`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerProps<B: DateBackend> {
        /// Callback when value changes
        #[props(default)]
        pub on_value_change: Callback<Option<B::Date>>,

        /// The selected date
        #[props(default)]
        pub selected_date: ReadSignal<Option<B::Date>>,

        /// Whether the date picker is disabled
        #[props(default)]
        pub disabled: ReadSignal<bool>,

        /// Whether the date picker is enable user input
        #[props(default = ReadSignal::new(Signal::new(false)))]
        pub read_only: ReadSignal<bool>,

        /// Lower limit of the range of available dates
        #[props(default = B::from_ymd(1925, B::JANUARY, 1).unwrap())]
        pub min_date: B::Date,

        /// Upper limit of the range of available dates
        #[props(default = B::from_ymd(2050, B::DECEMBER, 31).unwrap())]
        pub max_date: B::Date,

        /// Unavailable dates
        #[props(default)]
        pub disabled_ranges: ReadSignal<Vec<DateRange<B>>>,

        /// Whether focus should loop around when reaching the end.
        #[props(default = ReadSignal::new(Signal::new(false)))]
        pub roving_loop: ReadSignal<bool>,

        /// Additional attributes to extend the date picker element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the date picker element
        pub children: Element,
    }

    /// # DatePicker
    ///
    /// The [`DatePicker`] component provides an accessible date input interface.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::Calendar, date_picker::*, popover::*, ContentAlign};
    /// use ::time::Date;
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_date = use_signal(|| None::<Date>);
    ///    rsx! {
    ///        div {
    ///            DatePicker {
    ///                selected_date: selected_date(),
    ///                on_value_change: move |date| {
    ///                    tracing::info!("Date changed to: {date:?}");
    ///                    selected_date.set(date);
    ///               },
    ///                DatePickerPopover {
    ///                    DatePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DatePickerCalendar {
    ///                                calendar: Calendar,
    ///                            }
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    ///
    /// # Styling
    ///
    /// The [`DatePicker`] component defines the following data attributes you can use to control styling:
    /// - `data-disabled`: Indicates if the DatePicker is disabled. Possible values are `true` or `false`.
    #[component]
    pub fn DatePicker<B: DateBackend>(props: DatePickerProps<B>) -> Element {
        let open = use_signal(|| false);
        let focus = use_focus_provider(props.roving_loop);
        let available_ranges =
            use_memo(move || AvailableRanges::<B>::new(&props.disabled_ranges.read()));

        // Create context provider for child components
        use_context_provider(|| BaseDatePickerContext::<B> {
            open,
            read_only: props.read_only,
            disabled: props.disabled,
            focus,
            enabled_date_range: DateRange::<B>::new(props.min_date, props.max_date),
            available_ranges,
        });

        use_context_provider(|| DatePickerContext::<B> {
            on_value_change: props.on_value_change,
            selected_date: props.selected_date,
        });

        rsx! {
            div {
                role: "group",
                aria_label: "Date",
                "data-disabled": (props.disabled)(),
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The context provided by the [`DateRangePicker`] component to its children.
    #[derive(Copy, Clone)]
    pub struct DateRangePickerContext<B: DateBackend> {
        // Currently selected date range
        date_range: ReadSignal<Option<DateRange<B>>>,
        set_selected_range: Callback<Option<DateRange<B>>>,
    }

    impl<B: DateBackend> DateRangePickerContext<B> {
        /// Set the selected date
        pub fn set_range(&mut self, range: Option<DateRange<B>>) {
            if (self.date_range)() != range {
                self.set_selected_range.call(range);
            }
        }
    }

    /// The props for the [`DatePicker`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DateRangePickerProps<B: DateBackend> {
        /// Callback when value changes
        #[props(default)]
        pub on_range_change: Callback<Option<DateRange<B>>>,

        /// The selected date
        #[props(default)]
        pub selected_range: ReadSignal<Option<DateRange<B>>>,

        /// Whether the date picker is disabled
        #[props(default)]
        pub disabled: ReadSignal<bool>,

        /// Whether the date picker is enable user input
        #[props(default = ReadSignal::new(Signal::new(false)))]
        pub read_only: ReadSignal<bool>,

        /// Lower limit of the range of available dates
        #[props(default = B::from_ymd(1925, B::JANUARY, 1).unwrap())]
        pub min_date: B::Date,

        /// Upper limit of the range of available dates
        #[props(default = B::from_ymd(2050, B::DECEMBER, 31).unwrap())]
        pub max_date: B::Date,

        /// Unavailable dates
        #[props(default)]
        pub disabled_ranges: ReadSignal<Vec<DateRange<B>>>,

        /// Whether focus should loop around when reaching the end.
        #[props(default = ReadSignal::new(Signal::new(false)))]
        pub roving_loop: ReadSignal<bool>,

        /// Additional attributes to extend the date picker element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the date picker element
        pub children: Element,
    }

    /// # DateRangePicker
    ///
    /// The [`DateRangePicker`] component provides an accessible date range input interface.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::{DateRange, RangeCalendar}, date_picker::*, popover::*, ContentAlign};
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_range = use_signal(|| None::<DateRange>);
    ///    rsx! {
    ///        div {
    ///            DateRangePicker {
    ///                selected_range: selected_range(),
    ///                on_range_change: move |range| {
    ///                    tracing::info!("Selected range: {:?}", range);
    ///                    selected_range.set(range);
    ///               },
    ///                DatePickerPopover {
    ///                    DatePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DateRangePickerCalendar {
    ///                                calendar: RangeCalendar,
    ///                            }
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    ///
    /// # Styling
    ///
    /// The [`DateRangePicker`] component defines the following data attributes you can use to control styling:
    /// - `data-disabled`: Indicates if the DateRangePicker is disabled. Possible values are `true` or `false`.
    #[component]
    pub fn DateRangePicker<B: DateBackend>(props: DateRangePickerProps<B>) -> Element {
        let open = use_signal(|| false);
        let focus = use_focus_provider(props.roving_loop);

        let available_ranges =
            use_memo(move || AvailableRanges::<B>::new(&props.disabled_ranges.read()));

        // Create context provider for child components
        use_context_provider(|| BaseDatePickerContext {
            open,
            read_only: props.read_only,
            disabled: props.disabled,
            focus,
            enabled_date_range: DateRange::<B>::new(props.min_date, props.max_date),
            available_ranges,
        });

        use_context_provider(|| DateRangePickerContext {
            date_range: props.selected_range,
            set_selected_range: props.on_range_change,
        });

        rsx! {
            div {
                role: "group",
                aria_label: "Date Range",
                "data-disabled": (props.disabled)(),
                ..props.attributes,
                {props.children}
            }
        }
    }

    /// The props for the [`DatePickerPopover`] component.
    #[allow(unpredictable_function_pointer_comparisons)]
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerPopoverProps<B: DateBackend> {
        /// Whether the popover is a modal and should capture focus.
        #[props(default = ReadSignal::new(Signal::new(true)))]
        pub is_modal: ReadSignal<bool>,

        /// The controlled open state of the popover.
        pub open: ReadSignal<Option<bool>>,

        /// The default open state when uncontrolled.
        #[props(default)]
        pub default_open: bool,

        /// Callback fired when the open state changes.
        #[props(default)]
        pub on_open_change: Callback<bool>,

        /// Additional attributes to apply to the popover root element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the popover root component.
        pub children: Element,

        /// The popover root component to use.
        #[props(default = PopoverRoot)]
        pub popover_root: fn(PopoverRootProps) -> Element,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # DatePickerPopover
    ///
    /// The `DatePickerPopover` component wraps all the popover components and manages the state.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::Calendar, date_picker::*, popover::*, ContentAlign};
    /// use ::time::Date;
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_date = use_signal(|| None::<Date>);
    ///    rsx! {
    ///        div {
    ///            DatePicker {
    ///                selected_date: selected_date(),
    ///                on_value_change: move |date| {
    ///                    tracing::info!("Date changed to: {date:?}");
    ///                    selected_date.set(date);
    ///               },
    ///                DatePickerPopover {
    ///                    DatePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DatePickerCalendar {
    ///                                calendar: Calendar,
    ///                            }
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    #[component]
    pub fn DatePickerPopover<B: DateBackend>(props: DatePickerPopoverProps<B>) -> Element {
        let ctx = use_context::<BaseDatePickerContext<B>>();
        let mut open = ctx.open;

        let PopoverRoot = props.popover_root;

        rsx! {
            PopoverRoot {
                open: open(),
                on_open_change: move |v| open.set(v),
                attributes: props.attributes,
                {props.children}
            }
        }
    }

    #[doc(hidden)]
    /// A trait for types that can provide default calendar rendering.
    pub trait DefaultCalendarProps {
        /// Provide a default calendar rendering function.
        fn default_calendar(self) -> Element;
    }

    /// The props for the Calendar component.
    #[allow(unpredictable_function_pointer_comparisons)]
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerCalendarProps<
        B: DateBackend,
        T: DefaultCalendarProps + Properties + PartialEq = CalendarProps<B>,
    > {
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

        /// The calendar to render with
        #[props(default = T::default_calendar)]
        pub calendar: fn(T) -> Element,
    }

    /// # DatePickerCalendar
    ///
    /// The [`DatePickerCalendar`] component provides an accessible calendar interface with arrow key navigation, month switching, and date selection.
    /// Used as date picker popover component
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::Calendar, date_picker::*, popover::*, ContentAlign};
    /// use ::time::Date;
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_date = use_signal(|| None::<Date>);
    ///    rsx! {
    ///        div {
    ///            DatePicker {
    ///                selected_date: selected_date(),
    ///                on_value_change: move |date| {
    ///                    tracing::info!("Date changed to: {date:?}");
    ///                    selected_date.set(date);
    ///               },
    ///                DatePickerPopover {
    ///                    DatePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DatePickerCalendar {}
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    #[component]
    pub fn DatePickerCalendar<B: DateBackend>(
        props: DatePickerCalendarProps<B, CalendarProps<B>>,
    ) -> Element {
        let mut base_ctx = use_context::<BaseDatePickerContext<B>>();
        let mut ctx = use_context::<DatePickerContext<B>>();

        #[allow(non_snake_case)]
        let Calendar = props.calendar;
        let mut view_date = use_signal(|| props.today);
        use_effect(move || {
            if let Some(date) = (ctx.selected_date)() {
                view_date.set(date);
            }
        });

        let min_date = base_ctx.enabled_date_range.start();
        let max_date = base_ctx.enabled_date_range.end();

        rsx! {
            Calendar {
                selected_date: ctx.selected_date,
                on_date_change: move |date| {
                    ctx.set_date(date);
                    base_ctx.open.set(false);
                },
                disabled_ranges: base_ctx.available_ranges.read().to_disabled_ranges(),
                on_format_weekday: props.on_format_weekday,
                on_format_month: props.on_format_month,
                view_date: view_date(),
                on_view_change: move |date| view_date.set(date),
                today: props.today,
                disabled: props.disabled,
                first_day_of_week: props.first_day_of_week,
                min_date,
                max_date,
                attributes: props.attributes,
                {props.children}
            }
        }
    }

    /// # DateRangePickerCalendar
    ///
    /// The [`DateRangePickerCalendar`] component provides an accessible calendar interface with arrow key navigation, month switching, and date range selection.
    /// Used as date picker popover component
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::{DateRange, RangeCalendar}, date_picker::*, popover::*, ContentAlign};
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_range = use_signal(|| None::<DateRange>);
    ///    rsx! {
    ///        div {
    ///            DateRangePicker {
    ///                selected_range: selected_range(),
    ///                on_range_change: move |range| {
    ///                    tracing::info!("Selected range: {:?}", range);
    ///                    selected_range.set(range);
    ///               },
    ///                DatePickerPopover {
    ///                    DateRangePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DateRangePickerCalendar {}
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    #[component]
    pub fn DateRangePickerCalendar<B: DateBackend>(
        props: DatePickerCalendarProps<B, RangeCalendarProps<B>>,
    ) -> Element {
        let mut base_ctx = use_context::<BaseDatePickerContext<B>>();
        let mut ctx = use_context::<DateRangePickerContext<B>>();

        #[allow(non_snake_case)]
        let RangeCalendar = props.calendar;
        let mut view_date = use_signal(|| props.today);
        use_effect(move || {
            if let Some(r) = (ctx.date_range)() {
                view_date.set(r.start());
            }
        });

        let min_date = base_ctx.enabled_date_range.start();
        let max_date = base_ctx.enabled_date_range.end();

        rsx! {
            RangeCalendar {
                selected_range: ctx.date_range,
                on_range_change: move |range| {
                    ctx.set_range(range);
                    base_ctx.open.set(false);
                },
                disabled_ranges: base_ctx.available_ranges.read().to_disabled_ranges(),
                on_format_weekday: props.on_format_weekday,
                on_format_month: props.on_format_month,
                view_date: view_date(),
                on_view_change: move |date| view_date.set(date),
                today: props.today,
                disabled: props.disabled,
                first_day_of_week: props.first_day_of_week,
                min_date,
                max_date,
                attributes: props.attributes,
                {props.children}
            }
        }
    }

    // The props for the [`DateSegment`] component
    #[derive(Props, Clone, PartialEq)]
    struct DateSegmentProps<T: Clone + Integer + 'static> {
        // The index of the segment
        pub index: ReadSignal<usize>,

        // The controlled value of the date picker
        pub value: ReadSignal<Option<T>>,

        // Default value
        pub default: T,

        // Callback when value changes
        #[props(default)]
        pub on_value_change: Callback<Option<T>>,

        // The minimum value
        pub min: ReadSignal<T>,

        // The maximum value
        pub max: ReadSignal<T>,

        // Max field length
        pub max_length: usize,

        // Callback when display placeholder
        pub on_format_placeholder: Callback<(), String>,

        // Additional attributes for the value element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,
    }

    #[component]
    fn DateSegment<B: DateBackend, T: Clone + Copy + Integer + FromStr + Display + 'static>(
        props: DateSegmentProps<T>,
    ) -> Element {
        let mut text_value = use_signal(|| "".to_string());
        use_effect(move || {
            let text = match (props.value)() {
                Some(value) => value.to_string(),
                None => String::default(),
            };
            text_value.set(text);
        });

        let mut reset_value = use_signal(|| false);

        // The formatted text for the segment
        let display_value = use_memo(move || {
            let value = (props.value)();
            match value {
                Some(value) => format!("{:0>width$}", value, width = props.max_length),
                None => props
                    .on_format_placeholder
                    .call(())
                    .repeat(props.max_length),
            }
        });

        let now_value = use_memo(move || (props.value)().unwrap_or(props.default));

        let mut ctx = use_context::<BaseDatePickerContext<B>>();

        let mut set_value = move |text: String| {
            if text.is_empty() {
                props.on_value_change.call(None);
                ctx.focus.focus_prev();
                return;
            }
            let min = props.min.cloned();
            let max = props.max.cloned();

            let value = text.parse::<T>().map(|v| v.min(max)).ok();
            if let Some(value) = value {
                let in_range = value >= min && value <= max;

                // If adding a new digit would exceed max, move to next segment
                let new_value = (text + "0").parse::<T>().unwrap_or(value);
                if in_range && new_value > max {
                    ctx.focus.focus_next();
                }
            };

            props.on_value_change.call(value);
        };
        use_effect(move || {
            // If this item is not focused, always keep the value clamped
            if !ctx.focus.is_focused(props.index.cloned()) {
                if let Some(value) = (props.value)() {
                    let clamped_value = value.clamp(props.min.cloned(), props.max.cloned());
                    if clamped_value != value {
                        props.on_value_change.call(Some(clamped_value));
                    }
                }
            }
        });

        let roll_value = move |value: T| {
            let min = props.min.cloned();
            let max = props.max.cloned();
            if value < min {
                max
            } else if value > max {
                min
            } else {
                value
            }
        };

        let handle_keydown = move |event: Event<KeyboardData>| {
            let key = event.key();
            match key {
                Key::Character(actual_char) => {
                    // Don't block keyboard shortcuts
                    if event.modifiers().ctrl()
                        || event.modifiers().meta()
                        || event.modifiers().alt()
                    {
                        return;
                    }
                    if actual_char.parse::<T>().is_ok() {
                        let mut text = text_value();
                        if text.len() == props.max_length || reset_value() {
                            text = String::default();
                            reset_value.set(false);
                        };
                        text.push_str(&actual_char);
                        set_value(text);
                    }
                    event.prevent_default();
                    event.stop_propagation();
                }
                Key::Backspace => {
                    let mut text = text_value();
                    if event.modifiers().ctrl() || event.modifiers().meta() {
                        text.clear();
                    } else {
                        text.pop();
                    }
                    set_value(text);
                }
                Key::Delete => {
                    let mut text = text_value();
                    text.remove(0);
                    set_value(text);
                }
                Key::ArrowLeft => {
                    ctx.focus.focus_prev();
                }
                Key::ArrowRight => {
                    ctx.focus.focus_next();
                }
                Key::Enter => {
                    ctx.focus.focus_next();
                    event.prevent_default();
                    event.stop_propagation();
                }
                Key::ArrowUp => {
                    let value = match (props.value)() {
                        Some(mut value) => {
                            value.inc();
                            roll_value(value)
                        }
                        None => props.default,
                    };
                    props.on_value_change.call(Some(value));
                }
                Key::ArrowDown => {
                    let value = match (props.value)() {
                        Some(mut value) => {
                            value.dec();
                            roll_value(value)
                        }
                        None => props.default,
                    };
                    props.on_value_change.call(Some(value));
                }
                _ => (),
            }
        };

        let disabled = move || (ctx.disabled)();
        let onmounted = use_focus_controlled_item_disabled(props.index, disabled);

        let span_id = use_unique_id();
        let id = use_memo(move || format!("span-{span_id}"));
        let label_id = format!("{id}-label");

        rsx! {
            span {
                id,
                role: "spinbutton",
                aria_valuemin: props.min.to_string(),
                aria_valuemax: props.max.to_string(),
                aria_valuenow: now_value.to_string(),
                aria_labelledby: "{label_id}",
                inputmode: "numeric",
                contenteditable: !(ctx.read_only)(),
                spellcheck: false,
                tabindex: "0",
                enterkeyhint: "next",
                onkeydown: handle_keydown,
                onmounted,
                onfocus: move |_| {
                    reset_value.set(true);
                    ctx.focus.set_focus(Some(props.index.cloned()));
                    if (ctx.open)() {
                        ctx.open.set(false);
                    }
                },
                "no-date": (props.value)().is_none(),
                "data-disabled": (ctx.disabled)(),
                ..props.attributes,
                {display_value}
            }
        }
    }

    #[derive(Clone, Copy)]
    struct DateElementContext {
        start_index: usize,
        year_value: Signal<Option<i32>>,
        month_value: Signal<Option<u8>>,
        day_value: Signal<Option<u8>>,
        on_format_day_placeholder: Callback<(), String>,
        on_format_month_placeholder: Callback<(), String>,
        on_format_year_placeholder: Callback<(), String>,
    }

    /// The props for the [`DatePickerYearSegment`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerYearSegmentProps<B: DateBackend> {
        /// Additional attributes for the year segment element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`DatePickerMonthSegment`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerMonthSegmentProps<B: DateBackend> {
        /// Additional attributes for the month segment element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`DatePickerDaySegment`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerDaySegmentProps<B: DateBackend> {
        /// Additional attributes for the day segment element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`DatePickerSeparator`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerSeparatorProps {
        /// The separator symbol.
        #[props(default = '-')]
        pub symbol: char,

        /// Additional attributes for the separator element.
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,
    }

    /// A year segment in a date input.
    #[component]
    pub fn DatePickerYearSegment<B: DateBackend>(props: DatePickerYearSegmentProps<B>) -> Element {
        let mut ctx = use_context::<DateElementContext>();
        let base_ctx = use_context::<BaseDatePickerContext<B>>();
        let today = B::today();
        let min_year = B::year(base_ctx.enabled_date_range.start());
        let max_year = B::year(base_ctx.enabled_date_range.end());

        rsx! {
            DateSegment::<B, i32> {
                aria_label: "year",
                index: ctx.start_index,
                value: ctx.year_value,
                default: B::year(today),
                on_value_change: move |value: Option<i32>| ctx.year_value.set(value),
                min: min_year,
                max: max_year,
                max_length: 4,
                on_format_placeholder: ctx.on_format_year_placeholder,
                attributes: props.attributes,
            }
        }
    }

    /// A month segment in a date input.
    #[component]
    pub fn DatePickerMonthSegment<B: DateBackend>(
        props: DatePickerMonthSegmentProps<B>,
    ) -> Element {
        let mut ctx = use_context::<DateElementContext>();
        let base_ctx = use_context::<BaseDatePickerContext<B>>();
        let today = B::today();
        let min_date = base_ctx.enabled_date_range.start();
        let max_date = base_ctx.enabled_date_range.end();
        let min_year = B::year(min_date);
        let max_year = B::year(max_date);
        let min_month = match (ctx.year_value)() {
            Some(year) if year == min_year => B::month(min_date),
            _ => B::JANUARY,
        };
        let max_month = match (ctx.year_value)() {
            Some(year) if year == max_year => B::month(max_date),
            _ => B::DECEMBER,
        };

        rsx! {
            DateSegment::<B, u8> {
                aria_label: "month",
                index: ctx.start_index + 1usize,
                value: ctx.month_value,
                default: B::month_to_number(B::month(today)),
                on_value_change: move |value: Option<u8>| ctx.month_value.set(value),
                min: B::month_to_number(min_month),
                max: B::month_to_number(max_month),
                max_length: 2,
                on_format_placeholder: ctx.on_format_month_placeholder,
                attributes: props.attributes,
            }
        }
    }

    /// A day segment in a date input.
    #[component]
    pub fn DatePickerDaySegment<B: DateBackend>(props: DatePickerDaySegmentProps<B>) -> Element {
        let mut ctx = use_context::<DateElementContext>();
        let base_ctx = use_context::<BaseDatePickerContext<B>>();
        let today = B::today();
        let min_date = base_ctx.enabled_date_range.start();
        let max_date = base_ctx.enabled_date_range.end();
        let min_year = B::year(min_date);
        let max_year = B::year(max_date);
        let min_day = match ((ctx.year_value)(), (ctx.month_value)()) {
            (Some(year), Some(month))
                if year == min_year && month == B::month_to_number(B::month(min_date)) =>
            {
                B::day(min_date)
            }
            _ => 1,
        };
        let max_day = match ((ctx.year_value)(), (ctx.month_value)()) {
            (Some(year), Some(month))
                if year == max_year && month == B::month_to_number(B::month(max_date)) =>
            {
                B::day(max_date)
            }
            (Some(year), Some(month)) => {
                if let Some(month) = B::month_from_number(month) {
                    B::month_length(month, year)
                } else {
                    31
                }
            }
            _ => 31,
        };

        rsx! {
            DateSegment::<B, u8> {
                aria_label: "day",
                index: ctx.start_index + 2usize,
                value: ctx.day_value,
                default: B::day(today),
                on_value_change: move |value: Option<u8>| ctx.day_value.set(value),
                min: min_day,
                max: max_day,
                max_length: 2,
                on_format_placeholder: ctx.on_format_day_placeholder,
                attributes: props.attributes,
            }
        }
    }

    /// A separator in a date input.
    #[component]
    pub fn DatePickerSeparator(props: DatePickerSeparatorProps) -> Element {
        rsx! {
            span {
                aria_hidden: "true",
                tabindex: "-1",
                "is-separator": true,
                "no-date": true,
                ..props.attributes,
                "{props.symbol}"
            }
        }
    }

    /// The props for the [`DatePickerInputValue`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerInputValueProps<B: DateBackend> {
        /// Callback when display day placeholder
        #[props(default = Callback::new(|_| "D".to_string()))]
        pub on_format_day_placeholder: Callback<(), String>,

        /// Callback when display month placeholder
        #[props(default = Callback::new(|_| "M".to_string()))]
        pub on_format_month_placeholder: Callback<(), String>,

        /// Callback when display year placeholder
        #[props(default = Callback::new(|_| "Y".to_string()))]
        pub on_format_year_placeholder: Callback<(), String>,

        /// The children of the date value.
        #[props(default)]
        pub children: Option<Element>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`DateRangePickerInputValue`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DateRangePickerInputValueProps<B: DateBackend> {
        /// Callback when display day placeholder
        #[props(default = Callback::new(|_| "D".to_string()))]
        pub on_format_day_placeholder: Callback<(), String>,

        /// Callback when display month placeholder
        #[props(default = Callback::new(|_| "M".to_string()))]
        pub on_format_month_placeholder: Callback<(), String>,

        /// Callback when display year placeholder
        #[props(default = Callback::new(|_| "Y".to_string()))]
        pub on_format_year_placeholder: Callback<(), String>,

        /// The children of the date range value.
        #[props(default)]
        pub children: Option<Element>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`DateRangePickerStartValue`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DateRangePickerStartValueProps<B: DateBackend> {
        /// The children of the start date value.
        #[props(default)]
        pub children: Option<Element>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// The props for the [`DateRangePickerEndValue`] component.
    #[derive(Props, Clone, PartialEq)]
    pub struct DateRangePickerEndValueProps<B: DateBackend> {
        /// The children of the end date value.
        #[props(default)]
        pub children: Option<Element>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    #[derive(Clone, Copy)]
    struct DateRangeInputContext<B: DateBackend> {
        start_date: Signal<Option<B::Date>>,
        end_date: Signal<Option<B::Date>>,
        on_format_day_placeholder: Callback<(), String>,
        on_format_month_placeholder: Callback<(), String>,
        on_format_year_placeholder: Callback<(), String>,
    }

    #[derive(Props, Clone, PartialEq)]
    struct DateElementProps<B: DateBackend> {
        /// The start index (used for focus)
        #[props(default = 0)]
        pub start_index: usize,

        /// The selected date
        pub selected_date: ReadSignal<Option<B::Date>>,

        /// Callback when selected date changes
        #[props(default)]
        pub on_date_change: Callback<Option<B::Date>>,

        /// Callback when display day placeholder
        #[props(default = Callback::new(|_| "D".to_string()))]
        pub on_format_day_placeholder: Callback<(), String>,

        /// Callback when display month placeholder
        #[props(default = Callback::new(|_| "M".to_string()))]
        pub on_format_month_placeholder: Callback<(), String>,

        /// Callback when display year placeholder
        #[props(default = Callback::new(|_| "Y".to_string()))]
        pub on_format_year_placeholder: Callback<(), String>,

        /// The children of the date element.
        #[props(default)]
        pub children: Option<Element>,
    }

    #[component]
    fn DateElement<B: DateBackend>(props: DateElementProps<B>) -> Element {
        let ctx = use_context::<BaseDatePickerContext<B>>();
        let selected_date = props.selected_date.peek().cloned();

        let mut day_value = use_signal(move || selected_date.map(|date| B::day(date)));
        let mut month_value =
            use_signal(move || selected_date.map(|date| B::month_to_number(B::month(date))));
        let mut year_value = use_signal(move || selected_date.map(|date| B::year(date)));

        use_effect(move || {
            let date = (props.selected_date)();
            year_value.set(date.map(|d| B::year(d)));
            month_value.set(date.map(|d| B::month_to_number(B::month(d))));
            day_value.set(date.map(|d| B::day(d)));
        });

        use_effect(move || {
            if let (Some(year), Some(month), Some(day)) = (
                year_value(),
                month_value().and_then(B::month_from_number),
                day_value(),
            ) {
                if let Some(date) = B::from_ymd(year, month, day)
                    .filter(|date| ctx.enabled_date_range.contains(*date))
                    .filter(|date| ctx.available_ranges.read().valid_interval(*date))
                {
                    props.on_date_change.call(Some(date));
                }
            }
        });

        use_context_provider(|| DateElementContext {
            start_index: props.start_index,
            year_value,
            month_value,
            day_value,
            on_format_day_placeholder: props.on_format_day_placeholder,
            on_format_month_placeholder: props.on_format_month_placeholder,
            on_format_year_placeholder: props.on_format_year_placeholder,
        });

        let children = props.children.unwrap_or_else(|| {
            rsx! {
                DatePickerYearSegment::<B> {}
                DatePickerSeparator {}
                DatePickerMonthSegment::<B> {}
                DatePickerSeparator {}
                DatePickerDaySegment::<B> {}
            }
        });

        rsx! {
            {children}
        }
    }

    /// The editable date value for a single date picker input.
    #[component]
    pub fn DatePickerInputValue<B: DateBackend>(props: DatePickerInputValueProps<B>) -> Element {
        let mut base_ctx = use_context::<BaseDatePickerContext<B>>();
        let mut ctx = use_context::<DatePickerContext<B>>();

        rsx! {
            DateElement::<B> {
                selected_date: ctx.selected_date,
                on_date_change: move |date| {
                    ctx.set_date(date);
                    base_ctx.open.set(false);
                },
                on_format_day_placeholder: props.on_format_day_placeholder,
                on_format_month_placeholder: props.on_format_month_placeholder,
                on_format_year_placeholder: props.on_format_year_placeholder,
                children: props.children,
            }
        }
    }

    /// The editable date range value for a date range picker input.
    #[component]
    pub fn DateRangePickerInputValue<B: DateBackend>(
        props: DateRangePickerInputValueProps<B>,
    ) -> Element {
        let base_ctx = use_context::<BaseDatePickerContext<B>>();
        let mut ctx = use_context::<DateRangePickerContext<B>>();
        let selected_range = ctx.date_range.peek().cloned();

        let mut start_date = use_signal(move || selected_range.map(|range| range.start()));
        let mut end_date = use_signal(move || selected_range.map(|range| range.end()));

        use_effect(move || {
            let date_range = ctx.date_range.cloned();
            start_date.set(date_range.map(|r| r.start()));
            end_date.set(date_range.map(|r| r.end()));
        });

        use_effect(move || {
            if let (Some(start), Some(end)) = (start_date(), end_date()) {
                // force auto validation for input range
                if end < start {
                    return;
                }

                // checking non-contiguous ranges
                if base_ctx
                    .available_ranges
                    .read()
                    .available_range(start, base_ctx.enabled_date_range)
                    .is_some_and(|r| r.contains(end))
                {
                    let range = Some(DateRange::<B>::new(start, end));
                    ctx.set_range(range);
                }
            };
        });

        use_context_provider(|| DateRangeInputContext::<B> {
            start_date,
            end_date,
            on_format_day_placeholder: props.on_format_day_placeholder,
            on_format_month_placeholder: props.on_format_month_placeholder,
            on_format_year_placeholder: props.on_format_year_placeholder,
        });

        let children = props.children.unwrap_or_else(|| {
            rsx! {
                DateRangePickerStartValue::<B> {}
                DatePickerSeparator {
                    symbol: '—',
                }
                DateRangePickerEndValue::<B> {}
            }
        });

        rsx! {
            {children}
        }
    }

    /// The editable start date value in a range picker input.
    #[component]
    pub fn DateRangePickerStartValue<B: DateBackend>(
        props: DateRangePickerStartValueProps<B>,
    ) -> Element {
        let mut ctx = use_context::<DateRangeInputContext<B>>();

        rsx! {
            DateElement::<B> {
                selected_date: ctx.start_date,
                on_date_change: move |date| ctx.start_date.set(date),
                on_format_day_placeholder: ctx.on_format_day_placeholder,
                on_format_month_placeholder: ctx.on_format_month_placeholder,
                on_format_year_placeholder: ctx.on_format_year_placeholder,
                children: props.children,
            }
        }
    }

    /// The editable end date value in a range picker input.
    #[component]
    pub fn DateRangePickerEndValue<B: DateBackend>(
        props: DateRangePickerEndValueProps<B>,
    ) -> Element {
        let mut ctx = use_context::<DateRangeInputContext<B>>();

        rsx! {
            DateElement::<B> {
                start_index: 3,
                selected_date: ctx.end_date,
                on_date_change: move |date| ctx.end_date.set(date),
                on_format_day_placeholder: ctx.on_format_day_placeholder,
                on_format_month_placeholder: ctx.on_format_month_placeholder,
                on_format_year_placeholder: ctx.on_format_year_placeholder,
                children: props.children,
            }
        }
    }

    /// The props for the [`DatePickerInput`] component
    #[derive(Props, Clone, PartialEq)]
    pub struct DatePickerInputProps<B: DateBackend> {
        /// Callback when display day placeholder
        #[props(default = Callback::new(|_| "D".to_string()))]
        pub on_format_day_placeholder: Callback<(), String>,

        /// Callback when display month placeholder
        #[props(default = Callback::new(|_| "M".to_string()))]
        pub on_format_month_placeholder: Callback<(), String>,

        /// Callback when display year placeholder
        #[props(default = Callback::new(|_| "Y".to_string()))]
        pub on_format_year_placeholder: Callback<(), String>,

        /// Additional attributes for the value element
        #[props(extends = GlobalAttributes)]
        pub attributes: Vec<Attribute>,

        /// The children of the date picker element
        #[props(default)]
        pub children: Option<Element>,

        // Hidden marker that propagates the backend type parameter through
        // the builder when no other field constrains it.
        #[doc(hidden)]
        #[props(default)]
        _phantom: std::marker::PhantomData<B>,
    }

    /// # DatePickerInput
    ///
    /// The input element for the [`DatePicker`] component which allow users to enter a date value.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::Calendar, date_picker::*, popover::*, ContentAlign};
    /// use ::time::Date;
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_date = use_signal(|| None::<Date>);
    ///    rsx! {
    ///        div {
    ///            DatePicker {
    ///                selected_date: selected_date(),
    ///                on_value_change: move |date| {
    ///                    tracing::info!("Date changed to: {date:?}");
    ///                    selected_date.set(date);
    ///               },
    ///                DatePickerPopover {
    ///                    DatePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DatePickerCalendar {
    ///                                calendar: Calendar,
    ///                            }
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    #[component]
    pub fn DatePickerInput<B: DateBackend>(props: DatePickerInputProps<B>) -> Element {
        let children = props.children.unwrap_or_else(|| {
            rsx! {
                DatePickerInputValue::<B> {
                    on_format_day_placeholder: props.on_format_day_placeholder,
                    on_format_month_placeholder: props.on_format_month_placeholder,
                    on_format_year_placeholder: props.on_format_year_placeholder,
                }
            }
        });

        rsx! {
            div { ..props.attributes,
                {children}
            }
        }
    }

    /// # DateRangePickerInput
    ///
    /// The input element for the [`DateRangePicker`] component which allow users to enter a date range.
    ///
    /// ## Example
    /// ```rust
    /// use dioxus::prelude::*;
    /// use dioxus_primitives::{calendar::{DateRange, RangeCalendar}, date_picker::*, popover::*, ContentAlign};
    /// #[component]
    /// fn Demo() -> Element {
    ///    let mut selected_range = use_signal(|| None::<DateRange>);
    ///    rsx! {
    ///        div {
    ///            DateRangePicker {
    ///                selected_range: selected_range(),
    ///                on_range_change: move |range| {
    ///                    tracing::info!("Selected range: {:?}", range);
    ///                    selected_range.set(range);
    ///               },
    ///                DatePickerPopover {
    ///                    DateRangePickerInput {
    ///                        PopoverTrigger {
    ///                            "Select date"
    ///                        }
    ///                        PopoverContent {
    ///                            align: ContentAlign::End,
    ///                            DateRangePickerCalendar {
    ///                                calendar: RangeCalendar,
    ///                            }
    ///                        }
    ///                    }
    ///                }
    ///            }
    ///        }
    ///    }
    ///}
    /// ```
    #[component]
    pub fn DateRangePickerInput<B: DateBackend>(props: DatePickerInputProps<B>) -> Element {
        let children = props.children.unwrap_or_else(|| {
            rsx! {
                DateRangePickerInputValue::<B> {
                    on_format_day_placeholder: props.on_format_day_placeholder,
                    on_format_month_placeholder: props.on_format_month_placeholder,
                    on_format_year_placeholder: props.on_format_year_placeholder,
                }
            }
        });

        rsx! {
            div { ..props.attributes,
                {children}
            }
        }
    }

    #[cfg(all(test, feature = "time"))]
    mod tests {
        use super::*;
        use crate::date_backend::TimeBackend;

        #[component]
        fn ControlledDatePicker() -> Element {
            rsx! {
                DatePicker::<TimeBackend> {
                    selected_date: Some(TimeBackend::from_ymd(2026, TimeBackend::MAY, 7).unwrap()),
                    DatePickerInput::<TimeBackend> {}
                }
            }
        }

        #[component]
        fn ControlledDateRangePicker() -> Element {
            rsx! {
                DateRangePicker::<TimeBackend> {
                    selected_range: Some(DateRange::<TimeBackend>::new(
                        TimeBackend::from_ymd(2026, TimeBackend::MAY, 7).unwrap(),
                        TimeBackend::from_ymd(2026, TimeBackend::MAY, 11).unwrap(),
                    )),
                    DateRangePickerInput::<TimeBackend> {}
                }
            }
        }

        #[test]
        fn date_picker_input_renders_controlled_date_on_first_render() {
            let mut dom = VirtualDom::new(ControlledDatePicker);
            dom.rebuild_in_place();
            let html = dioxus_ssr::render(&dom);

            assert!(html.contains("2026"));
            assert!(html.contains("05"));
            assert!(html.contains("07"));
            assert!(!html.contains("YYYY"));
            assert!(!html.contains("MM"));
            assert!(!html.contains("DD"));
        }

        #[test]
        fn date_range_picker_input_renders_controlled_range_on_first_render() {
            let mut dom = VirtualDom::new(ControlledDateRangePicker);
            dom.rebuild_in_place();
            let html = dioxus_ssr::render(&dom);

            assert!(html.contains("2026"));
            assert!(html.contains("05"));
            assert!(html.contains("07"));
            assert!(html.contains("11"));
            assert!(!html.contains("YYYY"));
            assert!(!html.contains("MM"));
            assert!(!html.contains("DD"));
        }
    }
} // close pub mod generic

// ── Backend-pinned wrapper modules ──────────────────────────────────────

// Modules invoking `backend_module_body!` carry an outer
// `#[allow(missing_docs)]` since the items below mirror the ones
// documented in `super::generic`.
#[cfg(any(feature = "time", feature = "jiff"))]
macro_rules! backend_module_body {
    ($backend:ty) => {
        use dioxus::prelude::*;

        // Non-generic items pass through unchanged.
        pub use super::generic::{DatePickerSeparator, DatePickerSeparatorProps};

        // Generic structs/contexts become concrete aliases.
        pub type DatePickerProps = super::generic::DatePickerProps<$backend>;
        pub type DateRangePickerProps = super::generic::DateRangePickerProps<$backend>;
        pub type DateRangePickerContext = super::generic::DateRangePickerContext<$backend>;
        pub type DatePickerPopoverProps = super::generic::DatePickerPopoverProps<$backend>;
        pub type DatePickerYearSegmentProps = super::generic::DatePickerYearSegmentProps<$backend>;
        pub type DatePickerMonthSegmentProps =
            super::generic::DatePickerMonthSegmentProps<$backend>;
        pub type DatePickerDaySegmentProps = super::generic::DatePickerDaySegmentProps<$backend>;
        pub type DatePickerInputValueProps = super::generic::DatePickerInputValueProps<$backend>;
        pub type DateRangePickerInputValueProps =
            super::generic::DateRangePickerInputValueProps<$backend>;
        pub type DateRangePickerStartValueProps =
            super::generic::DateRangePickerStartValueProps<$backend>;
        pub type DateRangePickerEndValueProps =
            super::generic::DateRangePickerEndValueProps<$backend>;
        pub type DatePickerInputProps = super::generic::DatePickerInputProps<$backend>;

        // Generic components become non-generic wrappers.
        #[component]
        pub fn DatePicker(props: DatePickerProps) -> Element {
            super::generic::DatePicker::<$backend>(props)
        }
        #[component]
        pub fn DateRangePicker(props: DateRangePickerProps) -> Element {
            super::generic::DateRangePicker::<$backend>(props)
        }
        #[component]
        pub fn DatePickerPopover(props: DatePickerPopoverProps) -> Element {
            super::generic::DatePickerPopover::<$backend>(props)
        }
        #[component]
        pub fn DatePickerYearSegment(props: DatePickerYearSegmentProps) -> Element {
            super::generic::DatePickerYearSegment::<$backend>(props)
        }
        #[component]
        pub fn DatePickerMonthSegment(props: DatePickerMonthSegmentProps) -> Element {
            super::generic::DatePickerMonthSegment::<$backend>(props)
        }
        #[component]
        pub fn DatePickerDaySegment(props: DatePickerDaySegmentProps) -> Element {
            super::generic::DatePickerDaySegment::<$backend>(props)
        }
        #[component]
        pub fn DatePickerInputValue(props: DatePickerInputValueProps) -> Element {
            super::generic::DatePickerInputValue::<$backend>(props)
        }
        #[component]
        pub fn DateRangePickerInputValue(props: DateRangePickerInputValueProps) -> Element {
            super::generic::DateRangePickerInputValue::<$backend>(props)
        }
        #[component]
        pub fn DateRangePickerStartValue(props: DateRangePickerStartValueProps) -> Element {
            super::generic::DateRangePickerStartValue::<$backend>(props)
        }
        #[component]
        pub fn DateRangePickerEndValue(props: DateRangePickerEndValueProps) -> Element {
            super::generic::DateRangePickerEndValue::<$backend>(props)
        }
        #[component]
        pub fn DatePickerInput(props: DatePickerInputProps) -> Element {
            super::generic::DatePickerInput::<$backend>(props)
        }
        #[component]
        pub fn DateRangePickerInput(props: DatePickerInputProps) -> Element {
            super::generic::DateRangePickerInput::<$backend>(props)
        }

        // `DatePickerCalendar` and `DateRangePickerCalendar` take a second
        // generic param (the calendar variant); expose the two canonical
        // pairings as concrete components.
        #[component]
        pub fn DatePickerCalendar(
            props: super::generic::DatePickerCalendarProps<
                $backend,
                crate::calendar::generic::CalendarProps<$backend>,
            >,
        ) -> Element {
            super::generic::DatePickerCalendar::<$backend>(props)
        }
        #[component]
        pub fn DateRangePickerCalendar(
            props: super::generic::DatePickerCalendarProps<
                $backend,
                crate::calendar::generic::RangeCalendarProps<$backend>,
            >,
        ) -> Element {
            super::generic::DateRangePickerCalendar::<$backend>(props)
        }

        // The `DefaultCalendarProps` trait is re-exported so downstream
        // can name it without reaching into `generic::`.
        pub use super::generic::DefaultCalendarProps;
    };
}

/// DatePicker items pinned to [`crate::date_backend::TimeBackend`].
#[cfg(feature = "time")]
pub mod time {
    #![allow(missing_docs)]
    use crate::date_backend::TimeBackend;
    backend_module_body!(TimeBackend);
}

/// DatePicker items pinned to [`crate::date_backend::JiffBackend`].
#[cfg(feature = "jiff")]
pub mod jiff {
    #![allow(missing_docs)]
    use crate::date_backend::JiffBackend;
    backend_module_body!(JiffBackend);
}

#[cfg(feature = "time")]
pub use time::*;
