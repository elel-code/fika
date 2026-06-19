# Retained MIME/Theme Icon Image Cache Plan

This plan covers ordinary MIME/theme icons in Compact/Icons and Details. It
does not replace thumbnail handling. Thumbnails already use thumbnail-path
identity and the custom image layer; ordinary theme icons now use the full
custom image layer by default over the retained image model. The painter draws
`RenderImage` values with `Window::paint_image`; images can come from the
retained pixmap-key cache, SVG rasterization for a missing key, or GPUI's image
cache for non-SVG resources. Item rendering no longer keeps per-item GPUI
`img()` children.

The purpose of this plan is to define and guard the cache boundary required for
that full custom default. As of 2026-06-20, the GPUI image-element and hybrid
readiness-handoff runtime switches have been removed for ordinary MIME/theme
icons; `gpui_image_element=0` is the required normal pane state.

## Dolphin Reference

Dolphin's ordinary item icon path is custom-painted, but it is not a per-frame
decode path:

- `KStandardItemListWidget::updatePixmapCache()` owns widget-local pixmap
  refresh.
- `KStandardItemListWidget::pixmapForIcon()` looks up the themed pixmap through
  `QPixmapCache`.
- The cache key includes icon identity, requested size, device pixel ratio, and
  icon mode/state inputs.
- The widget keeps its current pixmap until content/role/layout changes require
  refresh.

The Fika equivalent is therefore retained pixmap identity plus self paint.
SVG/theme files may be rasterized when a missing pixmap key is first needed,
matching Dolphin's synchronous `QIcon::pixmap()` behavior; the guard is that a
real same-key image must not regress to a marker placeholder.

## Source-Level Comparison And Target Boundary

| Layer | Dolphin source fact | GPUI `img()` source fact | Fika target |
|---|---|---|---|
| Scheduling | `KFileItemModelRolesUpdater::startUpdating()` resolves visible icons first, then orders read-ahead with `indexesToResolve()`; view timers coalesce scroll/zoom work | `Img::request_layout()` calls `ImageSource::use_data()` per element and requests a loading element after 200ms | Image work is batched by a shared RoleUpdater/ImageResolver; Places and pane do not invent separate schedulers |
| Icon production | `KStandardItemListWidget::updatePixmapCache()` reads `iconPixmap` from model data, otherwise uses `iconName` through `pixmapForIcon()` | `ImageAssetLoader` reads bytes from `Resource` and decodes to `RenderImage`; SVG uses `svg_renderer.render_single_frame()` | Fika owns self-painted pixmap keys, retention, and budget |
| Cache key | `pixmapForIcon()` keys by name, height, DPR, and mode | `RetainAllImageCache` keys by `Resource` hash | Theme icons are keyed by `ThemeIconImageKey`; thumbnails remain keyed by thumbnail path |
| Painting | `drawPixmap()` only scales/draws the current pixmap; hover background is drawn by the item widget style cache | `Img::paint()` ultimately calls `window.paint_image()` | A single custom-layer image primitive and `img()` should cost about the same; differences come from element count, cache/lifecycle, and visible work |
| Places/pane | Dolphin item view and Places are both model/view/delegate loops | GPUI row/img elements tend to bind image and event lifetime to the UI tree | Places and pane must share retained image request/result/outcome modeling; future optimizations must not fork |

## Current Fika Boundary

Current accepted renderer policy:

- MIME/theme icons default to the custom image layer. Full custom painting is
  backed by retained `ThemeIconImageKey` pixmap keys and a bounded cache; it
  must keep `gpui_image_element=0` in normal pane logs.
- Thumbnail images use the custom item image layer and retained same-thumbnail
  image fallback.
- There is no current runtime renderer switch for ordinary MIME/theme icons.
  GPUI `img()` children and hybrid readiness handoff are historical paths only.
- Initial icon path resolution may use the current layout icon size, but once a
  file-icon kind has a resolved theme path, zoom reuses that stable path rather
  than creating another exact-size path request. Fika does not use a delayed
  second icon-size or path commit.

