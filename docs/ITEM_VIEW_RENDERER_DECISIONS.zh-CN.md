> 本文是 [ITEM_VIEW_RENDERER_DECISIONS.md](ITEM_VIEW_RENDERER_DECISIONS.md) 的简体中文翻译。

# Item View 渲染器决策

> 本文是 [ITEM_VIEW_RENDERER_DECISIONS.md](ITEM_VIEW_RENDERER_DECISIONS.md) 的简体中文翻译。

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
| Compact/Icons 缩略图图像 | custom image painter | image paint snapshots, pane-local thumbnail image cache, retained thumbnail image map, thumbnail scheduler roles | 保持 custom paint 用于缩略图，image decode/cache 使用 GPUI `RetainAllImageCache`；thumbnail pending/failure 行为保持 model-driven 且可绘制后备而不改变 MIME/theme icon 策略 | 日志含 `[fika item-image]` + `thumb_*` `image_sources`，prepaint 中无同步缩略图解码 |
| Compact/Icons MIME/theme-icon 图像 | GPUI `img()` element over retained item shell | retained item slots, visible icon role/path cache, background file-icon resolve queue | 默认使用 GPUI image elements。`/etc` 证据显示 custom image layer 对所有 48 个可见 theme icon 暴露了首帧加载占位 frame；GPUI elements 保持 retained model/controller 边界而不将 theme icons 馈入 custom painter。渲染转换仍然仅使用缓存/初步图标快照，zoom 时立即按当前 layout icon size 解析 theme icon path | 仅使用 `FIKA_CUSTOM_THEME_ICONS=1` 和默认运行的配对日志重新审视。默认应显示 `gpui_image_element>0`；custom override 应是唯一有 theme-icon `theme_placeholder` 抖动的路径 |
| Compact/Icons hover/cursor/click/menu/drop hit testing | retained viewport/custom hitboxes 加 active item-drag window tracker | viewport retained hit testing 和 `drag_drop` state | 保持 retained controller path | DnD 冒烟通过内部 item、pane、Places 和外部 drop；pane self-drags 应记录 `active-item-move` |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state 加临时 shell | 保持 GPUI shell 仅用于启动 | 不移除直到 GPUI 暴露公开 custom-element drag-start 或 Fika 携带经过审计的 GPUI patch |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model 和 overlay geometry | 保持 GPUI overlay | rename 编辑器计划中列出的行为矩阵（`docs/RENAME_EDITOR_PLAN.md`） |
| Details row 背景、图标、文本单元格、Trash 列 | custom content-level painter | Details paint snapshots, row layout projection, shape cache | 保持 custom paint | 运行时 Details perf 和 DnD 冒烟证据必须保持最新 |
| Details click/menu/navigation/hover/cursor/drop hit testing | retained row hit testing/controller state 加 active item-drag window tracker | viewport retained hit testing | 保持 retained controller path | painter 变更后 DnD 冒烟必须通过 |
| Details drag start | GPUI `Div::on_drag` row shell | retained drag payload state | 保持 GPUI shell | 与 Compact/Icons drag start 相同门 |
| Places rows 和 sidebar scrollbar | 默认 custom chrome layer 加 GPUI text/icons/event shells；`gpui` fallback 和 `FIKA_CUSTOM_PLACES_ROWS=1` full-text 基准路径仍可用 | `places` model/projection 和 `drag_drop` state | 保持 Dolphin 对齐的 chrome 拆分为默认。文本/图标或事件传递在 retained/static cache 与 hitbox 达到或超过 GPUI 前不得移出 GPUI | 默认日志必须通过 `--expect-custom-row-chrome-policy`，并显示 `text_gpui=rows`、`visual_kind=chrome`、无 row shape-cache 日志，且聚合 `[fika places-row-visual]` rows 匹配策略行数。GPUI fallback 必须通过 `--expect-current-gpui-policy`；full text 继续由 `--expect-custom-row-visual-policy` 约束 |

## Perf 日志收集

参考 `/etc` autosmoke 摘要以对比未来的退化：

```text
item_view_stage_max: raw=602us icon_sync=173us queue=336us convert=426us
phase geometry-change frames=5 max_total=1635us max_visible=64
renderer_policy_frames: max_image_layer=0 max_gpui_image_element=64
```

item-view autosmoke marker surface 现在由 `src/ui/file_grid/autosmoke.rs` 拥有，而非 `src/main.rs`。该模块拥有稳定场景标签以及 start/complete、zoom-action 和 scroll-action marker 格式化；app root 仅将预定的 zoom 和 scroll 变更应用到 pane state。证据：`/tmp/fika-item-view-autosmoke-marker-module.log` 通过了与 `/etc` zoom/scroll 证据相同的分析器门。

