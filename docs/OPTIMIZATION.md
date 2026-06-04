# Fika 性能优化

本文档记录 Fika 的性能改进方向，涵盖主栏横向滚动和焦点（focus）切换两大系统。
每个条目包含问题描述、涉及代码、改进方案和预估收益。

---

## 当前架构

### 滚动数据流

```
Slint ScrollView.viewport-x 变化
  └─ changed viewport-x 回调 (ui/split_pane.slint:147)
       └─ root.view_changed() → PaneRouting.view-changed(slot)
            └─ Rust: register_pane_routing_callbacks → view_changed_handler
                 └─ sync_virtual_entries()  [src/main.rs:3354, UI 线程]
                      ├─ MainGridLayout::from_ui()             [geometry.rs:44]
                      ├─ prepare_virtual_view_update()          [virtual_view.rs:55]
                      │    ├─ virtual_grid_plan()              [geometry.rs:169]
                      │    ├─ should_rebuild_virtual_model()   [virtual_view.rs:180]
                      │    ├─ filtered_entries_range()         (缓存未命中时)
                      │    └─ decorate_entries_with_cached..()  [thumbnail_pipeline.rs:12]
                      ├─ prioritize_thumbnail_entries()         [thumbnail_pipeline.rs:40]
                      ├─ schedule_visible_thumbnails()          (异步)
                      ├─ set_virtual_entries(VecModel)          → Slint
                      └─ sync_pane_slots_ui()                  [split_view.rs:41]
```

### 虚拟化三层结构 (ui/split_pane.slint)

```
ScrollView (viewport-x 双向绑定)
  └─ virtual-layer (全宽, 用于滚动条几何)
       └─ slice-layer (锚定到 virtual_start_column, 局部坐标)
            └─ for item in entries: FileTile  (每 tile 完整 Slint 组件)
```

### 现有优化

| 措施 | 位置 | 效果 |
|------|------|------|
| 虚拟化：Slint 只接收可见范围条目 | `virtual_view.rs` | 大目录不实例化全部 tile |
| 缓存命中免重建：`should_rebuild_virtual_model` | `virtual_view.rs:180` | 同范围内滚动零模型更新 |
| 缩略图优先可见列 | `thumbnail_pipeline.rs:40` | 减少首屏缩略图延迟 |
| 子像素漂移忽略 (epsilon=0.75) | `split_pane.slint:49-50` | 避免微小 viewport 变化触发同步 |
| 普通滚轮不重复请求焦点 | `split_pane.slint:78` | 减少 FFI 调用 |
| 滚动条 viewport-content-width 稳定全宽 | `split_pane.slint:45` | 避免滚动条宽度随虚拟切片抖动 |

---

## 改进方向

### P0 — 边界滚动提前退出

**问题**：当 viewport 已到达 0 或 `scroll-max-x` 边界时，每次 `changed viewport-x` 回调仍执行完整的 `stable-viewport-x` 计算和 Rust 侧 `sync_virtual_entries`。

**涉及代码**：
- `ui/split_pane.slint:147-155` — `changed viewport-x` 回调
- `src/app/split_view.rs:567` — `sync_virtual_entries`（通过 `PaneRouting.view-changed` 间接调用）

**改进**：在 Slint 侧的 `changed viewport-x` 回调中，夹紧后立即比较新旧 viewport-x：如果相同则直接返回，不调用 `view_changed()`。

```slint
changed viewport-x => {
    let clamped = root.stable-viewport-x(-self.viewport-x / 1px);
    if (root.viewport-x == clamped) {
        return; // 已夹紧，跳过 FFI 往返
    }
    root.viewport-x = clamped;
    root.viewport-offset = -root.viewport-x * 1px;
    root.view_changed();
    root.focus_requested();
}
```

需要确认当前 epsilon=0.75 的逻辑是否仍能正常工作——夹紧后 viewport-x 精确匹配 clamped，所以判断应该直接比较。

**收益**：消除所有边界滚动时的无效 FFI 往返和 Rust 计算。

**难度**：低。单文件、纯 Slint 修改。

---

### P1 — 滚轮事件合并 (Coalesce)

**问题**：快速滚轮滚动时，Slint 每帧触发 `changed viewport-x`，每个事件走过完整 `sync_virtual_entries` 调用链。即使在缓存命中时不需要重建模型，仍然要执行 `MainGridLayout::from_ui()` + `virtual_grid_plan()` + `sync_pane_slots_ui()`。

**涉及代码**：
- `src/main.rs:3354` — `sync_virtual_entries`
- `ui/split_pane.slint:147` — `changed viewport-x` 触发点

**改进**：在 Rust 侧加一个短合并窗口（~8ms，约半帧）：

```
滚动事件到达
  ├─ 立即写回 viewport_x 到 Slint（保证渲染不卡）
  └─ 启动/重置 8ms coalesce timer
       └─ timer 到期 → 执行一次 sync_virtual_entries
```

关键设计点：
- viewport_x 的 Slint 回写不在合并窗口内——每次事件都立即写回，保证 Slint 的 ScrollView 实时跟手
- 只有虚拟切片同步（`sync_virtual_entries`）在合并窗口内
- 需要从 `PaneRouting.view-changed` 回调路径中提取合并逻辑

