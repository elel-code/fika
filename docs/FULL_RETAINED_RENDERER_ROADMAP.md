# Full Retained Renderer Roadmap

This document is the execution entry point for the post-Places-chrome direction.
It complements:

- `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md` for the cross-surface
  Dolphin alignment contract and default-promotion rules.
- `docs/ITEM_VIEW_CUSTOM_PAINT_DESIGN.md` for architecture contracts.
- `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md` for current replacement state.
- `docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md` for per-slice implementation history.
- `docs/ITEM_VIEW_RENDERER_DECISIONS.md` for surface-level renderer decisions.
- `docs/PLACES_RENDERER_PLAN.md` and `docs/RENAME_EDITOR_PLAN.md` for
  surface-specific plans.

The goal is a Dolphin-style retained model/controller/painter architecture, not
a blanket rule that every pixel must be custom-painted immediately. A surface
only moves to custom paint when retained ownership is clear and runtime
evidence shows it is behavior-complete and neutral or better than the GPUI
built-in path.

## First Priority: Dolphin Implementation Model, GPUI As Paint Backend

The current top priority is to move file-grid and Places hot paths to the
Dolphin implementation model:

```text
model roles -> visible-first role updater -> retained slot/cache -> thin custom painter
```

GPUI `img()` / `image()` must not remain the architectural owner for item image
lifetime, visible-range scheduling, cache keys, or readiness handoff. It remains
only as an explicit bridge, baseline, or fallback. Final drawing still uses
GPUI custom elements/canvas through `Window::paint_image()`, `paint_quad()`, and
text painting; the performance win comes from Dolphin-style lifetime and cache
boundaries, not from replacing the image drawing primitive.

Source-level comparison:

| Responsibility | Dolphin source model | GPUI `img()` source model | Fika priority |
|---|---|---|---|
| Role/preview scheduling | `KFileItemModelRolesUpdater::startUpdating()` runs `updateVisibleIcons()` first, then `indexesToResolve()`; `MaxBlockTimeout=200ms`, `ReadAheadPages=5`, `ResolveAllItemsLimit=500` | Each `Img` calls `source.use_data()` from `request_layout()` and drives loading/fallback from element lifetime | Build a shared RoleUpdater/ImageResolver used by pane and Places with visible-first/read-ahead/size-DPR invalidation |
| Image cache key | `KStandardItemListWidget::pixmapForIcon()` keys by icon name + icon height + DPR + mode | `RetainAllImageCache` keys by `Resource` hash; `Img` decides when to use it | Theme icon key must be semantic: icon name + size + DPR + theme + color scheme + mode; thumbnail keys stay separate |
| Widget/item local state | `updatePixmapCache()` maintains widget-local `m_pixmap`, `m_scaledPixmapSize`, and `m_pixmapPos` | `ImgState` stores frame/loading state without item role/read-ahead ownership | Retained slots store content/geometry/visual/image/text dirty state; paint state only consumes resolved state |
| Painting | `KStandardItemListWidget::paint()` paints background, pixmap, and text; hover background is in `KItemListWidget::paint()` | `Img::paint()` ultimately calls `window.paint_image()` | Custom elements only paint background/hover/selection/image/fallback/text/indicator, with no theme scan, MIME probe, or decode |
| Places | `DolphinPlacesModel` + `KFilePlacesView` own the model/view/delegate loop | Per-row GPUI elements tend to bind events and visuals back to element identity | Places and pane share retained slot, image request, cache/readiness semantics; row shells are explicit bridges only |

This changes optimization order: implement Dolphin-style RoleUpdater, shared
image model, bounded retained cache, and slot dirty state before removing the
remaining GPUI bridges. Any slice that optimizes image/hover/cache for only pane
or only Places must explain how the other side reuses the same model.

2026-06-19 implementation progress:

- Pane and Places now share `RetainedImageRequest`, `RetainedImageLoad`,
  `RetainedImageReady`, and `RetainedImageLayerState`. Places no longer has a
  surface-specific image cache wrapper; the sidebar keyed state owns the shared
  retained image layer directly.
- Theme-icon ready events are produced by the shared load result. Compact/Icons,
  Details, and Places consume the same readiness contract instead of deriving it
  independently.
- Thumbnail retained fallback moved from an unbounded `HashMap` to a byte-budget
  LRU cache; eviction also removes the GPUI `RetainAllImageCache` resource and
  drops the render image.
