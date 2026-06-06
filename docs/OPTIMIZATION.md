# Fika 性能优化

本文档记录 Fika 的性能改进方向，涵盖主栏列优先横向虚拟化和焦点（focus）切换两大系统。
每个条目包含问题描述、涉及代码、改进方案和预估收益。

---

## 当前架构

### 滚动数据流

```
Slint SplitPaneView 自管 viewport-x
  ├─ TouchArea.scroll-event → set-viewport-x(raw)
  ├─ 自管 scrollbar drag/click → set-viewport-x(raw)
  └─ pane-local width / rows-per-column 变化 → relayout-visible-slice()
       └─ root.view_changed() → PaneRouting.view-changed(slot)
            └─ Rust: PaneViewSyncScheduler 同步重建当前 visible slice
                 └─ sync_virtual_entries_for_slot()  [src/main.rs]
                      ├─ MainItemViewLayout::from_ui_for_pane_width_with_text_lines()
                      │    └─ 只对 focused slot 扣搜索栏高度，并使用 pane-local 文本行数
                      ├─ VirtualViewSnapshotInput               [virtual_view.rs]
                      ├─ prepare_virtual_view_snapshot_update()  [virtual_view.rs, 后台线程]
                      │    ├─ compact_item_view_layout() / virtual_plan() [geometry.rs]
                      │    ├─ should_rebuild_virtual_cache()
                      │    ├─ snapshot_entries_range()
                      │    └─ annotate_snapshot_location_groups()
                      ├─ decorate_entries_with_cached_thumbnails_for_pane()
                      ├─ prioritize_thumbnail_entries()         [thumbnail_pipeline.rs]
                      ├─ schedule_visible_thumbnails()          (异步)
                      ├─ set pane-local paint/highlight/media/metadata → Slint；bounds 保留在 Rust sidecar
                      │    └─ ItemViewEntry stays Rust-side for controller/hit-test/DnD/token state
                      └─ sync_pane_view_ui()                   [split_view.rs]
```

### 当前主视图结构 (ui/split_pane.slint)

```
Rectangle viewport shell (clip: true)
  ├─ full-viewport DragArea + TouchArea (wheel / item click / double click / context / blank click / rectangle selection)
  ├─ Rust-projected item-view layout metrics (rows/cell/column/content/scroll extent)
  ├─ slice-layer (x = padding + column_offset + virtual_start_column * column_width - viewport_x; tile_index = virtual_start_row + slice_index)
  │    ├─ for highlight[index] in highlights: Rectangle primitive
  │    │    └─ 只绘制选中背景，slice_index 使用同一套列优先坐标
  │    ├─ single drag-target-slice-index Rectangle primitive
  │    │    └─ 只绘制当前 drop target，不再逐 item 比较 path
  │    ├─ for paint[index] in paint: Image primitive
  │    │    └─ 绘制 pane-level fallback；local tile bounds 来自 Rust projected paint sidecar
  │    ├─ for media[index] in media: Image primitive
  │    │    └─ 只叠加已加载 thumbnail；slice_index 使用同一套列优先坐标
  │    ├─ for paint[index] in paint: Text primitive
  │    │    └─ 普通 compact 标题使用 Dolphin-style whole-tile text frame
  │    └─ optional metadata overlay (show-location only)
  │         └─ for metadata[index] in metadata: Text primitive；pane-local 稀疏模型只包含非空 group/location
  ├─ selection rectangle overlay
  └─ self-managed horizontal scrollbar
```

主文件视图使用 Dolphin compact 模式语义：物理滚动轴固定为 X，条目按 `index % rows_per_column` 先填满一列，再按 `index / rows_per_column` 进入下一列。compact item 尺寸沿用 Dolphin 的公式：`itemWidth = padding * 4 + iconSize + fontHeight * 5`，`itemHeight = padding * 2 + max(iconSize, textLines * lineSpacing)`，列间 margin 为 `8px`。普通目录使用 1 行标题，Trash 和递归搜索使用 pane-local 的 3 行 group/title/location 布局；每个 pane 独立计算自身行数、可见范围和 viewport。Dolphin 对照点是 `KStandardItemListView::setItemLayout()` 在 CompactLayout 下设置 `Qt::Horizontal`，`KItemListViewLayouter` 通过转置逻辑把纵向流映射成物理横向滚动，`KStandardItemListWidget::updateCompactLayoutTextCache()` 预先计算 compact 文本位置和最大宽度，并把 compact text frame 关联到整个 item 高度；文字本身使用居中的 line position。Fika 同样在 Rust render plan 中投影非零 `title_y/title_line_height/text_width`；普通标题的 Rust-projected `Text` frame 覆盖整 tile 高度并由 Slint vertical alignment 居中，避免大 zoom 下单行 frame 被裁剪导致 name 消失；带 group/location 的标题行继续使用 Rust 投影的 line frame，额外 group/location 行通过 pane-local sparse metadata overlay 绘制。基础 `ItemViewEntry` 热 row 只留在 Rust `PaneView` 内部，携带业务身份、目录标记、缩略图状态和 media token，供 controller、hit-test、DnD 和 thumbnail token sidecar 使用；它不再作为 `PaneViewData` 字段传给 Slint。Slint 基础 icon/name loop 消费 pane-local `ItemViewPaintEntry`，成功 thumbnail media 由 pane-local `ItemViewMediaEntry` 稀疏 overlay 承载，selection highlight、thumbnail media 和 metadata overlay 都带有 Rust 投影的 item-local bounds，避免 overlay loop 再用 `root.bounds[slice_index]` 跨模型查表。drop target 也复用 `paint[drag-target-slice-index]` 的几何，因此完整 `ItemViewBoundsEntry` 模型不再进入 `PaneViewData`。media/text/title 几何以及通用 folder/file fallback image 都挂在 pane-level `PaneViewData` / `PaneView` cache 上，不再写入业务 row。metadata 由 Rust render plan 直接投影成 `ItemViewMetadataEntry` 稀疏 rows。`SplitPaneView` 只消费 pane-level 几何和 paint/overlay sidecar，不再在可见 title loop 内按 `group/location` 做 per-item 判断。

### 现有优化

| 措施 | 位置 | 效果 |
|------|------|------|
| 虚拟化：Slint 只接收可见范围条目 | `virtual_view.rs` | 大目录不实例化全部 tile |
| 缓存命中免重建：`should_rebuild_virtual_model` | `virtual_view.rs:180` | 同范围内滚动零模型更新 |
| 缩略图优先可见列 | `thumbnail_pipeline.rs:40` | 减少首屏缩略图延迟 |
| 自管 viewport clamp/round | `split_pane.slint` | 避免 ScrollView/Flickable viewport 回写和子像素漂移触发同步 |
| 普通滚轮不重复请求焦点 | `split_pane.slint:78` | 减少 FFI 调用 |
| 自管滚动条消费 Rust item-view layout metrics | `split_view.rs` / `split_pane.slint` | `rows_per_column`、column width/offset、content width、scroll max 与虚拟切片使用同一 Rust layouter |
| 每 pane latest-only virtual prepare | `pane.rs` / `main.rs` | 快速滚动时每个 pane 只保留一个后台 prepare，等待队列只保存最新请求 |
| Rust item-view hit-test | `item_view.rs` | click/activation/context/DnD/drop target 命中不再散落在 Slint tile 或 transfer 几何代码中 |
| Press-pinned drag source | `split_pane.slint` / `item_view.rs` | 主视图 `DragArea.data` 只发布 blank-suppress / pending sentinel；真实 drag payload 从 Rust pane-local press-time `drag_source` 延迟解析，hover/move 不再更新 data-transfer 绑定，也不再反复触发 Rust hit-test |
| Rust item-view render plan | `item_view_renderer.rs` / `split_view.rs` / `split_pane.slint` | 主视图行列/滚动 metrics、tile x/y/width/text width、media/text rect、尺寸/字体 token 和 folder/file fallback image 不再由 Slint 每项公式或 hot row data 承载；可见基础 Image/Text loop 直接消费 Rust projected `ItemViewPaintEntry` |
| Sparse thumbnail media overlay | `thumbnail_pipeline.rs` / `model_update.rs` / `models.slint` | 基础 `ItemViewEntry` 不再携带 `Image`；已加载 thumbnail 只发布 pane-local `ItemViewMediaEntry` rows，并通过 Rust-only media token sidecar 复用 pane-local model，zoom/cached relayout 时可独立裁剪和平移；overlay row 自带 item x/y，Slint 不再查 `root.bounds[media.slice_index]` |
| Sparse metadata overlay | `item_view.rs` / `model_update.rs` / `models.slint` | 基础 `ItemViewEntry` 不再携带 group/location 字符串和 metadata 几何；show-location 只发布 Rust 预投影的非空 `ItemViewMetadataEntry` rows，且 row 自带 item x/y，Slint metadata loop 不再查 bounds sidecar |
| Coalesced settings save | `settings_save.rs` / `settings.rs` | zoom/sidebar/split ratio 等交互触发的 `persist_ui_state()` 只抓取最新设置快照并延迟后台写入；窗口关闭和目录导航保留同步 latest 保存，避免连续 zoom 时 UI 线程同步写配置文件 |
| Coalesced icon zoom thumbnails | `app.slint` / `main.rs` | `icon_zoom_level` 变化立即刷新可见 pane layout，但不立刻调度缩略图；300ms 后按当前可见切片 latest-only 调度预览，贴近 Dolphin 的 icon-size updater 分层 |
| Cached icon zoom relayout + zoom range hint | `geometry.rs` / `virtual_view.rs` / `model_update.rs` / `main.rs` | 目录/滚动重建时为各 icon zoom 档的真实可见区预留 pane-local 热窗口；zoom 后只要求当前 slice 覆盖新真实可见区，未按新 `rows_per_column` 对齐的窗口也通过 `virtual_start_column + virtual_start_row` 直接复用，避免首次 zoom 裁剪 `VecModel` rows。compact 文本字号保持 Dolphin-style 稳定，只让 icon/media size 参与 zoom |

