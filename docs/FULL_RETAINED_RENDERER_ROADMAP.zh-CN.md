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

## 第一优先级：Dolphin 实现模型，GPUI 只负责绘制出口

当前最高优先级是把文件视图和 Places 的热路径全面转向 Dolphin 的实现模型：

```text
model roles -> visible-first role updater -> retained slot/cache -> thin custom painter
```

GPUI `img()` / `image()` 不能继续作为 item image 生命周期、可见范围调度、cache key
或 readiness handoff 的架构中心。它只保留为明确的 bridge、baseline 或 fallback。最终绘制
仍使用 GPUI custom element/canvas 的 `Window::paint_image()`、`paint_quad()` 和文本绘制；
性能收益来自 Dolphin 式生命周期与 cache 边界，而不是来自替换 image 绘制 primitive。

源码级对照：

| 责任 | Dolphin 源码模型 | GPUI `img()` 源码模型 | Fika 第一优先级 |
|---|---|---|---|
| Role/preview 调度 | `KFileItemModelRolesUpdater::startUpdating()` 先 `updateVisibleIcons()`，再 `indexesToResolve()`；`MaxBlockTimeout=200ms`，`ReadAheadPages=5`，`ResolveAllItemsLimit=500` | 每个 `Img` 在 `request_layout()` 中 `source.use_data()`，按 element 生命周期触发 loading/fallback | 建立共享 RoleUpdater/ImageResolver，pane 和 Places 共用 visible-first/read-ahead/size-dpr 失效策略 |
| 图像 cache key | `KStandardItemListWidget::pixmapForIcon()` key = icon name + icon height + DPR + mode | `RetainAllImageCache` key = `Resource` hash；`Img` 自己决定何时使用 cache | theme icon key 必须是 semantic key：icon name + size + DPR + theme + color scheme + mode；thumbnail key 独立 |
| Widget/item 本地状态 | `updatePixmapCache()` 维护 widget-local `m_pixmap`、`m_scaledPixmapSize`、`m_pixmapPos` | `ImgState` 保存 frame/loading state，不知道 item role/read-ahead | retained slot 保存 content/geometry/visual/image/text dirty，paint state 只消费 resolved state |
| 绘制 | `KStandardItemListWidget::paint()` 只画背景、pixmap、text；hover 背景在 `KItemListWidget::paint()` | `Img::paint()` 最终也是 `window.paint_image()` | custom element 只画背景/hover/selection/image/fallback/text/indicator，不做 theme scan、MIME probe 或 decode |
| Places | `DolphinPlacesModel` + `KFilePlacesView` 拥有 model/view/delegate 闭环 | per-row GPUI element 容易把事件和视觉绑回 element identity | Places 与 pane 共享 retained slot、image request、cache/readiness 语义；row shell 只保留明确 bridge |

因此后续优化排序必须改变：先补 Dolphin 式 RoleUpdater、shared image model、bounded retained cache
和 slot dirty state，再考虑删除剩余 GPUI bridge。任何只优化 pane 或只优化 Places 的 image/hover/cache
切片，都必须同时说明另一侧如何复用同一模型。

2026-06-19 实现进展：

- pane 与 Places 已共用 `RetainedImageRequest`、`RetainedImageLoad`、`RetainedImageReady`
  和 `RetainedImageLayerState`。Places 不再有专属 image cache 壳，sidebar keyed state
  直接持有 shared retained image layer。
- theme icon ready 事件现在跟随 shared load result 产生；Compact/Icons、Details 和
  Places 都消费同一 ready 语义，不再各自推导 image readiness。
- thumbnail retained fallback 已从无界 `HashMap` 改为按字节预算的 LRU cache；驱逐时同步
  移除 GPUI `RetainAllImageCache` resource 并 `drop_image`。
- Dolphin `ReadAheadPages=5` / `ResolveAllItemsLimit=500` 的 role-updater 索引顺序已移到
  `ui::retained::work_order`，thumbnail deferred work 和 file-icon resolve 不再各自维护
  分叉顺序。
