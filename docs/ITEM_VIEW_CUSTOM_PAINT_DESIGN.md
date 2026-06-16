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
- `Window::paint_image` only after thumbnail/icon cache ownership is moved into
  a Fika-controlled render-image cache

Paint layer must not:

- perform filesystem I/O
- parse MIME
- allocate per-frame business identity
- decide selection or DnD behavior

### Interaction Layer

Temporarily keep one GPUI `Div` per visible item for:

- stable `id(("item-slot", slot_id))`
- hover event reporting, cursor, and drag source
- `on_drag`

Viewport-level hit testing remains authoritative for normal click, context menu,
middle click, rubber band, and drop target routing.

Rename items keep the existing editor subtree. Thumbnail and theme-icon items use
slot-stable retained `img()` elements under a pane-local image cache until image
cache integration is moved behind a paint cache.

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
- A future direct paint handle must be introduced only after Fika owns the image
  cache contract explicitly.

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

Acceptance:

- no normal static item child tree except the interaction shell
- custom element owns all static item painting
- tests cover geometry math and cache invalidation

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
- Do not remove `img()` thumbnail rendering before image cache ownership is
  explicit.
- Do not introduce a new app-wide ECS or scene graph.
- Do not move file-manager decisions into GPUI paint code.
