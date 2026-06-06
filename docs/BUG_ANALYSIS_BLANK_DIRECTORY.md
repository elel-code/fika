# Bug 分析：启动时目录内容空白，分屏后恢复正常

**日期**: 2026-06-06  
**状态**: 已定位并修复  
**结论**: 真实原因不是 Slint `set_row_data` 对嵌套模型字段传播失败，而是启动阶段过早发布了一个空的 pane/virtual view。

## 问题描述

fika 启动时，目录区域显示为空白，不渲染任何文件条目。触发分屏后内容恢复正常。

## 真实根因

启动时 Rust 侧没有显式清空 Slint 的 pane 模型。实测在首个目录真正加载完成前，`ui.get_pane_slots()`/`ui.get_pane_views()` 已经能让 slot 0 被当作可同步目标处理。随后启动流程中的状态、选择、布局同步把一个 entries 为空的 pane surface 和 virtual view 发布到了 UI。

关键不是“有一瞬间为空”，而是这个空 virtual view 进入了正常的 generation/cache 流程：

1. `load_directory()` 先进入 uncached 初始加载，`/etc` 条目还没有读完。
2. 启动状态更新、选择清空、layout/zoom 回调把 slot 0 当作已存在 pane 同步。
3. `sync_virtual_entries_for_slot_with_count(... immediate=true ...)` 在 `entries=0` 时构造并应用了空 virtual result。
4. 空 result 发布了 `range=0..0 entries=0 entry_count=0`，同时推进 `virtual_generation`。
5. 真实目录加载完成后，`DirectoryLoaded` 里已经有 `/etc entries=197`，但后续 prepare 结果要么被已经推进的 generation 判 stale，要么走到已有空 cache 的快速路径，无法把真实 entries 发布到 pane surface。

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

### 强制 `set_pane_surfaces(new_model)`

整模型替换能解释“分屏后恢复”，但把它用于目录加载完成只是绕过症状。它依旧允许启动阶段生成空 virtual view，后续还会继续污染 generation/cache 行为。

## 修复策略

修复原则是：首个 pane surface 必须由真实目录数据驱动创建，启动阶段不能用空 entries 创建可渲染 pane。

当前修复点：

- 启动前显式清空 Slint 的 `pane_slots`、`pane_views`、`pane_surfaces`，移除默认/占位 row 对启动流程的影响。
- 去掉 `load_directory()` 后立即 `sync_navigation_ui()` 的启动全量 pane 同步，避免在目录结果返回前创建空 pane surface。
- uncached 初始加载时不再调用 `sync_pane_view_for_slot()` 生成空 virtual view；只更新路径、loading、状态等 chrome。
- `sync_pane_slot_ui()` 在找不到目标 row 时不再 fallback 到 `sync_pane_slots_ui()`，防止单行状态刷新创建缺失 pane。
- selection 更新只有在 pane view row 已存在时才刷新 `PaneViewData`，避免清空选择时触发首个空 view。
- layout/zoom 的 visible pane 同步在 pane model 为空时直接返回，不再把 “0 行” 解释成 slot 0。

## 验证

已通过：

- `cargo check`
- `cargo test`
- Wayland GUI 烟测：`FIKA_DEBUG_NAV=1 timeout 5s target/debug/fika /etc`

烟测确认 `/etc` 的首个真实 virtual result 成功应用：

```text
directory_loaded slot=0 ok pane_id=1 generation=1 path=/etc entries=197 preserve_view=false
virtual_view_result applied pane_id=1 generation=4 range=0..110 entries=110 entry_count=197
```

## 经验

启动首屏不要依赖 Slint 默认模型状态，也不要让 layout/selection/status 回调在目录数据到达前创建 pane surface。pane 独立模型下，首个 `PaneSlotSurface` 应该和真实 `PaneViewData` 一起发布；否则空 virtual cache 会成为后续 generation 判定和 cache fast path 的污染源。