The retained cache must give the custom theme-icon path the same stability that
Dolphin gets from widget-local pixmaps plus `QPixmapCache`: exact semantic keys
remain size/scale aware, and decoded images are not reused across different
pixmap keys merely because they share a source path.

## Cache Key

The first accepted key shape should be explicit and conservative:

```rust
struct ThemeIconImageKey {
    icon_name: SharedString,
    icon_size_px: u32,
    scale_bits: u32,
    theme_name: SharedString,
    color_scheme: IconColorScheme,
    mode: IconPaintMode,
}
```

Notes:

- `icon_name` is the semantic MIME/theme icon name, not the resolved file path.
- `icon_size_px` is the actual layout icon size used by the current mode/zoom.
- `scale_bits` preserves device-pixel-ratio differences.
- `theme_name` and `color_scheme` must be included once Fika can observe them;
  until then use stable sentinel values rather than omitting the fields.
- `mode` distinguishes normal/disabled/selected if the selected icon resource
  or tint differs.
- Thumbnail images must not use this key. They remain keyed by thumbnail path
  and source metadata.

## Stored Value

The retained cache should store only render-ready image state and status:

```rust
struct RetainedThemeIconImage {
    key: ThemeIconImageKey,
    resolved_path: Option<PathBuf>,
    image: Option<gpui::Image>,
    load_generation: u64,
    status: ThemeIconImageStatus,
}
```

The exact GPUI image type may differ. The important rule is that the custom
painter can reuse the last loaded same-key real image while a refresh or decode
is pending. It must not reuse decoded images across different pixmap keys merely
because they share a source path.

Status values:

- `Loaded`: draw the retained image.
- `Pending`: draw retained same-key image if present, otherwise draw neutral
  fallback.
- `Failed`: draw retained same-key image if present, otherwise draw neutral
  fallback.
- `StalePath`: keep drawing retained same-key image while a new resolved path is
  queued.

No text marker fallback is allowed for a key that previously loaded a real
image in the current cache lifetime.

## Ownership

The cache should live under file-grid or icon UI ownership, not in `main.rs`.
Candidate module boundary:

- `src/ui/icons/image_cache.rs`: generic retained theme-icon image cache.
- `src/ui/file_grid/image_layer.rs`: consumes retained image handles in custom
  paint.
- `src/ui/file_grid/renderer_policy.rs`: owns the default self-painted policy
  and renderer-policy stats.

Worker orchestration remains visible-first:

1. Raw snapshot determines visible and read-ahead icon requests.
2. `FileIconCache` returns cached/preliminary icon snapshots with current layout
   size.
3. Theme path resolve remains background/batched and visible-first.
4. Image decode/load uses the retained pixmap-key cache first, then SVG
   rasterization or GPUI image-cache loading for a missing key.
5. Retained image cache records loaded same-key images and exposes them to the
   custom painter.

The prepaint path must not scan the icon theme. SVG rasterization is allowed
only after the role/cache layer has already resolved a concrete theme icon path
for the requested pixmap key.

## Runtime Evidence

Before changing the image renderer or cache policy, capture default
self-painted logs and verify the current Dolphin-aligned invariants:

```bash
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-full-etc.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-full-downloads.log 2>&1
```

The default full custom path remains acceptable only if analyzer output proves:

- no steady `theme_placeholder` churn after a key has loaded once;
- no zoom-time `theme_decoded` burst that causes visible second-size jumps;
- no frame where a loaded same-key icon falls back to a marker or blank;
- `icon_sync`, queue, and convert phases stay within current retained evidence;
- image paint cost is visible through `[fika item-image]` and stays within the
  static visual budget.

The staged GPUI/hybrid readiness-handoff paths are historical only. Current
runtime evidence must not use image-renderer env switches for ordinary
MIME/theme icons.

Current `/etc` evidence from 2026-06-18:

- Default log: `/tmp/fika-icon-default-etc-p16k2.log`.
- Custom log: `/tmp/fika-icon-custom-etc-p16k2.log`.
- Comparison:
  `scripts/compare-item-image-renderers.sh --gate-default-promotion
  /tmp/fika-icon-custom-etc-p16k2.log
  /tmp/fika-icon-default-etc-p16k2.log`.