- Static item label、Details cell/header 和 Places row label 现在共用
  `RetainedShapeCache` 与 `TextShapeCacheStats`。各 surface 仍然拥有自己的 text key 和
  shape 函数，但 cache hit/miss/evict 语义已经是 retained 层代码，不再由 pane/Places
  各自复制。
- Places slot projection 现在包装 `RetainedSlotStats`，与 item-view slot delta accounting
  使用同一 retained 语义，同时保留 Places 专属 rows/sections 计数。
- thumbnail/theme image 的直接 load helper 已收回到 `RetainedImageLayerState` 私有实现；
  pane、Details 和 Places 必须通过 `RetainedImageRequest` 入口。
- 最终 core evidence 已全绿。`scripts/run-retained-renderer-evidence.sh
  --core --skip-build --prefix fika-core-final-retained-v3` 完成并输出
  `retained renderer evidence complete`。item 日志覆盖 Compact、Icons 和 Details
  （`/tmp/fika-core-final-retained-v3-item-etc-zoom-scroll.log`、
  `/tmp/fika-core-final-retained-v3-item-etc-icons-zoom-scroll.log`、
  `/tmp/fika-core-final-retained-v3-item-etc-details-zoom-scroll.log`）：warm steady
  max total 为 `1108us`，file-grid max build 为 `1344us`，image max paint 为
  `373us`，warm static visual max paint 为 `2546us`，warm custom/details visual
  max paint 为 `3309us`。Renderer policy 保持 retained：
  `gpui_image_element=0`、`gpui_directory_drop_shell=0`、`gpui_details_header=0`。
- 最终 Places 日志
  （`/tmp/fika-core-final-retained-v3-places-targets.log`、
  `/tmp/fika-core-final-retained-v3-places-overflow.log`、
  `/tmp/fika-core-final-retained-v3-places-layout.log`、
  `/tmp/fika-core-final-retained-v3-places-hit-test.log`、
  `/tmp/fika-core-final-retained-v3-places-targeting.log`、
  `/tmp/fika-core-final-retained-v3-places-dnd.log`）在默认 full policy 下通过：
  `visual_kind=full`、`row_gpui=0`、`text_gpui=0`、`icon_gpui=0`。
- Fika 现在维护专用 GPUI fork/branch 来承载 retained-hitbox typed DnD：
  `ssh://git@github.com/elel-code/zed.git` 的 `fika/gpui-hitbox-dnd`，
  pinned revision 为 `572d53326f722e5634647b2276c42069d6b5b63d`。Fika 的
  `gpui` 和 `gpui_platform` 都固定到这个 revision。
- fork 暴露 hitbox-level typed drag/drop 注册。Pane、Details 和 Places drag
  start 现在注册在 retained hitbox 上，而不是每 item/row 的 `Div::on_drag`
  shell。Places DnD move/drop target delivery 也注册在 retained sidebar content
  hitbox 上。
- 当前 gate 要求 GPUI DnD shell 为零：`gpui_drag_shell=0`、`drag_shells=0`、
  `gpui_typed_dnd_payload_shells=0`，Places retained DnD 日志必须通过
  `--expect-retained-event-policy`。

## 当前基线

已接受的 retained/custom surface：

- Compact/Icons 的 model、几何、基础视觉、标签、hover/drop/selection、
  thumbnail image layer 和 retained hit testing。
- Details 的 model、几何、行背景、图标、文本单元格、hover/drop/click hit
  testing 和 retained controller routing。
- Places 的 model/projection、slot 统计、目标决策、panel layout 状态，以及默认
  custom row chrome（background/drop/insert/trash）。

显式 GPUI bridge：

