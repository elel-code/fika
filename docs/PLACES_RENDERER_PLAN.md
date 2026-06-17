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

## Current Fika Boundary

Current ownership is already close to the Dolphin split:

- Model/order/device rows: `src/ui/places/model.rs` plus `src/ui/places/user/*`.
  Primary place ordering is persisted through `place_order_path`.
- Snapshot projection: `src/ui/places/projection.rs` maps active, hidden,
  drop-target, insert-indicator, trash, device, and icon state into
  `PlaceSnapshot`.
- GPUI row shell: `src/ui/places/sidebar/row.rs` builds row visuals, context
  menu routing, activation, drag start, and row-level DnD shell wiring.
- DnD geometry and preview: `src/ui/places/drag.rs` owns insert zones, reorder
  indices, export payload, and cursor-offset-compensated preview layout.
- Sidebar scroll: `src/ui/places/sidebar.rs` owns the GPUI scroll container and
  the current custom scrollbar canvas/hitbox.

## Proposed Retained Design

Do not replace the GPUI Places row renderer in one step. The target design is a
retained Places row surface with the same separations as file-grid:

- `places/paint_slots.rs`: retain `PlacePaintSlot` and section-heading slots.
  A place slot key should be stable by semantic identity, preferring device id
  for device rows and path/group for normal places. Slot stats should separate
  inserted, content changed, geometry changed, visual changed, unchanged, and
  removed rows.
- `places/interaction.rs`: retain row hitboxes for activation, context menu,
  drop target lookup, insert zones, and hover/cursor. Drag start remains a GPUI
  shell until the GPUI drag-start boundary changes.
- `places/visual.rs`: paint row background, active/drop states, label, trash
  marker, and insert indicators from retained snapshots. Icon rendering remains
  a separate renderer-policy decision; GPUI theme-icon elements may stay if they
  remain faster or more stable than custom image painting.
- `places/renderer_policy.rs`: log how many rows are custom-painted, GPUI icon
  elements, retained interaction hitboxes, drag-start shells, section headings,
  and scrollbar surfaces. This mirrors item-view renderer-policy logs.
- `places/perf.rs`: add `FIKA_PERF_PLACES_VIEW=1` timing for snapshot
  projection, slot projection, row visual prepaint/paint, icon path, scrollbar
  paint, and total sidebar build.

## Migration Order

1. Add Places perf and renderer-policy logs around the current GPUI sidebar.
   This is the baseline. No default renderer change is allowed before this.
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
4. Move hover/drop hit testing into retained Places interaction while keeping
   GPUI drag-start shells. Verify item-to-place, place-to-pane, external
   path-to-place, and reorder targets.
   Current implementation has `places/interaction.rs` owning the row/section
   target decision for item/external path drops and place reorders. GPUI row and
   section shells still provide event delivery and bounds, so
   `retained_interaction=0` remains correct until row hitboxes move out of GPUI.
5. Add a custom row visual painter behind an opt-in flag. Compare against the
   current GPUI row path for scroll and DnD.
   Current implementation provides `FIKA_CUSTOM_PLACES_ROWS=1` as an opt-in
   row visual painter for row background, active/drop state, label, trash
   marker, and insert indicator. It keeps GPUI icons, row event delivery,
   context menus, row DnD, and drag-start shells. The default path remains the
   current GPUI row renderer.
6. Switch the default only if the retained row painter is behavior-complete and
   perf-neutral or better. Otherwise keep the Dolphin-aligned model/projection
   and leave row rendering on GPUI.

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

## Current Baseline Smoke

