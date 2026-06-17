> 本文是 [SEARCH_REFERENCE.md](SEARCH_REFERENCE.md) 的简体中文翻译。

# 搜索和过滤参考

Fika 的 pane-local 过滤栏遵循 Dolphin 的内联过滤路径，而非渲染层搜索。

## Dolphin 源码

- `../dolphin/src/filterbar/filterbar.{h,cpp}`
  - 拥有内联过滤 widget。
  - `QLineEdit::textChanged` 发出 `FilterBar::filterChanged` 信号。
  - Escape 清除非空过滤内容；当输入框已为空时关闭过滤栏。
  - Enter 和导航键将焦点交回文件视图。
  - 默认模式是 Dolphin 的 glob 模式，使用不区分大小写的匹配。

- `../dolphin/src/dolphinviewcontainer.{h,cpp}`
  - 在每个视图容器中创建 `FilterBar`。
  - 将 `filterChanged`、`filterModeChanged` 和 `caseSensitiveChanged`
    连接到活动的 `DolphinView`。
  - `setFilterBarVisible(true)` 聚焦并选中过滤输入框。
  - 当锁定按钮未启用时，`DolphinView::urlChanged` 清除过滤文本。

- `../dolphin/src/views/dolphinview.{h,cpp}`
  - `DolphinView::setNameFilter()` 直接转发到条目 model。
  - 过滤状态不进入导航历史。

- `../dolphin/src/kitemviews/private/kfileitemmodelfilter.{h,cpp}`
  - 实现 `PlainText`、`Glob` 和 `Regex` 匹配。
  - Glob 模式使用通配符转换和非锚定匹配。
  - 不区分大小写的纯文本存储小写模式以实现高效匹配。

- `../dolphin/src/kitemviews/kfileitemmodel.{h,cpp}`
  - `KFileItemModel::setNameFilter()` 分发待定插入，更新 model 过滤条件，
    然后调用 `applyFilters()`。
  - `applyFilters()` 将隐藏条目移出可见条目列表，并将匹配条目从
    `m_filteredItems` 恢复。

## Fika 映射

- Core model 标识不变：`DirectoryModel` 仍存储所有条目和稳定的 `ItemId`。
- Pane-local 过滤状态镜像 Dolphin 的 view-container 过滤状态。
- 过滤栏 UI 状态以目录式 Rust 模块拆分：`src/ui/filter_bar.rs` 是模块入口，
  `src/ui/filter_bar/state.rs` 拥有 `FilterBarSnapshot`、`PaneFilterState`
  和过滤后 model 缓存键/条目结构体。
- 活动过滤条件产生一个从布局索引到 `DirectoryModel` 索引的缓存可见索引映射。
  缓存键包括 pane id、model generation、过滤模式、大小写敏感性和查询字符串。
- GPUI 渲染消耗带过滤条目数量的 `CompactLayout`，然后将每个可见布局索引
  映射回稳定的 model 条目。
- 仅当非空过滤条件生效时，rubber-band、hit-test、范围选择、键盘移动和
  全选在过滤映射上操作。
- 关闭过滤栏会清除查询并释放其缓存的索引向量。
