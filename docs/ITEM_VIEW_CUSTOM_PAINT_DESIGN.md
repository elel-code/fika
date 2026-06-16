# GPUI Item View Custom Paint Design

> Status: active plan. This replaces the older Slint slot-reuse plan for current
> GPUI mainline work. Historical Slint notes remain in
> `docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md`.

## Objective

Fika item views should converge on Dolphin's `KItemListView` model:

- model identity belongs to `DirectoryModel` / `ItemId`
- layout identity belongs to Rust-side projection and visible slot state
- UI hitboxes are stable interaction surfaces
- static item visuals are custom painted instead of rebuilt as GPUI child trees
- thumbnails and rename editors can remain specialized child paths until their
  cache and input contracts are ready

The practical target is not merely lower latency. The target is a retained item
view where resize, scroll, selection, hover, and metadata updates patch stable
state and paint from cached data.

Custom painting is an implementation technique, not the architecture boundary.
Fika must keep the Dolphin split between model, layouter, controller/hit testing,
and painter even when GPUI built-in elements remain faster for a specific
surface. Each custom-paint expansion needs perf evidence from
`FIKA_PERF_ITEM_VIEW=1` logs and render/build timings. If the GPUI built-in path
is measurably faster or simpler for a surface, keep that surface on the built-in
path until a retained-state or behavior requirement justifies moving it.

## Dolphin Reference

Relevant Dolphin flow:

- `KItemListView::setGeometry()` updates layouter size, then calls layout.
- `KItemListView::doLayout()` reuses `KItemListWidget` instances and updates
  geometry/properties.
- `KItemListViewLayouter::updateVisibleIndexes()` computes visible indexes
  without rebuilding widgets.
- `KFileItemModelRolesUpdater::updateVisibleIcons()` prepares visible item
  roles before painting when possible.

Fika equivalent:

- `raw_file_grid_snapshot()` and `pane_layout_projection()` own model/layout
  projection.
- `VisibleItemSlotPool` owns stable visual slot identity.
- `VisibleItemSnapshotCache` owns stable per-item content.
- custom-painted item visuals consume snapshots and paint quads/text/images.

## Architecture Boundary

### Model Layer

Owned by core and snapshot code:

- `ItemId`
- path, file type, MIME, thumbnail role
- selection/drop state
- rename draft state
- layout rects and visible item range

This layer must not depend on GPUI element identity.

### Slot Layer

Owned by `src/ui/file_grid`:

- stable slot id for visible items
- mapping from `ItemId` to slot
- retained paint content
- retained visual state for selection, drop target, and hover
- optional shaped text cache
- optional fallback icon paint cache

Slot id is not model index. It is a reusable visual instance id.

### Paint Layer

Custom-painted static item visuals should draw:

- item background, hover/selection/drop tint
- fallback icon background and marker
- item name text lines
- future metadata overlays
- future thumbnail/image quads once GPUI image cache integration is explicit

Paint layer may use:

- `Window::paint_quad`
- `WindowTextSystem::shape_line`
- `ShapedLine::paint`
- retained GPUI `img()` elements for thumbnail/theme-icon slots while GPUI owns
  path loading and decode cache
- `Window::paint_image` with GPUI `RenderImage` values loaded by a pane-local
  `RetainAllImageCache`

Paint layer must not:

- perform filesystem I/O
- parse MIME
- allocate per-frame business identity
- decide selection or DnD behavior

### Interaction Layer

Temporarily keep one GPUI `Div` per visible item for:

- stable `id(("item-slot", slot_id))`
- non-renaming drag source while GPUI lacks a public custom-element drag-start
  API
- rename hover/cursor/input until rename moves to an overlay boundary

Viewport-level hit testing remains authoritative for normal click, context menu,
middle click, rubber band, and drop target routing.

Rename items keep the existing editor subtree. Before Phase 8, thumbnail and
theme-icon items used slot-stable retained `img()` elements under a pane-local
image cache; Phase 8 moves non-renaming Compact/Icons images behind the custom
paint layer.

## Migration Phases

### Phase 0: Baseline and Docs

- Document current plan and acceptance criteria.
- Keep perf logs behind `FIKA_PERF_ITEM_VIEW=1`.
- Preserve current tests for drag, rename, viewport resize, snapshot caching.

