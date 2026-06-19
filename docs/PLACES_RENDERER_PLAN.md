# Places Renderer Plan

This plan covers the Places/sidebar surface only. It does not change the
current item-view renderer decision: item-view MIME/theme icons stay on GPUI
`img()` unless evidence proves a custom painter is neutral or better.

## Dolphin Reference

Dolphin's Places path is not a generic item-view clone:

- `src/dolphinplacesmodelsingleton.cpp` defines `DolphinPlacesModel` as a thin
  `KFilePlacesModel` specialization. Dolphin keeps the model authoritative and
  only adds Trash decoration, panel-lock group behavior, Ark DnD MIME acceptance
  for the view, and Ark drop rejection at the model boundary.
- `src/panels/places/placespanel.cpp` uses `KFilePlacesView` as the view. The
  panel enables drop-on-place, disables auto-resize items, persists icon size,
  rejects non-writable place drop targets during `dragMoveEvent`, delegates URL
  drops to `DragAndDropHelper::dropUrls`, connects device teardown signals, and
  injects Dolphin-specific context menu actions.

The Dolphin-aligned rule for Fika is therefore: keep Places model/order/device
semantics outside the renderer, and treat the row renderer as replaceable only
after the behavior gates are explicit.

For high-performance custom drawing, Dolphin's item-view implementation is
also a boundary rule, not a license to repaint everything every frame:

- `src/kitemviews/kitemlistview.cpp` creates widgets only for visible indexes,
  recycles invisible `KItemListWidget`s, and updates widget properties instead
  of rebuilding the whole view tree.
- `src/kitemviews/kitemlistwidget.cpp` and
  `src/kitemviews/kstandarditemlistwidget.cpp` use dirty flags for content,
  layout, and role changes. Paint work is refreshed only when the cached widget
  state is dirty.
- `KStandardItemListWidget::TextInfo` stores `QStaticText` with aggressive
  caching, so text layout/raster work is not repeated on every paint.
- Icon pixmaps are keyed through `QPixmapCache` by icon identity, size, device
  pixel ratio, and mode.

The corresponding Fika rule is: move row chrome first, keep text and icon
renderers on their fastest cached paths until Fika has retained/static text and
image caches that are measurably neutral or better.

## Current Fika Boundary

Current ownership is already close to the Dolphin split:

- Model/order/device rows: `src/ui/places/model.rs` plus `src/ui/places/user/*`.
  Primary place ordering is persisted through `place_order_path`.
- Snapshot projection: `src/ui/places/projection.rs` maps active, hidden,
  drop-target, insert-indicator, trash, device, and icon state into
  `PlaceSnapshot`.
- Retained row surface: `src/ui/places/visual.rs`,
  `src/ui/places/event_layer.rs`, and `src/ui/places/drag.rs` own default row
  visuals, activation/context-menu targeting, drag start, typed DnD target
  delivery, and row/section hitboxes.
- DnD geometry and preview: `src/ui/places/drag.rs` owns insert zones, reorder
  indices, export payload, and cursor-offset-compensated preview layout.
- Sidebar scroll: `src/ui/places/sidebar.rs` owns the GPUI scroll container and
  the current custom scrollbar canvas/hitbox.

## Proposed Retained Design

The retained Places row surface now follows the same separations as file-grid.
The old GPUI/chrome/text policies remain as explicit baselines, not as the
default path:

- `places/paint_slots.rs`: retain `PlacePaintSlot` and section-heading slots.
  A place slot key should be stable by semantic identity, preferring device id
  for device rows and path/group for normal places. Slot stats should separate
  inserted, content changed, geometry changed, visual changed, unchanged, and
  removed rows.
- `places/event_layer.rs` and `places/drag.rs`: retain row/section hitboxes for
  activation, context menu, drop target lookup, insert zones, hover/cursor,
  typed move/drop delivery, and drag start through the Fika GPUI fork.
- `places/visual.rs`: paint row background, active/drop states, label, icon,
  section heading, trash marker, and insert indicators from retained snapshots.
  Places icons share the retained image-cache/readiness model with pane images.
