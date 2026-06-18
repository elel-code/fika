> 本文是 [ITEM_VIEW_CUSTOM_PAINT_STATUS.md](ITEM_VIEW_CUSTOM_PAINT_STATUS.md) 的简体中文翻译。

# 条目视图自定义绘制状态

> 本文是 [ITEM_VIEW_CUSTOM_PAINT_STATUS.md](ITEM_VIEW_CUSTOM_PAINT_STATUS.md) 的简体中文翻译。

这是 Dolphin 风格条目视图迁移的当前替换地图。它是一份状态文档，而非承诺每个表面都必须变为自定义绘制。架构目标是保留 model/布局/controller/painter 状态；每个渲染器在成为默认值之前仍必须击败或匹配 GPUI 基线。

Places chrome 默认之后的当前执行路线图是
`docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md`。

## 当前替换矩阵

| 表面 | 当前状态 | 渲染器 | 剩余依赖 |
| --- | --- | --- | --- |
| Compact/Icons 条目 model 和几何 | 保留 | `DirectoryModel`、可见快照、slot 池 | 当前路径无 |
| Compact/Icons 基础背景、选择、悬停、放置色调、标签 | 已替换 | 自定义内容级绘制器 | 运行时性能和 DnD smoke 证据必须保持最新 |
| Compact/Icons 缩略图图像 | 已替换 | 自定义图像绘制器，使用 GPUI `RetainAllImageCache` 加上保留同缩略图图像 | 挂起/失败仍复用保留图像或绘制缩略图后备 |
| Compact/Icons MIME/主题图标图像 | 保留 model，GPUI 渲染器 | GPUI `img()` 元素叠加在保留条目 shell 上 | 主题图标路径在当前布局图标尺寸下脱离渲染路径解析；自定义主题图标绘制仅通过 `FIKA_CUSTOM_THEME_ICONS=1` 启用以获取 A/B 证据 |
| Compact/Icons 点击、菜单、悬停、光标和放置 hit testing | 已替换 | 保留 viewport/自定义 hitbox 加上活动条目拖拽窗口跟踪器 | 绘制器更改后仍需要运行时 DnD smoke |
| Compact/Icons 拖拽启动 | 未替换 | GPUI `Div::on_drag` shell | 公共 GPUI 自定义元素拖拽启动 API 或经过审计的 Fika GPUI patch |
| Compact/Icons 重命名编辑器 | 未替换 | GPUI 编辑器叠加层 | 仅在 caret、选择、IME 和文本输入行为被覆盖后才重新审视 |
| 详情行 model 和几何 | 保留 | 详情绘制快照和行布局投影 | 当前路径无 |
| 详情行背景、图标、文本单元格、回收站列 | 已替换 | 自定义内容级绘制器 | 详情图标使用相同的缓存/初步图标策略；运行时详情性能和 DnD smoke 证据必须保持最新 |
| 详情点击、菜单、导航、悬停、光标、放置 hit testing | 已替换 | 保留行 hit testing/controller 状态加上活动条目拖拽窗口跟踪器 | 绘制器更改后仍需要运行时 DnD smoke |
| 详情拖拽启动 | 未替换 | GPUI `Div::on_drag` 行 shell | 相同的拖拽启动 API 或经过审计的 GPUI patch 门 |
| Places 行和侧栏滚动条 | 保留 model/slot/目标决策状态，默认 row chrome 已替换 | 默认 `FIKA_PLACES_ROW_VISUAL_POLICY=chrome` 用一个 sidebar-level 自定义层绘制 background/drop/insert/trash，同时 GPUI 保留文本/图标/事件 shell；`gpui` fallback 和 `FIKA_CUSTOM_PLACES_ROWS=1` full-text 基准路径仍可用 | 保留 hitbox 以及任何文本/图标自定义绘制器仍需要 Places 特定的 DnD/滚动证据 |

实际状态是：条目视图静态视觉和大多数应用侧 controller 路径已迁移到保留/自定义绘制架构。拖拽启动和重命名仍然是 GPUI 渲染器/平台契约边界。Places 现在默认使用自定义 row chrome 层，但行文本、图标、拖拽启动、右键菜单和行级事件传递仍然是 GPUI。

