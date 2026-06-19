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
| Compact/Icons MIME/theme-icon 图像 | 默认 full custom image layer，`FIKA_GPUI_THEME_ICONS=1` 保留为 GPUI `img()` baseline | retained item slots、visible icon role/path cache、app-level `ThemeIconImageReadiness`、pane image layer、background file-icon resolve queue | 保持 full custom image layer 为默认。它仍使用 GPUI 高效的 `RetainAllImageCache -> RenderImage -> paint_image` 底层路径，但普通 pane 渲染器不再保留逐条目的 GPUI `img()` 子元素。`FIKA_GPUI_THEME_ICONS=1` 是同场景非自绘 image baseline | 默认路径日志必须保持 `gpui_image_element=0`、`theme_placeholder=0`、visible `theme_decoded=0`；修改 image layer 性能时必须和 `FIKA_GPUI_THEME_ICONS=1` 做同场景对比 |
| Compact/Icons hover/cursor/click/menu/drop hit testing | retained viewport/custom hitboxes 加 active item-drag window tracker | viewport retained hit testing 和 `drag_drop` state | 保持 retained controller path。目录 item drop hover 由 retained window-position hit testing 解析，不再由 per-directory GPUI drag-move shell 解析。 | DnD 冒烟通过内部 item、pane、Places 和外部 drop；pane self-drags 应记录 `active-item-move`。Renderer policy 必须保持 `gpui_directory_drop_shell=0` |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state 加临时 shell | 保持 GPUI shell 仅用于启动 | 不移除直到 GPUI 暴露公开 custom-element drag-start 或 Fika 携带经过审计的 GPUI patch |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model 和 overlay geometry | 保持 GPUI overlay | rename 编辑器计划中列出的行为矩阵（`docs/RENAME_EDITOR_PLAN.md`） |
| Details header、row 背景、图标、文本单元格、Trash 列 | custom content-level painter | Details paint snapshots, row layout projection, shape cache | 保持 custom paint。Header 背景、分隔线和标签由 Details visual layer 绘制，不再是 GPUI child element。 | 运行时 Details perf 和 DnD 冒烟证据必须保持最新；renderer policy 必须保持 `gpui_details_header=0` |
| Details click/menu/navigation/hover/cursor/drop hit testing | retained row hit testing/controller state 加 active item-drag window tracker | viewport retained hit testing | 保持 retained controller path。目录 row drop hover 由 retained window-position hit testing 解析，不再由 per-directory GPUI drag-move shell 解析。 | painter 变更后 DnD 冒烟必须通过；renderer policy 必须保持 `gpui_directory_drop_shell=0` |
| Details drag start | GPUI `Div::on_drag` row shell | retained drag payload state | 保持 GPUI shell | 与 Compact/Icons drag start 相同门 |
| Places rows、section headings 和 sidebar scrollbar | 默认 full custom row/section visual layer、retained-DnD mixed event delivery、一个 sidebar typed DnD payload shell 和 GPUI row drag-start shell；`gpui`、`chrome`、`text` fallback policy 仍可用 | `places` model/projection、`places/interaction.rs`、retained event layer、retained Places icon image cache、text shape cache 和 `drag_drop` state | 保持 Dolphin 对齐的 retained model/controller/painter 拆分为默认。行文本、section heading 文本和 Places 图标现在由 Fika 自己 custom paint；Places 图标通过 retained `RetainAllImageCache` 使用 GPUI 高效的底层 `RenderImage`/`paint_image` 路径，符合 Dolphin pixmap-cache 原则，同时不再在 Places row 或 heading 中留下 GPUI text/image 子元素。Typed DnD payload delivery 和 drag start 仍是明确 GPUI/平台边界。 | 默认日志必须通过 `--expect-custom-row-full-policy` 和 `--require-interaction-policy`，并显示 `event_policy=retained-dnd`、`text_gpui=0`、`icon_gpui=0`、`section_gpui=0`、`visual_kind=full`、`retained_hitboxes=rows+sections`、`gpui_event_shells=1`、`gpui_row_section_event_shells=0`、`gpui_typed_dnd_payload_shells=1`、`gpui_sidebar_leave_shells=0`，且聚合 `[fika places-row-visual]` rows 匹配策略行数。GPUI/chrome fallback 保留 GPUI heading text，并继续作为 analyzer 覆盖的基准。 |

## Perf 日志收集

## 2026-06-19 默认 Full 与 GPUI Baseline 对比

Places 当前默认就是 full custom row visual：
`DEFAULT_PLACES_ROW_VISUAL_POLICY = CustomFull`。GPUI row 路径只作为
`FIKA_PLACES_ROW_VISUAL_POLICY=gpui` 指定的 baseline 存在。

同场景 Places targets autosmoke 证据：

- Full default/handoff：
  `/tmp/fika-compare-places-full.log` 通过
  `scripts/analyze-places-perf.sh --expect-custom-row-handoff-policy`，
  ready 帧显示 `row_gpui=0`、`text_gpui=0`、`icon_gpui=0`、
  `visual_kind=full`。`places_view max_snapshot=624us`，
  `places_sidebar max_build=374us`，`places_slots max_project=42us`，
  row visual warm paint 小于 `472us`。
- GPUI baseline：
  `/tmp/fika-compare-places-gpui.log` 使用
  `FIKA_PLACES_ROW_VISUAL_POLICY=gpui`，显示 `row_gpui=11`、
  `row_visual_layer=0`、`visual_kind=gpui`，
  `places_view max_snapshot=1253us`，`places_sidebar max_build=551us`，
  `places_slots max_project=52us`。

