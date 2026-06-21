> 本文是 [ITEM_VIEW_CUSTOM_PAINT_STATUS.md](ITEM_VIEW_CUSTOM_PAINT_STATUS.md) 的简体中文翻译。

# 条目视图自定义绘制状态

> 本文是 [ITEM_VIEW_CUSTOM_PAINT_STATUS.md](ITEM_VIEW_CUSTOM_PAINT_STATUS.md) 的简体中文翻译。

这是 Dolphin 风格条目视图迁移的当前替换地图。它是一份状态文档，而非承诺每个表面都必须变为自定义绘制。架构目标是保留 model/布局/controller/painter 状态；每个渲染器在成为默认值之前仍必须击败或匹配 GPUI 基线。

本文现在是 GPUI 基线/历史文档。Places chrome 默认之后的 retained 路线图
`docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md` 仍可作为证据参考；活跃 UI 方向已经转为
`docs/WGPU_SHELL_ROADMAP.zh-CN.md` 中的 Fika 专用 upstream winit/wgpu shell。

## 当前第一优先级

第一优先级 retained-glyph 切片现在已经覆盖 Places 和文件网格文本。Places
是参考实现：Fika 拥有 retained `ShapedLine` 身份、retained
`GlyphRasterData` 生命周期和自定义 paint 调用点，而 GPUI 仍然是文本 raster/backend
底座。该契约按顺序应用到 pane 文本：先 Details cell/header，再 Compact/Icons
静态标签和 fallback marker，并已落地；下一要求是保持运行时证据新鲜。Shape
cache 可以继续保留 Dolphin 风格的几何复用 key，但 glyph-raster cache 必须使用
paint-geometry key，因为 GPUI raster data 绑定 origin、line height、align width 和
scale factor。

glyph-raster miss 峰值现在已经预算化。当前第一优先级的后续约束是剩余 cold
shape/layout 峰值，尤其是 Details text shape miss。Static item 和 Details text 使用
可见层优先的 glyph 预算：cache hit 直接走 retained raster paint；cache miss 只在预算内
同步计算；超预算 glyph 工作本帧回退到 GPUI normal text paint，并通过 `cx.notify()`
触发后续帧继续补齐。相反模式的 warm/static read-ahead 层排在真实可见层之后，只使用
已有 shape-cache hit，并使用自己的小 glyph 预算。证据必须同时观察 cache 总量和预算
画像：`[fika item-shape-cache]`、`[fika details-shape-cache]` 和
`[fika places-row-shape-cache]` 输出 `compute=...us`；`[fika item-glyph-budget]` /
`[fika details-glyph-budget]` 输出 `computed`、`deferred`、`budget_exhausted`
和 glyph `compute=...us`。

## 当前替换矩阵

