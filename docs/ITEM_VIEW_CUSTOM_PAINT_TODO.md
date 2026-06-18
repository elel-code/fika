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
- [x] Preserve theme-icon visual stability by reusing retained same-`iconName`
  images before falling back to a neutral markerless placeholder.
- [x] Keep thumbnail role success/failure model-driven while painting item
  fallback visuals during pending image loads or resource load failures only
  after same-source retained images have been tried.
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
- [x] Record the 2026-06-17 pane self-drag root cause and acceptance trace:
  GPUI can stop delivering pane/item move callbacks after drag start, so the
  retained `ActiveItemDrag` target must be ticked by preview repaint when
  necessary.
- [x] Split `src/ui/file_grid.rs` along Dolphin-style model/projection,
  controller/hit-test, painter, and renderer-policy boundaries without changing
  behavior.
- [x] Extract root file-grid render surface composition into
  `src/ui/file_grid/surface.rs` so `src/ui/file_grid.rs` is no longer the owner
  of viewport/layer/shell assembly.
- [x] Extract item-view painter perf counters into `src/ui/file_grid/perf.rs`
  so render instrumentation is no longer owned by the main file-grid surface.
- [x] Move FikaApp item-view perf accessor/record methods into
  `src/ui/file_grid/perf.rs`.
- [x] Move item-view perf frame phase classification into
  `src/ui/file_grid/perf.rs` so resize/mode/content/visual instrumentation is
  no longer defined in `main.rs`.
- [x] Extract file-grid item/place/external drag move and drop handlers into
  `src/ui/file_grid/dnd.rs` so controller routing is no longer owned by the
  main painter/render surface.
- [x] Move item drag preview rendering and selection-count label logic into
  `src/ui/file_grid/dnd.rs` so the remaining GPUI drag-start shell boundary is
  centralized.
- [x] Extract file-grid wheel, pane navigation, and item mouse-down controller
  decisions into `src/ui/file_grid/controller.rs`.
- [x] Move file-icon resolve candidate ordering into
  `src/ui/file_grid/snapshot/scheduler.rs` so visible/read-ahead role work is
  projected beside metadata and thumbnail scheduling instead of in `main.rs`.
- [x] Move `PaneVisibleWorkKey` into `src/ui/file_grid/snapshot/range.rs` so
  app-level work dedupe no longer owns raw visible/work range extraction.
- [x] Move Compact/Icons layout option builders and Dolphin sizing constants
  into `src/ui/file_grid/layout.rs` so layout policy is no longer owned by the
  main renderer surface.
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
- [x] Remove the obsolete Compact/Icons item-local GPUI `img()` and static text
  visual fallback branch from `src/ui/file_grid.rs`; item shells are now
  transparent drag-start/rename boundaries while visuals and images come from
  retained content-level painters.
- [x] Extract Compact/Icons transparent item shells into
  `src/ui/file_grid/item_shell.rs` so the remaining GPUI drag-start and rename
  overlay bridge is separate from the main file-grid renderer surface.
- [x] Extract Details table/header and transparent row shells into
  `src/ui/file_grid/details_shell.rs` so the remaining Details GPUI drag-start
  bridge is separate from the main file-grid renderer surface.
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
- [x] Extract shared file-grid visual style and item identity helpers into
  `src/ui/file_grid/style.rs` so text alignment, shape-cache stats, stable
  element ids, and row/tile/drop backgrounds are no longer owned by the root
  render surface.
- [x] Extract file-grid root API types into `src/ui/file_grid/types.rs` so
  props, render snapshots, mode, and pane viewport geometry are no longer
  defined in the module index.
- [x] Extract raw file-grid snapshot construction into
  `src/ui/file_grid/snapshot/builder.rs` so model/layout traversal is separate
  from render snapshot conversion and scheduler projection helpers.
- [x] Extract raw-to-render snapshot conversion into
  `src/ui/file_grid/snapshot/render.rs` so final visible item/details
  projection is separate from raw snapshot types and scheduler helpers.
- [x] Move raw-to-render conversion tests into
  `src/ui/file_grid/snapshot/render.rs` so the snapshot facade no longer imports
  render conversion test helpers.
- [x] Extract visible item slot assignment into
  `src/ui/file_grid/snapshot/slots.rs` so snapshot-to-reuse-pool projection is
  separate from raw snapshot types and render conversion.
- [x] Move visible item slot assignment tests into
  `src/ui/file_grid/snapshot/slots.rs` so the snapshot facade no longer imports
  slot-pool test helpers.
- [x] Extract metadata/thumbnail scheduler queue projection into
  `src/ui/file_grid/snapshot/scheduler.rs` so async role scheduling handoff is
  separate from raw snapshot types and render conversion.
- [x] Move metadata scheduler queue tests into
  `src/ui/file_grid/snapshot/scheduler.rs` so the snapshot facade no longer
  imports scheduler-private metadata test helpers.
- [x] Extract visible range/work range projection into
  `src/ui/file_grid/snapshot/range.rs` so scroll/read-ahead range derivation is
  separate from raw snapshot types and render conversion.
- [x] Move visible/work range projection tests into
  `src/ui/file_grid/snapshot/range.rs` so the snapshot facade no longer imports
  range-private test helpers.
- [x] Extract visible item snapshot/cache projection into
  `src/ui/file_grid/snapshot/visible.rs` so stable visible item content reuse is
  separate from raw directory snapshot construction.
- [x] Move visible item snapshot cache tests into
  `src/ui/file_grid/snapshot/visible.rs` so geometry-only cache reuse coverage
  lives with the cache implementation.
- [x] Extract thumbnail candidate and read-ahead projection into
  `src/ui/file_grid/snapshot/thumbnail.rs` so role scheduling decisions are
  separate from raw snapshot construction.
- [x] Move thumbnail/read-ahead projection tests into
  `src/ui/file_grid/snapshot/thumbnail.rs` so the snapshot facade no longer
  imports thumbnail-private test helpers.
