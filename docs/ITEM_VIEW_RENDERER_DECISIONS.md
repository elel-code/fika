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

## GPUI Dependency Baseline

As of the 2026-06 lockfile updates to Zed/GPUI commit `e4f6742a` and the
current baseline `69b602c797a62f09318916d24a98c930533fbdc8`, the resolved
dependency graph no longer contains `async-std`, `async-global-executor`, or
the old Zed `util` crate. This removes one historical dependency-weight concern
around keeping GPUI built-in surfaces.
It does not change renderer decisions by itself: image, drag-start, rename,
and Places boundaries still require same-scenario runtime evidence before a
custom renderer can replace a GPUI surface.

## Current Surface Decisions

| Surface | Current renderer | Dolphin-style owner | Decision | Evidence required before changing |
| --- | --- | --- | --- | --- |
| Compact/Icons base background and labels | custom content-level painter | visible item snapshots, paint slots, text shape cache | Keep custom paint. | Runtime logs must keep steady snapshot conversion sub-ms and static visual paint/build under budget. |
| Compact/Icons thumbnail images | custom image painter | image paint snapshots, pane-local thumbnail image cache, retained thumbnail image map, thumbnail scheduler roles | Keep custom paint for thumbnails while image decode/cache stays on GPUI `RetainAllImageCache`; thumbnail pending/failure behavior remains model-driven and can paint fallback without changing MIME/theme icon policy. | Logs must include `[fika item-image]` plus `thumb_*` `image_sources` when thumbnails are exercised; no thumbnail sync decode in prepaint. |
| Compact/Icons MIME/theme-icon images | default full custom image layer, with `FIKA_GPUI_THEME_ICONS=1` kept as the GPUI `img()` baseline | retained item slots, visible icon role/path cache, app-level `ThemeIconImageReadiness`, pane image layer, background file-icon resolve queue | Keep the full custom image layer as the default. It uses GPUI's efficient `RetainAllImageCache -> RenderImage -> paint_image` backend but removes per-item GPUI `img()` children from the normal pane renderer. `FIKA_GPUI_THEME_ICONS=1` remains the same-scenario non-custom image baseline. | Same-scenario logs must keep `gpui_image_element=0`, `theme_placeholder=0`, and visible `theme_decoded=0` for the default path, and must compare against `FIKA_GPUI_THEME_ICONS=1` when image-layer performance changes. |
| Compact/Icons hover, cursor, click, menu, drop hit testing | retained viewport/custom hitboxes plus active item-drag window tracker | viewport retained hit testing and `drag_drop` state | Keep retained controller path. Directory item drop hover is resolved from retained window-position hit testing, not per-directory GPUI drag-move shells. | DnD smoke must pass across internal item, pane, Places, and external drops; pane self-drags should log `active-item-move`. Renderer policy must keep `gpui_directory_drop_shell=0`. |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state plus temporary shell | Keep GPUI shell for initiation only. | Do not remove until GPUI exposes public custom-element drag-start or Fika carries an audited GPUI patch. |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model and overlay geometry | Keep GPUI built-in editor. | Only revisit when text input, caret hit testing, selection, and IME behavior can stay behavior-complete. |
| Details header, row backgrounds, icons, and text cells | custom content-level painter | Details paint slots, image cache, text shape cache, background file-icon resolve queue | Keep custom paint. Render frames use cached/preliminary icon snapshots only. The header background, separators, and labels are painted by the Details visual layer rather than GPUI child elements. | Logs must include `[fika details-visual]` and `[fika details-shape-cache]` with no steady build regression or synchronous icon-theme lookup spike. Renderer policy must keep `gpui_details_header=0`. |
| Details row click, menu, navigation, drop, hover, cursor | retained viewport/custom hitboxes plus active item-drag window tracker | viewport retained hit testing and Details row snapshots | Keep retained controller path. Directory row drop hover is resolved from retained window-position hit testing, not per-directory GPUI drag-move shells. | Runtime smoke must cover Details item drag, directory drop, pane drop, and rename overlay. Renderer policy must keep `gpui_directory_drop_shell=0`. |
| Details drag start | GPUI `Div::on_drag` row shell | retained Details drag fields plus temporary shell | Keep GPUI shell for initiation only. | Same public drag-start API or audited GPUI patch gate as Compact/Icons. |
| Places rows, section headings, and sidebar scrollbar | Default full custom row/section visual layer, retained-DnD mixed event delivery, one sidebar typed DnD payload shell, and GPUI row drag-start shells; `gpui`, `chrome`, and `text` fallback policies remain available | `places` model/projection, `places/interaction.rs`, retained event layer, retained Places icon image cache, text shape cache, and `drag_drop` state | Keep the Dolphin-aligned retained model/controller/painter split as default. Row text, section heading text, and Places icons are now Fika-owned custom paint; Places icons use GPUI's efficient underlying `RenderImage`/`paint_image` path through a retained `RetainAllImageCache`, matching Dolphin's pixmap-cache principle without leaving GPUI text/image child elements in Places rows or headings. Typed DnD payload delivery and drag start remain explicit GPUI/platform boundaries. | Default logs must pass `--expect-custom-row-full-policy` and `--require-interaction-policy` with `event_policy=retained-dnd`, `text_gpui=0`, `icon_gpui=0`, `section_gpui=0`, `visual_kind=full`, `retained_hitboxes=rows+sections`, `gpui_event_shells=1`, `gpui_row_section_event_shells=0`, `gpui_typed_dnd_payload_shells=1`, `gpui_sidebar_leave_shells=0`, and aggregated `[fika places-row-visual]` rows matching policy rows. GPUI/chrome fallbacks keep GPUI heading text and remain analyzer-covered baselines. |

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
  on the custom image layer, while MIME/theme icons use the hybrid renderer:
  not-yet-ready keys stay on GPUI `img()` children and ready keys paint through
  the retained custom image layer. Renderer-policy logs may show both
  `gpui_image_element>0` and `image_layer>0` depending on readiness.

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