| 表面 | 当前状态 | 渲染器 | 剩余依赖 |
| --- | --- | --- | --- |
| Compact/Icons 条目 model 和几何 | 保留 | `DirectoryModel`、可见快照、slot 池 | 当前路径无 |
| Compact/Icons 基础背景、选择、悬停、放置色调、标签 | 已替换 | 带 retained shape 和 glyph-raster text cache 的自定义内容级绘制器 | 运行时性能和 DnD smoke 证据必须保持最新 |
| Compact/Icons 缩略图图像 | 已替换 | 自定义图像绘制器，使用 GPUI `RetainAllImageCache` 加上保留同缩略图图像 | 挂起/失败仍复用保留图像或绘制缩略图后备 |
| Compact/Icons MIME/主题图标图像 | 默认 full custom image layer 已替换 | retained image layer 使用 GPUI `RetainAllImageCache -> RenderImage -> Window::paint_image`；`FIKA_GPUI_THEME_ICONS=1` 保留 GPUI `img()` baseline | 修改 image renderer policy 前必须有同场景 image A/B 证据 |
| Compact/Icons 点击、菜单、悬停、光标和放置 hit testing | 已替换 | 保留 viewport/自定义 hitbox 加上活动条目拖拽窗口跟踪器 | 绘制器更改后仍需要运行时 DnD smoke |
| Compact/Icons 拖拽启动 | 已替换 | 通过 Fika GPUI fork 的 retained hitbox typed drag | 保持 `gpui_drag_shell=0` 且 DnD smoke 通过 |
| Compact/Icons 重命名编辑器 | 未替换 | GPUI 编辑器叠加层 | 仅在 caret、选择、IME 和文本输入行为被覆盖后才重新审视 |
| 详情行 model 和几何 | 保留 | 详情绘制快照和行布局投影 | 当前路径无 |
| 详情行背景、图标、文本单元格、回收站列 | 已替换 | 自定义内容级绘制器 | 详情图标使用相同的缓存/初步图标策略；Details text 同时 retained shape 和 glyph-raster paint data；运行时详情性能和 DnD smoke 证据必须保持最新 |
| 详情点击、菜单、导航、悬停、光标、放置 hit testing | 已替换 | 保留行 hit testing/controller 状态加上活动条目拖拽窗口跟踪器 | 绘制器更改后仍需要运行时 DnD smoke |
| 详情拖拽启动 | 已替换 | 通过 Fika GPUI fork 的 retained row hitbox typed drag | 保持 `gpui_drag_shell=0` 且 Details DnD smoke 通过 |
| Places 行和侧栏滚动条 | 保留 model/slot/目标决策状态，默认 full row visual、retained event delivery 和 typed DnD 已替换 | 默认 `FIKA_PLACES_ROW_VISUAL_POLICY=full` 用一个 sidebar-level 自定义层绘制 background/drop/insert/trash、行标签、section heading 和 Places 图标，同时 `retained-dnd` 拥有 activation/context-menu targeting/DnD target lookup/drop dispatch；Places text 同时 retained shaped line 和 GPUI glyph-raster paint data；Places 图标使用 retained `RetainAllImageCache` 加 `paint_image` 路径，并保留稳定 fallback；drag start 和 typed payload delivery 使用 Fika GPUI fork 的 retained hitbox；`gpui`、`chrome`、`text` fallback 仍可用 | 保持 `gpui_event_shells=0`、`gpui_typed_dnd_payload_shells=0`、`drag_shells=0` 且 retained-event smoke 通过 |

实际状态是：条目视图静态视觉、image painting、hit testing、drop routing 和 drag start
都已迁移到保留/自定义绘制架构。重命名仍然是 GPUI editor/platform-contract 边界。
Places 现在默认使用自定义 full row visual 层加 retained-DnD row/section target delivery、
typed payload delivery 和 drag start，因此行标签、section heading、行图标和 DnD
交互默认都不需要 GPUI row 子元素。Places text 仍使用 GPUI backend 绘制，但 Fika 拥有
retained `ShapedLine` 和 glyph-raster paint-data 生命周期。Places 图标绘制复用 GPUI
`img()` 高效的底层机制：缓存后的 `RenderImage` 通过 `window.paint_image` 提交，
retained cache 在 pending reload 期间保留已有真实 image。

## 证据锚点

- 渲染器策略代码：`src/ui/file_grid/renderer_policy.rs`
- 根文件网格渲染表面组合：`src/ui/file_grid/surface.rs`
- Compact/Icons 布局选项和 Dolphin 尺寸常量：`src/ui/file_grid/layout.rs`
- Compact/Icons 静态视觉绘制器：`src/ui/file_grid/painter.rs`
- 保留交互/hitbox 层：`src/ui/file_grid/interaction.rs`
- 保留条目/详情绘制 slot 状态：`src/ui/file_grid/paint_slots.rs`
- Compact/Icons retained item hitbox/DnD 边界：
  `src/ui/file_grid/interaction.rs`、`src/ui/file_grid/dnd.rs`
- Compact/Icons text shape-cache 通道：`[fika item-shape-cache]`
  （`compute=...us`）
- Compact/Icons text retained glyph-raster cache 通道：`[fika item-glyph-cache]`
- Compact/Icons text glyph-raster miss 预算通道：`[fika item-glyph-budget]`
- 详情布局投影和行快照：`src/ui/file_grid/details.rs`
- Details retained row hitbox/DnD 边界：
  `src/ui/file_grid/interaction.rs`、`src/ui/file_grid/dnd.rs`
- 编辑器边界（重命名仍为 GPUI 编辑器叠加层）：`src/ui/file_grid/rename_overlay.rs`
- 性能测量门和基线：`scripts/analyze-item-view-perf.sh`
- Details text shape-cache 通道：`[fika details-shape-cache]`
  （`compute=...us`）
