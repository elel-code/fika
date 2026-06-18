> 本文是 [FULL_RETAINED_RENDERER_ROADMAP.md](FULL_RETAINED_RENDERER_ROADMAP.md)
> 的简体中文翻译。

# 全面 Retained 渲染器路线图

本文是 Places chrome 默认之后的执行入口。它补充：

- `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.zh-CN.md`：跨 surface 的
  Dolphin 对齐契约和默认提升规则。
- `docs/ITEM_VIEW_CUSTOM_PAINT_DESIGN.zh-CN.md`：架构契约。
- `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.zh-CN.md`：当前替换状态。
- `docs/ITEM_VIEW_CUSTOM_PAINT_TODO.zh-CN.md`：逐切片实现历史。
- `docs/ITEM_VIEW_RENDERER_DECISIONS.zh-CN.md`：各 surface 的渲染器决策。
- `docs/PLACES_RENDERER_PLAN.zh-CN.md` 和
  `docs/RENAME_EDITOR_PLAN.zh-CN.md`：surface 专属计划。

目标是 Dolphin 风格的 retained model/controller/painter 架构，而不是立刻强制
每个像素都自绘。只有当 retained 所有权清晰，且运行时证据证明行为完整、性能持平
或优于 GPUI built-in 路径时，某个 surface 才能切到 custom paint。

## 当前基线

已接受的 retained/custom surface：

- Compact/Icons 的 model、几何、基础视觉、标签、hover/drop/selection、
  thumbnail image layer 和 retained hit testing。
- Details 的 model、几何、行背景、图标、文本单元格、hover/drop/click hit
  testing 和 retained controller routing。
- Places 的 model/projection、slot 统计、目标决策、panel layout 状态，以及默认
  custom row chrome（background/drop/insert/trash）。

显式 GPUI bridge：

- Compact/Icons 和 Details drag start 使用 GPUI `Div::on_drag` shell。
- Rename 使用 GPUI editor overlay。
- Compact/Icons MIME/theme icon 默认使用 GPUI `img()` element。
- Places 文本、图标、事件传递、右键菜单、DnD shell 和 drag start 仍是 GPUI。

这些 bridge 是有意保留的平台或性能边界。只能通过下面的轨道移除。

## Dolphin 完整性诊断

剩余性能差距并不证明全自绘天然慢于 GPUI。它证明的是：部分 surface 还没有形成完整的
Dolphin 风格闭环。

Dolphin item view 快，是因为 `KItemListView` 拥有可见 widget 复用，
`KFileItemModelRolesUpdater` 拥有 visible-first role work，
`KStandardItemListWidget` 只从稳定的本地/全局 cache 绘制。它的
`updatePixmapCache()` 保留 widget-local pixmap，而 `pixmapForIcon()` 用 icon name、
icon height、device pixel ratio 和 mode 组成 cache key。Zoom 立即更新 item geometry，
但昂贵的 preview/role work 会延迟合并。Fika 的 custom image renderer 必须先匹配这个
cache 和 readiness 契约，才能默认替换 GPUI `img()`。

Dolphin Places panel 也是 model/view/delegate 闭环：`DolphinPlacesModel` 拥有
Places state，`KFilePlacesView` 拥有 interaction delivery。Fika Places renderer 只有在
row/section hit testing 和 event delivery 都变成 viewport-level retained state，而不是
per-row GPUI event shell 后，才算 Dolphin-complete。只自绘 row chrome 不是终点。

实际结论：

- Places 和 MIME/theme image 的全自绘仍然是有效目标。
- 路线不是替换 renderer，而是 retained identity、role readiness、image readiness、
  hit-test ownership 和 analyzer-backed default promotion。
- 在某个 surface 闭环完成前，保留 GPUI bridge 是 Dolphin 对齐的选择，不是 retained
  架构的倒退。

详细的跨 surface 契约见 `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.zh-CN.md`。这份文档是后续
“全面自绘”工作的守卫：renderer 只有在 model、layout、controller/hit-test、painter、
cache 和剩余 bridge 边界都显式，并且 analyzer 证据证明 custom path 不弱于 GPUI-backed
path 后，才能成为默认。

## 不可违反的规则

- Model identity、layout identity、selection、drop state 和 worker scheduling
  不得依赖 GPUI element identity。
- Custom paint 是 retained state 上的 renderer policy，不能拥有 file role、
  Places 排序、DnD 决策或 rename 语义。
- Visible-first 工作保持 Dolphin 对齐：可见 role/icon 优先、有界 read-ahead
  在后；scroll/zoom paint 不做同步 theme scan、thumbnail probe、MIME magic read
  或 image decode。
- 如果 custom renderer 在性能、启动稳定性、行为完整性或维护风险上输给 GPUI，
  保留 retained state，但该 surface 继续使用 GPUI，直到迁移被收窄。
- 每个实现切片必须在所属计划或决策文档中记录证据，并且每个完成的切片单独提交。

## 执行轨道

### Track 1：证据冻结

目的：在继续移除 GPUI bridge 前，保持当前桌面会话基线。

可执行清单：`docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.zh-CN.md`。

必需证据：

- `/etc` 和 `~/Downloads` 的 `FIKA_PERF_ITEM_VIEW=1` item-view 日志。
- `/etc` 的 `FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` item-view 日志。
- Details 模式运行时证据，包含 `[fika details-visual]`、
  `[fika details-shape-cache]` 和 retained interaction 计数。
- `FIKA_DEBUG_DND=1` DnD smoke，覆盖 pane item 到 pane 目录、pane item 到
  Places、Places 到 pane 目录，以及外部路径 drop。
