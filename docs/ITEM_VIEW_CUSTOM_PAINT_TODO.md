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
- [x] Review user-provided `FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads`
  logs after this slice.

## P2: Text Shape Cache

- [x] Define text paint cache key.
- [x] Cache shaped lines for static item labels.
- [x] Invalidate on view mode, zoom/font metrics, selection color, displayed
  lines, or rename state change.
- [x] Instrument cache hit/miss counts behind `FIKA_PERF_ITEM_VIEW`.
- [x] Verify resize does not reshape unchanged visible item labels when text
  content and text rect dimensions are stable.

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

- [x] Audit GPUI custom element hitbox insertion for hover and cursor support.
- [~] Replace non-renaming per-item interaction shells with retained hitboxes:
  P9a moves hover/cursor first; P9b removes drag shells only after GPUI exposes
  a public custom-element drag-start API or Fika carries an audited GPUI patch.
- [x] Route non-renaming Compact/Icons hover and cursor projection through
  retained item visual state.
- [x] Route directory drag-over projection through retained item visual state;
  item/row shells no longer paint ad hoc `drag_over` backgrounds.
- [x] Preserve item/place drag preview cursor offset behavior.
- [x] Preserve Rust viewport hit testing for click/menu/drop across Compact,
  Icons, and Details retained migration paths.
- [x] Add P9a interaction-layer perf logging for retained hitbox prepaint/paint
  count and timing.
- [x] Compare P9a perf logs against the previous GPUI hover/cursor shell path
  before expanding custom interaction further; user `~/Downloads` logs show
  warm resize/fullscreen item-view conversion remains sub-millisecond, while
  cold mode-switch cache warm-up stays tracked separately.

## P10: Rename Overlay Boundary

- [x] Keep normal item background/text/image in content-level layers when rename
  starts.
- [x] Position rename editor as the only item-local overlay subtree.
- [x] Preserve caret hit testing, UTF-8 selection, warning/error helper, and Tab
  rename-next.
- [x] Verify starting/stopping rename does not rebuild unrelated item layer
  content.

## P11: Details Mode Paint Path

- [x] P11a: Project Details rows into retained paint slots while keeping the
  existing GPUI row subtree as the render path.
- [x] P11b: Paint row backgrounds, icons, and text cells from a content-level
  custom layer while retaining row shells for interaction and drag.
- [x] P11c: Preserve menu/DnD/drop controller fields and Trash-specific visual
  columns at the retained painter boundary.
- [x] P11d: Split Details visual layer perf logging into a dedicated
  `[fika details-visual]` channel so GPUI row-shell cost and custom paint cost
  can be compared without mixing with Compact/Icons static visuals.
- [x] Share image/text cache concepts with Compact/Icons where practical:
  Details now uses the same GPUI retained image cache path and a pane-local
  Details text shape cache with separate perf stats.

## Acceptance Gates

- [ ] No behavior regression in rename, selection, context menu, item DnD,
  places DnD, and external drop paths.
- [x] `cargo test` stays green.
- [ ] Perf logs show resize steady path stays sub-millisecond for item snapshot
  conversion, no new large `file-grid build` regression, and Details custom
  visual/text-shape cost is visible separately through `[fika details-visual]`
  and `[fika details-shape-cache]`.
- [ ] Cold mode switch cost is tracked separately from resize cost.
- [ ] Any custom paint expansion keeps Dolphin's model/controller/painter split
  and is retained only when perf is neutral or better than the GPUI built-in
  path for that surface.
- [x] Custom paint path is used by non-renaming Compact and Icons base/image
  visuals.
- [x] Non-renaming Compact/Icons items no longer require per-item GPUI visual
  children after P9a; temporary drag shells remain until P9b.