2026-06-17 desktop-session command:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 target/debug/fika /etc > /tmp/fika-places-baseline.log 2>&1
scripts/analyze-places-perf.sh --expect-current-gpui-policy /tmp/fika-places-baseline.log
```

The current GPUI sidebar logs `source=11 visible=11 sections=2`, with
`rows=11 sections=2 elements=13`. Repeated cold first snapshots were around
`4.3ms`; steady snapshot frames were roughly `58-133us`. Sidebar row build was
usually `185-270us`, with occasional frames around `0.5-0.6ms`.
Renderer-policy logs showed the expected current state: `row_gpui=11`,
`row_visual_layer=0`, `icon_gpui=11`, `retained_interaction=0`,
`drag_shell=11`, `section_gpui=2`, and `scrollbar_canvas=1`.

After the retained slot cache landed, the same perf run also emits
`[fika places-slots]`. For the default `/etc` sidebar, the first projection has
`rows=11 sections=2 entries=13 inserted=13`; steady frames should move to
`unchanged=13`, with observed projection time around `21-46us` on the
2026-06-17 desktop session. Target-projection smoke should show visual changes
for drop or insert state without content or geometry churn.

## Current Autosmoke

2026-06-17 desktop-session command:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --expect-current-gpui-policy /tmp/fika-places-targets.log
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

The analyzer summary for the current GPUI baseline should include:

```text
places_slots_frames=... max_inserted=13 max_content=0 max_geometry=0 max_visual=2 max_unchanged=13 max_removed=0
places_renderer_policy_frames=... max_row_gpui=11 max_row_visual_layer=0 max_icon_gpui=11 max_retained_interaction=0 max_drag_shell=11
places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
```

## Overflow Autosmoke

For Places scroll/overflow evidence, run:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-default.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --expect-current-gpui-policy /tmp/fika-places-overflow-default.log
```

`FIKA_AUTOSMOKE_PLACES=overflow` appends 64 non-persistent test rows at the
snapshot layer. It does not write user Places configuration or mutate
`self.places`. The expected evidence is `visible=75`, an extra `Autosmoke`
section, `[fika places-scrollbar] visible=1`, and `max_scroll_y>0`.

2026-06-17 default GPUI overflow evidence:

```text
places_sidebar_frames=7 max_rows=75 max_sections=3 max_elements=78 max_build=3083us
places_renderer_policy_frames=7 max_row_gpui=75 max_row_visual_layer=0 max_icon_gpui=75
places_scrollbar_frames=7 max_visible=1 max_scroll_y=1909.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

## Opt-In Row Visual Smoke

The custom Places row visual path is experimental and must stay opt-in until it
beats or matches the GPUI row baseline. Run it with:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --expect-custom-row-visual-policy /tmp/fika-places-custom-rows.log
```

For overflow comparison, switch the scenario and analyzer gate:

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom.log
```

Expected policy shape:

```text
places_renderer_policy_frames=... max_row_gpui=0 max_row_visual_layer=11 max_icon_gpui=11 max_retained_interaction=0 max_drag_shell=11
places_row_visual_frames=... max_rows=1 max_prepaint=...us max_paint=...us
```

`max_rows=1` is expected because the current opt-in implementation paints one
canvas per row. This is acceptable for the first benchmark slice; a later
retained row/section layer can aggregate row visuals if the per-row canvas
overhead loses to the GPUI baseline.

2026-06-17 first opt-in desktop-session evidence:

```text
default: places_sidebar max_build=631us, max_row_gpui=11, max_row_visual_layer=0
custom: places_sidebar max_build=547us, max_row_gpui=0, max_row_visual_layer=11
custom: places_row_visual_frames=110 max_rows=1 max_prepaint=148us max_paint=921us
```

The opt-in path passed the non-destructive target/insert/clear autosmoke and
proved the renderer-policy split, but it is not default-ready. The high
per-row `max_paint` came from the first cold frames; later rows in the same log
were typically around `14-33us` paint each. Before replacing the default GPUI
row renderer, collect scroll/DnD behavior evidence and decide whether per-row
canvas overhead should be collapsed into a retained sidebar visual layer.

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
- Scroll/paint evidence shows no regression against the current GPUI sidebar
  baseline. A custom Places painter that loses to GPUI must stay behind an
  opt-in flag or be removed.