**收益**：高速滚动时 Rust 计算量下降 60-80%（从每帧变为每 2-3 帧一次）。

**难度**：中。需要引入 coalesce timer，且要与现有的 `RefCell<AppState>` 借用模式协调。

---

### P1 — `sync_pane_slots_ui` 去重

**问题**：`pane_slot_data()` 每次从 UI 读取 20+ 个属性（搜索查询、过滤器、缩放级别、选中状态等），然后构建 `Vec<PaneSlotData>`。即使用 `same_slots` 检查跳过 Slint model 重建，`vec` 分配、行数据设置和 FFI 读取仍然执行。

**涉及代码**：
- `src/app/split_view.rs:41-62` — `sync_pane_slots_ui`
- `src/app/split_view.rs:72-152` — `pane_slot_data`

**改进**：
1. 缓存上次写入的 `PaneSlotData` 快照，逐字段比较后再写入
2. 对于从 `AppState` 可获取的数据（如当前路径、历史状态），直接从内存读取而非回读 Slint 属性

```rust
fn sync_pane_slots_ui(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let slots = visible_pane_slots(ui);
    let current_model = ui.get_pane_slots();
    // 快速路径：slot 数量相同且每个 slot 的数据未变更
    if slots.len() == current_model.row_count() && !pane_slot_data_changed(ui, &slots) {
        return;
    }
    // ... 现有逻辑
}
```

**收益**：每次滚动同步减少 20+ 次 Slint FFI 属性读取。

**难度**：低。纯 Rust 修改，不改变任何 Slint 接口。

---

### P2 — Slint Model 增量更新

**问题**：`set_virtual_entries(ModelRc::new(Rc::new(VecModel::from(update.entries))))` 每次都创建全新 `VecModel`。当向右滚动 1 列时，新旧虚拟范围重叠 ~80%，但 Slint 仍然重新创建所有 `FileTile` 实例。

**涉及代码**：
- `src/main.rs:3407` — `set_virtual_entries`
- `src/app/split_view.rs:399` — `set_pane_slot_entries_ui`
- `ui/split_pane.slint:228` — `for item[index] in root.entries: FileTile`

**改进**：保留当前 `VecModel` 引用，比较新旧范围的重叠部分，增量更新：

```rust
fn apply_virtual_model_update(ui: &AppWindow, slot: i32, update: &VirtualViewUpdate) {
    let current = ui.get_virtual_entries(); // 假设有此 getter
    let overlap = compute_range_overlap(&current_range, &update.range);
    if overlap > 0.5 && current.row_count() > 0 {
        // 增量：set_row_data 逐行替换，push/remove 处理范围变化
        apply_incremental_model_update(ui, slot, current, update);
    } else {
        // 全量替换
        set_pane_slot_entries_ui(ui, slot, update.entries.clone());
    }
}
```

**收益**：连续滚动时 FileTile 实例重建开销下降 50-70%。

**难度**：中高。需要维护新旧范围的映射关系，处理前/后偏移的边界情况。Slint 1.16.1 的 `VecModel` 支持 `set_row_data` 和 `push`/`remove`，但需要确认 `set_row_data` 会正确触发 `FileTile` 的属性更新而非重建。

**注意**：Slint 的 `for` 循环在 model 变化时的行为需要实测验证——如果 `set_row_data` 触发的是组件复用而非重建，则收益巨大；如果仍然重建，则需要换用其他策略（如增加 overscan 列并降低重建频率）。

---

### P2 — 缩略图批量写入

**问题**：缩略图异步生成完成后，通过回调逐个写入 Slint model。每个缩略图写入触发一次 `Image` 属性变更和潜在的 `FileTile` 重绘。如果 20 张缩略图在 100ms 内到达，可能触发 20 次属性评估。

**涉及代码**：
- `src/main.rs` — `schedule_visible_thumbnails` 及相关回调
- `src/app/thumbnail_pipeline.rs:12` — `decorate_entries_with_cached_thumbnails`

**改进**：将缩略图完成事件收集到一个批次缓冲区中，每 16ms（一帧）批量写入 Slint model：

```
缩略图完成 → 写入 batch buffer
每 16ms tick → 一次性将所有新缩略图写入 Slint model
```

**收益**：减少缩略图密集到达时段的重绘触发频率。对大目录快速滚动时的新缩略图加载尤为有效。

**难度**：中。需要引入帧对齐的 flush 机制，可能与现有的 `spawn_local` 模式交互。

---

### P3 — UI 线程计算后移

**问题**：整个虚拟视图计算（条目克隆、缩略图缓存查找、`filtered_entries_range`）在 UI 线程执行，持有 `RefCell<AppState>` 借用。在 120Hz+ 显示器或条目数量大时可能触碰帧预算。

**涉及代码**：
- `src/main.rs:3354-3412` — `sync_virtual_entries` / `sync_virtual_entries_with_count`
- `src/app/virtual_view.rs:55` — `prepare_virtual_view_update`

**改进**：将计算阶段和写入阶段分离：

- **后台线程**：`prepare_virtual_view_update` 的计算部分（输入为 `VirtualViewInput`，输出为 `VirtualViewUpdate`）
- **UI 线程**：只做 Slint 属性写入（`set_virtual_entries`、`set_entry_count`）

这需要将 `AppState` 的访问模式改为线程安全（`Arc<Mutex<>>` 或读时快照），复杂度高。