---

## 改进方向

### P0 — 边界滚动提前退出（旧 ScrollView 路径）

**历史问题**：旧 `ScrollView` 路径下，当 viewport 已到达 0 或 `scroll-max-x` 边界时，每次 `changed viewport-x` 回调仍执行完整的 `stable-viewport-x` 计算和 Rust 侧 `sync_virtual_entries`。

**涉及代码**：
- `ui/split_pane.slint:147-155` — `changed viewport-x` 回调
- `src/app/split_view.rs:567` — `sync_virtual_entries`（通过 `PaneRouting.view-changed` 间接调用）

**旧路径改进**：在 Slint 侧的 `changed viewport-x` 回调中，夹紧后立即比较新旧 viewport-x：如果相同则直接返回，不调用 `view_changed()`。

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

当前 `SplitPaneView` 已删除 `ScrollView`、`changed viewport-x`、`viewport-offset` 和 epsilon 写回路径，改为 `set-viewport-x(raw)` 统一夹紧和取整。

**收益**：消除所有边界滚动时的无效 FFI 往返和 Rust 计算。

**难度**：低。单文件、纯 Slint 修改。

---

### P1 — 同步 visible slice 重建（替代旧 Coalesce）

**历史问题**：快速滚轮滚动时，Slint 每次 viewport 更新都会触发 `view_changed`，事件会进入 `sync_virtual_entries_for_slot` 调用链。旧 8ms coalesce timer 可以降低计算频率，但在大目录末尾 fullscreen/resize 和快速滚动后会把 visible slice 恢复推迟到 timer 之后，出现空白段需要后续手动拖动滚动条才能恢复。

**涉及代码**：
- `src/main.rs` — `sync_virtual_entries_for_slot`
- `ui/split_pane.slint` — `set-viewport-x(raw)` / `relayout-visible-slice()`

**实际实现**（✅ 已替换）：当前路径参考 Dolphin `KItemListView::setScrollOffset()`：viewport 变化后立刻进入 `sync_pane_viewport_for_slot()`，同步夹紧并重建当前 visible slice；`PaneViewSyncScheduler` 只保留 re-entrancy guard，防止 Slint 回调重入，不再延迟滚动布局。

```
滚动事件到达
  ├─ Slint 本地 live viewport-x 立即更新
  └─ Rust 同步 sync_virtual_entries_for_slot_with_count(..., immediate=true)
       ├─ 缓存覆盖当前 visible range：只更新 AppState viewport，必要时发布 clamp
       └─ 缓存不覆盖：同步准备并写入当前 PaneViewData + pane-local paint/overlay models
```

关键设计点：
- viewport-x 的 Slint 本地状态仍由 pane surface 持有，普通滚动不再每步把 viewport 反发布到 pane row。
- layout/fullscreen/rows-per-column 变化走 immediate layout rebuild，不等待 timer，也不需要手动拖动滚动条恢复。
- `PaneViewData` 承载 viewport、layout metrics、空状态等主视图热数据；Slint-facing view data 不再携带 `ItemViewEntry` 或完整 bounds model。可见业务 row 保留在 Rust `PaneView.virtual_entries`，用于 controller/hit-test/DnD/token state；Slint 只接收 pane-local paint、highlight、media、metadata sidecar。`PaneSlotData` 只保留地址栏、搜索、状态栏、chooser 等 pane chrome 冷数据。

`PaneViewSyncScheduler` 不再使用 `slint::Timer` 聚合滚动事件；它只持有 weak UI/state/bridge 并用 `syncing: Cell<bool>` 跳过递归重入。

**收益**：牺牲旧 timer 合并带来的少量计算节流，换取滚动、末尾 resize/fullscreen、pane-local layout 变化时的同步可见内容恢复，避免出现必须手动拖动滚动条才能恢复的空白段。

**验证**：源码守卫测试确认 `PaneViewSyncScheduler` 不再使用 `TimerMode::SingleShot` / pending slot 队列，`sync_pane_viewport_for_slot()` 走 `sync_virtual_entries_for_slot_with_count(..., immediate=true)`。

---

### P1 — `sync_pane_slots_ui` 去重

**问题**：`pane_slot_data()` 曾把 pane chrome、viewport、entries、layout metrics 和空状态混在一行 `PaneSlotData` 里。虚拟切片、selection 或 viewport 变化都会让 pane chrome row 跟着重建，且 `entries` 嵌套在 pane row struct 里会让 Slint delegate 刷新语义变得不稳定。

**涉及代码**：
- `src/app/split_view.rs:41-62` — `sync_pane_slots_ui`
- `src/app/split_view.rs:72-152` — `pane_slot_data`

**实际实现**（✅ 已完成）：
- `PaneSlotData` 只保留地址栏、搜索、状态栏、chooser、external edit 等 pane chrome 冷数据。
- `PaneViewData` 承载 viewport、entry count、layout metrics、空状态、drop/content interactive，以及 pane-local `paint` / `highlights` / `media` / `metadata` models；`ItemViewBoundsEntry` 保留为 Rust-side projection/update sidecar。
- 可见业务 `ItemViewEntry` 切片只保留在 Rust `PaneView.virtual_entries`；Slint-facing `PaneViewData` 只下发基础绘制和 overlay sidecar，不再通过 `pane_slot_0_*` / `pane_slot_1_*` 固定 sidecar 走顶层属性，也不重新引入主 pane 概念。
- `sync_pane_slots_ui()` 先 snapshot visible slots，然后分别同步 `pane_slots` 和 `pane_views`；slot shape 未变时使用 row-level `set_row_data`。
- `set_pane_viewport_ui()` 只写 `AppState.pane.view.viewport_x`，再通过 `sync_pane_view_viewport_ui()` patch 当前 `PaneViewData.viewport_x` 字段；view row 缺失时才回退到 `sync_pane_view_ui()`。

**收益**：虚拟切片、viewport clamp、目录 view-state 恢复、selection 更新不再重建 pane chrome row；paint/highlight/media/metadata 都是 pane-local view row 数据，bounds 只保留在 Rust sidecar，避免全局 slot 0/1 sidecar 重新引入“主 pane”概念。

**验证**：源码守卫测试确认 `PaneViewData` 不携带 `entries` 或 `bounds`，但携带 `paint`、`highlights`、`media`、`metadata`，`PaneSlotSurface` 直接绑定 `root.view.*`，并且 `ui/app.slint` / `split_view.rs` 不再包含 `pane_slot_0_*` / `pane_slot_1_*` sidecar 或 `sync_pane_entries_ui()`。

---

### P2 — Slint Model 增量更新

