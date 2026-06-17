> 本文是 [ITEM_VIEW_RENDERER_DECISIONS.md](ITEM_VIEW_RENDERER_DECISIONS.md) 的简体中文翻译。

# Item View 渲染器决策

本文件记录 Dolphin 风格 item-view 迁移的渲染器选择。它刻意与实现 TODO 分开：当 model、layouter、controller 和 painter 输入保持 Dolphin-aligned 时，渲染器可以保持 GPUI 内置组件。

当前替换状态和完整过渡路线图参见 `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`。

## 决策规则

- Model 所有权不可谈判：`DirectoryModel`、`ItemId`、pane-local 布局投影、slot pool 和 retained hit testing 拥有 item-view 状态。
- 渲染器选择是 per-surface 的。GPUI 内置组件和 custom paint 在从 retained model/layout/controller 数据驱动时都是可接受的渲染器。
- custom-painted surface 在替换 GPUI surface 之前必须有运行时性能证据和行为覆盖。
- 当 GPUI 基线存在时，证据必须在相同目录、viewport、模式和 action 下将 custom painter 与该基线比较。
- 当 GPUI 拥有 Fika 尚无法通过公开 API 复现的平台合约时，GPUI 内置 surface 应保留。

## 当前 Surface 决策

| Surface | 当前渲染器 | Dolphin 风格所有者 | 决策 | 变更所需证据 |
| --- | --- | --- | --- | --- |
| Compact/Icons 基础背景和标签 | custom content-level painter | visible item snapshots, paint slots, text shape cache | 保持 custom paint | 运行时日志 snapshot 转换保持亚毫秒，static visual paint/build 在预算内 |
| Compact/Icons 缩略图图像 | custom image painter | image paint snapshots, pane-local thumbnail image cache | 保持 custom paint，image decode/cache 使用 GPUI `RetainAllImageCache` | 日志含 `[fika item-image]` + `thumb_*`，prepaint 中无同步缩略图解码 |
| Compact/Icons MIME/theme-icon 图像 | GPUI `img()` element over retained item shell | retained item slots, visible icon role/path cache | 默认使用 GPUI image elements | `/etc` 证据显示 custom image layer 暴露了首帧占位 frame；GPUI elements 保持 retained model/controller 边界 |
| Compact/Icons hover/cursor/click/menu/drop hit testing | retained viewport/custom hitboxes | viewport retained hit testing 和 `drag_drop` state | 保持 retained controller path | DnD 冒烟通过 |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state | 保持 GPUI shell 仅用于启动 | 不移除直到 GPUI 暴露公开 custom-element drag-start |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model 和 overlay geometry | 保持 GPUI overlay | rename 编辑器计划中列出的行为矩阵 |

## Perf 日志收集

参考 `/etc` autosmoke 摘要：
```text
item_view_stage_max: raw=602us icon_sync=173us queue=336us convert=426us
phase geometry-change frames=5 max_total=1635us max_visible=64
renderer_policy_frames: max_image_layer=0 max_gpui_image_element=64
```

## MIME 图标闪烁与缩放对齐

目录加载时的 MIME 图标切换参考 Dolphin 的 `retrieveData()`/`updateVisibleIcons()`/`initializeItemListWidget()`。Dolphin 不会同步解析所有 model role，但在异步 `ResolveAll` 之前给已创建可见 widget 一个 `iconName`。Fika 保持相同分离：可见通用 MIME metadata 和可见 theme-icon 路径可在有界预算内同步解析；read-ahead/offscreen metadata 和 icon 路径保持排队。

## 下一批渲染器决策

1. 保持剩余 drag-start shells 直到 GPUI API 边界变化。
2. 使用运行时日志决定当前 custom-painted surface 是否保持 custom-paint 或回退到 GPUI 渲染器。
3. 在 item-view 运行时 DnD 和 perf 门刷新之前不启动 Places custom-paint 迁移。
