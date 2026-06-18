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
| Compact/Icons thumbnail images | custom image painter | image paint snapshots, pane-local thumbnail image cache, retained thumbnail image map, thumbnail scheduler roles | Keep custom paint for thumbnails while image decode/cache stays on GPUI `RetainAllImageCache`; thumbnail pending/failure behavior remains model-driven and can paint fallback without changing MIME/theme icon policy. | Logs must include `[fika item-image]` plus `thumb_*` `image_sources` when thumbnails are exercised; no thumbnail sync decode in prepaint. |
| Compact/Icons MIME/theme-icon images | GPUI `img()` element over retained item shell | retained item slots, visible icon role/path cache, background file-icon resolve queue | Use GPUI image elements by default. `/etc` evidence showed the custom image layer exposed a first-load placeholder frame for all 48 visible theme icons; GPUI elements keep the retained model/controller boundary without feeding theme icons through the custom painter. Render conversion still uses cached/preliminary icon snapshots only, and zoom resolves theme icon paths for the current layout icon size immediately. | Revisit only with paired logs from `FIKA_CUSTOM_THEME_ICONS=1` and default runs. The default should show `gpui_image_element>0`; the custom override should be the only path with theme-icon `theme_placeholder` churn. |
| Compact/Icons hover, cursor, click, menu, drop hit testing | retained viewport/custom hitboxes plus active item-drag window tracker | viewport retained hit testing and `drag_drop` state | Keep retained controller path. | DnD smoke must pass across internal item, pane, Places, and external drops; pane self-drags should log `active-item-move`. |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state plus temporary shell | Keep GPUI shell for initiation only. | Do not remove until GPUI exposes public custom-element drag-start or Fika carries an audited GPUI patch. |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model and overlay geometry | Keep GPUI built-in editor. | Only revisit when text input, caret hit testing, selection, and IME behavior can stay behavior-complete. |
| Details row backgrounds, icons, and text cells | custom content-level painter | Details paint slots, image cache, text shape cache, background file-icon resolve queue | Keep custom paint. Render frames use cached/preliminary icon snapshots only. | Logs must include `[fika details-visual]` and `[fika details-shape-cache]` with no steady build regression or synchronous icon-theme lookup spike. |
| Details row click, menu, navigation, drop, hover, cursor | retained viewport/custom hitboxes plus active item-drag window tracker | viewport retained hit testing and Details row snapshots | Keep retained controller path. | Runtime smoke must cover Details item drag, directory drop, pane drop, and rename overlay. |
| Details drag start | GPUI `Div::on_drag` row shell | retained Details drag fields plus temporary shell | Keep GPUI shell for initiation only. | Same public drag-start API or audited GPUI patch gate as Compact/Icons. |
| Places rows and sidebar scrollbar | GPUI elements by default; opt-in `FIKA_CUSTOM_PLACES_ROWS=1` sidebar-level row visual layer for benchmarking | `places` model/projection and `drag_drop` state | Keep GPUI renderer as default. The opt-in painter may be expanded only if perf and behavior evidence match or beat GPUI. | Run default `--expect-current-gpui-policy` and opt-in `--expect-custom-row-visual-policy` logs, then cover scroll, reorder, context menu, item/external drops, and device/trash rows before any default switch. The opt-in policy must log one aggregated `[fika places-row-visual]` rows count matching the policy rows. |

## Historical GPUI Image Baseline

Use `a3f5b0f` as the pre-retained/custom-paint image-renderer baseline for
Compact/Icons. In that state, item thumbnails and theme icons were still GPUI
`img()` children under a root `image_cache(retain_all(...))` provider. Use the
transition commits to localize regressions:

- `d497593`: retained item paint slot and static text shape cache introduced.
- `8d1198f`: hovered item state moved into retained paint visual state.
- `36da130`: static item visuals moved into a dedicated custom element.
- `b0cac9a`: static fallback visuals moved to the content-level paint layer.

For the current startup MIME blank, zoom size jump, and `/etc` zoom smoothness
investigations, compare three paths before accepting another custom image-layer
change:

- Dolphin source: `KStandardItemListWidget::updatePixmapCache()` and
  `pixmapForIcon()` synchronously produce a current-size pixmap and cache it by
  icon name/height.
- Historical Fika GPUI path: `a3f5b0f` / early transition commits rely on GPUI
  `img()` loading and element fallback behavior.
- Custom-theme override: run with `FIKA_CUSTOM_THEME_ICONS=1` to force
  theme/MIME icons back through the custom item-image paint layer.
- Current default path: run without image renderer overrides. Thumbnails stay
  on the custom image layer, while MIME/theme icons use GPUI `img()` children
  over retained item shells. Renderer-policy logs should show
  `gpui_image_element>0` for visible theme icons.

Decision rule: if the custom image layer keeps showing visible first-load
placeholders or zoom-time decode/size churn while the GPUI `img()` baseline is
visually smoother in the same scenario, keep the retained model/projection
architecture but reconsider the renderer. A GPUI image element over retained
slots, or an audited retained pixmap strategy, is preferable to a slower custom
image painter.

