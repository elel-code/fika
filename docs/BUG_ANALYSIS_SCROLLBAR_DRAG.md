# Pane Scrollbar Drag Analysis

Status: obsolete after the 2026-06-13 deletion and rewrite pass.

The previous pane scrollbar implementation has been removed completely:

- `src/ui/scrollbar.rs`
- `src/ui/scrollbar/*`
- pane-shell scrollbar slot wiring
- `FikaApp` pane scrollbar drag/cache state
- `src/ui/item_view_container.rs`
- `src/ui/item_view_container/*`
- `FikaApp` item-view scrollbar drag and smooth-scroll state
- core `HorizontalScrollBarLayout` / `horizontal_scroll_bar_layout`
- old pane scrollbar and UI smooth-scroll tests

The earlier drag-freeze analysis pointed to stale GPUI canvas state, cached
track geometry, and app-side smooth-scroll tick routing. Those code paths no
longer exist.

The current replacement is a new Dolphin `KItemListContainer` / `KItemListView`
aligned component under `src/ui/item_view_container.rs` and
`src/ui/item_view_container/*`. It must continue to be evaluated against
Dolphin's container/value/smooth-scroller behavior, not against the removed
implementation.
