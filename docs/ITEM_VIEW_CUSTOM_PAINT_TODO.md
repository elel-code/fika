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
- [x] Route directory item/row drop targeting through
  `src/ui/file_grid/dnd.rs` during the migration. This was later narrowed by
  the retained viewport/window-position path: per-directory GPUI drag-move
  shells are no longer the pane directory hover source.
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
  retained row/section slots and `[fika places-slots]` stats. This entry was
  later narrowed by P16dy: the default now uses the Dolphin-aligned custom
  chrome layer for background/drop/insert/trash while GPUI keeps text, icons,
  row event delivery, context menus, DnD, and drag-start shells.
  `FIKA_CUSTOM_PLACES_ROWS=1` remains the full custom-text benchmark path.
  `places/interaction.rs` now owns the row/section target decision, while GPUI
  shells still provide event delivery and bounds. The row visuals are
  aggregated into one sidebar-level layer, so `[fika places-row-visual] rows`
  must match the policy row count instead of logging one canvas per row.
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

The current post-Places-chrome execution entry point is
`docs/FULL_RETAINED_RENDERER_ROADMAP.md`; keep this backlog aligned with its
tracks.

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
  Details drag-start shells. Result: GPUI exposes typed drag initiation through
  `Interactivity::on_drag` / `StatefulInteractiveElement::on_drag` in
  `crates/gpui/src/elements/div.rs`. Custom elements can insert hitboxes with
  `Window::insert_hitbox()` and observe mouse events with
  `Window::on_mouse_event()`, but do not have a public API to start a typed
  drag from those retained hitboxes, so the item, Details, and Places
  drag-start shells remain explicit platform boundaries. 2026-06-19 refresh:
  the same blocker is still true at Zed commit
  `69b602c797a62f09318916d24a98c930533fbdc8`.
- [x] P16f: If an audited GPUI patch is chosen, design the smallest API that
  starts drags from retained hitboxes while preserving payload, preview,
  cursor offset, accepted transfer modes, and external drop behavior. Current
  design: `docs/FULL_RETAINED_RENDERER_ROADMAP.md` Track 4 now defines a
  minimal retained typed drag API split covering drag start
  (`Window::on_hitbox_drag`) and typed drag-move/drop payload delivery
  (`Window::on_hitbox_drag_move`, `Window::can_drop_on_hitbox`,
  `Window::on_hitbox_drop`), without recreating visible GPUI rows/items as
  drag sources or targets.
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
- [~] P16k1: Design and implement a retained MIME/theme icon image cache before
  making custom theme-icon paint the default. The cache should be keyed by at
  least `(iconName, icon_size_px)` plus theme/scale/color-scheme inputs when
  those affect the selected path. It must retain the last real same-key image
  during refresh, keep thumbnail retention separate by thumbnail path, and never
  synchronously decode theme icon files during prepaint. Design is now captured
  in `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md`; the foundation is implemented,
  while paired runtime evidence and analyzer gates remain pending.
- [ ] P16k2: Add paired default-vs-custom autosmoke evidence for the future
  MIME/theme icon renderer. Required scenarios: `/etc` and a mixed user
  directory, startup plus `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll`, default GPUI
  `img()` versus `FIKA_CUSTOM_THEME_ICONS=1` or a future retained-icon-cache
  flag. The offline comparison gate exists as
  `scripts/compare-item-image-renderers.sh --gate-default-promotion`; runtime
  logs still need to pass. 2026-06-18 `/etc` logs were captured at
  `/tmp/fika-icon-default-etc-p16k2.log` and
  `/tmp/fika-icon-custom-etc-p16k2.log`; the gate failed because the custom path
  still had `theme_placeholder=118` and `theme_decoded=5`. The custom path must
  show no steady `theme_placeholder` churn, no zoom-time `theme_decoded` burst,
  no visible size jump, and `icon_sync` within the Dolphin-style visible-first
  budget before the default renderer can change.
- [~] P16k2a: Build the prewarm/hybrid bridge before reconsidering default
  custom theme icons. `FIKA_PREWARM_THEME_ICONS=1` now prewarms retained
  theme-icon images while leaving visible theme icons on GPUI `img()`. The
  2026-06-18 `/tmp/fika-icon-prewarm-etc-p16k2.log` smoke kept
  `max_image_layer=0`, `max_gpui_image_element=64`, `theme_placeholder=0`, and
  `paint_count=0`, while exposing prewarm work as `theme_prewarm_loaded=598`,
  `theme_prewarm_decoded=5`, and `theme_prewarm_pending=118`. This validates
  the no-visible-placeholder bridge. The readiness handoff foundation is now
  implemented: app-level `ThemeIconImageReadiness` records exact size/scale
  theme keys only after a real `RenderImage` exists, `PaneSnapshot`/`FileGridProps`
  carry that snapshot to renderer policy, and opt-in `FIKA_HYBRID_THEME_ICONS=1`
  keeps visible icons on GPUI until the current key is ready.
  `/tmp/fika-icon-hybrid-etc-readiness.log` confirms the `/etc` handoff has
  `theme_placeholder=0`, `theme_decoded=0`, and `max_paint=383us` while the
  default comparison `/tmp/fika-etc-zoom-scroll.log` remains
  `max_image_layer=0`/`max_gpui_image_element=64`. Runtime default-vs-hybrid
  evidence still needs to pass before any default promotion because `/etc`
  still has a visible-item `icon_sync` spike around 24ms and the mixed-directory
  run is still missing.
- [ ] P16k3: Only after P16k1/P16k2 pass, reconsider the Compact/Icons
  MIME/theme icon renderer policy in `docs/ITEM_VIEW_RENDERER_DECISIONS.md`.
  Until then, keep the current split: thumbnails on the custom image layer and
  ordinary MIME/theme icons on GPUI `img()` over retained item shells.
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
- [x] P16bm: Move item-view bounds-update scroll sync policy into the scroll
  state. `ItemViewScrollState::sync_after_bounds_update()` now owns the
  scrollbar-drag branch, normal handle sync, authoritative tick, and
  handle-changed reporting after viewport bounds arrive. `src/main.rs` still
  applies returned pane-view sync actions, but no longer decides this
  lifecycle path itself.
- [x] P16bn: Move item-view user-scroll handle sync policy into the scroll
  state. `ItemViewScrollState::sync_handle_after_user_scroll()` now owns
  clearing transient view-authoritative state and syncing the GPUI scroll
  handle after wheel-driven pane model scroll changes. `src/main.rs` still
  applies the pane model scroll, but no longer combines those scroll-state
  lifecycle operations itself.
- [x] P16bo: Move item-view transient-clearing handle sync policy into the
  scroll state. `ItemViewScrollState::sync_handle_to_view_clearing_transients()`
  now owns clearing authoritative/scrollbar-drag transient state and syncing
  the GPUI handle when pane loading preserves model scroll. `src/main.rs` still
  supplies pane view scroll values, but no longer sequences those scroll-state
  operations itself.
- [x] P16bp: Move item-view scrollbar-drag sync policy into the scroll state.
  `ItemViewScrollState` now owns authoritative handle sync actions during
  scrollbar drag updates and the finish-drag handoff that reports both the
  pane-view sync action and whether a drag was active. `src/main.rs` still
  applies returned pane-view sync actions, but no longer reaches into the raw
  finish/sync primitives for this lifecycle path.
- [x] P16bq: Move item-view rubber-band drag threshold policy into the
  rubber-band module. `ui/rubber_band` now owns the Manhattan-distance start
  threshold for activating a pending rubber-band selection, while `src/main.rs`
  still supplies clamped content points and starts/updates the active
  selection band.
- [x] P16br: Move file-grid viewport window-to-content point policy into the
  viewport module. `ui/file_grid/viewport.rs` now owns converting window
  positions into scrolled content points and clamped content points from
  `PaneViewportGeometry` plus `ViewState`. `src/main.rs` still performs pane
  lookup and uses those points for hit testing, drag targets, and rubber-band
  selection.
- [x] P16bs: Move file-grid viewport pane hit-testing policy into the viewport
  module. `ui/file_grid/viewport.rs` now owns choosing the pane whose viewport
  contains a window position while preserving `PaneController::pane_ids()` order
  as the priority. `src/main.rs` still supplies the current pane order and
  cached viewport geometries for cross-pane drag target lookup.
- [x] P16bt: Move pending rubber-band state into the rubber-band module.
  `ui/rubber_band` now owns both active and pending rubber-band data models;
  `src/main.rs` still starts, updates, finishes, and applies selection results
  from those states.
- [x] P16bu: Move pending rubber-band activation policy into the rubber-band
  module. `PendingRubberBand` now owns pane matching plus the Dolphin-like
  Manhattan drag threshold through `can_activate()`, while `src/main.rs` still
  supplies the clamped current content point and starts/updates selection.
- [x] P16bv: Move active rubber-band state mutation policy into the rubber-band
  module. `RubberBandState` now owns construction, pane ownership checks, and
  same-pane current-point updates. `src/main.rs` still stores the active state,
  clears drafts, computes intersecting items, and applies selection changes.
- [x] P16bw: Move rubber-band finish state-clearing policy into the rubber-band
  module. `finish_rubber_band_for_pane()` now owns clearing only the pending
  and active rubber-band states that belong to the target pane. `src/main.rs`
  still decides when lifecycle events finish a rubber-band interaction.
- [x] P16bx: Move rubber-band selection activity update policy into the
  rubber-band module. `set_rubber_band_selection_activity_for_count()` now owns
  the rule that a pane remains rubber-band-selection active only while the
  latest rubber-band selection count is nonzero. `src/main.rs` still stores the
  active pane set and emits status text.
- [x] P16by: Move rubber-band selection activity clear/query policy into the
  rubber-band module. `clear_rubber_band_selection_activity_for_pane()` and
  `rubber_band_selection_activity_is_active()` now own production-path clearing
  and selected-count-aware activity checks. `src/main.rs` still supplies the
  activity set and pane selected count.
- [x] P16bz: Move active rubber-band viewport-rect projection into the
  rubber-band module. `active_rubber_band_viewport_rect_for_pane()` now owns
  pane ownership checking plus converting the active band into a clipped
  viewport rect for rendering. `src/main.rs` still supplies the active state
  snapshot and current pane view.
- [x] P16ca: Move active rubber-band pane ownership query/clear policy into the
  rubber-band module. `active_rubber_band_is_for_pane()` and
  `clear_active_rubber_band_for_pane()` now own production-path active-band
  pane checks and active-only clearing. `src/main.rs` still decides which app
  lifecycle events request that clearing.
- [x] P16cb: Move pending rubber-band press state replacement into the
  rubber-band module. `press_pending_rubber_band_for_pane()` now owns clearing
  the active band and installing a pending band for a blank-press start.
  `src/main.rs` still decides when a blank press is valid.
- [x] P16cc: Move active rubber-band start state replacement into the
  rubber-band module. `start_active_rubber_band_for_pane()` now owns clearing
  pending state and installing the active band when a pending drag activates.
  `src/main.rs` still clears drafts and applies selection updates.
- [x] P16cd: Move active rubber-band update writeback into the rubber-band
  module. `update_active_rubber_band_for_pane()` now owns same-pane current
  point updates and writing the updated active band back into the active state
  slot. `src/main.rs` still uses the returned band rect to compute selection.
- [x] P16ce: Move pending rubber-band activation start selection into the
  rubber-band module. `pending_rubber_band_activation_start()` now owns checking
  whether a pending band can activate for the current pane/content point and
  returning the start point for active-band startup. `src/main.rs` still
  supplies the clamped current point and performs draft cleanup/selection.
