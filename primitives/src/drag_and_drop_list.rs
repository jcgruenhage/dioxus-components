//! Defines the [`DragAndDropList`] component and its sub-components,
//! plus the [`DragAndDropBoard`] multi-column orchestrator.
//!
//! Mount a single [`DragAndDropList`] for in-list reordering, or wrap
//! multiple lists in a [`DragAndDropBoard`] to enable cross-column
//! drag. The single-list public API is unchanged from earlier
//! releases — [`DragAndDropContext`], [`DragAndDropItemContext`],
//! [`use_drag_and_drop_list_items`], the component set, etc., all
//! keep their existing shapes. [`DragAndDropBoard`] is additive: each
//! nested list registers its `column_id` with the board, and drops
//! route through `on_change` as a [`DragAndDropChange`] so the
//! consumer can persist the move.

use crate::use_unique_id;
use dioxus::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;

/// Sentinel column id used when a [`DragAndDropList`] is mounted
/// without an explicit `column_id` (i.e. the original single-list
/// API). Chosen to be unlikely to collide with a user-supplied id.
const DEFAULT_COLUMN_ID: &str = "__dnd_default__";

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum DropPosition {
    Before,
    Undefined,
    After,
}

impl From<std::cmp::Ordering> for DropPosition {
    fn from(ord: std::cmp::Ordering) -> Self {
        match ord {
            std::cmp::Ordering::Less => Self::Before,
            std::cmp::Ordering::Equal => Self::Undefined,
            std::cmp::Ordering::Greater => Self::After,
        }
    }
}

/// Resolves the final insertion index from a hovered item and pointer
/// position, accounting for the source item leaving its current slot.
fn resolve_drop_index(from: usize, hovered: usize, position: DropPosition) -> usize {
    let slot = match position {
        DropPosition::Before | DropPosition::Undefined => hovered,
        DropPosition::After => hovered + 1,
    };
    if from < slot {
        slot - 1
    } else {
        slot
    }
}

/// Resolves whether the final insertion index is before or after the source item.
fn resolve_drop_position(from: usize, to: usize) -> DropPosition {
    to.cmp(&from).into()
}

/// `(column_id, index)` location of an item inside a board.
#[derive(Clone, PartialEq, Debug)]
struct Loc {
    col: String,
    idx: usize,
}

#[derive(Clone, PartialEq, Debug)]
enum DragState {
    Idle,
    Dragging {
        from: Loc,
        to: Option<Loc>,
        position: DropPosition,
    },
    Dropped {
        from: Loc,
        to: Loc,
    },
}

/// Event payload fired by [`DragAndDropBoard`] when a drop completes.
/// Identifies the moved item by its stable `key:`, where it came
/// from, and where it landed.
#[derive(Clone, PartialEq, Debug)]
pub struct DragAndDropChange {
    /// Stable `key` of the item that moved (taken from the list-item's
    /// root element `key:`).
    pub item_key: String,
    /// Column the item left.
    pub from_column: String,
    /// Column the item entered. Equal to `from_column` on a within-list
    /// reorder.
    pub to_column: String,
    /// Insertion index inside `to_column` after the move.
    pub to_index: usize,
}

/// Boxed callback shape stored inside the board context. Aliased to
/// keep clippy's complex-type lint happy at usage sites.
type OnChangeFn = Rc<dyn Fn(DragAndDropChange)>;

/// One registered column's mutable storage. The signals are owned by
/// the `DragAndDropList` that registered them, but the board holds
/// `Copy`-able references so cross-column drag can read and write
/// either side from the same place. `label` is the human-readable
/// name (the list's `aria_label`) used in screen-reader
/// announcements; falls back to the column id when the list didn't
/// supply one.
#[derive(Clone, Copy)]
struct RegisteredColumn {
    items: Signal<Vec<Element>>,
    keys: Signal<Vec<String>>,
    label: Signal<Option<String>>,
}

/// Internal multi-column drag state shared between every
/// [`DragAndDropList`] inside the same [`DragAndDropBoard`] (or, in
/// the single-list case, a per-list fallback board provided by the
/// list itself).
#[derive(Clone, Copy)]
struct BoardContext {
    drag: Signal<DragState>,
    columns: Signal<HashMap<String, RegisteredColumn>>,
    column_order: Signal<Vec<String>>,
    focused: Signal<Option<Loc>>,
    /// Per-column roving tab entry: the index that becomes the tab
    /// stop for each column. Defaults to 0 when a column hasn't been
    /// interacted with yet, and follows the most recently focused
    /// item in that column thereafter. Lets keyboard users Tab
    /// between columns and pick up where they left off in each.
    tab_roving: Signal<HashMap<String, usize>>,
    announcement: Signal<String>,
    on_change: Signal<Option<OnChangeFn>>,
    /// Element id for the screen-reader instructions block. Per-board
    /// (or per-standalone-list) so multiple lists/boards on the same
    /// page don't collide.
    instructions_id: Signal<String>,
}

impl BoardContext {
    /// Idempotently insert or update a column's storage. Called from
    /// `use_hook` on first mount of each list. Dioxus 0.7 fires
    /// sibling `use_hook` initializers in reverse source order
    /// (last-rendered child first), so we *prepend* to `column_order`
    /// rather than appending: that way the order reflected to
    /// cross-column navigation matches the source-declared layout
    /// (`ArrowRight` moves from earlier columns to later ones).
    fn sync_column(&mut self, id: &str, col: RegisteredColumn) {
        self.columns.with_mut(|map| {
            map.insert(id.to_string(), col);
        });
        self.column_order.with_mut(|order| {
            if !order.iter().any(|c| c == id) {
                order.insert(0, id.to_string());
            }
        });
    }

    fn column(&self, id: &str) -> Option<RegisteredColumn> {
        self.columns.with(|map| map.get(id).copied())
    }

    fn column_len(&self, id: &str) -> usize {
        self.column(id).map(|c| (c.items)().len()).unwrap_or(0)
    }