- [x] Extract metadata role candidate projection and its
  `RawFileGridSnapshot` method impl into `src/ui/file_grid/snapshot/metadata.rs`
  so MIME magic scheduling decisions are separate from raw snapshot
  construction.
- [x] Extract raw snapshot model/projection types into
  `src/ui/file_grid/snapshot/types.rs` so raw data contracts are separate from
  construction, conversion, scheduler, and range helpers.
- [x] Align Compact/Icons read-ahead with Dolphin's role-updater boundary:
  invisible work-window items can reuse existing snapshot content for paint
  warm-up, but uncached read-ahead items no longer trigger synchronous
  icon/text content resolution during the current render conversion.
- [x] Move file-icon theme path resolution out of render conversion: visible
  Compact/Icons/Details items now use cached/preliminary icon snapshots in the
  frame. Visible synchronous icon warming follows Dolphin `updateVisibleIcons()`
  index order, while the background resolve queue follows Dolphin
  `indexesToResolve()` visible/read-ahead order.
- [x] Invalidate visible item snapshot caches when background icon resolve
  results arrive so preliminary icons are replaced without synchronous theme
  lookup in scroll or zoom frames.
- [x] Keep thumbnail/theme-icon pending or load-failure frames visually stable:
  reuse retained same-source real images first, then paint fallback visuals when
  no retained image exists.
- [x] Align zoom icon visuals with Dolphin: ordinary MIME/theme icons resolve
  against the current layout icon size immediately, matching Dolphin
  `KStandardItemListWidget::pixmapForIcon()`, while theme icon files are still
  not decoded synchronously in prepaint. Theme icon images and their first-load
  placeholders now paint into the same current square icon box to avoid a
  pending-small-icon then real-icon size jump.
- [x] Extract retained item/details paint slot state into
  `src/ui/file_grid/paint_slots.rs` so model-to-painter snapshot reuse is
  separate from the renderer construction code.
- [x] Extract retained item/details interaction hitbox layer into
  `src/ui/file_grid/interaction.rs` so hover/cursor hitboxes and active
  item-drag window tracking are separate from the main painter/render surface.
- [x] Move the remaining cross-module file-grid tests into
  `src/ui/file_grid/tests.rs` so `src/ui/file_grid.rs` is only the module
  facade and public export boundary.
- [ ] Keep remaining drag-start shells until public GPUI custom-element
  drag-start support exists or an audited GPUI patch is carried.
- [ ] Keep rename on the GPUI overlay until custom text editing has behavior
  coverage for focus, caret, selection, validation, commit/cancel, and IME.
- [x] Treat Places as a separate renderer migration with its own GPUI baseline
  and DnD/scroll acceptance gate. Result: `docs/PLACES_RENDERER_PLAN.md`
  defines the Dolphin model/view split, the retained-row migration gate, the
  DnD/scroll acceptance checks, and the current `FIKA_PERF_PLACES_VIEW=1`
  GPUI baseline.

## P15: Full Transition Execution Plan

This is the active plan after the retained item-view direction was accepted.
It moves the codebase toward full custom-painted/reuse-pool ownership without
pretending that every remaining GPUI boundary can be removed safely today.

- [~] P15a: Freeze current desktop-session evidence after the Dolphin-aligned
  zoom icon visual update. Required logs:
  `FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads`,
  `FIKA_PERF_ITEM_VIEW=1 cargo run -- /etc`, and one
  `FIKA_DEBUG_DND=1` pane self-drag trace. Current state: `/etc`
  zoom/scroll autosmoke and the pane self-drag `via=preview` trace are
  recorded; the full `~/Downloads`/Details/manual DnD desktop-session pass
  still needs a refresh before another shell-removal or painter-expansion
  slice.
- [x] P15b: Record the evidence summary in
  `docs/ITEM_VIEW_RENDERER_DECISIONS.md` before expanding or reverting any
  renderer surface. Current evidence keeps MIME/theme icons on GPUI `img()`
  elements by default and identifies remaining `/etc` autosmoke cost as static
  visual/text/base paint rather than synchronous theme-icon path lookup.
- [x] P15c: Decide the drag-start boundary from source, not guesswork: either
  confirm a public GPUI custom-element drag-start API exists, carry a small
  audited GPUI patch, or keep Compact/Icons and Details drag-start shells as
  explicit platform boundaries. Current decision: GPUI `0.2.2` exposes typed
  drag start through interactive elements only, so the shells stay as explicit
  platform boundaries.
- [ ] P15d: If P15c unlocks retained drag start, remove Compact/Icons
  non-renaming drag shells first, then Details row drag shells. Each removal
  needs DnD smoke for item-to-directory, pane drop, Places drop/reorder, and
  external path drop.
- [~] P15e: Benchmark a Places retained/custom row painter against the current
  GPUI sidebar before implementing it. Places migration is accepted only if
  scroll, reorder, mount/trash/device rows, context menu, and drop behavior are
  neutral or better. Current state: the GPUI sidebar baseline and
  renderer-policy logs exist, and `FIKA_AUTOSMOKE_PLACES=targets` covers
  non-persistent target/insert projection. `PlacePaintSlotCache` now records
  retained row/section slots and `[fika places-slots]` stats; no retained/custom
  row painter is default. `FIKA_CUSTOM_PLACES_ROWS=1` now provides an opt-in
  row visual painter for background, active/drop state, label, trash marker,
  and insert indicators while keeping GPUI icons, row event delivery, context
  menus, DnD, and drag-start shells. `places/interaction.rs` now owns the
  row/section target decision, while GPUI shells still provide event delivery
  and bounds. The opt-in row visuals are now aggregated into one sidebar-level
  layer, so `[fika places-row-visual] rows` must match the policy row count
  instead of logging one canvas per row.
