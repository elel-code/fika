# Places Retained Event Delivery Plan

This document is the implementation plan for Track 3 in
`docs/FULL_RETAINED_RENDERER_ROADMAP.md`. It covers event delivery only. It does
not change the current renderer policy: Places row chrome is custom by default,
while row text, icons, context menu rendering, DnD preview creation, and drag
start remain on GPUI unless a later gate proves otherwise.

## Dolphin Boundary

Dolphin's Places panel uses `KFilePlacesView` over `DolphinPlacesModel`. The
view owns user interaction and delegates model/order/device decisions to the
model and Dolphin action layer. The renderer/delegate does not own Places
ordering, device state, context-menu semantics, or drop rules.

The Fika equivalent is:

- `places/model.rs`, `places/user/*`, and app commands own Places data and
  mutation.
- `places/projection.rs` owns projected row state.
- `places/interaction.rs` owns row/section geometry, hit testing, drop-zone
  mapping, and target decisions.
- A retained event layer may deliver pointer and DnD events, but it must call
  the existing app methods for activation, context menus, drops, and cursor
  updates.
- GPUI row shells stay only for drag start until a retained-hitbox typed-drag
  API exists.

## Current State

Already implemented:

- Retained row/section geometry through `places_interaction_geometry()`.
- Retained row/section hit tests through `PlacesInteractionGeometry::hit_test_y()`.
- Retained target-decision helpers for item/external path drops and place
  reordering.
- Analyzer support for both current GPUI event shells and the future retained
  event policy.