**收益**：彻底解除 UI 线程的计算负担，在高刷显示器上保证帧率。

**实际实现**（✅ 已完成）：采用快照模式而非锁迁移——`PaneEntrySnapshot`（不含 `Image` 的轻量结构体，`Arc<[PaneEntrySnapshot]>` 零拷贝共享）+ `VirtualViewSnapshotInput`（完全 owned 的纯函数输入）。UI 线程按 pane slot 构建 snapshot 输入后通过 `tokio::spawn_blocking` 在后台执行 `prepare_virtual_view_snapshot_update`（纯函数），结果通过 `AsyncEvent::VirtualViewPrepared` 回传。`virtual_generation` 独立于 `load_generation` 做 staleness 检测，`apply_virtual_view_result` 先在 `borrow_mut` 块内写 state 再 drop 后写 Slint model，无 RefCell 跨线程风险。所有可见 pane 都走同一条 slot 驱动虚拟视图管线，旧的 preview/副 pane 专用路径已删除。

---

### P3 — FileTile 组件简化

**问题**：每个 `FileTile` 是完整 Slint 组件，包含图标、文件名、大小文本、选中状态、拖拽状态等子组件。当有 80-120 个可见 tile 时，每个都有自己的属性绑定评估树。

**涉及代码**：
- `ui/file_tile.slint` — `FileTile` 组件定义
- `ui/split_pane.slint:228-272` — tile 实例化循环

**改进**：
1. 将非交互的纯展示属性（图标大小、字体颜色、背景色）的计算上提到 `SplitPaneView` 层的 `private property`，避免每个 `FileTile` 独立计算
2. 确认 Slint 是否对 `for` 循环内的组件做属性缓存——如果已经在底层做了，则收益有限

**收益**：减少大量 tile 时的属性绑定评估开销。

**难度**：中。涉及 Slint 组件重构，需要确认 Slint 1.16.1 的组件实例化模型。

---

## 焦点（Focus）切换性能

### 焦点触发全景

几乎所有用户交互都会触发 `focus_requested`——包括滚动、点击、右键菜单、文件操作等。全链路如下：

```
Slint: 用户交互（滚动/点击/右键菜单/...）
  → focus_requested(slot) 回调
    → PaneRouting.focus(slot)
      → route-pane-focus(slot)           [ui/app.slint:930]
        → app-focus.focus()              ← 回到全局快捷键 FocusScope
        → pane_focus(slot)               ← Rust FFI
          → focus_pane_slot()            [src/main.rs:3894]
            → state.panes.focus_slot(slot)
            → if 实际切换: sync_navigation_ui()  [split_view.rs:571]
              → ~15 次 Slint setter
              → sync_pane_slots_ui()
```

焦点触发来源与频率：

| 触发来源 | Slint 位置 | 频率 |
|---------|-----------|------|
| 滚轮滚动 | `split_pane.slint:67` `pan-horizontal` | 每帧 |
| ScrollView viewport 变化 | `split_pane.slint:153` `changed viewport-x` | 每帧 |
| Ctrl+滚轮缩放 | `split_pane.slint:80` `handle-scroll` | 按需 |
| 点击 tile / 空白区 | `split_pane.slint` TouchArea | 按需 |
| PathBar Back/Forward 按钮 | `top_bar.slint:184,194` | 按需 |
| PathBar 输入框获焦 | `top_bar.slint:239` | 按需 |
| 右键菜单请求 | `split_pane.slint:246` | 按需 |
| 缩放/激活/选择等操作 | 多处 | 按需 |
| Chooser / external edit | `app.slint:338,1096` | 按需 |

已存在的 Rust 侧守卫（`src/main.rs:3894-3903`）：

```rust
fn focus_pane_slot(ui, state, slot) {
    let previous_slot = { state.borrow().panes.focused_slot() };
    let focused = { state.borrow_mut().panes.focus_slot(slot) };
    if focused && previous_slot != slot {
        sync_navigation_ui(ui, state);  // 只在焦点实际切换时执行
    }
}
```

但守卫只在 Rust 侧生效——Slint 侧的 `route-pane-focus` 和 FFI 调用仍然每次都执行。

---

### F0 — Slint 侧 `route-pane-focus` 提前退出

**问题**：`route-pane-focus(slot)` 曾无条件执行 focus + `pane_focus(slot)`，即使 slot 已经是当前焦点。快速滚动时每帧触发两次（`pan-horizontal` + `changed viewport-x`），每次都做无效的 `FocusScope` 重算和 FFI 往返。

```slint
// 当前 (ui/app.slint:1002)
public function route-pane-focus(slot: int) {
    app-focus.focus();
    root.pane_focus(slot);
}
```

**涉及代码**：
- `ui/app.slint:1002-1005` — `route-pane-focus`

**实际实现**（✅ 已完成）：`route-pane-focus` 先把焦点回收到 `app-focus`（全局 `KeyBinding` 所在的 FocusScope），然后只在 slot 变化时调用 `pane_focus(slot)`。这既恢复文件区点击后的 Ctrl+A/C/V/Delete 等窗口级快捷键，又避免同 slot 点击时重复 FFI。

```slint
public function route-pane-focus(slot: int) {
    app-focus.focus();
    if (root.focused_pane == slot) {
        return;
    }
    root.pane_focus(slot);
}
```

