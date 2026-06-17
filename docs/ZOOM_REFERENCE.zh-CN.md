> 本文是 [ZOOM_REFERENCE.md](ZOOM_REFERENCE.md) 的简体中文翻译。

# 缩放参考

Fika 的 pane-local zoom 映射到 Dolphin 的视图缩放级别路径。

## Dolphin 源码

- `../dolphin/src/views/zoomlevelinfo.cpp`
  - `ZoomLevelInfo::minimumLevel()` 和 `maximumLevel()` 定义允许的缩放范围。
  - `ZoomLevelInfo::iconSizeForZoomLevel()` 将缩放级别映射到图标尺寸。
- `../dolphin/src/views/dolphinviewactionhandler.cpp`
  - `DolphinViewActionHandler::zoomIn()` 递增当前视图的缩放级别。
  - `DolphinViewActionHandler::zoomOut()` 递减当前视图的缩放级别。
  - `DolphinViewActionHandler::zoomReset()` 恢复当前视图的默认缩放级别。
- `../dolphin/src/views/dolphinview.cpp`
  - `DolphinView::setZoomLevel()` 将级别变更路由到条目列表视图，并发出视图本地状态变更信号。
  - `DolphinView::resetZoomLevel()` 恢复默认级别。
- `../dolphin/src/views/dolphinitemlistview.cpp`
  - `DolphinItemListView::setZoomLevel()` 限制级别范围，将其映射到图标或预览尺寸，并更新网格大小。
- `../dolphin/src/kitemviews/kfileitemlistview.cpp`
  - `KFileItemListView::triggerIconSizeUpdate()` 暂停
    `KFileItemModelRolesUpdater`，启动带 `LongInterval`（300ms）的单次图标
    尺寸更新定时器，并停止可见索引范围定时器，以便重复缩放不会为中间尺寸
    重新生成预览/图标。
  - `KFileItemListView::updateIconSize()` 将最终的可用图标尺寸和设备像素比
    应用到 `KFileItemModelRolesUpdater`，更新可见索引范围，然后恢复 role 更新。
- `../dolphin/src/kitemviews/kstandarditemlistwidget.cpp`
  - `KStandardItemListWidget::updatePixmapCache()` 维护 widget 本地像素图
    状态，仅当尺寸/内容 role 需要时才刷新。
  - `KStandardItemListWidget::pixmapForIcon()` 按图标名称、图标高度、DPR
    和模式使用 `QPixmapCache`。

## Fika 映射

- Dolphin 当前视图缩放级别 -> `ViewState::zoom_level`。
- Dolphin 图标尺寸映射 -> `icon_size_for_zoom_level()`。
- Dolphin 条目列表网格更新 -> `compact_layout_options()` 从 `ViewState` 派生图标尺寸、条目宽度和条目高度。
- Dolphin 延迟图标 role 更新 -> 仅限后续预览/缩略图 role 工作。
  Dolphin 的普通 MIME/主题图标像素图仍然从 widget 当前的
  `styleOption().iconSize` 在 `KStandardItemListWidget::pixmapForIcon()` 中调整大小。
  因此 Fika 会立即以当前 pane 图标尺寸解析主题图标路径；
  它不维护 pane-local 的冻结主题图标尺寸。
- Dolphin active-view action 路由 -> `FikaApp` 中以 `PaneId` 聚焦的快捷键路由。

## 行为规则

- 缩放是 pane-local 的，存储在 core view state 中。
- 分屏 pane 继承源 pane 的缩放状态，因为 split 会克隆 `ViewState`。
- Ctrl+Plus、Ctrl+Minus 和 Ctrl+0 仅路由到聚焦的 pane。
- 缩放变更只使目标 pane 的 compact 列宽缓存失效，不会重新加载目录数据。
- 缩放不得在 GPUI prepaint 中同步解码主题图标文件。在连续缩放期间，
  Fika 应保持绘制保留的相同 `iconName` 图像或缓存/初步快照，
  同时解析当前布局图标尺寸。
- 主题图标必须绘制到当前的方形图标框内，匹配 Dolphin 的
  `pixmapForIcon(iconName, QSize(iconHeight, iconHeight), mode)` 行为。
  待定占位符使用相同的方形框，因此缩放不会先显示较小的临时图标，
  然后再进行第二次尺寸调整。
- 如果某项缩放优化看似减少了一帧，但却引入了可见的空白/标记切换或
  逐帧图标文件解码，则它不符合 Dolphin 对齐要求。
