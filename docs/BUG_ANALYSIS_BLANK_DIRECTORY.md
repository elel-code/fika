# Bug 分析：启动/切换目录后内容空白，分屏后恢复正常

**日期**: 2026-06-06  
**状态**: 已定位并修复  
**结论**: 真实原因不是 Slint `set_row_data` 对嵌套模型字段传播失败，而是启动阶段过早发布了一个空的 pane/virtual view。

> 2026-06 注：此 bug 的 root cause（过早发布空模型快照）在 GPUI 主线中已通过 `DirectoryModel` 的
> "keep previous listing on LoadingStarted" 策略和 `ListingRefreshed` 到达前保留旧模型的机制
> 一并修复。参见 `docs/DESIGN.md` 的 Directory Model 节和 `docs/TODO.md` 的
> "pane load 保留旧模型直到 ListingRefreshed" 条目。本文档保留为历史问题记录。

## 问题描述

fika 启动时或切换到未缓存目录后，目录区域显示为空白，不渲染任何文件条目。触发分屏后内容恢复正常。

## 真实根因

启动时 Rust 侧没有显式清空 Slint 的 pane 模型。实测在首个目录真正加载完成前，`ui.get_pane_slots()`/`ui.get_pane_views()` 已经能让 slot 0 被当作可同步目标处理。随后启动流程中的状态、选择、布局同步把一个 entries 为空的 pane surface 和 virtual view 发布到了 UI。

切换目录时也有同类问题：已有 pane row 存在，所以 uncached load 的 loading 状态会先发布空 `PaneViewData`。这一步会留下空 item model，并且 `pane.clear_entries()` 只清空了 virtual range，没有清掉旧目录的 layout signature / thumbnail size。真实目录条目返回后，如果旧 layout 或空 surface 继续被复用，就会出现“路径栏已经变了，pane 内容仍为空”的状态。

这里有两个独立触发点：

- virtual cache 侧：`cached_virtual_viewport_sync()` 原本只比较布局尺寸和缓存 range，没有比较当前目录可见 entry count，于是可能把 `entry_count=0` 的空 layout 当作可复用 cache，直接跳过真实条目的模型重建。
- Slint surface 侧：目录切换时 `PaneSlotSurface` 已经存在，`sync_pane_view_ui()` 只对 `pane_views` / `pane_surfaces` 做同 slot `set_row_data`。当内容从空 model 变成真实 `ModelRc` 时，Slint 可能保留已实例化的空 `SplitPaneView` 分支，导致必须像分屏那样让 surface model 发生一次结构性重绑才能恢复。

关键不是“有一瞬间为空”，而是这个空 virtual view 进入了正常的 generation/cache 流程：

1. `load_directory()` 先进入 uncached 初始加载，`/etc` 条目还没有读完。
2. 启动状态更新、选择清空、layout/zoom 回调把 slot 0 当作已存在 pane 同步。
3. `sync_virtual_entries_for_slot_with_count(... immediate=true ...)` 在 `entries=0` 时构造并应用了空 virtual result。
4. 空 result 发布了 `range=0..0 entries=0 entry_count=0`，同时推进 `virtual_generation`。
5. 真实目录加载完成后，`DirectoryLoaded` 里已经有 `/etc entries=197`，但后续 prepare 结果要么被已经推进的 generation 判 stale，要么走到已有空 cache 的快速路径，无法把真实 entries 发布到 pane surface。

切换目录时的关键链路是：

1. `navigate_pane_to_slot()` 更新 `current_dir` 后进入 `load_current_directory_for_slot()`。
2. uncached load 先调用 `pane.clear_entries()`，然后状态/选择同步发布空 view。
3. `clear_entries()` 清掉了 entries/model/range，但旧 virtual layout signature 仍留在 `PaneView` 内。
4. layout/viewport 同步在 `entries=0` 时生成空 virtual layout，或者后续仍可能带着旧 layout 身份判断 cache。
5. `DirectoryLoaded` 设置真实 entries 后调用 `sync_pane_view_for_slot()`。
6. 如果 cache fast path 误命中，真实 entries 不会进入 `apply_virtual_view_result()` 的 rebuild path；即使 rebuild 成功，空 surface 到非空 surface 的 model 分支切换也需要结构性重绑，单纯的同 row `set_row_data` 不够稳定。

调试日志能直接看到这个顺序。修复前：

```text
load_directory ... path=/etc cache_hit=false
virtual_view_result applied ... range=0..0 entries=0 entry_count=0
directory_loaded ... path=/etc entries=197
virtual_view_result stale ... generation=5
```

修复后：

```text
load_directory ... path=/etc cache_hit=false
directory_loaded ... path=/etc entries=197
virtual_view_result applied ... range=0..110 entries=110 entry_count=197
```