### 2026-06-17 `/etc` Image Renderer A/B Smoke

Automated launch commands:

```sh
timeout 8s env FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 target/debug/fika /etc
timeout 8s env FIKA_PERF_ITEM_VIEW=1 target/debug/fika /etc
```

Observed structured evidence from `scripts/compare-item-image-renderers.sh`:

- Custom-theme override:
  `max_image_layer=48`, `max_gpui_image_element=0`, `image_frames=3`,
  `theme_loaded=96`, `theme_decoded=1`, `theme_placeholder=48`.
- Default split renderer:
  `max_image_layer=0`, `max_gpui_image_element=48`, `image_frames=0`,
  `theme_placeholder=0`.

This proves the custom theme-icon image layer shows a first-load placeholder
frame for all 48 visible `/etc` MIME/theme icons, then switches when GPUI
image-cache decode completion feeds the custom painter. It also proves the
default split renderer keeps the retained item model/controller path while
routing MIME/theme icons through GPUI `img()` children.

This evidence is sufficient to explain startup placeholder-to-icon switching
in `/etc` for the custom image layer. It is not yet sufficient to decide the
full zoom/scroll renderer policy, because the 8s smoke did not exercise zoom
or scroll. The next automated comparison must save both logs and run
`scripts/compare-item-image-renderers.sh` after scripted or manually triggered
zoom/scroll interaction in the same directory.

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
is still following this table's surface choices. The standard runtime gate also
passes `--expect-retained-item-policy`, so base item visuals and interaction
must stay retained even while drag-start shells, rename overlays, and the
current theme-icon image elements remain GPUI boundaries. It should be reviewed
before removing a GPUI shell or reverting a custom-painted surface.

For scroll and zoom investigations, treat `[fika item-view] ... icon_sync=...
convert=...` as a renderer decision signal too: visible theme-icon path work
must stay inside the small Dolphin-style `icon_sync` budget, while read-ahead
icon work should still be absorbed by preliminary icon snapshots and a
background resolve queue, not by synchronous theme path lookup during
conversion.

### 2026-06-17 `/etc` Zoom/Scroll Autosmoke

The first `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` run exposed the remaining
`/etc` scroll hitch after MIME/theme icons moved back to GPUI `img()` elements:
the second `scroll-forward` frame logged
`phase=geometry-change visible=64 icon_sync=28340us total=29451us`. This was
not image decode or custom image painting; `image_frames=0` and
`theme_placeholder=0`. The spike came from visible theme-icon path resolution
duplicating work that had already been queued for the background read-ahead
resolver.

After changing visible icon sync to skip requests already queued or pending in
`FileIconResolveQueue`, the same autosmoke command logged
`icon_sync=173us` max and `geometry-change max_total=1635us`. This preserves
the Dolphin-style visible-first exception for unqueued work, while keeping
read-ahead icon theme scans on the role/update side instead of redoing them in
the scroll frame.

Current `/etc` autosmoke summary to compare against future regressions:

```text
item_view_stage_max: raw=602us icon_sync=173us queue=336us convert=426us
phase geometry-change frames=5 max_total=1635us max_visible=64
renderer_policy_frames: max_image_layer=0 max_gpui_image_element=64
```

The item-view autosmoke marker surface is now owned by
`src/ui/file_grid/autosmoke.rs`, not `src/main.rs`. The module owns stable
scenario labels plus start/complete, zoom-action, and scroll-action marker
formatting; the app root only applies the scheduled zoom and scroll changes to
pane state. Evidence:
`/tmp/fika-item-view-autosmoke-marker-module.log` passed the same analyzer
gates used for `/etc` zoom/scroll evidence. The analyzer now has a
`--require-autosmoke` gate for these markers, and renderer evidence summaries
include the parsed `autosmoke:` scenario/action line so future `/etc`
zoom/scroll logs cannot accidentally omit the scripted interaction markers.

Remaining visible cost in that log is static text/background painting:
`static_visual max_prepaint=5794us`, `max_paint=12043us`, with shape cache
misses only when new items enter the retained visible set. Treat future work
there as static visual painter/cache work, not MIME/theme icon renderer work.

For paint-layer investigations, compare `[fika static-item-visual]` and
`[fika item-image]` prepaint counts against visible item counts, not raw
read-ahead work counts. Read-ahead belongs to scheduler projection and retained
caches; it should not add image-cache loads or text shaping to the current
paint prepass. The analyzer's `image_sources` line separates thumbnail
first-ready GPUI decode results (`thumb_decoded`), already-ready cache loads
(`thumb_loaded`), retained fallback-to-last-real-image paths
(`thumb_retained`), and visible fallback paths (`thumb_fallback`). Theme
`image_sources` counters appear only when `FIKA_CUSTOM_THEME_ICONS=1` routes
MIME/theme icons through the custom image layer for A/B evidence.