- Rename 使用 GPUI editor overlay。
- Compact/Icons MIME/theme icon 默认使用 full custom image layer。Painter 仍复用
  GPUI 高效的 `RetainAllImageCache -> RenderImage -> Window::paint_image`
  后端，但普通 pane 渲染路径不再保留逐 item 的 GPUI `img()` 子元素。
  `FIKA_GPUI_THEME_ICONS=1` 是明确的旧 GPUI baseline，
  `FIKA_HYBRID_THEME_ICONS=1` 只保留为过渡 readiness-handoff 路径。
- Places 默认使用 full custom row visual 绘制背景、文本和图标；图标 image load/cache/readiness
  直接使用 shared retained image layer。Places row/section activation、context-menu
  targeting、DnD target lookup、drop dispatch 和 sidebar leave clearing 默认已经通过
  retained hitbox。默认 Places 路径没有 GPUI row/section event shell、没有 sidebar
  typed payload shell，也没有 GPUI row drag-start shell。

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
Places state，`KFilePlacesView` 拥有 interaction delivery。Fika 现在已经具备
row visual、row/section hit testing、targeting、drag start、typed DnD payload
delivery 和 drop dispatch 的 Dolphin-complete Places path：默认路径是 full row
visual 加 retained-hitbox DnD，并且 GPUI DnD shell 为零。

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
- 当修改 Places full-row visual policy、text-shape handoff 或默认提升阈值时，使用
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff` 采集
  Places 默认 chrome 与 full handoff 的 A/B 日志。
- 仅在更改 MIME/theme icon renderer 时，采集默认 full custom image 路径和
  `FIKA_GPUI_THEME_ICONS=1` 的对比日志。

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

默认值现在是 retained image model 上的 full custom。未来 icon renderer 变更必须继续满足：

- 默认 full custom 与 `FIKA_GPUI_THEME_ICONS=1` baseline 的配对日志在 `/etc` 与混合用户目录通过。
- 默认/custom 日志没有稳态 `theme_placeholder` 抖动、没有 zoom-time
  `theme_decoded` burst、没有可见图标大小二次跳变、没有同步 icon work 回归。
- `docs/ITEM_VIEW_RENDERER_DECISIONS.zh-CN.md` 记录证据。

### Track 3：Places Retained Event Delivery

目的：将 Places 从 GPUI row event shell 迁到 retained hitbox，但不改变文本/图标
renderer policy。

详细设计：`docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.zh-CN.md`。

当前默认：

- `retained-dnd` 通过 retained Places geometry 拥有 row/section activation、
  context-menu targeting、on-place drop target、insert-before/after、drop dispatch、
  sidebar leave clearing 和 cursor state。
- Retained hitbox DnD 由 Fika GPUI fork 提供，并在没有 sidebar-level GPUI payload
  bridge 的情况下拥有 typed payload move/drop delivery。
- Places drag start 注册在 retained row hitbox 上，不再注册在 GPUI row shell 上。
- 默认 row visual 是 full custom；文本、图标和 section heading 都由 Fika 绘制。

默认值只有在以下条件满足后才能改变：

- targets、overflow、layout、hit-test 和 DnD 专属 smoke 通过
  `scripts/analyze-places-perf.sh --expect-retained-event-policy`。
- 右键菜单仍区分空白侧栏、section、bookmark、trash 和 device 行。
- 内部 reorder 和 item/external drop 行为不变。

### Track 3a：Places Full Row Visual Default

目的：让 Places row/section visual 与 pane item 使用同一 Dolphin retained 模型：
共享 retained image request、image readiness/cache、text-shape cache 机制、retained
slot stats，以及薄 row visual painter。

当前默认：

- `DEFAULT_PLACES_ROW_VISUAL_POLICY = CustomFull`。
- Places row text、section text 和 icon 都由 custom row visual layer 绘制。
  `FIKA_PLACES_ROW_VISUAL_POLICY=gpui`、`chrome`、`text` 只保留为明确 fallback/A-B policy。
- `FIKA_PLACES_ROW_VISUAL_HANDOFF=1` 仍可作为 ready-only handoff 的回归 suite；
  它已经不是 full rows 成为默认值的前置条件。

2026-06-19 最终证据：

- core runner 在默认 full policy 下通过 Places targets、overflow、layout、hit-test、
  targeting 和 DnD 日志：`/tmp/fika-core-final-retained-v3-places-*.log`。
- analyzer summary 显示 `visual_kinds=full`、row visual layer 计数匹配 rows、
  `row_gpui=0`、`text_gpui=0`、`icon_gpui=0`。
- Interaction 对 row/section target delivery 和 typed payload delivery 都保持
  retained-DnD。完成 gate 现在要求 `gpui_event_shells=0`、
  `gpui_row_section_event_shells=0`、`gpui_typed_dnd_payload_shells=0`、
  `drag_shells=0` 和 `drag_start_models=rows`。

决策：

- Places full row visual 对 retained renderer transition 来说已经完成，并保持默认。
- Places retained event delivery 和 typed DnD shell 移除已经在 Fika GPUI fork 上完成。
  后续 Places renderer 工作是针对 chrome/GPUI fallback policy 的回归监控，以及让 fork
  patch 紧跟 upstream GPUI。

### Track 4：Typed Drag 边界

目的：在 Fika 把 retained-hitbox typed DnD 作为主路径使用时，保持专用 GPUI patch
小、可审计，并与 upstream GPUI 同步。

当前实现：

- fork branch：`git@github.com:elel-code/zed.git` 的 `fika/gpui-hitbox-dnd`，
  pinned revision 为 `572d53326f722e5634647b2276c42069d6b5b63d`。
- 新增 GPUI API：`Window::on_hitbox_drag`、
  `Window::on_hitbox_drag_with_cursor`、`Window::on_hitbox_drag_move` 和
  `Window::on_hitbox_drop`。
- Fika 从 retained hitbox 注册 item、Details row 和 Places row drag start，使用稳定
  element id/global id。Places typed move/drop handler 注册在 retained sidebar content
  hitbox 上。
- 不得为了 typed DnD 重新引入可见或拥有布局的 GPUI row/item `Div`。

维护 gate：

- Compact/Icons、Details 和 Places 的 DnD smoke 全部通过。
- Drag preview 在 Compact、Icons、Details 和 Places 的不同窗口大小下位置稳定。
- Renderer-policy 日志保持 `gpui_drag_shell=0`，且 retained interaction 计数不丢失。
- Places full retained-event 日志通过，并显示 `gpui_event_shells=0`、
  `gpui_row_section_event_shells=0`、`gpui_typed_dnd_payload_shells=0`、
  `drag_shells=0` 和 `drag_start_models=rows`。
- GPUI fork patch 保持最小；当 upstream GPUI drag/drop internals 变化时 rebase 或
  forward-merge。

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
- 仍在 Places renderer facade 外部的 Places full-handoff 证据和默认提升 helper。

验收：

- 没有配对 runtime log，不做行为变更。
- 状态归属哪个模块，invariant 测试就归属哪个模块。
- `src/main.rs` 成为 pane/app state 协调者，而不是 renderer internals 的 owner。

## 下一批队列

1. 保持 retained MIME/theme icon image cache 的 full-custom 默认，并在未来 image
   变更时与 `FIKA_GPUI_THEME_ICONS=1` 对比。
2. 将 `--places-full-handoff` 作为 chrome/full 回归 suite 保留，而不是默认提升 blocker。
3. 在 upstream GPUI 依赖更新后，让 Fika GPUI retained-hitbox typed DnD fork
   rebase 或 forward-merge。
4. 在 Track 5 前，把 rename 行为矩阵转为测试/smoke。

该队列有意 evidence-first。它让代码库继续走向完整 retained reuse，同时保持当前规则：
custom paint 只有在该 surface 上至少不弱于 GPUI 路径时，才保留为默认值。