### 2026-06-19 Pane Visible-Cohort Image Handoff

After Places moved to default full row visual, the same principle was applied to
Compact/Icons MIME/theme icons: own the retained image state in Fika, but keep
the efficient GPUI `RetainAllImageCache -> RenderImage -> paint_image` path.
The direct `FIKA_CUSTOM_THEME_ICONS=1` full-custom stress path still is not safe
as a cold default: `/tmp/fika-pane-full-custom-etc.log` showed
`theme_placeholder=52` and visible `theme_decoded=5`, which matches startup
blank-to-icon and zoom-time second-adjustment symptoms.

The accepted pane change is therefore not a forced full-custom cold paint. It is
a visible-cohort handoff: as long as any visible theme-icon key is not ready,
all visible theme icons stay on GPUI `img()` while the item image layer prewarms
retained images. When the cohort is ready, all visible theme icons hand off to
the custom image layer together. This avoids per-item GPUI/custom mixing inside
one viewport, which was the likely source of local size/paint jumps.

Evidence after the cohort handoff:

- `/tmp/fika-pane-cohort-default-downloads.log` against
  `/tmp/fika-pane-cohort-gpui-downloads.log` passed
  `--gate-hybrid-default-promotion` with `theme_placeholder=0` and visible
  `theme_decoded=0`.
- `/tmp/fika-pane-cohort-default-etc-r2.log` kept the important image stability
  counters clean (`theme_placeholder=0`, visible `theme_decoded=0`), but
  `--gate-hybrid-default-promotion` still failed because `/etc` icon-sync and
  content-change totals were higher than the paired GPUI baseline in that run.

Decision: keep the cohort handoff because it reduces visual switching without
reintroducing visible placeholders. Do not promote the `FIKA_CUSTOM_THEME_ICONS`
stress path to default yet. The next pane image work should target `/etc`
`icon_sync` variance and then rerun default-vs-GPUI promotion evidence.

### 2026-06-19 File Icon Kind Index And Wider Background Batch

The next `/etc` blocker was not image painting. The cohort handoff kept
`theme_placeholder=0` and visible `theme_decoded=0`, but paired runs still
failed when `icon_sync` spent 7-13ms handling visible icon candidates. The
structured logs showed many frames such as `candidates=64 cached=64` with only
one or two changed icons, which pointed at cache lookup overhead rather than
custom image painting.

Root cause: `FileIconCache::cached_icon_for_kind()` found a same-kind resolved
theme icon by scanning the whole exact-size cache. During resize/fullscreen or
scroll, visible sync performed that scan once per visible candidate. This was
not Dolphin-like enough: Dolphin's item widget keeps direct pixmap/icon role
state, so reusing a resolved icon name/pixmap is an indexed lookup rather than a
per-frame cache walk.

Implementation:

- `FileIconCache` now maintains `resolved_by_kind`, keyed by `FileIconKind`, for
  pathful resolved icons. Exact-size `cached` entries still own exact results
  and negative exact lookups; the kind index only accelerates reuse of real
  resolved theme paths across same MIME/icon kind and zoom size.
- The background file-icon resolve batch increased from 64 to 128 so the bounded
  visible/read-ahead work range is more likely to finish before resize or scroll
  brings additional items into view.

Evidence:

- `/tmp/fika-icon-batch128-default-etc.log` against
  `/tmp/fika-icon-batch128-gpui-etc.log` passed
  `--gate-hybrid-default-promotion`. Candidate `icon_sync` max was `103us`,
  with `theme_placeholder=0` and visible `theme_decoded=0`.
- `/tmp/fika-icon-batch128-default-downloads-r2.log` against
  `/tmp/fika-icon-batch128-gpui-downloads-r2.log` passed the same gate.

Decision: keep the kind index and wider background batch. This preserves the
visible-first Dolphin contract while moving repeated same-kind icon reuse out of
the render-frame hot path. Remaining image work should now target replacing the
remaining GPUI `img()` fallback boundary or reducing cold first-resolve cost,
not cached same-kind lookup.

## Post-P11e Evidence To Collect

### 2026-06-19 Default Full Vs GPUI Baselines

Places is now default full custom row visual:
`DEFAULT_PLACES_ROW_VISUAL_POLICY = CustomFull`. The GPUI row path is only a
baseline selected with `FIKA_PLACES_ROW_VISUAL_POLICY=gpui`.

Same-scenario Places target autosmoke evidence:

- Full default/handoff:
  `/tmp/fika-compare-places-full.log` passed
  `scripts/analyze-places-perf.sh --expect-custom-row-handoff-policy` with
  `row_gpui=0`, ready-frame `text_gpui=0`, `icon_gpui=0`, and
  `visual_kind=full`. `places_view max_snapshot=624us`,
  `places_sidebar max_build=374us`, `places_slots max_project=42us`, and
  row visual warm paint stayed under `472us`.
