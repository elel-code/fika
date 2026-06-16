# Item View Custom Paint TODO

This is the active task board for the GPUI item view custom-paint migration.

## P0: Preparation

- [x] Confirm Dolphin reference boundary for `KItemListView` widget reuse.
- [x] Keep current viewport resize priming and snapshot cache behavior.
- [x] Document design and migration phases.
- [x] Add a short comment in `file_grid.rs` marking the temporary interaction
  shell vs static paint boundary.

## P1: Static Fallback Visual Canvas

- [x] Add a static item visual element for non-renaming, non-thumbnail fallback
  icon items.
- [x] Paint fallback icon background and marker from `FileIconSnapshot`.
- [x] Paint Compact/Icons item name lines from `VisibleItemSnapshot`.
- [x] Keep thumbnail items on current `img()` path.
- [x] Keep real theme icon items on current cached icon path until image paint
  ownership is audited.
- [x] Keep rename items on current editor path.
- [x] Preserve item drag preview and payload behavior.
- [x] Run `cargo fmt`, `cargo check`, `cargo test`, `cargo build`.
- [ ] Ask for `FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads` logs after
  this slice.

## P2: Text Shape Cache

- [x] Define text paint cache key.
- [x] Cache shaped lines for static item labels.
- [x] Invalidate on view mode, zoom/font metrics, selection color, displayed
  lines, or rename state change.
- [x] Instrument cache hit/miss counts behind `FIKA_PERF_ITEM_VIEW`.
- [ ] Verify resize does not reshape unchanged visible item labels.

## P3: Retained Paint Slot State

- [x] Add `ItemPaintSlot` state beside `VisibleItemSlotPool`.
- [x] Project `VisibleItemSnapshot` into retained paint state.
- [x] Track geometry-only vs content changes.
- [x] Patch selection/drop/hover visual state without rebuilding content.
- [x] Keep slot identity stable across overlapping scroll and resize.

## P4: Thumbnail Paint Boundary

- [x] Audit GPUI `img()` and `Window::paint_image` cache ownership.
- [x] Decide retained image element vs direct paint handle.
- [x] Add pane-local retained image cache for file-grid image items.
- [x] Key thumbnail/theme-icon image elements by visual slot id.
- [x] Preserve freedesktop cached-thumbnail first-frame behavior.
- [x] Preserve thumbnail failed/invalidation model semantics.
- [ ] Revisit direct `Window::paint_image` after Fika owns render-image decode
  and invalidation.

## P5: Dedicated Custom Element

- [x] Replace canvas spike if direct custom element gives better retained
  prepaint state.
- [ ] Move paint timing instrumentation into the custom element.
- [ ] Add tests around geometry and content-key invalidation.

## Acceptance Gates

- [ ] No behavior regression in rename, selection, context menu, item DnD,
  places DnD, and external drop paths.
- [ ] `cargo test` stays green.
- [ ] Perf logs show resize steady path stays sub-millisecond for item snapshot
  conversion and no new large `file-grid build` regression.
- [ ] Cold mode switch cost is tracked separately from resize cost.
- [ ] Custom paint path is used by static Compact and Icons items.