`PathBar` 的 `TextInput` 获焦不再调用 `focus_requested()`，只通过 `pane_path_focus_changed(slot, true)` 切换 pane 状态，避免地址栏刚获焦又被 `app-focus.focus()` 抢走。

**注意事项**：
- 首次启动时 `focused_pane` 初始值为 0，与 slot 0 匹配，第一帧不会跳过分发
- `AppWindow` 没有其他地方直接修改 `focused_pane`（均走 `pane_focus` → Rust → `set_focused_pane`），不存在不同步风险
- 如果将来有 Slint 侧直接设置 `focused_pane` 的路径，需确保保持一致性

**收益**：消除同一 pane 内滚动/点击时的 Slint `FocusScope` 重算和一次 FFI 往返。快速滚动场景收益最明显——每帧省两次无效调用。

**难度**：极低。单行改动。

---

### F1 — 滚动事件中移除冗余的 `focus_requested`

**问题**：`pan-horizontal` 和 `changed viewport-x` 中每次都调用 `focus_requested()`。滚动的 pane 必然是用户正在交互的 pane，焦点从首次点击/滚轮时就已经设好。配合 F0 的提前退出后这些调用的成本已大幅降低，但仍是两次属性比较 + 分支。

```slint
// 当前 (ui/split_pane.slint:60-68)
function pan-horizontal(delta: length) {
    root.pan-target-viewport-x = ...;
    if (root.pan-target-viewport-x != root.viewport-x) {
        root.viewport-x = root.pan-target-viewport-x;
        root.viewport-offset = -root.viewport-x * 1px;
        root.view_changed();
    }
    root.focus_requested();  // ← 冗余
}
```

```slint
// 当前 (ui/split_pane.slint:147-155)
changed viewport-x => {
    let clamped = root.stable-viewport-x(-self.viewport-x / 1px);
    if (...) {
        root.viewport-x = clamped;
        root.viewport-offset = -root.viewport-x * 1px;
        root.view_changed();
        root.focus_requested();  // ← 冗余
    }
}
```

注意 `handle-scroll` 中 Ctrl+滚轮的 `focus_requested()` 是合理的——Ctrl+滚轮切换缩放模式，确实需要声明焦点。**只需移除 `pan-horizontal` 和 `changed viewport-x` 中的调用**。

**涉及代码**：
- `ui/split_pane.slint:67` — `pan-horizontal` 末尾的 `focus_requested()`
- `ui/split_pane.slint:153` — `changed viewport-x` 回调中的 `focus_requested()`

**安全性分析**：pane 内容现在通过 `PaneSlotSurface`/`PaneSlot` 统一路由，滚动和 viewport 变化都携带 slot 并写回对应 pane 的 `DirectoryViewState`。普通滚动不需要额外声明焦点；需要焦点语义的路径（点击激活、Ctrl+滚轮缩放、右键菜单、拖放）仍显式走 slot-aware focus/route 回调。

**收益**：每帧省两次属性比较 + 分支判断（配合 F0 后为两次整数比较）。

**难度**：低。两行删除。如果担心边缘情况可先只移除 `pan-horizontal` 中那处，保留 `changed viewport-x` 中作为保险（配合 F0 也几乎无开销）。

---

### F2 — 纯焦点切换时跳过左栏属性重写

**问题**：焦点从 slot 0 切到 slot 1（或反之）时，`sync_navigation_ui` 写入 ~8 个左栏属性（path、can_go_back、can_go_forward、in_trash、status、selected_count、selected_status）。这些值在焦点切换时完全没有变化，但每次 Slint setter 调用仍触发属性变更通知，可能引起下游绑定重算。

**涉及代码**：
- `src/app/split_view.rs:571-617` — `sync_navigation_ui`
- `src/main.rs:3894-3903` — `focus_pane_slot`（调用方）

当前 `sync_navigation_ui` 逻辑（简化）：

```rust
pub(crate) fn sync_navigation_ui(ui, state) {
    let snapshot = { /* 读 focused_slot, focused_dir, left_dir, ... */ };
    // 写左栏（~8 setter）——焦点切换时这些值不变
    ui.set_left_pane_path(...);
    ui.set_left_pane_can_go_back(...);
    // ...
    ui.set_split_view_open(...);
    // 写焦点 pane（~6 setter）——需要更新
    sync_focused_ui(ui, snapshot.focused_slot, ...);
    sync_pane_slots_ui(ui, state);
}
```

**改进**：将 `sync_navigation_ui` 拆分为两个路径，或内部做脏检查。

方案 A（拆分路径）：
```rust
fn sync_focus_change_only(ui, state) {
    let (focused_slot, focused_dir, focused_selection) = { ... };
    sync_focused_ui(ui, focused_slot, &focused_dir, &focused_selection);
    sync_pane_slots_ui(ui, state);
}
```

方案 B（内部脏检查）：在 `sync_navigation_ui` 中缓存上次写入的左栏值，比较后按需写入。`NavigationUiSnapshot` 已包含所有需要的字段。

**收益**：焦点切换时减少 ~8 次无效 Slint setter 调用及潜在的下游绑定重算。

**难度**：中。需要重构 `sync_navigation_ui`，拆分调用路径或增加内部缓存。逻辑清晰但涉及多处调用点（`focus_pane_slot`、`sync_pane_slot_directory` 等均调用 `sync_navigation_ui`）。

