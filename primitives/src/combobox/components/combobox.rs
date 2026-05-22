//! Root combobox component.

use dioxus::prelude::*;

use super::super::context::{default_combobox_filter, ComboboxContext};
use crate::{
    selectable::{
        use_multi_selectable_value, use_selectable_root, use_single_selectable_value,
        RcPartialEqValue, SelectionMode,
    },
    use_controlled, Controlled,
};

/// Props for [`Combobox`].
#[derive(Props, Clone, PartialEq)]
pub struct ComboboxProps<T: Clone + PartialEq + 'static = String> {
    /// The controlled value. If supplied, the combobox is controlled
    /// and the signal's `None` value means no option is selected.
    #[props(default)]
    pub value: Option<ReadSignal<Option<T>>>,

    /// The default uncontrolled value.
    #[props(default)]
    pub default_value: Option<T>,

    /// Callback fired when the value changes.
    #[props(default)]
    pub on_value_change: Callback<Option<T>>,

    /// Whether the combobox is disabled.
    #[props(default)]
    pub disabled: ReadSignal<bool>,

    /// The controlled open state of the popup.
    #[props(default)]
    pub open: ReadSignal<Option<bool>>,

    /// The initial open state when uncontrolled.
    #[props(default)]
    pub default_open: ReadSignal<bool>,

    /// Callback fired when the popup open state changes.
    #[props(default)]
    pub on_open_change: Callback<bool>,

    /// The controlled text query used to filter options.
    #[props(default)]
    pub query: ReadSignal<Option<String>>,

    /// The initial text query when uncontrolled.
    #[props(default)]
    pub default_query: ReadSignal<String>,

    /// Callback fired when the text query changes.
    #[props(default)]
    pub on_query_change: Callback<String>,

    /// Whether arrow-key navigation should wrap.
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub roving_loop: ReadSignal<bool>,

    /// Custom filter callback. Receives `(query, option_text_value)`.
    #[props(default = Callback::new(|(q, t): (String, String)| default_combobox_filter(&q, &t)))]
    pub filter: Callback<(String, String), bool>,

    /// Additional attributes for the root element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// Children.
    pub children: Element,
}

#[allow(clippy::too_many_arguments)]
fn use_combobox_root(
    values: Memo<Vec<RcPartialEqValue>>,
    set_value: Callback<RcPartialEqValue>,
    selection_mode: SelectionMode,
    disabled: ReadSignal<bool>,
    roving_loop: ReadSignal<bool>,
    open: Controlled<bool>,
    query: Controlled<String>,
    filter: Callback<(String, String), bool>,
) -> Memo<bool> {
    let selectable = use_selectable_root(
        values,
        set_value,
        selection_mode,
        disabled,
        roving_loop,
        open,
    );
    let (query, set_query) = use_controlled(query.value, query.default.cloned(), query.on_change);
    let open = selectable.open;

    use_context_provider(|| ComboboxContext {
        selectable,
        query,
        set_query,
        filter,
    });

    open
}

/// A single-select autocomplete input with a filterable popup list.
#[component]
pub fn Combobox<T: Clone + PartialEq + 'static>(props: ComboboxProps<T>) -> Element {
    let (selected, set_value) = use_single_selectable_value(
        props.value,
        props.default_value,
        props.on_value_change,
        "combobox",
    );

    let open = use_combobox_root(
        selected,
        set_value,
        SelectionMode::Single,
        props.disabled,
        props.roving_loop,
        Controlled {
            value: props.open,
            default: props.default_open,
            on_change: props.on_open_change,
        },
        Controlled {
            value: props.query,
            default: props.default_query,
            on_change: props.on_query_change,
        },
        props.filter,
    );

    rsx! {
        div {
            "data-state": if open() { "open" } else { "closed" },
            "data-disabled": (props.disabled)(),
            ..props.attributes,
            {props.children}
        }
    }
}