- An explicit `PlacesEventDeliveryPolicy`. The default remains `GpuiShells`.
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe` only reports the
  row/section hitbox count that a future retained layer would need; it keeps
  `retained_hitboxes=0` and `gpui_event_shells=rows+sections`.
- Default custom row chrome with GPUI text/icons/event shells.

Current policy shape:

```text
retained_hitboxes=0
gpui_event_shells=rows+sections
drag_shells=rows
```

Target policy shape before default promotion:

```text
retained_hitboxes=rows+sections
gpui_event_shells=0
drag_shells=rows
```

`drag_shells=rows` remains intentional. It is the GPUI typed drag-start
boundary, not an event-delivery failure.

## Retained Event Layer

Add one sidebar-level retained event layer over the scroll content, not one
GPUI event element per row. It should consume the same `PlaceSnapshot` list used
by the row visual layer and create:

- row hitbox records: visible index, place index, path, mounted/device/network
  state, label, device id, group, y/height, insert indexes, movable flag;
- section hitbox records: group, insert index, y/height;
- content height and scroll-local coordinate conversion;
- event counters for perf policy logging.

The layer should use `Window::insert_hitbox()` and `window.on_mouse_event()`
where GPUI supports retained hitboxes. If a GPUI event type cannot be delivered
from a retained hitbox, keep that event on the row shell and report the mixed
policy explicitly rather than claiming retained event delivery.

Coordinate rule:

```text
window position -> layer bounds -> content-local y -> PlacesInteractionGeometry::hit_test_y()
```

The content-local y must include the current scroll offset. The event layer must
share the scroll handle or equivalent scroll snapshot used by `places_sidebar`.

## Migration Phases

### Phase 1: Non-Mutating Retained Pointer Layer

Add an opt-in retained event layer that performs hit testing and logs policy
counts but does not mutate app state.

Acceptance:

- `retained_hitboxes=rows+sections` can be logged behind an opt-in policy.
- GPUI row shells still own activation, context menus, and DnD.
- Hit-test autosmoke still passes for current GPUI and opt-in custom row chrome.
- No user-visible behavior changes.

### Phase 2: Hover, Cursor, And Leave Clearing

Move hover/cursor state and sidebar leave clearing to the retained event layer.
This is the lowest-risk mutating step because it does not activate places,
open menus, or perform drops.

Acceptance:

- Row body and insert-edge cursor decisions match existing GPUI row DnD logic.
- Leaving the sidebar clears row/section drop targets for item, external path,
  and place drags.
- Current GPUI and opt-in custom visual policies both pass interaction geometry
  and targets autosmoke.

### Phase 3: Activation And Context Menu Targeting

Move left-click activation and right-click target selection to retained
hitboxes. Keep the existing app context-menu methods and GPUI menu rendering.

Acceptance:

- Normal place activation still passes path, device id, label, mounted, device,
  and network flags to `activate_place()`.
- Context menus still distinguish blank sidebar, section header, bookmark,
  trash, device, network, and mounted/unmounted rows.
- Blank sidebar context menu remains available outside row/section content.
- GPUI row shells no longer own click or context-menu callbacks.

### Phase 4: Drag Move And Drop Delivery

Move item/external path and place-drag move/drop delivery to retained hitboxes,
using the existing target-decision helpers and app drop methods.

Acceptance:

- Item/external path drops preserve insert-before, insert-after, and on-place
  behavior.
- Place reorder preserves no-op rejection and source-index adjustment.
- Place-to-pane drag remains unchanged.
- Drops still use current mouse position for menu/action placement.
- Analyzer `--expect-retained-event-policy` passes for targets, overflow,
  layout, hit-test, and a DnD-specific smoke.

### Phase 5: Remove GPUI Row/Section Event Shells

Remove row/section event callbacks after Phases 1-4 pass under both default
chrome and GPUI fallback visual policy. Keep only row drag-start shells.

Acceptance:

- Policy logs show `retained_interaction=rows+sections`,
  `retained_hitboxes=rows+sections`, `gpui_event_shells=0`, `drag_shells=rows`.
- `scripts/analyze-places-perf.sh --expect-retained-event-policy` passes.
- DnD smoke covers item-to-place, external-to-place, place reorder,
  place-to-pane directory, and sidebar leave clearing.

## Analyzer And Smoke Work

Before Phase 4 default promotion, add or extend smoke for:

- retained hover/cursor/leave clearing;
- activation and context-menu target selection;
- DnD-specific retained event delivery with isolated user-place config;
- overflow hit testing with non-zero scroll offset;
- parity across `FIKA_PLACES_ROW_VISUAL_POLICY=gpui`, default `chrome`, and
  opt-in `full`.

The existing analyzer already rejects false retained-event claims. Do not loosen
that gate. Extend it only when a new retained event log surface is added.

2026-06-18 policy-probe slice:

- `src/ui/places/perf.rs` owns `PlacesEventDeliveryPolicy`, matching the
  renderer-policy pattern used by item view.
- `retained-probe` is deliberately not accepted as `RetainedHitboxes` or
  `retained`; the name keeps the mixed state explicit.
- `[fika places-renderer-policy]` and `[fika places-interaction-policy]` now
  include `event_policy=...` and `retained_probe_hitboxes=...`.
- `scripts/check-places-perf-analyzer.sh` proves the probe still passes the
  current GPUI-shell interaction boundary and fails
  `--expect-retained-event-policy`.
- No activation, menu, hover, drop, DnD, or drag-start behavior changed.

2026-06-18 retained probe layer slice:

- `src/ui/places/event_layer.rs` adds an opt-in sidebar-level event probe layer
  behind `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe`.
- The layer consumes `PlacesInteractionGeometry`, inserts one normal GPUI
  hitbox per retained row/section, and does not register event handlers, set
  cursor state, or mutate app state.
- `[fika places-event-probe]` reports `rows`, `sections`, inserted `hitboxes`,
  hovered hitboxes, and prepaint/paint time.
- `scripts/analyze-places-perf.sh --require-event-probe` verifies the layer
  hitbox count matches the retained-probe policy count.
- This is Phase 1 structure only. Phase 2 is still responsible for moving
  hover/cursor/leave clearing out of GPUI shells.

2026-06-18 retained pointer slice:

- `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-pointer` enables the same
  sidebar-level retained layer, but now it sets the pointing-hand cursor for
  activatable rows from retained row hitboxes.
- In that policy, per-row GPUI cursor styling is disabled; click, context menu,
  typed DnD move/drop, and drag start remain on GPUI row/section shells.
- The retained layer also observes active mouse-drag movement and clears the
  current Places drop target when the pointer leaves the retained layer bounds.
  Existing GPUI typed drag handlers remain as a fallback until Phase 4.
- `[fika places-event-probe]` includes `pointer=1` for this policy. The full
  retained-event analyzer gate still rejects it because
  `retained_hitboxes=0` and `gpui_event_shells=rows+sections`.

## TODO

- [x] Add a `PlacesEventDeliveryPolicy` with `GpuiShells` default and an
  explicit `RetainedProbe` opt-in. Keep logs explicit in mixed states and do
  not let probe logs satisfy the retained-event policy gate.
- [x] Add a retained sidebar event probe layer that can insert row/section
  hitboxes and report counts without changing behavior.
- [~] Move hover/cursor/leave clearing to the retained layer. Current status:
  `retained-pointer` moves pointer cursor ownership and active-drag leave
  clearing behind an opt-in retained layer, while GPUI row/section shells still
  own typed DnD move/drop delivery.
- [ ] Add unit coverage for content-local coordinate conversion with scroll
  offsets and section/row boundaries.
- [ ] Move activation/context-menu targeting to the retained layer.
- [ ] Add isolated DnD smoke for retained item/external/place drops.
- [ ] Move drag-move/drop delivery to the retained layer.
- [ ] Remove GPUI row/section event callbacks after analyzer gates pass.
- [ ] Keep GPUI row drag-start shells until Track 4 solves typed drag start.