**优先级**：P2。F0+F1 已解决高频场景（每帧滚动触发），F2 只影响低频的焦点切换（slot 0↔1）。

---

### G0 — pane path focus 单 slot 更新

**原问题**：路径输入框焦点变化曾通过 `set-pane-slot-path-focused` 写固定左右 pane 属性并触发 `pane_slots_refresh_requested()`，导致完整重建 `Vec<PaneSlotData>`。但唯一变化的数据只是当前 slot 的 `PaneSlotData.path_focused`。

**实际实现**（✅ 已完成）：旧的 `set-pane-slot-path-focused` / `left_pane_path_focused` / `inactive_pane_path_focused` 路径已删除。`PaneSlotSurface` 直接把 `path_focus_changed(slot, focused)` 路由到 `AppWindow.pane_path_focus_changed(int, bool)`；Rust 回调写入对应 pane 的 `path_focused`，在 `focused == true` 时切换对应 pane 的 focused slot，然后调用 `sync_pane_slot_ui(&ui, &state, slot)`，只更新单个 `PaneSlotData` row。

**涉及代码**：
- `ui/app.slint` — `path_focus_changed(slot, focused) => root.pane_path_focus_changed(slot, focused);`
- `src/main.rs` — `ui.on_pane_path_focus_changed(...)`
- `src/app/split_view.rs` — `sync_pane_slot_ui`

**收益**：路径输入框获焦/失焦时，从重建整个 `Vec<PaneSlotData>` 降为单行 `set_row_data`。

**难度**：已完成。`pane_slots_refresh_requested` 仍保留给 view/layout/search/filter 等需要刷新全部 pane slot 数据的路径；纯 pane-local UI 状态变化走带 slot 的回调。

---

## 虚拟网格内部优化

以下优化针对虚拟网格计算链路本身（`virtual_view.rs`、`geometry.rs`、`selection.rs`），聚焦单次计算内部的微优化，与 Phase 1-4（控制何时重建模型）互补。

---

### V0 — `is_selected` 每 tile 一次 FFI 预计算

**问题**（`ui/split_pane.slint:253`）：

```slint
selected: root.selection-revision >= 0 && root.is_selected(item.path);
```

100 个可见 tile → 每次选择变化时 100 次 `PaneRouting.is-selected(slot, path)` FFI 调用。`selection-revision >= 0` 的守卫只在首次选择前生效。

**涉及代码**：
- `ui/split_pane.slint:253` — tile 的 `selected` 绑定
- `PaneRouting.is-selected` — 全局回调注册
- `src/app/selection.rs` — `PaneSelection` 查找逻辑

**实际实现**（✅ 已完成）：`FileEntry` 已新增 `selected: bool` 字段，`SplitPaneView` tile 直接读 model 字段：

```slint
selected: item.selected;
```

选择变化时，Rust 侧通过 `update_file_entries_model_selection` 对当前 pane 的虚拟 `VecModel<FileEntry>` 做逐行 `set_row_data` 脏更新；后台虚拟视图结果应用时也会用当前 pane 的 selection 调用 `annotate_selection_state`，防止旧异步结果覆盖当前高亮。渲染路径上的 `PaneRouting.is-selected` / `FilePane.is_selected` 回调已删除，`pane_is_selected(slot, path)` 仅保留给右键菜单命令逻辑。

**收益**：每次选择变化（点击、框选、Ctrl+A）省 80-120 次 FFI 调用。

**难度**：已完成。源码守卫测试覆盖 `selected: item.selected`，并防止恢复 tile 级 `is_selected` 回调。

---

### V1 — `virtual_entry_range` 双重计算融合

**问题**（`geometry.rs:186-203`）：

```rust
let range = virtual_entry_range(..., overscan_columns);  // 第一次
let visible_range = virtual_entry_range(..., 0);           // 第二次
```

两次调用重复计算相同的 column math。带 overscan 的范围天然包含不带 overscan 的范围。

**涉及代码**：
- `src/app/geometry.rs:169-215` — `virtual_grid_plan`
- `src/app/geometry.rs` — `virtual_grid_plan` / `virtual_entry_ranges`

**实际实现**（✅ 已完成）：`virtual_grid_plan` 现在调用内部 `virtual_entry_ranges`，一次计算 `first_visible_column` / `visible_end_column`，同时返回 overscan range 和 visible range。旧的单 range 包装函数已删除，避免非测试构建保留死代码。

```rust
fn virtual_entry_ranges(..., overscan_columns) -> (Range<usize>, Range<usize>) {
    let first_visible_column = ...;
    let visible_end_column = ...;
    let overscan_range = entry_range_for_columns(...);
    let visible_range = entry_range_for_columns(first_visible_column, visible_end_column, ...);
    (overscan_range, visible_range)
}
```

**收益**：每次 `virtual_grid_plan` 省一次除法/floor/ceil 链。

**难度**：已完成。现有 `virtual_grid_plan` 测试覆盖边界、overscan 和 viewport clamp 行为。

---

### V2 — `filtered_entries_range` 中 `filter_map` → `map`

**问题**（`selection.rs:212-220`）：当有 `visible_entry_indices` 时：

