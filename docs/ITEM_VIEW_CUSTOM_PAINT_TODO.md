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
- [x] Route Details hover projection through retained row visual state; Details
  row shells no longer paint ad hoc hover backgrounds.
- [x] Route Details hover/cursor hit testing through the retained interaction
  layer; Details row shells no longer own hover listeners or cursor styling.
- [x] Route Details click/menu/navigation/middle-paste through viewport-level
  retained hit testing; Details row shells no longer own mouse-down handlers or
  block mouse events.
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
  custom layer while initially retaining row shells as the bridge.
- [x] P11c: Preserve retained Details path/drag fields and Trash-specific
  visual columns at the retained painter boundary.
- [x] P11e: Shrink Details row shells to the remaining GPUI drag-start boundary;
  click, menu, navigation, scroll, and middle-paste controller behavior now
  routes through viewport retained hit testing.
- [x] P11f: Route Details drop dispatch through the viewport-level drop
  handlers; Details row shells no longer own per-row item/external/place drop
  handlers.
- [x] P11d: Split Details visual layer perf logging into a dedicated
  `[fika details-visual]` channel so GPUI row-shell cost and custom paint cost
  can be compared without mixing with Compact/Icons static visuals.
- [x] Share image/text cache concepts with Compact/Icons where practical:
  Details now uses the same GPUI retained image cache path and a pane-local
  Details text shape cache with separate perf stats.

## P12: Remaining Boundary Audit

- [x] Audit local GPUI drag APIs: GPUI 0.2.2 exposes drag initiation through
  `Div::on_drag`, while custom elements expose hitboxes and mouse listeners but
  no public custom-element drag-start hook.
- [x] Document the remaining item-local surfaces: Compact/Icons drag-start
  shells, Details drag-start row shells, and the rename text-editing overlay.
- [x] Add `docs/ITEM_VIEW_RUNTIME_SMOKE.md` with the runtime DnD, rename, and
  perf-log checklist for post-P11e verification.
- [x] Add `scripts/analyze-item-view-perf.sh` to summarize perf logs and enforce
  required steady/details/static-visual/interaction channels and exercised view
  modes, including Compact/Icons static visual mode coverage, during post-P11e
  review.
- [ ] Run a runtime DnD smoke pass after P11e: item drag, item-to-directory
  drop, pane drop, Places drop/reorder, external path drop, and rename caret
  click in Compact, Icons, and Details.
- [ ] Collect post-P11e `FIKA_PERF_ITEM_VIEW=1` logs across Compact, Icons, and
  Details resize/fullscreen paths before expanding custom paint or attempting
  another shell-removal slice.

## P13: Renderer Decision Gate

- [ ] Before each new custom-painted surface, identify the Dolphin-style model,
  layouter, controller/hit-test, and painter owners.
- [ ] Keep GPUI built-ins for surfaces where GPUI remains faster or owns a
  required platform contract, while still feeding them from retained model data.
- [ ] Expand custom paint only when runtime logs show neutral or better steady
  behavior and the migration keeps behavior-complete drag/drop, rename, and
  selection paths.
- [ ] For every surface that currently has a GPUI path, capture a same-scenario
  GPUI baseline before accepting a custom painter as the default renderer.
- [ ] Record the renderer decision and perf evidence in the relevant reference
  doc or TODO entry before removing any existing GPUI surface.
- [x] Add `docs/ITEM_VIEW_RENDERER_DECISIONS.md` as the current per-surface
  renderer decision log.
- [x] Add `scripts/summarize-item-view-renderer-evidence.sh` so passing runtime
  perf logs produce a renderer decision evidence block.
- [x] Centralize Compact/Icons renderer choices in an explicit
  `ItemRendererPolicy` so custom-paint vs GPUI surface decisions are not hidden
  behind ad hoc booleans.
- [x] Centralize Details row renderer choices in an explicit
  `DetailsRowRendererPolicy` covering visual layer, retained interaction, and
  GPUI drag-start shell boundaries.
- [x] Emit `[fika renderer-policy]` logs so runtime perf evidence includes the
  actual surface-count distribution for custom paint, retained interaction, and
  GPUI shell boundaries.
- [x] Require renderer-policy log coverage for Compact, Icons, and Details in
  the standard runtime perf gate.
- [x] Split renderer policy into `src/ui/file_grid/renderer_policy.rs` so the
  custom-paint vs GPUI renderer decision boundary is separate from rendering
  construction.
- [x] Make `scripts/analyze-item-view-perf.sh` reject impossible
  renderer-policy surface counts so custom-paint evidence cannot claim more
  custom/retained/GPUI surfaces than the logged item count.

## P14: Full Transition Roadmap

- [x] Add `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md` so the current replacement
  state, remaining GPUI boundaries, and full transition roadmap are explicit.
- [ ] Freeze a current desktop-session runtime evidence block for Compact,
  Icons, and Details before another painter expansion.
- [x] Refresh `FIKA_DEBUG_DND=1` runtime evidence after the active item-drag
  preview repaint fallback: pane item drag over a pane directory logs
  `active-item-move via=preview ... kind=Some(Directory)` and visually
  highlights the directory before drop.
- [ ] Split `src/ui/file_grid.rs` along Dolphin-style model/projection,
  controller/hit-test, painter, and renderer-policy boundaries without changing
  behavior.