- [x] P16cf: Move file-grid projected hit/intersection query composition into
  the projection module. `pane_content_item_hit_at_point()` and
  `pane_model_indexes_intersecting_visual_rect()` now own the sequence of
  building a pane layout projection, applying rename-draft visual bounds, and
  mapping filtered layout indexes back to model indexes. `src/main.rs` still
  supplies pane/filter/cache inputs and decides how query results affect
  selection, DnD, and context-menu behavior.
- [x] P16cg: Move item-view scroll sync outcome classification into the scroll
  state. `ItemViewScrollSyncAction::into_outcome()` now owns whether a returned
  scroll action carries pane-view values and whether those values differ from
  the current view snapshot. `src/main.rs` still applies the returned scroll
  values to the pane model.
- [x] P16ch: Move item-view scroll sync view-snapshot API into the scroll state.
  `ItemViewScrollViewSnapshot` now carries the pane view scroll tuple through
  handle-sync and authoritative-handle sync paths, and `src/main.rs` no longer
  passes those values as loose fields for those production paths.
- [x] P16ci: Record the future MIME/theme icon custom-renderer work stream.
  `docs/ITEM_VIEW_RENDERER_DECISIONS.md` now documents the retained
  `(iconName, icon_size)` image-cache direction, hybrid promotion option,
  no-sync-decode rule, and paired default/custom evidence gate needed before
  replacing the default GPUI `img()` MIME/theme renderer.
- [x] P16cj: Move item-view scroll lifecycle snapshot APIs into the scroll
  state. Bounds updates, scrollbar-drag finish sync, and layout-change scroll
  preservation now have `ItemViewScrollViewSnapshot` entry points; production
  paths in `src/main.rs` no longer pass those scroll values as loose fields.
- [x] P16ck: Move item-view handle-to-view snapshot sync APIs into the scroll
  state. Authoritative handle sync, user-scroll handle sync, and
  transient-clearing handle sync now consume `ItemViewScrollViewSnapshot` in
  production paths instead of loose scroll fields.
- [x] P16cl: Narrow item-view scroll tuple helper visibility. The loose-field
  scroll helpers are now scroll-state implementation details; production and
  cross-module tests use the snapshot API surface.
- [x] P16cm: Record the updated GPUI dependency baseline. The 2026-06-18
  lockfile update moved GPUI to Zed commit `e4f6742a`, and the current
  dependency baseline is Zed commit
  `69b602c797a62f09318916d24a98c930533fbdc8`. The resolved graph no longer
  includes `async-std`, `async-global-executor`, or the old Zed `util` crate.
  This lowers the dependency-weight concern for keeping GPUI surfaces, but
  renderer replacement decisions still require paired runtime evidence.
- [x] P16cn: Move item-view scroll sync-action application into scroll state.
  `ItemViewScrollSyncAction::apply_to_view()` now owns when a sync action writes
  pane view values and whether that write represents a view change; `src/main.rs`
  only supplies the pane model write closure.
- [x] P16co: Move item-view handle-sync action composition into scroll state.
  `sync_view_from_handle_snapshot()` and
  `sync_view_from_authoritative_handle_snapshot()` now own handle action
  creation plus view-write application; `src/main.rs` only supplies pane view
  snapshots and the pane model write closure.
- [x] P16cp: Move item-view bounds-update and scrollbar-finish scroll action
  application into scroll state. Bounds and drag-finish paths now expose
  snapshot APIs that own action creation, handle-change aggregation, and
  view-write application while `src/main.rs` keeps only pane bounds updates and
  pane model write closures.
- [x] P16cq: Move item-view layout-change scroll preservation writeback into
  scroll state. `preserve_layout_scroll_syncing_view_snapshot()` now owns the
  preserved scroll calculation plus view-write application; `src/main.rs` only
  supplies the pane view snapshot and pane model write closure.
- [x] P16cr: Move item-view scroll snapshot tuple construction into the
  item-view module. Production paths now use
  `ItemViewScrollViewSnapshot::from_view_state()` instead of hand-copying
  `scroll_x`, `scroll_y`, `max_scroll_x`, and `max_scroll_y` in `src/main.rs`.
- [x] P16cs: Hide the internal item-view scroll sync calculation type from
  cross-module writeback. Public scroll-state writeback callbacks now receive
  `ItemViewScrollViewSnapshot`, while `ItemViewScrollSync` is private to
  `scroll_state.rs`.
- [x] P16ct: Narrow item-view handle-to-view snapshot helper visibility.
  `sync_handle_to_view_snapshot()` is now an internal scroll-state helper;
  cross-module paths use the authoritative, user-scroll, or transient-clearing
  policy APIs instead of the raw handle sync helper.
- [x] P16cu: Encapsulate item-view scroll snapshot writeback. The snapshot
  fields are now private to `scroll_state.rs`; `main.rs` writes pane scroll via
  `ItemViewScrollViewSnapshot::apply_scroll_writeback()` and a single pane
  adapter instead of repeatedly unpacking the scroll tuple.
- [x] P16cv: Route wheel-scroll change detection through the item-view scroll
  snapshot protocol. `scroll_pane_from_wheel()` now compares
  `ItemViewScrollViewSnapshot` values before/after pane model scrolling instead
  of open-coding the four-field scroll tuple in `src/main.rs`.
- [x] P16cw: Move the item-view scroll snapshot pane writeback adapter into the
  item-view module. `main.rs` now supplies `PaneController` and `PaneId` to
  `apply_item_view_scroll_snapshot_to_pane()` instead of owning the adapter
  logic that unpacks the item-view scroll snapshot.
- [x] P16cx: Move pane-to-item-view scroll snapshot projection into the
  item-view module. `item_view_scroll_snapshot_for_pane()` and
  `item_view_scroll_snapshot_for_existing_pane()` now own projecting pane
  `ViewState` into `ItemViewScrollViewSnapshot`, so `main.rs` no longer keeps
  its own pane snapshot helper.
- [x] P16cy: Hide direct item-view scroll snapshot construction from
  `main.rs`. Filter-bar priming now uses
  `item_view_scroll_snapshot_for_view()`, wheel scroll uses
  `changed_item_view_scroll_snapshot()`, and app tests reuse the pane snapshot
  projection instead of constructing `ItemViewScrollViewSnapshot` directly.
- [x] P16cz: Move normal item-view scroll-handle-to-pane sync orchestration into
  the item-view facade. `main.rs` now delegates the ordinary handle sync path to
  `sync_pane_view_from_item_view_scroll_handle()` with the scroll state,
  pane controller, and pane id instead of assembling the snapshot/writeback
  closure locally.
- [x] P16da: Move authoritative item-view scroll-handle-to-pane sync
  orchestration into the item-view facade. Scrollbar-drag update now delegates
  through `sync_pane_view_from_authoritative_item_view_scroll_handle()` instead
  of assembling the authoritative handle snapshot/writeback closure in
  `main.rs`.
- [x] P16db: Move item-view scrollbar finish sync orchestration into the
  item-view facade. `finish_item_view_scrollbar_drag()` now owns the existing
  pane snapshot lookup, missing-pane drag-finish fallback, and pane writeback
  closure; `main.rs` only delegates the public action.
- [x] P16dc: Move item-view layout-change scroll preservation orchestration into
  the item-view facade. Zoom/layout paths now delegate preserved-scroll
  snapshot lookup and pane writeback through
  `preserve_item_view_scroll_for_layout_change()` instead of assembling that
  closure in `main.rs`.
- [x] P16dd: Move item-view transient-clearing handle sync orchestration into
  the item-view facade. Loading transitions that preserve pane scroll now
  delegate handle sync and transient cleanup through
  `sync_item_view_scroll_handle_to_pane_view()` instead of looking up the pane
  snapshot and calling the scroll-state API directly in `main.rs`.
- [x] P16de: Move item-view bounds-update scroll sync orchestration into the
  item-view facade. `set_pane_viewport_bounds()` still writes viewport bounds
  through the pane controller, but subsequent handle/action sync and pane
  scroll writeback now go through `sync_pane_view_after_item_view_bounds_update()`.
- [x] P16df: Move item-view wheel-scroll orchestration into the item-view
  facade. `scroll_pane_from_wheel()` now delegates wheel axis mapping, pane
  model scroll, snapshot change detection, and user-scroll handle sync through
  `scroll_pane_from_item_view_wheel()`.
- [x] P16dg: Move item-view authoritative handle-to-view priming into the
  item-view facade. Filter-bar viewport priming now delegates through
  `sync_item_view_scroll_handle_to_view_authoritatively()` instead of
  constructing a scroll snapshot and calling the scroll-state API in `main.rs`.
- [x] P16dh: Move thin item-view scroll lifecycle entry points into the
  item-view facade. `main.rs` now delegates handle lookup, scrollbar-drag start,
  pane reset, and pane removal through item-view functions instead of calling
  `ItemViewScrollState` methods directly in production paths.
- [x] P16di: Move item-view scroll transient test inspections into the
  item-view facade. App-side tests now query authoritative-scroll and
  scrollbar-dragging state through item-view helpers instead of directly
  invoking `ItemViewScrollState` inspection methods from `main.rs`.
- [x] P16dj: Group rubber-band interaction state into a rubber-band controller.
  `main.rs` now holds one `RubberBandController` instead of separate
  pending-band, active-band, and selection-activity fields; viewport and app
  paths query/mutate rubber-band state through controller methods while keeping
  the existing GPUI drag shell boundary.
- [x] P16dk: Move rubber-band drag-move active/pending branching out of the
  viewport shell. The GPUI shell now forwards drag moves to
  `move_rubber_band_drag_from_window()`, while the app/controller boundary
  decides whether to activate a pending band or update the active band.
- [x] P16dl: Move visible file-icon sync handoff behind the file-grid retained
  facade. The render loop now calls a pane-level
  `resolve_visible_file_icons_for_raw_grid()` method; the Dolphin visible-icon
  sync budget, queue-aware cache sync, and visible snapshot invalidation stay in
  the file-grid module instead of `main.rs`.
- [x] P16dm: Move file-icon resolve worker orchestration into the file-grid
  retained facade. Batch startup, background icon resolution, queue completion,
  resolved icon application, visible snapshot invalidation, and continued batch
  scheduling now live with the file-grid icon work boundary instead of
  `main.rs`.
- [x] P16dn: Move metadata role worker orchestration into the file-grid
  retained facade. Metadata role batch startup, background role collection,
  scheduler completion, model result application, continued scheduling, and
  notification decisions now live beside visible metadata sync instead of
  `main.rs`.
- [x] P16do: Move thumbnail probe worker orchestration into the file-grid
  retained facade. Thumbnail probe batch startup, background cache probing,
  scheduler completion, model result application, continued scheduling, and
  notification decisions now live beside thumbnail result application instead
  of `main.rs`.
- [x] P16dp: Keep visible model work startup inside the file-grid retained
  facade. Queueing now returns the typed `QueuedVisibleModelWork` contract, and
  `main.rs` delegates worker startup instead of unpacking metadata, thumbnail,
  and file-icon booleans.
- [x] P16dq: Move visible metadata resnapshot orchestration into the file-grid
  retained facade. The render loop now asks for a raw grid that has already
  applied same-frame visible metadata role results and receives the updated
  model data generation, instead of rebuilding the raw grid from `main.rs`.