- [ ] P15f: Keep rename on GPUI until a custom text-editing plan covers focus,
  caret hit testing, UTF-8 selection, validation, commit/cancel, Tab rename-next,
  and IME. Do not merge a custom rename painter without that behavior matrix.
- [x] P15g: Tighten reuse-pool evidence. Runtime renderer-policy logs now prove
  that ordinary Compact/Icons and Details frames keep base visuals and
  interaction on retained item surfaces, with only the known drag-start,
  rename, and image-renderer boundaries allowed.
- [ ] P15h: Move any remaining item-view orchestration still living in
  `src/main.rs` into Dolphin-aligned file-grid modules when it can be done
  without changing behavior. Candidate boundaries: icon-role update scheduling,
  file-icon resolve queue handoff, and runtime evidence collection helpers.
  Done so far: file-icon queued/seen/in-flight resolve state lives in
  `file_grid/icon_work.rs`; visible file-icon sync and queued work handoff now
  route through `file_grid/icon_work.rs`; the earlier pane-local theme-icon
  role-size debounce was removed because it caused a delayed second zoom
  adjustment; raw-to-retained render snapshot projection now lives in
  `file_grid/snapshot/render.rs`, covering visible slot assignment, visible
  snapshot cache conversion, hover projection, and paint-slot projection;
  visible metadata/thumbnail/icon work keying and queue handoff now live in
  `file_grid/snapshot/scheduler.rs`; retained hovered-item state now lives in
  `file_grid/hover.rs`; retained file-grid projection/mode-switch cleanup
  policy now lives in `file_grid/lifecycle.rs`; visible metadata role sync
  result collection now lives in `file_grid/snapshot/metadata.rs`.
  Runtime evidence collection helpers remain in `src/main.rs` and scripts.

## P16: Concrete Full-Transition Backlog

This phase turns the accepted direction into an executable queue. It is ordered
by risk and evidence, not by how custom-painted a surface looks.

- [x] P16a: Record the full-transition tracks in the planning, design, and TODO
  docs: evidence, painter, controller, shell boundary, Places, and ownership.
- [x] P16b: Collect a fresh desktop-session evidence set after the latest
  Dolphin-aligned theme-icon paint-bound change:
  `/etc` custom-theme vs default logs now prove default MIME/theme icons avoid
  first-load `theme_placeholder` churn, and
  `FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc`
  captures unattended zoom/scroll evidence.
- [x] P16c: Update `docs/ITEM_VIEW_RENDERER_DECISIONS.md` with that evidence,
  including whether `/etc` zoom/scroll still shows cold image-load jank or
  visible placeholder-to-icon switching. Current evidence: `icon_sync` max fell
  from `28340us` to `173us` after visible sync stopped duplicating queued
  read-ahead icon work; remaining `/etc` autosmoke cost is static visual
  text/base paint, not MIME/theme image rendering.
- [x] P16d: Add or extend runtime evidence tooling if the current logs cannot
  distinguish these cases: first-load theme-icon placeholder, retained
  same-`iconName` reuse, GPUI image-cache decode completion, and steady
  repaint cost. `[fika item-image]` now reports `theme_loaded`,
  `theme_decoded`, `theme_retained`, `theme_placeholder`, `thumb_loaded`,
  `thumb_decoded`, `thumb_retained`, and `thumb_fallback`; the runtime analyzer
  summarizes them as `image_sources`. `FIKA_AUTOSMOKE_ITEM_VIEW` now exercises
  zoom/scroll without manual input and adds `[fika autosmoke]` markers to the
  same perf log.
- [x] P16e: Audit local GPUI source for a retained/custom-element drag-start
  path. If no public API exists, document the exact blocker and keep item and
  Details drag-start shells. Result: GPUI `0.2.2` at Zed commit `f16a469`
  exposes typed drag initiation through `Interactivity::on_drag` /
  `InteractiveElement::on_drag` in `crates/gpui/src/elements/div.rs`.
  Custom elements can insert hitboxes with `Window::insert_hitbox()` but do not
  have a public API to start a typed drag from those retained hitboxes, so the
  item and Details drag-start shells remain explicit platform boundaries.
- [ ] P16f: If an audited GPUI patch is chosen, design the smallest API that
  starts drags from retained hitboxes while preserving payload, preview,
  cursor offset, accepted transfer modes, and external drop behavior.
- [x] P16g: Move the next behavior-preserving item-view orchestration boundary
  out of `src/main.rs`. Candidate: runtime item-view perf/evidence collection
  accessors, because painter perf state already lives under `file_grid/perf.rs`.
  Done so far: the `FIKA_PERF_ITEM_VIEW` flag and file-grid perf-layer callers
  are owned by `src/ui/file_grid/perf.rs`; item-view perf frame classification
  and perf-state cleanup are owned by `src/ui/file_grid/perf.rs`; frame-state
  and painter perf stats storage now live behind `ItemViewPerfState` in
  `src/ui/file_grid/perf.rs`; item-view perf summary emission is now owned by
  `src/ui/file_grid/perf.rs`; autosmoke scenario parsing and action sequencing
  now live in `src/ui/file_grid/autosmoke.rs`; autosmoke marker formatting for
  start/complete, zoom actions, and scroll actions also lives in
  `src/ui/file_grid/autosmoke.rs`.
- [x] P16h: Draft a Places retained row painter design before changing Places
  rendering. The design must cover row groups, hidden sections, device rows,
  reorder/drop insertion, context menus, and sidebar scroll. Result:
  `docs/PLACES_RENDERER_PLAN.md` compares Dolphin's
  `DolphinPlacesModel + KFilePlacesView` split with Fika's current
  `places/model`, `projection`, `sidebar/row`, `drag`, and custom scrollbar
  modules, then gates any retained row painter behind Places-specific perf logs,
  runtime smoke, and renderer-policy evidence.
