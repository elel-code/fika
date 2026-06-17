> 本文是 [STATUS_BAR_REFERENCE.md](STATUS_BAR_REFERENCE.md) 的简体中文翻译。

# 状态栏参考

Fika 的 pane-local 状态栏映射到 Dolphin 的 view-container 状态栏流程。

## Dolphin 源码

- `../dolphin/src/dolphinviewcontainer.cpp`
  - 为视图容器创建 `DolphinStatusBar`。
  - 将 `DolphinView::statusBarTextChanged` 连接到 `DolphinStatusBar::setDefaultText`。
  - 将 `DolphinView::zoomLevelChanged` 连接到 `DolphinStatusBar::setZoomLevel`。
  - 将 `DolphinStatusBar::zoomLevelChanged` 通过 `slotStatusBarZoomLevelChanged()` 连接回活动视图。
  - 将 `DolphinStatusBar::stopPressed` 连接到目录加载取消。
- `../dolphin/src/statusbar/dolphinstatusbar.cpp`
  - 拥有文本标签、缩放标签、缩放滑块、`StatusBarSpaceInfo`、进度条和停止按钮。
  - 使用延迟进度条定时器，使短暂操作不会闪现进度 UI。
  - 当状态栏滑块改变时发出 `zoomLevelChanged(int)` 信号。
- `../dolphin/src/statusbar/statusbarspaceinfo.cpp`
  - 拥有容量条和可用空间文本按钮。
  - 使用 `SpaceInfoObserver` 更新当前 URL 的可用大小、总大小和使用百分比。
- `../dolphin/src/views/dolphinview.cpp`
  - `requestStatusBarText()` 按文件夹数、文件数和文件总大小汇总选中条目。
  - `emitStatusBarText()` 为状态栏格式化选中和未选中的数量/大小文本。
- `../dolphin/src/views/zoomlevelinfo.cpp`
  - 定义缩放级别范围和图标尺寸映射，供状态栏滑块工具提示和视图缩放状态使用。

## Fika 映射

- Dolphin view-container 状态栏 -> `src/ui/status_bar.rs` 中可重用的 pane-local GPUI 状态栏，由 `src/ui/pane.rs` 渲染。
- Dolphin status snapshot/cache/progress 状态 -> `src/ui/status_bar.rs` 作为模块
  入口，`src/ui/status_bar/state.rs` 作为目录式子模块。
- Dolphin 状态文本 -> 每个 `src/ui/pane/snapshot.rs` 的 `PaneSnapshot` 携
  带自己的 `StatusBarSnapshot`，派生自该 pane 的 `DirectoryModel` 条目和
  `SelectionState`。
- Dolphin 缩放滑块 -> 状态栏可拖拽分段缩放控件，通过 `FikaApp::set_zoom_level(pane_id, ...)` 路由。
- Dolphin 空间信息 -> pane 路径空间快照由 `FikaApp` 缓存并在后台任务中刷新。
- Dolphin 进度条和停止按钮 -> pane-bound `OperationProgressHandle`，由 core `TransferProgress` 和内部复制/移动的 `AtomicBool` 取消标志支持。
- Dolphin 目录加载停止 -> pane 加载状态由 `PaneId + generation + request_serial` 跟踪，路由到 `ListingWorker::cancel_pane()`。
- Dolphin 延迟进度定时器 -> Fika 进度快照仅在经过相同的延迟进度间隔后才变为可见。
- Dolphin 小状态栏宽度受限于 `DolphinStatusBar::updateWidthToContent()` 中的父视图宽度。
  Fika 通过使 pane-local 状态栏填充 pane 外壳（`w_full + min_w_0`）来镜像此行为，
  而可见性阈值使用当前 pane allocation，因此分屏 pane 不会保留旧的更宽状态栏并对其进行裁剪。

## 行为规则

- 状态栏是每个可重用 pane 的一部分，匹配 Dolphin 的 `DolphinViewContainer -> DolphinStatusBar` 所有权模型。
- 状态文本按 `PaneId` 存储；绝不回退到聚焦 pane。
- 选择摘要使用 `ItemId` 成员关系，绝不调用 `selected_paths()` 来获取状态文本。
- 全选保持紧凑；状态摘要仅在 `model_generation` 或选择修订变更时扫描 model 条目。
- 缩放变更仅更新目标 pane 的视图状态，并使该 pane 的 compact 列指标失效。
- 空间信息在渲染路径外查询，渲染期间从缓存读取。
- 复制/移动进度从 core 文件操作回调报告，取消由 core 取消检查处理。
- 操作进度仅在操作所在的 pane 上显示。
- 目录加载 Stop 仅取消目标 pane 的当前请求键；过时的列表结果仍会通过现有目标检查失败。
- 在延迟进度阈值之前完成的短暂操作不会闪现进度条。