```rust
indices[..].iter()
    .filter_map(|index| state.panes.focused().entries.get(*index))
    .cloned().collect()
```

`visible_entry_indices` 的 index 由 `rebuild_visible_entry_index` 构建时保证指向有效条目，`filter_map` 过滤 `None` 是多余的防御性代码。

**涉及代码**：
- `src/app/selection.rs:207-244` — `filtered_entries_range`

**改进**：

```rust
indices[range.start..end]
    .iter()
    .map(|&index| state.panes.focused().entries[index].clone())
    .collect()
```

**安全性**：索引在目录切换/search 重置时重建，永不过期。

**收益**：有搜索/过滤时每条目省 `Option` 解包。

**难度**：极低。

---

### V3 — 旧 preview 路径删除

**原问题**：`prepare_pane_preview_update` 的 preview-only 路径曾在缓存命中滚动时提前 clone `current_dir`。

**实际状态**（✅ 已完成/不适用）：旧的 `prepare_pane_preview_update`、`sync_pane_slot_preview_ui` 和 preview-only pane 管线已删除。所有可见 pane 都通过同一套 slot-aware `sync_virtual_entries_for_slot` / `VirtualViewSnapshotInput` / `apply_virtual_view_result` 管线更新，因此不再存在可单独优化的 preview clone 路径。

---

### V4 — `annotate_visible_location_groups` 边界缓存

**问题**（`selection.rs:247-295`）：每次 `filtered_entries_range` 扫描整个虚拟切片找 group/location 边界。递归搜索场景下每次滚动都重新扫描。

**涉及代码**：
- `src/app/selection.rs:247-295` — `annotate_visible_location_groups`
- `src/app/virtual_view.rs` — `VirtualViewSnapshotInput.visible_location_groups`

**实际实现**（✅ 已完成）：不维护易过期的局部 `(start_visible_index, previous_location, annotations)` 滚动缓存，而是在过滤/search 重建可见索引时一次性生成 pane-local `visible_location_groups`。`VirtualViewSnapshotInput` 将该缓存随条目快照传入后台 `prepare_virtual_view_snapshot_update`，虚拟切片标注直接按 `start_visible_index + offset` 读取预计算 group；只有缺少缓存时才回退到按 slice 边界推断。

**收益**：递归搜索场景下滚动不再为每个虚拟切片重新查找前一条 location 边界，也不在后台 snapshot 路径重复推断 group 标签。

**验证**：`snapshot_update_uses_precomputed_visible_location_groups` 覆盖非零虚拟 range 起点，证明后台 snapshot 路径使用预计算 group 缓存而不是从 entry.location 重新推断。

**优先级**：P3（低频）。

---

## 跨系统通用优化

以下优化涉及 Places、剪贴板、缩略图、Slint 绑定等非滚动/非焦点的子系统。

---

### S0 — 缩略图后台 spawn 批量化

**问题**（`src/main.rs:3903-3927`）：`schedule_visible_thumbnails` 为每个路径创建独立的异步任务——N 个缩略图 = N 个 `bridge.handle.spawn` + N 个 `tokio::spawn_blocking`。Phase 4 的 `ThumbnailFlushScheduler` 已批量化 UI 线程的结果写入，但 spawn 风暴仍消耗 tokio 调度开销。

**涉及代码**：
- `src/main.rs:3880-3928` — `schedule_visible_thumbnails`

**改进**：将路径批量提交到单个 `spawn_blocking` 中顺序处理，减少 tokio task 数量从 N 到 1。

```rust
bridge.handle.spawn(async move {
    let results: Vec<_> = tokio::task::spawn_blocking(move || {
        paths.into_iter()
            .map(|path| thumbnails::load_thumbnail(path, size_px))
            .collect()
    }).await...;
    for load in results {
        send_async_event(..., AsyncEvent::ThumbnailLoaded { generation, load });
    }
});
```

**收益**：减少 tokio 调度开销。`send_async_event` 仍每个 load 调用一次（触发 `upgrade_in_event_loop`），但 spawn 数量降为 1。

**实际实现**（✅ 已完成）：`schedule_visible_thumbnails` 现在只创建一个 async task，并在其中用一个 `tokio::spawn_blocking` 顺序处理整批 `paths`。加载成功后仍逐个发送 `AsyncEvent::ThumbnailLoaded`，沿用现有 `ThumbnailFlushScheduler` 的 UI 线程批量写入；如果 blocking task 失败，会按原始 path 逐个生成失败 `ThumbnailLoad`，确保 pending 状态能被清理。

**难度**：已完成。`paths` 直接移入单个 spawn；失败兜底保留每个 path 自己的 fallback key。

---

### S1 — Places 模型增量更新

**问题**（`src/app/places.rs:11-13`）：`sync_places` 每次调用创建全新 `VecModel`：

```rust
pub(crate) fn sync_places(ui: &AppWindow, places: &[PlaceEntry]) {
    ui.set_places(ModelRc::new(Rc::new(VecModel::from(places.to_vec()))));
}
```

add/remove/rename/reorder 均触发全量重建。

**建议**：搁置。Places 通常 < 20 条目，操作不频繁，投入产出比极低。除非将来支持大列表（数百个书签），否则不值得。

---

### S2 — 右键菜单跳过剪贴板读取