- [x] P16dr: Move visible icon sync, model-work queueing, and queued worker
  startup behind one file-grid retained facade entry. The render loop keeps the
  same icon-sync and queue perf fields, but no longer sequences the metadata,
  thumbnail, and icon worker controller steps directly.
- [x] P16ds: Move retained projection frame assembly into the file-grid
  retained facade. The facade now owns visible-count derivation, retained slot
  projection, paint-slot stats, and item-view perf phase recording; `main.rs`
  only consumes the frame for pane snapshots and perf log emission.
- [x] P16dt: Record the GPUI scheduling dependency boundary after the 2026-06
  dependency update. The design now notes that `async-std` and
  `async-global-executor` are gone, while GPUI/platform async support crates
  still exist, and item-view worker orchestration should stay behind
  file-grid/places facades instead of returning to `main.rs`.
- [x] P16du: Collapse the raw/work/projection item-view render pipeline into a
  pane-level file-grid render frame. `main.rs` now receives file-grid snapshot,
  item/visible counts, slot stats, perf phase, and timing fields as one facade
  result instead of holding raw grid and model-generation intermediates.
- [x] P16dv: Hide item-view perf log field mapping inside the file-grid render
  frame. `main.rs` now passes only pane id, mode, and total pane elapsed time;
  raw/icon/queue/convert timings, visible count, perf phase, and slot stats stay
  encapsulated in the frame.
- [x] P16dw: Move the same-visible-work-range resize queue invariant out of
  app-side tests and into the file-grid snapshot scheduler tests. The raw
  snapshot/queue protocol is now covered where the work key and scheduler
  contract are owned, instead of requiring `main.rs` tests to call low-level
  file-grid methods.
- [x] P16dx: Advance the Places custom row visual layer with visible-row
  filtering, but do not make it the default yet. Root cause: the aggregated
  Places row visual layer used one canvas, but the overflow scenario still
  shaped and painted all 75 rows every frame. Implementation: the prepaint path
  now uses GPUI `Window::content_mask()` to keep only rows intersecting the
  current scroll clip; `[fika places-row-visual]` keeps total `rows` and adds
  `painted`, and the analyzer summarizes `max_painted`. Evidence:
  `/tmp/fika-places-custom-targets-visible-rows.log` passes the targets custom
  policy gate, and `/tmp/fika-places-custom-overflow-visible-rows.log` passes
  the overflow custom policy gate. Overflow drops from painting all 75 rows to
  at most 32 painted rows, with steady paint around `0.6-0.7ms`. It is still not
  default because the first two frames show roughly `7-8ms` glyph/text cold-start
  paint spikes; the next step must eliminate that spike or prove it neutral
  against the GPUI baseline.
- [x] P16dy: Make the Dolphin-aligned Places custom chrome policy the default
  while keeping full custom text opt-in. Root cause: Dolphin's high-performance
  item view recycles visible widgets and relies on static text/pixmap caches;
  Fika's full Places canvas text path still pays glyph/text cold-start costs.
  Implementation: `FIKA_PLACES_ROW_VISUAL_POLICY` now supports `gpui`,
  default `chrome`, and `full`; chrome paints row background/drop/insert/trash
  in one visible-row-filtered layer while GPUI keeps text and icons. The
  analyzer now has `--expect-custom-row-chrome-policy`, tracks `text_gpui` and
  `visual_kind`, and rejects row shape-cache logs for chrome. Evidence:
  `/tmp/fika-places-chrome-targets.log`, `/tmp/fika-places-chrome-overflow.log`,
  `/tmp/fika-places-chrome-layout.log`, and `/tmp/fika-places-chrome-hit-test.log`
  pass the chrome gates; `/tmp/fika-places-gpui-targets.log` passes the GPUI
  fallback gate; `/tmp/fika-places-full-targets.log` passes the full custom-text
  gate and remains opt-in because it shows `max_paint=5183us` with shape-cache
  activity compared with chrome `max_paint=83us` targets and `148us` overflow
  with no shape-cache channel.
- [x] P16dz0: Add the first opt-in full Places row visual path with GPUI icon
  removal evidence. Root cause: the previous `full` wording only meant
  text-only custom painting; Places icons still used GPUI row elements. The
  renderer policy now distinguishes `chrome`, `text`, and `full`; `full`
  custom-paints row text plus vector fallback icons in the existing aggregated
  sidebar layer and emits `icon_gpui=0`. Evidence:
  `/tmp/fika-places-full-icon-targets.log` passes
  `--expect-custom-row-full-policy` with `max_icon_gpui=0`, while default
  `/tmp/fika-places-chrome-after-full-icon.log` still passes the chrome gate.
  The full path is not default: vector icons removed the extra icon text cost,
  but full text painting still shows cold paint spikes (`max_paint=5669us`)
  versus default chrome's microsecond-level row visual paint (`max_paint=63us`).
  Next gate: solve Places custom text cold-start/warmup or keep GPUI text as the
  Dolphin-aligned default boundary.
- [x] P16dz1: Separate Places full custom-text cold paint from warm paint and
  remove the per-label clip layer in the opt-in path. Root cause: `max_paint`
  alone mixed the first two glyph/text paint frames with steady-state row
  painting, making it hard to judge whether the path was fundamentally slow or
  cold-start bound. Implementation: `scripts/analyze-places-perf.sh` now reports
  `warm_frames`, `max_warm_prepaint`, and `max_warm_paint` after skipping the
  first two `[fika places-row-visual]` frames; the opt-in Places custom-text
  painter now paints `ShapedLine`s directly with a max width instead of wrapping
  every label in its own `paint_layer`. Evidence:
  `/tmp/fika-places-full-direct-text.log` passes `--expect-custom-row-full-policy`
  with `max_icon_gpui=0`, `max_paint=5941us`, and `max_warm_paint=667us`;
  default `/tmp/fika-places-chrome-direct-text-check.log` passes the chrome gate
  with `max_warm_paint=48us`. Conclusion: full Places visual remains opt-in;
  the next performance target is the first two glyph/text paint frames, not row
  model, hit testing, icon drawing, or steady-state canvas paint.
- [x] P16dz2: Add explicit Places row-visual paint promotion gates and record
  the Dolphin-aligned ownership rule. Root cause: full Places visual can now
  remove GPUI icon/text row elements, but passing `icon_gpui=0` is not enough;
  it must also prove cold and warm row visual paint are both acceptable. This is
  not waiting for a special GPUI prewarm API. Dolphin's cache behavior is an
  application-level design: stable item identity, retained/static text and
  pixmap state, and a handoff only after the resource is ready. Implementation:
  `scripts/analyze-places-perf.sh` accepts `--row-visual-paint-us` and
  `--row-visual-warm-paint-us`; `scripts/check-places-perf-analyzer.sh` covers
  a synthetic case where warm paint passes but cold paint fails. Next gate:
  implement Fika-owned retained Places text/image handoff so full custom rows
  can pass both thresholds before default promotion.
- [x] P16dz3: Audit GPUI's efficient `img()` path and record the custom-image
  design rule. Root cause: GPUI image performance is not a hidden synchronous
  drawing primitive; it comes from retained resource identity and deferred
  atlas-backed painting. Implementation finding: `img()` resolves `Resource`
  values through `ImageCache`; `RetainAllImageCache` keeps either a shared
  background loading task or loaded `Arc<RenderImage>` keyed by resource hash
  and notifies on the next frame; `Window::paint_image` uses
  `(RenderImage.id, frame_index)` as the sprite atlas key. Follow-up: custom
  Places/image work must use stable semantic keys, retained loaded resources,
  visible-path-free decode/shape replacement, and ready-only handoff before it
  can outperform the current GPUI image baseline.
- [x] P16dz4: Add the first opt-in ready-only handoff for full custom Places
  rows. Root cause: the previous full path hid GPUI text/icons immediately, so
  the first visible custom frame paid text/glyph cold paint while the user saw
  the transition. Implementation: `FIKA_PLACES_ROW_VISUAL_HANDOFF=1` keeps GPUI
  text/icons for two warmup frames while the custom layer paints chrome only,
  then switches to full custom text+icon paint once the retained row visual
  path is ready. `scripts/analyze-places-perf.sh` now has
  `--expect-custom-row-handoff-policy` and emits `[fika places-row-handoff]`
  summaries; the analyzer fixture covers both successful fallback-to-ready
  handoff and a missing-ready failure. Evidence:
  `/tmp/fika-places-full-handoff.log` passes
  `--expect-custom-row-handoff-policy --row-visual-paint-us 1000
  --row-visual-warm-paint-us 1000`; first fallback frames paint chrome at
  roughly `50-59us`, ready full-custom frames paint at roughly `230-286us`, and
  the old 5-6ms full cold paint spike is no longer on the visible handoff path.
  Remaining work before default promotion: reduce the ready-frame text-shape
  prepaint miss (`max_prepaint=1175us` in this run), cover overflow/layout
  handoff evidence, and extend the same stable-key/ready-only pattern to real
  image resources instead of only fallback vector Places icons.
- [x] P16dz5: Use the handoff warmup frames to prefill retained Places row text
  shapes and gate row-visual prepaint. Root cause: P16dz4 moved the visible
  text/glyph paint spike out of the handoff path, but the first ready frame
  still paid a `PlacesRowTextShapeCache` miss in prepaint. Implementation:
  `places_row_visual_layer` now accepts a `warm_text_shapes` input; while
  `FIKA_PLACES_ROW_VISUAL_HANDOFF=1` is in fallback mode it keeps GPUI
  text/icons visible, paints only chrome, and shapes visible labels into the
  app-owned cache without painting them. `scripts/analyze-places-perf.sh` now
  adds `--row-visual-prepaint-us` and `--row-visual-warm-prepaint-us` gates.
  Evidence: `/tmp/fika-places-full-handoff-prewarm.log` passes
  `--expect-custom-row-handoff-policy --row-visual-prepaint-us 300
  --row-visual-paint-us 1000 --row-visual-warm-prepaint-us 100
  --row-visual-warm-paint-us 1000`, with `max_prepaint=113us`,
  `max_warm_prepaint=54us`, and `max_warm_paint=282us`. Overflow and layout
  smokes also pass handoff gates: `/tmp/fika-places-full-handoff-overflow.log`
  has `max_painted=29`, `max_warm_prepaint=77us`, `max_warm_paint=1058us`;
  `/tmp/fika-places-full-handoff-layout.log` has `max_warm_prepaint=47us` and
  `max_warm_paint=282us`. Remaining work before default promotion: decide
  whether overflow full-custom text paint around 1ms is acceptable against
  chrome/default evidence, then apply the same ready-only retained-resource
  model to real image resources.
- [x] P16dz6: Capture paired default-chrome evidence for the full Places
  handoff decision. Root cause: after P16dz5, full handoff no longer has cold
  prepaint/paint spikes, but promotion still needs a paired comparison against
  the current default. Evidence: default chrome runs
  `/tmp/fika-places-chrome-targets-compare.log`,
  `/tmp/fika-places-chrome-overflow-compare.log`, and
  `/tmp/fika-places-chrome-layout-compare.log` passed
  `--expect-custom-row-chrome-policy`. Compared to handoff logs, row-visual
  paint remains much lower in chrome: targets `85us` vs full handoff `282us`,
  layout `64us` vs `282us`, and overflow `154us` vs `1058us` for 29 painted
  rows. Decision: keep full Places handoff opt-in for now. This does not
  invalidate the custom-renderer direction because chrome row-visual metrics do
  not include the GPUI text/icon subtree cost; the next evidence step is either
  total render/chrome-vs-full accounting or further retained custom text paint
  optimization before default promotion.