- Details text retained glyph-raster cache 通道：`[fika details-glyph-cache]`
- Details text glyph-raster miss 预算通道：`[fika details-glyph-budget]`
- 渲染器决策日志：`docs/ITEM_VIEW_RENDERER_DECISIONS.md`
- Places 渲染器计划和基线：`docs/PLACES_RENDERER_PLAN.md`
- Places 行目标决策和保留 hitbox 数据：`src/ui/places/interaction.rs`
- Places 溢出自动 smoke 路径：`FIKA_AUTOSMOKE_PLACES=overflow` 和 `src/ui/places/sidebar/scroll_metrics.rs`

## 运行时证据要求

每个替换的表面必须获得运行时证据才能被接受为默认渲染器。需要的证据包括：

| 表面 | 所需证据 | 当前状态 |
| --- | --- | --- |
| 静态条目视觉的保留 model/布局投影 | `/etc` 和 `~/Downloads` 自动 smoke 日志，验证可见范围、slot 复用和缓存命中 | 已满足 |
| 静态条目视觉的自定义内容级绘制器 | Compact/Icons 调整大小/全屏稳定路径的每帧 `[fika static-item-visual]` 日志 | 已满足 |
| 缩略图的自定义图像绘制器 | `[fika item-image]` 每帧日志，验证图像源计数、保留同源图像和缩略图后备 | 已满足 |
| 非重命名条目的保留悬停/光标 hitbox | `[fika interaction]` 每帧日志，显示 hitbox 计数和计时与 GPUI 悬停/cursor 路径的对比 | 已满足 |
| 详情静态视觉的自定义内容级绘制器 | `[fika details-visual]`、`[fika details-shape-cache]`、`[fika details-glyph-cache]` 和 `[fika details-glyph-budget]` 每帧日志 | 已满足 |
| 详情的保留行 hit testing/controller | `[fika interaction]` 每帧日志，覆盖详情行计数 | 已满足 |
| Places 默认渲染器 | GPUI 侧栏 `FIKA_PERF_PLACES_VIEW=1` 基线 | 已捕获 |
| Places 可选行视觉绘制器 | 启用 `FIKA_CUSTOM_PLACES_ROWS=1` 时的 `[fika places-row-visual]` prepaint/paint 最大值 | 可选基准表面 |
| Places 溢出/滚动条 | `[fika places-scrollbar] visible=1` 和 `max_scroll_y`，通过 `FIKA_AUTOSMOKE_PLACES=overflow` 验证 | 已满足 |

运行时证据必须定期刷新，尤其是在任何绘制器扩展或 shell 移除切片之前。单一桌面会话运行不会无限期保持有效。证据日志必须包括紧凑和图标模式、`~/Downloads` 和 `/etc` 目录以及至少一次全屏/调整大小序列的覆盖。`scripts/analyze-item-view-perf.sh` 强制执行标准通道、视图模式和可接受的每帧最大成本。

## 当前门控

### R1：运行时证据

`scripts/analyze-item-view-perf.sh` 通过的标准证据门：

- `[fika item-view] phase=` 覆盖 `initial`、`mode-switch`、`content-change`、`geometry-change`、`visual-change` 和 `steady`
- 对于 Compact 和 Icons 模式，`[fika static-item-visual]` 低于每帧最大值
- `[fika item-image]` 低于每帧最大值，并显示源分布（解码图标、保留同 `iconName` 图像、首帧加载占位符、缩略图后备）
- `[fika interaction]` 覆盖 Compact、Icons 和 Details 的自定义 hitbox 计数
- `[fika details-visual]`、`[fika details-shape-cache]` 和
  `[fika details-glyph-cache]` 存在且低于每帧最大值
- `[fika item-glyph-budget]` 和 `[fika details-glyph-budget]` 存在；冷帧
  miss 可以被 `deferred`，但 `compute=...us` 必须保持在小帧预算内，并由后续帧补齐
- 可见条目/viewport 尺寸和有效缩放级别出现在 `[fika item-view]` 摘要中
- 调整大小运行在 `phase=geometry-change` 之后最终产生 `phase=steady`
- Compact、Icons 和 Details 模式切换出现并分别跟踪冷预热，与调整大小分开
- `[fika renderer-policy]` 日志显示自定义绘制、保留交互和 GPUI shell 边界的合理表面计数分布
- `scripts/check-item-view-perf-analyzer.sh` 通过（分析器自检门）

