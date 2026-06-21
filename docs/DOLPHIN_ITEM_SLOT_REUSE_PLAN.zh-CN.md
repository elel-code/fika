> 本文是 [DOLPHIN_ITEM_SLOT_REUSE_PLAN.md](DOLPHIN_ITEM_SLOT_REUSE_PLAN.md) 的简体中文翻译。

# Dolphin 风格 Item Slot 复用计划

> 已归档的 Slint 时代 slot 复用调查。后续 UI 工作针对 SCTK/wgpu shell，应参考
> `docs/WGPU_SHELL_ROADMAP.md`、`docs/TODO.md` 和 `docs/DESIGN.md`。

## 结论

核心思想：将 Slint row identity 从文件 item identity 中解耦。Slint 只持有一组稳定 slot rows；Rust 维护 `item identity -> slot` 映射，通过 `set_row_data()` 更新 slot 内容，避免使用 `insert()`/`remove()` 作为主路径。

## 数据模型

```rust
struct ItemViewSlotState {
    slots: ModelRc<ItemViewSlotEntry>,
    slot_tokens: Vec<ItemViewSlotToken>,
    key_to_slot: HashMap<ItemViewItemKey, usize>,
    free_slots: Vec<usize>,
}
```

Slint-facing row 包含 `active`、`name`、`media_kind`、`thumbnail`、metadata 文本几何和 `x`/`y`/`text_width` 绘制字段。`active=false` 的 row 保留 delegate 实例但不绘制、不参与 hit-test。

## 更新算法

1. 构造新可见 item key 集合（`path + occurrence`）
2. 遍历旧 `key_to_slot`：key 仍存在保留 slot；不存在则置 `active=false` 并放入 `free_slots`
3. 遍历新可见 items：key 已有 slot 则更新；没有则从 `free_slots` 取或 append 新 slot
4. 对变化 slot 调用 `set_row_data(slot, entry)`
5. 更新 slot token sidecar；same key + same thumbnail token 复用旧 `Image`

## 迁移阶段（Phase 1-8）

Phase 1-2：新增 `ItemViewSlotEntry`，将 fallback Image 和 title Text loop 切换到 stable slot pool。
Phase 3-4：thumbnail 和 metadata 输入 slot 化，删除 Slint-facing sidecar model。
Phase 5-6：删除滑动 row model，将 item identity 收归 Rust 侧。`ItemViewEntry` 从 Slint export 中删除。
Phase 7-8：精简 Slint slot row 到仅剩绘制字段；分离 slot identity 与 Slint draw rows。

## 收尾状态（2026-06-08）

Phase 1-8 架构迁移已收尾。当前代码边界已对齐 Dolphin-style item instance reuse：滚动和局部增删通过 Rust slot allocator 复用 stable slot。旧 Slint-facing item/bounds/paint/thumbnail/metadata row model 路径不再保留。

## 验收标准

- 普通滚动不调用 `set_vec()`，不调用 row insert/remove
- 仍可见 item 在滚动/插入/删除/thumbnail 更新后保持同一 slot id
- 目录 watcher 插入 1 个 item 时只更新新增 item slot 和必要坐标变化
- 大跳转/目录切换复用 slot pool，允许全部 `set_row_data()` 但不 reset
- same key + same thumbnail token 时不比较、不替换 `Image`
- 现有 selection/drag/context menu/drop target 行为不变