- Places 默认 chrome 的 targets、overflow、layout 和 hit-test autosmoke。
- 仅在更改 MIME/theme icon renderer 时，采集默认 GPUI image 路径和
  `FIKA_CUSTOM_THEME_ICONS=1` 的对比日志。

验收：

- 对应 analyzer 全部通过。
- 日志保存在 `/tmp`，并在变更的计划/决策文档中引用。
- 任何用户可见突破或回归都记录症状、根因、Dolphin 对比、实现、证据和未来守卫。

### Track 2：MIME/Theme Icon Renderer

目的：只有在能匹配 Dolphin widget-local pixmap 稳定性后，才将图标渲染自定义化。

详细设计：`docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md`。

下一步设计：

- 定义 retained icon image cache，至少以 `(icon_name, icon_size_px)` 为键，
  必要时加入 theme、scale、color-scheme。
- 刷新期间保留上一次同 key 已加载的真实图像。
- Thumbnail retention 继续按 thumbnail path，而不是 icon name。
- 除非替代方案胜出，否则 GPUI image cache 仍是 decode backend。

默认值只有在以下条件满足后才能改变：

- 默认 GPUI `img()` 和 custom icon renderer 的配对日志在 `/etc` 与混合用户目录通过。
- Custom 日志没有稳态 `theme_placeholder` 抖动、没有 zoom-time `theme_decoded`
  burst、没有可见图标大小二次跳变、没有同步 icon work 回归。
- `docs/ITEM_VIEW_RENDERER_DECISIONS.zh-CN.md` 记录证据。

### Track 3：Places Retained Event Delivery

目的：将 Places 从 GPUI row event shell 迁到 retained hitbox，但不改变文本/图标
renderer policy。

详细设计：`docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.zh-CN.md`。

下一步设计：

- 为 activation、context menu、on-place drop target、insert-before/after、
  sidebar leave clearing 和 cursor state 添加 retained row/section hitbox delivery。
- 在 Track 4 解锁 retained drag start 前，继续保留 GPUI drag-start shell。
- 默认 row chrome 继续自绘，文本/图标继续 GPUI。

默认值只有在以下条件满足后才能改变：

- targets、overflow、layout、hit-test 和 DnD 专属 smoke 通过
  `scripts/analyze-places-perf.sh --expect-retained-event-policy`。
- 右键菜单仍区分空白侧栏、section、bookmark、trash 和 device 行。
- 内部 reorder 和 item/external drop 行为不变。

### Track 4：Drag Start 边界

目的：只有当 GPUI 暴露或 Fika 携带经过审计的 retained-hitbox drag-start API 时，
才移除临时 GPUI drag shell。

下一步设计：

- 当前 GPUI 审计（`0.2.2`，Zed
  `69b602c797a62f09318916d24a98c930533fbdc8`）仍然没有公开的 retained hitbox
  drag-start 钩子。`Interactivity::on_drag` /
  `StatefulInteractiveElement::on_drag` 是 interactive-element API，而
  `Window::insert_hitbox()` 和 `Window::on_mouse_event()` 只提供 retained hit testing
  和鼠标观察。
- 如果使用 GPUI patch，定义最小 API：从 retained hitbox 启动 typed drag，同时保留
  payload、preview entity、cursor offset、accepted transfer modes、cancel、
  同窗口 drop dispatch 和 external drop 行为。该 API 不能要求为了作为拖拽源而重新创建一个
  可见 GPUI row 或 item element。
- 如果不接受 patch，保留 drag-start shell，并继续把它的视觉/identity 作用降到 0。

默认值只有在以下条件满足后才能改变：

- Compact/Icons、Details 和 Places 的 DnD smoke 全部通过。
- Drag preview 在 Compact、Icons、Details 和 Places 的不同窗口大小下位置稳定。
- Renderer-policy 日志显示 shell 移除后 retained interaction 计数不丢失。

### Track 5：Rename Editor

目的：在任何 custom text editor 替换 GPUI overlay 前，保证 rename 行为完整。

下一步设计：

- 将 `docs/RENAME_EDITOR_PLAN.zh-CN.md` 的行为矩阵尽可能转成 runtime 或 unit
  smoke：focus、caret hit testing、UTF-8 selection、commit/cancel、validation、
  Tab rename-next、mouse selection、focus-out 和 IME。

默认值只有在以下条件满足后才能改变：

- Custom 路径覆盖行为矩阵，至少不弱于 GPUI editor。
- Accessibility 和 IME 风险被明确覆盖或接受。
- 如果失败，保留 retained rename draft model，渲染继续使用 GPUI。

### Track 6：Ownership Cleanup

目的：继续将 item-view orchestration 从 `src/main.rs` 移入 Dolphin 对齐的
file-grid 和 Places facade。

下一候选：

- 仍在 app root 中的 runtime evidence helper ownership。
- 可变为 file-grid facade method 的剩余 pane render orchestration。
- Track 3 开始后的 Places event-delivery lifecycle。

验收：

- 没有配对 runtime log，不做行为变更。
- 状态归属哪个模块，invariant 测试就归属哪个模块。
- `src/main.rs` 成为 pane/app state 协调者，而不是 renderer internals 的 owner。

## 下一批队列

1. 按 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md` 实现 retained MIME/theme icon
   image cache 基础。
2. 按 `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.zh-CN.md` 启动 opt-in retained
   Places event layer。
3. 在依赖更新后重新审计 GPUI drag-start API，再进入 Track 4。
4. 在 Track 5 前，把 rename 行为矩阵转为测试/smoke。

该队列有意 evidence-first。它让代码库继续走向完整 retained reuse，同时保持当前规则：
custom paint 只有在该 surface 上至少不弱于 GPUI 路径时，才保留为默认值。