- [x] P16dz7: Add total render accounting to the Places analyzer and capture
  the first chrome-vs-full overflow render pair. Root cause: row-visual-only
  evidence showed full custom text paint much higher than chrome, but chrome's
  row visual layer intentionally excludes GPUI text/icon subtree work. The
  analyzer now parses `[fika render]` lines and supports `--render-total-us`.
  Evidence: with both `FIKA_PERF_ITEM_VIEW=1` and `FIKA_PERF_PLACES_VIEW=1`,
  `/tmp/fika-places-chrome-overflow-render.log` and
  `/tmp/fika-places-full-handoff-overflow-render.log` both pass their policy
  gates. Row visual still favors chrome (`max_warm_paint=153us`) over full
  handoff (`max_warm_paint=2041us` in this run), but ready-frame total render
  does not scale with that gap: chrome steady frames reached `2035-4959us`,
  while full handoff ready frames were `1463-2130us`. Decision: keep full
  handoff opt-in until more repeated total-render evidence exists, but use total
  render accounting as the promotion metric alongside row visual
  prepaint/paint.
- [x] P16dz: Add the post-Places-chrome full retained renderer roadmap. The new
  `docs/FULL_RETAINED_RENDERER_ROADMAP.md` and zh-CN translation define the
  current baseline, explicit GPUI bridges, non-negotiable Dolphin-aligned
  rules, and six execution tracks: evidence freeze, MIME/theme icon renderer,
  Places retained event delivery, drag-start boundary, rename editor, and
  ownership cleanup. This gives future implementation slices one planning entry
  point before continuing the broader transition.
- [x] P16ea: Add the retained MIME/theme icon image cache design. The new
  `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md` and zh-CN translation define the
  Dolphin `QPixmapCache` comparison, conservative `ThemeIconImageKey`, retained
  same-key loaded/pending/failed/stale image states, ownership boundary, paired
  default-vs-custom runtime evidence, and TODO gates required before custom
  theme-icon painting can become default.
- [x] P16eb: Implement the retained MIME/theme icon image cache foundation.
  `src/ui/icons/image_cache.rs` now owns `ThemeIconImageKey`,
  `RetainedThemeIconImageCache`, and loaded/pending/failed/stale status. The
  custom image layer keeps thumbnails keyed by thumbnail path but routes
  theme/MIME icons through a size/scale-aware key, including Details visual
  icons. Root cause: the old custom A/B path retained theme images by
  `iconName` only, so zoom could reuse an old-size image before the current-size
  image loaded. Default MIME/theme icons still use GPUI `img()` until paired
  evidence proves the custom path is neutral or better.
- [x] P16ec: Add the paired item-image default-promotion gate. The
  `scripts/compare-item-image-renderers.sh --gate-default-promotion` mode now
  exits non-zero if the custom log has theme placeholders, theme decode churn,
  missing custom item-image frames, or invalid default/custom renderer-policy
  evidence. `scripts/check-item-view-perf-analyzer.sh` covers both failing and
  passing synthetic comparisons. Real `/etc` and mixed-directory runtime
  evidence remains P16k2.
- [x] P16ed: Capture the first real `/etc` default-vs-custom P16k2 evidence
  after the retained theme image key landed. Default:
  `/tmp/fika-icon-default-etc-p16k2.log`; custom:
  `/tmp/fika-icon-custom-etc-p16k2.log`. The default-promotion gate correctly
  failed because custom still produced `theme_placeholder=118` and
  `theme_decoded=5`, despite valid custom/default renderer-policy evidence.
  This confirms the next architecture step is prewarming or hybrid delivery
  before default promotion, not switching ordinary MIME/theme icons fully to the
  custom image layer yet.
- [x] P16ee: Add opt-in theme-icon prewarm telemetry and runtime evidence.
  `FIKA_PREWARM_THEME_ICONS=1` adds non-painting image-layer prewarm items for
  GPUI-rendered theme icons and extends `[fika item-image]` with
  `theme_prewarm_loaded`, `theme_prewarm_decoded`, `theme_prewarm_retained`, and
  `theme_prewarm_pending`. `/tmp/fika-icon-prewarm-etc-p16k2.log` proves the
  bridge keeps default GPUI renderer policy and does not expose custom
  placeholders (`theme_placeholder=0`, `paint_count=0`) while warming retained
  images. This is still an intermediate bridge, not a default promotion.
- [x] P16ef: Add the paired hybrid handoff gate. The
  `scripts/compare-item-image-renderers.sh --gate-hybrid-handoff` mode now
  fails unless the candidate log shows GPUI fallback, prewarm activity,
  ready-key image-layer paint, and no visible theme placeholder/decode churn.
  `scripts/check-item-view-perf-analyzer.sh` covers passing and failing
  synthetic hybrid comparisons; real `/etc` and mixed-directory promotion
  evidence remains tracked by P16k2/P16k2a.
- [x] P16eg: Align zoom-time MIME/theme icon path identity with Dolphin's stable
  `iconName` role. Root cause: the old `FileIconCacheKey` included `size_px` in
  the exact key, so zoom could create a new exact-size request even after the
  same file-icon kind already had a resolved path. That could cause visible
  path lookup, a second GPUI image identity commit, and a perceived icon-size
  jump. Implementation: `FileIconCache::resolve_request_for()` and
  `resolve_now_for()` now treat any resolved same-`FileIconKind` path as cached;
  visible icon sync counts a missing request as cached and skips synchronous
  resolve; exact keys that resolved to no path are also treated as completed so
  negative theme lookups do not repeat; `find_icon_direct()` skips missing
  directories and uses one `metadata` call for file/length checks to reduce
  theme-miss syscalls.
  Verification: `cargo fmt --check`, `cargo check`, `cargo build`,
  `cargo test -q`, `scripts/check-item-view-perf-analyzer.sh`, and
  `scripts/check-places-perf-analyzer.sh` pass. The current automation
  environment has no Wayland compositor, so `/etc` runtime autosmoke hit GPUI
  `NoCompositor`; refresh real logs in a desktop session.
- [x] P16eh: Add the implementation-level Places retained event-delivery plan.
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md` and the Chinese translation now
  define the Dolphin boundary, the current GPUI-shell policy, the target
  retained-hitbox policy, the sidebar-level event layer, the scroll-local
  coordinate rule, phased migration order, analyzer/smoke requirements, and
  TODOs. The plan keeps row drag-start shells on GPUI until Track 4 and makes
  the next implementation slice an opt-in retained hitbox layer with no
  behavior change.
- [x] P16ei: Add the first Places event-delivery policy implementation slice.
  `PlacesEventDeliveryPolicy` now defaults to `GpuiShells` and supports
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe`. The probe reports
  `retained_probe_hitboxes=rows+sections` in renderer/interaction policy logs
  while keeping `retained_hitboxes=0` and `gpui_event_shells=rows+sections`, so
  it cannot satisfy the future retained-event gate. This records the Dolphin
  conclusion that full custom Places performance requires viewport-level event
  ownership, not only row chrome paint.
- [x] P16ej: Add the non-mutating Places retained event probe layer. The opt-in
  layer consumes `PlacesInteractionGeometry`, inserts normal row/section
  hitboxes with `Window::insert_hitbox()`, and reports
  `[fika places-event-probe]` without registering event handlers or changing
  app state. The analyzer now has `--require-event-probe`, proving the inserted
  hitbox count matches `retained_probe_hitboxes` while the retained-event gate
  still rejects this mixed GPUI-shell policy.
- [x] P16ek: Add the first retained-pointer Places event slice. The opt-in
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-pointer` policy reuses the
  retained event layer to set row pointer cursors and clear active Places drop
  targets when a drag leaves the retained layer bounds. Row shell cursor styling
  is disabled in that policy, but GPUI row/section shells still own click,
  context menu, typed DnD move/drop, and drag start. `[fika places-event-probe]`
  now reports `pointer=1` for this mixed state and the retained-event gate still
  rejects it.
- [x] P16el: Add the retained-targeting Places event slice. The opt-in
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-targeting` policy keeps using the
  sidebar retained event layer, but now row activation and row/section context
  menu targeting are dispatched from retained row/section hitboxes. GPUI row
  `on_click`, row right-click, and section right-click shells are disabled in
  that policy. Typed DnD move/drop and row drag-start remain on GPUI shells, so
  the analyzer records `retained_targeting=rows+sections` and
  `pointer=1 targeting=1` while the full retained-event gate still rejects the
  mixed state.
- [x] P16em: Add the retained-DnD Places event slice. The opt-in
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-dnd` policy uses one
  sidebar-level GPUI typed drag shell, because GPUI exposes app-internal typed
  drag payloads through `Div::on_drag_move` / `Div::on_drop`. Row/section DnD
  move/drop shells are disabled in that policy, and retained
  `PlacesInteractionGeometry` owns row/section target lookup for item,
  external-path, and place drags. Row drag-start remains on GPUI shells. The
  analyzer records `retained_dnd=rows+sections`, `gpui_event_shells=1`, and
  `pointer=1 targeting=1 dnd=1`; the full retained-event gate still rejects the
  mixed state.
- [x] P16en: Add non-destructive retained Places DnD autosmoke. The
  `FIKA_AUTOSMOKE_PLACES=dnd` scenario now emits retained target-decision
  samples for path-list drags over row body, row edge, and section heading, plus
  a place drag over another row. `scripts/analyze-places-perf.sh` supports
  `--require-retained-dnd-autosmoke`, and
  `scripts/check-places-perf-analyzer.sh` covers both valid markers and an
  invalid failed-decision fixture. This proves the Dolphin-style retained
  geometry/controller decision boundary without mutating user Places ordering.
  Evidence: `/tmp/fika-places-retained-dnd.log` passed
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`.
- [x] P16eo: Move Places drag-start source modeling out of the row shell. The
  GPUI platform boundary still requires row `Div::on_drag`, but
  `src/ui/places/drag.rs` now owns `PlaceDragStartSource` projection from
  `PlaceSnapshot`, including path, label, icon, source index, movable flag,
  export payload, and preview model. `[fika places-interaction-policy]` now
  reports `drag_start_models=rows`, and the Places analyzer rejects interaction
  logs where the model count does not match visible rows. This keeps the
  Dolphin-style source model boundary explicit while drag-start shells remain.
  Evidence: `/tmp/fika-places-drag-start-model.log` passed
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`
  with `max_drag_start_models=11`.
- [x] P16ep: Centralize the remaining Places GPUI drag-start shell installer.
  Row construction now calls `install_place_drag_start_shell()` from
  `src/ui/places/drag.rs` instead of installing `Div::on_drag` and constructing
  `PlaceDragPreview` inline. This keeps the platform shell explicit while
  payload projection, preview construction, and GPUI drag-start wiring share the
  same owned drag module boundary. Evidence:
  `/tmp/fika-places-drag-start-model.log` passed
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`.
- [x] P16eq: Add retained Places content-y conversion and boundary tests.
  `places_content_y_from_viewport_y()` now centralizes the viewport-local y plus
  scroll offset conversion that feeds retained hit testing, and unit coverage
  proves non-zero scroll maps to the expected row/section while row, section,
  and content bounds remain half-open. This keeps the future viewport-level
  event layer from regressing drop/activation targets when it no longer lives
  inside scroll content.