### Phase 1: Static Fallback Visual Canvas

Replace non-renaming fallback-icon static visual children with a custom-painted
visual element:

- fallback icon + text are painted together
- real theme icon path remains the cached icon child path until image paint
  ownership is audited
- thumbnail path remains `img()` child path
- rename path remains editor subtree
- per-item drag surface remains one `Div`

Acceptance:

- `cargo test` passes
- visible behavior unchanged for Compact/Icons fallback static items
- `file-grid build` steady path should not regress in user perf logs

### Phase 2: Shaped Text Cache

Move icon/compact item text shaping into a pane-local cache keyed by:

- `ItemId`
- displayed lines
- selected/text color
- width/height
- view mode
- font size and line height

Acceptance:

- resize with same visible items reuses shaped text
- mode switch cold path is measured separately from resize
- text cache invalidates on rename, zoom, font/style changes

### Phase 3: Paint Slot State

Introduce an explicit retained slot paint state:

- `ItemPaintSlot`
- `ItemPaintContent`
- `ItemPaintGeometry`
- `ItemPaintVisualState`
- `ItemPaintSlotCache`

The render function should project visible snapshots into slot paint state before
building GPUI elements.

Acceptance:

- stable visible item keeps slot id across resize/scroll overlap
- selection/drop changes patch state only for affected slots
- hover enter/leave patches visual state without changing retained content
- directory local insert/delete does not rebuild unrelated content caches

### Phase 4: Thumbnail/Image Paint Integration

Replace thumbnail `img()` subtree after image ownership is clear:

- GPUI's path/URI `ImageSource` loader remains crate-private, so direct
  `Window::paint_image` would require Fika to own file reads, image format
  detection, decode, invalidation, and render-image lifetime.
- Current boundary keeps a minimal retained image element per thumbnail/theme
  icon slot, using a pane-local `retain_all` image cache and a stable
  `("item-image", slot_id)` id.
- Direct image painting can still reuse GPUI's public `RetainAllImageCache`,
  `ImageAssetLoader`, `RenderImage`, and `Window::paint_image` APIs. Fika should
  only reimplement decode/invalidation if GPUI's cache contract proves
  insufficient.

Acceptance:

- cached thumbnails still show on first relevant frame
- thumbnail failures and invalidations remain model-driven
- no sync image decode in paint
- image element identity is tied to visual slots, not transient GPUI child order

### Phase 5: Custom Element

Replace `canvas` spike with a dedicated custom GPUI element if needed:

- explicit layout/prepaint/paint state
- optional hitbox insertion for future per-item interaction consolidation
- direct instrumentation for shape/paint/cache hit counts

Current boundary:

- static fallback visuals use `StaticItemVisualLayerElement` instead of
  `gpui::canvas`
- the layer owns prepaint/paint state and reports pane-local aggregate timing
- item interaction still remains on the outer shell while the painter boundary
  is migrated

Acceptance:

- no normal static item child tree except the interaction shell
- custom element owns all static item painting
- tests cover geometry math and cache invalidation

### Phase 6: Pane-Level Static Visual Layer

Hoist static fallback item painting from per-item elements to one content-level
layer for Compact and Icons:

- build a filtered static paint list from retained `ItemPaintSnapshot` values
- paint all non-renaming, non-thumbnail, non-theme-icon fallback items in one
  custom element
- keep each item slot as a transparent interaction and drag shell
- keep thumbnail, theme-icon, and rename paths as specialized child paths

Acceptance:

- static fallback Compact and Icons visuals no longer allocate one custom
  element per item
- selection/hover/drop visual changes are projected through retained item paint
  state into the layer
- image and rename items continue to use their existing paths
- tests prove only fallback static items enter the layer

### Phase 7: Non-Rename Base Visual and Image Layer

Move all non-renaming Compact and Icons base visuals into content-level layers:

- the custom visual layer paints every non-renaming item's background and text
- fallback icon marker painting remains in the visual layer only for items
  without thumbnail or theme-icon paths
- thumbnail and theme-icon `img()` elements live in one content-level image layer
  keyed by retained visual slot id for this phase; Phase 8 replaces that layer
  with direct custom image painting
- each non-renaming item slot remains a transparent interaction/drag shell
- rename items keep the current child subtree and editor behavior