    fn column_items(&self, id: &str) -> Vec<Element> {
        self.column(id).map(|c| (c.items)()).unwrap_or_default()
    }

    fn column_keys(&self, id: &str) -> Vec<String> {
        self.column(id).map(|c| (c.keys)()).unwrap_or_default()
    }

    fn column_label(&self, id: &str) -> String {
        self.column(id)
            .and_then(|c| (c.label)())
            .unwrap_or_else(|| id.to_string())
    }

    fn drag_from(&self) -> Option<Loc> {
        match (self.drag)() {
            DragState::Idle => None,
            DragState::Dragging { from, .. } | DragState::Dropped { from, .. } => Some(from),
        }
    }

    fn drop_to(&self) -> Option<Loc> {
        match (self.drag)() {
            DragState::Idle => None,
            DragState::Dragging { to, .. } => to,
            DragState::Dropped { to, .. } => Some(to),
        }
    }

    fn drop_position(&self) -> DropPosition {
        match (self.drag)() {
            DragState::Dragging { position, .. } => position,
            _ => DropPosition::Undefined,
        }
    }

    fn is_dragging(&self) -> bool {
        !matches!((self.drag)(), DragState::Idle)
    }

    fn start_drag(&mut self, loc: Loc) {
        self.drag.set(DragState::Dragging {
            from: loc,
            to: None,
            position: DropPosition::Undefined,
        });
    }

    fn end_drag(&mut self) {
        let focus_target = self.drop_to().or(self.drag_from());
        self.focused.set(focus_target);
        self.drag.set(DragState::Idle);
    }

    fn cancel_drag(&mut self) {
        self.focused.set(self.drag_from());
        self.drag.set(DragState::Idle);
    }

    /// Update the drag target while the pointer is over a specific
    /// item. `hovered_col` is the column id of the list under the
    /// pointer, `hovered_idx` the item's index, and `position` the
    /// Before/After/Undefined bias resolved by the item's hover
    /// handler. Within-column moves apply the upstream
    /// `resolve_drop_index` shift; cross-column moves use
    /// `hovered_idx` directly (with After biasing by +1).
    fn drag_over(&mut self, hovered_col: &str, hovered_idx: usize, position: DropPosition) {
        let DragState::Dragging { from, .. } = (self.drag)() else {
            return;
        };
        let (to, pos) = if from.col == hovered_col {
            let resolved = resolve_drop_index(from.idx, hovered_idx, position);
            let p = resolve_drop_position(from.idx, resolved);
            (
                Loc {
                    col: hovered_col.to_string(),
                    idx: resolved,
                },
                p,
            )
        } else {
            let slot = match position {
                DropPosition::Before | DropPosition::Undefined => hovered_idx,
                DropPosition::After => hovered_idx + 1,
            };
            (
                Loc {
                    col: hovered_col.to_string(),
                    idx: slot,
                },
                position,
            )
        };
        self.drag.set(DragState::Dragging {
            from,
            to: Some(to),
            position: pos,
        });
    }

    /// Update the drag target to the end of `target_col` — used when
    /// the pointer is over the bare list area (between items or past
    /// the last item) so dragging into the tail of a column "just
    /// works."
    fn drag_over_column_tail(&mut self, target_col: &str) {
        let DragState::Dragging { from, .. } = (self.drag)() else {
            return;
        };
        let len = self.column_len(target_col);
        let idx = if from.col == target_col {
            // Same-column: after the remove-then-insert the highest
            // meaningful index is `len - 1`. We use `saturating_sub`
            // defensively — the invariant says the source column is
            // non-empty during its own drag, but a misbehaving caller
            // shouldn't underflow.
            len.saturating_sub(1)
        } else {
            len
        };
        self.drag.set(DragState::Dragging {
            from,
            to: Some(Loc {
                col: target_col.to_string(),
                idx,
            }),
            position: DropPosition::Undefined,
        });
    }

    fn drop(&mut self) {
        let DragState::Dragging {
            from, to: Some(to), ..
        } = (self.drag)()
        else {
            return;
        };
        if from == to {
            self.drag.set(DragState::Dropped { from, to });
            return;
        }
        let Some(from_col) = self.column(&from.col) else {
            return;
        };
        let mut from_items = (from_col.items)();
        let mut from_keys = (from_col.keys)();
        if from.idx >= from_items.len() {
            return;
        }
        let element = from_items.remove(from.idx);
        let key = from_keys.remove(from.idx);

        if from.col == to.col {
            let insert_at = to.idx.min(from_items.len());
            from_items.insert(insert_at, element);
            from_keys.insert(insert_at, key.clone());
            let mut items_sig = from_col.items;
            let mut keys_sig = from_col.keys;
            items_sig.set(from_items);
            keys_sig.set(from_keys);
        } else {
            let mut items_sig = from_col.items;
            let mut keys_sig = from_col.keys;
            items_sig.set(from_items);
            keys_sig.set(from_keys);

            let Some(to_col) = self.column(&to.col) else {
                return;
            };
            let mut to_items = (to_col.items)();
            let mut to_keys = (to_col.keys)();
            let insert_at = to.idx.min(to_items.len());
            to_items.insert(insert_at, element);
            to_keys.insert(insert_at, key.clone());
            let mut to_items_sig = to_col.items;
            let mut to_keys_sig = to_col.keys;
            to_items_sig.set(to_items);
            to_keys_sig.set(to_keys);
        }

        if let Some(cb) = (self.on_change)().clone() {
            cb(DragAndDropChange {
                item_key: key,
                from_column: from.col.clone(),
                to_column: to.col.clone(),
                to_index: to.idx,
            });
        }

        self.drag.set(DragState::Dropped { from, to });
    }

