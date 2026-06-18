# Places Retained Event Delivery Plan

This document is the implementation plan for Track 3 in
`docs/FULL_RETAINED_RENDERER_ROADMAP.md`. It covers event delivery only. It does
not change the row renderer policy: Places row chrome is custom by default,
while row text, icons, context menu rendering, DnD preview creation, typed drag
payload delivery, and drag start remain on GPUI unless a later gate proves
otherwise.

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
- Analyzer support for the explicit GPUI event-shell fallback, the current
  retained-DnD mixed default, and the future full retained event policy.
- An explicit `PlacesEventDeliveryPolicy`. The default is now `RetainedDnd`.
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` remains the explicit fallback.
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe` only reports the
  row/section hitbox count that a future retained layer would need; it keeps
  `retained_hitboxes=0` and `gpui_event_shells=rows+sections`.
- Default custom row chrome with GPUI text/icons, retained row/section
  activation/context-menu/DnD target delivery, one sidebar-level GPUI typed DnD
  payload shell, and GPUI row drag-start shells.

Default mixed policy shape:

```text
event_policy=retained-dnd
retained_hitboxes=rows+sections
retained_interaction=rows+sections
gpui_event_shells=1
drag_shells=rows
drag_start_models=rows
```

Full retained event policy shape:

```text
retained_hitboxes=rows+sections
retained_interaction=rows+sections
gpui_event_shells=0
drag_shells=rows
drag_start_models=rows
```

`drag_shells=rows` remains intentional. It is the GPUI typed drag-start
boundary, not an event-delivery failure. `drag_start_models=rows` records that
the payload, movable flag, export metadata, and preview model are owned by the
Places drag module; the row shell should only call GPUI's `on_drag` API.

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

2026-06-18 retained targeting slice:

- `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-targeting` extends the same
  sidebar-level retained layer to own row activation and row/section context
  menu targeting.
- The retained layer uses the inserted row/section hitboxes and
  `Hitbox::is_hovered()` for dispatch instead of recomputing pointer positions
  from raw scroll offsets. This matches the Dolphin direction: the viewport
  event layer owns target lookup, while the model/controller methods still own
  activation and menu state changes.
- In that policy, GPUI row `on_click`, row right-click, and section right-click
  shells are disabled. GPUI row/section shells still own typed DnD move/drop,
  and row shells still own drag-start.
- `[fika places-event-probe]` includes `pointer=1 targeting=1`, and
  `[fika places-interaction-policy]` includes `retained_targeting=rows+sections`.
  The full retained-event analyzer gate still rejects this mixed state because
  `gpui_event_shells=rows+sections`.

2026-06-18 retained DnD slice:

- `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-dnd` keeps the same retained
  layer and moves typed item, external-path, and place drag move/drop target
  lookup to retained `PlacesInteractionGeometry`.
- GPUI still exposes typed drag payloads only through `Div::on_drag_move` and
  `Div::on_drop`, so this slice deliberately uses one sidebar-level GPUI typed
  drag shell instead of row/section shells. The Dolphin alignment is the target
  lookup and state transition: a viewport event layer owns hit testing, while
  model/controller methods own drop target and drop execution.
- In this policy, row/section DnD move/drop shells are disabled. Row drag-start
  shells remain, because GPUI still starts app-internal drags from `Div::on_drag`.
- `[fika places-interaction-policy]` reports `retained_dnd=rows+sections` and
  `gpui_event_shells=1`. `[fika places-event-probe]` reports
  `pointer=1 targeting=1 dnd=1`. The full retained-event analyzer gate still
  rejects this state because `gpui_event_shells=1` and `drag_shells=rows`.

Retained DnD autosmoke slice:

- `FIKA_AUTOSMOKE_PLACES=dnd` now exercises retained Places DnD target
  decisions without mutating user Places ordering or writing bookmarks.
- The smoke samples path-list drags over a row body, a row edge, and a section
  heading, then samples a place drag over another row. The expected retained
  decisions are `Place`/`DropMenu`, `Insert`/`Copy`, `Insert`/`Copy`, and
  `Insert`/`Move`.