Acceptance:

- non-renaming thumbnail/theme-icon items no longer build per-item text/background
  child trees
- image rendering is separated from base item visual painting
- fallback marker shaping is skipped for image-backed items
- tests prove visual-layer and image-layer membership stay split correctly

### Phase 8: Direct Image Paint Layer

Replace the content-level thumbnail/theme-icon `img()` layer with a custom paint
element:

- keep using GPUI's `ImageAssetLoader` and pane-local `RetainAllImageCache` for
  path loading, SVG rendering, image decode, and render-image lifetime
- draw loaded images from the custom layer with `Window::paint_image`
- keep fallback marker painting in the image layer only when a theme-icon path
  fails to load
- keep thumbnail failures model-driven; a missing thumbnail render image does not
  synthesize a file icon in paint

Acceptance:

- non-renaming thumbnail/theme-icon items no longer allocate per-image `img()`
  elements
- image loads still happen asynchronously and notify the pane on completion
- loaded image bounds match GPUI `ObjectFit::Contain`
- image cache state remains pane-local and is released with the pane/layer

### Phase 9: Painted Interaction Hitboxes

Move item interaction out of per-item `Div` shells in two steps, matching the
current GPUI public API boundary.

#### Phase 9a: Retained Hover/Cursor Hitboxes

Route non-renaming Compact/Icons hover and cursor through a content-level custom
element:

- custom element inserts one stable hitbox per visible item visual rect
- hover and cursor route through the retained slot table
- per-item shell stays only as the GPUI drag source boundary
- viewport hit testing remains the source of truth for click/menu/drop behavior
- drag preview offset continues to use GPUI's cursor offset, independent of item
  geometry

Acceptance:

- non-renaming Compact/Icons hover/cursor no longer require per-item hover
  handlers or cursor styles
- hover/selection/drop visuals are projected through retained visual state
- item drag payload and preview behavior remain unchanged
- perf logs do not show a new steady render/build regression

#### Phase 9b: Drag Source Hitboxes

Remove the remaining non-renaming per-item drag shells only after GPUI exposes a
public custom-element drag-start API or Fika carries a small audited GPUI patch:

- drag source starts from retained hitboxes
- Compact/Icons non-renaming items allocate no per-item element at all
- internal item DnD, pane DnD, Places DnD, and external drop behavior remain
  unchanged

### Phase 10: Rename Overlay Boundary

Keep rename as the only item-local child path until text input is separated from
item painting:

- the selected item's normal base visual remains painted by the layer
- thumbnail/theme-icon images for the renaming item remain painted by the image
  layer
- the editor, caret, selection highlight, warning/error helper, and click caret
  hit testing remain in the existing rename subtree
- the rename subtree is positioned as an overlay, not as the default item visual
  path

Acceptance:

- starting/stopping rename does not rebuild unrelated item visual/image layers
- rename caret and UTF-8 selection tests remain green
- Tab rename-next preserves model order and pane-local draft state

### Phase 11: Details Mode Paint Path

After Compact/Icons are fully retained, move Details rows to the same model:

- row backgrounds, text cells, and icons are painted from retained row snapshots
- column resize/sort/drop hit testing stays model-driven
- inline rename in Details uses the same overlay boundary as Compact/Icons

Acceptance:

- Details steady render no longer builds one row subtree per visible item
- selection, context menu, drag/drop, and Trash columns retain behavior
- Compact/Icons and Details share slot/image/text cache concepts where practical

## Invariants

- Click/menu/drop behavior continues to use Rust hit testing.
- Drag source payload remains path and selection-count correct.
- Rename editor remains fully interactive and UTF-8 safe.
- Thumbnail role scheduling remains visible-first and generation guarded.
- Window resize does not require a second notify when projected viewport width
  already matches measured bounds.
- Places and item drag preview stay cursor-stable across modes and item sizes.

## Non-goals

- Do not rewrite Details mode in the first static paint slice.
- Do not reimplement image decode/cache ownership while GPUI's public
  `RetainAllImageCache` and `ImageAssetLoader` remain sufficient.
- Do not remove remaining `img()` paths for rename/Details before their
  interaction and paint boundaries are migrated.
- Do not introduce a new app-wide ECS or scene graph.
- Do not move file-manager decisions into GPUI paint code.