    fn remove_at(&mut self, col_id: &str, index: usize) {
        let Some(col) = self.column(col_id) else {
            return;
        };
        let mut items = (col.items)();
        let mut keys = (col.keys)();
        if index >= items.len() {
            return;
        }
        let _ = items.remove(index);
        let _ = keys.remove(index);
        let new_len = items.len();
        let mut items_sig = col.items;
        let mut keys_sig = col.keys;
        items_sig.set(items);
        keys_sig.set(keys);
        self.focused.set(new_len.checked_sub(1).map(|last| Loc {
            col: col_id.to_string(),
            idx: index.min(last),
        }));
        self.announcement.set(format!(
            "Removed item from position {}. {} items remaining",
            index + 1,
            new_len
        ));
    }

    fn announce(&mut self, msg: String) {
        self.announcement.set(msg);
    }

    fn is_focused(&self, loc: &Loc) -> bool {
        (self.focused)().as_ref().is_some_and(|f| f == loc)
    }

    fn set_focus(&mut self, loc: Option<Loc>) {
        if let Some(ref l) = loc {
            self.tab_roving.with_mut(|m| {
                m.insert(l.col.clone(), l.idx);
            });
        }
        self.focused.set(loc);
    }

    fn tab_roving_for(&self, col: &str) -> usize {
        (self.tab_roving)().get(col).copied().unwrap_or(0)
    }

    fn focus_next_in(&mut self, col: &str) {
        let Some(current) = (self.focused)() else {
            return;
        };
        if current.col != col {
            return;
        }
        let len = self.column_len(col);
        if len == 0 {
            return;
        }
        self.focused.set(Some(Loc {
            col: col.to_string(),
            idx: (current.idx + 1) % len,
        }));
    }

    fn focus_prev_in(&mut self, col: &str) {
        let Some(current) = (self.focused)() else {
            return;
        };
        if current.col != col {
            return;
        }
        let len = self.column_len(col);
        if len == 0 {
            return;
        }
        self.focused.set(Some(Loc {
            col: col.to_string(),
            idx: current.idx.checked_sub(1).unwrap_or(len - 1),
        }));
    }

    fn move_up(&mut self, here: Loc) {
        let DragState::Dragging { from, to, .. } = (self.drag)() else {
            return;
        };
        let current = to.unwrap_or(here);
        if current.col != from.col {
            // Keyboard reorder is constrained to the source column.
            return;
        }
        let len = self.column_len(&current.col);
        if len == 0 {
            return;
        }
        let new_idx = current.idx.checked_sub(1).unwrap_or(len - 1);
        let new_to = Loc {
            col: current.col,
            idx: new_idx,
        };
        let pos = resolve_drop_position(from.idx, new_to.idx);
        self.drag.set(DragState::Dragging {
            from,
            to: Some(new_to),
            position: pos,
        });
    }

    /// Cross-column keyboard move during an active drag. `direction`
    /// is `-1` to move to the previous column, `+1` to move to the
    /// next. The target lands at the end of the new column (or at
    /// the source's original slot if the new column happens to be
    /// the source). No-op if there isn't a column in that direction
    /// or no drag is in flight.
    fn move_to_neighbor_column(&mut self, direction: i32) {
        let DragState::Dragging { from, to, .. } = (self.drag)() else {
            return;
        };
        let current = to.clone().unwrap_or_else(|| from.clone());
        let order = (self.column_order)();
        let Some(pos) = order.iter().position(|c| c == &current.col) else {
            return;
        };
        let new_pos = (pos as i32) + direction;
        if new_pos < 0 || new_pos >= order.len() as i32 {
            return;
        }
        let target_col = order[new_pos as usize].clone();
        let target_len = self.column_len(&target_col);
        let idx = if target_col == from.col {
            // Re-targeting the source's own column: snap back to
            // the slot the item came from.
            from.idx
        } else {
            // Land at the tail of the new column. Drop's clamp
            // handles bounds.
            target_len
        };
        self.drag.set(DragState::Dragging {
            from,
            to: Some(Loc {
                col: target_col,
                idx,
            }),
            position: DropPosition::Undefined,
        });
    }

    fn move_down(&mut self, here: Loc) {
        let DragState::Dragging { from, to, .. } = (self.drag)() else {
            return;
        };
        let current = to.unwrap_or(here);
        if current.col != from.col {
            return;
        }
        let len = self.column_len(&current.col);
        if len == 0 {
            return;
        }
        let new_idx = (current.idx + 1) % len;
        let new_to = Loc {
            col: current.col,
            idx: new_idx,
        };
        let pos = resolve_drop_position(from.idx, new_to.idx);
        self.drag.set(DragState::Dragging {
            from,
            to: Some(new_to),
            position: pos,
        });
    }

    /// True if more than one column has registered with this board —
    /// used to decide whether announcements should name the column.
    fn is_multi_column(&self) -> bool {
        (self.column_order)().len() > 1
    }

    fn announce_move(&mut self, here: Loc) {
        let from_col = self.drag_from().map(|l| l.col);
        let to = self.drop_to().unwrap_or(here);
        let pos = to.idx + 1;
        // The drop hasn't happened yet, so `column_len(to.col)` is the
        // target column's pre-move length. For a cross-column hover the
        // item will land *in addition* to those, so bump count by one.
        let count = self.column_len(&to.col)
            + if from_col.as_deref() == Some(&to.col) {
                0
            } else {
                1
            };
        let msg = if self.is_multi_column() {
            format!(
                "You have moved the item to position {pos} of {count} in {col}",
                col = self.column_label(&to.col),
            )
        } else {
            format!("You have moved the item to position {pos} of {count}")
        };
        self.announcement.set(msg);
    }

