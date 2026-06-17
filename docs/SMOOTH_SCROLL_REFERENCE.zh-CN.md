> 本文是 [SMOOTH_SCROLL_REFERENCE.md](SMOOTH_SCROLL_REFERENCE.md) 的简体中文翻译。

# 平滑滚动参考

本文档记录 Dolphin 的条目列表平滑滚动模型。Fika 先前的 `src/core/scroll.rs` 和
`src/ui/item_view_container/*` 平滑路径已随损坏的 pane-coupled 滚动条一起删除。
当前代码在 `src/ui/item_view/scroll_bar.rs` 中保留独立的 item-view 滚动条；
滚轮输入通过 Dolphin 的 `setScrollOffset()` 所有权模型直接更新 pane `ViewState`。
平滑和惯性滚动是未来重建的参考行为，不是当前的兼容代码。

## Dolphin 源码

- `../dolphin/src/kitemviews/private/kitemlistsmoothscroller.h`
  - 定义 `KItemListSmoothScroller` 作为围绕 `QScrollBar`、目标对象和动画滚动
    属性的辅助类。
  - 暴露 `scrollContentsBy()`、`scrollTo()`、`requestScrollBarUpdate()`、
    `handleWheelEvent()` 和 `scrollingStopped()`。
- `../dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp`
  - 为目标滚动属性创建 `QPropertyAnimation`。
  - `scrollContentsBy()` 在滚动条变化后计算目标偏移，通过将起始偏移推进一帧
    保持中断动画的连续性，新动画使用 `InOutQuad`，重定向动画使用 `OutQuad`。
  - 当动画已在运行时，保留 Dolphin 公式：
    `distance += currentOffset - oldEndOffset`，
    `endOffset = currentOffset - distance`，然后
    `startOffset += (endOffset - currentOffset) * 1000 / (duration * 60)`，
    向 `endOffset` 方向限制。
  - `requestScrollBarUpdate()` 在滚动条最大值变化时停止运行动画，
    因此重新布局/内容变化不会保留过时的动画目标。
  - `handleWheelEvent()` 将滚轮事件转发到滚动条，同时为该事件启用平滑滚动。
- `../dolphin/src/kitemviews/kitemlistcontainer.cpp`
  - 拥有单独的水平/垂直 `KItemListSmoothScroller` 实例。
  - 将 `scrollContentsBy(dx, dy)` 转发到相应的平滑滚动器。
  - 使用 `QScroller::scroller(viewport())` / `grabGesture()` 进行惯性手势
    滚动，并通过控制器停止。
  - 将 smoother-scroller 的 `scrollingStopped` 连接回 `KItemListView`。
- `../dolphin/src/kitemviews/kitemlistview.cpp`
  - `KItemListView::setScrollOffset()` 限制偏移并立即调用
    `doLayout(NoAnimation)`，因此平滑滚动仍在每个动画偏移处布局可见条目。

## Fika 映射

- Dolphin `KItemListContainer` 拥有的滚动条 -> 当前为
  `src/ui/item_view/scroll_bar.rs`，由 `src/ui/file_grid.rs` 作为跟踪条目
  viewport 的同级 overlay 挂载，而非由 `src/ui/pane.rs` 挂载；
  几何和拖拽数学读写 pane-local `gpui::ScrollHandle`。
- Dolphin `KItemListSmoothScroller` 在此仅作为未来平滑/惯性目标的文档记录。
  Fika 当前没有活动的平滑滚动器模块，没有动画 tick 任务，也没有 viewport
  惯性状态。
- Dolphin 滚动条最大值失效和 `updateGeometries()` -> viewport 边界由
  GPUI `track_scroll()` 拥有。`ViewState` 拥有当前最大滚动偏移，
  并在布局边界报告不同最大值时限制当前滚动位置。
- Dolphin `setScrollOffset()` 同步布局路径映射到 GPUI `ScrollHandle` 偏移变化，
  相同的偏移写入 `ViewState.scroll_x` / `ViewState.scroll_y` 用于可见条目虚拟化。
- Dolphin `QScroller` 惯性手势行为在当前代码中未连线。重建时必须与滚动条
  滑块释放保持分离。
- Zed `SplitEditorView` / `PaneGroup` 调整大小行为 -> splitter 拖拽根据父行
  边界和 pane flex allocation 解析。Fika 将该 allocation 投影到
  `viewport_width` 后再构建 compact 布局，因此虚拟化的可见列不会等待
  split 调整大小期间的后续子元素 prepaint 处理。

## 实现说明

- 先前的 GPUI 应用层 smooth-scroll bridge、pane 滚动条 drag/cache 实现和
  `item_view_container` 重写已被移除。当前代码中没有活动的
  `scroll_pane_smooth()`、缓存的滚动条轨道或 `src/core/scroll.rs` 模块。
- 普通滚轮事件首先计算 Dolphin 方向映射（compact = 水平，icons/details = 垂直），
  然后调用与滚动条拖拽相同的 pane-local 滚动偏移路径。滚轮处理器安装在 viewport
  和条目可视行上，因此悬停条目不会绕过 pane 滚动。滚动条页面点击和滑块拖拽
  通过相同的 handle 立即写入视图偏移，不进入平滑滚动或惯性释放。
  Ctrl/secondary+滚轮仍路由到 pane-local 缩放。
- 目录导航/后退/前进在 core 中将 `ViewState` 滚动重置为 `0,0`。
- 缩放/布局变化通过将视图拥有的偏移写回 `ScrollHandle` 保持当前滚动偏移，
  直到布局边界稳定。
- Viewport 宽度/高度在布局前从 GPUI 测量的 pane 边界规范化。分数宽度向下取整，
  因此水平滚动条不能变得比当前 pane 可见宽度更宽然后被 slot 裁剪。
- 在 split 拖拽期间，来自 splitter 状态的 pane allocation 用作即时布局
  viewport。测量的 viewport 仍在绘制后协调精确的 GPUI 边界，
  但它不再是调整大小期间虚拟化的第一数据源。
- 已移除的水平滚动条 widget 使用 GPUI canvas、pane-local 拖拽状态和缓存的
  轨道快照。这些文件已删除；当前滚动条是一个新的容器组件，从跟踪的 viewport
  `ScrollHandle` 派生实时几何信息。
- Ctrl/secondary+滚轮路由到 pane-local 缩放，取消活动的 rubber-band 选择，
  不更新水平滚动状态。
- 空白按下记录待定的 rubber-band 原点，但绘制和选择仅在越过 Dolphin
  拖拽距离阈值后才开始；纯空白点击清除选择而不绘制微小矩形。
- model 保持不变：滚动仅改变视图偏移，不分配超出现有虚拟化范围的额外可见条目。
- 滚动状态保持为 `f32`；GPUI 渲染将平移的内容偏移舍入到整像素。