当前状态：最终 core evidence 已冻结并通过，新的 hitbox-DnD evidence 也已通过。`scripts/run-retained-renderer-evidence.sh --core --skip-build --prefix fika-full-retained-hitbox-dnd-v2` 覆盖 Compact、Icons、Details 以及 Places targets/overflow/layout/hit-test/targeting/dnd，并显示 `gpui_drag_shell=0`、`gpui_event_shells=0`、`gpui_typed_dnd_payload_shells=0` 和 `drag_shells=0`。

### R2：图像和主题图标视觉稳定性

在 P8 自定义图像绘制层被接受后，主题图标渲染对首帧加载占位符抖动很敏感。当前默认路径已经推进到 full custom image layer：pane MIME/theme icon 通过 retained semantic key、app-level readiness/cache、source-image reuse 和有界预算绘制，底层仍复用 GPUI `RetainAllImageCache -> RenderImage -> Window::paint_image`。`FIKA_GPUI_THEME_ICONS=1` 保留旧 GPUI baseline，`FIKA_HYBRID_THEME_ICONS=1` 保留过渡 handoff 路径。在任一渲染器中，主题图标解码保持在 GPUI 的图像缓存/RenderImage 路径上；普通渲染/prepaint 代码不得无界同步读取或解码主题图标文件。缩略图仅按精确缩略图路径保留，并继续使用容纳的图像边界。缩略图后备图标仍然在没有真实图像存在或语义源更改时绘制。

即时非 GUI 安全的工作是在 Dolphin 对齐的缩放/图标视觉更新后冻结新的运行时证据，然后执行 P15 转换顺序。大型文件网格渲染器/controller 模块已拆分为聚焦的 model/投影、controller/hit-test、painter 和 renderer-policy 模块。

### R3：解决拖拽启动边界

拖拽启动边界已通过 Fika GPUI fork 解决。当前维护规则：

- Fika pin `gpui`/`gpui_platform` 到 fork revision
  `02f256ffd7edfbcbb5354ad03db7a193def08590`。
- 条目、Details 和 Places drag start 必须继续从 retained hitbox 注册，而不是从可见
  GPUI row/item `Div` 注册。
- Analyzer gate 必须保持 `gpui_drag_shell=0`。

此前的源审计使用来自 Zed 提交
`69b602c797a62f09318916d24a98c930533fbdc8` 的 GPUI `0.2.2`，说明公开 API 不足；
当前 fork patch 是该边界的正式实现路径。
拖拽启动通过 `crates/gpui/src/elements/div.rs` 中的 `Interactivity::on_drag` /
`StatefulInteractiveElement::on_drag` 暴露，它从交互元素 hitbox 构造类型化拖拽预览。
GPUI 自定义元素可以使用 `Window::insert_hitbox()` 插入 hitbox，并可以通过
`Window::on_mouse_event()` 观察鼠标事件，但没有公共 API 从任意保留绘制器 hitbox
启动类型化拖拽。Fika 现在有意携带小型经过审计的 GPUI patch，并通过 retained hitbox
注册条目、详情和 Places 的拖拽启动；这些路径不再需要 GPUI row/item drag-start shell。

DnD shell 已不再是当前边界。Pane 内部条目拖拽悬停不得依赖 GPUI 每元素 `on_drag_move`；运行时证据显示自拖拽可以在没有后续元素拖拽移动回调的情况下发出 `item-start`。Fika 通过保留交互层安装的窗口鼠标监听器跟踪活动条目拖拽，然后将窗口位置通过相同的保留 pane hit-test 路由，该 hit-test 由 Places 和外部放置使用。

已接受的后备是拖拽预览重绘路径。GPUI 可能在指针移动时继续重绘拖拽预览，即使它不传递底层 pane 在同窗口条目拖拽中的拖拽移动回调。因此 Fika 使用预览渲染 pass 仅作为时钟来查询当前窗口鼠标位置并运行相同的保留 hit test。有效的 smoke 日志可以仅显示 `active-item-move via=preview`；所需信号是移动在放置前到达 `kind=Some(Directory)` 并且当光标在其上时目录条目高亮。