Pane 的 GPUI baseline 范围更窄，因为 Compact/Icons base visual 和 retained
interaction 已经默认自绘。当前可用的非自绘 image baseline 是
`FIKA_GPUI_THEME_ICONS=1`：它只把普通 MIME/theme icon 放回 GPUI `img()`，但
条目背景/文本和交互层仍保持 retained/custom。

加入 alternate-mode static text warmup 后的同场景 pane autosmoke 证据：

- `/etc` 默认 full：
  `/tmp/fika-compare-pane-full-etc-r3.log` 保持 `gpui_image_element=0`、
  `image_layer=48`、`theme_placeholder=0`、visible `theme_decoded=0`，
  image `max_prepaint=165us max_paint=384us`。剩余主要成本仍是 static text，
  `static_visual max_prepaint=2996us max_paint=9303us`。
- `/etc` GPUI image baseline：
  `/tmp/fika-compare-pane-gpui-etc-r3.log` 显示 `gpui_image_element=48`、
  `image_layer=0`，static text 成本接近
  `max_prepaint=2938us max_paint=8981us`。
- Downloads 默认 full：
  `/tmp/fika-compare-pane-full-downloads-r3.log` 的 image 稳定指标干净
  （`gpui_image_element=0`、`theme_placeholder=0`、visible `theme_decoded=0`），
  但仍有 static text 冷成本 `max_prepaint=16866us max_paint=17580us`。
- Downloads GPUI image baseline：
  `/tmp/fika-compare-pane-gpui-downloads-r3.log` 仍有类似 static text 冷成本
  （`max_prepaint=15175us max_paint=17754us`），同时 theme icons 使用
  `gpui_image_element=39`。

决策：Places full 继续作为默认。Pane image full 继续作为默认，因为 image layer 稳定且移除了
GPUI image elements；但不能宣称 pane full 已完全达到目标性能。剩余 pane 工作是 text
shape/paint retention 与 handoff，而不是 image decode 或 MIME icon renderer policy。

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