该日志中剩余的可见成本是静态文本/背景绘制：`static_visual max_prepaint=5794us`、`max_paint=12043us`，仅当新条目进入 retained visible set 时出现形状缓存 miss。将未来工作视为静态视觉 painter/cache 工作，而非 MIME/theme icon 渲染器工作。

对于绘制层调查，比较 `[fika static-item-visual]` 和 `[fika item-image]` 的 prepaint 计数与可见条目计数，而非原始 read-ahead 工作计数。Read-ahead 属于 scheduler projection 和 retained caches；它不应向当前绘制 prepass 添加 image-cache 加载或文本形状。分析器的 `image_sources` 行分离了缩略图首次就绪 GPUI 解码结果（`thumb_decoded`）、已就绪缓存加载（`thumb_loaded`）、retained 回退到最后真实图像路径（`thumb_retained`）和可见后备路径（`thumb_fallback`）。Theme `image_sources` 计数器仅在 `FIKA_CUSTOM_THEME_ICONS=1` 将 MIME/theme icons 路由通过 custom image layer 做 A/B 证据时出现。

## MIME 图标闪烁与缩放对齐

对于 MIME 图标闪烁调查，对比 Dolphin 的 `KStandardItemListWidget::updatePixmap()` 和 `pixmapForIcon()`：Dolphin 保持 widget-local `m_pixmap` 并按 icon name/size 使用 `QPixmapCache`，因此已加载的真实图标不会在相同图标资源刷新时被标记替换。Fika 的默认路径通过将 MIME/theme icons 保持在由 retained item snapshots 和当前尺寸 icon path 驱动的 GPUI `img()` elements 上来保持该行为。如果使用 `FIKA_CUSTOM_THEME_ICONS=1`，custom image painter 必须通过按 MIME/theme `iconName`、icon size 和 scale keyed 的 retained images 保持相同行为。缩略图 retention 保持按精确缩略图路径键控。Fika 不会通过读取和解码 SVG 在 GPUI prepaint 中复制 Dolphin 的同步 `QIcon::pixmap()`；GPUI image loading 保持为解码路径。中性无标记占位符仅可作为 custom-theme 首帧加载/失败后备，而非已加载真实图标的退化。详细 retained image-cache 设计见 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md`；基础实现位于 `src/ui/icons/image_cache.rs`，默认 MIME/theme renderer 仍保持 GPUI `img()`，直到配对运行时证据通过。

对于缩放调查，对比 `KFileItemListView::triggerIconSizeUpdate()` 和 `updateIconSize()`：Dolphin 立即更新条目几何但暂停 `KFileItemModelRolesUpdater`，在 `LongInterval`（300ms）后重新启动 preview/visible-range role work。Dolphin 的普通 `iconName` pixmap 路径不同：`pixmapForIcon()` 使用 widget 的当前 style-option icon size。因此 Fika 在每个缩放步中按当前 layout icon size 解析 MIME/theme icon path，且不得为 theme icons 安排延迟的第二次 icon-size 提交。

对于目录加载时的 MIME 图标切换，对比 `KFileItemModel::retrieveData()`、`KFileItemModelRolesUpdater::updateVisibleIcons()` 和 `KFileItemListView::initializeItemListWidget()`：Dolphin 不会同步解析所有 model role，但在异步 `ResolveAll` pass 遍历其余部分之前确实给已创建的可见 widget 一个 `iconName`。Fika 应保持相同分离：可见通用 MIME metadata 和可见 theme-icon path 可在有界预算内同步解析；read-ahead/offscreen metadata 和 icon path 保持排队。这复制了 Dolphin 的 `iconName` 加 `pixmapForIcon()` 路径，而不将 read-ahead icon-theme 扫描移入渲染转换。图像解码本身保持在 scheduler/image-cache 路径上；默认 theme icons 通过 GPUI `img()` 解码，而 custom-theme A/B 绘制层可保留先前相同 `iconName` 图像但不得在 prepaint 期间同步解码 theme icon 文件。

## 下一批渲染器决策

1. 保持剩余 drag-start shells 直到 GPUI API 边界变化。不要将 GPUI per-element `on_drag_move` 用作 pane self-drag 悬停的真实来源；active item-drag window tracker 拥有该路径。
2. 使用运行时日志决定当前 custom-painted surface 是否保持 custom-paint 或回退到 GPUI 渲染器叠加在 retained model 上。
3. 在 item-view 运行时 DnD 和 perf 门刷新之前不启动 Places custom-paint 迁移。