- [x] Extract item-view painter perf counters into `src/ui/file_grid/perf.rs`
  so render instrumentation is no longer owned by the main file-grid surface.
- [x] Move FikaApp item-view perf accessor/record methods into
  `src/ui/file_grid/perf.rs`.
- [x] Extract file-grid item/place/external drag move and drop handlers into
  `src/ui/file_grid/dnd.rs` so controller routing is no longer owned by the
  main painter/render surface.
- [x] Move item drag preview rendering and selection-count label logic into
  `src/ui/file_grid/dnd.rs` so the remaining GPUI drag-start shell boundary is
  centralized.
- [x] Extract file-grid wheel, pane navigation, and item mouse-down controller
  decisions into `src/ui/file_grid/controller.rs`.
- [x] Extract Compact/Icons image paint layer into
  `src/ui/file_grid/image_layer.rs` so image-cache prepaint and image/fallback
  painting are no longer owned by the main file-grid surface.
- [x] Extract Compact/Icons static visual paint layer into
  `src/ui/file_grid/static_visual.rs` so base item background, fallback icon,
  and label shaping/painting are no longer owned by the main file-grid surface.
- [x] Extract Details visual paint layer into
  `src/ui/file_grid/details_visual.rs` so row backgrounds, icon prepaint,
  fallback icon painting, text shaping, and cell painting are no longer owned by
  the main file-grid surface.
- [x] Extract the GPUI rename overlay boundary into
  `src/ui/file_grid/rename_overlay.rs` so caret placement, UTF-8 selection
  clamping, helper text, and editor positioning are separate from the main
  file-grid renderer surface while remaining on GPUI.
- [x] Centralize GPUI item/details drag-start shell installation in
  `src/ui/file_grid/dnd.rs` while keeping the shell as the current platform
  boundary.
- [x] Move item/details drag payload projection into `src/ui/file_grid/dnd.rs`
  so the remaining GPUI shell consumes DnD-owned data.
- [x] Centralize viewport-level item/external/place drag-move and drop shell
  installation in `src/ui/file_grid/dnd.rs`.
- [x] Install directory item/row drop-target shells through
  `src/ui/file_grid/dnd.rs`; these remain positive target assertions, not the
  only source of pane-internal drag hover state.
- [x] Track pane-internal active item drags from a window mouse listener in the
  retained interaction layer so self-drags update retained directory highlight
  while moving even when GPUI does not deliver per-element `on_drag_move`
  callbacks after `item-start`.
- [x] Extract viewport measurement and shell wiring into
  `src/ui/file_grid/viewport.rs`, keeping scroll, retained hit testing,
  rubber-band selection, and viewport-level DnD handlers outside the main
  painter/render surface.
- [x] Extract retained item/details paint slot state into
  `src/ui/file_grid/paint_slots.rs` so model-to-painter snapshot reuse is
  separate from the renderer construction code.
- [x] Extract retained item/details interaction hitbox layer into
  `src/ui/file_grid/interaction.rs` so hover/cursor hitboxes and active
  item-drag window tracking are separate from the main painter/render surface.
- [ ] Keep remaining drag-start shells until public GPUI custom-element
  drag-start support exists or an audited GPUI patch is carried.
- [ ] Keep rename on the GPUI overlay until custom text editing has behavior
  coverage for focus, caret, selection, validation, commit/cancel, and IME.
- [ ] Treat Places as a separate renderer migration with its own GPUI baseline
  and DnD/scroll acceptance gate.

## Acceptance Gates

- [~] No behavior regression in rename, selection, context menu, item DnD,
  places DnD, and external drop paths: unit coverage now includes a retained
  behavior matrix across Compact, Icons, and Details for app-side hit testing,
  selection, item menus, rename draft routing, item drag source state, external
  path normalization/drop target menus, and item/place drop-target handoff.
  Keep this partial until full `cargo test` and runtime DnD smoke are both
  refreshed after each shell-removal or painter expansion slice.
- [x] `cargo test` stays green.
- [ ] Perf logs show resize steady path stays sub-millisecond for item snapshot
  conversion, no new large `file-grid build` regression, Compact/Icons custom
  visual cost is visible through `[fika static-item-visual]`, image paint cost
  is visible through `[fika item-image]` when image-backed icons/thumbnails are
  present, aggregate custom paint cost is summarized, and Details custom
  visual/text-shape cost is visible separately through `[fika details-visual]`
  and `[fika details-shape-cache]`.
- [x] Cold mode switch cost is tracked separately from resize cost: `[fika
  item-view]` now includes `phase=initial|mode-switch|content-change|
  geometry-change|visual-change|steady`, with unit coverage proving mode
  switches are not classified as resize/geometry changes.
- [ ] Any custom paint expansion keeps Dolphin's model/controller/painter split
  and is retained only when perf is neutral or better than the GPUI built-in
  path for that surface.
- [ ] If a custom-painted surface loses to GPUI built-ins on perf or behavior
  completeness, keep the Dolphin-aligned retained model but leave that surface on
  the GPUI renderer until the migration can be narrowed or justified.
- [x] Custom paint path is used by non-renaming Compact and Icons base/image
  visuals.
- [x] Non-renaming Compact/Icons items no longer require per-item GPUI visual
  children after P9a; temporary drag shells remain until P9b.
