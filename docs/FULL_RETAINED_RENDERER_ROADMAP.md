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

## Current Baseline

Accepted retained/custom surfaces:

- Compact/Icons model, geometry, base visuals, labels, hover/drop/selection,
  thumbnail image layer, and retained hit testing.
- Details model, geometry, row backgrounds, icons, text cells, hover/drop/click
  hit testing, and retained controller routing.
- Places model/projection, slot stats, target decisions, panel layout state,
  and default custom row chrome for background/drop/insert/trash.

Explicit GPUI bridges:

- Compact/Icons and Details drag start use GPUI `Div::on_drag` shells.
- Rename uses the GPUI editor overlay.
- Compact/Icons MIME/theme icons use the hybrid renderer by default: GPUI
  `img()` remains the fallback for not-yet-ready keys, while ready retained
  image keys paint through the custom image layer.
- Places text/icons remain GPUI-rendered. Places row/section activation,
  context-menu targeting, DnD target lookup, drop dispatch, and sidebar leave
  clearing now use the retained-DnD event layer by default; one sidebar-level
  GPUI typed payload bridge and row drag-start shells remain.

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
delivery. A Fika Places renderer becomes Dolphin-complete only when row/section
hit testing and event delivery are viewport-level retained state and the
remaining typed payload / drag-start platform bridges are explicit. Default
retained-DnD has removed row/section GPUI event shells from target delivery;
full retained Places is still blocked by the single sidebar typed payload
bridge and drag-start shells. Row chrome custom paint alone is not the finish
line.

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
  `[fika details-shape-cache]`, and retained interaction counts.
- DnD smoke with `FIKA_DEBUG_DND=1` covering pane item to pane directory,
  pane item to Places, Places to pane directory, and external path drop.
- Places default chrome targets, overflow, layout, and hit-test autosmoke logs.
- Places default-chrome versus full-handoff A/B logs with
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff` when
  changing Places full-row visual policy, text-shape handoff, or promotion
  thresholds.
- Default GPUI image path versus `FIKA_CUSTOM_THEME_ICONS=1` only when changing
  MIME/theme icon rendering.

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

The default is now hybrid. Future icon-renderer changes must keep:

- Paired default hybrid and `FIKA_GPUI_THEME_ICONS=1` baseline logs passing for
  `/etc` and a mixed user directory.
- Hybrid/custom logs free of steady `theme_placeholder` churn, zoom-time
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
- A single sidebar-level GPUI typed payload bridge remains because GPUI still
  exposes typed drag move/drop payloads through interactive elements.
- GPUI drag-start shells remain until Track 4 unlocks retained drag start.
- Default row chrome is custom; text/icons remain GPUI.

Default may change only when:

- `scripts/analyze-places-perf.sh --expect-retained-event-policy` passes for
  targets, overflow, layout, hit-test, and DnD-specific smoke.
- Context menus still distinguish blank sidebar, section, bookmark, trash, and
  device rows.
- Internal reorder and item/external drop behavior remain unchanged.

### Track 3a: Places Full Row Visual Handoff

Purpose: move Places text and vector-icon painting toward a fully retained row
visual path without promoting it before it is neutral or better than the
current chrome split.

Current opt-in state:

- `FIKA_PLACES_ROW_VISUAL_POLICY=full` paints full row text and vector icons in
  the custom row visual layer.
- `FIKA_PLACES_ROW_VISUAL_HANDOFF=1` keeps GPUI text/icons for the warmup
  frames, prewarms `PlacesRowTextShapeCache`, and hands off only after the
  retained row visual resources are ready.
- The handoff path is analyzer-backed through
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff`, which
  captures targets, overflow, and layout logs for both default chrome and full
  handoff.

2026-06-19 evidence:

- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-targets.log`
  passed the full-handoff row-visual gates with warm row paint at `379us`, but
  first-frame `[fika render] total` reached `27268us`.
- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-overflow.log`
  passed with 75 rows, 29 painted rows, and warm row paint at `1090us`.
- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-layout.log`
  passed with warm row paint at `724us`.

Decision:

- The full path has a real architectural breakthrough: ready-only handoff and
  text-shape prewarming remove the earlier cold row-paint blocker, and the
  retained custom row visual path is measurable under repeatable gates.
- It is not default yet. The remaining blocker is whole-frame startup/target
  total-render variance, not row visual painting alone. Continue comparing
  row-visual cost and `[fika render] total=` before promotion.

Next design step:

- Split first-frame Places snapshot, item-pane work, root work, and full row
  visual work clearly enough that the default-promotion gate can identify which
  owner caused a total-render spike.
- Reduce or amortize any full-handoff-specific first-frame work before lowering
  the full path's 30ms total-render guard.
- Keep the default chrome policy until full handoff matches or beats chrome in
  the same targets/overflow/layout A/B suite.

### Track 4: Typed Drag Boundary

Purpose: remove temporary GPUI drag shells and typed payload bridges only if
GPUI exposes or Fika carries an audited retained-hitbox typed drag API.

Next design step:

- Current GPUI audit (`0.2.2`, Zed
  `69b602c797a62f09318916d24a98c930533fbdc8`) still has no public retained
  hitbox typed drag hook. `Interactivity::on_drag`,
  `Interactivity::on_drag_move`, `Interactivity::on_drop`, and
  `StatefulInteractiveElement::on_drag` are interactive-element APIs, while
  `Window::insert_hitbox()` and `Window::on_mouse_event()` only provide retained
  hit testing and ordinary mouse observation.
- If using a GPUI patch, keep the API split and minimal:
  `Window::on_hitbox_drag<T, W>(hitbox, value, preview_constructor)` starts a
  typed drag from an existing retained hitbox with the same payload, preview
  entity, cursor offset, accepted transfer modes, cancellation, and external
  drop semantics as `Interactivity::on_drag`.
- The matching target side is
  `Window::on_hitbox_drag_move<T>(hitbox, listener)`,
  `Window::can_drop_on_hitbox<T>(hitbox, predicate)`, and
  `Window::on_hitbox_drop<T>(hitbox, listener)`. These callbacks must use the
  same active-drag payload source and dispatch ordering as
  `Interactivity::on_drag_move` / `Interactivity::on_drop`, but they must not
  require a visible or layout-owning `Div`.
- The API must be registered from retained paint/prepaint state against
  `HitboxId`, not from row/item GPUI element identity. It must not require
  recreating a visual GPUI row or item element as the drag source or target.
- If no patch is accepted, keep drag-start shells and the Places sidebar typed
  payload bridge, while continuing to reduce their visual/identity role to
  zero.

Default may change only when:

- Compact/Icons, Details, and Places all pass DnD smoke.
- Drag preview position remains stable across Compact, Icons, Details, and
  Places at different window sizes.
- Renderer-policy logs show shell removal without losing retained interaction
  counts.
- Places full retained-event logs pass with `gpui_typed_dnd_payload_shells=0`,
  and item/details policy logs show drag-start shell removal without adding
  replacement visual GPUI rows.

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

1. Keep the retained MIME/theme icon image cache foundation aligned with
   `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md`.
2. Continue Track 3a by reducing full-handoff first-frame total-render
   variance and keeping `--places-full-handoff` A/B evidence current.
3. Re-audit GPUI drag-start API after dependency updates before Track 4.
4. Convert rename behavior matrix items into tests/smoke before Track 5.

This queue is intentionally evidence-first. It moves the codebase toward full
retained reuse while preserving the current rule: custom paint only stays
default when it is at least as good as the GPUI path for that surface.