- [x] P16er: Distinguish retained probe hitboxes from retained target-delivery
  hitboxes. `retained_probe_hitboxes` remains the inserted retained layer count,
  while `retained_hitboxes` now becomes rows+sections only for
  `retained-targeting` and `retained-dnd`, where row/section hitboxes actually
  dispatch targets. The full retained-event gate is unchanged and still rejects
  those mixed states until `gpui_event_shells=0`. Evidence:
  `/tmp/fika-places-hitbox-accounting.log` passed
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`
  with `max_retained_hitboxes=13`, while
  `--expect-retained-event-policy` still failed as expected.
- [x] P16es: Make Places renderer retained-interaction accounting event-policy
  aware. `PlacesEventDeliveryPolicy::retained_interaction()` now reports
  rows+sections for `retained-targeting` and `retained-dnd`, where the retained
  event layer actually owns row/section target delivery, while probe and
  pointer-only policies continue to report zero. The Places analyzer validates
  custom chrome/full visual policy against that event-policy-aware count, but
  the full retained-event gate still rejects `retained-dnd` until
  `gpui_event_shells=0`.
- [x] P16et: Add non-mutating retained Places targeting autosmoke. The
  `FIKA_AUTOSMOKE_PLACES=targeting` scenario now samples retained
  activation-row, row context-menu, and section context-menu target
  classification from `PlacesInteractionGeometry` without activating places or
  opening menus. `scripts/analyze-places-perf.sh` now supports
  `--require-retained-targeting-autosmoke` and rejects missing or failed
  targeting samples before any retained-targeting default promotion.
- [x] P16eu: Promote Places event delivery default to the retained-DnD mixed
  policy. `places_event_delivery_policy()` now falls back to `RetainedDnd`,
  while `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` remains the explicit GPUI
  row/section event-shell fallback. Default logs are expected to show
  `event_policy=retained-dnd`, `retained_hitboxes=rows+sections`,
  `gpui_event_shells=1`, and `drag_start_models=rows`; the full retained-event
  analyzer gate remains intentionally failing until the sidebar typed DnD shell
  can be removed.
- [x] P16ev: Remove the redundant root sidebar GPUI leave-clear shells from
  retained pointer policies. The retained event layer already clears active
  Places drop targets when an active drag leaves its bounds, so retained-pointer,
  retained-targeting, and retained-DnD no longer install the item,
  external-path, and place root sidebar `on_drag_move` leave handlers. GPUI and
  probe policies keep those three fallback shells. The interaction policy log
  now reports `gpui_sidebar_leave_shells`, and the analyzer rejects
  retained-DnD logs that reintroduce them without loosening the full
  retained-event gate.
- [x] P16ew: Split the remaining Places GPUI event-shell accounting into
  row/section event shells and the sidebar typed DnD payload shell. The
  interaction policy log now reports `gpui_row_section_event_shells` and
  `gpui_typed_dnd_payload_shells` in addition to the total
  `gpui_event_shells`. Default retained-DnD must show
  `gpui_row_section_event_shells=0` and `gpui_typed_dnd_payload_shells=1`,
  proving row/section target delivery is retained while the typed payload entry
  point remains a GPUI platform boundary. The full retained-event gate still
  requires both split counters to be zero.
- [x] P16ex: Re-audit the GPUI drag-start API after the dependency update.
  Current GPUI `0.2.2` at Zed
  `69b602c797a62f09318916d24a98c930533fbdc8` still exposes typed drag start
  through interactive elements, not retained painter hitboxes. Track 4 now
  records the minimum audited patch/API shape needed before removing
  Compact/Icons, Details, or Places drag-start shells: payload, preview entity,
  cursor offset, transfer modes, cancellation, same-window drop dispatch, and
  external drop behavior must all survive without recreating visual GPUI rows
  as drag sources.
- [x] P16ey: Add the Track 1 retained-renderer evidence checklist. The new
  `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.md` and Chinese translation define
  the desktop-session commands, `/tmp` log names, analyzer gates, image A/B
  gates, Places retained-DnD expectations, manual DnD/rename smoke reminders,
  and recording rule required before promoting a custom renderer or removing a
  GPUI bridge.
- [x] P16ez: Add a retained-renderer evidence runner. The new
  `scripts/run-retained-renderer-evidence.sh` automates the core Track 1 item
  and Places desktop-session captures, runs the matching analyzer gates, and
  verifies that the current Places full-retained gate still fails until the
  typed DnD payload shell is removed. MIME/theme icon A/B evidence is available
  behind `--icons` so the current non-promotable custom icon path does not block
  every baseline freeze.
- [x] P16fa: Document the cross-surface Dolphin retained-renderer alignment
  contract. `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md` records why full
  custom rendering remains theoretically valid, why the current gaps are
  incomplete model/cache/event loops rather than proof that GPUI is inherently
  faster, and which model, layout, role-readiness, painter, controller and
  analyzer gates must be satisfied before a GPUI bridge can be removed or a
  custom renderer can become default.
- [x] P16fb: Split the retained-renderer evidence runner into narrow item-only
  and Places-only modes. `scripts/run-retained-renderer-evidence.sh
  --items-only` now captures only the item-view Track 1 logs, while
  `--places-only` captures only the Places targets/overflow/layout/hit-test/
  targeting/DnD logs and still verifies the full retained-event gate fails as
  expected. The default `--core` behavior remains item plus Places.
- [x] P16fc: Add a hybrid MIME/theme icon evidence runner mode.
  `scripts/run-retained-renderer-evidence.sh --hybrid-icons` captures paired default and
  `FIKA_HYBRID_THEME_ICONS=1` zoom-scroll logs for `/etc` and the mixed user
  directory, then runs `scripts/compare-item-image-renderers.sh
  --gate-hybrid-handoff`. This makes the next image-readiness step measurable
  without forcing the current non-promotable full custom icon path through
  `--gate-default-promotion`.
- [x] P16fd: Make retained-renderer evidence runner selection explicit.
  The script now enables core item+Places capture only when no selection
  option is passed, so `--hybrid-icons` by itself runs only the hybrid icon
  handoff evidence while `--core --hybrid-icons` still runs both groups.
- [x] P16fe: Capture paired hybrid MIME/theme icon evidence for `/etc` and the
  mixed user directory. `scripts/run-retained-renderer-evidence.sh
  --hybrid-icons --skip-build --prefix fika-hybrid-icons-20260619` passed
  `--gate-hybrid-handoff` for both `/etc` and Downloads with
  `theme_placeholder=0` and visible `theme_decoded=0`. The result supports
  continuing toward a default hybrid renderer; P16ff adds the stricter gate and
  P16fg switches the default.
- [x] P16ff: Add a strict hybrid icon default-promotion gate.
  `scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion`
  now extends the handoff gate with explicit tolerances for `icon_sync`,
  item-view phase max totals, static visual prepaint/paint, and image paint
  against the GPUI baseline. The 2026-06-19 `/etc` and Downloads hybrid logs
  both pass this stricter gate, so the next code slice can attempt a default
  hybrid renderer policy and rerun the same gate.
- [x] P16fg: Switch ordinary MIME/theme icons to the hybrid renderer by
  default. `FIKA_GPUI_THEME_ICONS=1` now forces the previous GPUI `img()`
  baseline, while the default path keeps not-yet-ready theme-icon keys on GPUI
  and hands ready keys to the retained custom image layer. The evidence runner
  now uses `FIKA_GPUI_THEME_ICONS=1` for baseline logs and no hybrid env for the
  default candidate. Evidence:
  `scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-default-20260619`
  passed `--gate-hybrid-default-promotion` for `/etc` and Downloads.
- [x] P16fh: Synchronize top-level roadmap and status docs after the default
  hybrid switch. `docs/FULL_RETAINED_RENDERER_ROADMAP.md`,
  `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`, and
  `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md` now describe the current MIME/theme
  icon state as hybrid-by-default with `FIKA_GPUI_THEME_ICONS=1` as the baseline
  override.
- [x] P16fi: Record the remaining Places typed DnD payload bridge boundary.
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md` now distinguishes default
  row/section event-callback removal from the still-required sidebar typed
  payload bridge. Default retained-DnD must show
  `gpui_row_section_event_shells=0` and `gpui_typed_dnd_payload_shells=1`.
  The latter remains an audited GPUI API boundary until retained hitboxes can
  deliver typed `ItemDrag`, `ExternalPaths`, and `PlaceDrag` move/drop payloads
  and the full retained-event analyzer plus isolated DnD smoke pass.
- [x] P16fj: Re-audit GPUI typed drag-move/drop delivery after the dependency
  update. Current `Cargo.lock` resolves GPUI to Zed
  `69b602c797a62f09318916d24a98c930533fbdc8`; `DragMoveEvent<T>`,
  `Interactivity::on_drag_move<T>()`, and `Interactivity::on_drop<T>()` remain
  interactive-element APIs, while `Window::insert_hitbox()` and
  `Window::on_mouse_event<Event: MouseEvent>()` still do not expose typed drag
  payloads for retained painter hitboxes. This confirms the Places sidebar
  typed payload bridge is still an API boundary rather than removable
  row/section shell debt.
- [x] P16fk: Extend Track 4 into a retained typed drag API design. The roadmap
  now treats drag start and typed drag-move/drop payload delivery as one GPUI
  boundary family. The minimal patch shape is split into retained hitbox drag
  source registration and retained hitbox drag target callbacks, both keyed by
  retained `HitboxId` and explicitly forbidden from recreating visible GPUI
  rows/items as replacement shell owners.
- [x] P16fl: Record the drag-time pane/Places target isolation regression.
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md` now documents that GPUI typed
  `on_drag_move` capture handlers are not automatically clipped to element
  bounds. The retained Places typed payload bridge must bounds-gate move events,
  clear only Places state when the pointer leaves Places, and avoid clearing
  pane item targets owned by pane preview/window drag tracking.
- [x] P16fm: Add the Places drag-bounds debug trace to the evidence checklist.
  `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.md` now says manual
  `FIKA_DEBUG_DND=1` pane-drag traces may include `places-dnd-leave`; that line
  proves the Places typed bridge rejected an out-of-bounds capture drag move,
  and it must not coincide with persistent Places highlight while the pointer
  is inside a pane.
- [x] P16fn: Extend retained Places DnD autosmoke with a no-target clear path.
  `FIKA_AUTOSMOKE_PLACES=dnd` now emits a `path-outside` sample expecting
  `Clear`/`NotAllowed`, and `scripts/analyze-places-perf.sh
  --require-retained-dnd-autosmoke` requires it. This guards drag-time target
  isolation in the unattended smoke while the manual `places-dnd-leave` trace
  remains the GUI bounds proof.
- [x] P16fo: Synchronize top-level Places status after the retained-DnD default.
  `docs/FULL_RETAINED_RENDERER_ROADMAP.md`,
  `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`, and
  `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md` now describe Places as custom
  row chrome plus retained row/section target delivery, with only text/icons,
  the sidebar typed payload bridge, and row drag-start shells remaining on
  GPUI. The full retained Places gate still requires typed payload and
  drag-start boundary removal.
- [x] P16fp: Synchronize image decisions after the hybrid icon default.
  `docs/ITEM_VIEW_RENDERER_DECISIONS.md` and
  `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md` now treat hybrid as the current
  default, `FIKA_GPUI_THEME_ICONS=1` as the old GPUI baseline, and
  `FIKA_CUSTOM_THEME_ICONS=1` as the full-custom stress path. The remaining
  image TODO is to keep hybrid default until a future full-custom run beats the
  hybrid/default gate on `/etc` and a mixed directory without placeholder,
  zoom-decode, image-paint, or renderer-policy regressions.
- [x] P16fq: Rename item-image comparison output from default log to baseline
  log. `scripts/compare-item-image-renderers.sh` now describes its second log
  as `BASELINE_LOG`, which matches the current hybrid default workflow where
  `FIKA_GPUI_THEME_ICONS=1` supplies the GPUI image-element baseline.
- [x] P16fr: Record pane/Places drag-target ownership during active drags.
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md`,
  `docs/DRAG_DROP_REFERENCE.md`, and
  `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.md` now state that a retained
  Places typed payload bridge must defer to pane viewport ownership when the
  pointer is inside a pane. The required runtime trace is
  `places-dnd-defer-to-pane`, and stale Places highlights must not survive
  while pane retained hit testing owns an item drop target.