对于 MIME 图标闪烁调查，对比 Dolphin 的 `KStandardItemListWidget::updatePixmap()` 和 `pixmapForIcon()`：Dolphin 保持 widget-local `m_pixmap` 并按 icon name/size 使用 `QPixmapCache`，因此已加载的真实图标不会在相同图标资源刷新时被标记替换。Fika 的默认 hybrid 路径会让 MIME/theme icon 在当前 retained image key ready 前继续使用 GPUI `img()`，ready 后再通过 custom image layer 绘制。如果使用 `FIKA_CUSTOM_THEME_ICONS=1`，custom image painter 必须通过按 MIME/theme `iconName`、icon size 和 scale keyed 的 retained images 保持相同行为。缩略图 retention 保持按精确缩略图路径键控。Fika 不会通过读取和解码 SVG 在 GPUI prepaint 中复制 Dolphin 的同步 `QIcon::pixmap()`；GPUI image loading 保持为解码路径。中性无标记占位符仅可作为 custom-theme 首帧加载/失败后备，而非已加载真实图标的退化。详细 retained image-cache 设计见 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md`；基础实现位于 `src/ui/icons/image_cache.rs`，默认 MIME/theme renderer 由 `scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion` 守卫。

GPUI 高效的 `img()` 路径本质上也是这个架构形状，而不是特殊的同步绘制 API：`img()` 将 `Resource` 交给 `ImageCache`；`RetainAllImageCache` 以 resource hash 为 key 保存共享后台加载任务或已加载的 `Arc<RenderImage>`，加载完成后通知下一帧；`Window::paint_image` 再按稳定的 `(RenderImage.id, frame_index)` 放入 sprite atlas 并提交 sprite primitive。Fika 的 custom image/text 路径需要模仿的是这些应用层约束：稳定语义 key、retained loaded resource、可见路径不重复 decode/shape replacement，以及 retained resource ready 后才 handoff。

对于缩放调查，对比 `KFileItemListView::triggerIconSizeUpdate()` 和 `updateIconSize()`：Dolphin 立即更新条目几何但暂停 `KFileItemModelRolesUpdater`，在 `LongInterval`（300ms）后重新启动 preview/visible-range role work。Dolphin 的普通 `iconName` pixmap 路径不同：`pixmapForIcon()` 使用 widget 的当前 style-option icon size，但 item role 仍是稳定 `iconName`。因此 Fika 立即改变 layout/icon bounds；同一文件图标类型首次解析出 MIME/theme icon path 后，path identity 保持稳定，且不得为 theme icons 安排延迟的第二次 icon-size 或 path 提交。

对于目录加载时的 MIME 图标切换，对比 `KFileItemModel::retrieveData()`、`KFileItemModelRolesUpdater::updateVisibleIcons()` 和 `KFileItemListView::initializeItemListWidget()`：Dolphin 不会同步解析所有 model role，但在异步 `ResolveAll` pass 遍历其余部分之前确实给已创建的可见 widget 一个 `iconName`。Fika 应保持相同分离：可见通用 MIME metadata 和可见 theme-icon path 可在有界预算内同步解析；read-ahead/offscreen metadata 和 icon path 保持排队。Zoom 是单独情况：同一文件图标类型已有任意 resolved theme path 后，Fika 复用该稳定 path，而不是再排队另一个 exact-size path 请求。这复制了 Dolphin 的 `iconName` 加 `pixmapForIcon()` 路径，而不将 read-ahead icon-theme 扫描移入渲染转换，也不在 zoom 时提交第二个 image identity。图像解码本身保持在 scheduler/image-cache 路径上；默认 theme icons 通过 GPUI `img()` 解码，而 custom-theme A/B 绘制层可保留先前相同 `iconName` 图像但不得在 prepaint 期间同步解码 theme icon 文件。

2026-06-18 `/etc` 成对证据未通过 default-promotion gate：
`/tmp/fika-icon-custom-etc-p16k2.log` 有 `theme_placeholder=118` 和
`theme_decoded=5`；`/tmp/fika-icon-default-etc-p16k2.log` 继续让普通 MIME/theme
icon 走 GPUI `img()`，且 `[fika item-image]` 中没有 placeholder/decode churn。因此当时默认策略保持不变。

Opt-in prewarm bridge 现在可通过 `FIKA_PREWARM_THEME_ICONS=1` 使用。
`/tmp/fika-icon-prewarm-etc-p16k2.log` 显示该 bridge 继续让普通 MIME/theme icon 走
GPUI（`max_image_layer=0`、`max_gpui_image_element=64`），且不暴露 custom theme
placeholder（`theme_placeholder=0`、`paint_count=0`），同时通过 `theme_prewarm_*`
单独记录 retained-image readiness。这只是 staging step；默认提升仍需要 readiness
handoff，确保可见 icon 只有在当前 key 的 retained image ready 后才离开 GPUI。

readiness handoff 基础现在位于 `FIKA_HYBRID_THEME_ICONS=1` 后面。app 拥有 size/scale
aware 的 `ThemeIconImageReadiness` snapshot；image layer 只有在真实 `RenderImage` 可用后才
标记 key ready；renderer policy、item shell 和 image layer 都消费同一份 readiness 输入。
这个阶段仍然没有改变默认 renderer。Hybrid 必须在 `/etc` 和混合目录的成对 zoom/scroll 证据中证明没有
placeholder、没有 zoom-time decode burst、没有 paint 回归，之后该决策表才能把 MIME/theme
icon 从 GPUI `img()` 提升出去。

第一份 `/etc` hybrid smoke 记录在 `/tmp/fika-icon-hybrid-etc-readiness.log`，默认对照为
`/tmp/fika-etc-zoom-scroll.log`。它证明 handoff 路径可以在没有 theme placeholder 或
zoom-time decode churn 的情况下工作（`theme_placeholder=0`、`theme_decoded=0`、
`max_paint=383us`），同时默认 split 保持不变（`max_image_layer=0`、
`max_gpui_image_element=64`）。这仍不足以提升默认值，因为 `/etc` 滚动进入新条目时仍有约
24ms 的 visible-item `icon_sync` spike，混合目录证据也还未采集。

2026-06-19 成对 hybrid 运行补齐了混合目录证据缺口：
`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix
fika-hybrid-icons-20260619` 生成了 `/etc` 和 Downloads 的 default-vs-hybrid 日志，且两组都通过
`scripts/compare-item-image-renderers.sh --gate-hybrid-handoff` 和
`--gate-hybrid-default-promotion`。`/etc` hybrid 报告
`theme_loaded=444`、`theme_placeholder=0`、`theme_decoded=0`、
`theme_prewarm_pending=52`、`max_paint=504us`；Downloads hybrid 报告
`theme_loaded=310`、`theme_placeholder=0`、`theme_decoded=0`、
`theme_prewarm_pending=44`、`max_paint=378us`。这支持后续默认策略代码切片：如果代码变更后
仍能通过同一 gate，并且对尚未 ready 的 key 保持 GPUI fallback，普通 MIME/theme icon 可以默认
切到 hybrid renderer。

默认策略代码切片已用
`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix
fika-hybrid-default-20260619` 验证。Candidate 日志使用默认 renderer policy，不设置
`FIKA_HYBRID_THEME_ICONS`；baseline 日志使用 `FIKA_GPUI_THEME_ICONS=1`；`/etc` 和
Downloads 都通过了 `--gate-hybrid-default-promotion`，且 `theme_placeholder=0`、visible
`theme_decoded=0`。

## 2026-06-19 Pane 可见集合级 Image Handoff

Places 默认 full row visual 之后，同一条经验被应用到 Compact/Icons MIME/theme icon：
Fika 拥有 retained image 状态，但继续使用 GPUI 高效的
`RetainAllImageCache -> RenderImage -> paint_image` 路径。直接把
`FIKA_CUSTOM_THEME_ICONS=1` full-custom 压力路径设为冷启动默认仍不安全：
`/tmp/fika-pane-full-custom-etc.log` 显示 `theme_placeholder=52` 和 visible
`theme_decoded=5`，这对应启动时空白再变真实图标、zoom 时二次调整的症状。

因此这次接受的 pane 改动不是强制 full-custom 冷绘制，而是可见集合级 handoff：只要
当前可见集合里还有任何 theme-icon key 未 ready，所有可见 theme icons 都继续使用
GPUI `img()`，item image layer 只做 retained image 预热；当这组 key 全部 ready 后，
整组 theme icons 同一批切到 custom image layer。这样避免同一 viewport 内逐项
GPUI/custom 混切，这是局部尺寸/绘制跳变的高风险来源。

切换后的证据：

- `/tmp/fika-pane-cohort-default-downloads.log` 对
  `/tmp/fika-pane-cohort-gpui-downloads.log` 通过
  `--gate-hybrid-default-promotion`，并且 `theme_placeholder=0`、visible
  `theme_decoded=0`。
- `/tmp/fika-pane-cohort-default-etc-r2.log` 保持关键 image 稳定指标干净
  （`theme_placeholder=0`、visible `theme_decoded=0`），但
  `--gate-hybrid-default-promotion` 仍因该次 `/etc` 的 `icon_sync` 和
  content-change total 高于配对 GPUI baseline 而失败。

决策：保留可见集合级 handoff，因为它减少可见切换且没有重新暴露 visible placeholder。
暂不把 `FIKA_CUSTOM_THEME_ICONS=1` 压力路径提升为默认。下一步 pane image 工作应继续压
`/etc` 的 `icon_sync` 波动，然后重新跑 default-vs-GPUI promotion 证据。

## 2026-06-19 File Icon Kind 索引和更宽后台批次

下一个 `/etc` 阻塞点不是 image painting。可见集合级 handoff 已经保持
`theme_placeholder=0` 和 visible `theme_decoded=0`，但配对运行仍会因为
`icon_sync` 在可见 icon candidates 上花 7-13ms 而失败。结构化日志里常见
`candidates=64 cached=64`，实际只有一两个 changed icon，这说明热点更像 cache lookup
开销，而不是 custom image 绘制。

根因：`FileIconCache::cached_icon_for_kind()` 为了复用同 kind 的 resolved theme icon，
每次都会扫描整个 exact-size cache。resize/fullscreen 或 scroll 时，visible sync 会对每个
可见 candidate 做一次这种扫描。这还不够 Dolphin-like：Dolphin 的 item widget 持有直接的
pixmap/icon role 状态，复用已解析 iconName/pixmap 是索引查找，而不是每帧 cache walk。

实现：

- `FileIconCache` 新增 `resolved_by_kind`，按 `FileIconKind` 索引 pathful resolved icons。
  exact-size `cached` 仍然拥有精确 size 结果和 negative exact lookup；kind 索引只用于同
  MIME/icon kind、跨 zoom size 复用真实 resolved theme path。
- 后台 file-icon resolve batch 从 64 提到 128，让 bounded visible/read-ahead work range
  更可能在 resize 或 scroll 让额外 item 进入可见区域之前完成。

证据：

- `/tmp/fika-icon-batch128-default-etc.log` 相对
  `/tmp/fika-icon-batch128-gpui-etc.log` 通过
  `--gate-hybrid-default-promotion`。Candidate `icon_sync` 最大值为 `103us`，
  且 `theme_placeholder=0`、visible `theme_decoded=0`。
- `/tmp/fika-icon-batch128-default-downloads-r2.log` 相对
  `/tmp/fika-icon-batch128-gpui-downloads-r2.log` 通过同一 gate。

决策：保留 kind 索引和更宽后台批次。它保留 Dolphin visible-first 契约，同时把同 kind
图标复用从 render-frame 热路径移出去。后续 image 工作应转向替换剩余 GPUI `img()`
fallback 边界或降低 cold first-resolve 成本，而不是继续处理 cached same-kind lookup。

## 2026-06-19 Places Full Handoff A/B

Places full row visual 路径现在有真实的 opt-in 突破，但还不是默认提升决策。

当前 full 路径是：

- `FIKA_PLACES_ROW_VISUAL_POLICY=full`：在 retained row visual layer 中绘制文本和
  vector icon。
- `FIKA_PLACES_ROW_VISUAL_HANDOFF=1`：ready-only handoff。warmup 帧继续显示 GPUI
  text/icons，预热 `PlacesRowTextShapeCache`，只有 retained 资源 ready 后 row 才切到
  full custom paint。

证据采集命令：

```sh
scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix fika-places-full-handoff-runner-20260619
scripts/run-retained-renderer-evidence.sh --places-full-handoff --analyze-only --skip-build --prefix fika-places-full-handoff-runner-20260619
```

关键日志：

- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-targets.log`
  通过 full-handoff row-visual gate。ready/warm row paint 为 `379us`，但首帧
  `[fika render] total` 达到 `27268us`。
- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-overflow.log`
  在 75 行、29 个 painted rows 下通过，warm row paint 为 `1090us`。
- `/tmp/fika-places-full-handoff-runner-20260619-places-handoff-full-layout.log`
  通过，warm row paint 为 `724us`。

决策：当前默认继续保持 Places custom chrome 加 GPUI text/icons。阻塞点已经不是 cold row
visual paint 本身，而是启用 full handoff 时整帧 startup/target total-render 波动。后续默认提升
工作需要把首帧 total 中的 Places snapshot、pane item、root 和 row visual owner 分开，再降低
full 专属波动，之后才能下调 full 路径的 30ms total-render guard。

后续 owner accounting 在
`/tmp/fika-places-full-owner-20260619-places-handoff-full-targets.log` 中把 max-total
residual 降到 `4us`，并显示同一帧主要 owner 是 `chrome_inputs=7817us`，不是 row visual
painting。因此下一步优化目标是 toolbar/chrome icon/input preparation，然后再重新评估 row
visual 默认提升阈值。

后续拆分在
`/tmp/fika-places-chrome-split-20260619-places-handoff-full-targets.log` 中显示 max total
帧为 `chrome_state=2us`、`chrome_icons=8360us`。这确认剩余首帧目标是 named
toolbar/chrome icon resolution，而不是一般 render state projection。

chrome icon prewarm 切片随后把这个 owner 从默认 chrome 和 full handoff 两条路径都移除了。
`FikaApp::new()` 现在会在首帧 render 前解析固定 toolbar/sidebar snapshot。证据来自
`scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
fika-places-chrome-prewarm-20260619`：所有 handoff gate 通过，`chrome_icons` 降到
chrome targets `12us`、full targets `6us`、chrome overflow `10us`、full overflow
`9us`、chrome layout `7us`、full layout `7us`。因此 full 路径确实有首帧层面的实质
突破：旧的 8-14ms chrome icon 尖峰已经消失。它仍保持 opt-in，因为默认提升现在取决于
row visual、pane elements 和 root cost 的重复 total-render 证据，而不是已经解决的
chrome icon owner。

## 2026-06-19 Places Section Heading 所有权

Places full visual 成为默认后，section heading label 仍然是 GPUI text child。这留下了一个
很小但真实的所有权不一致：row text 和 icon 已经是 retained/custom，而 group heading
仍由 GPUI element shape/paint。

实现：Places visual layer 现在使用与 row 相同的 snapshot 投影 section heading geometry，
通过 `PlacesRowTextShapeCache` prepaint 可见 section label，并在同一个 canvas 中先于
row 绘制它们。`group_heading` 仍作为 section targeting/DnD 边界 shell 存在，但当
custom visual layer 绘制文本时不再挂载 GPUI label child。

证据：

```sh
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-section-full-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy /tmp/fika-places-section-full-targets.log
timeout 8s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-section-full-overflow.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-full-policy /tmp/fika-places-section-full-overflow.log
```

决策：默认 Places full visual 应与 `text_gpui=0`、`icon_gpui=0` 一起报告
`section_gpui=0`。GPUI/chrome fallback 仍可以报告 `section_gpui=sections`；
typed DnD payload 和 row drag-start shell 继续是明确的 GPUI/平台边界。

保存的日志已经通过这些 gate。`/tmp/fika-places-section-full-targets.log` 报告
`max_section_gpui=0`、`max_text_gpui=0`、`max_icon_gpui=0`、`visual_kinds=full`
和 warm row paint `247us`。overflow 日志报告 `max_rows=75`、`max_sections=3`、
`max_section_gpui=0`，visible event hitboxes 裁到 `32`，warm row paint 为 `785us`。

## 2026-06-19 Pane 目录 Drop Shell 移除

Pane 目录 hover/drop targeting 不再需要 per-directory GPUI `on_drag_move` shell。
retained 路径已经具备所需 model：`update_dragged_paths_drop_target_from_window_position()`
把窗口坐标映射到 pane/item geometry，选择目录 item 或 pane target，并更新
`DropTargetState`。active item-drag preview/window tracker 会在 GPUI 停止派发
per-element drag move 后继续更新同 pane 拖拽。

实现：Compact/Icons item shell 和 Details row 不再安装
`install_directory_drop_target_shell`；该 helper 和 `directory-shell-hit` 路径已移除。
透明 row/item shell 现在只保留 typed drag start 和 rename overlay 边界。Renderer-policy
日志现在分离 `retained_directory_drop_target` 和 `gpui_directory_drop_shell`，并且
`--expect-retained-item-policy` 会拒绝任何非零 GPUI directory drop shell 计数。

决策：pane 目录 drop hover 属于 retained viewport/window-position hit testing。这与
Places 方向一致：GPUI 仍可启动 typed drag，但持续 hover/drop targeting 应由 retained
controller state 拥有。

证据：`/tmp/fika-item-retained-directory-drop.log` 通过
`scripts/analyze-item-view-perf.sh --require-autosmoke --require-renderer-policy
--require-interaction --expect-retained-item-policy`。其 renderer-policy 摘要报告
`max_retained_directory_drop_target=60` 和 `max_gpui_directory_drop_shell=0`；
item interaction hitbox 仍匹配可见 retained layer，`max_prepaint_count=64`。

## 2026-06-19 Details Header 视觉所有权

Details row 已经由 custom Details visual layer 绘制，但 Details header 仍是带 text
child 的 GPUI `Div` 树。这让 Details 模式里还残留一个静态 GPUI 视觉 surface。

实现：`details_visual_layer_view()` 现在除了 row projection 之外也拥有 header
projection。它通过现有 Details visual canvas 和 `DetailsTextShapeCache` 绘制 header
背景、底部分隔线、列分隔线和已 shape 的列标题。`details_shell.rs` 不再构建 GPUI
`details_header()` 子树。Renderer-policy 日志现在报告 `details_header_visual_layer` 和
`gpui_details_header`，retained item policy 会拒绝 `gpui_details_header != 0`。

决策：Details header rendering 属于 custom Details painter。后续应补专门的
Details-mode runtime smoke 作为更强证据；本切片由单元测试、`cargo check`、对刚才抖动
失败测试的单独重跑，以及 analyzer guard 覆盖。

## 2026-06-19 Details 运行时证据门

Details painter 现在有自己的无人值守运行时路径，不再只依赖默认 Compact zoom/scroll
smoke。`FIKA_AUTOSMOKE_ITEM_VIEW=details-zoom-scroll` 会先把 active pane 切到
Details，然后运行同一组 zoom 和 scroll action。item-view analyzer 接受
`DetailsZoomScroll` scenario，要求 `view-details` marker，并可以用
`--require-details`、`--require-modes Details`、
`--require-renderer-policy-modes Details` 和 `--expect-retained-item-policy` gate 日志。

决策：任何触碰 Details row、header、text shaping 或 retained interaction 行为的
custom paint 改动，都必须使用这个 gate。retained renderer evidence runner 会把它采集为
`item-etc-details-zoom-scroll`。

## 2026-06-19 Pane 图标 Path-Ready Handoff

Pane MIME/theme icon handoff 仍有一个 exact-key artifact：zoom 会产生新的 size/scale
`ThemeIconImageKey`，因此 `Resource::Path` 已经加载过的可见图标仍可能短暂回到 GPUI
fallback，或在 custom visible paint 中被统计为新的 first-ready decode。这不符合 Dolphin
路径；Dolphin 的 pixmap 路径按语义图标数据缓存，而已加载资源在 style size 变化时仍可被
widget 使用。

实现：`ThemeIconImageReadiness` 现在同时记录 ready semantic key 和 ready resource
path。Visible-cohort handoff 接受 exact key ready 或 resource path ready 的 theme icon。
`RetainedThemeIconImageCache` 也按 path 建立 loaded image 索引，因此同一路径的新 size key
会被视为 retained reuse，而不是 first-ready decode。

决策：保留 cohort handoff，但允许同 resource 的 custom paint 跨 zoom 复用。这把 Places
full-image 的经验迁到 pane image，同时不会把未知路径强行推入 custom placeholder。

证据：`/tmp/fika-path-ready-hybrid-downloads.log` 相对
`/tmp/fika-path-ready-gpui-downloads.log` 通过
`scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion`，且
`theme_placeholder=0`、visible `theme_decoded=0`。
`/tmp/fika-path-ready-hybrid-etc-r2.log` 通过 handoff 部分并移除 visible decode churn
（`theme_decoded=0`），但完整 default promotion 仍因 `/etc` icon-sync/content-change
方差失败；该失败点不在 image handoff 路径。

## 2026-06-19 Pane Full 图标 Key-Size 缓存

上面的 path-ready 方案已被替代。重新对照 Dolphin 后确认，正确模型不是按
`Resource::Path` 判断 MIME/theme icon 是否 ready，而是与
`KStandardItemListWidget::pixmapForIcon()` 一致：model 持有稳定 `iconName`，绘制层按
`iconName + iconHeight + devicePixelRatio + mode` 查 pixmap cache。Path 只是 icon theme
resolver 找到的加载入口，不应该成为上层 ready/cache 主 key。

实现：

- pane MIME/theme icon 默认改为 full custom image layer；`FIKA_GPUI_THEME_ICONS=1` 仅作为
  GPUI baseline，`FIKA_HYBRID_THEME_ICONS=1` 降为显式过渡路径。
- `ThemeIconImageReadiness` 只记录 `ThemeIconImageKey(iconName, size, scale, theme,
  color-scheme, mode)`，不再记录 ready resource path。
- `RetainedThemeIconImageCache` 不再用 `images_by_path` 为新 size key 复用旧 image。相同
  path 的不同 size 必须形成自己的 key；底层 `Resource::Path` 去重仍由 GPUI
  `RetainAllImageCache` 或同步 SVG loader 负责。
- `FileIconCache` 的 resolved kind 索引改为 exact `FileIconCacheKey`，并新增
  `MIME + size` 索引，用于同 MIME 不同扩展名在同一 size 下复用 resolved icon。它不再把
  48px path 带到 64px。
- 对 SVG theme icons，full image layer 在冷 key 上同步调用 GPUI `svg_renderer` 生成
  `RenderImage`，然后仍通过 `Window::paint_image`/sprite atlas 绘制。这复制 Dolphin
  `QIcon::pixmap()` 的首帧语义，同时避免回到 GPUI `img()` element。

证据：

- `/tmp/fika-full-syncsvg-custom-etc.log` 相对
  `/tmp/fika-full-syncsvg-gpui-etc.log`：full path 报告
  `max_image_layer=64`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_retained=497`；`content-change max_total=28663us` 低于 GPUI baseline
  `38298us`，`icon_sync=27661us` 低于 baseline `37062us`。