- `places/renderer_policy.rs`: log how many rows are custom-painted, GPUI icon
  elements, retained interaction hitboxes, drag-start shells, section headings,
  and scrollbar surfaces. This mirrors item-view renderer-policy logs.
- `places/perf.rs`: keep `FIKA_PERF_PLACES_VIEW=1` timing for snapshot
  projection, slot projection, row visual prepaint/paint, icon path, scrollbar
  paint, and total sidebar build.
- `places-interaction-policy`: log retained row/section target-decision counts
  separately from GPUI event-shell, typed-payload shell, and drag-start shell
  counts. The default retained-DnD state requires retained hitboxes for
  rows+sections and zero GPUI DnD shell counts.
- `places_interaction_geometry()`: project row and section y/height data from
  the same row/section constants as the visual layer. It is the retained
  geometry boundary consumed by row/section hitboxes.
- `PlacesInteractionGeometry::hit_test_y()`: convert content-local y
  coordinates into retained row/section hits. Row hits reuse the same
  edge/body `PlaceDropZone` rule as the legacy GPUI row DnD handlers, so the
  retained hitbox layer preserves insertion and on-place semantics.
- `src/ui/places/autosmoke.rs`: own retained hit-test autosmoke reporting and
  sample expectations. `src/main.rs` only supplies the current `PlaceSnapshot`
  projection, so runtime evidence stays beside the Places interaction model
  instead of becoming another app-root geometry owner.
- `src/ui/places/autosmoke.rs`: own Places snapshot autosmoke summaries for
  visible rows, section transitions, active rows, place targets, and insert
  indicators. `src/main.rs` applies scenario actions, but it does not compute
  target/overflow/layout evidence counts.
- `src/ui/places/autosmoke.rs`: own Places DropTargets action-report
  formatting for target path labels, insert indexes, and clear-target markers.
  `src/main.rs` still chooses the target and mutates drop-target state, but it
  does not own the marker model consumed by the analyzer.
- `src/ui/places/autosmoke.rs`: own the DropTargets first-target selection
  rule over `PlaceSnapshot` data. `src/main.rs` applies the selected path to
  mutable app state, but it does not decide which projected Places row the
  non-destructive smoke targets.
- `src/ui/places/autosmoke.rs`: own Places autosmoke start/complete marker
  formatting and stable scenario labels. `src/main.rs` schedules the scenario,
  but analyzer-facing marker strings no longer depend on app-root `Debug`
  formatting.
- `src/ui/places/autosmoke.rs`: own Places layout autosmoke reporting for
  sidebar width/visibility state, deterministic resize targets, action log
  formatting, and saved-settings verification. `src/main.rs` keeps the actual
  app mutations and settings load, but it does not own the report model that
  proves hide/show/resize/reset/restore behavior.
- Row rendering policy is three-state:
  `FIKA_PLACES_ROW_VISUAL_POLICY=gpui` keeps the old GPUI row renderer, the
  default `chrome` policy paints row background, active/drop state, trash
  marker, and insert indicators in one sidebar-level custom layer while keeping
  GPUI text and icons, and `FIKA_CUSTOM_PLACES_ROWS=1` / `full` keeps the full
  custom text benchmark path.

Panel layout belongs beside the Places view/controller boundary, not inside the
row painter. The Places panel has state for visibility and width; the sidebar
splitter updates that width and asks the pane row to remeasure item viewports
through the same next-frame resize path as pane splitters. The toggle button
hides or shows the panel without destroying pane state. Width/visibility are
saved to `$XDG_CONFIG_HOME/fika/settings.tsv` through a latest-only delayed
settings save task. Drag frames update memory and bump a generation counter;
the actual file write is delayed by 120ms and runs on the GPUI background
executor, so sidebar dragging does not synchronously write config files.
Future panel polish should remain in this layout/controller layer: expose the
same width and hidden state through the main menu or shortcut surface, keep a
usable splitter hit target, preserve the last non-hidden width when toggling,
and prove through `FIKA_AUTOSMOKE_PLACES=layout` that hiding/resizing does not
reset pane scroll, selection, Places ordering, or the active renderer policy.
Dolphin wires the Places panel through a dock action named `show_places_panel`
with `Qt::Key_F9` in `src/dolphinmainwindow.cpp`, so Fika mirrors that
shortcut for the Places panel toggle while keeping the toolbar button and the
splitter on the same app-level layout state.