- [x] P16i: Draft a rename custom-editor behavior matrix before changing the
  GPUI rename overlay. It must cover focus, caret hit testing, UTF-8 selection,
  validation helper text, commit/cancel, Tab rename-next, and IME. Result:
  `docs/RENAME_EDITOR_PLAN.md` compares Dolphin's
  `DolphinView::renameSelectedItems()`, `KItemListView::editRole()`, and
  `KItemListRoleEditor` path with Fika's `RenameDraft`, shortcut routing, and
  GPUI overlay. The matrix keeps the overlay as default until IME,
  focus/focus-out, mouse selection, accessibility, and runtime smoke are
  covered.
- [x] P16j: Establish the historical image-renderer baseline before the next
  MIME/theme-icon flicker fix. Use `a3f5b0f` as the pre-retained/custom-paint
  GPUI `img()` baseline, and use `d497593`, `8d1198f`, `36da130`, and
  `b0cac9a` as transition checkpoints to decide whether the regression belongs
  to model/projection, retained slot state, custom element paint, or the
  custom image layer. Compare these with Dolphin
  `KStandardItemListWidget::updatePixmapCache()` / `pixmapForIcon()` before
  changing the current image renderer. Current-code A/B support is available
  through `FIKA_CUSTOM_THEME_ICONS=1`, which keeps retained item state but
  forces MIME/theme icons through the custom item-image layer for
  desktop-session comparison against the default GPUI theme-icon renderer.
  `scripts/compare-item-image-renderers.sh` now standardizes the paired-log
  comparison, and the 2026-06-17 `/etc` smoke evidence is recorded in
  `docs/ITEM_VIEW_RENDERER_DECISIONS.md`.
- [x] P16k: Decide the Compact/Icons theme-icon renderer from evidence:
  default now uses GPUI `img()` elements for MIME/theme icons and keeps
  thumbnails on the custom image layer. Keep this split unless paired
  default-vs-`FIKA_CUSTOM_THEME_ICONS=1` zoom/scroll logs prove the custom
  theme-icon painter is neutral or better without first-load placeholders,
  zoom-time `theme_decoded` churn, or size jumps.
- [x] P16l: Establish the Places GPUI sidebar baseline before any retained row
  painter work. `FIKA_PERF_PLACES_VIEW=1` now logs snapshot time, sidebar build
  time, and the current renderer-policy surface counts for the GPUI row path;
  `docs/PLACES_RENDERER_PLAN.md` records the 2026-06-17 desktop-session
  baseline.
- [x] P16m: Add a non-destructive Places runtime smoke path before any retained
  row painter work. `FIKA_AUTOSMOKE_PLACES=targets` now drives place target,
  insert-start, insert-end, clear, and snapshot logging without reordering or
  persisting bookmarks. Full reorder/drop mutation smoke remains gated on
  isolated user-place configuration or manual review.
- [x] P16n: Add retained Places paint slots and stats without changing visible
  rendering. `PlacePaintSlotCache` retains section headings and place rows by
  stable semantic identity, preferring device id for device rows and path/group
  for normal rows. `[fika places-slots]` now reports inserted/content/
  geometry/visual/unchanged/removed slot activity for the current GPUI sidebar.
- [x] P16o: Extract Places row/section target decisions out of GPUI row
  closures before any retained hitbox or custom row painter work.
  `places/interaction.rs` now returns shared target/cursor decisions for
  item/external path drops and place reorders. The GPUI row/section shells still
  provide event delivery, bounds, and drag start.
- [x] P16p: Add a Places perf/autosmoke analyzer before benchmarking a custom
  row painter. `scripts/analyze-places-perf.sh` now summarizes
  `[fika places-view]`, `[fika places-sidebar]`, `[fika places-slots]`,
  `[fika places-renderer-policy]`, and non-destructive Places autosmoke
  markers. `scripts/check-places-perf-analyzer.sh` covers the analyzer gates.
- [x] P16s: Add the first opt-in Places row visual painter without switching
  the default renderer. `FIKA_CUSTOM_PLACES_ROWS=1` custom-paints row
  background, active/drop visual state, label, trash marker, and insert
  indicator; default Places rows remain GPUI. Analyzer support now includes
  `--expect-custom-row-visual-policy` and `[fika places-row-visual]`
  prepaint/paint maxima.
- [x] P16t: Add non-destructive Places overflow autosmoke and scrollbar perf
  evidence. `FIKA_AUTOSMOKE_PLACES=overflow` appends snapshot-only test rows
  without writing user Places configuration, `[fika places-scrollbar]` reports
  visible overflow and `max_scroll_y`, and `scripts/analyze-places-perf.sh`
  now supports `--require-overflow-autosmoke`.
- [x] P16u: Aggregate the opt-in Places row visual painter into one
  sidebar-level layer before considering a default switch. Root cause:
  the first opt-in painter used one canvas per row, so the overflow smoke logged
  `places_row_visual_frames=675 max_rows=1` for 75 visible rows. Implementation:
  `places_row_visual_layer` paints all row backgrounds, labels, trash markers,
  and insert indicators from the sidebar snapshot stream while GPUI keeps icons,
  event delivery, context menus, DnD, and drag-start shells. Evidence:
  `/tmp/fika-places-custom-rows-layer.log` passed
  `--require-autosmoke --expect-custom-row-visual-policy` with `max_rows=11`,
  and `/tmp/fika-places-overflow-custom-layer.log` passed
  `--require-overflow-autosmoke --expect-custom-row-visual-policy` with
  `max_rows=75`. Guard: the analyzer now rejects custom row visual policy logs
  where `[fika places-row-visual] rows` does not match the policy row count.
- [x] P16v: Add retained text shaping for the opt-in Places row visual layer.
  Root cause: after row visuals were aggregated into one canvas, the opt-in
  prepaint path still reshaped every row label every frame. Implementation:
  `PlacesRowTextShapeCache` lives on `FikaApp` and caches row labels by
  label/font/font-size/text color for `FIKA_CUSTOM_PLACES_ROWS=1` only.
  Evidence/guard: `FIKA_PERF_PLACES_VIEW=1` now emits
  `[fika places-row-shape-cache] hits=... misses=... evicted=... entries=...`,
  and `scripts/analyze-places-perf.sh --expect-custom-row-visual-policy`
  requires that channel for opt-in custom row logs.
