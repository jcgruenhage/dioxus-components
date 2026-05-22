Allows users to create vertically sortable lists supporting drag and drop, touch or keyboard input. Multiple lists can be combined into a kanban-style board where items can be moved between columns.

## Component Structure

A single sortable list:

```rust
DragAndDropList {
    // Items to be rendered. Each element's `key:` is treated as the
    // stable identity of that row across reorders.
    items
    // Whether the list items should be removable
    is_removable
}
```

A multi-column board. Each `DragAndDropList` gets a distinct
`column_id`; the board fires `on_change` once per successful drop
with a `DragAndDropChange { item_key, from_column, to_column,
to_index }` so the consumer can persist the move. The board owns one
`DragAndDropInstructions` and one `DragAndDropLiveRegion` for the
whole group — do not render either inside the child lists.

```rust
DragAndDropBoard {
    on_change: |change: DragAndDropChange| {
        // persist the move: change.item_key has moved from
        // change.from_column to change.to_column at index change.to_index
    },
    aria_label: "Engineering board",
    DragAndDropList {
        items: todo_items,
        column_id: "todo",
        aria_label: "To do",
    }
    DragAndDropList {
        items: in_progress_items,
        column_id: "in_progress",
        aria_label: "In progress",
    }
    DragAndDropList {
        items: done_items,
        column_id: "done",
        aria_label: "Done",
    }
    // Compose these once per board, not once per list.
    DragAndDropInstructions {}
    DragAndDropLiveRegion {}
}
```

## Keyboard

| Key                                     | Action                                                                  |
| --------------------------------------- | ----------------------------------------------------------------------- |
| `Tab`                                   | Move focus into a list. In a board, Tab cycles between columns.         |
| `Arrow Up` / `Arrow Down`               | Move focus within a column. While dragging, reorder up / down.          |
| `Arrow Left` / `Arrow Right`            | While dragging in a board, move the grabbed item to the adjacent column.|
| `Enter` / `Space`                       | Lift the focused item; press again to drop it.                          |
| `Escape`                                | Cancel an in-flight drag and return the item to its starting position.  |
| `Delete` / `Backspace`                  | Remove the focused item (when the list is configured to allow it).      |
| `Home` / `End`                          | Focus the first / last item of the current column.                      |