2026-06-18 runtime layout smoke kept both renderer policies intact after adding
that panel state and settings persistence:

```bash
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-targets-sidebar-layout.log
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-overflow-sidebar-layout.log
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-sidebar-layout.log
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-layout.log
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-layout-custom.log
```

## Migration Order

1. Add Places perf and renderer-policy logs around the historical GPUI sidebar
   baseline. No default renderer change was allowed before this.
   Current implementation uses `FIKA_PERF_PLACES_VIEW=1` to emit
   `[fika places-view]`, `[fika places-sidebar]`, and
   `[fika places-renderer-policy]` for the existing GPUI sidebar.
2. Add a deterministic runtime smoke path for Places if manual testing is still
   needed for reorder/drop/scroll. Prefer the same pattern as
   `FIKA_AUTOSMOKE_ITEM_VIEW`.
   Current implementation uses `FIKA_AUTOSMOKE_PLACES=targets` for a safe,
   non-persistent target-projection smoke. It sets a place target, start/end
   insert targets, clears the target, and logs snapshot counts after each step.
   It intentionally does not reorder or add bookmarks.
3. Add retained paint slots and stats without changing visible rendering.
   Confirm primary order persistence and hidden-section projection still pass
   unit tests.
   Current implementation keeps `PlacePaintSlotCache` in app state and emits
   `[fika places-slots]` with row/section entries plus inserted/content/
   geometry/visual/unchanged/removed counts. It does not change the GPUI row
   renderer.
4. Move hover/drop hit testing and typed DnD into retained Places interaction.
   Verify item-to-place, place-to-pane, external path-to-place, and reorder
   targets.
   Current implementation has retained row/section hitboxes owning activation,
   context-menu targeting, drag start, typed move/drop delivery, and target
   decisions for item/external path drops and place reorders. GPUI row/section
   event and DnD shell counts must remain zero.
   `[fika places-interaction-policy]` is the retained policy log: target
   decisions, activation/context-menu targeting, drag/drop event delivery, and
   drag start are retained in the default path.
   `[fika places-interaction-geometry]` is the companion retained geometry
   projection. It must match row/section counts, and analyzer gates reject
   non-zero GPUI event, typed-payload, or drag shell counts for the default
   retained-DnD policy.
5. Add a custom row visual painter behind a renderer policy and compare against
   the GPUI row path for scroll and DnD.
   Current implementation defaults to the full retained/custom row visual path:
   row background, active/drop state, trash marker, insert indicator, text,
   icons, event delivery, and DnD are retained/custom. `gpui`, `chrome`, and
   `text` policies remain explicit comparison baselines.
6. Expand beyond chrome only if the retained row painter is behavior-complete
   and perf-neutral or better. This has been accepted for the default full path;
   future regressions should keep the Dolphin-aligned model/projection and use
   fallback renderer policies only as measured baselines.

## Runtime Evidence Rule

Places changes follow the same unattended-evidence rule as item-view changes:
repeatable behavior must be driven by `FIKA_AUTOSMOKE_PLACES` or a new isolated
runtime smoke before a renderer decision depends on it. The current
`targets` smoke is intentionally non-destructive, so reorder/drop persistence
still needs either an isolated user-place configuration or manual review until
that fixture exists.

Each Places optimization breakthrough must be recorded in this plan or the
renderer-decision document in the same slice. The record should name the user
visible symptom, the Dolphin Places source boundary used for comparison, the
root cause in Fika, the implementation change, the saved log/analyzer command,
and the regression guard that future Places work must run.

## Historical GPUI Baseline Smoke