**问题**：早期虚拟切片每次更新都会创建全新 `VecModel`。当向右滚动 1 列时，新旧虚拟范围重叠很高，但 Slint 仍需要重新绑定可见 tile primitive。

**涉及代码**：
- `src/app/model_update.rs` — `update_pane_item_view_entries_model`
- `src/app/pane.rs` — `PaneView.virtual_entries` / `virtual_entry_tokens`
- `src/app/split_view.rs` — `pane_view_data`
- `ui/split_pane.slint` — `for paint[index] in root.paint: Image` / `Text`

**实际实现**（✅ 已完成）：`update_pane_item_view_entries_model()` 保留 pane-local `VecModel<ItemViewEntry>`，根据 old/new virtual start 处理前后滑动：
- 无重叠或空模型：`VecModel::set_vec`
- 向前/向后滑动：`remove` / `insert` 修正前缀
- 重叠行：比较 Rust sidecar `ItemViewRowToken`，只有 token 不同才 `set_row_data`
- 尾部长度差：`remove` / `extend`

`ItemViewRowToken` 覆盖 name/path/is_dir/selection/thumbnail_state/media token 等轻量身份字段，避免为了比较复用而从 Slint `VecModel` 读取 row；它现在归属 `src/app/model_update.rs`，与 virtual row reuse / selection sidecar/media/paint sidecar 更新放在同一层，而不是放在 input/controller/hit-test 的 `item_view.rs`。split pane snapshot 会复制到独立 `VecModel` 和 sidecar，避免两个 pane 共享同一个可见 row model。选中态只保存在 token sidecar 中，并投影成 pane-local cached `ItemViewHighlightEntry` 稀疏 model，由 Slint 单独绘制 highlight overlay；selection 或 virtual row 变化时复用同一个 highlight `VecModel` 做 `set_vec`，不再由 `split_view` 每次同步临时 collect 新 model。基础 `Image` / `Text` loop 和 `ItemViewEntry` row data 不再携带 per-item selected 状态、成功 thumbnail image、group/location metadata 或 per-row 几何；基础 icon/name 绘制消费由 row token + Rust bounds 投影出的 pane-local `ItemViewPaintEntry` sidecar。成功 thumbnail image 从缓存投影成独立的 pane-local sparse `ItemViewMediaEntry` model，show-location metadata 从 `FileEntry` snapshot 经 Rust render plan 投影成独立的 pane-local sparse `ItemViewMetadataEntry` model。可见项的 x/y/width/text-width 继续由 pane-local `ItemViewBoundsEntry` sidecar 在 Rust 侧维护并投影到 Slint-facing paint/highlight/media/metadata rows；Slint 基础 loop 不再重复计算 `virtual-start-row + index` 和 modulo row，也不再接收完整 bounds model。highlight/media/metadata sparse rows 携带自身 item geometry，drop target 复用 `paint[drag-target-slice-index]`，所有 overlay 都不再查 `root.bounds[slice_index]`。Rust bounds sidecar 和 Slint paint sidecar 都复用自己的 pane-local `VecModel`，普通滚动和 cached zoom relayout 只做 row 级滑动/更新，不再每次替换整个 model。media sidecar 使用 Rust-only `slice_index + media_token` sidecar 判断 sparse thumbnail row 是否变更，复用已有 pane-local `VecModel<ItemViewMediaEntry>`，避免为了模型复用去比较或读取 `Image`。metadata sidecar 是稀疏 overlay，一个 item 可能对应 0/1/2 行，因此不套用 item-range 滑动算法；非空 metadata 更新复用已有 pane-local `VecModel` 做 row 级比较/更新，空 metadata 仍回到空模型语义，避免普通目录保留 stale metadata 文本。

**收益**：连续滚动时重叠 virtual rows 不再因为 slice-local 坐标、pane-level 几何或 pane-level fallback image 变化被重发；selection 变化只刷新 token sidecar/sparse highlight，真实 thumbnail 变化只刷新 sparse media overlay；Rust bounds sidecar、Slint paint/media 和非空 metadata model 保持稳定，且 Slint 少持有一份完整可见 bounds model，避免 Rust-projected 几何/图像/文本 overlay 下沉后又通过整模型替换刺激 Slint 可见 primitive 绑定。

**验证**：`app::model_update` 测试覆盖前滑、后滑、无重叠、sidecar 修复、media token 比较、selection sidecar 更新、bounds/paint sidecar 滑动复用、media sidecar token 驱动复用、metadata sidecar 稀疏 row 级复用/清空、cached relayout 复用 bounds/paint/media model 和裁剪 sparse media overlay；源码守卫测试防止重叠 row reuse 重新读取 `VecModel::row_data()`，防止 `ItemViewEntry.selected` / `ItemViewEntry.media` 回归，并确认 Slint-facing `PaneViewData` 不再携带 `bounds`。

---

### P2 — Dolphin-style zoom thumbnail 合并

**问题**：`zoom-main-in/out()` 修改 `icon_zoom_level` 后，旧路径通过 `changed icon_zoom_level => pane_layout_changed()` 立即重建所有可见 pane 的 virtual slice，并在同一路径里调度缩略图。连续 Ctrl+wheel zoom 会把多次 layout、fallback media、thumbnail scheduling 和 Slint model 更新叠在同一段交互里。即使缩略图调度已延迟，每个目录第一次 zoom 仍可能因为当前虚拟 slice 只覆盖旧 zoom 的 overscan、无法覆盖新 zoom 的可见区，而在 UI 线程同步 snapshot rebuild。

**Dolphin 对照**：Dolphin 的 `KItemListView::setStyleOption()` 会立即更新可见 widget 的 style、size hint cache 和 layouter；`KItemListViewLayouter::updateVisibleIndexes()` 在 row offsets 上用二分找 first/last visible index；`KFileItemListView::triggerIconSizeUpdate()` 只对 icon-size 引发的昂贵 role/preview 更新使用 `LongInterval` timer 合并。Compact layout 的 item hints 使用 `styleOption.fontMetrics`，icon zoom 改 `iconSize`，文本字体本身不随每档 icon zoom 改变。Fika 采用同样的分层：滚动/resize/zoom layout 不延迟，只把 zoom 后的缩略图调度延后，并让 compact 文本字号保持稳定；固定 compact grid 不需要 Dolphin 的 row-offset 二分，但需要提前保留相邻 zoom 档会用到的可见 index。

**涉及代码**：
- `ui/app.slint` — `changed icon_zoom_level => icon_zoom_layout_changed()`
- `src/main.rs` — `PaneLayoutSyncScheduler`
- `src/app/virtual_view.rs` — `VirtualViewSnapshotInput.range_hint`

