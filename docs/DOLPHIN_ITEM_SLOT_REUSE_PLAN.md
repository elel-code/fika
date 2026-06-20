# Dolphin-style Item Slot Reuse Plan

> Archived Slint-era slot-reuse investigation. Future UI work targets the
> winit/wgpu shell and should follow `docs/WGPU_SHELL_ROADMAP.md`,
> `docs/TODO.md`, and `docs/DESIGN.md`.

## Verdict

当前 Slint 足够实现 Dolphin-style item 实例复用，但实现方式不能依赖
`VecModel::insert()` / `VecModel::remove()` 作为主路径。

Slint 1.17.0 当前能力：

- `VecModel::set_row_data(row, data)` 发送 `row_changed(row)`，Repeater 会更新已有
  row instance，不会创建新的 delegate。
- `VecModel::insert()` / `remove()` 发送 `row_added` / `row_removed`，但 Repeater 会把变更点
  之后的实例标记为 dirty，因为 row index 变化了。
- `VecModel::set_vec()` / `clear()` 发送 reset，会清空 Repeater instances。

因此彻底方案是：把 Slint row identity 从文件 item identity 中解耦。Slint 只持有一组稳定
slot rows；Rust 维护 `item identity -> slot` 映射，并通过 `set_row_data()` 改 slot 的内容、
坐标、可见状态。正常滚动、目录局部插入/删除、过滤局部变化、缩略图/选择状态变化都不走
Slint row insert/remove/reset。

## Target

对齐 Dolphin 的 `KItemListView` 思路：

- Dolphin: `m_visibleItems[index] -> KItemListWidget`
- Fika target: `visible_item_key -> ItemSlotId -> Slint row`

一个 slot 是可复用的 tile instance，不等于当前目录中的第 N 个文件。文件离开当前 virtual
paint range 时，slot 进入 free pool；新文件进入时优先复用 free slot。文件仍在范围内时，
slot id 保持不变，只更新位置和绘制数据。

## Data Model

新增 Rust-side state，归属 `PaneView`：

```rust
struct ItemViewSlotState {
    slots: ModelRc<ItemViewSlotEntry>,
    slot_tokens: Vec<ItemViewSlotToken>,
    key_to_slot: HashMap<ItemViewItemKey, usize>,
    free_slots: Vec<usize>,
}

struct ItemViewItemKey {
    path: SharedString,
    occurrence: usize,
}

struct ItemViewSlotToken {
    key: Option<ItemViewItemKey>,
    absolute_index: i32,
    thumbnail_token: i32,
}

struct ItemViewSlotProjection {
    absolute_index: i32,
    path: SharedString,
    thumbnail_token: i32,
    entry: ItemViewSlotEntry,
}
```

Slint-facing row:

```slint
export struct ItemViewSlotEntry {
    active: bool,
    name: string,
    media_kind: int,
    has_thumbnail: bool,
    thumbnail: image,
    has_metadata_group: bool,
    metadata_group: string,
    has_metadata_location: bool,
    metadata_location: string,
    metadata_text_x: float,
    metadata_text_width: float,
    metadata_group_y: float,
    metadata_location_y: float,
    metadata_line_height: float,
    metadata_font_size: float,
    x: float,
    y: float,
    text_width: float,
}
```

`active == false` 的 row 保留 delegate 实例但不绘制、不参与 hit-test。池容量按当前 pane
最大可见 slice 增长，普通滚动不收缩；目录切换或内存压力路径可以显式 compact。

## Rendering Boundary

第一阶段保持当前渲染后端：

- raster base layer 继续由 Rust 生成，覆盖 selection/drop target。
- fallback icon 和 loaded thumbnail 继续由 Slint `Image` primitive 绘制，但都绑定到同一组
  稳定 `ItemViewSlotEntry` rows，不再有独立 thumbnail model。
- title `Text` 继续由 Slint 原生 `Text` 绘制。
- metadata overlay 继续用 Slint 原生 `Text`，但需要跟随 slot，而不是跟随 slice-local row。

目标不是立即把文字 raster 化，而是先把 tile instance identity 稳定下来。

## Update Algorithm

输入：新的 virtual range、对应 entries、bounds、media sources、metadata sources、selection。

步骤：

1. 构造新可见 item key 集合：`path + occurrence`，避免目录局部插入/删除导致
   absolute index 漂移时丢失 slot，同时避免重复路径合并。
2. 遍历旧 `key_to_slot`：
   - key 仍存在：保留 slot。
   - key 不存在：把 slot row 改为 `active=false`，key 移除，slot 放入 `free_slots`。