- GPUI baseline:
  `/tmp/fika-compare-places-gpui.log` used
  `FIKA_PLACES_ROW_VISUAL_POLICY=gpui` and showed `row_gpui=11`,
  `row_visual_layer=0`, `visual_kind=gpui`,
  `places_view max_snapshot=1253us`, `places_sidebar max_build=551us`, and
  `places_slots max_project=52us`.

Pane comparison has a narrower GPUI baseline because Compact/Icons base
visuals and retained interaction are already custom by default. The available
non-custom image baseline is `FIKA_GPUI_THEME_ICONS=1`, which moves ordinary
MIME/theme icons back to GPUI `img()` while leaving retained item visual/text
and interaction layers in place.

Same-scenario pane autosmoke evidence after adding alternate-mode static text
warmup:

- `/etc` default full:
  `/tmp/fika-compare-pane-full-etc-r3.log` kept `gpui_image_element=0`,
  `image_layer=48`, `theme_placeholder=0`, visible `theme_decoded=0`, and
  `image max_prepaint=165us max_paint=384us`. Static text remained the main
  residual cost, with `static_visual max_prepaint=2996us max_paint=9303us`.
- `/etc` GPUI image baseline:
  `/tmp/fika-compare-pane-gpui-etc-r3.log` showed `gpui_image_element=48`,
  `image_layer=0`, and similar static text cost
  (`max_prepaint=2938us max_paint=8981us`).
- Downloads default full:
  `/tmp/fika-compare-pane-full-downloads-r3.log` kept image stability clean
  (`gpui_image_element=0`, `theme_placeholder=0`, visible `theme_decoded=0`),
  but still had static text cold cost
  (`max_prepaint=16866us max_paint=17580us`).
- Downloads GPUI image baseline:
  `/tmp/fika-compare-pane-gpui-downloads-r3.log` still had similar static text
  cold cost (`max_prepaint=15175us max_paint=17754us`) while using
  `gpui_image_element=39` for theme icons.

Decision: keep Places full as default. Keep pane image full as default because
the image layer is stable and removes GPUI image elements, but do not claim pane
full has fully reached the target performance yet. The remaining pane work is
text shape/paint retention and handoff, not image decode or MIME icon renderer
policy.

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
Fika's default hybrid path preserves that behavior by keeping MIME/theme icons
on GPUI `img()` until the current retained image key is ready, then painting the
ready key through the custom image layer. If `FIKA_CUSTOM_THEME_ICONS=1` is used,
the custom image painter must preserve the same behavior with retained images
keyed by MIME/theme `iconName`.
Thumbnail retention remains keyed by the exact thumbnail path. Fika does not
mirror Dolphin's synchronous `QIcon::pixmap()` by reading and decoding SVGs in
GPUI prepaint; GPUI image loading remains the decode path. A neutral markerless
placeholder is only acceptable as the custom-theme first-load/failure fallback,
not as a regression from an already loaded real icon.

GPUI's efficient `img()` path follows the same high-level shape rather than a
special synchronous drawing API: `img()` resolves a `Resource` through an
`ImageCache`; `RetainAllImageCache` stores a shared background load task or a
loaded `Arc<RenderImage>` keyed by the resource hash and notifies on the next
frame when loading finishes; `Window::paint_image` then inserts the image into
the sprite atlas by stable `(RenderImage.id, frame_index)` and submits a sprite
primitive. Fika's custom image/text paths should mimic that architecture:
stable semantic keys, retained loaded resources, no repeated visible-path
decode/shape replacement, and a handoff only after the retained resource is
ready.

For zoom investigations, compare against
`KFileItemListView::triggerIconSizeUpdate()` and `updateIconSize()`: Dolphin
updates item geometry immediately but pauses `KFileItemModelRolesUpdater`,
restarting preview/visible-range role work after `LongInterval` (300ms).
Dolphin's ordinary `iconName` pixmap path is different: `pixmapForIcon()` uses
the widget's current style-option icon size while the item role remains the
same `iconName`. Fika therefore changes layout/icon bounds immediately, but
keeps MIME/theme icon path identity stable after the same file-icon kind has
resolved once. It must not schedule a delayed second icon-size or path commit
for theme icons.

For directory-load MIME icon switching, compare against
`KFileItemModel::retrieveData()`, `KFileItemModelRolesUpdater::updateVisibleIcons()`,
and `KFileItemListView::initializeItemListWidget()`: Dolphin does not resolve
all model roles synchronously, but it does give created visible widgets an
`iconName` before the async `ResolveAll` pass walks the rest. Fika should keep
the same split: visible generic MIME metadata and visible theme-icon paths may
be resolved synchronously within bounded budgets; read-ahead/offscreen metadata
and icon paths remain queued. Zoom is a separate case: after the same
file-icon kind has any resolved theme path, Fika reuses that stable path rather
than enqueueing another exact-size path request. This mirrors Dolphin's
`iconName` plus `pixmapForIcon()` path without moving read-ahead icon-theme
scans into render conversion or committing a second image identity during zoom.
Image decoding itself stays on the scheduler/image-cache path;
default theme icons decode through GPUI `img()`, while the custom-theme A/B
paint layer may retain a previous same-`iconName` image but must not
synchronously decode theme icon files during prepaint.

## Future MIME/Theme Icon Custom Renderer

The current default is hybrid: Compact/Icons MIME/theme icons stay on GPUI
`img()` until the retained image for the current key is ready, then hand off to
the custom image layer. A future full-custom renderer is allowed only as a
separate work stream, because the retained model/slot architecture is already
in place and the remaining risk is image-resource readiness, not item identity.
The detailed retained image-cache design is
`docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md`.
The implementation foundation now exists in `src/ui/icons/image_cache.rs`;
`FIKA_GPUI_THEME_ICONS=1` keeps the old GPUI baseline and
`FIKA_CUSTOM_THEME_ICONS=1` remains the full custom stress path.

