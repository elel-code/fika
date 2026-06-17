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

For drag/drop routing diagnostics, add `FIKA_DEBUG_DND=1`:

```sh
FIKA_DEBUG_DND=1 cargo run -- ~/Downloads
```

To save and summarize logs:

```sh
FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads 2>&1 | tee /tmp/fika-item-view.log
scripts/check-item-view-runtime-log.sh /tmp/fika-item-view.log
```

## Drag And Drop

For each view mode:

- Start dragging a file item and confirm the preview stays near the cursor.
- While dragging a pane item over a visible pane directory, confirm the directory
  highlights before drop. With `FIKA_DEBUG_DND=1`, this should emit
  `active-item-move ... kind=Some(Directory)`; `item-start` without a later
  active move line means the active item-drag hover path is not running.
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

Expected DnD debug interpretation:

- `item-start`: GPUI drag-start shell created the item drag payload.
- `active-item-move via=window`: Fika's retained interaction layer is tracking
  the active pane item drag from window mouse movement.
- `active-item-move via=preview`: GPUI did not deliver the underlying pane move
  callback, so Fika is using the active drag preview repaint to run the same
  retained pane hit-test update.
- Both active item move paths are driven by Fika's `ActiveItemDrag` state, not
  by GPUI `MouseMoveEvent::dragging()`, because platform move events during an
  active drag may not report a pressed button.
- `viewport-place-move`: Places-to-pane drag is using the viewport retained
  hit-test path.
- `directory-shell-hit`: a visible directory shell asserted a positive target;
  this is helpful but not sufficient for pane self-drag hover because GPUI may
  skip per-element drag-move callbacks after drag start.

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
- `[fika renderer-policy]` should appear for Compact, Icons, and Details and
  show how many visible surfaces are using the visual layer, image layer,
  retained interaction layer, GPUI drag-start shell, and rename overlay for
  each exercised mode. Each surface count must be internally consistent with
  the logged item count; impossible counts are not valid renderer evidence.

Use `scripts/analyze-item-view-perf.sh` as the first pass. It summarizes
item-view phases, file-grid build maxima, Compact/Icons static custom visual
activity, image paint activity when the directory exercises image-backed icons
or thumbnails, aggregate custom paint maxima, Details visual/shape-cache
activity, retained interaction hitbox activity, and renderer-policy surface
counts. It rejects renderer-policy counts that cannot fit inside the logged item
count. Human review is still required for whether the exercised mode switches,
resizes, fullscreen toggles, and DnD actions match this checklist.

After a passing runtime review, update
`docs/ITEM_VIEW_RENDERER_DECISIONS.md` with the evidence for any surface whose
renderer will be kept, expanded, or reverted.

The runtime-log gate and analyzer itself can be checked with:

```sh
scripts/check-item-view-runtime-log.sh --help
scripts/summarize-item-view-renderer-evidence.sh --help
scripts/check-item-view-perf-analyzer.sh
```

## Decision Gate

Do not remove the remaining drag-start shells unless one of these is true:

- GPUI exposes a public custom-element drag-start API.
- Fika carries a small audited GPUI patch that exposes drag-start for retained
  hitboxes.

If a custom-painted surface loses to GPUI built-ins in steady perf or behavior
completeness, keep the Dolphin-aligned retained model and leave that surface on
the GPUI renderer until a narrower migration is justified.