- Dolphin `ReadAheadPages=5` / `ResolveAllItemsLimit=500` role-updater ordering
  now lives in `ui::retained::work_order`, so thumbnail deferred work and
  file-icon resolve no longer maintain separate ordering code.
- Static item labels, Details cells/headers, and Places row labels now share
  `RetainedShapeCache` and `TextShapeCacheStats`. Surface modules still own
  their text keys and shape functions, but cache hit/miss/evict semantics are
  retained-layer code instead of pane/Places copies. Shape-cache stats now also
  include `compute=...us`, and both pane item-view and Places analyzers surface
  that field as `max_compute`. Places also retains GPUI glyph-raster paint data
  through the same cache primitive; this is the first consumer of the Fika GPUI
  fork's `ShapedLine::compute_glyph_raster_data` and
  `ShapedLine::paint_with_raster_data` hooks.
- File-grid static text and Details text now advance retained glyph-raster
  misses under a visible-layer frame budget. Cache hits paint retained raster
  data directly; cache misses compute synchronously only while the budget
  allows it; over-budget text falls back to GPUI normal text paint for that
  frame and continues cache fill on subsequent frames. Static opposite-mode
  warm/read-ahead is ordered after the real visible layer, probes shape cache
  hits only, and uses a small glyph budget so it cannot steal current visible
  shape or glyph miss budget. The `[fika item-glyph-budget]` and
  `[fika details-glyph-budget]` channels are the cold glyph miss spike profile.
- Places slot projection now wraps `RetainedSlotStats`, matching item-view slot
  delta accounting while keeping Places-specific row/section counts.
- Direct thumbnail/theme image load helpers are private to `RetainedImageLayerState`;
  pane, Details, and Places must enter through `RetainedImageRequest`.
- Final core evidence is green. `scripts/run-retained-renderer-evidence.sh
  --core --skip-build --prefix fika-core-final-retained-v3` completed with
  `retained renderer evidence complete`. The item logs cover Compact, Icons,
  and Details (`/tmp/fika-core-final-retained-v3-item-etc-zoom-scroll.log`,
  `/tmp/fika-core-final-retained-v3-item-etc-icons-zoom-scroll.log`,
  `/tmp/fika-core-final-retained-v3-item-etc-details-zoom-scroll.log`) with
  warm steady max total `1108us`, max file-grid build `1344us`, max image paint
  `373us`, warm static visual max paint `2546us`, and warm custom/details
  visual max paint `3309us`. Renderer policy stayed retained:
  `gpui_image_element=0`, `gpui_directory_drop_shell=0`, and
  `gpui_details_header=0`.
- The final Places logs
  (`/tmp/fika-core-final-retained-v3-places-targets.log`,
  `/tmp/fika-core-final-retained-v3-places-overflow.log`,
  `/tmp/fika-core-final-retained-v3-places-layout.log`,
  `/tmp/fika-core-final-retained-v3-places-hit-test.log`,
  `/tmp/fika-core-final-retained-v3-places-targeting.log`,
  `/tmp/fika-core-final-retained-v3-places-dnd.log`) passed with
  `visual_kind=full`, `row_gpui=0`, `text_gpui=0`, and `icon_gpui=0`.
- Fika now carries a dedicated GPUI fork/branch for retained-hitbox typed DnD
  and retained glyph-raster paint hooks:
  `ssh://git@github.com/elel-code/zed.git` at
  `02f256ffd7edfbcbb5354ad03db7a193def08590`
  (`fika/gpui-hitbox-dnd`). Fika pins both `gpui` and `gpui_platform` to that
  revision.
- The fork exposes hitbox-level typed drag/drop registration. Pane,
  Details, and Places drag start now register against retained hitboxes instead
  of per-item/per-row `Div::on_drag` shells. Places DnD move/drop target
  delivery also registers against the retained sidebar content hitbox.
- Current gates require zero GPUI DnD shells:
  `gpui_drag_shell=0`, `drag_shells=0`,
  `gpui_typed_dnd_payload_shells=0`, and retained Places DnD logs must pass
  `--expect-retained-event-policy`.
- Post-fork core evidence is green. `scripts/run-retained-renderer-evidence.sh
  --core --skip-build --prefix fika-full-retained-hitbox-dnd-v2` completed with
  `retained renderer evidence complete`; item logs show `max_gpui_drag_shell=0`,
  and Places targets/overflow/layout/hit-test/targeting/dnd logs show
  `max_gpui_event_shells=0`, `max_gpui_typed_dnd_payload_shells=0`,
  `max_drag_shells=0`, and successful `places full retained-event`.