- `scripts/analyze-places-perf.sh --require-retained-dnd-autosmoke` rejects
  missing start/complete markers, missing sample coverage, failed sample
  decisions, or summaries without both row and section geometry. This gives the
  next drag-start / GPUI-shell-removal slices a non-destructive regression
  guard before any destructive reorder/drop smoke is added.

Retained drag-start source-model slice:

- Local GPUI source at Zed commit
  `69b602c797a62f09318916d24a98c930533fbdc8` still exposes typed drag
  initiation through `Interactivity::on_drag` /
  `StatefulInteractiveElement::on_drag`; retained hitboxes do not have a public
  typed drag-start API.
- The row shell therefore remains the platform drag-start trigger, but
  `places/drag.rs` now owns the `PlaceDragStartSource` projection from
  `PlaceSnapshot`. That projection decides the path, label, icon, source index,
  movable flag, export payload, and preview model before the GPUI shell is
  installed. The shell installation itself is centralized through
  `install_place_drag_start_shell()`, so row construction does not own preview
  creation or drag-start payload wiring.
- `[fika places-interaction-policy]` reports `drag_start_models=rows`, and
  `scripts/analyze-places-perf.sh --require-interaction-policy` rejects logs
  where the drag-start model count differs from visible row count. This keeps
  the Dolphin model/controller boundary explicit while the platform shell
  remains.

Retained content-y conversion test slice:

- `places_content_y_from_viewport_y()` now owns the future viewport-local y plus
  scroll offset conversion that feeds `PlacesInteractionGeometry::hit_test_y()`.
  The current retained event layer lives in scroll content and passes zero
  scroll, but the conversion is now explicit for a future viewport-level layer.
- Unit coverage proves non-zero scroll maps viewport y into the expected row or
  section and that row/section/content bounds use half-open ranges. This guards
  later event-layer relocation from off-by-one row/section target regressions.

Retained hitbox accounting slice:

- `retained_probe_hitboxes` continues to report the inserted retained layer
  hitboxes for opt-in retained policies.
- `retained_hitboxes` now reports rows+sections only once those hitboxes carry
  retained target delivery (`retained-targeting` and `retained-dnd`). Probe and
  pointer-only policies still report `retained_hitboxes=0`.
- The full retained-event gate is unchanged: it still requires
  `gpui_event_shells=0` and `drag_shells=rows`, so mixed retained-targeting and
  retained-dnd states remain rejected.

Retained interaction policy accounting slice:

- Renderer policy logs now report `retained_interaction=rows+sections` for
  `retained-targeting` and `retained-dnd`, where the retained layer actually
  owns row/section activation, context-menu targeting, DnD target lookup, and
  drop dispatch.
- Probe and pointer-only policies still report `retained_interaction=0` because
  they do not own target delivery.
- The custom row visual analyzer gates now validate `retained_interaction`
  against the selected event policy instead of assuming every custom chrome/full
  visual run has GPUI event ownership. The full retained-event gate is still not
  loosened: `retained-dnd` remains rejected until the remaining typed GPUI DnD
  shell can be removed.

Retained targeting autosmoke slice:

- `FIKA_AUTOSMOKE_PLACES=targeting` now emits non-mutating retained targeting
  samples for activation-row, row context-menu, and section context-menu target
  classification.
- The smoke consumes the same `PlacesInteractionGeometry` as the retained event
  layer and does not activate a place or open menus. It proves the target
  classification layer that retained event handlers rely on before any default
  policy promotion.
- `scripts/analyze-places-perf.sh --require-retained-targeting-autosmoke`
  rejects missing markers, failed samples, or summaries that do not include both
  rows and sections.

Default retained-DnD promotion slice:

- Places event delivery now defaults to `retained-dnd`, the strongest currently
  verified mixed policy. This removes default per-row/section GPUI activation,
  context-menu, and DnD target shells while keeping the GPUI text/icon renderer,
  one sidebar-level typed DnD payload shell, and row drag-start shells.