2026-06-17 desktop-session GPUI-baseline command:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 target/debug/fika /etc > /tmp/fika-places-baseline.log 2>&1
scripts/analyze-places-perf.sh --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-baseline.log
```

That historical GPUI sidebar logged `source=11 visible=11 sections=2`, with
`rows=11 sections=2 elements=13`. Repeated cold first snapshots were around
`4.3ms`; steady snapshot frames were roughly `58-133us`. Sidebar row build was
usually `185-270us`, with occasional frames around `0.5-0.6ms`.
Renderer-policy logs showed the expected historical state: `row_gpui=11`,
`row_visual_layer=0`, `icon_gpui=11`, `retained_interaction=0`,
`drag_shell=11`, `section_gpui=2`, and `scrollbar_canvas=1`.

After the retained slot cache landed, the same perf run also emits
`[fika places-slots]`. For the default `/etc` sidebar, the first projection has
`rows=11 sections=2 entries=13 inserted=13`; steady frames should move to
`unchanged=13`, with observed projection time around `21-46us` on the
2026-06-17 desktop session. Target-projection smoke should show visual changes
for drop or insert state without content or geometry churn.

## Historical Target Autosmoke

2026-06-17 desktop-session GPUI-baseline command:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-targets.log
```

Expected markers:

```text
[fika autosmoke] places start scenario=DropTargets
[fika autosmoke] places action=target-first-place ... changed=true
[fika autosmoke] places snapshot=after-place-target ... place_targets=1
[fika autosmoke] places action=target-insert-start index=0 changed=true
[fika autosmoke] places snapshot=after-insert-start ... insert_before=1
[fika autosmoke] places action=target-insert-end ... changed=true
[fika autosmoke] places snapshot=after-insert-end ... insert_after=1
[fika autosmoke] places action=clear-targets changed=true
[fika autosmoke] places snapshot=after-clear ... place_targets=0 insert_before=0 insert_after=0
[fika autosmoke] places complete scenario=DropTargets
```

This smoke is deliberately non-destructive. A later Places smoke can cover
actual reorder/drop persistence only after it can run with isolated user-place
configuration or an explicit test fixture.

The analyzer summary for the historical GPUI baseline should include:

```text
places_slots_frames=... max_inserted=13 max_content=0 max_geometry=0 max_visual=2 max_unchanged=13 max_removed=0
places_renderer_policy_frames=... max_row_gpui=11 max_row_visual_layer=0 max_icon_gpui=11 max_retained_interaction=0 max_drag_shell=11
places_interaction_policy_frames=... max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0 max_gpui_event_shells=13 max_drag_shells=11
places_interaction_geometry_frames=... max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2
places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
```

2026-06-18 interaction policy evidence:

```text
/tmp/fika-places-targets-interaction.log:
  places_interaction_policy_frames=10 max_rows=11 max_sections=2 max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0 max_gpui_event_shells=13 max_drag_shells=11
/tmp/fika-places-custom-targets-interaction.log:
  places_interaction_policy_frames=14 max_rows=11 max_sections=2 max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0 max_gpui_event_shells=13 max_drag_shells=11
/tmp/fika-places-snapshot-autosmoke-module.log:
  places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
  places_renderer_policy_frames=10 max_rows=11 max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-layout-autosmoke-module.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  places_renderer_policy_frames=9 max_rows=11 max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-target-actions-autosmoke-module.log:
  places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
  places_renderer_policy_frames=10 max_rows=11 max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-first-target-autosmoke-module.log:
  places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
  places_renderer_policy_frames=12 max_rows=11 max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-start-complete-autosmoke-module.log:
  places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
  places_renderer_policy_frames=18 max_rows=11 max_row_gpui=11 max_row_visual_layer=0
```

2026-06-18 retained geometry evidence:

```text
/tmp/fika-places-targets-geometry.log:
  places_interaction_geometry_frames=10 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=4us
/tmp/fika-places-custom-targets-geometry.log:
  places_interaction_geometry_frames=9 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=3us
/tmp/fika-places-targets-hit-test.log:
  places_interaction_geometry_frames=12 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=4us
/tmp/fika-places-custom-targets-hit-test.log:
  places_interaction_geometry_frames=13 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=7us
```

## Retained Hit-Test Autosmoke

For non-destructive retained row/section hit-test evidence, run:

```bash
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=hit-test target/debug/fika /etc > /tmp/fika-places-retained-hit-test.log 2>&1
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy --expect-retained-event-policy /tmp/fika-places-retained-hit-test.log
```

For an explicit full-row stress path, add `FIKA_CUSTOM_PLACES_ROWS=1` and keep
the same retained-event analyzer policy:

```bash
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=hit-test target/debug/fika /etc > /tmp/fika-places-custom-retained-hit-test.log 2>&1
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy --expect-retained-event-policy /tmp/fika-places-custom-retained-hit-test.log
```

Expected markers:

```text
[fika autosmoke] places start scenario=HitTest
[fika autosmoke] places hit-test ... sample=row-before ... kind=Row zone=InsertBefore ... ok=true
[fika autosmoke] places hit-test ... sample=row-body ... kind=Row zone=OnPlace ... ok=true
[fika autosmoke] places hit-test ... sample=row-after ... kind=Row zone=InsertAfter ... ok=true
[fika autosmoke] places hit-test ... sample=section ... kind=Section zone=Section ... ok=true
[fika autosmoke] places hit-test-summary ... rows=... sections=... ok=true
[fika autosmoke] places complete scenario=HitTest
```

The analyzer summary should include:

```text
places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1
```

This is the retained geometry acceptance gate for the current row/section
event-delivery path.

2026-06-18 evidence:

```text
/tmp/fika-places-retained-hit-test.log:
  places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2
  places_interaction_geometry_frames=15 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=6us
  max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-custom-retained-hit-test.log:
  places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2
  places_interaction_geometry_frames=10 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=15us
  max_row_gpui=0 max_row_visual_layer=11
/tmp/fika-places-hit-test-autosmoke-module.log:
  places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2
  places_interaction_geometry_frames=11 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=4us
  max_row_gpui=11 max_row_visual_layer=0
```

## Retained Event Delivery Plan

Places event delivery was a separate migration from row visual painting. It is
now complete in the default retained-DnD path: retained row/section hitboxes own
activation clicks, context-menu targeting, drag move/drop callbacks, row hover,
cursor state, and drag start. Dolphin's `KFilePlacesView` keeps
model/order/device semantics outside the delegate renderer; Fika mirrors that by
keeping behavior in retained model/interaction state and rendering from
snapshots.

The implementation-level plan now lives in
`docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md`. Keep this section as the current
summary and use that document for historical phased notes.

Required evidence for the default retained policy:

```text
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-retained-event-policy ...
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-retained-event-policy ...
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-retained-event-policy ...
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-retained-event-policy ...
```

`--expect-retained-event-policy` rejects mixed claims where row visuals are
custom-painted but event delivery or typed DnD still depends on GPUI row shells.
The accepted default requires `gpui_event_shells=0`,
`gpui_row_section_event_shells=0`, `gpui_typed_dnd_payload_shells=0`,
`drag_shells=0`, and retained hitbox drag-start models for rows.

## Overflow Autosmoke

For Places scroll/overflow evidence, run:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-default.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy --expect-retained-event-policy /tmp/fika-places-overflow-default.log
```

`FIKA_AUTOSMOKE_PLACES=overflow` appends 64 non-persistent test rows at the
snapshot layer. It does not write user Places configuration or mutate
`self.places`. The expected evidence is `visible=75`, an extra `Autosmoke`
section, `[fika places-scrollbar] visible=1`, and `max_scroll_y>0`.

2026-06-17 historical GPUI overflow evidence:

```text
places_sidebar_frames=7 max_rows=75 max_sections=3 max_elements=78 max_build=3083us
places_renderer_policy_frames=7 max_row_gpui=75 max_row_visual_layer=0 max_icon_gpui=75
places_scrollbar_frames=7 max_visible=1 max_scroll_y=1909.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

## Layout Autosmoke

For Places panel width/visibility and settings persistence evidence, run with
an isolated config directory:

```bash
XDG_CONFIG_HOME=/tmp/fika-places-layout-config \
  timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=layout \
  target/debug/fika /etc > /tmp/fika-places-layout.log 2>&1
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy --expect-retained-event-policy /tmp/fika-places-layout.log
```

