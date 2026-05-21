use super::super::component::*;
use dioxus::prelude::*;

#[component]
pub fn Demo() -> Element {
    let mut query = use_signal(String::new);
    let frameworks: &[(&str, &str)] = &[
        ("next", "Next.js"),
        ("svelte", "SvelteKit"),
        ("nuxt", "Nuxt.js"),
        ("remix", "Remix"),
        ("astro", "Astro"),
        ("solid", "SolidStart"),
        ("dioxus", "Dioxus"),
    ];

    rsx! {
        ComboboxMulti::<String> {
            query: Some(query()),
            on_query_change: move |next| query.set(next),
            default_values: vec!["dioxus".to_string(), "solid".to_string()],
            placeholder: "Select frameworks...",
            aria_label: "Select frameworks",
            list_aria_label: "Frameworks",
            ComboboxEmpty { "No framework found." }
            for (i , (value , label)) in frameworks.iter().enumerate() {
                ComboboxOption::<String> {
                    index: i,
                    value: value.to_string(),
                    text_value: label.to_string(),
                    {*label}
                }
            }
        }
    }
}