## 证据锚点

- 渲染器策略代码：`src/ui/file_grid/renderer_policy.rs`
- 根文件网格渲染表面组合：`src/ui/file_grid/surface.rs`
- Compact/Icons 布局选项和 Dolphin 尺寸常量：`src/ui/file_grid/layout.rs`
- Compact/Icons 静态视觉绘制器：`src/ui/file_grid/painter.rs`
- 保留交互/hitbox 层：`src/ui/file_grid/interaction.rs`
- 保留条目/详情绘制 slot 状态：`src/ui/file_grid/paint_slots.rs`
- 编辑器边界（重命名仍为 GPUI 编辑器叠加层）：`src/ui/file_grid/item_shell.rs`
- 详情布局投影和行快照：`src/ui/file_grid/details.rs`
- 详情 shell 边界（拖拽启动）：`src/ui/file_grid/details_shell.rs`
- 性能测量门和基线：`scripts/analyze-item-view-perf.sh`
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
| 详情静态视觉的自定义内容级绘制器 | `[fika details-visual]` 和 `[fika details-shape-cache]` 每帧日志 | 已满足 |
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
- `[fika details-visual]` 和 `[fika details-shape-cache]` 存在且低于每帧最大值
- 可见条目/viewport 尺寸和有效缩放级别出现在 `[fika item-view]` 摘要中
- 调整大小运行在 `phase=geometry-change` 之后最终产生 `phase=steady`
- Compact、Icons 和 Details 模式切换出现并分别跟踪冷预热，与调整大小分开
- `[fika renderer-policy]` 日志显示自定义绘制、保留交互和 GPUI shell 边界的合理表面计数分布
- `scripts/check-item-view-perf-analyzer.sh` 通过（分析器自检门）

当前状态：`/etc` 自动 smoke 满足 Compact/Icons 缩放-滚动图标同步部分。详情和完整 DnD 运行时 smoke 仍需要桌面会话刷新。下一个 shell 移除或绘制器扩展切片在证据被冻结之前不得继续进行。

### R2：图像和主题图标视觉稳定性

在 P8 自定义图像绘制层被接受后，主题图标渲染对首帧加载占位符抖动很敏感。GPUI 的 `img()` 元素避免了自定义图像层暴露的首帧加载占位符帧。自定义主题图标绘制路径仅通过 `FIKA_CUSTOM_THEME_ICONS=1` 可用以获取配对证据。在任一渲染器中，主题图标解码保持在 GPUI 的图像缓存路径上；渲染/prepaint 代码不得同步读取或解码主题图标文件。缩略图仅按精确缩略图路径保留，并继续使用容纳的图像边界。缩略图后备图标仍然在没有真实图像存在或语义源更改时绘制。

即时非 GUI 安全的工作是在 Dolphin 对齐的缩放/图标视觉更新后冻结新的运行时证据，然后执行 P15 转换顺序。大型文件网格渲染器/controller 模块已拆分为聚焦的 model/投影、controller/hit-test、painter 和 renderer-policy 模块。

### R3：解决拖拽启动边界

在以下条件之一为真之前，不移除剩余的 GPUI 拖拽启动 shell：

- GPUI 暴露公共自定义元素拖拽启动 API。
- Fika 携带小型经过审计的 GPUI patch，从保留 hitbox 暴露拖拽启动，并有运行时 DnD 证据。

在此门之前移除 shell 将使架构更不可靠，即使它看起来更接近完全自定义绘制。

当前源审计使用来自 Zed 提交 `f16a469` 的 GPUI `0.2.2`，保持此门关闭。拖拽启动通过 `crates/gpui/src/elements/div.rs` 中的 `Interactivity::on_drag` / `InteractiveElement::on_drag` 暴露，它从交互元素 hitbox 构造类型化拖拽预览。GPUI 自定义元素可以使用 `Window::insert_hitbox()` 插入 hitbox，并可以观察鼠标/拖拽移动，但没有公共 API 从任意保留绘制器 hitbox 启动类型化拖拽。`App::has_active_drag()` 仅是已启动拖拽的观察器。因此实际边界不变：条目和详情拖拽启动 shell 保持，直到 GPUI 暴露该钩子或 Fika 有意携带小型经过审计的 patch。