The full-custom target architecture should mirror Dolphin's pixmap stability
rather than the custom-theme A/B path:

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
- Prevent zoom-time second commits: ordinary MIME/theme icons must update icon
  bounds with layout immediately, but their path/image identity should remain
  stable once the same file-icon kind has a resolved theme path. Any custom
  renderer must paint in the same icon bounds used by layout without forcing a
  new path/decode identity on every zoom step.
- Keep the hybrid handoff rule unless a future full-custom run beats it:
  not-yet-ready visible icons stay on GPUI `img()`, and a visible icon routes
  through the custom image layer only when the retained image for its current
  `(iconName, size)` key is ready or when the fallback is a true first-load
  placeholder.

The default renderer policy may switch from hybrid to a full custom
MIME/theme image layer only after paired desktop-session evidence proves all of
the following for `/etc` and a mixed user directory:

- Default and custom runs both pass
  `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` analyzer gates.
- `scripts/compare-item-image-renderers.sh --gate-default-promotion` passes.
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

2026-06-18 `/etc` paired evidence did not pass this gate:
`/tmp/fika-icon-custom-etc-p16k2.log` had `theme_placeholder=118` and
`theme_decoded=5`, while `/tmp/fika-icon-default-etc-p16k2.log` kept ordinary
MIME/theme icons on GPUI `img()` with no item-image placeholder/decode churn.
At that point, the default policy stayed unchanged.

The opt-in prewarm bridge is now available as `FIKA_PREWARM_THEME_ICONS=1`.
`/tmp/fika-icon-prewarm-etc-p16k2.log` shows the bridge keeps ordinary
MIME/theme icons on GPUI (`max_image_layer=0`, `max_gpui_image_element=64`) and
does not expose custom theme placeholders (`theme_placeholder=0`,
`paint_count=0`) while recording retained-image readiness separately through
`theme_prewarm_*`. This is a staging step only; default promotion still requires
a readiness handoff so visible icons leave GPUI only after their retained image
for the current key is ready.

The readiness handoff foundation now exists behind
`FIKA_HYBRID_THEME_ICONS=1`. The app owns a size/scale-aware
`ThemeIconImageReadiness` snapshot; the image layer marks keys ready only after a
real `RenderImage` is available; renderer policy, item shells, and the image
layer all consume the same readiness input. This still does not change the
default renderer. Hybrid must produce paired `/etc` and mixed-directory
zoom/scroll evidence with no placeholders, no zoom-time decode burst, and no
paint regression before this decision table can promote MIME/theme icons away
from GPUI `img()`.

The first `/etc` hybrid smoke is recorded at
`/tmp/fika-icon-hybrid-etc-readiness.log` with the default comparison at
`/tmp/fika-etc-zoom-scroll.log`. It proves the handoff path works without
theme placeholders or zoom-time decode churn (`theme_placeholder=0`,
`theme_decoded=0`, `max_paint=383us`) while preserving the unchanged default
split (`max_image_layer=0`, `max_gpui_image_element=64`). It is not sufficient
for promotion because `/etc` still shows a roughly 24ms visible-item
`icon_sync` spike when scrolling into new entries, and mixed-directory evidence
has not been captured.

The paired 2026-06-19 hybrid run closes that mixed-directory evidence gap:
`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix
fika-hybrid-icons-20260619` produced `/etc` and Downloads default-vs-hybrid
logs, and both passed `scripts/compare-item-image-renderers.sh
--gate-hybrid-handoff` and `--gate-hybrid-default-promotion`. `/etc` hybrid reported `theme_loaded=444`,
`theme_placeholder=0`, `theme_decoded=0`, `theme_prewarm_pending=52`, and
`max_paint=504us`; Downloads hybrid reported `theme_loaded=310`,
`theme_placeholder=0`, `theme_decoded=0`, `theme_prewarm_pending=44`, and
`max_paint=378us`. This supports a follow-up default-policy code slice: ordinary
MIME/theme icons can move to the hybrid renderer by default if the code change
preserves the same gate pass and keeps GPUI fallback for not-yet-ready keys.

The default-policy code slice was validated with
`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix
fika-hybrid-default-20260619`. Candidate logs used the default renderer policy
with no `FIKA_HYBRID_THEME_ICONS` override, baseline logs used
`FIKA_GPUI_THEME_ICONS=1`, and both `/etc` and Downloads passed
`--gate-hybrid-default-promotion` with `theme_placeholder=0` and visible
`theme_decoded=0`.

## 2026-06-19 Places Full Handoff A/B

The full Places row visual path now has a real opt-in breakthrough, but not a
default-promotion decision.

The current full path is:

- `FIKA_PLACES_ROW_VISUAL_POLICY=full` for text plus vector-icon painting in
  the retained row visual layer.
- `FIKA_PLACES_ROW_VISUAL_HANDOFF=1` for ready-only handoff: GPUI text/icons
  stay visible during warmup frames, `PlacesRowTextShapeCache` is prewarmed,
  and the row switches to full custom paint only after retained resources are
  ready.

Evidence was captured with:

```sh
scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix fika-places-full-handoff-runner-20260619
scripts/run-retained-renderer-evidence.sh --places-full-handoff --analyze-only --skip-build --prefix fika-places-full-handoff-runner-20260619
```

Key logs:

- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-targets.log`
  passed the full-handoff row-visual gates. Ready/warm row paint stayed at
  `379us`, while first-frame `[fika render] total` reached `27268us`.
- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-overflow.log`
  passed with 75 rows, 29 painted rows, and warm row paint at `1090us`.
- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-layout.log`
  passed with warm row paint at `724us`.

Decision: keep default Places rows on custom chrome plus GPUI text/icons for
now. The blocker is no longer cold row visual paint by itself; it is the
whole-frame startup/target total-render variance when full handoff is enabled.
Future promotion work should separate Places snapshot, pane item, root, and row
visual ownership in the first-frame total, then reduce full-specific variance
before lowering the full path's 30ms total-render guard.

Follow-up owner accounting in
`/tmp/fika-places-full-owner-20260619-places-handoff-full-targets.log` reduced
the max-total residual to `4us` and showed the dominant same-frame owner was
`chrome_inputs=7817us`, not row visual painting. The next optimization target is
therefore toolbar/chrome icon/input preparation before revisiting row visual
default promotion thresholds.

The follow-up split in
`/tmp/fika-places-chrome-split-20260619-places-handoff-full-targets.log` showed
`chrome_state=2us` and `chrome_icons=8360us` at max total. This confirms the
remaining first-frame target is named toolbar/chrome icon resolution, not
general render state projection.

The chrome icon prewarm slice then removed that owner from both default chrome
and full handoff. `FikaApp::new()` now resolves the fixed toolbar/sidebar
snapshots before the first render. Evidence from
`scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
fika-places-chrome-prewarm-20260619` passed all handoff gates and reduced
`chrome_icons` to chrome targets `12us`, full targets `6us`, chrome overflow
`10us`, full overflow `9us`, chrome layout `7us`, and full layout `7us`. The
full path therefore has a substantive first-frame breakthrough: the old
8-14ms chrome icon spike is gone. It is still opt-in because promotion now
depends on repeated total-render evidence for row visual, pane elements, and
root cost rather than the solved chrome icon owner.

## 2026-06-19 Places Section Heading Ownership

After Places full visual became the default, section heading labels were still
GPUI text children. That left a small but real ownership mismatch: row text and
icons were retained/custom, while group headings were still shaped and painted
through GPUI elements.

Implementation: the Places visual layer now projects section heading geometry
with the same snapshot used for rows, prepaints visible section labels through
`PlacesRowTextShapeCache`, and paints them in the same canvas before rows.
`group_heading` remains as a shell for section targeting/DnD boundaries, but it
omits the GPUI label child whenever the custom visual layer paints text.

Evidence:

```sh
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-section-full-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy /tmp/fika-places-section-full-targets.log
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-section-full-overflow.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy /tmp/fika-places-section-full-overflow.log
```

Decision: default Places full visual should report `section_gpui=0` alongside
`text_gpui=0` and `icon_gpui=0`. GPUI/chrome fallbacks may still report
`section_gpui=sections`; typed DnD payload and row drag-start shells remain the
explicit GPUI/platform boundaries.

The saved logs passed those gates. `/tmp/fika-places-section-full-targets.log`
reported `max_section_gpui=0`, `max_text_gpui=0`, `max_icon_gpui=0`,
`visual_kinds=full`, and warm row paint `247us`. The overflow log reported
`max_rows=75`, `max_sections=3`, `max_section_gpui=0`, visible event hitboxes
clipped to `32`, and warm row paint `785us`.

## 2026-06-19 Pane Directory Drop Shell Removal

Pane directory hover/drop targeting no longer needs per-directory GPUI
`on_drag_move` shells. The retained path already has the required model:
`update_dragged_paths_drop_target_from_window_position()` maps window
coordinates to pane/item geometry, chooses a directory item or pane target, and
updates `DropTargetState`. The active item-drag preview/window tracker keeps
same-pane drags updated even when GPUI stops dispatching per-element drag moves.

Implementation: Compact/Icons item shells and Details rows no longer install
`install_directory_drop_target_shell`; that helper and its
`directory-shell-hit` path were removed. Transparent row/item shells remain only
for typed drag start and rename overlay boundaries. Renderer-policy logs now
separate `retained_directory_drop_target` from `gpui_directory_drop_shell`, and
`--expect-retained-item-policy` rejects any nonzero GPUI directory drop shell
count.

Decision: pane directory drop hover belongs to retained viewport/window-position
hit testing. This matches the Places direction: GPUI may still initiate typed
drags, but ongoing hover/drop targeting should be owned by retained controller
state.

Evidence: `/tmp/fika-item-retained-directory-drop.log` passed
`scripts/analyze-item-view-perf.sh --require-autosmoke --require-renderer-policy
--require-interaction --expect-retained-item-policy`. Its renderer-policy
summary reported `max_retained_directory_drop_target=60` and
`max_gpui_directory_drop_shell=0`; item interaction hitboxes still matched the
visible retained layer with `max_prepaint_count=64`.

## 2026-06-19 Details Header Visual Ownership

The Details header was still a GPUI `Div` tree with text children even though
Details rows were already painted by the custom Details visual layer. That left
a static GPUI visual surface in Details mode.

Implementation: `details_visual_layer_view()` now owns a header projection in
addition to row projections. It paints the header background, bottom border,
column separators, and shaped column titles through the existing Details visual
canvas and `DetailsTextShapeCache`. `details_shell.rs` no longer builds the
GPUI `details_header()` subtree. Renderer-policy logs now report
`details_header_visual_layer` and `gpui_details_header`, and the retained item
policy rejects `gpui_details_header != 0`.

Decision: Details header rendering is part of the custom Details painter.
Runtime Details-mode smoke should be added as a later evidence improvement; this
slice is covered by unit tests, `cargo check`, full test rerun of the previously
flaky failing test, and analyzer guards.

## 2026-06-19 Details Runtime Evidence Gate

The Details painter now has its own unattended runtime path instead of relying
on the default Compact zoom/scroll smoke. `FIKA_AUTOSMOKE_ITEM_VIEW=details-zoom-scroll`
switches the active pane to Details, then runs the same zoom and scroll action
sequence. The item-view analyzer accepts the `DetailsZoomScroll` scenario,
requires the `view-details` marker, and can gate the log with
`--require-details`, `--require-modes Details`,
`--require-renderer-policy-modes Details`, and
`--expect-retained-item-policy`.

Decision: Details custom paint changes must use this gate when they touch row,
header, text shaping, or retained interaction behavior. The retained renderer
evidence runner captures it as `item-etc-details-zoom-scroll`.

## 2026-06-19 Pane Icon Path-Ready Handoff

The pane MIME/theme icon handoff still had one exact-key artifact: zoom changes
produce a new size/scale `ThemeIconImageKey`, so a visible icon whose
`Resource::Path` was already loaded could briefly return to GPUI fallback or be
counted as a new first-ready custom decode. That is not Dolphin-like; Dolphin's
pixmap path is keyed by semantic icon data while loaded resources remain
available to the widget when style size changes.

Implementation: `ThemeIconImageReadiness` now tracks both ready semantic keys
and ready resource paths. The visible-cohort handoff accepts a theme icon when
either its exact key or its resource path is ready. `RetainedThemeIconImageCache`
also indexes loaded images by path, so a new size key with the same path is
treated as retained reuse rather than a first-ready decode.

Decision: keep the cohort handoff, but allow same-resource custom paint across
zoom. This applies the Places full-image lesson to pane images without forcing
unknown paths into custom placeholders.

Evidence: `/tmp/fika-path-ready-hybrid-downloads.log` passed
`scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion`
against `/tmp/fika-path-ready-gpui-downloads.log` with `theme_placeholder=0`
and visible `theme_decoded=0`. `/tmp/fika-path-ready-hybrid-etc-r2.log` passed
the handoff portion and removed visible decode churn (`theme_decoded=0`), while
full default promotion still failed on `/etc` icon-sync/content-change variance
outside the image handoff path.

## 2026-06-19 Pane Full Icon Key-Size Cache

The path-ready approach above is now superseded. A closer Dolphin comparison
shows that cache identity must follow `KStandardItemListWidget::pixmapForIcon()`:
the model owns a stable `iconName`, and the painter looks up a pixmap by
`iconName + iconHeight + devicePixelRatio + mode`. The resolved path is only the
icon-theme resource source; it is not the upper-level readiness or cache key.

Implementation:

- Pane MIME/theme icons now default to the full custom image layer.
  `FIKA_GPUI_THEME_ICONS=1` remains the GPUI baseline, and
  `FIKA_HYBRID_THEME_ICONS=1` is an explicit transitional path.
- `ThemeIconImageReadiness` only tracks `ThemeIconImageKey(iconName, size,
  scale, theme, color-scheme, mode)` and no longer tracks ready resource paths.
- `RetainedThemeIconImageCache` no longer reuses an old image for a new size key
  via `images_by_path`. Different sizes for the same path must have their own
  semantic key; low-level `Resource::Path` reuse remains the responsibility of
  GPUI `RetainAllImageCache` or the synchronous SVG loader.
- `FileIconCache` now keeps exact-size resolved kind entries and adds a
  `MIME + size` index so files with the same MIME but different extensions reuse
  resolved icons at the same size. It no longer carries a 48px resolved path
  into a 64px key.
- For SVG theme icons, the full image layer synchronously asks GPUI's
  `svg_renderer` for a `RenderImage` on cold keys, then still paints through
  `Window::paint_image` and the sprite atlas. This matches Dolphin's
  `QIcon::pixmap()` first-frame behavior without returning to GPUI `img()`
  elements.

Evidence:

- `/tmp/fika-full-syncsvg-custom-etc.log` versus
  `/tmp/fika-full-syncsvg-gpui-etc.log`: full path reports
  `max_image_layer=64`, `max_gpui_image_element=0`, `theme_placeholder=0`, and
  `theme_retained=497`; content-change max total is `28663us` versus the GPUI
  baseline `38298us`, and `icon_sync=27661us` versus `37062us`.
- `/tmp/fika-full-syncsvg-custom-downloads.log` versus
  `/tmp/fika-full-syncsvg-gpui-downloads.log`: full path reports
  `max_image_layer=32`, `max_gpui_image_element=0`, `theme_placeholder=0`, and
  `theme_retained=543`; initial total is `11899us` versus baseline `15103us`.

Remaining issue: the Downloads cold run has `item-image max_prepaint=38250us`
from synchronously decoding 22 theme SVGs. The follow-up is not to return to
hybrid/path-ready; it is to promote the theme `RenderImage` cache to an
app/global owner and prewarm visible `ThemeIconImageKey`s after directory load,
keeping full custom first frames placeholder-free while moving cold decode out
of the paint prepass.

## 2026-06-19 Pane Theme Icon Snapshot Prewarm

The full custom MIME/theme icon path exposed a second ownership issue. Keeping
the retained `RenderImage` cache inside the image-layer element meant cold SVG
work could only happen during element prepaint, so the first custom frame was
placeholder-free but still paid decode cost in `[fika item-image]`.

Implementation: `FikaApp` now owns the pane theme `RenderImage` cache. During
`PaneSnapshot` construction, after the visible `FileGridRenderSnapshot` is
known and before `theme_icon_readiness` is handed to pane rendering, Fika
collects visible custom-theme `ThemeIconImageKey`s, deduplicates them by
`iconName + size + scale + theme + mode`, synchronously materializes SVG
`RenderImage`s through GPUI's `svg_renderer`, records them in the app cache,
and marks those semantic keys ready. The file-grid surface no longer performs
model updates or uses a separate prewarm element; it consumes the refreshed
readiness snapshot and paints retained images through `Window::paint_image`.

Decision: early theme-icon preparation belongs to the Fika model/snapshot stage,
not to an image element prepaint. This matches the Dolphin split more closely:
the model/snapshot path owns stable icon identity and visible work discovery,
while the painter consumes ready pixmap/image entries by semantic key. The
resolved path remains only the current icon-theme resource source.

Evidence:

- `/tmp/fika-early-prewarm-custom-etc.log` versus
  `/tmp/fika-early-prewarm-gpui-etc.log`: default full custom reports
  `max_image_layer=64`, `max_gpui_image_element=0`, `theme_placeholder=0`,
  `theme_decoded=0`, `theme_prewarm_decoded=0`, and `theme_retained=454`.
  `item-image max_prepaint=166us`.
- `/tmp/fika-early-prewarm-custom-downloads.log` versus
  `/tmp/fika-early-prewarm-gpui-downloads.log`: default full custom reports
  `max_image_layer=32`, `max_gpui_image_element=0`, `theme_placeholder=0`,
  `theme_decoded=0`, `theme_prewarm_decoded=0`, and `theme_retained=187`.
  `item-image max_prepaint=315us`.

Remaining issue: the cold work has moved out of the image element, but `/etc`
can still show high `icon_sync` during content-change frames. That is now a
model/icon-resolution path, not a visible image paint path, and should be
handled by continuing Dolphin-style MIME/icon model caching and visible-work
batching.

## 2026-06-19 Detached Common File-Icon Prewarm

The `/etc` zoom-scroll smoke isolated the remaining scroll hitch to two cold
visible semantic icon resolutions, not image paint: `.pwd.lock`
(`application/octet-stream`) synchronously scanned the theme path for about
28ms, and `.updated` (`text/plain`) added another roughly 2ms. This matches the
Dolphin model lesson: common MIME/icon-name results must live in a semantic
model cache keyed by icon kind/MIME and size; the concrete file path is not the
cache identity.

Implementation: startup now launches a detached background prewarm for common
file-icon semantic keys across zoom icon sizes, prioritizing the default 48px
size and adjacent zoom levels before filling the rest. The table includes
directory plus common text, binary, archive, office, image, video, audio, and
PDF MIME keys. The work writes into the same `FileIconCache` through
`finish_resolve_results`, but deliberately does not occupy
`FileIconResolveQueue` cover keys. A first experiment queued those keys through
the visible resolver queue and removed the scroll hitch, but it also made the
first `/etc` content frame treat visible directories as queued and temporarily
lose the image layer. Detached prewarm preserves first-frame custom images
while still filling the shared semantic cache before scroll/zoom work needs it.

GPUI `img()` remains the reference for the bottom half of the image path:
`RetainAllImageCache` caches `Resource` loads as background tasks and stores
`Arc<RenderImage>`; `Window::paint_image` then inserts by
`(RenderImage.id, frame_index)` into the sprite atlas. Fika's full custom path
keeps that efficient `RenderImage -> paint_image` GPU/atlas route, but moves
the upper-half identity to Dolphin-style semantic keys instead of GPUI's
resource hash.

Evidence: `/tmp/fika-common-icon-prewarm-detached-etc.log` with
`FIKA_DEBUG_ICON_SYNC=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` reports no
scroll-time `application/octet-stream` or `text/plain` sync resolves.
`icon_sync max_total` falls from the previous roughly 30ms
(`/tmp/fika-debug-icon-sync-etc.log`) to `104us`, `max_resolved=1` only for the
initial directory key, and the first content frame keeps
`max_image_layer=48`/`max_gpui_image_element=0` with `theme_placeholder=0`.
The expanded run `/tmp/fika-common-icon-prewarm-expanded-etc.log` remains in
the same class (`icon_sync max_total=241us`, no scroll-time file MIME resolves).
`/tmp/fika-common-icon-prewarm-expanded-downloads.log` shows that common archive
MIME keys such as `application/x-tar` can be made cheap, but a first-visible
`application/java-archive` key can still race detached prewarm and synchronously
resolve on the first content frame. That is a separate mixed-directory
first-visible scheduling problem, not the `/etc` scroll regression fixed here.

## 2026-06-19 Default-Size MIME Negative Cache

The mixed-directory follow-up showed that detached prewarm alone is not enough
for the first content frame. It can lose a race against visible `icon_sync`, and
MIME keys whose theme lookup fails need to be cached as semantic negative
results. Without that, a prewarmed `application/java-archive` miss does not
protect the visible `.jar` entry; it scans the theme again.

Implementation: common file-icon prewarm now resolves the default 48px semantic
MIME table synchronously during app initialization, before loading the first
pane. Remaining zoom sizes are still filled by detached background prewarm.
`FileIconCache` also stores pathless MIME results in its `MIME + size` index.
When a later file reuses that MIME entry, Fika keeps the resolved
`iconName/path` identity but recomputes the fallback marker and colors from the
current file kind, so a `.jar` fallback can still show `JAR` without another
theme scan.

Decision: this is closer to Dolphin's split than letting visible render
conversion own first MIME icon lookup. The startup prewarm is semantic and
bounded, not path-based; it prepares common model-level icon roles before the
first pane snapshot needs them. The painter remains full custom and still uses
retained `RenderImage -> Window::paint_image`.

Evidence: `/tmp/fika-common-icon-sync48-downloads.log` reports
`max_resolved=0`, no `[fika icon-sync-resolve]` lines, `icon_sync
max_total=235us`, `max_gpui_image_element=0`, and `theme_placeholder=0`.
`/tmp/fika-common-icon-sync48-etc.log` reports `max_resolved=0` and
`icon_sync max_total=33us`.

## 2026-06-19 SVG Source RenderImage Retention

Reviewing GPUI `img(Resource::Path(svg))` showed that GPUI does not decode a
new SVG image for every layout size. The asset loader renders one
`Arc<RenderImage>` for the resource and `Window::paint_image` scales it by
paint bounds while the sprite atlas is keyed by `(RenderImage.id,
frame_index)`. Fika's full custom path already used `paint_image`, but the
theme image cache only indexed by `ThemeIconImageKey`, so the same scalable SVG
source could be materialized again for a new zoom-size key.

Implementation: `RetainedThemeIconImageCache` now keeps an additional
`source path -> RenderImage` index. `ThemeIconImageKey` and readiness remain
size/scale-aware, so a new zoom size still needs its own semantic ready key,
but if the source SVG already has a retained `RenderImage`, Fika records the
new key from that source image instead of reading and rendering the SVG again.
The source reuse reports as retained, not decoded, so `[fika item-image]`
telemetry distinguishes source-level reuse from real decode/materialization.

Decision: this preserves the Dolphin-style upper model key
(`iconName + size + scale + theme + mode`) while matching GPUI's efficient
lower image ownership (`RenderImage -> paint_image -> atlas`). The resolved path
still is not a readiness key and does not make unrelated semantic keys ready;
it is only a retained image source.

Evidence: `/tmp/fika-svg-source-retain-etc.log` reports
`theme_decoded=0`, `theme_retained=982`, `theme_placeholder=0`,
`max_gpui_image_element=0`, and `item-image max_prepaint=480us`.
`/tmp/fika-svg-source-retain-downloads.log` reports `theme_decoded=0`,
`theme_retained=702`, `theme_placeholder=0`, `max_gpui_image_element=0`, and
`item-image max_prepaint=788us`.

## 2026-06-19 Pane Static Text Shape Reuse

After image/icon ownership moved out of the hot path, the remaining full custom
variance moved to `[fika static-item-visual]`. Comparing GPUI text elements
showed that GPUI shapes text during layout and only records bounds during
prepaint; Fika's custom layer shapes all visible item labels in prepaint. That
makes cold mode switches and first visible frames pay text shaping cost in the
custom painter unless the retained cache is already warm.

Implementation: static item text shapes are now keyed by actual text/style
inputs rather than item identity. `StaticItemTextShapeCacheKey` no longer
includes `item_id`. Center-aligned Icons labels ignore text rect width/height
after the visible label lines have been selected, because those bounds only
affect paint alignment/clipping and do not change `shape_line`. Fallback marker
line height is ignored when no fallback marker is painted. The static painter
also skips transparent background quads for ordinary unselected/unhovered
items. `FIKA_AUTOSMOKE_ITEM_VIEW=icons-zoom-scroll` now switches to Icons before
running zoom/scroll so this path is covered by runtime evidence.

Decision: this is the correct direction but not the final text solution. It
matches Dolphin's content/style/layout-keyed retention better than item-local
keys and removes repeated Icons zoom misses, but it does not eliminate the
first-enter cold text/glyph spike. The next step should mirror the Places text
handoff: warm target-mode label shapes/glyphs in a retained state pool before
the first full custom visual frame for that mode.

Evidence: `/tmp/fika-full-icons-keyed-etc.log` covers `modes: Icons,Compact`
with `max_gpui_image_element=0`, `theme_placeholder=0`, and
`theme_decoded=0`. After the initial Icons switch, zoom frames report
`hits=24 misses=0`, `hits=28 misses=0`, and `hits=40 misses=0`, with repeated
zoom prepaint at 93-254us. Remaining risk is documented by
`/tmp/fika-full-icons-keyed-downloads-r2.log`: first Icons switch still reports
`hits=1 misses=39`, `static-item-visual prepaint=52840us`, and the first text
paint frame reaches `17698us`.

## Next Renderer Decisions

1. Keep the remaining drag-start shells until the GPUI API boundary changes.
   Do not use GPUI per-element `on_drag_move` as the source of truth for pane
   self-drag hover; the active item-drag window tracker owns that path.
2. Use runtime logs to decide whether any currently custom-painted surface
   should stay custom-painted or fall back to a GPUI renderer over the retained
   model.
3. Keep `FIKA_GPUI_THEME_ICONS=1` as the GPUI baseline path and use
   `--gate-hybrid-default-promotion` for future MIME/theme icon renderer
   changes.
4. Continue Places full-row visual work through the `--places-full-handoff`
   A/B gate. Do not promote full rows until row-visual cost and whole-frame
   `[fika render] total=` are neutral or better than the default chrome policy.
