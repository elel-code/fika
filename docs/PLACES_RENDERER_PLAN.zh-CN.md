> 本文是 [PLACES_RENDERER_PLAN.md](PLACES_RENDERER_PLAN.md) 的简体中文翻译。

# Places 渲染器计划

本计划仅覆盖 Places/侧栏 surface。不改变当前 item-view 渲染器决策。

## Dolphin 参考

Dolphin 的 Places 路径不是通用的 item-view 克隆：`DolphinPlacesModel` 作为薄 `KFilePlacesModel` 特化，仅添加 Trash 装饰等。`PlacesPanel` 使用 `KFilePlacesView` 作为视图，启用 drop-on-place、禁用 auto-resize、持久化图标大小等。

## 当前 Fika 边界

- Model/order/device rows：`src/ui/places/model.rs` + `src/ui/places/user/*`。主 Places 排序通过 `place_order_path` 持久化。
- Snapshot projection：`src/ui/places/projection.rs` 映射状态到 `PlaceSnapshot`。
- GPUI row shell：`src/ui/places/sidebar/row.rs` 构建行视觉、右键菜单路由、激活、drag start。
- DnD 几何：`src/ui/places/drag.rs` 拥有插入区域、重排索引、导出载荷。
- Sidebar scroll：`src/ui/places/sidebar.rs` 拥有 GPUI 滚动容器和当前 custom scrollbar。

## 提议的 Retained 设计

不分步替换 GPUI Places row renderer。目标设计是 retained Places row surface，与 file-grid 相同分离：
- `places/paint_slots.rs`：retain `PlacePaintSlot` 和 section-heading slots
- `places/interaction.rs`：retain row hitboxes
- `places/visual.rs`：从 retained snapshots 绘制行背景、active/drop 状态等
- `places/renderer_policy.rs`：日志记录渲染器策略
- `places/perf.rs`：`FIKA_PERF_PLACES_VIEW=1` 计时

## 当前基准 Perf 证据

2026-06-17 桌面会话，`/etc` 侧栏，默认 GPUI row：
- `places_sidebar max_build=631us, max_row_gpui=11, max_row_visual_layer=0`
- Autosmoke 目标/insert/clear 全部通过
- Overflow autosmoke：`max_rows=75, max_row_gpui=75, max_icon_gpui=75`

## 实验性自定义行视觉

自定义 Places row visual 路径是实验性的，在达到或超过 GPUI row 基线前保持 opt-in：`FIKA_CUSTOM_PLACES_ROWS=1`。

2026-06-17 opt-in 证据：
```text
default: max_build=631us, max_row_gpui=11, max_row_visual_layer=0
custom:  max_build=547us, max_row_gpui=0,  max_row_visual_layer=11
custom:  places_row_visual_frames=110 max_rows=1 max_prepaint=148us max_paint=921us
```

Opt-in 路径通过了非破坏性 autosmoke，但尚未默认就绪。per-row canvas 开销高（冷帧 `max_paint=921us`）。在替换默认 GPUI row 渲染器之前需收集滚动/DnD 行为证据，决定是否将 per-row canvas 聚合为 retained sidebar visual layer。

## 验收门

- 主 Places 排序跨重启持久化，动态设备刷新不重写用户排序
- 隐藏 places 和 sections 保持仅投影状态
- Drop-on-place 一致拒绝不可写/网络目标，内部重排仍允许
- 右键菜单区分空白侧栏、section header、普通 place、书签、回收站、设备行
- 运行时冒烟覆盖行激活、重排、item drop、外部路径 drop、place drag 到 pane 目录等
- 滚动/绘制证据显示相对当前 GPUI 侧栏基线无退化
