# Pane Scrollbar Drag Analysis

Status: obsolete after the 2026-06-13 deletion and pane-decoupling pass.

The previous pane scrollbar implementation has been removed completely:

- `src/ui/scrollbar.rs`
- `src/ui/scrollbar/*`
- pane-shell scrollbar slot wiring
- `FikaApp` pane scrollbar drag/cache state
- `src/ui/item_view_container.rs`
- `src/ui/item_view_container/*`
- `FikaApp` item-view scrollbar drag and smooth-scroll state
- `src/core/scroll.rs`
- core `HorizontalScrollBarLayout` / `horizontal_scroll_bar_layout`
- old pane scrollbar and UI smooth-scroll tests

The earlier drag-freeze analysis pointed to stale GPUI canvas state, cached
track geometry, and app-side smooth-scroll tick routing. Those code paths no
longer exist.

The current replacement starts from a pane-decoupled component:

- `src/ui/item_view.rs`
- `src/ui/item_view/scroll_bar.rs`
- `src/ui/item_view/scroll_bar/state.rs`

`src/ui/pane.rs` no longer creates or sizes the item-view scrollbar. The file
grid mounts it beside the viewport. The GPUI event layer follows the same
paint-phase hitbox and pointer-capture pattern used by the working Other
Application chooser scrollbar, while scrollbar geometry, page press and thumb
drag mapping live in the independent state module. Smooth/kinetic scrolling was
deleted with the broken pane path and must be rebuilt only after the basic
independent scrollbar is verified.