- [x] P16w: Add runtime Places panel width and visibility state without changing
  the row renderer default. The top toolbar now has a Places toggle, the
  sidebar splitter can resize the panel and double-click reset it, and resize
  requests flow through the existing pane-row remeasure path so item-view
  viewports are recalculated after width changes. This is intentionally runtime
  only; a later persistence slice must save width/visibility through a
  coalesced settings path rather than writing config on every drag frame.
- [x] P16x: Persist Places panel width and visibility through a narrow app
  settings model. `src/core/settings.rs` stores `places.sidebar.width` and
  `places.sidebar.visible` in `$XDG_CONFIG_HOME/fika/settings.tsv`; startup
  loads those values before rendering the panel. UI changes schedule a
  latest-only 120ms delayed background save using a generation counter, so
  repeated sidebar drag frames update memory without synchronous config writes.
- [x] P16y: Add unattended Places panel layout smoke before depending on manual
  sidebar testing. `FIKA_AUTOSMOKE_PLACES=layout` drives hide, show, resize,
  reset, restore, and final settings-file verification through the same app
  state/update-save path as the toolbar and splitter. The analyzer gate
  `--require-layout-autosmoke` rejects missing actions or a final
  `layout-verify-saved ok=false`, so future Places renderer work can prove it
  did not break panel layout state while comparing GPUI and opt-in custom row
  policies. Evidence: `/tmp/fika-places-layout.log` passed
  `--require-layout-autosmoke --expect-current-gpui-policy`, and
  `/tmp/fika-places-layout-custom.log` passed
  `--require-layout-autosmoke --expect-custom-row-visual-policy`.
- [x] P16z: Make the Places interaction boundary measurable before moving row
  hitboxes out of GPUI. `[fika places-interaction-policy]` reports retained
  row/section target-decision counts separately from the current GPUI
  event-shell and drag-start shell counts. The analyzer option
  `--require-interaction-policy` requires row and section target decisions to
  match visible rows/sections while `retained_hitboxes=0`,
  `gpui_event_shells=rows+sections`, and `drag_shells=rows`; this keeps the
  current Dolphin-aligned decision layer explicit without pretending that
  activation, menus, DnD event delivery, or drag start have already left GPUI.
  Evidence: `/tmp/fika-places-targets-interaction.log` passed
  `--require-autosmoke --require-interaction-policy --expect-current-gpui-policy`;
  `/tmp/fika-places-custom-targets-interaction.log` passed
  `--require-autosmoke --require-interaction-policy --expect-custom-row-visual-policy`.
- [x] P16aa: Add retained Places interaction geometry projection without
  changing event delivery. `places_interaction_geometry()` projects row and
  section y/height data from the same `PLACE_ROW_HEIGHT` and
  `PLACE_SECTION_HEADING_HEIGHT` constants used by the opt-in visual layer.
  `[fika places-interaction-geometry]` reports rows, sections, entries,
  content height, hit-test samples, and projection time;
  `--require-interaction-geometry` requires those counts to match renderer
  policy. This creates the future retained hit-test data boundary while keeping
  `retained_hitboxes=0` and GPUI row/section event shells explicit. Evidence:
  `/tmp/fika-places-targets-geometry.log` passed
  `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy`;
  `/tmp/fika-places-custom-targets-geometry.log` passed
  `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy`.
- [x] P16ab: Add retained Places geometry hit-test logic without changing event
  delivery. `PlacesInteractionGeometry::hit_test_y()` maps a content-local y
  coordinate to a retained row or section, and row hits reuse the same
  `place_drop_zone_for_y()` edge/body rule as the existing GPUI row DnD
  handlers. This prepares the future retained hitbox layer while keeping
  activation, context menus, DnD event delivery, and drag start on GPUI shells.
  Evidence: `/tmp/fika-places-targets-hit-test.log` passed
  `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy`;
  `/tmp/fika-places-custom-targets-hit-test.log` passed
  `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy`,
  both with `max_hit_tests=2`.
- [x] P16ac: Add unattended retained Places hit-test autosmoke before moving
  row/section event delivery out of GPUI shells. `FIKA_AUTOSMOKE_PLACES=hit-test`
  samples the first retained row at insert-before, on-place, and insert-after
  y positions, samples the first section heading, and emits a summary that
  requires both rows and sections to exist. `scripts/analyze-places-perf.sh`
  now has `--require-hit-test-autosmoke`, and
  `scripts/check-places-perf-analyzer.sh` covers both valid and invalid marker
  fixtures. The runtime evidence paths are documented in
  `docs/PLACES_RENDERER_PLAN.md`: `/tmp/fika-places-retained-hit-test.log`
  passed the current GPUI policy gate, and
  `/tmp/fika-places-custom-retained-hit-test.log` passed the opt-in custom row
  visual policy gate.
- [x] P16ad: Polish the user-facing Places sidebar layout controls after the
  retained renderer boundary is stable. The current code already has runtime
  width, hide/show, reset, settings persistence, and
  `FIKA_AUTOSMOKE_PLACES=layout`. Dolphin exposes the Places panel dock action
  as `show_places_panel` with `Qt::Key_F9`; Fika now mirrors that with an F9
  Places toggle while the toolbar button shares the same app-level visibility
  path. Unit coverage proves the shortcut classification and that toggling
  preserves the last sidebar width. Pane viewport remeasurement remains covered
  by the layout autosmoke; `/tmp/fika-places-f9-layout.log` passed
  `--require-layout-autosmoke --expect-current-gpui-policy`.
