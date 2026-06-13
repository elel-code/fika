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

The current replacement reproduces Zed's scrollbar model inside Fika:

- `src/ui/item_view.rs`
- `src/ui/item_view/scroll_bar.rs`

Each pane owns a `gpui::ScrollHandle`. `src/ui/file_grid.rs` makes the item
viewport the tracked scroll container via `track_scroll()` and
`overflow_x_scroll()`. The viewport is a normal flex child so GPUI can measure
its scrollable content size; `src/ui/item_view/scroll_bar.rs` is mounted as an
absolute sibling overlay in the same wrapper, so it reads the tracked
viewport's `ScrollHandle::bounds()` but is not part of the scrollable content.
`src/main.rs` no longer mounts a root-level scrollbar overlay and no longer
carries old app-side drag state.

`src/ui/item_view/scroll_bar.rs` now directly mirrors the Zed scrollbar
mechanics for the compact item view: thumb ranges come from
`ScrollHandle::max_offset()`, `ScrollHandle::bounds()` and
`ScrollHandle::offset()`; track clicks compute the Zed click offset; thumb
drags write negative GPUI offsets back with `ScrollHandle::set_offset()`; and
event handling switches between bubble and capture phases in the same pattern
as Zed's scrollbar element. The deleted `state.rs` module and old canvas
metrics are not retained.

Smooth/kinetic scrolling remains absent after the deletion pass and must be
rebuilt only on top of the `ScrollHandle` path.