## Current Baseline

Accepted retained/custom surfaces:

- Compact/Icons model, geometry, base visuals, labels, hover/drop/selection,
  thumbnail image layer, and retained hit testing.
- Details model, geometry, row backgrounds, icons, text cells, hover/drop/click
  hit testing, and retained controller routing.
- Places model/projection, slot stats, target decisions, panel layout state,
  and default custom row chrome for background/drop/insert/trash.

Explicit GPUI bridges:

- Rename uses the GPUI editor overlay.
- Compact/Icons MIME/theme icons use the full custom image layer by default.
  The painter still uses GPUI's efficient `RetainAllImageCache -> RenderImage
  -> Window::paint_image` backend, but ordinary pane rendering no longer keeps
  per-item GPUI `img()` children. `FIKA_GPUI_THEME_ICONS=1` is the explicit
  old GPUI baseline, and `FIKA_HYBRID_THEME_ICONS=1` remains only as a
  transitional readiness-handoff path.
- Places uses full custom row visual by default for backgrounds, text, and
  icons. Icon image load/cache/readiness uses the shared retained image layer
  directly. Places row/section activation, context-menu targeting, DnD target
  lookup, drop dispatch, and sidebar leave clearing now use retained hitboxes
  by default. The default Places path has no GPUI row/section event shells, no
  sidebar typed payload shell, and no GPUI row drag-start shells.

These bridges are intentional platform or performance boundaries. They should
be removed only through the tracks below.

## Dolphin Completeness Diagnosis

The remaining performance gaps are not evidence that full custom paint is
inherently slower than GPUI. They are evidence that some surfaces are not yet
complete Dolphin-style loops.

Dolphin's item view is fast because `KItemListView` owns visible widget reuse,
`KFileItemModelRolesUpdater` owns visible-first role work, and
`KStandardItemListWidget` paints only from stable local/global caches. Its
`updatePixmapCache()` keeps the widget-local pixmap, while `pixmapForIcon()`
uses a cache key built from icon name, icon height, device pixel ratio, and
mode. Zoom updates item geometry immediately, but expensive preview/role work
is delayed and coalesced. A Fika custom image renderer must match that cache
and readiness contract before it can replace GPUI `img()` by default.

Dolphin's Places panel is similarly a model/view/delegate loop:
`DolphinPlacesModel` owns Places state and `KFilePlacesView` owns interaction
delivery. Fika now has the Dolphin-complete Places path for row visuals,
row/section hit testing, targeting, drag start, typed DnD payload delivery, and
drop dispatch: the default path is full row visual plus retained-hitbox DnD
with zero GPUI DnD shells.

The practical conclusion is:

- Full custom paint is still a valid target for Places and MIME/theme images.
- The route is not a renderer swap. It is retained identity, role readiness,
  image readiness, hit-test ownership, and analyzer-backed default promotion.
- Until a surface has that loop, keeping a GPUI bridge is the Dolphin-aligned
  choice, not a retreat from the retained architecture.

The detailed cross-surface contract is in
`docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md`. That document is the guardrail
for future “full custom rendering” work: a renderer can become default only
after the model, layout, controller/hit-test, painter, cache and remaining
bridge boundaries are explicit and analyzer evidence proves the custom path is
not worse than the GPUI-backed path.

## Non-Negotiable Rules

- Model identity, layout identity, selection, drop state, and worker scheduling
  must not depend on GPUI element identity.
- Custom paint is a renderer policy over retained state. It is not allowed to
  own file roles, Places ordering, DnD decisions, or rename semantics.
- Visible-first work must stay Dolphin-aligned: visible roles/icons first,
  bounded read-ahead after, no synchronous theme scan, thumbnail probe, MIME
  magic read, or image decode in scroll/zoom paint.
- If a custom renderer loses to GPUI on perf, startup stability, behavior
  completeness, or maintenance risk, keep the retained state and keep that
  surface on GPUI until the migration is narrowed.
- Every implementation slice must end with evidence in the owning plan or
  decision document, and each completed slice must be committed separately.

## Execution Tracks

### Track 1: Evidence Freeze

Purpose: keep a current desktop-session baseline before removing any more GPUI
bridges.

Runnable checklist: `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.md`.

Required evidence:

- `/etc` and `~/Downloads` item-view logs with `FIKA_PERF_ITEM_VIEW=1`.
- `/etc` item-view `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll`.
- Details mode runtime evidence with `[fika details-visual]`,
  `[fika details-shape-cache]`, `[fika details-glyph-cache]`,
  `[fika details-glyph-budget]`, and retained interaction counts.
- DnD smoke with `FIKA_DEBUG_DND=1` covering pane item to pane directory,
  pane item to Places, Places to pane directory, and external path drop.
- Places default full retained/custom targets, overflow, layout, hit-test,
  targeting, and DnD autosmoke logs.
- Places GPUI/chrome/text fallback comparisons with
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff` only when
  changing Places full-row visual policy, text-shape handoff, or fallback
  thresholds.
