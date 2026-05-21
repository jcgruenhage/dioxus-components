The Combobox component is an autocomplete input with a filterable popup list.

Filtering preserves the order defined by the rendered `ComboboxOption` elements and their `index`
props. If you want query-dependent ranking, control `query`, sort your item data in user code,
render the options in that sorted order, and assign indexes from the sorted list.

## Component Structure

```rust
let mut value = use_signal(|| None::<String>);
let mut query = use_signal(String::new);

Combobox::<String> {
    value: Some(value.into()),
    on_value_change: move |next: Option<String>| {
        value.set(next);
    },
    query: Some(query()),
    on_query_change: move |next| query.set(next),
    placeholder: "Select framework...",
    aria_label: "Select framework",
    list_aria_label: "Frameworks",
    ComboboxEmpty { "No framework found." }
    ComboboxOption::<String> {
        index: 0usize,
        value: "next".to_string(),
        text_value: "Next.js",
        "Next.js"
    }
}
```

## Multi-select

`ComboboxMulti` toggles options in and out of a `Vec<T>` and keeps the popup
open across picks. When the popup is closed the input displays the comma-joined
text of the selected options.

```rust
let mut values = use_signal(|| Some(Vec::<String>::new()));
let mut query = use_signal(String::new);

ComboboxMulti::<String> {
    values: Some(values.into()),
    on_values_change: move |next: Vec<String>| {
        values.set(Some(next));
    },
    query: Some(query()),
    on_query_change: move |next| query.set(next),
    placeholder: "Select frameworks...",
    aria_label: "Select frameworks",
    list_aria_label: "Frameworks",
    ComboboxEmpty { "No framework found." }
    ComboboxOption::<String> {
        index: 0usize,
        value: "next".to_string(),
        text_value: "Next.js",
        "Next.js"
    }
}
```