- `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` remains available as an explicit
  fallback for the old row/section event-shell path.
- The full retained-event analyzer gate is intentionally unchanged and still
  rejects the default mixed policy because `gpui_event_shells=1`.

Retained sidebar leave shell removal slice:

- Default retained-DnD now relies on the retained pointer layer for active-drag
  leave clearing and no longer installs the three root sidebar GPUI
  `on_drag_move` leave-clear shells for item, external-path, and place drags.
- `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` and `retained-probe` still install
  those GPUI leave shells because they do not own retained pointer movement.
- `[fika places-interaction-policy]` reports `gpui_sidebar_leave_shells=0` for
  retained-pointer, retained-targeting, and retained-DnD policies, and `3` for
  GPUI/probe fallback policies. The analyzer rejects retained-DnD logs that
  reintroduce those shells, while the full retained-event gate remains strict
  because the sidebar typed DnD payload shell is still present.

Remaining shell accounting split slice:

- `[fika places-interaction-policy]` now splits the overloaded
  `gpui_event_shells` count into `gpui_row_section_event_shells` and
  `gpui_typed_dnd_payload_shells`.
- Default retained-DnD should report `gpui_row_section_event_shells=0` and
  `gpui_typed_dnd_payload_shells=1`: row/section target delivery is retained,
  but GPUI still owns the typed drag payload entry point at sidebar level.
- GPUI/probe/pointer/targeting fallback states still report
  `gpui_row_section_event_shells=rows+sections` and
  `gpui_typed_dnd_payload_shells=0`.
- The full retained-event gate remains strict and now verifies both split
  counters are zero. The default retained-DnD mixed state should therefore fail
  specifically on the typed payload shell rather than an ambiguous event-shell
  total.

Typed payload bridge audit:

- The default retained-DnD path has removed row/section GPUI event callbacks
  from target delivery. `gpui_row_section_event_shells=0` means activation,
  row/section context-menu targeting, item/external-path drag target lookup,
  place reorder target lookup, drop target state, and drop dispatch are owned by
  the retained Places event layer and `places/interaction.rs`.
- The remaining `gpui_typed_dnd_payload_shells=1` is a single sidebar-level
  typed payload bridge installed by
  `src/ui/places/event_layer.rs::install_places_event_dnd_handlers()`. It
  exists because the public GPUI drag/drop API currently exposes typed
  `ItemDrag`, `ExternalPaths`, and `PlaceDrag` move/drop payloads through
  interactive elements (`Div::on_drag_move` / `Div::on_drop`), not through
  retained painter hitboxes.
- This bridge must not grow back into row/section ownership. It may only decode
  the typed payload, read the current mouse position, and call the retained
  geometry/controller path that already owns target decisions and drop
  execution.
- It can be removed only after GPUI exposes, or Fika carries an audited patch
  for, typed retained-hitbox drag move/drop delivery with the same payload
  types. The replacement must preserve same-window item drops, external path
  drops, place reorder/drop, place-to-pane drag behavior, cursor updates,
  leave clearing, and current-position drop menu placement.
- The removal gate is:

```text
scripts/analyze-places-perf.sh --expect-retained-event-policy ...
FIKA_AUTOSMOKE_PLACES=dnd ...
future isolated destructive drop/reorder smoke with a temporary Places config
```

- Re-audit after GPUI dependency updates by searching the local GPUI checkout
  for retained hitbox drag/drop payload APIs before changing the bridge:

```sh
rg -n "on_drag_move|on_drop|insert_hitbox|DragMoveEvent|DropEvent|ExternalPaths|PlaceDrag|ItemDrag" ~/.cargo/git/checkouts/zed-* src
```

## TODO

- [x] Add a `PlacesEventDeliveryPolicy` with an explicit `GpuiShells` fallback,
  a retained-DnD mixed default, and an explicit `RetainedProbe` opt-in. Keep
  logs explicit in mixed states and do not let probe logs satisfy the
  retained-event policy gate.