    fn toggle_drag(&mut self, here: Loc) {
        if self.is_dragging() {
            let from = self.drag_from().unwrap_or(here.clone());
            let to = self.drop_to().unwrap_or(here);
            let multi = self.is_multi_column();
            self.drop();
            self.end_drag();
            let from_label = self.column_label(&from.col);
            let to_label = self.column_label(&to.col);
            let msg = if multi {
                format!(
                    "You have dropped the item. It has moved from position {} in {} to position {} in {}.",
                    from.idx + 1,
                    from_label,
                    to.idx + 1,
                    to_label,
                )
            } else {
                format!(
                    "You have dropped the item. It has moved from position {} to position {}",
                    from.idx + 1,
                    to.idx + 1,
                )
            };
            self.announcement.set(msg);
        } else {
            let count = self.column_len(&here.col);
            let here_label = here.clone();
            self.start_drag(here.clone());
            self.drag_over(&here.col, here.idx, DropPosition::Undefined);
            let col_label = self.column_label(&here_label.col);
            let msg = if self.is_multi_column() {
                format!(
                    "You have lifted an item in position {} of {count} in {col_label}",
                    here_label.idx + 1,
                )
            } else {
                format!(
                    "You have lifted an item in position {} of {count}",
                    here_label.idx + 1,
                )
            };
            self.announcement.set(msg);
        }
    }
}

/// Context provided by [`DragAndDropList`] to its descendants.
/// Use `use_context::<DragAndDropContext>()` to access list-level
/// operations. The shape and methods on this type are unchanged from
/// the original single-list API even when the list is nested inside
/// a [`DragAndDropBoard`].
#[derive(Clone, Copy)]
pub struct DragAndDropContext {
    board: BoardContext,
    column_id: Signal<String>,
}

impl DragAndDropContext {
    fn col(&self) -> String {
        (self.column_id)()
    }

    fn drag_from(&self) -> Option<usize> {
        let col = self.col();
        self.board
            .drag_from()
            .filter(|l| l.col == col)
            .map(|l| l.idx)
    }

    fn drop_to(&self) -> Option<usize> {
        let col = self.col();
        self.board.drop_to().filter(|l| l.col == col).map(|l| l.idx)
    }

    fn drop_position(&self) -> DropPosition {
        self.board.drop_position()
    }

    fn is_dragging(&self) -> bool {
        self.board.is_dragging()
    }

    fn start_drag(&mut self, index: usize) {
        let col = self.col();
        self.board.start_drag(Loc { col, idx: index });
    }

    fn end_drag(&mut self) {
        self.board.end_drag();
    }

    fn cancel_drag(&mut self) {
        self.board.cancel_drag();
    }

    fn drag_over(&mut self, hovered: usize, position: DropPosition) {
        let col = self.col();
        self.board.drag_over(&col, hovered, position);
    }

    fn drag_over_column_tail(&mut self) {
        let col = self.col();
        self.board.drag_over_column_tail(&col);
    }

    fn drop(&mut self) {
        self.board.drop();
    }

    /// Remove the item at the given index from this list.
    pub fn remove(&mut self, index: usize) {
        let col = self.col();
        self.board.remove_at(&col, index);
    }

    fn announce(&mut self, msg: String) {
        self.board.announce(msg);
    }

    fn item_count(&self) -> usize {
        let col = self.col();
        self.board.column_len(&col)
    }

    fn items(&self) -> Vec<Element> {
        let col = self.col();
        self.board.column_items(&col)
    }

    fn keys(&self) -> Vec<String> {
        let col = self.col();
        self.board.column_keys(&col)
    }

    fn is_focused(&self, index: usize) -> bool {
        let col = self.col();
        self.board.is_focused(&Loc { col, idx: index })
    }

    fn set_focus(&mut self, id: Option<usize>) {
        let col = self.col();
        self.board.set_focus(id.map(|idx| Loc { col, idx }));
    }

    fn focus_next(&mut self) {
        let col = self.col();
        self.board.focus_next_in(&col);
    }

    fn focus_prev(&mut self) {
        let col = self.col();
        self.board.focus_prev_in(&col);
    }

    fn move_up(&mut self, index: usize) {
        let col = self.col();
        self.board.move_up(Loc { col, idx: index });
    }

    fn move_down(&mut self, index: usize) {
        let col = self.col();
        self.board.move_down(Loc { col, idx: index });
    }

    fn move_prev_column(&mut self) {
        self.board.move_to_neighbor_column(-1);
    }

    fn move_next_column(&mut self) {
        self.board.move_to_neighbor_column(1);
    }

    fn announce_move(&mut self, index: usize) {
        let col = self.col();
        self.board.announce_move(Loc { col, idx: index });
    }

    fn toggle_drag(&mut self, index: usize) {
        let col = self.col();
        self.board.toggle_drag(Loc { col, idx: index });
    }

    /// Returns the index that is the current Tab stop for this
    /// list's column (roving focus). Defaults to 0 before any
    /// interaction; tracks the last-focused item thereafter so Tab
    /// returns to where the user left off. Clamped to the current
    /// column length so a stale roving index from a since-shrunk
    /// column doesn't point past the end. Returns `None` when the
    /// column is empty (no item is tab-reachable).
    fn tab_roving_idx(&self) -> Option<usize> {
        let col = self.col();
        let len = self.board.column_len(&col);
        if len == 0 {
            return None;
        }
        Some(self.board.tab_roving_for(&col).min(len - 1))
    }
}

/// Context provided by [`DragAndDropListItem`] to its children.
/// Use `use_context::<DragAndDropItemContext>()` to access the current
/// item's index.
#[derive(Clone, Copy)]
pub struct DragAndDropItemContext {
    index: Signal<usize>,
}

impl DragAndDropItemContext {
    /// Returns the index of the current item in the list.
    pub fn index(&self) -> usize {
        (self.index)()
    }
}

/// The props for the [`DragAndDropList`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropListProps {
    /// Items (labels) to be rendered. Each item's `key:` (if any) is
    /// used as the stable identity for that row.
    pub items: Vec<Element>,

    /// Column id within the parent [`DragAndDropBoard`]. Defaults to
    /// a sentinel string so single-list callers don't need to supply
    /// one.
    #[props(default = DEFAULT_COLUMN_ID.to_string())]
    pub column_id: String,

    /// Accessible label for the list.
    #[props(default)]
    pub aria_label: Option<String>,

    /// Fires once per successful drop with the resulting move so the
    /// caller can persist the new order. Ignored when this list is
    /// nested inside a [`DragAndDropBoard`] — the board's
    /// `on_change` is authoritative in that case so drops route once.
    /// Standalone callers must wire this up: `items` is resynced from
    /// props on every render, so an un-persisted drop will revert on
    /// the next parent re-render.
    #[props(default)]
    pub on_change: Callback<DragAndDropChange>,

    /// Additional attributes to apply to the list element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the list component.
    #[props(default)]
    pub children: Option<Element>,
}

