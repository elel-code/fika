> 本文是 [BUG_ANALYSIS_SCROLLBAR_DRAG.md](BUG_ANALYSIS_SCROLLBAR_DRAG.md) 的简体中文翻译。

# Pane 滚动条拖拽分析

状态：在 2026-06-13 删除和 pane 解耦处理之后已过时。

之前的 pane 滚动条实现已被完全移除：

- `src/ui/scrollbar.rs`
- `src/ui/scrollbar/*`
- pane-shell 滚动条 slot 连接
- `FikaApp` pane 滚动条 drag/cache 状态
- `src/ui/item_view_container.rs`
- `src/ui/item_view_container/*`
- `FikaApp` item-view 滚动条拖拽和 smooth-scroll 状态
- `src/core/scroll.rs`
- core `HorizontalScrollBarLayout` / `horizontal_scroll_bar_layout`
- 旧的 pane 滚动条和 UI smooth-scroll 测试

早期的拖拽冻结分析指出了过时的 GPUI canvas 状态、缓存的轨道几何和
应用层 smooth-scroll tick 路由问题。这些代码路径已不存在。

当前的替代方案在 Fika 内部复现了 Zed 的滚动条模型：

- `src/ui/item_view.rs`
- `src/ui/item_view/scroll_bar.rs`

每个 pane 拥有一个 `gpui::ScrollHandle`，但权威的滚动偏移和最大偏移存在于
`ViewState` 中，匹配 Dolphin 的视图/布局器所有权模型。
`src/ui/file_grid.rs` 通过 `track_scroll()` 和 `overflow_x_scroll()` 使条目
viewport 成为跟踪的滚动容器。viewport 是一个普通 flex 子元素，因此 GPUI
可以测量其可滚动内容尺寸；`src/ui/item_view/scroll_bar.rs` 作为绝对定位的
同级 overlay 挂载在同一 wrapper 中，因此它读取跟踪 viewport 的
`ScrollHandle::bounds()`，但不是可滚动内容的一部分。
`src/main.rs` 不再挂载根级滚动条 overlay，也不再携带旧的应用层拖拽状态。

`src/ui/item_view/scroll_bar.rs` 现在直接镜像 Zed 的滚动条机制用于 compact
条目视图：滑块范围来自 `ScrollHandle::max_offset()`、
`ScrollHandle::bounds()` 和 `ScrollHandle::offset()`；轨道点击计算 Zed 的
点击偏移；滑块拖拽通过 `ScrollHandle::set_offset()` 写回负的 GPUI 偏移；
事件处理在冒泡和捕获阶段之间切换，模式与 Zed 的滚动条元素相同。
app 仅在 handle 最大值不再落后于视图拥有的最大值之后才接受 handle-to-view
偏移同步，因此临时的 GPUI 零最大值不能将 pane 滚动回开头。已删除的
`state.rs` 模块和旧 canvas metrics 不再保留。

在缩放或任何布局尺寸变化之后，`ViewState` 在接下来的两次 viewport-bounds
同步中保持权威。在此 settle 窗口期间，Fika 将保留的 pane 偏移写回
`ScrollHandle`，而不是接受 handle-to-view 写入。这防止在缩放期间临时的
GPUI handle 偏移为零重置水平滚动，同时仍将控制权交还给滚动条以进行正常的
拖拽和滚轮输入。

滚轮输入现在使用与滚动条拖拽相同的直接 `ViewState`/`ScrollHandle` 偏移路径。
文件网格在条目行/块和 viewport 上安装了相同的滚轮路由，因此条目悬停不会
阻塞滚轮滚动。惯性滚动未连接到滚动条滑块释放。