- `/tmp/fika-full-syncsvg-custom-downloads.log` 相对
  `/tmp/fika-full-syncsvg-gpui-downloads.log`：full path 报告
  `max_image_layer=32`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_retained=543`；initial total `11899us` 低于 baseline `15103us`。

剩余问题：Downloads cold run 的 `item-image max_prepaint=38250us` 来自一次性同步 SVG
decode 22 个 theme icons。方向不是回退 hybrid/path-ready，而是把 theme `RenderImage`
cache 提升到 app/global owner，并在目录加载/可见集确定后按 `ThemeIconImageKey` 预热，
让 full custom 首帧继续无 placeholder，同时把冷 decode 从 paint prepass 移走。

## 2026-06-19 Pane Theme Icon Snapshot 预热

full custom MIME/theme icon 路径暴露了第二个 ownership 问题。保留的 `RenderImage`
cache 如果仍然属于 image-layer element，冷 SVG 工作只能发生在 element prepaint 中；
首个 custom frame 虽然没有 placeholder，但 `[fika item-image]` 仍会承担 decode 成本。

实现：`FikaApp` 现在拥有 pane theme `RenderImage` cache。在构建 `PaneSnapshot` 时，
等可见 `FileGridRenderSnapshot` 已确定、但还没把 `theme_icon_readiness` 交给 pane
rendering 之前，Fika 会收集可见 custom-theme `ThemeIconImageKey`，按
`iconName + size + scale + theme + mode` 去重，通过 GPUI `svg_renderer` 同步生成 SVG
`RenderImage`，写入 app cache，并把这些语义 key 标记 ready。file-grid surface 不再做
model update，也不再使用单独的 prewarm element；它只消费刷新后的 readiness snapshot，
并通过 `Window::paint_image` 绘制 retained image。

决策：早期 theme-icon 准备属于 Fika model/snapshot 阶段，不属于 image element
prepaint。这更接近 Dolphin 的拆分：model/snapshot 路径拥有稳定 icon identity 和可见工作
发现，painter 只按语义 key 消费已 ready 的 pixmap/image。Resolved path 仍只是当前 icon
theme 的资源入口。

证据：

- `/tmp/fika-early-prewarm-custom-etc.log` 相对
  `/tmp/fika-early-prewarm-gpui-etc.log`：默认 full custom 报告
  `max_image_layer=64`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_decoded=0`、`theme_prewarm_decoded=0`、`theme_retained=454`；
  `item-image max_prepaint=166us`。
