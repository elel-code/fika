# Retained MIME/Theme Icon Image Cache Plan

This plan covers ordinary MIME/theme icons in Compact/Icons and Details. It
does not replace thumbnail handling. Thumbnails already use thumbnail-path
identity and the custom image layer; ordinary theme icons currently stay on GPUI
`img()` elements because that path has better first-load evidence.

The purpose of this plan is to define the cache boundary required before
`FIKA_CUSTOM_THEME_ICONS=1` or a future custom icon renderer can become the
default.

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

The Fika equivalent must therefore be retained image identity plus GPUI image
cache-backed decode, not SVG/theme file decoding in prepaint and not a marker
placeholder after a real same-key image has already loaded.

## Current Fika Boundary

Current accepted renderer policy:

- MIME/theme icons default to GPUI `img()` over retained item shells.
- Thumbnail images use the custom item image layer and retained same-thumbnail
  image fallback.
- `FIKA_CUSTOM_THEME_ICONS=1` forces theme icons through the custom image layer
  only for A/B evidence.
- Render conversion resolves icon snapshots against the current layout icon
  size and does not use a delayed second icon-size commit.

The missing piece is a retained same-theme-icon image cache that gives the
custom theme-icon path the same stability that Dolphin gets from widget-local
pixmaps plus `QPixmapCache`.

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
is pending.

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
- `src/ui/file_grid/renderer_policy.rs`: keeps the default GPUI image-element
  policy until evidence changes.

Worker orchestration remains visible-first:

1. Raw snapshot determines visible and read-ahead icon requests.
2. `FileIconCache` returns cached/preliminary icon snapshots with current layout
   size.
3. Theme path resolve remains background/batched and visible-first.
4. Image decode/load remains GPUI image-cache backed.
5. Retained image cache records loaded same-key images and exposes them to the
   custom painter.

The prepaint path must not scan the icon theme, read SVG/PNG files directly, or
decode image data synchronously.

## Runtime Evidence

Before changing the default renderer, capture paired logs:

```bash
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-default-etc.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-custom-etc.log 2>&1

FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-default-downloads.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-custom-downloads.log 2>&1
```

The future custom path is acceptable only if analyzer output proves:

- no steady `theme_placeholder` churn after a key has loaded once;
- no zoom-time `theme_decoded` burst that causes visible second-size jumps;
- no frame where a loaded same-key icon falls back to a marker or blank;
- `icon_sync`, queue, and convert phases stay within the current GPUI baseline;
- image paint cost is visible through `[fika item-image]` and stays within the
  static visual budget.

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
- [ ] Keep GPUI `img()` as the default MIME/theme icon renderer until the
  paired evidence passes and `docs/ITEM_VIEW_RENDERER_DECISIONS.md` is updated.