**实际实现**（✅ 已完成）：`icon_zoom_level` 不再调用普通 `pane_layout_changed()`：
- 目录/滚动重建时，`icon_zoom_range_hint()` 计算 `MIN_ICON_ZOOM_LEVEL..=MAX_ICON_ZOOM_LEVEL` 每个档位的真实可见 range，并把 union 作为 `VirtualViewSnapshotInput.range_hint` 传给后台 snapshot。每个候选 range 会先按对应 zoom 档的 `rows_per_column` 对齐，再由 `prepare_virtual_view_snapshot_update()` 与当前 zoom 的 overscan range 合并，降低新目录第一次 zoom 因目标行数对齐失败掉出热窗口的概率。
- `icon_zoom_layout_changed()` 立即调用 `sync_visible_pane_icon_zoom_layouts()`；`try_relayout_cached_pane_icon_zoom_layout()` 只要求当前 virtual slice 覆盖新 zoom 的真实可见 range。`cached_zoom_relayout_range()` 不再要求旧 slice 起点按新 `rows_per_column` 对齐，也不再裁剪到对齐列边界；Slint 侧用派生的 `virtual_start_column + virtual_start_row` 绘制任意起点的窗口，保留 selection/highlight sidecar，不重新 snapshot 当前目录。
- fast path 覆盖不了新真实可见 range，或 show-location metadata 仍在热路径里时，不再在 Ctrl+wheel/input 事件里同步 snapshot rebuild；`prepare_pane_icon_zoom_layout_for_slot()` 只发起后台 virtual prepare，并在结果返回后更新当前 pane。这样 cache miss 仍保留正确性，但不把过滤、clone、render-plan 装饰压进 zoom 输入路径。
- `CompactItemVisualMetrics` 只让 media/icon size 随 zoom 改变，title/metadata font metrics 保持稳定，贴近 Dolphin 的 `styleOption.fontMetrics` 用法，减少 Slint 首次 zoom 时的 Text 字体冷启动。
- `PaneView` 的 fallback file/folder media cache 按 pane/theme 固定为一套 72px 源图；目录/滚动 snapshot、theme refresh 和 zoom fast path 只确保当前主题 cache 已就绪。zoom 只改变 pane-level media 目标几何，不再把新的 folder/file fallback `Image` source 下发给 Slint，贴近 Dolphin 的 pixmap cache 行为并降低 `/etc` 这类 fallback-heavy 目录的首次 zoom 卡顿。
- `PaneLayoutSyncScheduler` 使用 `TimerMode::SingleShot` 以 300ms 合并连续 zoom 后的缩略图调度；timer flush 时从当前可见 virtual slice 调用 `schedule_visible_thumbnails_for_visible_panes()`。该路径从 pane-local `ItemViewRowToken` sidecar 派生轻量 `ThumbnailScheduleEntry`，不再从 Slint `VecModel<ItemViewEntry>` 读取 row；临时 path 使用 `SharedString`，只有真正入队的缩略图任务才转成 owned `PathBuf`。
- 普通非缩略图候选文件会在首次 virtual slice 装饰时写入 `THUMBNAIL_STATE_NOT_CANDIDATE`，该状态随 `ItemViewRowToken` 复用；zoom 后的延迟缩略图调度看到该 token 会直接跳过，不再重复构造 `PathBuf`、判断扩展名或查询 thumbnailer。`is_thumbnail_candidate()` 也会先用扩展名 MIME 映射预筛，`.conf`、无扩展等文件不会初始化 XDG thumbnailer registry。
- thumbnail 调度按当前 size 的 media token 判断已加载状态；旧 zoom size 的 thumbnail 不会阻塞新 size 的延迟任务。
- 普通 `pane_layout_changed()` 仍调用 `sync_now()`，会停止 pending zoom thumbnail timer 并立即刷新；窗口大小、sidebar、split ratio、pane 宽度变化不等待 timer。

**收益**：连续 zoom 时保留即时视觉反馈，避免每个 zoom step 都启动缩略图解码/预览更新；普通目录第一次 zoom 更容易直接命中预热热窗口，未按新行数对齐的热窗口也只做 pane-level layout 更新和 active fallback cache 切换，不再裁剪 `VecModel`；`/etc` 这类非图片目录的首次 zoom 不再因为缩略图候选探测而冷启动 thumbnailer registry；cache miss 也不会同步 clone/装饰虚拟切片；稳定字体减少新 zoom 档位的 Text layout 冷启动；同时保留 resize/fullscreen/末尾 viewport 修复的即时路径。

**验证**：源码守卫测试确认 `PaneViewSyncScheduler` 的滚动路径仍无 timer，`icon_zoom_layout_changed` 同步刷新 layout，优先尝试 cached relayout，cache miss 进入后台 prepare，缩略图调度才走独立 coalesced timer。`virtual_view` 测试覆盖 `range_hint` 的对齐扩展；`main` 测试覆盖 cached zoom relayout 只要求真实可见 range、已对齐和未对齐热窗口都不裁剪，以及 zoom hint 按目标 rows 对齐；`pane` 测试覆盖 fallback media 多槽复用和 split snapshot 继承热 cache；`model_update` 测试覆盖 cached relayout 复用 `VecModel`、保留 selection/highlight 并裁剪 sparse media overlay；`thumbnail_pipeline` 测试覆盖旧 size thumbnail 不阻塞新 size 调度、row-token 调度不读取 Slint row model，以及非候选 row/token 在 zoom 后直接跳过；`fs::thumbnails` 测试覆盖未知扩展不进入 thumbnailer lookup。

---

### P2 — 交互设置保存合并

**问题**：`persist_ui_state()` 路径虽然不再重复刷新虚拟网格，但旧实现仍在 UI 线程同步执行 `save_settings()`，连续 Ctrl+wheel zoom 或快速拖动 split/sidebar 时会把文件系统写入叠到布局/交互路径上。

**涉及代码**：
- `ui/app.slint` — `zoom-main-in/out()`、split/sidebar 拖动结束时调用 `persist_ui_state()`
- `src/main.rs` — `ui.on_persist_ui_state(...)`
- `src/app/settings_save.rs` — latest-only coalesced save scheduler
- `src/config/settings.rs` — `save_settings()`

**实际实现**（✅ 已完成）：交互触发的 `persist_ui_state()` 改为 `SettingsSaveScheduler`：
- UI 线程只读取当前 `AppSettings` 快照并放入 latest-only pending slot。
- `TimerMode::SingleShot` 以 120ms 合并多次请求；flush 后通过 `tokio::spawn_blocking` 写 `settings.tsv`。
- 全局 generation 防止旧后台写覆盖新的状态。
- 窗口关闭和目录导航仍走同步 latest 保存，确保最终状态落盘。

**收益**：连续 zoom 时少掉一次 UI 线程同步文件写入；持久化变成低优先级后台工作，窗口关闭和导航仍保留最终同步保存。

**验证**：源码守卫测试确认 `persist_ui_state` 不调用 `save_current_settings()` / `save_settings()`，而是调度 coalesced scheduler；关闭窗口路径仍执行同步 final save。

---

### P2 — 缩略图批量写入

**问题**：缩略图异步生成完成后，通过回调逐个写入 Slint model。每个缩略图写入触发一次 `Image` 属性变更和潜在的 tile 重绘。如果 20 张缩略图在 100ms 内到达，可能触发 20 次属性评估。

**涉及代码**：
- `src/main.rs` — `schedule_visible_thumbnails` 及相关回调
- `src/app/thumbnail_pipeline.rs` — `decorate_entries_with_cached_thumbnails_for_pane`

**实际实现**（✅ 已完成）：缩略图完成事件写入 `ThumbnailFlushScheduler` 批次缓冲，16ms flush 一次。flush 时只更新共享 thumbnail cache / pending map，然后刷新当前可见 virtual slice；`AsyncEvent::ThumbnailLoaded` 不再逐张直接触发 Slint row 写入。

```
缩略图完成 → 写入 batch buffer
每 16ms tick → 一次性将所有新缩略图写入 Slint model
```

**收益**：减少缩略图密集到达时段的重绘触发频率。对大目录快速滚动时的新缩略图加载尤为有效。

**验证**：`thumbnail_results_are_batched_before_virtual_refresh` 守卫 flush 路径；thumbnail pipeline 测试覆盖 pending key、成功/失败 cache 和 visible-first 调度。

---

### P3 — UI 线程计算后移

**问题**：整个虚拟视图计算（条目克隆、缩略图缓存查找、`filtered_entries_range`）在 UI 线程执行，持有 `RefCell<AppState>` 借用。在 120Hz+ 显示器或条目数量大时可能触碰帧预算。

**涉及代码**：
- `src/main.rs:3354-3412` — `sync_virtual_entries` / `sync_virtual_entries_with_count`
- `src/app/virtual_view.rs` — `prepare_virtual_view_snapshot_update`

**改进**：将计算阶段和写入阶段分离：

- **后台线程**：`prepare_virtual_view_snapshot_update` 的纯函数计算（输入为 `VirtualViewSnapshotInput`，输出为 `VirtualViewSnapshotUpdate`）
- **UI 线程**：只做 Slint 属性写入（`set_virtual_entries`、`set_entry_count`）

这需要将 `AppState` 的访问模式改为线程安全（`Arc<Mutex<>>` 或读时快照），复杂度高。

**收益**：彻底解除 UI 线程的计算负担，在高刷显示器上保证帧率。

**实际实现**（✅ 已完成）：采用快照模式而非锁迁移——`PaneEntrySnapshot`（不含 `Image` 的轻量结构体，`Arc<[PaneEntrySnapshot]>` 零拷贝共享）+ `VirtualViewSnapshotInput`（完全 owned 的纯函数输入）。UI 线程按 pane slot 构建 snapshot 输入后通过 `tokio::spawn_blocking` 在后台执行 `prepare_virtual_view_snapshot_update`（纯函数），结果通过 `AsyncEvent::VirtualViewPrepared` 回传。`virtual_generation` 独立于 `load_generation` 做 staleness 检测，`apply_virtual_view_result` 先在 `borrow_mut` 块内写 state 再 drop 后写 Slint model，无 RefCell 跨线程风险。所有可见 pane 都走同一条 slot 驱动虚拟视图管线，旧的 preview/副 pane 专用路径已删除。