For the explicit full-row stress path, add `FIKA_CUSTOM_PLACES_ROWS=1` and keep
the same analyzer policy:

```bash
XDG_CONFIG_HOME=/tmp/fika-places-layout-custom-config \
  timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 \
  FIKA_AUTOSMOKE_PLACES=layout target/debug/fika /etc \
  > /tmp/fika-places-layout-custom.log 2>&1
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy --expect-retained-event-policy /tmp/fika-places-layout-custom.log
```

`FIKA_AUTOSMOKE_PLACES=layout` does not mutate user Places ordering. It captures
the startup panel state, hides the sidebar, shows it again, resizes it, resets
to the default width, restores the captured startup state, and verifies the
coalesced settings write by reading `$XDG_CONFIG_HOME/fika/settings.tsv`.

Expected markers:

```text
[fika autosmoke] places start scenario=Layout
[fika autosmoke] places action=layout-hide ... visible=false changed=true
[fika autosmoke] places action=layout-show ... visible=true changed=true
[fika autosmoke] places action=layout-resize ... changed=true
[fika autosmoke] places action=layout-reset ... changed=true
[fika autosmoke] places action=layout-restore ...
[fika autosmoke] places action=layout-verify-saved ... ok=true
[fika autosmoke] places complete scenario=Layout
```

The analyzer summary should include:

```text
places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
```

2026-06-18 evidence:

```text
/tmp/fika-places-layout.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-layout-custom.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  max_row_gpui=0 max_row_visual_layer=11
  places_row_visual_frames=8 max_rows=11
/tmp/fika-places-f9-layout.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  max_row_gpui=11 max_row_visual_layer=0
```

## Opt-In Row Visual Smoke

The custom Places row visual path is experimental and must stay opt-in until it
beats or matches the GPUI row baseline. Run it with:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-rows.log
```

For overflow comparison, switch the scenario and analyzer gate:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom.log
```

Expected policy shape:

```text
places_renderer_policy_frames=... max_row_gpui=0 max_row_visual_layer=11 max_icon_gpui=11 max_retained_interaction=0 max_drag_shell=11
places_row_visual_frames=... max_rows=11 max_prepaint=...us max_paint=...us
```

`max_rows` must match the renderer-policy row count. In this historical opt-in
implementation, one sidebar-level visual layer painted row backgrounds,
active/drop state, label, trash marker, and insert indicators while GPUI still
owned icons, row event delivery, context menus, DnD, and drag-start shells. The
analyzer rejected custom row visual logs that fell back to one canvas per row.

2026-06-17 first opt-in desktop-session evidence:

```text
default: places_sidebar max_build=631us, max_row_gpui=11, max_row_visual_layer=0
custom: places_sidebar max_build=547us, max_row_gpui=0, max_row_visual_layer=11
custom: places_row_visual_frames=110 max_rows=1 max_prepaint=148us max_paint=921us
```

The opt-in path passed the non-destructive target/insert/clear autosmoke and
proved the renderer-policy split, but it is not default-ready. The high
per-row `max_paint` came from the first cold frames; later rows in the same log
were typically around `14-33us` paint each. This was evidence for collapsing
per-row canvas overhead into a retained sidebar visual layer before the later
full retained/custom default was accepted.

2026-06-17 opt-in overflow evidence:

```text
places_sidebar_frames=9 max_rows=75 max_sections=3 max_elements=78 max_build=3968us
places_renderer_policy_frames=9 max_row_gpui=0 max_row_visual_layer=75 max_icon_gpui=75
places_row_visual_frames=675 max_rows=1 max_prepaint=249us max_paint=951us
places_scrollbar_frames=9 max_visible=1 max_scroll_y=1684.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

This confirms the first opt-in visual painter works under overflow, but it also
shows the expected cost of one canvas per row (`675` row-visual frames in this
5s smoke). That is evidence for the next renderer slice: aggregate Places row
visuals into a retained sidebar layer before considering a default switch.

2026-06-17 aggregated opt-in row visual evidence:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows-layer.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-rows-layer.log

timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom-layer.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom-layer.log
```