/// The props for the [`DragAndDropListItems`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropListItemsProps {
    /// Accessible label for the list.
    pub aria_label: String,

    /// Additional attributes to apply to the inner list element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the inner list element.
    #[props(default)]
    pub children: Option<Element>,
}

/// The props for the [`DragAndDropInstructions`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropInstructionsProps {
    /// Additional attributes to apply to the instructions element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
}

/// The props for the [`DragAndDropLiveRegion`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropLiveRegionProps {
    /// Additional attributes to apply to the live region element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
}

/// # DragAndDropList
///
/// A list can be used to display content related to a single subject.
/// The content can consist of multiple elements of varying type and size.
/// Used when a user wants to change a collection order.
///
/// `items` is resynced from props every render so the caller owns the
/// ordering. Standalone lists should hold the items in a [`Signal`] and
/// update it from [`DragAndDropListProps::on_change`]; without that
/// wiring, an unrelated parent re-render will revert the most recent
/// drop. Lists nested in a [`DragAndDropBoard`] route their changes
/// through the board's `on_change` instead.
///
/// ## Example
///
/// ```rust
///use dioxus::prelude::*;
///use dioxus_primitives::drag_and_drop_list::{DragAndDropList, DragAndDropChange};
///#[component]
///pub fn Demo() -> Element {
///    let mut items = use_signal(|| {
///        ["Item1", "Item2", "Item3"]
///            .map(|t| rsx! { div { key: "{t}", "{t}" } })
///            .to_vec()
///    });
///    let mut keys = use_signal(|| vec!["Item1".to_string(), "Item2".to_string(), "Item3".to_string()]);
///    rsx! {
///        DragAndDropList {
///            items: items(),
///            on_change: move |change: DragAndDropChange| {
///                let mut k = keys.write();
///                let mut e = items.write();
///                if let Some(i) = k.iter().position(|x| x == &change.item_key) {
///                    let key = k.remove(i);
///                    let el = e.remove(i);
///                    let at = change.to_index.min(k.len());
///                    k.insert(at, key);
///                    e.insert(at, el);
///                }
///            },
///        }
///    }
///}
/// ```
#[component]
pub fn DragAndDropList(props: DragAndDropListProps) -> Element {
    // Stable column id for this list. Held in a Signal so
    // DragAndDropContext can stay Copy.
    let column_id_init = props.column_id.clone();
    let column_id_sig = use_signal(|| column_id_init.clone());

    // Compute keys parallel to items so the board can shuffle them
    // alongside the rendered elements during drag-induced reorders.
    let keys: Vec<String> = props
        .items
        .iter()
        .enumerate()
        .map(|(idx, el)| {
            el.as_ref()
                .ok()
                .and_then(|v| v.key.clone())
                .unwrap_or_else(|| idx.to_string())
        })
        .collect();

    // Per-column item / key signals. Owned by this list, registered
    // with the surrounding board so cross-column drag can read and
    // write them.
    let items_sig: Signal<Vec<Element>> = use_signal(|| props.items.clone());
    let keys_sig: Signal<Vec<String>> = use_signal(|| keys.clone());
    let label_sig: Signal<Option<String>> = use_signal(|| props.aria_label.clone());

    // Always refresh `items_sig` from props. The drop logic mutates
    // it locally to reflect a cross-column move, so after a drop the
    // primitive's `items_sig` and the parent's freshly-rendered items
    // share the same keys — and the previous "only sync when keys
    // changed" guard would then keep the pre-drop `Element` (with
    // pre-drop props like `archived = false`) forever. `keys_sig` is
    // only rewritten when the key set actually changes, to avoid an
    // identity-only signal churn. Mid-drag the parent has no reason
    // to re-render, so the unconditional items sync doesn't fight
    // the drag-preview bookkeeping.
    {
        let mut items_sig = items_sig;
        let mut keys_sig = keys_sig;
        items_sig.set(props.items.clone());
        if *keys_sig.peek() != keys {
            keys_sig.set(keys.clone());
        }
    }

    // Inherit a board context if there is one (the list is nested in
    // a [`DragAndDropBoard`]); otherwise create a private one. The
    // fallback signals are allocated unconditionally so hook order
    // stays stable across renders; the actual decision happens inside
    // `use_hook` below.
    let fb_drag = use_signal(|| DragState::Idle);
    let fb_columns: Signal<HashMap<String, RegisteredColumn>> = use_signal(HashMap::new);
    let fb_column_order: Signal<Vec<String>> = use_signal(Vec::new);
    let fb_focused: Signal<Option<Loc>> = use_signal(|| None);
    let fb_tab_roving: Signal<HashMap<String, usize>> = use_signal(HashMap::new);
    let fb_announcement = use_signal(String::new);
    let on_change_callback = props.on_change;
    let fb_on_change: Signal<Option<OnChangeFn>> = use_signal(|| {
        Some(Rc::new(move |change: DragAndDropChange| {
            on_change_callback.call(change);
        }) as OnChangeFn)
    });
    let fb_instructions_id = use_unique_id();

    let parent_board = try_consume_context::<BoardContext>();
    let has_parent_board = parent_board.is_some();
    let board = use_hook(|| match parent_board {
        Some(b) => b,
        None => {
            let ctx = BoardContext {
                drag: fb_drag,
                columns: fb_columns,
                column_order: fb_column_order,
                focused: fb_focused,
                tab_roving: fb_tab_roving,
                announcement: fb_announcement,
                on_change: fb_on_change,
                instructions_id: fb_instructions_id,
            };
            provide_context(ctx);
            ctx
        }
    });

    // Register this list's column with the board on first mount.
    // Using `use_hook` (rather than `use_effect`) so the registration
    // runs synchronously in render order; `use_effect` fires in
    // reverse allocation order across sibling components, which would
    // put the last-rendered column at position 0 in `column_order`
    // and break cross-column keyboard navigation. The stored
    // `RegisteredColumn` holds Signal handles, not values, so we
    // don't need to re-register when the underlying data changes.
    {
        let mut board = board;
        let column_id_for_hook = (column_id_sig)();
        use_hook(|| {
            board.sync_column(
                &column_id_for_hook,
                RegisteredColumn {
                    items: items_sig,
                    keys: keys_sig,
                    label: label_sig,
                },
            );
        });
    }

    // Provide per-list context. `use_context_provider` is the right
    // hook here because this is stable across renders for this list.
    use_context_provider(|| DragAndDropContext {
        board,
        column_id: column_id_sig,
    });

    let label = props
        .aria_label
        .as_deref()
        .unwrap_or("Sortable list")
        .to_string();

    // When this list is nested in a [`DragAndDropBoard`], the
    // surrounding board owns the instructions block and live region
    // (so screen readers don't see duplicates). The default-children
    // branch skips them in that case.
    let children = props.children.unwrap_or_else(|| {
        if has_parent_board {
            rsx! {
                DragAndDropListItems { aria_label: label }
            }
        } else {
            rsx! {
                DragAndDropInstructions {}
                DragAndDropListItems { aria_label: label }
                DragAndDropLiveRegion {}
            }
        }
    });

    rsx! {
        div {
            ..props.attributes,
            {children}
        }
    }
}