---

### P3 — 可见 tile primitive 简化

**问题**：主视图每个可见 tile 仍包含图标、文件名、位置文本、选中状态、drop target 等子节点。当有 80-120 个可见 tile 时，每个都有自己的属性绑定评估树。

**涉及代码**：
- `ui/split_pane.slint` — tile primitive 循环

**实际实现**（✅ 已完成）：
1. 将 zoom 派生的展示 token（tile 高度、padding、spacing、缩略图大小、字体大小）迁到 Rust `ItemViewRenderMetrics`，随虚拟切片装饰为 `ItemViewEntry` 字段，避免每个 tile 独立计算；Dolphin compact visual metrics 的 media/font/line/cell/row 公式现在由 `src/app/item_view_metrics.rs` 单点持有，layout 和 render plan 都只消费同一组 metrics
2. 删除独立 tile 组件边界，把可见 tile primitive 内联到 `SplitPaneView` 的 slice layer，避免继续维护旧 path-based item 组件
3. 将 media/icon rect、text rect、group/title/location y 坐标和 line height 继续迁到 Rust render plan，`SplitPaneView` 的可见 item loop 不再为每项使用 `HorizontalLayout` / `VerticalLayout`
4. 普通 compact item-view 的 tile height 与 row height 同源；普通标题按 Dolphin compact text cache 的分层方式由 Rust 投影整 tile 高度作为 text frame，带 group/location 的 metadata title 行继续复用 Rust 投影的 line frame 并通过 sparse overlay 绘制额外文本
5. 基础 compact loop 无条件绘制 icon/media 和 `item.name`，避免普通目录标题依赖 metadata 绘制路径
6. `show-location` metadata overlay 已改成 pane-local `ItemViewMetadataEntry` 稀疏模型，只下发非空 group/location 行；`SplitPaneView` 不再对普通空 metadata item 实例化隐藏 `Text`
7. pane-level 颜色 token 仍由 `SplitPaneView` 下发；后续若切换到自绘 renderer，再把颜色/字体/media icon cache 一并纳入 renderer state

**收益**：减少大量 tile 时的属性绑定评估开销。

**下一步**：剩余成本主要是可见 tile primitive loop 本身。下一轮应做 Dolphin-style renderer/reuse spike，验证 text/tile frame 缓存或 `SharedPixelBuffer`/`Image` 自绘 tile frame，再决定是否替换当前可见 primitive loop。

### P0-next — Dolphin-style 自管主视图

**问题**：Phase 1-6 和 V0-V4 已经把大量无效同步、后台计算、缩略图 flush、选择 FFI、重复绑定降到较低水平，但 `/etc` 这种基本没有图片的大目录仍然会在滚动、末尾 fullscreen/resize 后出现明显卡顿或空白恢复延迟。第一阶段已经把 viewport source of truth 从 `ScrollView`/`Flickable` 改为 `SplitPaneView` 自管，但剩余瓶颈仍在 Slint 主视图可见 primitive 树。

**Dolphin 对照**：
- `dolphin/src/kitemviews/kfileitemmodel.*` — 文件模型
- `dolphin/src/kitemviews/private/kitemlistviewlayouter.*` — visible index / item rect 布局
- `dolphin/src/kitemviews/kstandarditemlistview.*` — CompactLayout 选择横向滚动
- `dolphin/src/kitemviews/kitemlistview.*` — viewport、可见 widget 管理、drop indicator
- `dolphin/src/kitemviews/kitemlistcontroller.*` — selection、activation、drag 控制
- `dolphin/src/kitemviews/kstandarditemlistwidget.*` — item 绘制与复用，compact text frame 覆盖整个 widget 高度

Dolphin 的关键不是某个滚动控件，而是 model、layouter、view/controller、item rendering/reuse 分层。Fika 要接近 Dolphin 的滚动上限，已经先移除通用 `ScrollView` 作为 viewport 底座，并删除独立 tile 组件边界；下一步要把主视图核心继续转成 Rust 自管 item view / renderer。

**Slint 底座**：
- `Rectangle { clip: true; }`：只做 viewport 壳、背景和裁剪，不承担滚动模型
- `TouchArea`：统一接管滚轮、pointer move、click、double click、右键、框选、hover
- `DropArea`：覆盖 viewport，`can-drop` / `dropped` 坐标交给 Rust hit-test 决定目标 item 或 blank area
- `DragArea`：优先验证 viewport-level drag source；如果 Slint 需要更具体的 press target，再做最小数量的 drag layer，而不是每个文件一个完整 tile
- `Image` / `SharedPixelBuffer`：作为后续自绘路径，把 Rust 绘制好的 tile/icon/text frame 交给 Slint 显示

**目标架构**：

```
Slint: Rectangle viewport + input/DnD overlays
  └─ Rust: ItemViewController
       ├─ model snapshot / visible index
       ├─ layouter: scroll_offset -> visible indexes + item rects
       ├─ hit-test: pointer/drop point -> item/blank/gap
       ├─ selection/hover/drag/drop state
       └─ renderer:
            ├─ v1: 只输出可见 primitive/model，移除 ScrollView/Flickable 主导权
            └─ v2: SharedPixelBuffer/Image 自绘 tile frame
```

**验收标准**：
- 主文件 item-view 核心不再依赖 `ScrollView` / `Flickable` 的 viewport 状态作为 source of truth
- 滚动位置、可见范围、item rect、hit-test、selection、drop target 都由 Rust 自管并可测试
- 不再以完整 per-item Slint 组件树作为核心渲染路径；第一版可以保留少量可见 primitive，最终目标是自绘 frame 或可复用 item layer
- DnD 仍使用 Slint 原生 `DragArea` / `DropArea` 和 `data-transfer`，但目标解析完全走 Rust hit-test
- `/etc`、`/usr/lib`、split view 双 pane、末尾 fullscreen/resize、快速滚轮、拖放、框选、右键菜单都要单独验证

**收益预期**：理论上可以接近 Dolphin 的架构上限，因为性能边界从 Slint 的每 tile 组件树和通用滚动容器，转移到 Rust 侧的布局、命中测试、缓存和绘制策略。是否真正媲美 Dolphin 需要用 spike 和实测确认，尤其是文字/icon 绘制缓存、DnD 启动层、滚动条手感和 HiDPI 下的 frame 更新成本。

