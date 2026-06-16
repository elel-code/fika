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
- [x] Revisit direct `Window::paint_image`: P8 uses GPUI's public
  `RetainAllImageCache` / `ImageAssetLoader` / `RenderImage` contract instead
  of reimplementing image decode in Fika.

## P5: Dedicated Custom Element

- [x] Replace canvas spike if direct custom element gives better retained
  prepaint state.
- [x] Move paint timing instrumentation into the custom element.
- [x] Add tests around geometry and content-key invalidation.

## P6: Pane-Level Static Visual Layer

- [x] Paint static fallback Compact and Icons visuals through one content-level
  custom layer.
- [x] Keep item slots as transparent interaction shells for static fallback
  items.
- [x] Keep thumbnail, theme-icon, and rename items on their specialized child
  paths.
- [x] Add tests proving only fallback static items enter the layer.
- [x] Revisit whether thumbnail/theme-icon retained image items can join a
  viewport painter: P8 moves them into a custom image paint layer backed by
  GPUI's retained image cache.

## P7: Non-Rename Base Visual and Image Layer

- [x] Include every non-renaming Compact/Icons item in the content-level base
  visual layer.
- [x] Paint fallback icon markers only for items without thumbnail/theme-icon
  paths.
- [x] Move thumbnail/theme-icon `img()` elements into a content-level image
  layer keyed by retained visual slot id.
- [x] Keep non-renaming item shells transparent and interaction-only.
- [x] Keep rename items on the current child subtree.
- [x] Skip fallback marker shaping and cache-key fragmentation for image-backed
  items.
- [x] Revisit direct `Window::paint_image`: P8 uses GPUI's retained image cache
  contract for direct painting without adding a Fika-owned decoder yet.

## P8: Direct Image Paint Layer

- [x] Replace content-level thumbnail/theme-icon `img()` children with one
  custom image paint element.
- [x] Use pane-local `RetainAllImageCache` plus GPUI `ImageAssetLoader` for
  async path/SVG/image decode.
- [x] Paint loaded images with `Window::paint_image`.
- [x] Preserve theme-icon fallback marker rendering on image load failure.
- [x] Keep thumbnail failures model-driven; do not synthesize fallback icons for
  failed thumbnail paths in paint.
- [x] Match `ObjectFit::Contain` image bounds.
- [x] Add tests for image-paint membership and fallback policy.

## P9: Painted Interaction Hitboxes

- [ ] Audit GPUI custom element hitbox insertion for drag source, hover, and
  cursor support.
- [ ] Replace non-renaming per-item interaction shells with retained hitboxes.
- [ ] Route hover and directory drag-over projection through retained item visual
  state.
- [ ] Preserve item/place drag preview cursor offset behavior.
- [ ] Preserve Rust viewport hit testing for click/menu/drop.

## P10: Rename Overlay Boundary

- [ ] Keep normal item background/text/image in content-level layers when rename
  starts.
- [ ] Position rename editor as the only item-local overlay subtree.
- [ ] Preserve caret hit testing, UTF-8 selection, warning/error helper, and Tab
  rename-next.
- [ ] Verify starting/stopping rename does not rebuild unrelated item layer
  content.

## P11: Details Mode Paint Path

- [ ] Project Details rows into retained paint slots.
- [ ] Paint row backgrounds, icons, and text cells from a custom layer.
- [ ] Preserve sort/menu/DnD/Trash column behavior.
- [ ] Share image/text cache concepts with Compact/Icons where practical.

## Acceptance Gates

- [ ] No behavior regression in rename, selection, context menu, item DnD,
  places DnD, and external drop paths.
- [ ] `cargo test` stays green.
- [ ] Perf logs show resize steady path stays sub-millisecond for item snapshot
  conversion and no new large `file-grid build` regression.
- [ ] Cold mode switch cost is tracked separately from resize cost.
- [ ] Custom paint path is used by non-renaming Compact and Icons base/image
  visuals.
- [ ] Non-renaming Compact/Icons items no longer require per-item GPUI visual
  children after P9.