- Result: gate failed. Custom rendered through the image layer, but still showed
  `theme_placeholder=118` and `theme_decoded=5`; default GPUI `img()` showed no
  theme placeholder/decode churn in `[fika item-image]`.
- Decision: do not promote the custom theme-icon renderer yet. The next
  architecture step must avoid first-load/new-size placeholders, most likely by
  warming retained images before routing visible MIME/theme icons away from GPUI
  `img()`.

Opt-in prewarm evidence from 2026-06-18:

- Prewarm log: `/tmp/fika-icon-prewarm-etc-p16k2.log`, captured with
  `FIKA_PREWARM_THEME_ICONS=1`.
- Renderer policy stayed on GPUI for ordinary theme icons:
  `max_image_layer=0`, `max_gpui_image_element=64`.
- The item-image layer did not paint theme placeholders:
  `theme_placeholder=0`, `paint_count=0`, `max_paint=9us`.
- Prewarm activity was visible separately:
  `theme_prewarm_loaded=598`, `theme_prewarm_decoded=5`,
  `theme_prewarm_pending=118`.
- Decision: keep prewarm opt-in. It proves retained images can be warmed without
  visible placeholder fallback, but it still does not make custom theme-icon
  painting default-ready; the next slice must use warmed readiness to decide
  when a visible icon can leave GPUI `img()`.

Hybrid readiness handoff foundation from 2026-06-18:

- `FikaApp` now owns an app-level `ThemeIconImageReadiness` cache keyed by the
  same size/scale-aware `ThemeIconImageKey` used by retained image storage.
- The custom image layer marks a theme key ready only after GPUI's
  `RetainAllImageCache` returns a real `RenderImage`; pending placeholders do
  not mark readiness.
- `PaneSnapshot`/`FileGridProps` carry a cheap readiness snapshot into the
  renderer path so renderer-policy stats, item shells, and the image layer all
  use the same decision input.
- Hybrid became the readiness-handoff promotion step at that time. Visible
  MIME/theme icons stayed on GPUI `img()` until the exact current `(iconName,
  icon_size_px, scale)` key or the resolved resource path was ready, then handed
  off to the custom image painter. This runtime branch was removed on
  2026-06-20.
- At that time, `FIKA_HYBRID_THEME_ICONS=0` disabled hybrid handoff, and
  `FIKA_GPUI_THEME_ICONS=1` forced the old GPUI baseline for paired evidence.

Hybrid `/etc` smoke evidence from 2026-06-18:

- Default log: `/tmp/fika-etc-zoom-scroll.log`.
- Hybrid log: `/tmp/fika-icon-hybrid-etc-readiness.log`, captured with
  `FIKA_HYBRID_THEME_ICONS=1`.
- Default remained unchanged: `max_image_layer=0`, `max_gpui_image_element=64`,
  and no `[fika item-image]` frames for ordinary theme icons.
- Hybrid staged the handoff: early frames stayed on GPUI while prewarm reported
  `theme_prewarm_pending=118`; later frames painted ready keys through the image
  layer with `theme_loaded=396`, `theme_placeholder=0`, `theme_decoded=0`, and
  `max_paint=383us`.
- `scripts/compare-item-image-renderers.sh --gate-hybrid-handoff
  /tmp/fika-icon-hybrid-etc-readiness.log /tmp/fika-etc-zoom-scroll.log` passes
  the handoff-specific gate.
- Decision: the readiness handoff works mechanically and avoids visible
  placeholder/decode churn in this `/etc` smoke, but it is still not a default
  promotion. The run still shows a visible-item `icon_sync` spike around 24ms
  when scrolling into new `/etc` entries, and the mixed user-directory evidence
  is still missing.

Path-ready handoff update from 2026-06-19:

- `ThemeIconImageReadiness` now records both ready size/scale-aware semantic
  keys and ready `Resource::Path` values.
- `RetainedThemeIconImageCache` indexes loaded images by resolved path as well
  as by `ThemeIconImageKey`. If zoom creates a new size key for the same loaded
  path, the custom painter treats it as retained reuse instead of a new
  first-ready decode.
