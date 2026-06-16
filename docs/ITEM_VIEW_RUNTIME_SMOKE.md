# Item View Runtime Smoke

This checklist verifies the runtime behavior that unit tests cannot fully cover
after the retained/custom-painted item view migration.

## Scope

Run this after any slice that removes an item/row shell handler, expands a
custom painter, or changes drag/drop routing.

Required view modes:

- Compact
- Icons
- Details

Recommended launch command:

```sh
FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads
```

To save and summarize logs:

```sh
FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads 2>&1 | tee /tmp/fika-item-view.log
scripts/analyze-item-view-perf.sh \
  --require-steady \
  --require-details \
  --require-interaction \
  --steady-total-us 1000 \
  /tmp/fika-item-view.log
```

## Drag And Drop

For each view mode:

- Start dragging a file item and confirm the preview stays near the cursor.
- Drop one file onto a visible directory item; the drop menu/cursor should target
  that directory.
- Drop one file onto blank pane space; the drop menu/cursor should target the
  pane directory.
- Drag an item over Places and confirm place drop targets update, then leave the
  sidebar and confirm the target clears.
- Reorder a movable primary Place and restart Fika; the order should persist.
- Drop one external folder onto Places; it should be added at the insert target.
- Drop external paths onto a directory item and onto blank pane space; both
  should use the same target logic as internal item drags.

## Rename

For Compact and Icons:

- Start rename on a normal item.
- Click inside the rename editor and confirm the caret moves to the clicked text
  position.
- Edit non-ASCII text and confirm selection/caret movement stays UTF-8 safe.
- Press Tab from an active rename and confirm rename-next follows model order.

For Details:

- Start rename from a Details row and confirm the normal row visual remains
  painted while the rename overlay receives text input.

## Perf Log Review

Exercise this sequence while `FIKA_PERF_ITEM_VIEW=1` is enabled:

- cold launch into `~/Downloads`
- switch Compact -> Icons -> Details -> Compact
- resize the window narrower/wider several times
- toggle fullscreen and return

Expected log properties:

- `[fika item-view]` includes `phase=initial|mode-switch|content-change|
  geometry-change|visual-change|steady`.
- Cold `initial` and `mode-switch` frames may show cache warm-up cost.
- Warm `steady` resize/fullscreen item snapshot conversion should stay
  sub-millisecond on ordinary directories.
- `[fika file-grid]` build time should not show a new sustained multi-ms steady
  regression after shell-removal slices.
- Details runs should emit `[fika details-visual]` and
  `[fika details-shape-cache]` so custom painter and text-shape costs remain
  attributable.
- `[fika item-interaction]` hitbox count should match the visible retained
  interaction items for Compact/Icons and Details.

Use `scripts/analyze-item-view-perf.sh` as the first pass. It summarizes
item-view phases, file-grid build maxima, Details visual/shape-cache activity,
and retained interaction hitbox activity. Human review is still required for
whether the exercised mode switches, resizes, fullscreen toggles, and DnD
actions match this checklist.

## Decision Gate

Do not remove the remaining drag-start shells unless one of these is true:

- GPUI exposes a public custom-element drag-start API.
- Fika carries a small audited GPUI patch that exposes drag-start for retained
  hitboxes.

If a custom-painted surface loses to GPUI built-ins in steady perf or behavior
completeness, keep the Dolphin-aligned retained model and leave that surface on
the GPUI renderer until a narrower migration is justified.