3. 遍历新可见 items：
   - key 已有 slot：更新该 slot 的 x/y/name/text/media kind/thumbnail image。
   - key 没有 slot：从 `free_slots` 取 slot；没有则 append 新 slot row。
4. 对发生变化的 slot 调用 `set_row_data(slot, entry)`。
5. 更新 slot token sidecar；selection/drop 变化只 bump pane raster revision，thumbnail token
   变化 patch 受影响的 slot row。same key + same thumbnail token 时复用旧 `Image`，不比较也不
   替换图片。

稳态滚动时，离开的 items 释放 slot，新进入的 items 复用 slot；仍可见 items 不换 slot。

## Migration Plan

### Phase 1: Add Slot Model Beside Existing Models

- 新增 `ItemViewSlotEntry` 到 `ui/models.slint`。
- 在 `PaneView` 增加 slot state，但不接入 `SplitPaneView`。
- 在 `model_update.rs` 实现 slot allocator 和 patcher。
- 单测覆盖：
  - range 向前/向后滚动时重叠 item 保持 slot id。
  - 中间插入/删除时未变 item 保持 slot id。
  - 无重叠跳转时复用旧 slots，不 reset model。

### Phase 2: Switch Title Paint Loop To Slots

- `SplitPaneView` 的 fallback `Image` 和 title `Text` loop 从旧 `paint` model 切到
  `root.item-view-slots`。
- `active=false` rows 不可见。
- hit-test 仍走 Rust bounds/layout，不从 Slint slot 反查业务身份。
- 不保留旧 `paint` 兼容层，`ItemViewPaintEntry` 已删除。

### Phase 3: Slotize Thumbnail Input

- `ItemViewTileFrameBatch` 继续提供 source-index 到 media-token 的 lookup。
- `ItemViewMediaSource` 只保留 Rust-only `slice_index + image`。
- `model_update.rs` 把 media source 附着到匹配 active slot，写入
  `has_thumbnail` / `thumbnail`；`thumbnail_token` 留在 Rust-only slot token。
- 删除 Slint-facing `ItemViewThumbnailEntry` / `PaneViewData.thumbnails` / thumbnail model sidecar。

### Phase 4: Metadata Overlay Slotization

- metadata source 保持 Rust-only `ItemViewMetadataOverlaySource`。
- `model_update.rs` 按 source `slice_index` 把 group/location 文本和 text geometry 附着到
  active `ItemViewSlotEntry` 的 metadata 字段。
- 删除旧 Slint-facing metadata struct、`PaneViewData` metadata property 和 metadata sidecar。
- show-location 场景中，稳定 item 的 group/title/location 文本更新只 patch 对应 slot row，
  不触发 tail dirty。

### Phase 5: Remove Sliding Row Models

- 已删除 `update_vec_model()` / `update_sliding_vec_model()` 的 hot-path 使用。
- `PaneView.virtual_entries` / `PaneView.virtual_bounds_entries` 改为 Rust-only `Vec`
  sidecar，用于 controller、hit-test、DnD、row token 和 raster batch。
- Slint 主绘制只消费 stable `ItemViewSlotEntry` slot pool；slot pool 仍按 slot key
  复用 row，entry/bounds 不再发布为 Slint row model。
- `paint`、thumbnail、metadata 等旧 sidecar model 已退役；thumbnail 和 metadata 都 patch
  到 active slot row。

### Phase 6: Rust-native Visible Row Identity

- `ItemViewEntry` 已从 `ui/models.slint` / `ui/app.slint` export 中删除。
- `src/main.rs` 定义 Rust-native `ItemViewEntry`，只用于 Rust-side visible slice、
  renderer frame source、thumbnail role scheduling 和 controller/hit-test/DnD token state。
- 结构测试确认 Slint `models.slint` 不再导出 `ItemViewEntry`；Slint-facing 主视图 item
  数据只剩 stable `ItemViewSlotEntry` slot row。
- 这一步进一步收窄边界：Rust 拥有 item identity，Slint 只拥有可复用 UI instance。

### Phase 7: Slim Slint Slot Rows

- `ItemViewSlotEntry.thumbnail_state` 已删除；thumbnail state 保留在 Rust-native
  `ItemViewEntry` / pane-local `ItemViewRowToken`。
- `ItemViewSlotEntry.width` 已删除；整 tile width 保留在 Rust-only
  `ItemViewItemBounds` sidecar，用于 raster、hit-test 和 layout。
- Slint slot row 只保留当前 primitive 绘制实际读取或 slot patch 必须携带的字段：
  active、fallback media kind、thumbnail image、metadata 文本几何和 x/y/text width。