- Evidence: `/tmp/fika-path-ready-hybrid-downloads.log` passed
  `--gate-hybrid-default-promotion` against
  `/tmp/fika-path-ready-gpui-downloads.log` with `theme_placeholder=0` and
  visible `theme_decoded=0`. `/tmp/fika-path-ready-hybrid-etc-r2.log` passed
  the handoff portion and removed visible decode churn (`theme_decoded=0`), but
  full default promotion still failed on `/etc` icon-sync/content-change
  variance outside image handoff.

Paired hybrid evidence from 2026-06-19:

- Runner: `scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-icons-20260619`.
- `/etc` logs:
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-default-etc.log` and
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-etc.log`.
- Downloads logs:
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-default-downloads.log` and
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-downloads.log`.
- Both comparisons passed `scripts/compare-item-image-renderers.sh
  --gate-hybrid-handoff` and the stricter
  `--gate-hybrid-default-promotion`.
- `/etc` hybrid showed `renderer_state=hybrid-readiness-handoff`,
  `theme_loaded=444`, `theme_placeholder=0`, `theme_decoded=0`,
  `theme_prewarm_pending=52`, and `max_paint=504us`; the default comparison
  remained `max_image_layer=0`, `max_gpui_image_element=64`.
- Downloads hybrid showed `renderer_state=hybrid-readiness-handoff`,
  `theme_loaded=310`, `theme_placeholder=0`, `theme_decoded=0`,
  `theme_prewarm_pending=44`, and `max_paint=378us`.
- Decision: the paired evidence closed the previous mixed-directory gap and
  passed the explicit hybrid default-promotion gate. It supported the next
  code slice, but the final default moved further to full custom after the
  retained semantic cache, prewarm, and budget pieces landed.

Default-handoff code-slice evidence from 2026-06-19:

- Runner: `scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-default-20260619`.
- Candidate logs used the then-current default renderer policy with no
  `FIKA_HYBRID_THEME_ICONS` override; baseline logs used
  `FIKA_GPUI_THEME_ICONS=1`.
- `/etc` logs:
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-default-etc.log` and
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-etc.log`.
- Downloads logs:
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-default-downloads.log` and
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-downloads.log`.
- Both pairs passed `--gate-hybrid-default-promotion` with
  `theme_placeholder=0` and visible `theme_decoded=0`.

Default full-custom completion evidence from 2026-06-19:

- Full custom became the default pane MIME/theme icon renderer in this slice.
  The then-current GPUI baseline and hybrid transition overrides were removed
  on 2026-06-20.
- Final core evidence:
  `scripts/run-retained-renderer-evidence.sh --core --skip-build --prefix
  fika-core-final-retained-v3`.
- Item evidence spans Compact, Icons, and Details:
  `/tmp/fika-core-final-retained-v3-item-etc-zoom-scroll.log`,
  `/tmp/fika-core-final-retained-v3-item-etc-icons-zoom-scroll.log`, and
  `/tmp/fika-core-final-retained-v3-item-etc-details-zoom-scroll.log`.
- Analyzer summary: modes `Details,Icons,Compact`, warm steady max total
  `1108us`, max file-grid build `1344us`, max image paint `373us`, warm image
  max paint `363us`, warm static visual max paint `2546us`, and warm
  custom/details visual max paint `3309us`.
- Renderer policy proved the image transition: `gpui_image_element=0`,
  `gpui_directory_drop_shell=0`, and `gpui_details_header=0`.

Dolphin model consolidation from 2026-06-19:

- `RetainedImageRequest` now represents both thumbnail and theme-icon image
  work. The pane image layer, Details visual layer, and Places full row visual
  all load through `RetainedImageLayerState::load_request_or_retained_with_outcome()`.
- `RetainedImageLoad` now carries `RetainedImageReady`. Ready is emitted only
  when a real image exists, so pending/missing loads do not contaminate
  readiness; pane, Details, and Places consume the same load-ready contract.
- Places no longer has a `PlacesIconImageCache` wrapper. The sidebar keyed
  state owns `RetainedImageLayerState` directly, matching the pane custom
  element state model.
