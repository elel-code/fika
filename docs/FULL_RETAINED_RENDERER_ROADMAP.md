# Full Retained Renderer Roadmap

This document is the execution entry point for the post-Places-chrome direction.
It complements:

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
- Compact/Icons MIME/theme icons default to GPUI `img()` elements.
- Places text, icons, event delivery, context menus, DnD shells, and drag start
  remain GPUI.

These bridges are intentional platform or performance boundaries. They should
be removed only through the tracks below.

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

Required evidence:

- `/etc` and `~/Downloads` item-view logs with `FIKA_PERF_ITEM_VIEW=1`.
- `/etc` item-view `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll`.
- Details mode runtime evidence with `[fika details-visual]`,
  `[fika details-shape-cache]`, and retained interaction counts.
- DnD smoke with `FIKA_DEBUG_DND=1` covering pane item to pane directory,
  pane item to Places, Places to pane directory, and external path drop.
- Places default chrome targets, overflow, layout, and hit-test autosmoke logs.
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

Next design step:

- Define a retained icon image cache keyed by at least
  `(icon_name, icon_size_px)` plus theme, scale, and color-scheme inputs when
  they affect the selected path.
- Preserve the last loaded same-key real image during refresh.
- Keep thumbnail retention keyed by thumbnail path, not icon name.
- Keep GPUI image cache as the decode backend unless a replacement beats it.

Default may change only when:

- Paired default GPUI `img()` and custom icon-renderer logs pass for `/etc` and
  a mixed user directory.
- Custom logs show no steady `theme_placeholder` churn, no zoom-time
  `theme_decoded` burst, no visible icon size second-jump, and no synchronous
  icon work regression.
- `docs/ITEM_VIEW_RENDERER_DECISIONS.md` is updated with the evidence.

### Track 3: Places Retained Event Delivery

Purpose: move Places from GPUI row event shells to retained hitboxes without
changing text/icon renderer policy.

Next design step:

- Add retained row/section hitbox delivery for activation, context menu,
  on-place drop target, insert-before/after, sidebar leave clearing, and cursor
  state.
- Keep GPUI drag-start shells until Track 4 unlocks retained drag start.
- Keep default row chrome custom and text/icons GPUI.

Default may change only when:

- `scripts/analyze-places-perf.sh --expect-retained-event-policy` passes for
  targets, overflow, layout, hit-test, and DnD-specific smoke.
- Context menus still distinguish blank sidebar, section, bookmark, trash, and
  device rows.
- Internal reorder and item/external drop behavior remain unchanged.

### Track 4: Drag Start Boundary

Purpose: remove temporary GPUI drag shells only if GPUI exposes or Fika carries
an audited retained-hitbox drag-start API.

Next design step:

- If using a GPUI patch, specify the smallest API to start a typed drag from a
  retained hitbox while preserving payload, preview entity, cursor offset,
  accepted transfer modes, cancellation, and external drop behavior.
- If no patch is accepted, keep drag-start shells and continue reducing their
  visual/identity role to zero.

Default may change only when:

- Compact/Icons, Details, and Places all pass DnD smoke.
- Drag preview position remains stable across Compact, Icons, Details, and
  Places at different window sizes.
- Renderer-policy logs show shell removal without losing retained interaction
  counts.

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
- Places event-delivery lifecycle once Track 3 starts.

Acceptance:

- No behavior changes without a paired runtime log.
- Module tests own invariants where the state is owned.
- `src/main.rs` becomes a coordinator of pane/app state, not owner of
  renderer internals.

## Next Queue

1. Add a small smoke/evidence checklist update that makes Track 1 repeatable
   for both item-view and Places before any more shell removal.
2. Design the retained MIME/theme icon image cache for Track 2.
3. Design and gate retained Places event delivery for Track 3.
4. Re-audit GPUI drag-start API after dependency updates before Track 4.
5. Convert rename behavior matrix items into tests/smoke before Track 5.

This queue is intentionally evidence-first. It moves the codebase toward full
retained reuse while preserving the current rule: custom paint only stays
default when it is at least as good as the GPUI path for that surface.