/// Returns true if the calling component is nested inside a
/// [`DragAndDropBoard`]. Useful for layers that compose
/// [`DragAndDropList`] and want to avoid double-rendering
/// [`DragAndDropInstructions`] / [`DragAndDropLiveRegion`] (the board
/// owns those, one per board).
pub fn in_drag_and_drop_board() -> bool {
    try_consume_context::<BoardContext>().is_some()
}

/// Return render data for the current sortable items in the
/// surrounding [`DragAndDropList`].
pub fn use_drag_and_drop_list_items() -> Vec<DragAndDropListRenderItem> {
    let ctx: DragAndDropContext = use_context();
    let items = ctx.items();
    let keys = ctx.keys();
    items
        .into_iter()
        .enumerate()
        .map(|(index, children)| {
            let key = keys.get(index).cloned().unwrap_or_else(|| {
                children
                    .as_ref()
                    .ok()
                    .and_then(|vnode| vnode.key.clone())
                    .unwrap_or_else(|| index.to_string())
            });
            DragAndDropListRenderItem {
                index,
                key,
                children,
            }
        })
        .collect()
}

/// The inner list element for sortable items.
#[component]
pub fn DragAndDropListItems(props: DragAndDropListItemsProps) -> Element {
    let mut ctx: DragAndDropContext = use_context();
    let describedby = (ctx.board.instructions_id)();

    let children = props.children.unwrap_or_else(|| {
        rsx! {
            for item in use_drag_and_drop_list_items() {
                Fragment {
                    key: "{item.key}",
                    DragAndDropDropIndicator {
                        index: item.index,
                        position: "before",
                    }
                    DragAndDropListItem {
                        index: item.index,
                        {item.children}
                    }
                    DragAndDropDropIndicator {
                        index: item.index,
                        position: "after",
                    }
                }
            }
        }
    });

    rsx! {
        ul {
            aria_label: "{props.aria_label}",
            aria_roledescription: "sortable list",
            aria_describedby: "{describedby}",
            ondragover: move |event: Event<DragData>| {
                // Fires only when the pointer is over the bare list
                // area (children stop propagation so the per-item
                // target wins for hovers over a row). Set the drop
                // target to the end of this column so dragging into
                // the tail "just works."
                event.prevent_default();
                event.data_transfer().set_drop_effect("move");
                ctx.drag_over_column_tail();
            },
            ondrop: move |event: Event<DragData>| {
                event.prevent_default();
                ctx.drop();
            },
            ..props.attributes,
            {children}
        }
    }
}

/// Screen-reader instructions for keyboard sorting. Text varies
/// depending on whether the calling site is inside a
/// [`DragAndDropBoard`] (mentions cross-column arrow keys) or a
/// standalone [`DragAndDropList`] (single-column wording).
#[component]
pub fn DragAndDropInstructions(props: DragAndDropInstructionsProps) -> Element {
    let id_sig = try_consume_context::<DragAndDropContext>()
        .map(|ctx| ctx.board.instructions_id)
        .or_else(|| try_consume_context::<BoardContext>().map(|b| b.instructions_id))
        .expect(
            "DragAndDropInstructions must be rendered inside a DragAndDropList or DragAndDropBoard",
        );
    let id = id_sig();
    let text = if in_drag_and_drop_board() {
        "Use Tab to move between columns and the up and down Arrow keys to move within a column. \
         Press Enter or Space to start reordering an item. While reordering, the up and down \
         Arrow keys change position within the column and the left and right Arrow keys move \
         the item to an adjacent column. Press Enter or Space to confirm, or Escape to cancel."
    } else {
        "Press Enter to start reordering. Use Arrow keys to change position. \
         Press Enter to confirm or Escape to cancel."
    };
    rsx! {
        div {
            id: "{id}",
            style: "position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0);",
            ..props.attributes,
            "{text}"
        }
    }
}