- [x] P16ae: Move retained Places hit-test autosmoke report ownership out of
  `src/main.rs` and into `src/ui/places/autosmoke.rs`. The app root now only
  supplies the projected `PlaceSnapshot` list; Places owns retained row/section
  sampling, expected edge/body zones, summary calculation, and module-level
  tests. This keeps runtime evidence collection aligned with the Places
  model/controller boundary before row/section event delivery leaves GPUI
  shells. Evidence: `/tmp/fika-places-hit-test-autosmoke-module.log` passed
  `--require-hit-test-autosmoke --expect-current-gpui-policy`.
- [x] P16af: Move Places autosmoke snapshot summary ownership out of
  `src/main.rs` and into `src/ui/places/autosmoke.rs`. The Places module now
  owns visible row count, section transition count, active row count, place
  target count, and insert-before/after counts for non-destructive runtime
  smoke logs. This keeps target/overflow/layout evidence on the same retained
  snapshot boundary used by Places projection. Evidence:
  `/tmp/fika-places-snapshot-autosmoke-module.log` passed
  `--require-autosmoke --expect-current-gpui-policy`.
- [x] P16ag: Move Places layout autosmoke reporting out of `src/main.rs`.
  `src/ui/places/autosmoke.rs` now owns the sidebar layout smoke state type,
  resize target policy, layout action log formatting, and saved-settings
  verification report. The app root still mutates panel visibility/width and
  reads settings, but no longer owns the evidence/reporting logic for hide,
  show, resize, reset, restore, or verify. Evidence:
  `/tmp/fika-places-layout-autosmoke-module.log` passed
  `--require-layout-autosmoke --expect-current-gpui-policy`.
- [x] P16ah: Move Places drop-target autosmoke action reporting out of
  `src/main.rs`. `src/ui/places/autosmoke.rs` now owns the target path label,
  insert-index action report, and clear-target action log formatting used by
  the non-destructive DropTargets scenario. The app root still chooses and
  mutates the target state, but the Places module owns the runtime evidence
  markers consumed by the analyzer. Evidence:
  `/tmp/fika-places-target-actions-autosmoke-module.log` passed
  `--require-autosmoke --expect-current-gpui-policy`.
- [x] P16ai: Move the DropTargets first-place selection rule out of
  `src/main.rs`. `src/ui/places/autosmoke.rs` now owns the rule that picks the
  first mounted `PlaceSnapshot` for the non-destructive place-target action.
  The app root still applies the selected path to app state, but the scenario
  model no longer depends on app-root iteration over projected Places rows.
  Evidence: `/tmp/fika-places-first-target-autosmoke-module.log` passed
  `--require-autosmoke --expect-current-gpui-policy`.
- [x] P16aj: Move Places autosmoke start/complete marker formatting out of
  `src/main.rs`. `src/ui/places/autosmoke.rs` now owns the stable scenario
  marker labels consumed by the analyzer instead of relying on app-root
  `Debug` formatting. The app root still schedules scenario actions, but the
  marker surface belongs to the Places autosmoke module. Evidence:
  `/tmp/fika-places-start-complete-autosmoke-module.log` passed
  `--require-autosmoke --expect-current-gpui-policy`.
- [x] P16ak: Move item-view autosmoke marker formatting out of `src/main.rs`.
  `src/ui/file_grid/autosmoke.rs` now owns stable scenario labels plus
  start/complete, zoom-action, and scroll-action marker formatting for
  `FIKA_AUTOSMOKE_ITEM_VIEW`. The app root still applies zoom and scroll to
  pane state, but item-view runtime evidence markers belong to the file-grid
  autosmoke module. Evidence:
  `/tmp/fika-item-view-autosmoke-marker-module.log` passed the item-view
  analyzer gates used for `/etc` zoom/scroll evidence.
- [x] P16al: Require item-view autosmoke markers in the analyzer. The
  item-view perf analyzer now supports `--require-autosmoke` and validates
  start/complete scenario markers plus the required zoom and changed scroll
  actions for `Zoom`, `Scroll`, and `ZoomScroll` scenarios. The analyzer
  summary always includes an `autosmoke:` line so renderer evidence blocks can
  prove which scripted scenario produced the log. Evidence:
  `scripts/check-item-view-perf-analyzer.sh` covers the positive `ZoomScroll`
  fixture and a negative missing-scroll-action fixture.
- [x] P16am: Split the next Places migration boundary into retained event
  delivery instead of treating row visual painting as enough. The Places plan
  now defines a future retained event policy gate, keeps GPUI drag-start shells
  explicit, and orders the work as hover/cursor, activation/context-menu
  targeting, drag-move/drop delivery, then GPUI row/section shell removal. This
  prevents the opt-in row visual painter from being mistaken for
  behavior-complete retained Places rows.
- [x] P16an: Add the Places retained event-delivery analyzer gate before
  changing event routing. `scripts/analyze-places-perf.sh` now supports
  `--expect-retained-event-policy`, which accepts either current GPUI row
  visuals or the aggregated opt-in custom visual layer while requiring
  `retained_interaction` and retained hitboxes to equal rows+sections,
  `gpui_event_shells=0`, and drag shells to remain rows. The analyzer fixture
  covers default visuals, custom visuals, and the negative mixed state where
  custom row visuals still depend on GPUI event shells.
- [x] P16ao: Record the item-view reuse-pool ownership boundary. The status
  doc now makes `VisibleItemSlotPool` and `ItemPaintSlotCache` the source of
  Compact/Icons reusable item identity, with Details paint state retained by
  `ItemId`. GPUI ids remain only as consumers for shell/image surfaces, not as
  the primary reuse mechanism. Future reuse-pool work must preserve that
  boundary and update the retained slot/paint-slot tests or runtime
  `[fika item-paint-slots]` evidence if it changes.