- [x] P16fs: Move item-view autosmoke runner ownership into the file-grid
  facade. `src/ui/file_grid/autosmoke.rs` now owns the async
  `FIKA_AUTOSMOKE_ITEM_VIEW` action loop and marker emission, while
  `src/main.rs` only reads the selected scenario and triggers the facade. This
  keeps runtime evidence collection with the item-view controller/perf surface
  instead of leaving action orchestration in the app root.
- [x] P16ft: Move Places autosmoke runner and action application into the
  Places facade. `src/ui/places/autosmoke.rs` now owns the async
  `FIKA_AUTOSMOKE_PLACES` loop, target/layout action dispatch, settings
  verification marker emission, and retained targeting/DnD smoke calls.
  `src/main.rs` only reads the selected scenario and triggers the facade, so
  Places runtime evidence collection stays with the Places projection and
  interaction modules.
- [x] P16fu: Move the Places autosmoke sidebar layout mutation helper into the
  Places facade. `src/ui/places/autosmoke.rs` now owns the smoke-only
  width/visibility update path used by layout evidence, while `src/main.rs`
  keeps the regular sidebar commands and settings persistence scheduler as app
  coordination. This reduces app-root knowledge of Places evidence mechanics
  without changing persisted sidebar behavior.
- [x] P16fv: Move regular Places sidebar width/visibility commands into the
  Places sidebar facade. `src/ui/places/sidebar.rs` now owns toggling,
  resetting, drag-resizing, clamping, and app-settings save handoff for sidebar
  layout state. `src/main.rs` still stores the app-level persisted fields and
  calls the facade from shortcuts/render closures, but no longer owns the
  sidebar mutation policy.
- [x] P16fw: Move Places snapshot projection orchestration into the Places
  projection facade. `src/ui/places/projection.rs` now owns the app-facing
  `place_snapshots()` method, including active-place projection, hidden-place
  filtering, autosmoke extra rows, paint-slot projection, and Places snapshot
  perf logs. `src/main.rs` still asks for snapshots during render but no longer
  wires Places projection internals or evidence emission directly.
- [x] P16fx: Move app-facing removable-device Places update methods into the
  Places devices facade. `src/ui/places/devices.rs` now owns
  `finish_device_refresh()`, `apply_device_snapshot()`, and the
  `replace_removable_device_places()` app method, while `src/main.rs` keeps the
  monitor scheduling/draining loop. The low-level device-section replacement
  helper is no longer re-exported through the top-level Places facade.
- [x] P16fy: Move app-facing hidden Places visibility commands into the Places
  visibility facade. `src/ui/places/visibility.rs` now owns
  `hide_place()`, `hide_place_section()`, and `show_hidden_places()` app
  methods plus status updates, while the low-level visibility helpers stay
  private to the Places visibility module instead of being re-exported through
  the top-level Places facade.
- [x] P16fz: Move app-facing user-place persistence wrappers into the Places
  user facade. `src/ui/places/user.rs` now owns `user_places()` and
  `save_user_places()` on `FikaApp`, including user bookmark export and primary
  place order persistence. `src/main.rs` still calls the save facade from
  add/edit/remove/reorder paths but no longer owns the persistence wiring.
- [x] P16gaa: Move app-facing user-place removal into the Places user facade.
  `src/ui/places/user.rs` now owns `remove_place()` on `FikaApp`, including
  removable-place validation, draft cleanup, hidden-place cleanup, persistence,
  and status updates. The low-level `remove_user_place()` helper stays private
  to the user-place module instead of being re-exported through `ui::places`.
- [x] P16gab: Move app-facing dropped-folder place insertion into the Places
  user facade. `src/ui/places/user.rs` now owns
  `insert_place_from_dropped_paths()` on `FikaApp`, including validation,
  user-place insertion, persistence, and status updates. The low-level dropped
  helper stays private to the user-place module instead of being re-exported
  through `ui::places`.
- [x] P16gac: Move app-facing user-place reorder and insert-index helpers into
  the Places user facade. `src/ui/places/user.rs` now owns
  `move_user_place_to_insert_index()` and `user_place_insert_index()` on
  `FikaApp`, including status mapping and persistence. The low-level ordering
  result enum and functions stay private to the user-place module.
- [x] P16gad: Move app-facing place-draft commit into the Places user facade.
  `src/ui/places/user.rs` now owns `commit_place_draft()` on `FikaApp`,
  including draft extraction, current-directory lookup, validation result
  mapping, persistence, and status updates. The low-level
  `commit_user_place_draft()` helper stays private to the user-place edit
  module.
- [x] P16gae: Move app-facing add/edit place draft startup into the Places user
  facade. `src/ui/places/user.rs` now owns `start_add_place()` and
  `start_edit_place()` on `FikaApp`, including pane focus, conflicting draft
  cleanup, default label projection, editable-place lookup, draft creation, and
  status updates. `src/main.rs` keeps the network-drive draft startup for now
  because that path has separate network-auth semantics.
- [x] P16gaf: Move app-facing Places drop-target state into the Places drag
  facade. `src/ui/places/drag.rs` now owns setting, querying, clearing, and
  pane-viewport ownership clearing for `PlaceDropTarget`; generic item/path-list
  drop-target state remains outside Places.
- [x] P16gag: Move app-facing Places drop execution into the Places drag
  facade. `src/ui/places/drag.rs` now owns dropping place drags, item drags, and
  external paths onto Places targets or insertion positions, while generic pane
  and filesystem transfer helpers remain outside Places.
- [x] P16gah: Move app-facing Places activation and device operations into the
  Places devices facade. `src/ui/places/devices.rs` now owns opening mounted
  places, mounting unmounted device places, and finishing mount/unmount/eject
  background operations.
- [x] P16gai: Move app-facing Places context-menu target projection into the
  Places sidebar facade. `src/ui/places/sidebar.rs` now owns converting place,
  section, and blank-sidebar interactions into `ContextMenuTarget` values.
- [x] P16gaj: Move app-facing place-draft lifecycle wrappers into the
  place-draft facade. `src/ui/place_draft.rs` now owns pane-scoped clearing,
  dismiss, and focus changes for `PlaceDraft`, while draft creation and commit
  remain in the owning Places user/network paths.
- [x] P16gak: Record the Places full-row visual handoff breakthrough and
  default-promotion blocker. The breakthrough is not raw full custom paint; it
  is ready-only handoff plus `PlacesRowTextShapeCache` prewarming, which keeps
  GPUI text/icons visible during warmup and switches to retained full row
  painting only after resources are ready. Evidence from
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-full-handoff-runner-20260619` shows full targets warm row paint
  at `379us`, overflow warm row paint at `1090us` for 75 rows/29 painted rows,
  and layout warm row paint at `724us`; the same analyze-only runner passes
  the row-visual gates. Default promotion remains blocked because the targets
  full run still reached `27268us` first-frame `[fika render] total`, so the
  remaining work is first-frame owner accounting and total-render variance, not
  cold row visual paint alone. The roadmap, renderer decisions, and evidence
  checklist now require `--places-full-handoff` A/B evidence before changing
  the Places full-row visual default.
- [x] P16gal: Add first-frame render owner accounting for Places full handoff.
  Root cause of the previous opaque full-target total was missing ownership in
  `[fika render]`: the analyzer only had max fields for broad render phases, so
  the 2026-06-19 full targets log showed a `17285us` residual at max total.
  Implementation: `[fika render]` now reports `window_setup`,
  `chrome_inputs`, and `overlays`; `scripts/analyze-places-perf.sh` now emits
  `render_at_max_total` with same-frame owner values, `accounted`, `residual`,
  `max_accounted`, and `max_residual`. Evidence:
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-full-owner-20260619` passed all full handoff gates. The new
  owner line reduced residual to `4-5us` in the new run and identified
  `chrome_inputs` as the dominant first-frame owner: chrome targets `7846us`,
  full targets `7817us`, chrome overflow `8768us`, full overflow `7832us`,
  chrome layout `7824us`, and full layout `8638us`. This moves the next
  optimization from row visual paint to toolbar/chrome icon/input preparation.
- [x] P16gam: Split `chrome_inputs` into state and icon owners. The previous
  owner accounting narrowed the total-frame spike to `chrome_inputs`, but it
  still mixed cheap render state projection with synchronous named icon
  resolution for toolbar/chrome controls. Implementation: `[fika render]` now
  reports `chrome_state` and `chrome_icons`; `scripts/analyze-places-perf.sh`
  keeps `chrome_inputs` as the normalized sum so old logs still parse.
  Evidence:
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-chrome-split-20260619` passed all full handoff gates and showed
  `chrome_state` at only `2-7us` while `chrome_icons` dominated the max-total
  frame: chrome targets `8380us`, full targets `8360us`, chrome overflow
  `14626us`, full overflow `10708us`, chrome layout `11679us`, and full layout
  `9101us`. The next optimization target is now specifically first-frame
  named toolbar/chrome icon resolution.
- [x] P16gan: Prewarm fixed chrome icon snapshots before the first render.
  Root cause: both default chrome and full handoff were paying synchronous
  named icon snapshot resolution for fixed toolbar/sidebar controls on the
  first rendered frame, so the residual full-path spike was not row visual
  painting. Implementation: `FikaApp::new()` now calls
  `prewarm_chrome_icon_cache()` before device place replacement, resolving the
  filter toggle, pane split/close, and Places panel icon snapshots into the
  shared file-icon cache. Evidence:
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-chrome-prewarm-20260619` passed all full handoff gates. The
  max `chrome_icons` owner dropped from the split run's `8380us`/`8360us`
  targets, `14626us`/`10708us` overflow, and `11679us`/`9101us` layout to
  chrome targets `12us`, full targets `6us`, chrome overflow `10us`, full
  overflow `9us`, chrome layout `7us`, and full layout `7us`. This is a real
  full-path breakthrough because it removes the shared first-frame chrome icon
  spike; default promotion is still gated on repeated row-visual/root/pane
  total-render evidence rather than this owner alone.
- [x] P16gao: Remove the empty GPUI row spacer after Places full visual
  handoff. Root cause: once full handoff reached `text_gpui=0` and
  `icon_gpui=0`, every retained Places row shell still carried an empty
  `flex_1` child only to keep the shell wide. Implementation: custom chrome
  row shells now set `w_full()` with the fixed row height, so ready full rows
  keep their hitbox width without constructing the spacer subtree. Evidence:
  `/tmp/fika-places-full-overflow-no-spacer.log` passed the full handoff
  overflow gate and reduced the previous prewarm overflow maxima from
  `max_total=4760us`, `max_pane_elements=1603us`, and `max_root=2008us` to
  `max_total=3813us`, `max_pane_elements=1191us`, and `max_root=1583us`.
  This is incremental rather than a default-promotion decision, but it moves
  full handoff closer to Dolphin-style retained rows by removing a visual-only
  GPUI child once the custom painter owns text and icon output.