/// Live region for drag-and-drop announcements.
///
/// Composes both inside a standalone [`DragAndDropList`] (reads the
/// list's `DragAndDropContext`) and directly under a
/// [`DragAndDropBoard`] (reads the board's `BoardContext`).
#[component]
pub fn DragAndDropLiveRegion(props: DragAndDropLiveRegionProps) -> Element {
    let announcement_sig = try_consume_context::<DragAndDropContext>()
        .map(|ctx| ctx.board.announcement)
        .or_else(|| try_consume_context::<BoardContext>().map(|b| b.announcement))
        .expect(
            "DragAndDropLiveRegion must be rendered inside a DragAndDropList or DragAndDropBoard",
        );
    let announcement = announcement_sig();

    rsx! {
        div {
            role: "status",
            aria_live: "assertive",
            aria_atomic: "true",
            style: "position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0);",
            ..props.attributes,
            "{announcement}"
        }
    }
}

/// The props for the [`DragAndDropListItemProps`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropListItemProps {
    /// The index of the item in the list
    pub index: usize,

    /// Additional attributes to apply to the list item element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,

    /// The children of the list item component.
    pub children: Element,
}

/// The props for the [`DragAndDropDropIndicator`] component.
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropDropIndicatorProps {
    /// The index of the item this indicator is adjacent to.
    pub index: usize,

    /// The indicator position relative to the item.
    pub position: &'static str,

    /// Additional attributes to apply to the drop indicator element.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
}

/// Data for rendering a sortable list item.
#[derive(Clone, PartialEq)]
pub struct DragAndDropListRenderItem {
    /// The current index of this item.
    pub index: usize,

    /// The stable key for this item.
    pub key: String,

    /// The rendered item children.
    pub children: Element,
}

/// # DragAndDropListItem
///
/// This component represents an individual draggable item in the dnd list.
/// This must be used inside a [`DragAndDropList`] component.
///
/// ## Example
///
/// ```rust
///use dioxus::prelude::*;
///use dioxus_primitives::drag_and_drop_list::{DragAndDropList, DragAndDropListItem};
///#[component]
///pub fn Demo() -> Element {
///    let items = ["Item1", "Item2", "Item3"]
///        .map(|t| {
///            rsx! { {t} }
///        })
///        .to_vec();
///    rsx! {
///        DragAndDropList { items }
///    }
///}
/// ```
#[component]
pub fn DragAndDropListItem(props: DragAndDropListItemProps) -> Element {
    let mut ctx: DragAndDropContext = use_context();

    let index = props.index;
    let mut item_ctx = use_context_provider(move || DragAndDropItemContext {
        index: Signal::new(index),
    });
    if *item_ctx.index.peek() != index {
        item_ctx.index.set(index);
    }

    let mut item_ref: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    use_effect(move || {
        if ctx.is_focused(index) {
            if let Some(md) = item_ref() {
                spawn(async move {
                    let _ = md.set_focus(true).await;
                });
            }
        }
    });

    let onkeydown = move |event: Event<KeyboardData>| {
        let key = event.key();

        match key {
            Key::ArrowUp => {
                event.prevent_default();
                if ctx.is_dragging() {
                    ctx.move_up(index);
                    ctx.announce_move(index);
                } else {
                    ctx.focus_prev();
                }
            }
            Key::ArrowDown => {
                event.prevent_default();
                if ctx.is_dragging() {
                    ctx.move_down(index);
                    ctx.announce_move(index);
                } else {
                    ctx.focus_next();
                }
            }
            // ArrowLeft / ArrowRight: only meaningful while dragging
            // — moves the grabbed item to the adjacent column. Plain
            // arrow navigation between columns happens via Tab.
            Key::ArrowLeft if ctx.is_dragging() => {
                event.prevent_default();
                ctx.move_prev_column();
                ctx.announce_move(index);
            }
            Key::ArrowRight if ctx.is_dragging() => {
                event.prevent_default();
                ctx.move_next_column();
                ctx.announce_move(index);
            }
            Key::Enter => {
                event.prevent_default();
                ctx.toggle_drag(index);
            }
            Key::Character(ref c) if c == " " => {
                event.prevent_default();
                ctx.toggle_drag(index);
            }
            Key::Escape => {
                event.prevent_default();
                if ctx.is_dragging() {
                    let pos = ctx.drag_from().unwrap_or(index) + 1;
                    ctx.cancel_drag();
                    ctx.announce(format!(
                        "Movement cancelled. The item has returned to its starting position of {pos}"
                    ));
                }
            }
            Key::Delete | Key::Backspace => {
                event.prevent_default();
                if !ctx.is_dragging() {
                    ctx.remove(index);
                }
            }
            Key::Home => {
                event.prevent_default();
                if !ctx.is_dragging() {
                    ctx.set_focus(Some(0));
                }
            }
            Key::End => {
                event.prevent_default();
                if !ctx.is_dragging() {
                    ctx.set_focus(ctx.item_count().checked_sub(1));
                }
            }
            _ => {}
        };
    };

    let is_grabbing = ctx.drag_from().is_some_and(|from| from == index);
    let drop_at_origin = is_grabbing && ctx.drop_to() == Some(index);
    // Tab stop: the grabbed item while dragging (so the user can
    // continue acting on it), otherwise the column's roving entry.
    let is_tab_reachable =
        ctx.is_focused(index) || (!ctx.is_dragging() && ctx.tab_roving_idx() == Some(index));

    rsx! {
        li {
            aria_roledescription: "sortable item",
            draggable: "true",
            tabindex: if is_tab_reachable { "0" } else { "-1" },
            aria_grabbed: if is_grabbing { "true" } else { "false" },
            "data-is-grabbing": if is_grabbing { "true" },
            // Set when the drop target has returned to this item's starting slot —
            // i.e. dropping now would leave it in place. The primitive suppresses
            // the drop indicator in that case (no gap to point to), so styling
            // hooks off this attribute to surface the "stays here" state.
            "data-drop-at-origin": if drop_at_origin { "true" },
            "data-focus-visible": if ctx.is_focused(index) { "true" },
            onmounted: move |data| item_ref.set(Some(data.data())),
            onfocus: move |_| {
                if !ctx.is_dragging() {
                    ctx.set_focus(Some(index));
                }
            },
            ondragstart: move |event: Event<DragData>| {
                ctx.start_drag(index);
                event.data_transfer().set_effect_allowed("move");
                event.data_transfer().set_drop_effect("move");
                // Note: this is only for Firefox (without it, DnD won't work)
                let _ = event.data_transfer().set_data("text/html", "");
                let mut document_drop_ctx = ctx;
                let mut document_drop = document::eval(
                    r#"
                    function cleanup() {
                        document.removeEventListener("dragover", onDragOver, true);
                        document.removeEventListener("drop", onDrop, true);
                        document.removeEventListener("dragend", onDragEnd, true);
                    }

                    function onDragOver(event) {
                        event.preventDefault();
                        if (event.dataTransfer) {
                            event.dataTransfer.dropEffect = "move";
                        }
                    }

                    function onDrop(event) {
                        event.preventDefault();
                        dioxus.send("drop");
                        cleanup();
                    }

                    function onDragEnd() {
                        dioxus.send("end");
                        cleanup();
                    }

                    document.addEventListener("dragover", onDragOver, true);
                    document.addEventListener("drop", onDrop, true);
                    document.addEventListener("dragend", onDragEnd, true);

                    await dioxus.recv();
                    cleanup();
                    "#,
                );
                spawn(async move {
                    if let Ok(action) = document_drop.recv::<String>().await {
                        if action == "drop" {
                            document_drop_ctx.drop();
                        }
                    }
                    // The `<li>` that originated the drag may have been
                    // reparented (cross-column move) or unmounted by the
                    // time the browser fires `dragend`, in which case its
                    // own `ondragend` never reaches Dioxus and the drag
                    // state stays at `Dropped`. `drag_from()` returns
                    // `Some` for both `Dragging` and `Dropped`, so any
                    // item now sitting at the original `from.idx` would
                    // light up with `data-is-grabbing="true"` styling.
                    // Force the transition to `Idle` here.
                    document_drop_ctx.end_drag();
                    let _ = document_drop.send(true);
                });
            },
            ondragend: move |_| ctx.end_drag(),
            ondragover: move |event: Event<DragData>| {
                event.prevent_default();
                // Stop bubbling so the surrounding `<ul>` doesn't
                // overwrite the per-item target with the column-tail
                // fallback.
                event.stop_propagation();
                event.data_transfer().set_drop_effect("move");
                async move {
                    if let Some(md) = item_ref() {
                        let cursor_y = event.client_coordinates().y;
                        if let Ok(rect) = md.get_client_rect().await {
                            let mid_y = rect.origin.y + rect.size.height / 2.0;
                            let position = if cursor_y < mid_y {
                                DropPosition::Before
                            } else {
                                DropPosition::After
                            };
                            ctx.drag_over(index, position);
                        }
                    }
                }
            },
            onkeydown,
            ..props.attributes,
            {props.children}
        }
    }
}