- [x] P16ap: Make retained item paint-slot evidence analyzer-visible. The
  item-view analyzer now summarizes `[fika item-paint-slots]` retained slot
  activity and supports `--require-paint-slots`; the standard runtime log gate
  uses it so renderer evidence includes inserted, content, geometry, visual,
  unchanged, removed, and entries maxima. The analyzer fixture covers valid
  Compact/Icons/Details paint-slot logs plus missing and empty slot evidence.
- [x] P16aq: Make retained item renderer-policy evidence analyzer-enforced.
  `scripts/analyze-item-view-perf.sh --expect-retained-item-policy` now rejects
  renderer-policy logs unless every item has a retained base visual,
  `retained_interaction + rename_overlay == items`, the current GPUI drag shell
  is explicit for every item, and image surfaces stay within the item count.
  The standard runtime log gate enables this check, and the analyzer fixture
  covers a count-valid but retained-interaction-invalid policy.
- [x] P16ar: Move raw item-view snapshot conversion into the file-grid module.
  `project_retained_file_grid_snapshot()` now owns the behavior-preserving
  sequence from raw grid snapshot to retained render snapshot: assign
  `VisibleItemSlotPool` slots, convert through `VisibleItemSnapshotCache`,
  apply hovered-item visual state, and project into `ItemPaintSlotCache`.
  `src/main.rs` still owns pane/app state storage and icon resolution, but no
  longer hand-wires that retained projection sequence inline. Unit coverage
  proves slot assignment, icon request, paint-slot insertion, and hover visual
  projection through the new boundary.
- [x] P16as: Move visible raw-grid work queue handoff into the file-grid
  module. `queue_raw_file_grid_model_work()` now owns the
  `PaneVisibleWorkKey` duplicate-work gate plus metadata role, thumbnail probe,
  and file-icon resolve candidate queueing for a raw grid snapshot. `src/main.rs`
  keeps a thin pane/app-state wrapper and still starts the background workers,
  but no longer hand-wires the three scheduler handoffs inline. Unit coverage
  proves unchanged work keys skip duplicate queueing after the first metadata
  and icon work submission.
- [x] P16at: Move retained hovered-item controller state into the file-grid
  module. `RetainedHoveredItem` now owns pane/item hover identity, change
  detection, pane clearing, and per-pane lookup for retained visual projection.
  `src/main.rs` still exposes the event-facing methods used by current GPUI
  shells and retained hitbox callbacks, but the state model is no longer a raw
  app-root `Option<(PaneId, ItemId)>`. Unit coverage proves idempotent set,
  item clear, pane clear, and cross-pane lookup behavior.
- [x] P16au: Move retained file-grid lifecycle cleanup policy into the
  file-grid module. `file_grid/lifecycle.rs` now owns which retained item-view
  slots, paint slots, snapshot caches, text-shape caches, perf phase/layer
  stats, hover state, compact widths, and visible work keys are cleared for
  projection invalidation versus mode-switch invalidation. `src/main.rs`
  still decides when a pane/filter/view-mode transition triggers cleanup, but
  no longer repeats the retained state cleanup list inline.
- [x] P16av: Move visible metadata role sync collection into the file-grid
  module. `visible_metadata_role_results_for_raw_grid()` now owns the
  visible-candidate loop, sync budget cutoff, request filtering, and metadata
  role result generation for a raw grid snapshot. `src/main.rs` still applies
  those results to the pane model and invalidates visible snapshots when model
  roles change. Unit coverage proves zero-budget cutoff and visible-only
  candidate conversion.
- [x] P16aw: Move file-grid visible snapshot cache invalidation policy into the
  file-grid lifecycle module. `file_grid/lifecycle.rs` now owns pane-local and
  global visible snapshot cache invalidation used after visible icon sync,
  visible metadata sync, and background icon resolve completion. `src/main.rs`
  still decides when role/icon results changed, but no longer reaches directly
  into `visible_item_snapshot_caches` for those invalidation paths.
- [x] P16ax: Move retained file-grid projection state handoff into the
  file-grid module. `file_grid/retained.rs` now owns removing and reinserting
  pane-local `VisibleItemSlotPool`, `VisibleItemSnapshotCache`, and
  `ItemPaintSlotCache` state around raw-to-retained projection, including the
  retained hovered-item lookup and icon snapshot callback. `src/main.rs` still
  decides when a pane render needs conversion, but no longer wires the retained
  slot/cache handoff inline.
- [x] P16ay: Move the app-side raw-grid model-work queue wrapper into the
  file-grid module. `file_grid/retained.rs` now owns the thin pane lookup and
  app-state handoff into `queue_raw_file_grid_model_work()`, while `src/main.rs`
  only consumes the queued metadata/thumbnail/icon booleans to start the
  existing workers. This keeps the Dolphin-style visible-work dedupe and role
  scheduling handoff behind the file-grid boundary.
- [x] P16az: Move the app-side raw file-grid snapshot wrapper into the
  file-grid module. `file_grid/retained.rs` now owns pane lookup and
  `RawFileGridSnapshotInput` assembly, including selection, rename draft,
  drop-target, filter, source revision, and compact column-width state.
  `src/main.rs` still decides when snapshots are needed, but no longer builds
  raw file-grid snapshot inputs inline.
- [x] P16ba: Move the visible metadata sync application wrapper into the
  file-grid module. `file_grid/retained.rs` now owns collecting visible
  metadata role results for a raw grid, applying them through the existing app
  model result path, and invalidating the pane visible snapshot cache when
  visible roles change. Background metadata workers still use the shared model
  result application path in `src/main.rs`.
- [x] P16bb: Move background metadata and thumbnail result application into the
  file-grid retained boundary. `file_grid/retained.rs` now owns applying
  generation-checked `MetadataRoleResult` and `ThumbnailProbeResult` batches to
  pane models, while `src/main.rs` keeps only the worker scheduling, scheduler
  completion, restart, and notify decisions. This keeps raw-grid visible sync
  and background role/thumbnail result mutation on the same retained model
  side of the Dolphin-style boundary.