**当前进度**：
1. 主文件区已直接替换为 `Rectangle { clip: true; } + DragArea + TouchArea + self-managed scrollbar`，删除 `ScrollView` / `Flickable` viewport 写回。
2. `src/app/item_view.rs` 已开始承载 pane-local layout、drop hit-test、矩形选择候选范围和 tile 命中几何，transfer/DnD、selection、activation 与 context menu 不再私有持有主视图几何。
3. Pane-local `ItemViewInputState` 已接管空白区 press/move/release/cancel 决策；Slint 只负责报告事件和绘制选择框 overlay，不再直接提交 `select_rect` 路由。
4. Item press、double-click activation、item context menu 与主视图内部 drag source 已迁到 `SplitPaneView` 的 pane-level input controller；可见 tile primitive 不再拥有 `TouchArea`、`DragArea`、滚轮、双击、右键或 path-based DnD 数据源。主视图 `DragArea.data` 现在只区分 blank suppress sentinel 和 item pending sentinel；item drag payload 从 Rust pane-local `ItemViewInputState.drag_source` 延迟解析，普通 hover/move 不再更新 data-transfer 绑定或重新进入 Rust hit-test。
5. 虚拟切片仍输出 Rust-side `virtual_entries`，但它只留在 `PaneView` 中服务 controller、hit-test、DnD 和 token sidecar；主视图热字段已通过 `PaneViewData` 接收 Rust item-view layouter metrics（`rows_per_column`、cell size、padding、content width、virtual slice width、scroll max）、media/text/title 几何、pane-level folder/file fallback image、viewport 和空状态。Slint-facing `PaneViewData` 只下发 paint、highlights、media、metadata pane-local sidecar，避免 nested model in row，也不再下发完整 bounds model；`PaneSlotData` 只保留 pane chrome/status/search/chooser 冷数据。普通 item 使用 Dolphin-style compact 横向布局，图标在左、名字在右，基础 icon/name loop 直接消费 Rust-projected `ItemViewPaintEntry` paint rows；带 group/location 的递归搜索结果通过 sparse metadata overlay 叠加 group/location 文本。item `x/y/width/text_width` 由 Rust-side `ItemViewBoundsEntry` sidecar 维护并投影到 Slint-facing rows，不写入 `ItemViewEntry` hot row；Slint 不再在主视图内计算 content width、scroll extent、zoom 派生公式、title metadata 分支或每个 loop 的 row modulo。
6. 独立 tile 组件文件已删除，可见 tile primitive 内联在 `SplitPaneView` 的 slice layer 中，减少一层 Slint 组件边界，并把后续 renderer/reuse 替换点集中到一个主视图文件。
7. 可见 tile 内部的 media/text 布局也已转为 `item_view_renderer.rs` 输出；`SplitPaneView` 只绘制 `Image` / `Text` primitive，不再对每个文件项运行 Slint layout 容器。普通 item 标题绘制已按 Dolphin compact text cache 的分层方式由 Rust 投影为整 tile 高度 text frame，避免最大 zoom 下单行 title frame 被裁剪造成 name 消失；递归搜索带位置元数据的 item 继续通过 sparse metadata model 使用 Rust 投影的多行 text frame。基础 title loop 不再读取 `item.group` / `item.location` 来决定自身 frame。
8. 文件/目录 fallback media 已从 Slint `FolderGlyph` 组件迁到 `item_view_renderer.rs` 并继续下沉为 pane-level cache：通用 folder/file fallback image 由 `PaneViewData.item_view_folder_media` / `item_view_file_media` 下发，主视图 base media loop 始终绘制 pane-level fallback，成功 thumbnail image 通过 pane-local sparse `ItemViewMediaEntry` overlay 覆盖。fallback file/folder media cache 挂在 pane-local `PaneView` 上，按 theme 复用固定 72px 源图；目录/滚动 snapshot、theme refresh 和 zoom fast path 只确保当前主题 cache 已就绪，split snapshot 也继承已预热 cache。zoom 时只更新 media rect/row/cell 目标几何，不再切换 fallback `Image` source，为后续 renderer state 继续收敛到 Rust 侧铺路。
9. `ItemViewEntry.media_token` 作为真实 thumbnail 更新令牌进入可见 row；`src/app/model_update.rs` 同时维护 pane-local `ItemViewRowToken` sidecar、`ItemViewBoundsEntry` bounds sidecar、`ItemViewPaintEntry` paint sidecar、Rust-only media token sidecar 和 `ItemViewMediaEntry` sparse overlay。虚拟切片滑动的重叠 row 先比较 sidecar token，token 相同就不读取 `VecModel::row_data()`；bounds/paint sidecar 复用各自 pane-local `VecModel` 做滑动更新，media overlay 复用同一个 pane-local `VecModel` 并只用 `slice_index + media_token` 判断 thumbnail row 更新，cached relayout 会同步裁剪 bounds、paint 和 sparse media overlay。split pane 快照也会复制到独立 `VecModel`、cloned row-token sidecar、cloned paint sidecar、cloned media sidecar 和 cloned media token sidecar，避免两个 pane 共享同一个可见 row 模型。
10. show-location metadata overlay 已从 per-item 透明 `Rectangle` wrapper 和 `visible:` 过滤，收敛成 pane-local `ItemViewMetadataEntry` 稀疏模型；普通空 metadata row 不再进入 Slint overlay loop，非空 metadata overlay 会复用同一个 pane-local `VecModel` 做稀疏 row 级更新，空 metadata 仍清回空模型，split snapshot 也会复制独立 metadata model，保持 pane 独立。
11. Dolphin compact visual metrics 已收敛到 `src/app/item_view_metrics.rs`：`geometry.rs` 负责 viewport/visible range/layout，`item_view.rs` 负责 input/controller/hit-test，`item_view_renderer.rs` 负责 render plan、metadata projection 和 fallback media。它们不再分别维护 zoom/media/font/line/cell/row 公式；fallback 源图和 zoom 几何已经解耦，为后续 renderer cache 或 tile-frame 自绘提供更稳定的输入边界。
12. DnD 仍保留 Slint 原生 `data-transfer` 路径，drag payload 和 drop target 解析都继续向 Rust hit-test 收敛。

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
| 滚轮滚动 | `split_pane.slint` `set-viewport-x(raw)` | 每帧 |
| 自管滚动条拖动/点击 | `split_pane.slint` `set-viewport-x(raw)` | 每帧 |
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

**问题**：`route-pane-focus(slot)` 曾无条件执行 focus + `pane_focus(slot)`，即使 slot 已经是当前焦点。旧滚动路径中快速滚动可在 `pan-horizontal` 和 `changed viewport-x` 两处重复触发，每次都做无效的 `FocusScope` 重算和 FFI 往返。

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

### F1 — 滚动事件中移除冗余的 `focus_requested`（旧 ScrollView 路径）

**历史问题**：旧 `pan-horizontal` 和 `changed viewport-x` 中每次都调用 `focus_requested()`。滚动的 pane 必然是用户正在交互的 pane，焦点从首次点击/滚轮时就已经设好。配合 F0 的提前退出后这些调用的成本已大幅降低，但仍是两次属性比较 + 分支。

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

当前自管 viewport 路径中普通滚动只走 `set-viewport-x(raw)`，不再从滚动路径请求焦点。`handle-scroll` 中 Ctrl+滚轮的 `focus_requested()` 仍是合理的——Ctrl+滚轮切换缩放模式，确实需要声明焦点。

**涉及代码**：
- `ui/split_pane.slint:67` — `pan-horizontal` 末尾的 `focus_requested()`
- `ui/split_pane.slint:153` — `changed viewport-x` 回调中的 `focus_requested()`

**安全性分析**：pane 内容现在通过 `PaneSlotSurface`/`PaneSlot` 统一路由，滚动和 viewport 变化都携带 slot 并写回对应 pane 的 `DirectoryViewState`。普通滚动不需要额外声明焦点；需要焦点语义的路径（点击激活、Ctrl+滚轮缩放、右键菜单、拖放）仍显式走 slot-aware focus/route 回调。

**收益**：每帧省两次属性比较 + 分支判断（配合 F0 后为两次整数比较）。

**难度**：低。旧路径为两行删除；当前路径已经删除 `changed viewport-x` 回调和 `viewport-offset` 写回。

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

**实际实现**（✅ 已完成）：焦点切换走 `sync_focus_navigation_ui(ui, state, previous_slot)`，不调用完整 `sync_navigation_ui`，也不在 `sync_focused_ui` 内部触发 `sync_pane_slots_ui`。该路径只写 focused pane 的全局导航/选择属性，然后用 `sync_pane_slot_ui` 增量刷新旧 slot 和新 focused slot 两行，使旧 pane 的 focused 派生字段降级、新 pane 升级，同时避免重扫整个 pane slot model。

```rust
fn sync_focus_navigation_ui(ui, state, previous_slot) {
    let (focused_slot, focused_dir, focused_selection) = { ... };
    sync_focused_ui(ui, focused_slot, &focused_dir, &focused_selection);
    sync_pane_slot_ui(ui, state, previous_slot);
    if previous_slot != focused_slot {
        sync_pane_slot_ui(ui, state, focused_slot);
    }
}
```

**收益**：焦点切换时减少 ~8 次无效 Slint setter 调用及潜在的下游绑定重算，并跳过完整 pane slots 同步；只更新焦点切换实际影响的旧/新两行。

**验证**：源码守卫测试确认 `focus_pane_slot` 只调用 `sync_focus_navigation_ui(ui, state, previous_slot)`，且 `sync_focus_navigation_ui` 不调用 `sync_pane_slots_ui`，只调用旧/new slot 的 `sync_pane_slot_ui`。

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

## 虚拟 Item-View 内部优化

以下优化针对 Dolphin compact 横向 item-view 的虚拟切片计算链路本身（`virtual_view.rs`、`geometry.rs`、`selection.rs`），聚焦单次计算内部的微优化，与 Phase 1-4（控制何时重建模型）互补。

---

### V0 — `is_selected` FFI 移出可见 tile loop