现在 shell 仅是拖拽启动边界。Pane 内部条目拖拽悬停不得依赖 GPUI 每元素 `on_drag_move`；运行时证据显示自拖拽可以在没有后续元素拖拽移动回调的情况下发出 `item-start`。Fika 通过保留交互层安装的窗口鼠标监听器跟踪活动条目拖拽，然后将窗口位置通过相同的保留 pane hit-test 路由，该 hit-test 由 Places 和外部放置使用。

已接受的后备是拖拽预览重绘路径。GPUI 可能在指针移动时继续重绘拖拽预览，即使它不传递底层 pane 在同窗口条目拖拽中的拖拽移动回调。因此 Fika 使用预览渲染 pass 仅作为时钟来查询当前窗口鼠标位置并运行相同的保留 hit test。有效的 smoke 日志可以仅显示 `active-item-move via=preview`；所需信号是移动在放置前到达 `kind=Some(Directory)` 并且当光标在其上时目录条目高亮。

2026-06-17 运行时追踪确认了这一确切路径：pane 自拖拽首先报告 `kind=Some(Pane)`，然后越过目录并报告 `kind=Some(Directory) changed=true` 通过 `via=preview`，无需每条目 `on_drag_move`。这意味着已接受的架构是保留 hit-testing 加上预览驱动 tick，直到 GPUI 暴露公共保留拖拽启动/移动 API 以替换剩余的 shell 边界。

### R4：评估重命名边界

在文本编辑仍然是 GPUI 拥有的平台契约时保持 GPUI 重命名叠加层。自定义重命名渲染器在被接受之前需要行为覆盖：焦点、caret 移动、选择、验证状态、提交/取消和 IME。

具体行为矩阵和 Dolphin 源比较位于 `docs/RENAME_EDITOR_PLAN.md`。

### R5：单独评估 Places 渲染器

Places 是独立于 item-view 的渲染器决策。当前已接受的步骤是 Dolphin 对齐的
chrome 拆分：默认自定义层绘制 row background/drop/insert/trash 状态，而 GPUI
仍负责行文本、图标、事件传递、行右键菜单、行 DnD shell 和拖拽启动 shell。

继续扩展之前：

- 保留滚动、重排、外部放置、条目放置、设备条目、隐藏 section 和右键菜单的
  GPUI fallback 基线
- 在替换 GPUI 事件传递之前先证明 retained Places hitbox
- 只有在 retained/static cache 路径达到或超过 GPUI 时，才把文本或图标迁入自定义绘制

`FIKA_CUSTOM_PLACES_ROWS=1` 仍是 opt-in full-text 基准表面。溢出证据通过
`FIKA_AUTOSMOKE_PLACES=overflow` 可用，它添加非持久仅快照行并验证
`[fika places-scrollbar] visible=1`。Places 分析器通过要求
`[fika places-row-visual] rows` 匹配渲染器策略行计数来拒绝旧的 per-row canvas
形状；默认 chrome gate 还会拒绝 row shape-cache 日志，因为文本必须继续由 GPUI
渲染。

具体保留行设计和 Dolphin 源比较位于 `docs/PLACES_RENDERER_PLAN.md`。

Places 作为 pane 放置悬停的行为参考仍然有用：将 Place 拖到 pane 目录上和将 pane 条目拖到 pane 目录上应在移动时都产生保留的 `Directory` 条目放置目标。

### R6：池复用目标

长期复用池目标仅在可复用视觉 identity 在 GPUI 子 identity 之外拥有时有效：

- Compact/Icons 使用可见 slot id 和保留绘制快照
- 详情使用行绘制快照和形状缓存
- 图像和文本形状缓存是 pane 本地且按 slot/内容键控的
- 渲染器策略日志证明哪些表面保持为 GPUI shell