- [x] P16bc: Move file-grid model-work lifecycle helpers into the retained
  boundary. `file_grid/retained.rs` now owns pane-local metadata-role and
  thumbnail cancellation, stale-generation cleanup, and file-icon snapshot
  lookup for retained projection. `src/main.rs` still triggers these actions
  from pane load/refresh/close events and worker scheduling, but no longer owns
  the scheduler cleanup or icon snapshot policy.
- [x] P16bd: Move item-view scroll transient state into the item-view module.
  `ItemViewScrollState` now owns GPUI scroll handles, post-layout
  authoritative-scroll frame counters, and scrollbar-drag state together.
  `src/main.rs` still syncs pane `ViewState` to and from the controller, but no
  longer carries parallel `HashMap`/`HashSet` state for item-view scroll
  lifecycle.
- [x] P16be: Move item-view scroll-handle sync decision logic into the
  item-view module. `ItemViewScrollState` now returns `ItemViewScrollSyncAction`
  for normal handle sync, post-layout authoritative view sync, and scrollbar
  drag sync. `src/main.rs` still applies resulting scroll values to the pane
  model, but no longer decides which scroll source is authoritative.
- [x] P16bf: Move item-view scrollbar-axis viewport policy into the item-view
  module. `ui/item_view.rs` now owns which view modes use horizontal item-view
  scrollbars and the projected item viewport width calculation for a pane
  width. `src/main.rs` still supplies pane geometry and applies viewport
  priming, but no longer embeds the scrollbar-axis width deduction rule.
- [x] P16bg: Move item-view wheel scroll axis policy into the item-view module.
  `ui/item_view.rs` now owns how Compact maps wheel input onto horizontal
  scrolling and how Icons/Details keep wheel input vertical. `src/main.rs`
  still applies the resulting delta to the pane model, but no longer embeds
  per-view-mode wheel-axis mapping.
- [x] P16bh: Move item-view view-mode axis-change viewport priming policy into
  the item-view module. `ui/item_view.rs` now owns how switching between
  horizontal-scrollbar and vertical-scrollbar modes shifts the cached viewport
  width/height by the reserved scrollbar extent. `src/main.rs` still writes the
  resulting dimensions to the pane view and resets scroll maxima.
- [x] P16bi: Move item-view filter-bar viewport-height priming policy into the
  item-view module. `ui/item_view.rs` now owns how showing or hiding the filter
  bar adjusts the cached item viewport height and applies the core viewport
  normalization rule. `src/main.rs` still supplies the filter-bar height,
  writes the pane view height, and keeps the scroll handle temporarily
  authoritative.
- [x] P16bj: Move item-view window-resize viewport prime policy into the
  item-view module. `ui/item_view.rs` now owns normalization of render viewport
  dimensions, resize delta detection, and applying the resulting width/height
  deltas to cached item-view extents. `src/main.rs` still updates pane-row
  width, projects per-pane item widths from split geometry, and writes the
  pane view dimensions.
- [x] P16bk: Move item-view layout-change scroll authoritative policy into the
  scroll state. `ItemViewScrollState::preserve_for_layout_change()` now owns the
  two-frame view-authoritative handoff after preserving scroll through zoom or
  layout changes. `src/main.rs` still writes the preserved scroll values to the
  pane model, but no longer knows the frame-count policy for that path.
- [x] P16bl: Move item-view authoritative handle-sync policy into the scroll
  state. `ItemViewScrollState::sync_handle_to_view_authoritatively()` now owns
  the two-frame view-authoritative handoff used after app-side viewport
  priming such as filter-bar visibility changes. `src/main.rs` still supplies
  the pane view scroll values, but no longer combines raw handle sync with a
  frame-count mark itself.
- [ ] P16q: After every P16 implementation slice, commit separately with the
  relevant verification: docs-only slices need `git diff --check`; code slices
  need `cargo fmt`, `cargo check`, `cargo test -q`,
  `scripts/check-item-view-perf-analyzer.sh`,
  `scripts/check-places-perf-analyzer.sh`, and `git diff --check`.
- [x] P16r: Document the runtime self-test and breakthrough-recording rule.
  Repeatable scroll, zoom, startup-icon, resize, mode-switch, and Places target
  regressions should be reproduced through autosmoke logs and analyzer scripts
  before relying on manual timing. Any confirmed optimization breakthrough must
  record the symptom, Dolphin comparison boundary, root cause, implementation,
  saved log/analyzer command, and future regression guard in the owning design
  or decision document.

## Acceptance Gates

- [~] No behavior regression in rename, selection, context menu, item DnD,
  places DnD, and external drop paths: unit coverage now includes a retained
  behavior matrix across Compact, Icons, and Details for app-side hit testing,
  selection, item menus, rename draft routing, item drag source state, external
  path normalization/drop target menus, and item/place drop-target handoff.
  Keep this partial until full `cargo test` and runtime DnD smoke are both
  refreshed after each shell-removal or painter expansion slice.
- [x] `cargo test` stays green.
- [~] Perf logs show resize steady path stays sub-millisecond for item snapshot
  conversion, no new large `file-grid build` regression, Compact/Icons custom
  visual cost is visible through `[fika static-item-visual]`, image paint cost
  is visible through `[fika item-image]` when image-backed icons/thumbnails are
  present, item-image source counts show whether frames are using decoded
  theme icons, retained same-`iconName` images, first-load placeholders, or
  thumbnail fallbacks, aggregate custom paint cost is summarized, and Details
  custom visual/text-shape cost is visible separately through
  `[fika details-visual]` and `[fika details-shape-cache]`. Scroll/zoom evidence
  should also show that
  cold theme-icon work no longer appears as a synchronous render conversion
  spike after the first frame has switched to preliminary icons. Current
  `/etc` autosmoke satisfies the Compact/Icons zoom-scroll icon-sync part;
  Details and full DnD runtime smoke still need a desktop-session refresh.
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