**问题**（`ui/split_pane.slint:253`）：

```slint
selected: root.selection-revision >= 0 && root.is_selected(item.path);
```

100 个可见 tile → 每次选择变化时 100 次 `PaneRouting.is-selected(slot, path)` FFI 调用。`selection-revision >= 0` 的守卫只在首次选择前生效。

**涉及代码**：
- `ui/split_pane.slint:253` — tile 的 `selected` 绑定
- `PaneRouting.is-selected` — 全局回调注册
- `src/app/selection.rs` — `PaneSelection` 查找逻辑

**实际实现**（✅ 已完成）：`ItemViewEntry` 不再带 `selected` 字段。Rust 侧把当前 pane 的 selection 只写入 pane-local `ItemViewRowToken` sidecar，再由 `split_view.rs` 投影成稀疏 `ItemViewHighlightEntry` model：

```slint
for highlight[index] in root.highlights: Rectangle { ... }
```

选择变化时只更新 token sidecar 和 pane-local cached sparse highlight model，不再对 `VecModel<ItemViewEntry>` 做 row-data 写回，也不再由 `split_view` 每次 UI 同步临时构造 highlight model。旧的 `selection_revision` 全局属性和 `PaneViewData.selection_revision` 热字段已删除，选择变化不再为了触发 tile 绑定而污染 pane view row；后台虚拟视图结果应用时会用当前 pane selection 初始化 token sidecar，旧异步结果不会把高亮写进基础 row。渲染路径上的 `PaneRouting.is-selected` / `FilePane.is_selected` 回调已删除；item 右键命中后是否需要先选中由 Rust 坐标 helper 按 pane selection 状态直接判断。

**收益**：每次选择变化（点击、框选、Ctrl+A）省 80-120 次 FFI 调用，并避免 selection 变化重写可见 `ItemViewEntry` rows。

**难度**：已完成。源码守卫测试覆盖 sparse highlight overlay，并防止恢复 tile 级 `is_selected` 回调、`selection-revision`、`selected: item.selected` 或 `ItemViewEntry.selected`。

---

### V1 — `virtual_entry_range` 双重计算融合

**问题**（`geometry.rs:186-203`）：

```rust
let range = virtual_entry_range(..., overscan_columns);  // 第一次
let visible_range = virtual_entry_range(..., 0);           // 第二次
```

两次调用重复计算相同的 column math。带 overscan 的范围天然包含不带 overscan 的范围。

**涉及代码**：
- `src/app/geometry.rs` — `CompactItemViewLayout::virtual_plan`
- `src/app/geometry.rs` — `virtual_entry_ranges`

**实际实现**（✅ 已完成）：`CompactItemViewLayout::virtual_plan` 现在调用内部 `virtual_entry_ranges`，一次计算 `first_visible_column` / `visible_end_column`，同时返回 overscan range 和 visible range。旧的单 range 包装函数已删除，避免非测试构建保留死代码。

```rust
fn virtual_entry_ranges(..., overscan_columns) -> (Range<usize>, Range<usize>) {
    let first_visible_column = ...;
    let visible_end_column = ...;
    let overscan_range = entry_range_for_columns(...);
    let visible_range = entry_range_for_columns(first_visible_column, visible_end_column, ...);
    (overscan_range, visible_range)
}
```

**收益**：每次 `virtual_plan` 省一次除法/floor/ceil 链。

**难度**：已完成。现有 compact layout / virtual view 测试覆盖边界、overscan 和 viewport clamp 行为。

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

**实际实现**（✅ 已完成）：

```rust
indices[range.start..end]
    .iter()
    .map(|&index| state.panes.focused().entries[index].clone())
    .collect()
```

该直接索引路径不只用于 `filtered_entries_range`，也用于可见路径收集、单项读取、可见 range 迭代、虚拟视图后台 snapshot，以及缩略图可见范围判断。`PaneState::set_entries` / `clear_entries` 会清空旧的 `visible_entry_indices` 和 `visible_location_groups`，目录加载路径使用 `set_entries_with_location_state(entries, has_locations)` 复用已有 location 统计，避免为了失效缓存额外扫描大目录。

**安全性**：`visible_entry_indices` 只由 `rebuild_visible_entry_index` 从当前 pane entries 枚举生成；条目集替换或清空时会立即失效索引缓存，保留 query/filter 条件等待下一次过滤重建。因此直接索引不会使用过期 index。

**收益**：有搜索/过滤时每条目省 `Option` 解包。

**验证**：`filtered_entries_range_clones_only_requested_filtered_window` / `visible_entry_index_drives_virtual_range_without_rescanning_filters` 覆盖可见索引驱动的虚拟范围；`pane_set_entries_invalidates_visible_index_cache_without_clearing_filters` 覆盖条目替换时的索引缓存失效。

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

**相关缓存约束**（✅ 已完成）：`src/fs/thumbnails.rs` 读取 freedesktop 缩略图缓存时不再只看缓存文件 mtime；复用前会验证 PNG 文本元数据中的 `Thumb::URI` 和 `Thumb::MTime`，缺失、不匹配或读取失败时删除旧缓存并重新生成，避免把外部/陈旧缓存误当成当前文件的有效缩略图。

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

**实际实现**（✅ 已完成）：`sync_places` 现在优先复用当前 `VecModel<PlaceEntry>`，按行比较后用 `set_row_data` 更新 rename/reorder 变化，用 `remove` 删除尾部多余行，并用 `extend` 追加新增 Places。只有当前模型不是 `VecModel<PlaceEntry>` 时才回退到新建 model。

**收益**：Places add/remove/rename/reorder 不再重建整个 Slint model，避免 sidebar row 组件不必要重建。虽然 Places 列表通常较小，但这让 Places 与文件虚拟列表的 model 更新策略保持一致。

**验证**：`places_model_updates_rows_without_replacing_vec_model` 覆盖 rename/reorder、remove、append 场景，并确认 `ModelRc` 保持同一个 `VecModel`。

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

**实际实现**（✅ 已完成）：右键菜单回调中将 `refresh_clipboard_availability()` 替换为 Slint 回调 `sync_clipboard_state()`；Rust 侧 `ui.on_sync_clipboard_state` 只调用 `sync_clipboard_ui(&ui, &state)`，不读取 Wayland clipboard。Ctrl+V / Paste 也不再同步刷新缓存，而是通过 `ClipboardPasteLoaded` 异步事件先导入当前桌面剪贴板，结果回到 UI 线程后再入队传输。启动/菜单入口的后台 availability refresh 现在带 `clipboard_refresh_pending` single-flight 保护，已有读取未返回时不会重复 spawn Wayland clipboard 查询。

右键 service-menu 发现同样保持异步和 generation guarded：打开 item/blank 菜单时先清空旧的 Slint action model，再在后台扫描 desktop/service-menu 文件并只应用仍匹配当前路径快照的结果，避免同步读取 desktop metadata 阻塞弹窗打开。这条路径现在由 `src/app/context_service_menu.rs` 独立拥有，`main.rs` 只做 slot 路由和 async event 分发。`X-KDE-Submenu` 分组现在同步为根菜单 submenu 父行，hover 时再按组名写入当前 child action model，并复用 `MenuLifecycleController` / `ChildSubmenuLayer`；菜单几何只统计根 action/submenu 行和一个配置入口行，避免 Slint 侧遍历模型或把分组子项误算进根菜单高度。用户启用/禁用策略在 Rust 侧先过滤可见 action model，同时保留全量匹配快照给配置弹窗，禁用项不会从配置界面丢失。`Icon=` 元数据在后台发现阶段通过 `src/desktop/icons.rs` 解析为图标文件路径，UI 同步阶段只加载已解析路径，避免右键菜单弹出后再同步扫描 icon theme 目录。

**收益**：消除每次右键的 clipboard 协议查询延迟。

**难度**：已完成。源码守卫测试限制右键菜单函数只能走 `sync_clipboard_state()`，并限制 Ctrl+V 只能直接请求 Paste；`file_clipboard.rs` 测试覆盖 Paste 不同步读取 Wayland clipboard 以及 availability refresh single-flight。

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

### S4 — Devices mounter item 合并入口

**问题**：Devices 发现路径已经同时包含 mountinfo/root-scan 和 UDisks2，但合并逻辑直接围绕 Slint `DeviceEntry` 运行。这样会把“后端来源/能力”和“侧栏投影字段”绑在一起，后续如果加入 GVfs/network 类后端，容易复制一套合并与诊断统计逻辑。