当前条目视图复用已经遵循这个所有权规则。`VisibleItemSlotPool`
将 `ItemId` 映射到 pane 本地 `slot_id`，通过有界 free-list 回收离屏
slot，并在原始快照变成渲染快照之前分配这些 slot。随后
`ItemPaintSlotCache` 按 `slot_id` 保留 Compact/Icons 的绘制内容、几何和
视觉状态；详情按 `ItemId` 保留行绘制状态。GPUI id 仍然存在于剩余的
shell 表面，但它们是保留 identity 的消费者，不是条目复用的来源。例如
`item_shell.rs` 使用 `("item-slot", slot_id)`，GPUI 主题图标 image 元素也
只用 `slot_id` 稳定当前 GPUI 渲染器表面；可复用条目状态仍然属于 slot 池
和 paint-slot cache。

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
条目，剩余 GPUI 拖拽/image 边界必须在策略计数中保持显式。

此目标可以在拖拽启动和重命名保持在 GPUI 上的同时推进。池边界是保留条目/行状态，而非声称今天每个渲染器都是自定义绘制的。

### R7：完整转换执行顺序

下一个转换工作必须遵循此顺序：

1. 在 Dolphin 对齐的缩放图标视觉更新后冻结当前桌面会话证据。使用 `~/Downloads` 测试普通 MIME/缩略图行为，`/etc` 测试大型混合目录滚动，以及 `FIKA_DEBUG_DND=1` 测试 pane 自拖拽悬停。
2. 在更改渲染器表面之前用证据更新 `docs/ITEM_VIEW_RENDERER_DECISIONS.md`。不要将通过的单元测试视为 DnD、调整大小、全屏或缩放视觉稳定性的足够证据。
3. 从 GPUI 源或经过审计的本地 patch 解决拖拽启动平台边界。仅在保留 hitbox 可以在不丢失 payload、预览、光标偏移或外部放置行为的情况下启动拖拽后，才移除条目/详情拖拽 shell。
4. 将 Places 视为其自己的迁移。它需要 GPUI 基线和一个 Places 特定的保留行绘制器计划，才能进行任何自定义绘制切换。
5. 在自定义编辑器覆盖焦点、caret hit testing、UTF-8 选择、验证、提交/取消、Tab 重命名下一个和 IME 之前，保持重命名为 GPUI 文本编辑边界。
6. 继续收紧复用池证据：普通条目视图帧应显示保留视觉/图像/文本/交互所有权，仅保留显式接受的 GPUI 平台边界。

此顺序的详细任务板位于 `docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md` 中的 P15。

### R8：具体完整转换轨道

已接受的方向是保留/自定义绘制的条目视图，但执行必须保持为证据支持的轨道：

1. **证据轨道**：继续刷新 `~/Downloads` 和 `/etc` 的桌面会话日志，包括调整大小、全屏、滚动、缩放、模式切换和 DnD。这些日志决定渲染器是否保持自定义绘制，而非仅凭架构偏好。对于图像闪烁和缩放尺寸调查，在更改当前图像渲染器之前，包括 `a3f5b0f` 的历史 GPUI 图像基线和转换检查点 `d497593`/`8d1198f`/`36da130`/`b0cac9a`。
2. **绘制器轨道**：继续仅在绘制器消费保留快照且能匹配 Dolphin widget 行为的地方将视觉工作移入内容级绘制器。下一个绘制器工作是图像冷加载/缩放路径的稳定化和测量，而非盲目添加新的视觉表面。
3. **Controller 轨道**：保持点击、菜单、悬停、光标、选择、pane 放置、条目放置和外部放置通过保留 viewport hit testing 路由。GPUI 每条目回调仅是临时的平台桥梁。
4. **Shell 边界轨道**：仅在公共 GPUI 自定义元素拖拽启动 API 或经过审计的本地 GPUI patch 存在后才移除拖拽启动 shell。在行为矩阵覆盖文本输入和 IME 之前保持重命名在 GPUI 上。
5. **Places 轨道**：将 Places 视为单独的渲染器迁移。其 model 和 DnD 状态可以先保留，但 GPUI 渲染器保持，直到 Places 特定的基线和绘制器设计被记录。
6. **所有权轨道**：继续在行为保持时将编排从 `src/main.rs` 提取到 Dolphin 对齐的文件网格模块。这包括角色调度移交、运行时证据助手，以及最终的 shell 边界所有权。

这是"完全转换"的实际含义：每个条目视图行为应由保留 model/布局/controller/painter 状态拥有，而任何剩余的 GPUI 渲染器是具有证据和移除门的显式平台边界。