## 为什么分屏能恢复

分屏会改变 pane model 的 shape，从单 pane 变成两个 pane。`sync_pane_slots_ui()` 因为 slot 列表变化而走整模型替换路径，新的 `PaneSlotSurface` 会用当时已有的真实 pane view 数据重建，所以内容重新出现。

这只是副作用修复，不是根治。它绕开了启动时已经发布的空 surface/cache，而不是解决首个真实目录结果被空 virtual view 抢跑的问题。

## 为什么之前的方案失败

### 预填充 `pane_surfaces`

在 `load_directory()` 前调用 `sync_pane_slots_ui()` 会更早创建空 surface。它没有阻止空 virtual view 抢跑，反而把“空 pane 已存在”变成确定状态，所以用户看到的问题不变。

### 扁平化 `PaneSurfaceData`

把 `PaneSurfaceData { view: PaneViewData }` 扁平化成 `entries/bounds/media/...` 字段没有触及根因。真实问题发生在 `entries=197` 的 virtual result 被应用之前：UI 先收到了空 virtual result，后续真实 result 又被 generation/cache 流程挡掉。因此这不是 Slint 嵌套 `ModelRc` 字段传播问题。

### 全量强制 `set_pane_surfaces(new_model)`

整模型替换能解释“分屏后恢复”，但把它用于目录加载完成只是绕过症状。它依旧允许启动阶段生成空 virtual view，后续还会继续污染 generation/cache 行为。

后续保留的是更窄的做法：只在 pane view 从空 item model 变成非空 item model，或从非空变成空时，重绑 `pane_surfaces`。普通滚动、选择高亮、thumbnail 更新不能触发 surface 重绑，否则会破坏虚拟视图性能。

## 修复策略

修复原则是：首个 pane surface 必须由真实目录数据驱动创建，启动阶段不能用空 entries 创建可渲染 pane。

当前修复点：

- 启动前显式清空 Slint 的 `pane_slots`、`pane_views`、`pane_surfaces`，移除默认/占位 row 对启动流程的影响。
- 去掉 `load_directory()` 后立即 `sync_navigation_ui()` 的启动全量 pane 同步，避免在目录结果返回前创建空 pane surface。
- uncached 初始加载时不再调用 `sync_pane_view_for_slot()` 生成空 virtual view；只更新路径、loading、状态等 chrome。
- `sync_pane_slot_ui()` 在找不到目标 row 时不再 fallback 到 `sync_pane_slots_ui()`，防止单行状态刷新创建缺失 pane。
- selection 更新只有在 pane view row 已存在时才刷新 `PaneViewData`，避免清空选择时触发首个空 view。
- layout/zoom 的 visible pane 同步在 pane model 为空时直接返回，不再把 “0 行” 解释成 slot 0。
- `cached_virtual_viewport_sync()` 增加当前目录可见 entry count 校验，禁止把 `entry_count=0` 的旧 virtual layout 复用于新加载的非空目录。
- `pane.clear_entries()` 改为彻底清理 virtual layout signature 和 thumbnail size，目录切换 loading 状态不再携带旧目录的 layout 身份。
- `sync_pane_view_ui()` 只在 item model 跨过空/非空边界时重绑 `pane_surfaces`，让已实例化的空 `SplitPaneView` 正确切到真实条目分支，同时避免滚动路径反复重建 surface。

## 验证

已通过：

- `cargo check`
- `cargo test`
- targeted test: `cached_virtual_viewport_rejects_stale_empty_layout_after_directory_switch`
- targeted test: `pane_clear_entries_drops_directory_virtual_layout_signature`
- targeted test: `pane_surface_rebind_is_limited_to_empty_model_boundary`
- Wayland GUI 烟测：`FIKA_DEBUG_NAV=1 timeout 5s target/debug/fika /etc`

烟测确认 `/etc` 的首个真实 virtual result 成功应用：

```text
directory_loaded slot=0 ok pane_id=1 generation=1 path=/etc entries=197 preserve_view=false
virtual_view_result applied pane_id=1 generation=4 range=0..156 entries=156 entry_count=197
```

## 经验

启动首屏不要依赖 Slint 默认模型状态，也不要让 layout/selection/status 回调在目录数据到达前创建 pane surface。pane 独立模型下，首个 `PaneSlotSurface` 应该和真实 `PaneViewData` 一起发布；否则空 virtual cache 会成为后续 generation 判定和 cache fast path 的污染源。

虚拟视图 cache 的命中条件不能只看几何尺寸。目录内容、搜索过滤结果、chooser 过滤结果都会改变 entry identity；至少必须校验当前可见 entry count，一旦无法可靠证明 cache 对应当前数据，就必须走 rebuild path。