- [x] P16gap: Skip redundant ordinary Places row background fills in the custom
  visual layer. Root cause: the full/custom Places painter was filling every
  ordinary row with the same `0xf8f9fb` color as the sidebar background, so the
  retained painter submitted unnecessary rounded quads for rows with no active
  or drop state. Implementation: `paint_place_row_visual()` now paints row
  background and border only for active or drop-target rows; plain rows let the
  sidebar background show through while text, icons, trash markers, and insert
  indicators remain unchanged. Evidence:
  `/tmp/fika-places-full-overflow-skip-plain-bg.log` passed the full handoff
  overflow gate with `places_row_visual max_paint=828us` and
  `max_warm_paint=828us`, down from the recent full-overflow runs around
  `1.1-1.3ms`. This is a direct Dolphin-style retained paint improvement:
  paint only stateful row chrome instead of repainting the static parent
  background per item.
- [x] P16gaq: Filter Places retained event hitboxes to the visible content
  mask. Root cause: the retained event layer still inserted hitboxes for every
  Places row and section in overflow scenarios, even though pointer, click, and
  context-menu delivery only need current viewport hitboxes. DnD move/drop
  continues to use full interaction geometry. Implementation:
  `places_event_probe_prepaint()` now intersects row/section y ranges with
  `Window::content_mask()` before calling `Window::insert_hitbox()`, while the
  analyzer treats `[fika places-event-probe] rows/sections` as visible hitbox
  counts and keeps the renderer-policy count as the retained projection
  capacity. Evidence:
  `/tmp/fika-places-full-overflow-visible-hitboxes.log` passed the full
  handoff overflow gate with `--require-event-probe`; overflow event hitboxes
  dropped from the retained projection capacity of `78` to the visible set of
  `32`, and event paint stayed at `max_paint=52us`. This moves Places further
  away from per-row GPUI/event work and toward viewport-owned retained hit
  testing.
- [x] P16gar: Share Places snapshots across sidebar visual and event layers.
  Root cause: `places_sidebar()` cloned the full `Vec<PlaceSnapshot>` separately
  for the visual layer and event layer, then consumed the original vector to
  build remaining row shells. Implementation: the sidebar now moves the input
  vector into `Arc<[PlaceSnapshot]>`; the visual layer and event layer share
  that snapshot slice, while row-shell construction clones only the row being
  handed to the remaining GPUI drag-start boundary. Evidence:
  `/tmp/fika-places-full-overflow-shared-snapshots.log` passed the full
  handoff overflow gate with `--require-event-probe`, `max_build=1112us`,
  `max_total=3198us`, and event probe `max_paint=15us`. This moves Places
  closer to a retained model/painter split: one projected snapshot feeds all
  viewport layers instead of cloning per surface.
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
- [x] P16gas: Promote Places default visual ownership to full retained paint and
  replace row GPUI icon elements with retained image painting. Root cause:
  moving the default from chrome to text would still leave Places icons as GPUI
  `img()` row children, which is not the Dolphin-style model/controller/painter
  split. Implementation: `places_row_visual_policy()` now defaults to
  `CustomFull`; the Places visual layer owns a keyed `PlacesIconImageCache` that
  uses GPUI `RetainAllImageCache` plus `window.paint_image()` for real theme
  icons, with stable fallback during pending/failed loads. Evidence:
  `/tmp/fika-places-default-full-targets-scale.log` passes
  `--expect-custom-row-full-policy` with `visual_kinds=full`, `text_gpui=0`,
  `icon_gpui=0`, `max_total=2247us`, and warm row paint `395us`.
  `/tmp/fika-places-default-full-overflow.log` passes the overflow full gate;
  at 75 policy rows, viewport event hitboxes are clipped to `32`,
  `max_total=2162us`, and warm row paint is `655us`. Cold row paint still
  includes first-use image-atlas/text cost (targets `5179us`, overflow
  `8263us`), so the next step is cold-frame prewarm or amortization, but default
  Places row text and icons no longer depend on GPUI child elements.
- [x] P16gat: Apply the Places full-handoff lesson to pane MIME/theme icons.
  Root cause: forcing `FIKA_CUSTOM_THEME_ICONS=1` as a cold default still
  exposes first-load custom image placeholders and decode completion churn
  (`/tmp/fika-pane-full-custom-etc.log`: `theme_placeholder=52`, visible
  `theme_decoded=5`). Implementation: the default hybrid renderer now uses a
  visible-cohort handoff. While any visible theme-icon key is not ready, all
  visible theme icons remain on GPUI `img()` and the item image layer only
  prewarms retained images; when the cohort is ready, all visible theme icons
  switch to the retained custom image layer together. Evidence:
  `/tmp/fika-pane-cohort-default-downloads.log` passed
  `--gate-hybrid-default-promotion` against
  `/tmp/fika-pane-cohort-gpui-downloads.log` with `theme_placeholder=0` and
  visible `theme_decoded=0`. `/tmp/fika-pane-cohort-default-etc-r2.log` kept
  those image stability counters clean, but the full promotion gate still
  failed on `/etc` icon-sync/content-change variance, so the next pane image
  target is reducing `/etc` `icon_sync` cost rather than changing placeholder
  behavior.
- [x] P16gau: Remove the same-kind icon cache scan from pane `icon_sync` and
  widen background icon resolve batches. Root cause: after visible-cohort
  handoff, `/etc` still showed 7-13ms `icon_sync` frames even when most
  candidates were counted as cached, because `FileIconCache::cached_icon_for_kind()`
  scanned the exact-size cache once per visible candidate to find a reusable
  resolved theme path. Implementation: `FileIconCache` now keeps a
  `resolved_by_kind` index for pathful `FileIconKind` results while preserving
  exact-size and negative exact cache entries; file icon background resolve
  batches now process up to 128 requests so bounded read-ahead is more likely
  to finish before resize/scroll makes those items visible. Evidence:
  `/tmp/fika-icon-batch128-default-etc.log` passed
  `--gate-hybrid-default-promotion` against
  `/tmp/fika-icon-batch128-gpui-etc.log` with candidate `icon_sync=103us`,
  `theme_placeholder=0`, and visible `theme_decoded=0`;
  `/tmp/fika-icon-batch128-default-downloads-r2.log` passed the same gate
  against `/tmp/fika-icon-batch128-gpui-downloads-r2.log`.
- [x] P16gav: Move Places section heading labels into the default full visual
  layer. Root cause: after Places rows and row icons became full custom by
  default, group headings still used GPUI text children, so the default full
  policy could not honestly report complete Places text visual ownership.
  Implementation: `places_row_visual_layer` now projects section headings from
  the same snapshot as rows, prepaints visible heading labels through
  `PlacesRowTextShapeCache`, and paints them in the sidebar visual canvas;
  `group_heading` keeps the section targeting/DnD shell but omits the label
  child when custom visual text is active. Evidence:
  `/tmp/fika-places-section-full-targets.log` and
  `/tmp/fika-places-section-full-overflow.log` passed
  `--expect-custom-row-full-policy` with `section_gpui=0`; targets warm row
  paint was `247us`, and overflow kept visible event hitboxes clipped to `32`
  with warm row paint `785us`.
- [x] P16gaw: Remove pane per-directory GPUI drag-move shells. Root cause:
  directory item/row drop hover was still asserted through transparent GPUI
  shells even though retained window-position hit testing already resolved pane
  and directory targets for item, external-path, and Place drags. Implementation:
  Compact/Icons item shells and Details row shells no longer install
  `install_directory_drop_target_shell`; the helper and `directory-shell-hit`
  handler were removed from `file_grid/dnd.rs`. Renderer-policy logs now report
  `retained_directory_drop_target` and `gpui_directory_drop_shell`, and
  `scripts/analyze-item-view-perf.sh --expect-retained-item-policy` rejects any
  nonzero GPUI directory drop shell count. Remaining GPUI item/row shells are
  the typed drag-start boundary and rename overlay only. Evidence:
  `/tmp/fika-item-retained-directory-drop.log` passed the item-view autosmoke,
  renderer-policy and interaction gates with
  `max_retained_directory_drop_target=60` and
  `max_gpui_directory_drop_shell=0`.
- [x] P16gax: Move the Details header into the custom Details visual layer.
  Root cause: Details rows were custom-painted, but the header still used GPUI
  `Div`/text children for background, separators, and labels. Implementation:
  `details_visual_layer_view()` now carries a header projection, prepaints
  header labels through `DetailsTextShapeCache`, and paints the header in the
  same canvas as Details rows; `details_shell.rs` no longer builds the GPUI
  `details_header()` subtree. Renderer-policy logs now expose
  `details_header_visual_layer` and `gpui_details_header`, and
  `--expect-retained-item-policy` rejects GPUI Details headers. Remaining
  follow-up: add a dedicated Details-mode runtime autosmoke so this surface has
  the same runtime evidence strength as Compact zoom/scroll.
- [x] P16gay: Add a dedicated Details-mode item-view runtime autosmoke gate.
  Root cause: the retained item-view smoke only exercised the default Compact
  path, so Details custom paint regressions could still pass the standard
  runtime evidence even after the header moved into the visual layer.
  Implementation: `FIKA_AUTOSMOKE_ITEM_VIEW=details-zoom-scroll` now switches
  the active pane to Details before running zoom/scroll actions, the item-view
  analyzer recognizes `DetailsZoomScroll` and requires its `view-details`
  marker, and the retained renderer evidence script runs a Details gate with
  `--require-details`, `--require-modes Details`, `--require-renderer-policy-modes
  Details`, and `--expect-retained-item-policy`. This turns Details visual
  ownership into repeatable runtime evidence for future pane painter work.
- [x] P16gaz: Reuse ready MIME/theme icon resource paths across zoom handoff.
  Root cause: pane image handoff was exact-size-key ready only. During zoom, a
  theme icon with the same loaded `Resource::Path` but a new size/scale key
  could briefly fall back to GPUI or be counted as a new visible custom decode,
  causing the second icon identity adjustment the Dolphin comparison was meant
  to avoid. Implementation: `ThemeIconImageReadiness` now tracks both ready
  semantic keys and ready resource paths; the visible-cohort handoff accepts a
  visible icon when either its exact key or resource path is ready. The retained
  theme icon cache also indexes loaded images by path, so a new size key with
  the same path is treated as retained reuse rather than first-ready decode.
  Evidence: `/tmp/fika-path-ready-hybrid-downloads.log` passed
  `--gate-hybrid-default-promotion` against
  `/tmp/fika-path-ready-gpui-downloads.log` with `theme_placeholder=0` and
  visible `theme_decoded=0`. `/tmp/fika-path-ready-hybrid-etc-r2.log` passed
  the handoff portion and removed visible decode churn (`theme_decoded=0`),
  though full default promotion still failed on `/etc` icon-sync/content-change
  variance outside the image handoff path.