**涉及代码**：
- `src/fs/devices.rs` — mountinfo/root-scan、UDisks2 discovery、duplicate merge、diagnostics stats

**实际实现**（✅ 已完成）：mountinfo/root-scan 和 UDisks2 discovery 现在先生成 backend-tagged `MounterDevice`，在内部 mounter item 层完成去重、能力合并、kind 升级和 merge stats 统计，最后再投影成现有 Slint `DeviceEntry`。本地可移动设备操作仍走 UDisks2 system bus；UI model 字段不变。

**收益**：把后端发现/合并路径和 sidebar Slint model 投影分开，后续 GVfs/network 后端可以接入同一个 merge/statistics/projection 路径，而不是重新实现一套 Devices sidebar 合并逻辑。

**验证**：`mounter_device_merge_keeps_backend_semantics_before_sidebar_projection` 覆盖内部 mounter item 合并；现有 `devices` 测试继续覆盖 UDisks2 parsing、mountinfo fallback、diagnostics 和 sidebar status projection。

---

## 潜在问题排查

以下是与性能相关的已知限制或需要排查的点：

### 自管 scrollbar 几何

`viewport-content-width`、`virtual-slice-width` 和 `scroll-max-x` 现在由 Rust `pane_slot_item_view_metrics()` 通过同一套 `MainItemViewLayout` / `compact_item_view_layout()` 计算，并随 `PaneViewData` 下发给 `SplitPaneView`。Slint 只消费这些 metrics 做 scrollbar、viewport clamp 和切片偏移，不再自己根据 `entry-count` / `rows-per-column` / `zoom-level` 重算主视图 layouter。

已处理的布局恢复问题：`SplitPaneView` 现在在 pane-local `width` 或 `rows-per-column` 变化时主动夹紧 `viewport-x` 并请求虚拟切片刷新。这样全屏/布局变化发生在大目录末尾时，不再依赖后续手动拖动滚动条来触发旧切片重建。

### TouchArea 覆盖范围

```slint
TouchArea {
    width: parent.width;
    height: parent.height;
```

当前 `TouchArea` / `DragArea` 只覆盖可见 viewport，不随目录内容宽度增长。大目录的主要 UI 成本仍是可见 tile primitive 树和后续 renderer/reuse 策略。

空白区和 item 输入现在都通过 pane-local item-view controller 决策。`SplitPaneView` 报告 press/move/release/cancel 或 item 坐标，Rust 按同一套 item-view geometry 决定选择、激活、右键菜单、drag payload、清空选择或提交 rectangle selection；这一步减少了 Slint 内部选择逻辑分支，但还没有移除 visible primitive 循环的渲染成本。

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
| **Phase 2** | P1 旧 8ms 合并窗口，后续替换为同步 visible slice | 2-3h | ✅ 已替换 |
| **Phase 3** | P2 Slint model 增量更新 (`model_update.rs`) | 3-4h | ✅ 已完成 |
| **Phase 4** | P2 缩略图批量写入 (Flush 16ms) | 2-3h | ✅ 已完成 |
| **Phase 5** | P3 UI 线程计算后移 | 1-2d | ✅ 已完成 |
| **Phase 6** | P3 tile primitive 简化 | 1-2h | ✅ 已完成 |

**Phase 1-6 实现要点**：
- **Phase 1**: `changed viewport-x` 提前退出 + `sync_pane_slots_ui` row_data 脏检查 + 新增 `sync_pane_slot_ui` 单 slot 增量；主视图热字段现已拆到 `PaneViewData`
- **Phase 2**: 旧 `PaneViewSyncScheduler` 8ms timer 已删除；当前 scheduler 同步调用 `sync_pane_viewport_for_slot` 并只保留重入保护，layout/viewport 变化立即重建当前 visible slice
- **Phase 3**: 新模块 `src/app/model_update.rs` — `VecModel::downcast_ref` 增量更新，支持前/后滑动 + `set_row_data` 逐行脏检查；当前重叠 row 复用通过 pane-local `ItemViewRowToken` sidecar 判断，避免为了比较而读取并克隆 Slint row data
- **Phase 4**: `ThumbnailFlushScheduler` (16ms) — 缩略图结果入队批量写入，`AsyncEvent::ThumbnailLoaded` 不再逐张触发 `sync_virtual_entries`
- **Phase 5**: `PaneEntrySnapshot`（不含 `Image` 的轻量快照, `Arc` 零拷贝共享）+ `VirtualViewSnapshotInput`（完全 owned 的纯函数输入）— 虚拟视图的条目过滤/切片/clone/location 标注全部在 `tokio::spawn_blocking` 中完成，UI 线程只做 generation staleness 检查 + Slint 模型写入 + 缩略图缓存装饰。`virtual_generation` 独立于 `load_generation`，目录切换时自动推进。`apply_virtual_view_result` 先在 `borrow_mut` 内写 state 再 drop 后写 Slint，避免 RefCell 跨线程风险。所有可见 pane 走同一条 slot-aware 虚拟视图管线。
- **Phase 6**: zoom 派生尺寸/字体 token、tile size、media/text rect 和 group/title/location line 坐标迁到 Rust item-view render plan；slice-local tile x/y 改由 Rust-projected bounds/paint sidecar 提供，避免滑动窗口时重叠条目因局部坐标变化触发 row-data 写入；可见 tile primitive 内联到 `SplitPaneView`，且不再为每个 item 使用 Slint layout 容器

**审查发现的后继微优化**：
- **cleanup-1**: 旧 state-based 虚拟视图更新 helper 和测试路径已删除，虚拟视图测试改为覆盖当前 snapshot 管线
- **f2-note**: `sync_focus_navigation_ui` 已跳过完整 `sync_pane_slots_ui`，纯焦点切换只增量刷新旧 slot 和新 focused slot 两行
- **borrow-note**: pane status、pane slot model、transfer menu 准备/完成、FileAction 完成、file open 完成、privileged operation 完成、external edit 启动/完成、Undo 注册/启动/完成状态等先在 `AppState` 借用内生成快照/summary，再释放借用后写 Slint row/model、权限弹窗、external edit 控件、Undo 按钮状态或清理旧 overwrite backup，避免 async operation status 更新期间被 Slint 回调重入触发 `RefCell` 借用 panic

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
- **F2**: 新增 `sync_focus_navigation_ui` — 与 `sync_navigation_ui` 相比跳过左栏 8 setter 和 `set_split_view_open`，只读取 focused pane 数据并写入 `sync_focused_ui`；旧 pane 和新 focused pane 的 row data 通过 `sync_pane_slot_ui` 增量刷新，不再执行完整 `sync_pane_slots_ui`

### 虚拟 Item-View 内部优化

| 阶段 | 改进 | 预计工作量 | 状态 |
|------|------|-----------|------|
| **Phase V0** | `is_selected` FFI 移到 token sidecar / sparse highlight | 1h | ✅ 已完成 |
| **Phase V1** | `virtual_entry_range` 双重计算融合 | 15min | ✅ 已完成 |
| **Phase V2** | `filtered_entries_range` filter_map→map | 5min | ✅ 已完成 |
| **Phase V3** | 旧 preview 路径删除 | 5min | ✅ 已完成/不适用 |
| **Phase V4** | `annotate_visible_location_groups` 缓存 | 30min | ✅ 已完成 |

### 跨系统通用优化

| 阶段 | 改进 | 预计工作量 | 状态 |
|------|------|-----------|------|
| **Phase S0** | 缩略图后台 spawn 批量化 | 15min | ✅ 已完成 |
| **Phase S1** | Places 模型增量更新 | 10min | ✅ 已完成 |
| **Phase S2** | 右键菜单跳过剪贴板读取 | 10min | ✅ 已完成 |
| **Phase S3** | `file-operation-shortcuts-blocked` 归约 | 5min | ✅ 已完成 |
| **Phase S4** | Devices mounter item 合并入口 | 30min | ✅ 已完成 |

**综合建议**：滚动 Phase 1-6、焦点 F0-F2/G0、V0-V4、S0-S4 已完成。后续性能工作应优先来自实测卡顿或新的 Dolphin/COSMIC 对照发现，而不是继续堆叠低收益微优化。

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