- Thumbnail retained fallback is now a bounded LRU. Over-budget eviction also
  removes the GPUI `RetainAllImageCache` resource and drops the `RenderImage`,
  preserving Dolphin-style memory ownership.
- Dolphin read-ahead index ordering moved into `ui::retained::work_order`;
  `fika_core::thumbnail_read_ahead_indexes` was removed so thumbnail deferred
  work and file-icon resolve cannot maintain separate visible/read-ahead rules.
- `RetainedShapeCache` and `RetainedSlotStats` now cover both pane and Places
  retained projections. Text shape caches keep surface-specific keys, but the
  cache/stat machinery is shared.
- The direct thumbnail/theme load helpers are private. New image users must
  construct `RetainedImageRequest`, which keeps thumbnail/theme readiness,
  outcome logging, and retained fallback behavior on the same path.

## TODO

- [x] Add a `ThemeIconImageKey` type beside the file icon snapshot path.
- [x] Add retained same-key image storage with loaded/pending/failed/stale
  status.
- [x] Make `[fika item-image]` distinguish retained same-key theme image reuse
  from first-load placeholders.
- [x] Extend `scripts/compare-item-image-renderers.sh` or the item-view
  analyzer so paired default/custom logs fail on placeholder churn or zoom-time
  decode bursts. `--gate-default-promotion` now fails on custom theme
  placeholders, theme decode churn, or invalid renderer-policy evidence.
- [x] Extend the paired comparison gate for hybrid handoff evidence.
  `--gate-hybrid-handoff` now requires GPUI fallback, theme prewarm activity,
  ready-key image-layer paint, and no visible theme placeholder/decode churn.
- [x] Add opt-in theme-icon prewarm evidence. `FIKA_PREWARM_THEME_ICONS=1`
  creates a non-painting image layer for GPUI-rendered theme icons and reports
  `theme_prewarm_*` counts without increasing `theme_placeholder`.
- [x] Add an app-level readiness handoff foundation. At the time, renderer
  policy could receive a size/scale-aware `theme_icon_ready` input and the
  hybrid path used that input without changing the default GPUI `img()` path.
- [x] Capture paired default-vs-hybrid runtime evidence for `/etc` and a mixed
  user directory. Hybrid must keep `theme_placeholder=0`, avoid zoom-time
  `theme_decoded` bursts, and show that ready-key custom painting is not slower
  than the default GPUI image element path before any default promotion.
  2026-06-18 `/etc` evidence passed the placeholder/decode portion but not the
  full promotion bar because the `icon_sync` spike and mixed-directory run still
  needed follow-up. The runner was later retired when the runtime branch was
  removed.
- [x] Add a stricter handoff/default-promotion gate before switching renderer
  policy. The gate proves GPUI fallback, prewarm, ready-key handoff, no visible
  placeholder/decode churn, explicit performance thresholds, and renderer-policy
  distribution.
- [x] Use the hybrid readiness handoff as the intermediate promotion step, then
  move the default MIME/theme icon renderer to full custom after the retained
  semantic cache and cache budget pieces pass evidence.
- [x] Update renderer decisions after the full-custom default switch.
  `docs/ITEM_VIEW_RENDERER_DECISIONS.md` records the default self-painted
  policy and the removal of GPUI image-element/hybrid runtime switches for
  ordinary MIME/theme icons.
- [x] Unify pane/Details/Places retained image request/load semantics and
  remove the Places-specific image cache wrapper.
- [x] Make thumbnail retained fallback a bounded LRU and centralize Dolphin
  role-updater read-ahead ordering in `ui::retained::work_order`.
- [x] Share retained text shape cache/stat machinery and retained slot delta
  stats across pane and Places.
- [x] Seal direct thumbnail/theme retained image loading behind
  `RetainedImageRequest`.
- [x] Keep the full-custom MIME/theme icon renderer as the default under the
  final core evidence gate. Future changes must keep `gpui_image_element=0`,
  placeholder churn, zoom-time decode bursts, image-paint regressions, and
  renderer-policy drift at zero.