**问题**（`ui/app.slint:1047-1048`）：每次打开右键菜单都调用 `refresh_clipboard_availability()` → `clipboard::read_clipboard_snapshot()` —— 这是 Wayland clipboard 协议查询，可能阻塞。

剪贴板内容不会因为打开菜单而变化。只需在以下时机刷新：
- 窗口重新获得焦点
- Ctrl+C/Ctrl+X 操作后
- 初始启动时

**涉及代码**：
- `ui/app.slint:1047` — `route-pane-request-context-menu` 中 `refresh_clipboard_availability()`
- `src/app/file_clipboard.rs:287` — `refresh_clipboard_availability`

**实际实现**（✅ 已完成）：右键菜单回调中将 `refresh_clipboard_availability()` 替换为 Slint 回调 `sync_clipboard_state()`；Rust 侧 `ui.on_sync_clipboard_state` 只调用 `sync_clipboard_ui(&ui, &state)`，不读取 Wayland clipboard。Ctrl+V 仍保留 `refresh_clipboard_availability()`，用于 paste 前主动刷新缓存。

**收益**：消除每次右键的 clipboard 协议查询延迟。

**难度**：已完成。源码守卫测试限制右键菜单函数只能走 `sync_clipboard_state()`，但不禁止 Ctrl+V 的主动刷新路径。

---

### S3 — `file-operation-shortcuts-blocked` 依赖归约

**问题**（`ui/app.slint:747`）：19 个 property 的 OR 表达式，Slint 需要追踪 19 个依赖。每次任一 property 变化时标记 binding dirty 并重算。

当前 `transient-popup-open`（`ui/app.slint:746`）已归约了 15 个 popup 相关 property。`file-operation-shortcuts-blocked` 可以用 `transient-popup-open` 替代其中 15 个，把依赖从 19 降到 4：

```slint
private property <bool> file-operation-shortcuts-blocked:
    root.search-input-focused || root.chooser-save-input-focused || root.transient-popup-open;
```

**涉及代码**：
- `ui/app.slint:747` — `file-operation-shortcuts-blocked`

**实际实现**（✅ 已完成）：`file-operation-shortcuts-blocked` 已归约为 `root.search-input-focused || root.chooser-save-input-focused || root.transient-popup-open`。旧的固定 left/inactive path focused 属性已经删除，pane path focus 现在是 `PaneSlotData` 的 pane-local 状态，不参与全局文件操作快捷键阻塞。

**收益**：依赖从 19 降到 3，binding 重算频率大幅降低。Slint OR 短路求值本就高效，但依赖追踪的 dirty-marking 成本确实随依赖数量线性增长。

**难度**：已完成。源码守卫测试覆盖归约后的表达式。

---

## 潜在问题排查

以下是与性能相关的已知限制或需要排查的点：

### ScrollView viewport-width 动态计算

```slint
viewport-width: max(parent.width, root.viewport-content-width);
```

`viewport-content-width` 依赖 `column-count`，而 `column-count` 依赖 `entry-count`。在目录切换时 `entry-count` 变化会导致 viewport 宽度变化，触发 ScrollView 内部重布局。当前 `entry-count` 只在模型重建时更新，路径是正常的。

### TouchArea 覆盖全宽

```slint
TouchArea {
    width: preview.viewport-width;
    height: preview.viewport-height;
```

当 viewport 很宽（如 10000 条目目录）时，`TouchArea` 面积很大。但 Slint 的 ScrollView 会自动裁剪触摸区域，实际开销应可控。

### pan-horizontal 中的 viewport-x 比较

```slint
if (root.pan-target-viewport-x != root.viewport-x) {
```

这是浮点数精确比较。得益于 `stable-viewport-x` 中的 `floor(.. + 0.5)` 取整，两个值在正常路径下总是精确整数，比较是安全的。但如果将来修改了取整逻辑，需要改为 epsilon 比较。

---

## 实施路线图

### 滚动优化

| 阶段 | 改进 | 预计工作量 | 状态 |
|------|------|-----------|------|
| **Phase 1** | P0 边界提前退出 + P1 `sync_pane_slots_ui` 去重 | 1-2h | ✅ 已完成 |
| **Phase 2** | P1 滚轮事件合并 (Coalesce 8ms) | 2-3h | ✅ 已完成 |
| **Phase 3** | P2 Slint model 增量更新 (`model_update.rs`) | 3-4h | ✅ 已完成 |
| **Phase 4** | P2 缩略图批量写入 (Flush 16ms) | 2-3h | ✅ 已完成 |
| **Phase 5** | P3 UI 线程计算后移 | 1-2d | ✅ 已完成 |
| **Phase 6** | P3 FileTile 简化 | 1-2h | ✅ 已完成 |

