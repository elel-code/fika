# Item View Renderer Decisions

This file records renderer choices for the Dolphin-style item-view migration.
It is intentionally separate from the implementation TODO: a renderer can stay
on GPUI built-ins while the model, layouter, controller, and painter inputs
remain Dolphin-aligned.

Current replacement status and the full transition roadmap are tracked in
`docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`.

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
| Compact/Icons thumbnail and theme-icon images | custom image painter | image paint snapshots, pane-local thumbnail image cache, retained theme/thumbnail image map, background file-icon resolve queue | Keep custom paint while image decode/cache stays on GPUI `RetainAllImageCache`; theme icons reuse retained same-`iconName` images through pending loads. Zoom follows Dolphin's delayed role-size update instead of decoding each intermediate icon size. Render conversion uses cached/preliminary icon snapshots only. | Logs must include `[fika item-image]`; no synchronous icon-theme lookup in conversion, no thumbnail sync decode, no theme-icon file decode in prepaint, and no regression where a previously visible real MIME icon flashes back to fallback while a new image resource is pending. |
| Compact/Icons hover, cursor, click, menu, drop hit testing | retained viewport/custom hitboxes plus active item-drag window tracker | viewport retained hit testing and `drag_drop` state | Keep retained controller path. | DnD smoke must pass across internal item, pane, Places, and external drops; pane self-drags should log `active-item-move`. |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state plus temporary shell | Keep GPUI shell for initiation only. | Do not remove until GPUI exposes public custom-element drag-start or Fika carries an audited GPUI patch. |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model and overlay geometry | Keep GPUI built-in editor. | Only revisit when text input, caret hit testing, selection, and IME behavior can stay behavior-complete. |
| Details row backgrounds, icons, and text cells | custom content-level painter | Details paint slots, image cache, text shape cache, background file-icon resolve queue | Keep custom paint. Render frames use cached/preliminary icon snapshots only. | Logs must include `[fika details-visual]` and `[fika details-shape-cache]` with no steady build regression or synchronous icon-theme lookup spike. |
| Details row click, menu, navigation, drop, hover, cursor | retained viewport/custom hitboxes plus active item-drag window tracker | viewport retained hit testing and Details row snapshots | Keep retained controller path. | Runtime smoke must cover Details item drag, directory drop, pane drop, and rename overlay. |
| Details drag start | GPUI `Div::on_drag` row shell | retained Details drag fields plus temporary shell | Keep GPUI shell for initiation only. | Same public drag-start API or audited GPUI patch gate as Compact/Icons. |
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

For scroll and zoom investigations, treat `[fika item-view] ... icon_sync=...
convert=...` as a renderer decision signal too: visible theme-icon path work
must stay inside the small Dolphin-style `icon_sync` budget, while read-ahead
icon work should still be absorbed by preliminary icon snapshots and a
background resolve queue, not by synchronous theme path lookup during
conversion.

For paint-layer investigations, compare `[fika static-item-visual]` and
`[fika item-image]` prepaint counts against visible item counts, not raw
read-ahead work counts. Read-ahead belongs to scheduler projection and retained
caches; it should not add image-cache loads or text shaping to the current
paint prepass.

For MIME icon flicker investigations, compare against Dolphin's
`KStandardItemListWidget::updatePixmap()` and `pixmapForIcon()`: Dolphin keeps a
widget-local `m_pixmap` and uses `QPixmapCache` by icon name/size, so a loaded
real icon is not replaced by a marker while a same-icon resource is refreshed.
Fika's custom image painters must preserve that behavior with retained images
keyed by MIME/theme `iconName`; thumbnail retention remains keyed by the exact
thumbnail path. Fika does not mirror Dolphin's synchronous `QIcon::pixmap()` by
reading and decoding SVGs in GPUI prepaint; GPUI image loading remains the
decode path, and retained same-`iconName` images cover pending frames. A neutral
markerless placeholder is only the first-load/failure fallback, not a
regression from an already loaded real icon.

For zoom investigations, compare against
`KFileItemListView::triggerIconSizeUpdate()` and `updateIconSize()`: Dolphin
updates item geometry immediately but pauses `KFileItemModelRolesUpdater`,
restarting icon-size/visible-range role work after `LongInterval` (300ms). Fika
mirrors that with a pane-local icon role size. The layout icon rect changes on
each zoom step, but icon snapshot conversion and file-icon resolve requests keep
using the frozen role size until the 300ms debounce fires; then Fika invalidates
the visible snapshot/work caches and resolves the final size.

For directory-load MIME icon switching, compare against
`KFileItemModel::retrieveData()`, `KFileItemModelRolesUpdater::updateVisibleIcons()`,
and `KFileItemListView::initializeItemListWidget()`: Dolphin does not resolve
all model roles synchronously, but it does give created visible widgets an
`iconName` before the async `ResolveAll` pass walks the rest. Fika should keep
the same split: visible generic MIME metadata and visible theme-icon paths may
be resolved synchronously within bounded budgets; read-ahead/offscreen metadata
and icon paths remain queued. This mirrors Dolphin's `iconName` plus
`pixmapForIcon()` path without moving read-ahead icon-theme scans into render
conversion. Image decoding itself stays on the scheduler/image-cache path; the
paint layer may retain a previous same-`iconName` image but must not
synchronously decode theme icon files during prepaint.

## Next Renderer Decisions

1. Keep the remaining drag-start shells until the GPUI API boundary changes.
   Do not use GPUI per-element `on_drag_move` as the source of truth for pane
   self-drag hover; the active item-drag window tracker owns that path.
2. Use runtime logs to decide whether any currently custom-painted surface
   should stay custom-painted or fall back to a GPUI renderer over the retained
   model.
3. Do not start a Places custom-paint migration until item-view runtime DnD and
   perf gates are refreshed.