- Default full custom image path versus `FIKA_GPUI_THEME_ICONS=1` only when
  changing MIME/theme icon rendering.

First-priority retained-glyph work:

- Keep the Places retained glyph-raster implementation as the reference.
- File-grid Details text and Compact/Icons static labels/fallback markers now
  use the same retained shape plus glyph-raster model.
- File-grid glyph-raster miss spikes must be proven bounded through
  `[fika item-glyph-budget]` and `[fika details-glyph-budget]`: cold misses may
  be `deferred`, but synchronous `compute=...us` must remain within a small
  frame budget.
- Do not claim this bypasses GPUI's backend. The target is Dolphin-style
  retained ownership above GPUI's text/image/window/render substrate.

Acceptance:

- Existing analyzers pass for the relevant logs.
- Logs are saved under `/tmp` and referenced in the changed plan/decision doc.
- Any user-visible breakthrough or regression has symptom, root cause,
  Dolphin comparison, implementation, evidence, and future guard recorded.

### Track 2: MIME/Theme Icon Renderer

Purpose: make image rendering custom only after it can match Dolphin's
widget-local pixmap stability.

Detailed design: `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md`.

Next design step:

- Define a retained icon image cache keyed by at least
  `(icon_name, icon_size_px)` plus theme, scale, and color-scheme inputs when
  they affect the selected path.
- Preserve the last loaded same-key real image during refresh.
- Keep thumbnail retention keyed by thumbnail path, not icon name.
- Keep GPUI image cache as the decode backend unless a replacement beats it.

The default is now full custom over the retained image model. Future
icon-renderer changes must keep:

- Paired default full-custom and `FIKA_GPUI_THEME_ICONS=1` baseline logs
  passing for `/etc` and a mixed user directory.
- Default/custom logs free of steady `theme_placeholder` churn, zoom-time
  `theme_decoded` burst, visible icon size second-jump, and synchronous icon
  work regression.
- `docs/ITEM_VIEW_RENDERER_DECISIONS.md` updated with the evidence.

### Track 3: Places Retained Event Delivery

Purpose: move Places from GPUI row event shells to retained hitboxes without
changing text/icon renderer policy.

Detailed design: `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md`.

Current default:

- `retained-dnd` owns row/section activation, context-menu targeting,
  on-place drop target, insert-before/after, drop dispatch, sidebar leave
  clearing, and cursor state through retained Places geometry.
- Retained hitbox DnD is provided by the Fika GPUI fork and owns typed payload
  move/drop delivery without a sidebar-level GPUI payload bridge.
- Places drag start registers on retained row hitboxes, not GPUI row shells.
- Default row visual is full custom; text/icons/section headings are painted by
  Fika.

Default may change only when:

- `scripts/analyze-places-perf.sh --expect-retained-event-policy` passes for
  targets, overflow, layout, hit-test, and DnD-specific smoke.
- Context menus still distinguish blank sidebar, section, bookmark, trash, and
  device rows.
- Internal reorder and item/external drop behavior remain unchanged.

### Track 3a: Places Full Row Visual Default

Purpose: keep Places row/section visuals on the same Dolphin retained model as
pane items: shared retained image requests, shared image readiness/cache,
shared text-shape cache machinery, retained slot stats, and a thin row visual
painter.

Current default:

- `DEFAULT_PLACES_ROW_VISUAL_POLICY = CustomFull`.
- Places row text, section text, and icons are painted by the custom row visual
  layer. `FIKA_PLACES_ROW_VISUAL_POLICY=gpui`, `chrome`, and `text` remain as
  explicit fallback/A-B policies only.