- `/tmp/fika-early-prewarm-custom-downloads.log` 相对
  `/tmp/fika-early-prewarm-gpui-downloads.log`：默认 full custom 报告
  `max_image_layer=32`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_decoded=0`、`theme_prewarm_decoded=0`、`theme_retained=187`；
  `item-image max_prepaint=315us`。

剩余问题：冷工作已经移出 image element，但 `/etc` 在 content-change frame 中仍可能出现
高 `icon_sync`。这现在是 model/icon-resolution 路径问题，不是 visible image paint
路径问题，后续应继续按 Dolphin 风格推进 MIME/icon model cache 和 visible-work batching。

## 2026-06-19 常见 File Icon 独立预热

`/etc` zoom-scroll smoke 把剩余滚动卡顿定位到两个冷的可见语义 icon 解析，而不是 image
paint：`.pwd.lock`（`application/octet-stream`）同步扫 theme path 约 28ms，`.updated`
（`text/plain`）约 2ms。这正对应 Dolphin 的模型约束：常见 MIME/icon-name 结果必须存在按
icon kind/MIME 和 size keyed 的语义 model cache 中；具体文件 path 不是 cache identity。

实现：启动时现在会独立后台预热常见 file-icon 语义 key，并优先处理默认 48px size 与邻近 zoom
level，然后补全剩余 size。预热表包含 directory，以及常见 text、binary、archive、office、
image、video、audio 和 PDF MIME key。这批工作通过 `finish_resolve_results` 写入同一个
`FileIconCache`，但刻意不占用 `FileIconResolveQueue` 的 cover key。第一次实验把这些 key
直接排入 visible resolver queue，确实消除了滚动卡顿，但也让首个 `/etc` 内容帧把可见目录视为
queued，临时失去 image layer。独立预热保留首帧 custom image，同时仍在 scroll/zoom 需要前填充
共享语义 cache。

GPUI `img()` 仍然是 image 路径下半部分的参考：`RetainAllImageCache` 把 `Resource` load 作为
后台任务缓存并保存 `Arc<RenderImage>`；`Window::paint_image` 再用
`(RenderImage.id, frame_index)` 放入 sprite atlas。Fika 的 full custom 路径保留这条高效的
`RenderImage -> paint_image` GPU/atlas 路线，但把上半部分 identity 改成 Dolphin 风格的语义
key，而不是 GPUI 的 resource hash。

证据：`/tmp/fika-common-icon-prewarm-detached-etc.log` 使用
`FIKA_DEBUG_ICON_SYNC=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` 后，不再出现 scroll-time
`application/octet-stream` 或 `text/plain` sync resolve。`icon_sync max_total` 从之前约
30ms（`/tmp/fika-debug-icon-sync-etc.log`）降到 `104us`，`max_resolved=1` 只剩初始 directory
key；首个内容帧保持 `max_image_layer=48`/`max_gpui_image_element=0`，且
`theme_placeholder=0`。
扩展表之后的 `/tmp/fika-common-icon-prewarm-expanded-etc.log` 仍保持同一等级：
`icon_sync max_total=241us`，没有 scroll-time 文件 MIME resolve。
`/tmp/fika-common-icon-prewarm-expanded-downloads.log` 显示
`application/x-tar` 等常见 archive MIME 已变为百微秒级，但首个可见
`application/java-archive` 仍可能和独立预热竞态，并在首个内容帧同步 resolve。这属于混合目录
first-visible scheduling 问题，不是本次已修复的 `/etc` scroll 回归。

## 2026-06-19 默认尺寸 MIME Negative Cache

混合目录后续验证显示，单靠 detached prewarm 不足以覆盖首个内容帧。它可能输给 visible
`icon_sync`；而 theme lookup 失败的 MIME key 也需要作为语义 negative result 缓存。否则
预热过的 `application/java-archive` miss 不能保护可见 `.jar` 条目，后者仍会再次扫描 theme。

实现：常见 file-icon 预热现在会在 app 初始化期间、加载第一个 pane 之前，同步解析默认 48px
语义 MIME 表。剩余 zoom size 仍由 detached background prewarm 补齐。`FileIconCache` 也会把
pathless MIME 结果写入 `MIME + size` 索引。后续文件复用该 MIME entry 时，Fika 保留已解析的
`iconName/path` identity，但用当前 file kind 重新计算 fallback marker 和颜色，所以 `.jar`
fallback 仍能显示 `JAR`，同时不再扫描 theme。

决策：这比让 visible render conversion 拥有首个 MIME icon lookup 更接近 Dolphin 的拆分。
启动预热是有界语义预热，不按 path；它在首个 pane snapshot 需要之前准备常见 model-level
icon role。Painter 仍保持 full custom，并继续使用 retained `RenderImage -> Window::paint_image`。

证据：`/tmp/fika-common-icon-sync48-downloads.log` 报告 `max_resolved=0`，没有
`[fika icon-sync-resolve]` 行，`icon_sync max_total=235us`，
`max_gpui_image_element=0`，`theme_placeholder=0`。`/tmp/fika-common-icon-sync48-etc.log`
报告 `max_resolved=0`，`icon_sync max_total=33us`。

## 2026-06-19 SVG Source RenderImage 保留

重新审查 GPUI `img(Resource::Path(svg))` 后确认，GPUI 不会为每个 layout size 重新 decode
SVG。asset loader 会为 resource 生成一个 `Arc<RenderImage>`，`Window::paint_image` 再按
paint bounds 缩放；sprite atlas key 是 `(RenderImage.id, frame_index)`。Fika full custom
已经使用 `paint_image`，但 theme image cache 只按 `ThemeIconImageKey` 索引，因此同一个
scalable SVG source 在新的 zoom-size key 下仍可能重复 materialize。

实现：`RetainedThemeIconImageCache` 现在额外维护 `source path -> RenderImage` 索引。
`ThemeIconImageKey` 和 readiness 仍然 size/scale-aware，所以新的 zoom size 仍需要自己的
语义 ready key；但如果 source SVG 已有 retained `RenderImage`，Fika 会直接用该 source image
记录新 key，不再重新读取和渲染 SVG。source 复用在 telemetry 中记为 retained，而不是 decoded，
因此 `[fika item-image]` 能区分 source-level reuse 和真实 decode/materialization。

决策：这同时保留 Dolphin 风格的上层 model key
（`iconName + size + scale + theme + mode`）和 GPUI 高效的下层 image ownership
（`RenderImage -> paint_image -> atlas`）。Resolved path 仍不是 readiness key，也不会让无关
semantic key ready；它只是 retained image source。

证据：`/tmp/fika-svg-source-retain-etc.log` 报告 `theme_decoded=0`、
`theme_retained=982`、`theme_placeholder=0`、`max_gpui_image_element=0`、
`item-image max_prepaint=480us`。`/tmp/fika-svg-source-retain-downloads.log` 报告
`theme_decoded=0`、`theme_retained=702`、`theme_placeholder=0`、
`max_gpui_image_element=0`、`item-image max_prepaint=788us`。

## 2026-06-19 Pane Static Text Shape 复用

image/icon ownership 移出热路径后，剩余 full custom 方差转移到
`[fika static-item-visual]`。对照 GPUI text element 后确认，GPUI 在 layout 阶段 shape
文本，prepaint 只记录 bounds；Fika custom layer 则在 prepaint 中 shape 全部可见 item
label。因此冷模式切换和首个可见帧会把文本 shaping 成本压到 custom painter，除非 retained
cache 已经预热。

实现：static item text shape 现在按真实 text/style 输入缓存，而不是按 item identity 缓存。
`StaticItemTextShapeCacheKey` 移除 `item_id`。Center/Icons 标签在已经选定可见 label lines
之后，不再把 text rect 宽高作为 key 维度，因为这些 bounds 只影响 paint 对齐/裁剪，不改变
`shape_line`。没有绘制 fallback marker 时，也不再把 fallback marker line height 放进 key。
static painter 还会跳过普通未选中/未悬停条目的透明 background quad。
`FIKA_AUTOSMOKE_ITEM_VIEW=icons-zoom-scroll` 现在会先切到 Icons 再执行 zoom/scroll，使这条
路径能被运行时证据覆盖。

决策：这是正确方向，但不是最终文本方案。它比 item-local key 更接近 Dolphin 的
content/style/layout-keyed retention，并消除了 Icons 后续 zoom 的重复 miss；但它还没有移除
首次进入模式时的冷文本/glyph 尖峰。下一步应复用 Places text handoff 思路：在某个模式的
第一个 full custom visual frame 前，用 retained state pool 预热目标模式 label shapes/glyphs。

证据：`/tmp/fika-full-icons-keyed-etc.log` 覆盖 `modes: Icons,Compact`，
`max_gpui_image_element=0`、`theme_placeholder=0`、`theme_decoded=0`。初次切入 Icons 后，
zoom 帧报告 `hits=24 misses=0`、`hits=28 misses=0`、`hits=40 misses=0`，重复 zoom 的
prepaint 为 93-254us。剩余风险由 `/tmp/fika-full-icons-keyed-downloads-r2.log` 记录：
首次 Icons 切换仍报告 `hits=1 misses=39`、`static-item-visual prepaint=52840us`，第一次
text paint frame 达到 `17698us`。

## 下一批渲染器决策

1. 保持剩余 drag-start shells 直到 GPUI API 边界变化。不要将 GPUI per-element `on_drag_move` 用作 pane self-drag 悬停的真实来源；active item-drag window tracker 拥有该路径。
2. 使用运行时日志决定当前 custom-painted surface 是否保持 custom-paint 或回退到 GPUI 渲染器叠加在 retained model 上。
3. 保留 `FIKA_GPUI_THEME_ICONS=1` 作为 GPUI baseline 路径，并在未来 MIME/theme icon
   renderer 变更时继续使用 `--gate-hybrid-default-promotion`。
4. 通过 `--places-full-handoff` A/B gate 继续推进 Places full-row visual；只有当
   row-visual cost 和整帧 `[fika render] total=` 对比默认 chrome policy 达到中性或更优后，
   才能提升 full rows 默认值。
