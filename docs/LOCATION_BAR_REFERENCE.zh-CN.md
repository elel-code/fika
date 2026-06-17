> 本文是 [LOCATION_BAR_REFERENCE.md](LOCATION_BAR_REFERENCE.md) 的简体中文翻译。

# 地址栏参考

Fika 的 pane-local 地址栏映射到 Dolphin 的 `KUrlNavigator` 路径。

## Dolphin 源码

- `../dolphin/src/dolphinmainwindow.cpp`
  - `DolphinMainWindow::replaceLocation()` 处理将活动视图的 URL 导航器切换到可编辑模式的快捷键。
  - `DolphinMainWindow::toggleEditLocation()` 处理显式的可编辑地址切换。
  - `DolphinMainWindow::changeUrl()` 接收来自活动视图容器的 URL 变更，并通过活动视图进行路由。
  - action 设置注册 `replace_location`（Ctrl+L / Alt+D）和 `editable_location`（F6）。
- `../dolphin/src/dolphinnavigatorswidgetaction.cpp`
  - 为每个活动分屏视图容器创建一个 `DolphinUrlNavigator`。
  - 将 `KUrlNavigator::urlChanged` 连接到视图导航。
  - 将 Trash/Network 等特殊按钮绑定到导航器的当前 URL。
- `../dolphin/src/dolphinnavigatorswidgetaction.h`
  - 存储分屏视图的主、副 URL 导航器访问器。

## Fika 映射

- Dolphin `KUrlNavigator` -> Fika pane header 地址栏。
- Dolphin active view container URL 路由 -> `FikaApp::load_pane(PaneId, PathBuf)`。
- Dolphin 可编辑 URL 模式 -> pane-scoped `LocationDraft`。
- 可编辑 draft/caret/snapshot 状态 -> `src/ui/location_bar.rs` 作为模块
  入口，`src/ui/location_bar/draft.rs` 作为目录式子模块。
- 可编辑 metrics/caret hit-test 状态 -> `src/ui/location_bar/metrics.rs`
  作为目录式子模块。
- Dolphin 面包屑按钮 -> core `BreadcrumbSegment { label, path }`，
  由 `src/core/location.rs` 构建，由可重用 pane 组件渲染。
- Dolphin URL 解析/补全行为 -> `src/core/location.rs`，拥有
  `~` 展开、启动目录规范化、绝对/相对路径解析、面包屑段构建和文件系统补全字符串，
  供启动路径解析、Places Add/Edit 路径输入和 pane 地址栏共享使用。
- 可编辑地址模式使用 pane-local caret 和水平滚动状态，
  因此长路径在 pane header 内截断，不会强制 pane 宽度增长。
- 可编辑文本 metrics 包括当前可见宽度，当分屏 pane 比光标本身更窄时，
  光标绘制会安全限制。

## 行为规则

- 地址状态是 pane-local 的，由 `PaneId` 路由。
- 面包屑点击通过与其他 pane 加载相同的路径导航，确保历史保持在 pane-local 范围内。
- Ctrl+L 和 Alt+D 将聚焦 pane 切换到可编辑地址模式。
- Enter 提交输入的路径，Escape 退出可编辑模式，Tab 尝试文件系统补全。
- 路径文本解析和补全是 UI-neutral core 行为；GPUI 应用仅拥有
  active draft、caret metrics 和 pane 导航分发。
- caret 移动使光标保持可见，不重置水平滚动（除非光标越过可见边界）。
- 面包屑段文本可收缩并在 pane header 内裁剪；长路径段不能强制新的最小 pane 宽度。