- [x] Add a retained sidebar event probe layer that can insert row/section
  hitboxes and report counts without changing behavior.
- [~] Move hover/cursor/leave clearing to the retained layer. Current status:
  `retained-pointer` moves pointer cursor ownership and active-drag leave
  clearing behind an opt-in retained layer, while GPUI row/section shells still
  own typed DnD move/drop delivery.
- [x] Add unit coverage for content-local coordinate conversion with scroll
  offsets and section/row boundaries.
- [~] Move activation/context-menu targeting to the retained layer. Current
  status: `retained-targeting` owns row activation and row/section context menu
  targeting, but the policy remains opt-in while typed DnD move/drop and
  drag-start still need GPUI shells. A non-mutating targeting autosmoke now
  covers activation-row, row context-menu, and section context-menu target
  classification.
- [~] Add isolated DnD smoke for retained item/external/place drops. Current
  status: `FIKA_AUTOSMOKE_PLACES=dnd` proves retained path-list and place drag
  target decisions for row body, row edge, and section targets without mutating
  user Places. It intentionally does not execute destructive drops, so full
  isolated drop/reorder smoke remains open.
- [~] Move drag-move/drop delivery to the retained layer. Current status:
  `retained-dnd` owns row/section target lookup and drop dispatch behind one
  sidebar-level GPUI typed drag shell. The remaining GPUI boundary is payload
  delivery and drag-start, not per-row/section DnD target logic.
- [x] Move Places drag-start source modeling out of the row shell. Current
  status: `PlaceDragStartSource` and `install_place_drag_start_shell()` live in
  `places/drag.rs`, and analyzer logs require `drag_start_models=rows`.
- [x] Distinguish probe hitboxes from retained target-delivery hitboxes in
  policy logs. Current status: retained-targeting and retained-dnd report
  `retained_hitboxes=rows+sections`, while probe/pointer-only policies do not.
- [x] Make renderer `retained_interaction` event-policy aware. Current status:
  retained-targeting and retained-dnd report rows+sections, probe/pointer keep
  zero, and full retained-event policy still fails while `gpui_event_shells=1`.
- [x] Add non-mutating retained targeting autosmoke and analyzer gate. Current
  status: `FIKA_AUTOSMOKE_PLACES=targeting` proves activation-row,
  context-row, and context-section target classification without changing app
  state or opening menus.
- [x] Promote Places event delivery default to retained-DnD mixed policy.
  Current status: default logs show `event_policy=retained-dnd`,
  `retained_hitboxes=rows+sections`, `gpui_event_shells=1`, and
  `drag_start_models=rows`; explicit `gpui` remains the fallback.
- [x] Remove redundant root sidebar GPUI leave-clear shells from retained
  pointer policies. Current status: retained-pointer, retained-targeting, and
  retained-DnD report `gpui_sidebar_leave_shells=0`; GPUI/probe policies report
  `3`; analyzer fixtures reject retained-DnD logs that reintroduce them.
- [x] Split remaining GPUI event-shell accounting by boundary type. Current
  status: retained-DnD reports `gpui_row_section_event_shells=0` and
  `gpui_typed_dnd_payload_shells=1`, while fallback states report row/section
  shells explicitly; analyzer fixtures reject retained-DnD logs that reintroduce
  row/section GPUI event shells.
- [x] Remove GPUI row/section event callbacks from default target delivery.
  Current status: retained-DnD reports `gpui_row_section_event_shells=0`; the
  remaining full retained-event blocker is the single sidebar typed payload
  bridge, not row/section activation, menu, or DnD target ownership.
- [ ] Remove the sidebar-level GPUI typed DnD payload bridge after a retained
  hitbox typed drag-move/drop API exists and the full retained-event analyzer
  plus isolated DnD smoke pass.
- [ ] Keep GPUI row drag-start shells until Track 4 solves typed drag start.
