# Item View Renderer Decisions

This file records renderer choices for the Dolphin-style item-view migration.
It is intentionally separate from the implementation TODO: a renderer can stay
on GPUI built-ins while the model, layouter, controller, and painter inputs
remain Dolphin-aligned.

## Decision Rules

- Model ownership is not negotiable: `DirectoryModel`, `ItemId`, pane-local
  layout projections, slot pools, and retained hit testing own item-view state.
- Renderer choice is per surface. GPUI built-ins and custom paint are both
  acceptable renderers when fed from retained model/layout/controller data.
- A custom-painted surface must have runtime perf evidence and behavior
  coverage before it replaces a GPUI surface.
- When a GPUI baseline exists, the evidence must compare the custom painter
  against that baseline under the same directory, viewport, mode, and action.
- A GPUI built-in surface should stay when GPUI owns a platform contract that
  Fika cannot yet reproduce through public APIs.

## Current Surface Decisions

| Surface | Current renderer | Dolphin-style owner | Decision | Evidence required before changing |
| --- | --- | --- | --- | --- |
| Compact/Icons base background and labels | custom content-level painter | visible item snapshots, paint slots, text shape cache | Keep custom paint. | Runtime logs must keep steady snapshot conversion sub-ms and static visual paint/build under budget. |
| Compact/Icons thumbnail and theme-icon images | custom image painter backed by GPUI `RetainAllImageCache` | image paint snapshots and pane-local image cache | Keep custom paint while GPUI owns decode/cache. | Logs must include `[fika item-image]`; no sync decode or thumbnail fallback regression. |
| Compact/Icons hover, cursor, click, menu, drop hit testing | retained viewport/custom hitboxes | viewport retained hit testing and `drag_drop` state | Keep retained controller path. | DnD smoke must pass across internal item, pane, Places, and external drops. |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state plus temporary shell | Keep GPUI shell. | Do not remove until GPUI exposes public custom-element drag-start or Fika carries an audited GPUI patch. |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model and overlay geometry | Keep GPUI built-in editor. | Only revisit when text input, caret hit testing, selection, and IME behavior can stay behavior-complete. |
| Details row backgrounds, icons, and text cells | custom content-level painter | Details paint slots, image cache, text shape cache | Keep custom paint. | Logs must include `[fika details-visual]` and `[fika details-shape-cache]` with no steady build regression. |
| Details row click, menu, navigation, drop, hover, cursor | retained viewport/custom hitboxes | viewport retained hit testing and Details row snapshots | Keep retained controller path. | Runtime smoke must cover Details item drag, directory drop, pane drop, and rename overlay. |
| Details drag start | GPUI `Div::on_drag` row shell | retained Details drag fields plus temporary shell | Keep GPUI shell. | Same public drag-start API or audited GPUI patch gate as Compact/Icons. |
| Places rows and sidebar scrollbar | GPUI elements with retained drag/drop state helpers | `places` model/projection and `drag_drop` state | Keep GPUI renderer for now. | A future custom painter needs a separate perf case; current priority is item-view shell removal only after DnD evidence. |

## Post-P11e Evidence To Collect

Run `FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads` from a desktop compositor
session, exercise Compact, Icons, and Details, then save the log and run:

```sh
scripts/check-item-view-runtime-log.sh /tmp/fika-item-view.log
scripts/summarize-item-view-renderer-evidence.sh /tmp/fika-item-view.log
```

Human review still needs to confirm the DnD and rename checklist in
`docs/ITEM_VIEW_RUNTIME_SMOKE.md`.

The `[fika renderer-policy]` summary is the runtime check that the current frame
is still following this table's surface choices. It should be reviewed before
removing a GPUI shell or reverting a custom-painted surface.

## Next Renderer Decisions

1. Keep the remaining drag-start shells until the GPUI API boundary changes.
2. Use runtime logs to decide whether any currently custom-painted surface
   should stay custom-painted or fall back to a GPUI renderer over the retained
   model.
3. Do not start a Places custom-paint migration until item-view runtime DnD and
   perf gates are refreshed.