- 结构测试确认 slot row 不回归 `thumbnail_state` 或整 tile `width` 字段。

### Phase 8: Split Slot Identity From Slint Draw Rows

- 新增 renderer-owned `ItemViewSlotProjection`，把 `absolute_index`、`path` 和
  `thumbnail_token` 作为 Rust-only projection/token 数据传给 `model_update.rs`。
- `ItemViewSlotToken` 现在保存 slot key、absolute index 和 thumbnail token；slot allocator、
  thumbnail image 复用、metadata/thumbnail attach 和 deferred icon-size thumbnail 保留都读
  Rust sidecar，不再从 Slint row 反查身份。
- `ItemViewSlotEntry.absolute_index`、`ItemViewSlotEntry.path` 和
  `ItemViewSlotEntry.thumbnail_token` 已删除；Slint-facing row 只剩绘制字段。
- 结构测试确认 Slint slot row 不回归 identity/token 字段；`app::model_update` 覆盖
  duplicate path key、range slide 复用、thumbnail token 复用和 metadata patch。
- 2026-06-08 验证：`cargo fmt --check`、`cargo check`、`cargo test app::model_update`、
  `cargo test app::geometry` 和全量 `cargo test` 均通过。

## Closeout Status

2026-06-08：Phase 1-8 架构迁移已收尾。旧的 Slint-facing item/bounds/paint/thumbnail/
metadata row model 路径不再作为兼容层保留；Rust 侧拥有 visible slice、item identity、
bounds、slot key 和 thumbnail token，Slint 侧只消费稳定的可复用 draw slot row。

当前代码边界已经对齐 Dolphin-style item instance reuse：滚动和局部增删通过 Rust slot
allocator 复用 stable slot，仍可见 item 保持 slot id；thumbnail 与 metadata 作为 Rust-only
source/projection 输入 patch 到 active slot；selection/drop target 保留在 Rust raster base
layer。

未在本次收尾中伪造性能数据。真实性能验收仍需要单独跑 benchmark/instrumentation，记录
`set_vec/reset`、row insert/remove、`set_row_data`、raster rebuild 计数，以及 10k 文件目录
快速滚动时 UI thread p95 refresh time。

## Acceptance Criteria

- 普通滚动不调用 `VecModel::set_vec()`。
- 普通滚动不调用 Slint row insert/remove，除非 slot pool 需要增长。
- `ui/models.slint` 不导出 `ItemViewEntry`；Rust-only visible slice 不依赖 Slint 生成类型。
- `ItemViewSlotEntry` 不携带 thumbnail state、整 tile width、path、absolute index 或
  thumbnail token；这些状态分别留在 Rust row token、slot token、projection 和 bounds
  sidecar。
- 当前 virtual range 中仍存在的 item，在滚动、局部插入、局部删除、thumbnail 更新、selection
  更新后保持同一个 slot id。
- directory watcher 在可见范围中间插入 1 个 item 时，只更新新增 item slot、必要的坐标变化
  slots 和 raster revision，不重置全部 title rows。
- 大跳转或目录切换复用现有 slot pool；允许把所有 slots `set_row_data()` 为新内容，但不 reset。
- same key + same thumbnail token 时不比较、不替换 `Image`；token 变化只 patch 对应 slot row。
- 现有 selection、drag source、context menu、drop target、thumbnail roles updater 行为不变。
- 对比基线记录：
  - virtual refresh 中 `set_vec/reset` 次数。
  - row insert/remove 次数。
  - `set_row_data` 次数。
  - raster rebuild 次数。
  - 10k 文件目录快速滚动时 UI thread p95 refresh time。

## Risks

- Slot row 顺序不再等于视觉顺序。任何依赖 Slint row index 的逻辑必须继续留在 Rust
  hit-test/controller 中。
- inactive slots 仍保留 delegate。池容量必须绑定 overscan 后的最大可见 item 数，避免无界增长。
- metadata 与 thumbnail 共用 slot row；metadata 字符串变化会 patch 该 slot，因此需要继续保证
  show-location 只对非空 group/location 生成 source。
- Rust-only projection/token 中的 `absolute_index + path` 在排序/filter 改变时能区分同一路径
  的新位置；如果要让同一路径跨排序保留 slot，可在后续把 key 策略切到 stable file
  id/path-only，但必须重新评估动画和 bounds 语义。

## Non-goals

- 不在此计划中实现 Details/Icons layout。
- 不在此计划中把 title/metadata 强制 raster 化。
- 不依赖 Slint ListView 内建虚拟化；Fika 继续使用现有 self-managed viewport。
