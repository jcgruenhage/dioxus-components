use super::super::component::*;
use dioxus::prelude::*;
use dioxus_primitives::drag_and_drop_list::{
    DragAndDropBoard, DragAndDropChange, DragAndDropInstructions, DragAndDropLiveRegion,
};

const INLINE_STYLE: &str = r#".dx-board-demo {
  width: 100%;
  max-width: 720px;
  margin: 0 auto;
}

.dx-board-header {
  margin-bottom: 14px;
}

.dx-board-title {
  margin: 0;
  color: var(--secondary-color-2);
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
}

.dx-board-subtitle {
  margin: 4px 0 0;
  color: var(--secondary-color-5);
  font-size: 12px;
  line-height: 1.4;
}

.dx-board-columns {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  align-items: start;
  gap: 12px;
}

.dx-board-column {
  display: flex;
  min-height: 220px;
  flex-direction: column;
  padding: 10px 8px 12px;
  gap: 6px;
  border: 1px solid var(--primary-color-7);
  border-radius: 8px;
  background: var(--primary-color-9);
}

.dx-board-column-title {
  margin: 0 2px 4px;
  color: var(--secondary-color-5);
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.dx-board-card {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 3px;
}

.dx-board-card-meta {
  color: var(--secondary-color-5);
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  font-size: 10.5px;
  font-weight: 500;
  letter-spacing: 0.03em;
}

.dx-board-card-title {
  overflow: hidden;
  color: var(--secondary-color-2);
  font-size: 13px;
  font-weight: 500;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}"#;

#[derive(Clone, Copy)]
struct Card {
    key: &'static str,
    title: &'static str,
}

const TODO: &[Card] = &[
    Card { key: "LNC-201", title: "Draft Q3 OKRs" },
    Card { key: "LNC-205", title: "Schedule design review" },
    Card { key: "LNC-208", title: "Spec billing webhook v2" },
];

const IN_PROGRESS: &[Card] = &[
    Card { key: "LNC-191", title: "Ship dashboard filters" },
    Card { key: "LNC-194", title: "Investigate flaky CI step" },
];

const DONE: &[Card] = &[Card { key: "LNC-180", title: "Migrate auth to OIDC" }];

fn card_item(c: Card) -> Element {
    rsx! {
        div { key: "{c.key}", class: "dx-board-card",
            span { class: "dx-board-card-meta", "{c.key}" }
            span { class: "dx-board-card-title", "{c.title}" }
        }
    }
}

fn items_of(cards: &[Card]) -> Vec<Element> {
    cards.iter().map(|c| card_item(*c)).collect()
}

fn keys_of(cards: &[Card]) -> Vec<String> {
    cards.iter().map(|c| c.key.to_string()).collect()
}

#[component]
pub fn Demo() -> Element {
    let mut todo = use_signal(|| items_of(TODO));
    let mut in_progress = use_signal(|| items_of(IN_PROGRESS));
    let mut done = use_signal(|| items_of(DONE));

    let mut todo_keys = use_signal(|| keys_of(TODO));
    let mut in_progress_keys = use_signal(|| keys_of(IN_PROGRESS));
    let mut done_keys = use_signal(|| keys_of(DONE));

    let lookup = |k: &str| -> Option<Card> {
        TODO.iter()
            .chain(IN_PROGRESS.iter())
            .chain(DONE.iter())
            .find(|c| c.key == k)
            .copied()
    };

    let on_change = move |change: DragAndDropChange| {
        let DragAndDropChange {
            item_key,
            from_column,
            to_column,
            to_index,
        } = change;
        let Some(card) = lookup(&item_key) else {
            return;
        };

        let mut remove_from = |col: &str| match col {
            "todo" => {
                let mut k = todo_keys.write();
                if let Some(i) = k.iter().position(|x| x == &item_key) {
                    k.remove(i);
                    let _ = todo.write().remove(i);
                }
            }
            "in_progress" => {
                let mut k = in_progress_keys.write();
                if let Some(i) = k.iter().position(|x| x == &item_key) {
                    k.remove(i);
                    let _ = in_progress.write().remove(i);
                }
            }
            "done" => {
                let mut k = done_keys.write();
                if let Some(i) = k.iter().position(|x| x == &item_key) {
                    k.remove(i);
                    let _ = done.write().remove(i);
                }
            }
            _ => {}
        };

        remove_from(&from_column);

        let new_el = card_item(card);
        match to_column.as_str() {
            "todo" => {
                let mut k = todo_keys.write();
                let mut e = todo.write();
                let at = to_index.min(k.len());
                k.insert(at, item_key);
                e.insert(at, new_el);
            }
            "in_progress" => {
                let mut k = in_progress_keys.write();
                let mut e = in_progress.write();
                let at = to_index.min(k.len());
                k.insert(at, item_key);
                e.insert(at, new_el);
            }
            "done" => {
                let mut k = done_keys.write();
                let mut e = done.write();
                let at = to_index.min(k.len());
                k.insert(at, item_key);
                e.insert(at, new_el);
            }
            _ => {}
        }
    };

    rsx! {
        style { {INLINE_STYLE} }
        div { class: "dx-board-demo",
            div { class: "dx-board-header",
                h3 { class: "dx-board-title", "Engineering board" }
                p { class: "dx-board-subtitle",
                    "Drag cards across columns - within a column too. While reordering with the keyboard, use the left and right Arrow keys to move between columns."
                }
            }
            DragAndDropBoard {
                on_change: on_change,
                aria_label: "Engineering board",
                div { class: "dx-board-columns",
                    div { class: "dx-board-column",
                        h4 { class: "dx-board-column-title", "To do" }
                        DragAndDropList {
                            items: todo(),
                            column_id: "todo".to_string(),
                            aria_label: "To do".to_string(),
                        }
                    }
                    div { class: "dx-board-column",
                        h4 { class: "dx-board-column-title", "In progress" }
                        DragAndDropList {
                            items: in_progress(),
                            column_id: "in_progress".to_string(),
                            aria_label: "In progress".to_string(),
                        }
                    }
                    div { class: "dx-board-column",
                        h4 { class: "dx-board-column-title", "Done" }
                        DragAndDropList {
                            items: done(),
                            column_id: "done".to_string(),
                            aria_label: "Done".to_string(),
                        }
                    }
                }
                DragAndDropInstructions {}
                DragAndDropLiveRegion {}
            }
        }
    }
}