2026-06-17 运行时追踪确认了这一确切路径：pane 自拖拽首先报告 `kind=Some(Pane)`，然后越过目录并报告 `kind=Some(Directory) changed=true` 通过 `via=preview`，无需每条目 `on_drag_move`。当前已接受的架构是 retained hit-testing、retained hitbox drag start，加上预览驱动 tick；Analyzer gate 必须保持 `gpui_drag_shell=0`。

### R4：评估重命名边界

在文本编辑仍然是 GPUI 拥有的平台契约时保持 GPUI 重命名叠加层。自定义重命名渲染器在被接受之前需要行为覆盖：焦点、caret 移动、选择、验证状态、提交/取消和 IME。

具体行为矩阵和 Dolphin 源比较位于 `docs/RENAME_EDITOR_PLAN.md`。

### R5：单独评估 Places 渲染器

Places 是独立于 item-view 的渲染器决策。当前默认已经是 Dolphin 对齐的 full path：
默认自定义层绘制 row background/drop/insert/trash、标签、section heading 和图标；
retained-DnD event layer 拥有 row/section activation、context-menu targeting、DnD
target lookup、typed payload delivery、drop dispatch、sidebar leave clearing 和 drag start。

继续扩展之前：

- 保留滚动、重排、外部放置、条目放置、设备条目、隐藏 section 和右键菜单的
  GPUI fallback 基线
- 保持 retained Places event-delivery smoke 最新，并要求 `--expect-retained-event-policy`
- 只有在 retained text/image cache 持续达到或超过 GPUI baseline 时，才保持文本和图标在默认 full retained/custom 路径

`FIKA_CUSTOM_PLACES_ROWS=1` 仍是显式 full-row stress alias。溢出证据通过
`FIKA_AUTOSMOKE_PLACES=overflow` 可用，它添加非持久仅快照行并验证
`[fika places-scrollbar] visible=1`。Places 分析器通过要求
`[fika places-row-visual] rows` 匹配渲染器策略行计数来拒绝旧的 per-row canvas
形状；默认 full gate 要求 row shape-cache 和 glyph-cache 证据，并要求 GPUI
event/typed-payload/drag shell 计数为 0；chrome/text/GPUI policy 仅作为对照 baseline
保留。

具体保留行设计和 Dolphin 源比较位于 `docs/PLACES_RENDERER_PLAN.md`。

Places 作为 pane 放置悬停的行为参考仍然有用：将 Place 拖到 pane 目录上和将 pane 条目拖到 pane 目录上应在移动时都产生保留的 `Directory` 条目放置目标。

### R6：池复用目标

长期复用池目标仅在可复用视觉 identity 在 GPUI 子 identity 之外拥有时有效：

- Compact/Icons 使用可见 slot id 和保留绘制快照
- 详情使用行绘制快照和形状缓存
- 图像和文本形状缓存是 pane 本地且按 slot/内容键控的
- 渲染器策略日志证明哪些 fallback 表面仍由 GPUI-backed 路径承担

当前条目视图复用已经遵循这个所有权规则。`VisibleItemSlotPool`
将 `ItemId` 映射到 pane 本地 `slot_id`，通过有界 free-list 回收离屏
slot，并在原始快照变成渲染快照之前分配这些 slot。随后
`ItemPaintSlotCache` 按 `slot_id` 保留 Compact/Icons 的绘制内容、几何和
视觉状态；详情按 `ItemId` 保留行绘制状态。GPUI id 可能仍存在于明确的
fallback/baseline 表面，但它们是保留 identity 的消费者，不是条目复用的来源。
Retained hitbox 和 full custom image layer 消费 `slot_id`/`ItemId` 状态；可复用条目状态
仍然属于 slot 池和 paint-slot cache。

证据锚点是保留测试：
`visible_item_slot_pool_reuses_offscreen_slots`、
`visible_item_slot_pool_caps_recycled_slots`、`src/ui/file_grid/tests.rs` 中的
paint-slot 内容/几何/视觉变化测试，以及运行时
`[fika item-paint-slots]` / `[fika renderer-policy]` 日志。未来的复用池变更如果
改变视觉 identity 来源，必须更新这些测试或日志。它不应依赖 GPUI 子 key
作为主要复用机制。`scripts/analyze-item-view-perf.sh --require-paint-slots`
是保留 paint-slot 证据的运行时门；它拒绝缺失非空 `[fika item-paint-slots]`
条目的日志，并汇总 inserted、content、geometry、visual、unchanged、removed
和 entries 最大值。`--expect-retained-item-policy` 是配套 renderer-policy 门：
基础视觉必须覆盖每个条目的保留表面，保留交互加重命名叠加层必须覆盖每个
条目，GPUI image baseline 和 rename 边界必须在策略计数中保持显式。