/// Props for [`ComboboxMulti`].
#[derive(Props, Clone, PartialEq)]
pub struct ComboboxMultiProps<T: Clone + PartialEq + 'static = String> {
    /// The controlled list of selected values. `None` leaves the combobox
    /// uncontrolled; `Some(vec![])` controls it to an empty selection.
    #[props(default)]
    pub values: ReadSignal<Option<Vec<T>>>,

    /// The default list of selected values when uncontrolled.
    #[props(default)]
    pub default_values: Vec<T>,

    /// Callback fired when the selection changes. Receives the full
    /// post-toggle list.
    #[props(default)]
    pub on_values_change: Callback<Vec<T>>,

    /// Whether the combobox is disabled.
    #[props(default)]
    pub disabled: ReadSignal<bool>,

    /// The controlled open state of the popup.
    #[props(default)]
    pub open: ReadSignal<Option<bool>>,

    /// The initial open state when uncontrolled.
    #[props(default)]
    pub default_open: ReadSignal<bool>,

    /// Callback fired when the popup open state changes.
    #[props(default)]
    pub on_open_change: Callback<bool>,

    /// The controlled text query used to filter options.
    #[props(default)]
    pub query: ReadSignal<Option<String>>,

    /// The initial text query when uncontrolled.
    #[props(default)]
    pub default_query: ReadSignal<String>,

    /// Callback fired when the text query changes.
    #[props(default)]
    pub on_query_change: Callback<String>,

    /// Whether arrow-key navigation should wrap.
    #[props(default = ReadSignal::new(Signal::new(true)))]
    pub roving_loop: ReadSignal<bool>,

    /// Custom filter callback. Receives `(query, option_text_value)`.
    #[props(default = Callback::new(|(q, t): (String, String)| default_combobox_filter(&q, &t)))]
    pub filter: Callback<(String, String), bool>,

    /// Additional attributes for the root element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// Children.
    pub children: Element,
}

/// # ComboboxMulti
///
/// A multi-select autocomplete input with a filterable popup list.
/// Selecting an option toggles it in or out of the selection and the
/// popup stays open across picks; it closes via Escape, clicking
/// outside, or tabbing past the listbox. For single-selection use
/// [`Combobox`] instead.
///
/// When the popup is closed the input displays the comma-joined text
/// of the selected options; while open it shows the current query.
///
/// ## Example
///
/// ```rust
/// use dioxus::prelude::*;
/// use dioxus_primitives::combobox::{
///     ComboboxInput, ComboboxItemIndicator, ComboboxList, ComboboxMulti, ComboboxOption,
/// };
/// #[component]
/// fn Demo() -> Element {
///     rsx! {
///         ComboboxMulti::<String> {
///             default_values: vec!["dioxus".into()],
///             ComboboxInput { placeholder: "Pick frameworks..." }
///             ComboboxList {
///                 aria_label: "Framework Picker",
///                 ComboboxOption::<String> {
///                     index: 0usize,
///                     value: "dioxus",
///                     "Dioxus"
///                     ComboboxItemIndicator { "✔️" }
///                 }
///                 ComboboxOption::<String> {
///                     index: 1usize,
///                     value: "leptos",
///                     "Leptos"
///                     ComboboxItemIndicator { "✔️" }
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// ## Styling
///
/// The [`ComboboxMulti`] component defines the following data attributes you can use to control styling:
/// - `data-state`: Indicates the current state of the combobox. Values are `open` or `closed`.
/// - `data-disabled`: Indicates whether the combobox is disabled. Values are `true` or `false`.
#[component]
pub fn ComboboxMulti<T: Clone + PartialEq + 'static>(props: ComboboxMultiProps<T>) -> Element {
    let (values, set_value) = use_multi_selectable_value(
        props.values,
        props.default_values,
        props.on_values_change,
        "combobox",
    );

    let open = use_combobox_root(
        values,
        set_value,
        SelectionMode::Multiple,
        props.disabled,
        props.roving_loop,
        Controlled {
            value: props.open,
            default: props.default_open,
            on_change: props.on_open_change,
        },
        Controlled {
            value: props.query,
            default: props.default_query,
            on_change: props.on_query_change,
        },
        props.filter,
    );

    rsx! {
        div {
            "data-state": if open() { "open" } else { "closed" },
            "data-disabled": (props.disabled)(),
            ..props.attributes,
            {props.children}
        }
    }
}