**Phase 1-6 实现要点**：
- **Phase 1**: `changed viewport-x` 提前退出 + `sync_pane_slots_ui` row_data 脏检查 + 新增 `sync_pane_slot_ui` 单 slot 增量
- **Phase 2**: `PaneViewSyncScheduler` (8ms `slint::Timer` SingleShot) + `sync_pane_viewport_for_slot` viewport-only 路径 + layout/flush 分离
- **Phase 3**: 新模块 `src/app/model_update.rs` — `VecModel::downcast_ref` 增量更新，支持前/后滑动 + `set_row_data` 逐行脏检查
- **Phase 4**: `ThumbnailFlushScheduler` (16ms) — 缩略图结果入队批量写入，`AsyncEvent::ThumbnailLoaded` 不再逐张触发 `sync_virtual_entries`
- **Phase 5**: `PaneEntrySnapshot`（不含 `Image` 的轻量快照, `Arc` 零拷贝共享）+ `VirtualViewSnapshotInput`（完全 owned 的纯函数输入）— 虚拟视图的条目过滤/切片/clone/location 标注全部在 `tokio::spawn_blocking` 中完成，UI 线程只做 generation staleness 检查 + Slint 模型写入 + 缩略图缓存装饰。`virtual_generation` 独立于 `load_generation`，目录切换时自动推进。`apply_virtual_view_result` 先在 `borrow_mut` 内写 state 再 drop 后写 Slint，避免 RefCell 跨线程风险。所有可见 pane 走同一条 slot-aware 虚拟视图管线。
- **Phase 6**: `FileTile` 所有 zoom/dark 计算上移到 `SplitPaneView`

**审查发现的后继微优化**：
- **cleanup-1**: 老路径 `prepare_virtual_view_update`（`VirtualViewUpdate`）已无主路径调用，可加 `#[allow(dead_code)]` 或移除
- **f2-note**: `sync_focus_navigation_ui` 调用的 `sync_focused_ui` 内部仍执行 `sync_pane_slots_ui`。Phase 1 的 row_data 脏检查使其开销极小（O(2) 次比较），进一步跳过属于可选微优化

### 焦点优化

| 阶段 | 改进 | 预计工作量 | 状态 |
|------|------|-----------|------|
| **Phase F0** | Slint 侧 `route-pane-focus` 提前退出 | 5min | ✅ 已完成 |
| **Phase F1** | 滚动事件中移除冗余 `focus_requested` | 5min | ✅ 已完成 |
| **Phase F2** | 纯焦点切换时跳过左栏重写 | 30min-1h | ✅ 已完成 |
| **Phase G0** | pane path focus 单 slot 更新 | 15min | ✅ 已完成 |

**焦点已实现要点**：
- **F0**: `route-pane-focus` 加 `if root.focused_pane == slot` 守卫，且额外处理输入框焦点回收
- **F1**: `pan-horizontal` 和 `changed viewport-x` 中的 `focus_requested()` 已移除；`handle-scroll` 中 Ctrl+滚轮的调用保留
- **F2**: 新增 `sync_focus_navigation_ui` — 与 `sync_navigation_ui` 相比跳过左栏 8 setter 和 `set_split_view_open`，只读取 focused pane 数据并写入 `sync_focused_ui`。内部 `sync_pane_slots_ui` 由 Phase 1 脏检查保护（O(2) 比较即返回），进一步跳过为可选微优化

### 虚拟网格内部优化

| 阶段 | 改进 | 预计工作量 | 状态 |
|------|------|-----------|------|
| **Phase V0** | `is_selected` FFI 预计算到 FileEntry | 1h | ✅ 已完成 |
| **Phase V1** | `virtual_entry_range` 双重计算融合 | 15min | ✅ 已完成 |
| **Phase V2** | `filtered_entries_range` filter_map→map | 5min | ✅ 已完成 |
| **Phase V3** | 旧 preview 路径删除 | 5min | ✅ 已完成/不适用 |
| **Phase V4** | `annotate_visible_location_groups` 缓存 | 30min | ✅ 已完成 |

### 跨系统通用优化

| 阶段 | 改进 | 预计工作量 | 状态 |
|------|------|-----------|------|
| **Phase S0** | 缩略图后台 spawn 批量化 | 15min | ✅ 已完成 |
| **Phase S1** | Places 模型增量更新 | 搁置 | — |
| **Phase S2** | 右键菜单跳过剪贴板读取 | 10min | ✅ 已完成 |
| **Phase S3** | `file-operation-shortcuts-blocked` 归约 | 5min | ✅ 已完成 |

**综合建议**：S1 继续搁置，除非 Places 列表规模显著变大。

**综合建议**：滚动 Phase 1-6、焦点 F0-F2/G0、V0-V4、S0/S2/S3 已完成。剩余 S1 搁置。

### 验证方法

#### 滚动验证

1. **大目录滚动**：`/usr/lib`（1000+ 条目），快速连续滚轮
2. **边界反弹**：滚动到最左/最右，继续滚轮确认无多余计算
3. **搜索后滚动**：搜索结果 500+ 项，通过 `visible_entry_indices` 切片的滚动
4. **split view 滚动**：两个 pane 同时打开，确认两个 pane 的虚拟切片各自按 slot 同步
5. **缩略图密集目录**：`~/Pictures`（大量图片），快速滚动确认缩略图加载不阻塞滚动

设置 `FIKA_DEBUG_NAV=1` 可在终端观察 `sync_virtual_entries` 调用频率作为量化指标。

#### 焦点验证

1. **单 pane 滚动**：快速滚轮，确认无卡顿，焦点指示器无闪烁
2. **split view 焦点切换**：点击 slot 0 tile → 点击 slot 1 tile → 路径栏和状态栏正确切换
3. **split view 滚动**：在 slot 0 上滚动不意外切换到 slot 1
4. **Ctrl+滚轮缩放**：缩放正常工作，焦点保持在正确的 pane