For MIME icon flicker investigations, compare against Dolphin's
`KStandardItemListWidget::updatePixmap()` and `pixmapForIcon()`: Dolphin keeps a
widget-local `m_pixmap` and uses `QPixmapCache` by icon name/size, so a loaded
real icon is not replaced by a marker while a same-icon resource is refreshed.
Fika's default path preserves that behavior by keeping MIME/theme icons on GPUI
`img()` elements fed by retained item snapshots and current-size icon paths.
If `FIKA_CUSTOM_THEME_ICONS=1` is used, the custom image painter must preserve
the same behavior with retained images keyed by MIME/theme `iconName`.
Thumbnail retention remains keyed by the exact thumbnail path. Fika does not
mirror Dolphin's synchronous `QIcon::pixmap()` by reading and decoding SVGs in
GPUI prepaint; GPUI image loading remains the decode path. A neutral markerless
placeholder is only acceptable as the custom-theme first-load/failure fallback,
not as a regression from an already loaded real icon.

For zoom investigations, compare against
`KFileItemListView::triggerIconSizeUpdate()` and `updateIconSize()`: Dolphin
updates item geometry immediately but pauses `KFileItemModelRolesUpdater`,
restarting preview/visible-range role work after `LongInterval` (300ms).
Dolphin's ordinary `iconName` pixmap path is different: `pixmapForIcon()` uses
the widget's current style-option icon size. Fika therefore resolves MIME/theme
icon paths against the current layout icon size on every zoom step and must not
schedule a delayed second icon-size commit for theme icons.

For directory-load MIME icon switching, compare against
`KFileItemModel::retrieveData()`, `KFileItemModelRolesUpdater::updateVisibleIcons()`,
and `KFileItemListView::initializeItemListWidget()`: Dolphin does not resolve
all model roles synchronously, but it does give created visible widgets an
`iconName` before the async `ResolveAll` pass walks the rest. Fika should keep
the same split: visible generic MIME metadata and visible theme-icon paths may
be resolved synchronously within bounded budgets; read-ahead/offscreen metadata
and icon paths remain queued. This mirrors Dolphin's `iconName` plus
`pixmapForIcon()` path without moving read-ahead icon-theme scans into render
conversion. Image decoding itself stays on the scheduler/image-cache path;
default theme icons decode through GPUI `img()`, while the custom-theme A/B
paint layer may retain a previous same-`iconName` image but must not
synchronously decode theme icon files during prepaint.

## Future MIME/Theme Icon Custom Renderer

The current default intentionally keeps Compact/Icons MIME/theme icons on GPUI
`img()` elements. A future full-custom renderer is allowed only as a separate
work stream, because the retained model/slot architecture is already in place
and the remaining risk is image-resource readiness, not item identity.

The target architecture should mirror Dolphin's pixmap stability rather than
the current custom-theme A/B path:

- Add an explicit retained MIME/theme icon image cache keyed by at least
  `(iconName, icon_size_px)`. Include theme identity, scale factor, or color
  scheme in the key if the selected icon path can differ across those inputs.
- Keep thumbnail retention keyed by thumbnail path. Do not mix thumbnail and
  theme-icon retention, because their invalidation and failure semantics differ.
- Keep the previously loaded same-key real image visible while a same-key
  resource refresh is pending. A markerless placeholder is acceptable only for
  true first-load or permanent failure, never as a replacement for an already
  loaded real icon.
- Do not synchronously decode SVGs or raster theme icon files during GPUI
  prepaint. Path resolution may use the existing visible-first bounded
  `icon_sync` policy; image decode must stay on the image-cache/scheduler path.
- Prevent zoom-time second commits: ordinary MIME/theme icons must request the
  current layout icon size immediately, and any custom renderer must paint that
  size in the same icon bounds used by layout.
- Consider a hybrid promotion path: keep GPUI `img()` as the default renderer
  while warming the retained theme-icon image cache, then route a visible icon
  through the custom image layer only when the retained image for its current
  `(iconName, size)` key is ready or when the fallback is a true first-load
  placeholder.

The default renderer policy may switch from GPUI `img()` to the custom
MIME/theme image layer only after paired desktop-session evidence proves all of
the following for `/etc` and a mixed user directory:

- Default and custom runs both pass
  `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` analyzer gates.
- The custom run has no steady-state `theme_placeholder` churn, no zoom-time
  `theme_decoded` burst, and no visible icon size jump.
- `icon_sync` remains within the Dolphin-style visible-first budget; read-ahead
  icon path work must not re-enter render conversion.
- Renderer-policy logs still prove retained base visuals and retained
  interaction for item surfaces. Only the image renderer surface is allowed to
  change.
- Manual review or screenshot/video evidence confirms startup, first directory
  load, zoom, scroll, and mode switching are visually no worse than the GPUI
  baseline.

## Next Renderer Decisions

1. Keep the remaining drag-start shells until the GPUI API boundary changes.
   Do not use GPUI per-element `on_drag_move` as the source of truth for pane
   self-drag hover; the active item-drag window tracker owns that path.
2. Use runtime logs to decide whether any currently custom-painted surface
   should stay custom-painted or fall back to a GPUI renderer over the retained
   model.
3. Do not start a Places custom-paint migration until item-view runtime DnD and
   perf gates are refreshed.
