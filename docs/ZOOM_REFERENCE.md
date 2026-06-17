# Zoom Reference

Fika pane-local zoom maps to Dolphin's view zoom level path.

## Dolphin Source

- `../dolphin/src/views/zoomlevelinfo.cpp`
  - `ZoomLevelInfo::minimumLevel()` and `maximumLevel()` define the allowed zoom range.
  - `ZoomLevelInfo::iconSizeForZoomLevel()` maps zoom levels to icon sizes.
- `../dolphin/src/views/dolphinviewactionhandler.cpp`
  - `DolphinViewActionHandler::zoomIn()` increments the current view zoom level.
  - `DolphinViewActionHandler::zoomOut()` decrements the current view zoom level.
  - `DolphinViewActionHandler::zoomReset()` restores the current view default zoom level.
- `../dolphin/src/views/dolphinview.cpp`
  - `DolphinView::setZoomLevel()` routes level changes to the item list view and emits view-local state changes.
  - `DolphinView::resetZoomLevel()` restores the default level.
- `../dolphin/src/views/dolphinitemlistview.cpp`
  - `DolphinItemListView::setZoomLevel()` clamps the level, maps it to icon or preview size, and updates the grid size.
- `../dolphin/src/kitemviews/kfileitemlistview.cpp`
  - `KFileItemListView::triggerIconSizeUpdate()` pauses
    `KFileItemModelRolesUpdater`, starts the single-shot icon-size update timer
    with `LongInterval` (300ms), and stops the visible-index-range timer so
    repeated zoom does not regenerate previews/icons for intermediate sizes.
  - `KFileItemListView::updateIconSize()` applies the final available icon size
    and device pixel ratio to `KFileItemModelRolesUpdater`, updates the visible
    index range, then unpauses role updates.
- `../dolphin/src/kitemviews/kstandarditemlistwidget.cpp`
  - `KStandardItemListWidget::updatePixmapCache()` keeps widget-local pixmap
    state and refreshes only when size/content roles require it.
  - `KStandardItemListWidget::pixmapForIcon()` uses `QPixmapCache` by
    icon-name, icon-height, DPR, and mode.

## Fika Mapping

- Dolphin current view zoom level -> `ViewState::zoom_level`.
- Dolphin icon-size mapping -> `icon_size_for_zoom_level()`.
- Dolphin item list grid update -> `compact_layout_options()` deriving icon size, item width, and item height from `ViewState`.
- Dolphin delayed icon role update -> pane-local icon role size in `FikaApp`;
  layout icon rects use the current zoom immediately, while icon snapshot
  conversion and file-icon resolve requests use the frozen role size until the
  300ms Dolphin-style debounce fires.
- Dolphin active-view action routing -> focused `PaneId` shortcut routing in `FikaApp`.

## Behavioral Rules

- Zoom is pane-local and stored in core view state.
- Split panes inherit the source pane zoom state because split clones `ViewState`.
- Ctrl+Plus, Ctrl+Minus, and Ctrl+0 route to the focused pane only.
- Zoom changes invalidate only the target pane compact column width cache and do not reload directory data.
- Zoom must not synchronously decode theme icon files in GPUI prepaint. During
  repeated zoom, Fika should keep painting retained same-`iconName` images or
  cached/preliminary snapshots until the final debounced role size is resolved.
- If a zoom optimization appears to reduce one frame but reintroduces visible
  blank/marker switching or per-frame icon file decoding, it is not Dolphin
  aligned.