此目标现在要求拖拽启动保持 retained hitbox 路径；重命名仍保持在 GPUI 上。池边界是保留条目/行状态，而非声称今天每个渲染器都是自定义绘制的。

### R7：完整转换执行顺序

下一个转换工作必须遵循此顺序：

1. 在 Dolphin 对齐的缩放图标视觉更新后冻结当前桌面会话证据。使用 `~/Downloads` 测试普通 MIME/缩略图行为，`/etc` 测试大型混合目录滚动，以及 `FIKA_DEBUG_DND=1` 测试 pane 自拖拽悬停。
2. 在更改渲染器表面之前用证据更新 `docs/ITEM_VIEW_RENDERER_DECISIONS.md`。不要将通过的单元测试视为 DnD、调整大小、全屏或缩放视觉稳定性的足够证据。
3. 保持 Fika GPUI retained-hitbox typed DnD patch 紧跟 upstream。条目/详情/Places drag start 必须保持 retained-hitbox 路径，且不丢失 payload、预览、光标偏移或外部放置行为。
4. 将 Places 视为其自己的迁移；默认 full row visual 和 retained-DnD 已完成，后续只在保留 GPUI/chrome/text fallback 基线与 text/glyph cache 证据的前提下调整。
5. 在自定义编辑器覆盖焦点、caret hit testing、UTF-8 选择、验证、提交/取消、Tab 重命名下一个和 IME 之前，保持重命名为 GPUI 文本编辑边界。
6. 继续收紧复用池证据：普通条目视图帧应显示保留视觉/图像/文本/交互所有权，仅保留显式接受的 GPUI 平台边界。

此顺序的详细任务板位于 `docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md` 中的 P15。

### R8：具体完整转换轨道

已接受的方向是保留/自定义绘制的条目视图，但执行必须保持为证据支持的轨道：

1. **证据轨道**：继续刷新 `~/Downloads` 和 `/etc` 的桌面会话日志，包括调整大小、全屏、滚动、缩放、模式切换和 DnD。这些日志决定渲染器是否保持自定义绘制，而非仅凭架构偏好。对于图像闪烁和缩放尺寸调查，在更改当前图像渲染器之前，包括 `a3f5b0f` 的历史 GPUI 图像基线和转换检查点 `d497593`/`8d1198f`/`36da130`/`b0cac9a`。
2. **绘制器轨道**：继续仅在绘制器消费保留快照且能匹配 Dolphin widget 行为的地方将视觉工作移入内容级绘制器。下一个绘制器工作是图像冷加载/缩放路径的稳定化和测量，而非盲目添加新的视觉表面。
3. **Controller 轨道**：保持点击、菜单、悬停、光标、选择、pane 放置、条目放置和外部放置通过保留 viewport hit testing 路由。GPUI 每条目回调仅是临时的平台桥梁。
4. **Shell 边界轨道**：通过 Fika GPUI retained-hitbox typed DnD patch 将 GPUI DnD shell 计数保持为 0。在行为矩阵覆盖文本输入和 IME 之前保持重命名在 GPUI 上。
5. **Glyph-raster 轨道**：Places full rows 是参考实现；同一 retained text/glyph paint-data 模型现在已覆盖 Details cells/header 和 Compact/Icons labels/fallback markers。每个 surface 的 evidence gate 必须同时包含已有 shape-cache 通道、glyph-cache 通道，以及证明冷 glyph miss 工作被预算化和 deferred 而不是塞进单个 prepaint pass 的 glyph-budget 通道。Shape-cache `compute=...us` 是下一项 cold-frame 压力指标；Details 需要 warm-only/read-ahead 或显式 deferral 设计后，才能无保留地宣称完成。
6. **所有权轨道**：继续在行为保持时将编排从 `src/main.rs` 提取到 Dolphin 对齐的文件网格模块。这包括角色调度移交、运行时证据助手，以及最终的 shell 边界所有权。

这是"完全转换"的实际含义：每个条目视图行为应由保留 model/布局/controller/painter 状态拥有，而任何剩余的 GPUI 渲染器是具有证据和移除门的显式平台边界。