- `FIKA_PLACES_ROW_VISUAL_HANDOFF=1` is still available as a regression suite
  for ready-only handoff; it is no longer a prerequisite for making full rows
  the default.

2026-06-19 final evidence:

- The core runner passed targets, overflow, layout, hit-test, targeting, and
  DnD Places logs under the default full policy:
  `/tmp/fika-core-final-retained-v3-places-*.log`.
- Analyzer summaries show `visual_kinds=full`, row visual layer counts matching
  rows, `row_gpui=0`, `text_gpui=0`, and `icon_gpui=0`.
- 2026-06-20 retained glyph-raster cache evidence:
  `/tmp/fika-retained-glyph-raster-cache-v2-places-targets.log` and
  `/tmp/fika-retained-glyph-raster-cache-v2-places-overflow.log`. Targets warm
  row visual max is `prepaint=185us` / `paint=191us`; overflow warm row visual
  max is `prepaint=324us` / `paint=580us`. `[fika places-row-glyph-cache]`
  reaches `hits=13 misses=0` for 11-row targets and `hits=32 misses=0` for
  overflow after geometry settles.
- 2026-06-20 file-grid retained glyph-raster evidence:
  `/tmp/fika-retained-glyph-file-grid-v2-item-etc-zoom-scroll.log`,
  `/tmp/fika-retained-glyph-file-grid-v2-item-etc-icons-zoom-scroll.log`, and
  `/tmp/fika-retained-glyph-file-grid-v2-item-etc-details-zoom-scroll.log`
  pass the standard item runtime gate. The combined item summary reports
  `item_glyph_frames=24 hits=1105 misses=1051 max_entries=435` and
  `details_glyph_frames=12 hits=871 misses=563 max_entries=563`; warm paint
  maxima are `warm_static_visual_paint=1352us` and
  `warm_details_visual_paint=2674us`.
- 2026-06-20 file-grid glyph-raster miss budget evidence:
  `/tmp/fika-glyph-budget-v2-item-etc-zoom-scroll.log`,
  `/tmp/fika-glyph-budget-v2-item-etc-icons-zoom-scroll.log`, and
  `/tmp/fika-glyph-budget-v2-item-etc-details-zoom-scroll.log` pass the
  standard item runtime gate. The combined summary reports
  `item_glyph_budget_frames=27 ... computed=526 deferred=98 ...
  max_compute=2165us` and `details_glyph_budget_frames=12 ... computed=528
  deferred=188 ... max_compute=2006us`. This proves cold glyph-raster miss
  spikes moved from one-frame full synchronous fill to small-budget synchronous
  compute plus deferred fill on later frames; remaining cold prepaint spikes
  should be split next as shape/layout/image-state costs rather than glyph
  compute alone.
- 2026-06-20 shape compute attribution evidence:
  `/tmp/fika-shape-attribution-v1-item-etc-zoom-scroll.log`,
  `/tmp/fika-shape-attribution-v1-item-etc-icons-zoom-scroll.log`, and
  `/tmp/fika-shape-attribution-v1-item-etc-details-zoom-scroll.log` add
  `compute=...us` to shape-cache logs. The combined summary reports
  `item_shape_frames ... max_compute=7184us` and `details_shape_frames ...
  max_compute=9208us`, with Details visual prepaint still peaking at
  `19240us`. `/tmp/fika-shape-hitonly-v2-item-etc-zoom-scroll.log`,
  `/tmp/fika-shape-hitonly-v2-item-etc-icons-zoom-scroll.log`, and
  `/tmp/fika-shape-hitonly-v2-item-etc-details-zoom-scroll.log` then pass the
  item runtime gate after static opposite-mode warm prepaint became
  shape-cache-hit-only and started reporting its read-ahead glyph budget. The
  combined v2 summary reports `item_shape ... max_compute=5888us`, but Details
  remains the largest cold shape source (`details_shape ... max_compute=13442us`
  and `details_visual max_prepaint=17353us`). Places v2 evidence also carries
  the same shape compute field (`places_row_shape_cache ... max_compute=3617us`
  in overflow). The next completion-grade step is a Details
  warm-only/read-ahead text channel or an explicit column/row shape deferral
  design, not more glyph-raster work.