Targets summary:

```text
places_sidebar_frames=7 max_rows=11 max_sections=2 max_elements=13 max_build=938us
places_renderer_policy_frames=7 max_row_gpui=0 max_row_visual_layer=11 max_icon_gpui=11
places_row_visual_frames=7 max_rows=11 max_prepaint=1515us max_paint=7570us
places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
```

Overflow summary:

```text
places_sidebar_frames=11 max_rows=75 max_sections=3 max_elements=78 max_build=3289us
places_renderer_policy_frames=11 max_row_gpui=0 max_row_visual_layer=75 max_icon_gpui=75
places_row_visual_frames=11 max_rows=75 max_prepaint=12610us max_paint=16108us
places_scrollbar_frames=11 max_visible=1 max_scroll_y=1663.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

The root cause of the first opt-in overflow cost was structural: each row owned
its own canvas, so one sidebar frame with 75 rows produced 75 row-visual
prepaint/paint passes. Dolphin's `KFilePlacesView` keeps the model/view split
and does not make row rendering own Places ordering or device semantics, so
Fika can collapse row-only visuals without changing Places behavior. The
implementation moves row visuals into one absolute sidebar layer that computes
section-heading offsets from the same `PlaceSnapshot` stream. The regression
guard is `--expect-custom-row-visual-policy`, which now requires
`places_row_visual max_rows == places_renderer_policy max_rows` and fails the
old per-row `rows=1` shape.

The next opt-in row visual cost was text shaping, not Places model work. The
aggregated layer still reshaped every row label during each prepaint pass even
when the same `PlaceSnapshot` labels, font, and visual text color were stable.
Fika now mirrors the item-view text-cache pattern with an app-level
`PlacesRowTextShapeCache`, keyed by label/font/font-size/color. The cache is
used by the default `full` retained/custom path; the `chrome` baseline keeps
text on GPUI and must not emit this channel. Runtime logs include:

```text
[fika places-row-shape-cache] hits=... misses=... evicted=... compute=...us entries=...
```

`--expect-custom-row-full-policy` requires this shape-cache channel for the
default full row path, so future Places row painter changes cannot silently
return to per-frame row label shaping without runtime evidence.

2026-06-18 opt-in row text shape-cache evidence:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows-shape-cache.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-rows-shape-cache.log

timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom-shape-cache.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom-shape-cache.log
```

Targets summary:

```text
places_row_visual_frames=11 max_rows=11 max_prepaint=1139us max_paint=5175us
places_row_shape_cache_frames=11 max_hits=11 max_misses=11 max_evicted=0 max_entries=11
```

Overflow summary:

```text
places_row_visual_frames=6 max_rows=75 max_prepaint=9578us max_paint=8794us
places_row_shape_cache_frames=6 max_hits=75 max_misses=75 max_evicted=0 max_entries=75
```

The maxima include the cold first frame, where every visible row label is a
cache miss. The same overflow log then stabilizes at `hits=75 misses=0`, with
row visual prepaint around `148-176us`; the repeated row-label shaping cost is
removed from steady opt-in Places frames.

2026-06-18 visible-row custom layer update:

The aggregated custom row visual layer used to shape and paint every Places row
inside the scroll content, even when the GPUI scroll container clipped most of
those rows. The layer now uses `Window::content_mask()` during prepaint to
retain only rows intersecting the current clipped content bounds. Runtime logs
keep `rows` as the policy row count and add `painted` for the per-frame
visible-row count:

```text
[fika places-row-visual] rows=75 painted=29 prepaint=...us paint=...us
```

Evidence:

```bash
timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-targets-visible-rows.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-targets-visible-rows.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-custom-overflow-visible-rows.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-overflow-visible-rows.log
```

Targets summary:

```text
places_row_visual_frames=9 max_rows=11 max_painted=11 max_prepaint=1110us max_paint=5264us
places_row_shape_cache_frames=9 max_hits=11 max_misses=11 max_evicted=0 max_entries=11
```

Overflow summary:

```text
places_row_visual_frames=18 max_rows=75 max_painted=32 max_prepaint=2395us max_paint=8398us
places_row_shape_cache_frames=18 max_hits=31 max_misses=20 max_evicted=0 max_entries=49
```

Steady overflow frames are improved because only roughly `29-32` visible rows
are painted instead of all `75`; typical prepaint falls to `70-90us` once the
visible labels are cached and steady paint is around `0.6-0.7ms`. This is not
yet sufficient to make the custom Places row visual the default: the first two
frames still show glyph/raster cold-start paint spikes around `7-8ms` and must
be eliminated or proven neutral against the GPUI baseline before default
switching.

2026-06-18 Dolphin-aligned Places chrome policy update, superseded by the
2026-06-19 full retained/custom default:

The previous full custom row visual layer was not Dolphin-like enough to become
the default because it moved text into GPUI canvas painting and reintroduced
font/glyph cold-start spikes. Dolphin keeps item widgets visible-only and uses
static text and pixmap caches. This evidence led to the retained Places text
shape cache and icon cache work; after that work, the default policy moved to
the full retained/custom path. The narrower custom chrome path remains a
baseline:

- `FIKA_PLACES_ROW_VISUAL_POLICY=full` is the default.
- The custom layer paints row background, active/drop border, insert
  indicators, trash state, labels, section headings, and icons.
- `chrome` still paints only chrome while GPUI paints row text/icons, so the row
  shape-cache channel must stay absent in chrome logs.
- `FIKA_PLACES_ROW_VISUAL_POLICY=gpui` remains the baseline fallback.
- `FIKA_CUSTOM_PLACES_ROWS=1` remains an alias/stress path for full custom rows.

Historical runtime evidence:

```bash
timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-chrome-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-targets.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-chrome-overflow.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-overflow.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_PLACES_ROW_VISUAL_POLICY=gpui FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-gpui-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-gpui-targets.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-full-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy --expect-retained-event-policy /tmp/fika-places-full-targets.log
```

Historical chrome targets summary:

```text
places_renderer_policy_frames=10 max_rows=11 max_row_gpui=0 max_row_visual_layer=11 max_text_gpui=11 visual_kinds=chrome
places_row_visual_frames=10 max_rows=11 max_painted=11 max_prepaint=23us max_paint=83us
places_row_shape_cache_frames=0
```

Historical chrome overflow summary:

```text
places_renderer_policy_frames=6 max_rows=75 max_row_gpui=0 max_row_visual_layer=75 max_text_gpui=75 visual_kinds=chrome
places_row_visual_frames=6 max_rows=75 max_painted=29 max_prepaint=28us max_paint=148us
places_row_shape_cache_frames=0
```

Historical full custom-text comparison:

```text
places_renderer_policy_frames=10 max_rows=11 max_row_gpui=0 max_row_visual_layer=11 max_text_gpui=0 visual_kinds=full
places_row_visual_frames=10 max_rows=11 max_painted=11 max_prepaint=1046us max_paint=5183us
places_row_shape_cache_frames=10 max_hits=11 max_misses=11 max_entries=11
```

Additional historical chrome guards passed:

```bash
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-layout.log
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-hit-test.log
```

## Acceptance Gates

- Primary Places order persists across restart and dynamic device refresh does
  not rewrite user order.
- Hidden places and hidden sections remain projection-only state.
- Drop-on-place rejects non-writable/network targets consistently with the
  current rules, while internal reorder remains allowed for primary places.
- Context menus still distinguish blank sidebar, section header, normal place,
  editable/removable bookmark, trash, and device rows.
- Runtime smoke covers row activation, reorder insert-before/after, item drop
  to place, external path drop to place, place drag to pane directory, device
  teardown action visibility, and sidebar leave clearing.
- Scroll/paint evidence shows no regression against the explicit GPUI sidebar
  baseline. A custom Places painter that loses to GPUI must stay behind an
  opt-in flag or be removed.
- Sidebar width/visibility changes remeasure pane viewports without resetting
  pane content, scroll, selection, Places order, or the current renderer policy.
  Persistence for width/visibility must stay latest-only/coalesced; future
  settings additions should not write config files from every drag frame.
