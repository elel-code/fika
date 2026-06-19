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
| Compact/Icons MIME/theme-icon 图像 | 可见集合级 hybrid GPUI fallback 加 custom image layer handoff | retained item slots、visible icon role/path cache、app-level `ThemeIconImageReadiness`、pane image layer、background file-icon resolve queue | 默认使用 hybrid。当前可见集合中任意 theme-icon key 未 ready 时，整组继续使用 GPUI `img()`，custom image layer 只预热 retained `RenderImage`；当前可见集合全部 ready 后，这组 theme icons 同一批切到 retained custom image layer 绘制。`FIKA_GPUI_THEME_ICONS=1` 强制旧 GPUI baseline，`FIKA_CUSTOM_THEME_ICONS=1` 继续作为 full custom 压力路径 | 默认 hybrid 日志必须在 `/etc` 和混合用户目录中相对 `FIKA_GPUI_THEME_ICONS=1` baseline 通过 `scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion` |
| Compact/Icons hover/cursor/click/menu/drop hit testing | retained viewport/custom hitboxes 加 active item-drag window tracker | viewport retained hit testing 和 `drag_drop` state | 保持 retained controller path | DnD 冒烟通过内部 item、pane、Places 和外部 drop；pane self-drags 应记录 `active-item-move` |
| Compact/Icons drag start | GPUI `Div::on_drag` shell | retained drag payload state 加临时 shell | 保持 GPUI shell 仅用于启动 | 不移除直到 GPUI 暴露公开 custom-element drag-start 或 Fika 携带经过审计的 GPUI patch |
| Compact/Icons rename editor | GPUI text/editor subtree overlay | rename draft model 和 overlay geometry | 保持 GPUI overlay | rename 编辑器计划中列出的行为矩阵（`docs/RENAME_EDITOR_PLAN.md`） |
| Details row 背景、图标、文本单元格、Trash 列 | custom content-level painter | Details paint snapshots, row layout projection, shape cache | 保持 custom paint | 运行时 Details perf 和 DnD 冒烟证据必须保持最新 |
| Details click/menu/navigation/hover/cursor/drop hit testing | retained row hit testing/controller state 加 active item-drag window tracker | viewport retained hit testing | 保持 retained controller path | painter 变更后 DnD 冒烟必须通过 |
| Details drag start | GPUI `Div::on_drag` row shell | retained drag payload state | 保持 GPUI shell | 与 Compact/Icons drag start 相同门 |
| Places rows 和 sidebar scrollbar | 默认 full custom row visual layer、retained-DnD mixed event delivery、一个 sidebar typed DnD payload shell 和 GPUI row drag-start shell；`gpui`、`chrome`、`text` fallback policy 仍可用 | `places` model/projection、`places/interaction.rs`、retained event layer、retained Places icon image cache 和 `drag_drop` state | 保持 Dolphin 对齐的 retained model/controller/painter 拆分为默认。行文本和 Places 图标现在由 Fika 自己 custom paint；Places 图标通过 retained `RetainAllImageCache` 使用 GPUI 高效的底层 `RenderImage`/`paint_image` 路径，符合 Dolphin pixmap-cache 原则，同时不再在 row 中留下 GPUI `img()` 子元素。Typed DnD payload delivery 和 drag start 仍是明确 GPUI/平台边界。 | 默认日志必须通过 `--expect-custom-row-full-policy` 和 `--require-interaction-policy`，并显示 `event_policy=retained-dnd`、`text_gpui=0`、`icon_gpui=0`、`visual_kind=full`、`retained_hitboxes=rows+sections`、`gpui_event_shells=1`、`gpui_row_section_event_shells=0`、`gpui_typed_dnd_payload_shells=1`、`gpui_sidebar_leave_shells=0`，且聚合 `[fika places-row-visual]` rows 匹配策略行数。GPUI/chrome/text fallback 继续作为 analyzer 覆盖的基准。 |

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

## 下一批渲染器决策

1. 保持剩余 drag-start shells 直到 GPUI API 边界变化。不要将 GPUI per-element `on_drag_move` 用作 pane self-drag 悬停的真实来源；active item-drag window tracker 拥有该路径。
2. 使用运行时日志决定当前 custom-painted surface 是否保持 custom-paint 或回退到 GPUI 渲染器叠加在 retained model 上。
3. 保留 `FIKA_GPUI_THEME_ICONS=1` 作为 GPUI baseline 路径，并在未来 MIME/theme icon
   renderer 变更时继续使用 `--gate-hybrid-default-promotion`。
4. 通过 `--places-full-handoff` A/B gate 继续推进 Places full-row visual；只有当
   row-visual cost 和整帧 `[fika render] total=` 对比默认 chrome policy 达到中性或更优后，
   才能提升 full rows 默认值。