- [x] P16gba: Replace pane path-ready with a Dolphin-style key-size full path.
  Root cause: Dolphin's `KStandardItemListWidget::pixmapForIcon()` looks up
  `QPixmapCache` by `iconName + iconHeight + devicePixelRatio + mode`; path is
  only the icon-theme resource source and should not be the MIME/theme icon
  readiness/cache key. Implementation: pane MIME/theme icons now default to the
  full custom image layer; `ThemeIconImageReadiness` only records
  `ThemeIconImageKey`; `RetainedThemeIconImageCache` no longer reuses old
  images across size keys via `images_by_path`; `FileIconCache` no longer
  carries pathful kind snapshots across sizes and adds `MIME + size` reuse; cold
  SVG theme icon keys synchronously produce GPUI `RenderImage`s through
  `svg_renderer` and still paint through `Window::paint_image`/the sprite atlas.
  Evidence: `/tmp/fika-full-syncsvg-custom-etc.log` versus
  `/tmp/fika-full-syncsvg-gpui-etc.log` reports `max_image_layer=64`,
  `max_gpui_image_element=0`, `theme_placeholder=0`, `theme_retained=497`, with
  content-change and `icon_sync` below the GPUI baseline. Downloads full logs
  report `max_image_layer=32`, `max_gpui_image_element=0`,
  `theme_placeholder=0`, `theme_retained=543`, and lower initial total than the
  GPUI baseline.
- [x] P16gbb: Promote the pane theme `RenderImage` cache to an app/global owner
  and prewarm visible `ThemeIconImageKey`s during snapshot construction. Root
  cause: after the Dolphin-style key-size full path, the retained
  `RenderImage` cache still lived in the image-layer element, so cold SVG
  decode happened in element prepaint even though placeholder behavior was
  fixed. Implementation: `FikaApp` now owns the theme `RenderImage` cache;
  `PaneSnapshot` construction collects visible custom-theme keys from the
  `FileGridRenderSnapshot`, deduplicates by `ThemeIconImageKey`, synchronously
  materializes SVG `RenderImage`s through GPUI's `svg_renderer`, marks semantic
  keys ready, and passes the refreshed readiness snapshot to pane rendering.
  The separate prewarm element was removed, so the file-grid surface only
  consumes readiness and paints retained images. Evidence:
  `/tmp/fika-early-prewarm-custom-etc.log` reports `max_image_layer=64`,
  `max_gpui_image_element=0`, `theme_placeholder=0`, `theme_decoded=0`,
  `theme_prewarm_decoded=0`, `theme_retained=454`, and
  `item-image max_prepaint=166us`; `/tmp/fika-early-prewarm-custom-downloads.log`
  reports `max_image_layer=32`, `max_gpui_image_element=0`,
  `theme_placeholder=0`, `theme_decoded=0`, `theme_prewarm_decoded=0`,
  `theme_retained=187`, and `item-image max_prepaint=315us`.
- [x] P16gbc: Reduce `/etc` cold/content-change `icon_sync` variance in the
  model/icon-resolution path. Root cause: the remaining `/etc` spike was two
  cold visible semantic icon resolves, not image paint. `.pwd.lock`
  (`application/octet-stream`) spent about 28ms in theme lookup and `.updated`
  (`text/plain`) spent about 2ms after read-ahead preliminary keys changed into
  real MIME keys. Implementation: startup launches a detached background prewarm
  for common semantic file-icon keys, prioritizing the default 48px size and
  adjacent zoom levels before filling the rest. The table includes directory
  plus common text, binary, archive, office, image, video, audio, and PDF MIME
  keys. The prewarm writes into
  `FileIconCache` through `finish_resolve_results`, but does not occupy
  `FileIconResolveQueue` cover keys, because queue-covered visible items caused
  the first `/etc` content frame to drop the image layer temporarily. Evidence:
  `/tmp/fika-common-icon-prewarm-detached-etc.log` and
  `/tmp/fika-common-icon-prewarm-expanded-etc.log` report no scroll-time
  `application/octet-stream` or `text/plain` sync resolve, with expanded
  `icon_sync max_total=241us`, `max_resolved=1` only for the initial directory
  key, `max_image_layer=64`, `max_gpui_image_element=0`, and
  `theme_placeholder=0`. Downloads still shows a first-visible
  `application/java-archive` race, so mixed-directory initial MIME prewarm
  remains future work rather than claimed fixed.
- [x] P16gbe: Remove the mixed-directory first-visible MIME icon race. Root
  cause: detached common prewarm can lose the race against the first visible
  `icon_sync`, and MIME theme lookup misses were not stored in the `MIME + size`
  semantic index. A prewarmed `application/java-archive` miss therefore did not
  protect a visible `.jar` entry. Implementation: resolve the default 48px
  common semantic MIME table synchronously during app initialization before the
  first pane load, keep detached prewarm for the remaining zoom sizes, and store
  pathless MIME results in `FileIconCache::resolved_by_mime`. Reused MIME
  entries recompute fallback marker/colors from the current file kind, so `.jar`
  still shows `JAR` without repeating theme lookup. Evidence:
  `/tmp/fika-common-icon-sync48-downloads.log` reports `max_resolved=0`, no
  `[fika icon-sync-resolve]` lines, `icon_sync max_total=235us`,
  `max_gpui_image_element=0`, and `theme_placeholder=0`;
  `/tmp/fika-common-icon-sync48-etc.log` reports `max_resolved=0` and
  `icon_sync max_total=33us`.
- [x] P16gbd: Align pane SVG theme-image retention with GPUI `img()` internals.
  Root cause: Fika's full custom path already used `Window::paint_image`, but
  the retained theme image cache only indexed by `ThemeIconImageKey`, so the
  same scalable SVG source could be materialized again when zoom produced a new
  size key. GPUI `img(Resource::Path(svg))` creates one `Arc<RenderImage>` for
  the resource and scales by paint bounds. Implementation:
  `RetainedThemeIconImageCache` now keeps an additional
  `source path -> RenderImage` index. Readiness remains size/scale-aware by
  `ThemeIconImageKey`, but new semantic keys can be recorded from the retained
  source image without another SVG read/render. Source reuse reports as
  retained rather than decoded in `[fika item-image]`. Evidence:
  `/tmp/fika-svg-source-retain-etc.log` reports `theme_decoded=0`,
  `theme_retained=982`, `theme_placeholder=0`, `max_gpui_image_element=0`, and
  `item-image max_prepaint=480us`; `/tmp/fika-svg-source-retain-downloads.log`
  reports `theme_decoded=0`, `theme_retained=702`, `theme_placeholder=0`,
  `max_gpui_image_element=0`, and `item-image max_prepaint=788us`.
- [x] P16gbd1: Add a Dolphin/Qt QPixmapCache-style budget to the pane retained
  theme icon image cache. Root cause: Dolphin `pixmapForIcon()` uses a
  `name + size + dpr + mode` `QPixmapCache` key and relies on global pixmap
  cache budgeting; GPUI `img()` also has element/global image cache lifecycle.
  Fika's full custom path directly owns `Arc<RenderImage>` values, and
  `RetainedThemeIconImageCache` plus the source-path map previously had no
  limit, so visiting many directories could grow retained images without
  pruning. Implementation: cache hits refresh a generation; `prune_to_budget()`
  evicts least-recently-used semantic keys against a 10MB budget calculated from
  retained `RenderImage` frame bytes, not a coarse entry count. When the last
  key for a source path is removed, the `source path -> RenderImage` entry is
  released. Paint-path pruning also calls
  `RetainAllImageCache::remove(Resource::Path)` and
  `cx.drop_image(image, Some(window))`, so GPUI's resource cache or atlas cannot
  keep owning the same image. Acceptance: the user verifies with debug `/etc`
  USS/private-memory measurement; RSS, release builds, and current GPUI
  fallback are not substitutes for that evidence.
- [x] P16gbd2: Apply the same QPixmapCache-budgeted release strategy to the
  Places full row icon path. Root cause: Places owns a separate
  `PlacesIconImageCache`, which could also retain sidebar icon images through
  `RetainAllImageCache` and the source map indefinitely. Implementation: Places
  now finds by `ThemeIconImageKey`, reuses by source path on miss, and only then
  loads. After each load/insert it evicts LRU semantic keys against the 10MB
  frame-byte budget; when the last source reference is released it also calls
  `RetainAllImageCache::remove(Resource::Path)` and `cx.drop_image`.
- [~] P16gbf: Reduce remaining custom pane cold visual/text paint variance after
  image/icon ownership is stable. Current `/etc` and Downloads logs have image
  and icon-sync under budget, but `[fika static-item-visual]` can still show
  multi-millisecond cold prepaint/paint. Compare this path with Dolphin item
  text/pixmap caches and GPUI text shaping, then decide whether to add retained
  text-shape/source prewarm, tighten paint invalidation, or introduce a more
  Dolphin-like visible widget/state pool.
- [x] P16gbf1: Tighten pane static text/visual reuse for Icons zoom. Root cause:
  static item text shapes were keyed by `item_id` and by paint-only text bounds
  in center-aligned Icons mode, so a zoom/resize could miss even when the
  actual shaped label lines were unchanged. Implementation: remove item identity
  from `StaticItemTextShapeCacheKey`, ignore text rect width/height for
  center-aligned labels after line selection has been computed, ignore fallback
  marker line height when no fallback marker is painted, skip transparent
  background quads for ordinary items, and add
  `FIKA_AUTOSMOKE_ITEM_VIEW=icons-zoom-scroll`. Evidence:
  `/tmp/fika-full-icons-keyed-etc.log` covers `modes: Icons,Compact` with
  `max_gpui_image_element=0`, `theme_placeholder=0`, `theme_decoded=0`; after
  the initial Icons switch, zoom frames show `hits=24 misses=0`,
  `hits=28 misses=0`, and `hits=40 misses=0`, with
  `[fika static-item-visual]` prepaint dropping to 93-254us on repeated zoom
  frames.
- [~] P16gbf2: Remove the remaining first-enter cold text/glyph spike for
  Icons/Compact full custom visual paint. Current evidence shows the next root
  cause: Downloads still reports a stable first Icons switch cold shape spike
  (`/tmp/fika-full-icons-keyed-downloads-r2.log`, `hits=1 misses=39`,
  `static-item-visual prepaint=52840us`) and a first text paint spike
  (`paint=17698us`) even though image/icon work is clean. The next cut should
  add a Dolphin-style retained text warmup/state pool so target-mode label
  shapes and glyph paint are warmed before handoff, similar to the Places row
  text handoff model, instead of shifting all cold shaping into the first
  visible custom visual frame.
- [x] P16gbf2a: Add the first pane alternate-mode static text warmup path. The
  pane render frame now projects a temporary Compact/Icons target-mode snapshot
  with local slot/cache state, passes it to the file-grid surface, and mounts a
  warm-only static visual layer before the visible layer. The warm layer shapes
  text into the pane-local `StaticItemTextShapeCache` but does not paint or
  report visible static-visual timing. Its `ElementId` is distinct from the
  visible static visual layer so GPUI retained element identity cannot collide.
  Evidence after the ID fix: `/tmp/fika-compare-pane-full-etc-r3.log` kept
  `max_gpui_image_element=0`, `theme_placeholder=0`, visible
  `theme_decoded=0`, and reduced `/etc` visible static prepaint to
  `2996us` while the paired `FIKA_GPUI_THEME_ICONS=1` baseline had
  `2938us`. Downloads remains incomplete:
  `/tmp/fika-compare-pane-full-downloads-r3.log` still shows
  `static_visual max_prepaint=16866us max_paint=17580us`, similar to the GPUI
  image baseline's text cost. Next work should target glyph/text paint
  retention or a ready-only handoff, not image renderer policy.

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
  `/etc` autosmoke satisfies the Compact/Icons zoom-scroll icon-sync part, and
  `details-zoom-scroll` now covers Details visual/header runtime evidence. Full
  DnD runtime smoke still needs a desktop-session refresh.
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