/// The drop indicator rendered next to a sortable item.
#[component]
pub fn DragAndDropDropIndicator(props: DragAndDropDropIndicatorProps) -> Element {
    let ctx: DragAndDropContext = use_context();
    let render = ctx.drop_to().is_some_and(|to| to == props.index)
        && match props.position {
            "before" => ctx.drop_position() == DropPosition::Before,
            "after" => ctx.drop_position() == DropPosition::After,
            _ => false,
        };
    if !render {
        return rsx! {};
    }

    rsx! {
        div {
            "data-position": "{props.position}",
            ..props.attributes,
        }
    }
}

// ── Multi-column board API ───────────────────────────────────────────────

/// Props for [`DragAndDropBoard`].
#[derive(Props, Clone, PartialEq)]
pub struct DragAndDropBoardProps {
    /// Fires once per successful drop (within- or cross-column) with
    /// the resulting `(item_key, from_column, to_column, to_index)`.
    pub on_change: Callback<DragAndDropChange>,
    /// Accessible label announced for the board grouping. Surfaces
    /// to assistive tech as the name of the `role="group"` container
    /// that wraps the columns.
    #[props(default)]
    pub aria_label: Option<String>,
    /// Additional attributes for the wrapping `<div>`.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
    /// Child [`DragAndDropList`]s that share this board's drag state.
    pub children: Element,
}

/// Multi-column container that orchestrates drag between any number
/// of nested [`DragAndDropList`]s. Each list registers its column id
/// with the board on first render; drops fire `on_change` with the
/// resulting move so the consumer can persist it.
///
/// `DragAndDropBoard` provides only the shared drag context. The
/// caller is responsible for composing [`DragAndDropInstructions`]
/// and [`DragAndDropLiveRegion`] in the layout — render them once
/// per board (not once per list) so screen readers see a single
/// instructions block and a single live region.
#[component]
pub fn DragAndDropBoard(props: DragAndDropBoardProps) -> Element {
    let drag = use_signal(|| DragState::Idle);
    let columns: Signal<HashMap<String, RegisteredColumn>> = use_signal(HashMap::new);
    let column_order: Signal<Vec<String>> = use_signal(Vec::new);
    let focused: Signal<Option<Loc>> = use_signal(|| None);
    let tab_roving: Signal<HashMap<String, usize>> = use_signal(HashMap::new);
    let announcement = use_signal(String::new);
    let on_change_callback = props.on_change;
    let on_change: Signal<Option<OnChangeFn>> = use_signal(|| {
        Some(Rc::new(move |change: DragAndDropChange| {
            on_change_callback.call(change);
        }) as OnChangeFn)
    });
    let instructions_id = use_unique_id();

    use_context_provider(|| BoardContext {
        drag,
        columns,
        column_order,
        focused,
        tab_roving,
        announcement,
        on_change,
        instructions_id,
    });

    let aria_label = props.aria_label.unwrap_or_else(|| "Board".to_string());
    let describedby = instructions_id();
    rsx! {
        div {
            role: "group",
            aria_label: "{aria_label}",
            aria_describedby: "{describedby}",
            ..props.attributes,
            {props.children}
        }
    }
}
