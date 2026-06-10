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

## Fika Mapping

- Dolphin current view zoom level -> `ViewState::zoom_level`.
- Dolphin icon-size mapping -> `icon_size_for_zoom_level()`.
- Dolphin item list grid update -> `compact_layout_options()` deriving icon size, item width, and item height from `ViewState`.
- Dolphin active-view action routing -> focused `PaneId` shortcut routing in `FikaApp`.

## Behavioral Rules

- Zoom is pane-local and stored in core view state.
- Split panes inherit the source pane zoom state because split clones `ViewState`.
- Ctrl+Plus, Ctrl+Minus, and Ctrl+0 route to the focused pane only.
- Zoom changes invalidate only the target pane compact column width cache and do not reload directory data.