- Interaction is retained-DnD for row/section target delivery and typed payload
  delivery. The completion gates now require `gpui_event_shells=0`,
  `gpui_row_section_event_shells=0`, `gpui_typed_dnd_payload_shells=0`,
  `drag_shells=0`, and `drag_start_models=rows`.

Decision:

- Places full row visual and file-grid text retained glyph-raster paint are
  complete for the retained renderer transition. Text still uses GPUI's
  backend, but Fika owns retained line/glyph-raster lifetime and the custom
  paint call sites.
- Places retained event delivery and typed DnD shell removal are complete on
  the Fika GPUI fork. Future Places renderer work is regression monitoring
  against the chrome/GPUI fallback policies and keeping the fork patch current
  with upstream GPUI.

### Track 4: Typed Drag Boundary

Purpose: keep the Fika GPUI retained-hitbox typed DnD patch small, auditable,
and synchronized with upstream GPUI while Fika uses it as the primary path.

Current implementation:

- Fork branch: `git@github.com:elel-code/zed.git`, branch
  `fika/gpui-hitbox-dnd`, pinned revision
  `02f256ffd7edfbcbb5354ad03db7a193def08590`.
- Added GPUI APIs:
  `Window::on_hitbox_drag`, `Window::on_hitbox_drag_with_cursor`,
  `Window::on_hitbox_drag_move`, `Window::on_hitbox_drop`,
  `ShapedLine::compute_glyph_raster_data`, and
  `ShapedLine::paint_with_raster_data`.
- Fika registers item, Details row, and Places row drag start from retained
  hitboxes using stable element ids/global ids. Places registers typed
  move/drop handlers on the retained sidebar content hitbox.
- No visible or layout-owning GPUI row/item `Div` may be reintroduced just to
  carry typed DnD.

Maintenance gates:

- Compact/Icons, Details, and Places all pass DnD smoke.
- Drag preview position remains stable across Compact, Icons, Details, and
  Places at different window sizes.
- Renderer-policy logs keep `gpui_drag_shell=0` without losing retained
  interaction counts.
- Places full retained-event logs pass with `gpui_event_shells=0`,
  `gpui_row_section_event_shells=0`, `gpui_typed_dnd_payload_shells=0`,
  `drag_shells=0`, and `drag_start_models=rows`.
- The GPUI fork patch remains minimal and is rebased or forward-merged when
  upstream GPUI changes drag/drop, text, or paint internals.

### Track 5: Rename Editor

Purpose: keep rename behavior complete before any custom text editor replaces
the GPUI overlay.

Next design step:

- Turn the `docs/RENAME_EDITOR_PLAN.md` behavior matrix into runtime or unit
  smoke where possible: focus, caret hit testing, UTF-8 selection,
  commit/cancel, validation, Tab rename-next, mouse selection, focus-out, and
  IME.

Default may change only when:

- The custom path covers the behavior matrix at least as well as the GPUI
  editor.
- Accessibility and IME risk are explicitly accepted or covered.
- Failure keeps the retained rename draft model and leaves rendering on GPUI.

### Track 6: Ownership Cleanup

Purpose: continue moving item-view orchestration out of `src/main.rs` into
Dolphin-aligned file-grid and Places facades.

Next candidates:

- Runtime evidence helper ownership that still lives in app root.
- Remaining pane render orchestration that can become file-grid facade methods.
- Places full-handoff evidence and promotion helpers that still live outside
  the Places renderer facade.

Acceptance:

- No behavior changes without a paired runtime log.
- Module tests own invariants where the state is owned.
- `src/main.rs` becomes a coordinator of pane/app state, not owner of
  renderer internals.

## Next Queue

1. Freeze fresh item-view evidence for Details and Compact/Icons now that
   retained glyph-raster paint/cache covers both file-grid text paths.
2. Keep the retained MIME/theme icon image cache on the full-custom default and
   compare future image changes against `FIKA_GPUI_THEME_ICONS=1`.
3. Keep `--places-full-handoff` as a chrome/full regression suite, not a
   default-promotion blocker.
4. Keep the Fika GPUI retained-hitbox typed DnD/glyph-raster fork rebased or
   forward-merged after upstream GPUI dependency updates.
5. Convert rename behavior matrix items into tests/smoke before Track 5.

This queue is intentionally evidence-first. It moves the codebase toward full
retained reuse while preserving the current rule: custom paint only stays
default when it is at least as good as the GPUI path for that surface.
