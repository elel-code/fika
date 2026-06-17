# Item View Runtime Smoke

This checklist verifies the runtime behavior that unit tests cannot fully cover
after the retained/custom-painted item view migration.

## Scope

Run this after any slice that removes an item/row shell handler, expands a
custom painter, or changes drag/drop routing.

Runtime evidence should be automated whenever GPUI exposes enough structured
signals. Prefer paired launch commands, saved logs, and analyzer scripts over
manual visual-only judgment. Manual review is still allowed for behavior that
has no reliable log signal yet, but any repeated investigation should become a
scripted evidence path before the next renderer decision.

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
- A valid pane self-drag can report only `via=preview`. The important acceptance
  signal is that the same drag reaches `kind=Some(Directory)` before drop and
  the directory highlight updates while hovering.
- `viewport-place-move`: Places-to-pane drag is using the viewport retained
  hit-test path.
- `directory-shell-hit`: a visible directory shell asserted a positive target;
  this is helpful but not sufficient for pane self-drag hover because GPUI may
  skip per-element drag-move callbacks after drag start.

Pane item self-drag root cause:

- The remaining GPUI `Div::on_drag` shell is a drag initiation boundary, not the
  owner of hover state after the drag starts.
- Places-to-pane drag stays responsive because the viewport drag-move path keeps
  delivering target updates while the drag is moving.
- Pane-item-to-pane-directory drag is different: after the same-window item
  drag starts, GPUI can keep only the drag preview moving and stop delivering
  reliable move callbacks to the underlying pane/item elements. Earlier fixes
  that depended on per-element `on_drag_move`, directory shell hits, or a window
  mouse event gated by `MouseMoveEvent::dragging()` could therefore update only
  at drop time or not at all while hovering.
- The stable owner is now Fika's `ActiveItemDrag` state. Both `via=window` and
  `via=preview` call the same retained pane hit-test from the current window
  pointer position. The preview path exists because GPUI reliably repaints the
  drag preview during the active drag, so it can drive the same target update
  even when the underlying pane move event is absent.

Verified pane self-drag trace from 2026-06-17:

```text
[fika dnd] item-start pane=1 path=/home/yk/.viminfo selected=true selection_count=1
[fika dnd] active-item-move via=preview source_pane=1 target_pane=1 pos=(592.9,653.8) kind=Some(Pane) changed=true sources=/home/yk/.viminfo
[fika dnd] active-item-move via=preview source_pane=1 target_pane=1 pos=(476.7,648.6) kind=Some(Pane) changed=false sources=/home/yk/.viminfo
[fika dnd] active-item-move via=preview source_pane=1 target_pane=1 pos=(470.7,648.6) kind=Some(Directory) changed=true sources=/home/yk/.viminfo
[fika dnd] active-item-move via=preview source_pane=1 target_pane=1 pos=(467.7,648.6) kind=Some(Directory) changed=false sources=/home/yk/.viminfo
```

This trace is the important distinction between the broken and fixed states.
The drag start shell created the payload, but no pane/item element move callback
was required after that. While the cursor was over blank pane space, the
retained hit-test returned `kind=Some(Pane)`. As soon as the preview repaint
queried the current mouse position over the directory bounds, the same retained
hit-test returned `kind=Some(Directory)` with `changed=true`; later moves over
the same directory correctly stayed `changed=false` because the active target
was already installed.

The earlier visible symptom, "directory highlights only at drop time", was
therefore not caused by wrong directory geometry, wrong drop-target styling, or
a failed final drop dispatch. The missing piece was a continuous hover clock for
same-window pane item drags. Final drop still reached the pane drop handler, so
the target could be computed at the end, but hover could not repaint while the
cursor was moving. The accepted fix is to keep `ActiveItemDrag` as the single
owner of pane item-drag hover state and let the drag preview repaint path drive
the same retained hit-test when GPUI does not deliver pane-level move events.

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
  geometry-change|visual-change|steady` and `icon_sync=...us`.
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
item-view phases and stage maxima (`raw`, `icon_sync`, `queue`, `convert`),
file-grid build maxima, Compact/Icons static custom visual activity, image
paint activity when the directory exercises image-backed icons or thumbnails,
aggregate custom paint maxima, Details visual/shape-cache activity, retained
interaction hitbox activity, and renderer-policy surface counts. It rejects
renderer-policy counts that cannot fit inside the logged item count. Human
review is still required for whether the exercised mode switches, resizes,
fullscreen toggles, and DnD actions match this checklist.

For MIME/theme-icon image renderer A/B, repeat the same `~/Downloads` and
`/etc` runs with `FIKA_CUSTOM_THEME_ICONS=1`. The default run keeps retained
item snapshots and controller routing while rendering MIME/theme icons through
GPUI `img()` children; the override forces MIME/theme icons back through the
custom item-image layer. Compare `renderer-policy gpui_image_element` counts
and the absence or presence of theme-icon `[fika item-image]` source churn.

Use `scripts/compare-item-image-renderers.sh` for this comparison:

```sh
FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 cargo run -- /etc 2>&1 | tee /tmp/fika-etc-custom-theme.log
FIKA_PERF_ITEM_VIEW=1 cargo run -- /etc 2>&1 | tee /tmp/fika-etc-default.log
scripts/compare-item-image-renderers.sh /tmp/fika-etc-custom-theme.log /tmp/fika-etc-default.log
```

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
