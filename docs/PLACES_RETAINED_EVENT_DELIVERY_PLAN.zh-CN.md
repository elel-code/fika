> 本文是 [PLACES_RETAINED_EVENT_DELIVERY_PLAN.md](PLACES_RETAINED_EVENT_DELIVERY_PLAN.md)
> 的简体中文翻译。

# Places Retained 事件传递计划

本文是 `docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md` 中 Track 3 的历史 GPUI 基线证据。
它只覆盖事件传递，不定义新的 winit/wgpu shell 路线。当前状态：Places full row visual、
retained event delivery、typed DnD move/drop 和 drag start 已经在 Fika GPUI fork 上完成。
本文较早的 mixed-policy 段落是历史实现记录，以“当前状态”和“TODO”中的完成态为准。

## Dolphin 边界

Dolphin Places 面板使用 `KFilePlacesView` 加 `DolphinPlacesModel`。View 拥有用户交互，
model 和 Dolphin action 层拥有 model/order/device 决策。renderer/delegate 不拥有 Places
排序、设备状态、context-menu 语义或 drop 规则。

Fika 的等价边界是：

- `places/model.rs`、`places/user/*` 和 app command 拥有 Places 数据和变更。
- `places/projection.rs` 拥有投影后的 row state。
- `places/interaction.rs` 拥有 row/section 几何、hit testing、drop-zone 映射和目标决策。
- retained event layer 可以传递 pointer 和 DnD 事件，但必须调用现有 app 方法执行
  activation、context menu、drop 和 cursor update。
- Drag start 和 typed DnD move/drop 通过 Fika GPUI fork 注册在 retained hitbox 上。
  不得重新引入 GPUI row shell 作为交互所有者。

## 当前状态

已实现：

- `places_interaction_geometry()` 提供 retained row/section geometry。
- `PlacesInteractionGeometry::hit_test_y()` 提供 retained row/section hit test。
- item/external path drop 和 place reorder 的 retained target-decision helpers。
- 显式 GPUI event-shell fallback 和当前 full retained event policy 的 analyzer 支持。
- 显式的 `PlacesEventDeliveryPolicy`。默认现在是 `RetainedDnd`。
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` 保留为显式 fallback。
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe` 只报告未来 retained layer
  需要覆盖的 row/section hitbox 计数；它仍保持 `retained_hitboxes=0` 和
  `gpui_event_shells=rows+sections`。
- 默认 full custom row visual，同时 retained layer 承担 row/section activation、
  context-menu、DnD target delivery、typed payload move/drop 和 row drag start。

默认 policy 形状：

```text
event_policy=retained-dnd
retained_hitboxes=rows+sections
retained_interaction=rows+sections
gpui_event_shells=0
gpui_row_section_event_shells=0
gpui_typed_dnd_payload_shells=0
drag_shells=0
drag_start_models=rows
```

`drag_start_models=rows` 记录 payload、movable flag、export metadata 和 preview model
由 Places drag 模块拥有；drag source 注册基于 retained hitbox，`drag_shells` 必须为 0。

## 历史实现记录

下面的 phase notes 记录迁移到当前 retained-hitbox 实现之前的切片。凡是提到
`gpui_event_shells=1`、`drag_shells=rows`、sidebar typed payload bridge 或 row
drag-start shell 的段落，除非后续条目另有说明，均指 fork 落地前的中间状态。

## Retained Event Layer

添加一个 sidebar-level retained event layer 覆盖 scroll content，不要为每个 row
创建一个 GPUI event element。它应消费 row visual layer 使用的同一份 `PlaceSnapshot`
列表，并创建：

- row hitbox records：visible index、place index、path、mounted/device/network state、
  label、device id、group、y/height、insert indexes、movable flag；
- section hitbox records：group、insert index、y/height；
- content height 和 scroll-local 坐标转换；
- perf policy 日志所需的 event counters。

如果 GPUI 支持 retained hitbox，应使用 `Window::insert_hitbox()` 和
`window.on_mouse_event()`。如果某类 GPUI event 无法从 retained hitbox 传递，就保持该事件在
row shell 上，并明确报告 mixed policy，不要声称已经 retained event delivery。

坐标规则：

```text
window position -> layer bounds -> content-local y -> PlacesInteractionGeometry::hit_test_y()
```

content-local y 必须包含当前 scroll offset。event layer 必须共享 `places_sidebar` 使用的
scroll handle 或等价 scroll snapshot。

## 迁移阶段

### Phase 1：非变更 Retained Pointer Layer

添加 opt-in retained event layer，只做 hit testing 和 policy 计数，不变更 app state。

验收：

- 可以在 opt-in policy 下记录 `retained_hitboxes=rows+sections`。
- GPUI row shell 仍拥有 activation、context menu 和 DnD。
- 当前 GPUI 和 opt-in custom row chrome 的 hit-test autosmoke 仍通过。
- 没有用户可见行为变化。

### Phase 2：Hover、Cursor 和 Leave Clearing

把 hover/cursor state 和 sidebar leave clearing 移到 retained event layer。这是最低风险的
变更步骤，因为它不会激活 place、打开菜单或执行 drop。

验收：

- Row body 和 insert-edge cursor 决策与现有 GPUI row DnD 逻辑一致。
- 离开 sidebar 会为 item、external path 和 place drag 清除 row/section drop targets。
- 当前 GPUI 和 opt-in custom visual policy 都通过 interaction geometry 和 targets autosmoke。

### Phase 3：Activation 和 Context Menu Targeting

把左键 activation 和右键 target selection 移到 retained hitboxes。保留现有 app context-menu
方法和 GPUI menu 渲染。

验收：

- 普通 place activation 仍把 path、device id、label、mounted、device、network flags 传给
  `activate_place()`。
- Context menu 仍区分空白 sidebar、section header、bookmark、trash、device、network 和
  mounted/unmounted rows。
- Row/section content 之外仍可以打开空白 sidebar context menu。
- GPUI row shell 不再拥有 click 或 context-menu callbacks。

### Phase 4：Drag Move 和 Drop Delivery

使用现有 target-decision helpers 和 app drop methods，把 item/external path 和 place-drag
move/drop delivery 移到 retained hitboxes。

验收：

- Item/external path drops 保留 insert-before、insert-after 和 on-place 行为。
- Place reorder 保留 no-op rejection 和 source-index adjustment。
- Place-to-pane drag 不变。
- Drop 仍使用当前 mouse position 放置 menu/action。
- targets、overflow、layout、hit-test 和 DnD-specific smoke 通过 analyzer
  `--expect-retained-event-policy`。

### Phase 5：移除 GPUI Row/Section Event Shells

Phases 1-4 在默认 chrome 和 GPUI fallback visual policy 下都通过后，移除 row/section event
callbacks。只保留 row drag-start shells。

验收：

- Policy logs 显示 `retained_interaction=rows+sections`、
  `retained_hitboxes=rows+sections`、`gpui_event_shells=0`、`drag_shells=rows`。
- `scripts/analyze-places-perf.sh --expect-retained-event-policy` 通过。
- DnD smoke 覆盖 item-to-place、external-to-place、place reorder、
  place-to-pane directory 和 sidebar leave clearing。

## Analyzer 和 Smoke 工作

Phase 4 默认提升前，添加或扩展 smoke：

- retained hover/cursor/leave clearing；
- activation 和 context-menu target selection；
- 使用隔离 user-place config 的 DnD-specific retained event delivery；
- 非零 scroll offset 下的 overflow hit testing；
- `FIKA_PLACES_ROW_VISUAL_POLICY=gpui`、默认 `chrome` 和 opt-in `full` 的一致性。

现有 analyzer 已经会拒绝虚假的 retained-event 声明，不要放宽 gate。只有新增 retained
event 日志 surface 时才扩展 analyzer。

2026-06-18 policy-probe 切片：

- `src/ui/places/perf.rs` 拥有 `PlacesEventDeliveryPolicy`，与 item view 的
  renderer-policy 模式一致。
- `retained-probe` 有意不接受 `RetainedHitboxes` 或 `retained` 作为别名；名称本身
  保持 mixed state 显式。
- `[fika places-renderer-policy]` 和 `[fika places-interaction-policy]` 现在包含
  `event_policy=...` 和 `retained_probe_hitboxes=...`。
- `scripts/check-places-perf-analyzer.sh` 证明 probe 仍通过当前 GPUI-shell
  interaction boundary，并且不能通过 `--expect-retained-event-policy`。
- activation、menu、hover、drop、DnD 或 drag-start 行为都没有改变。

2026-06-18 retained probe layer 切片：

- `src/ui/places/event_layer.rs` 添加 opt-in sidebar-level event probe layer，
  由 `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe` 开启。
- 该 layer 消费 `PlacesInteractionGeometry`，为每个 retained row/section 插入一个
  normal GPUI hitbox；它不注册 event handler、不设置 cursor state，也不修改 app state。
- `[fika places-event-probe]` 报告 `rows`、`sections`、插入的 `hitboxes`、
  hovered hitboxes，以及 prepaint/paint 时间。
- `scripts/analyze-places-perf.sh --require-event-probe` 验证 layer hitbox 数匹配
  retained-probe policy 计数。
- 这只是 Phase 1 结构层。Phase 2 仍负责把 hover/cursor/leave clearing 从 GPUI shell
  移出。

2026-06-18 retained pointer 切片：

- `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-pointer` 启用同一个 sidebar-level retained
  layer，但现在会根据 retained row hitbox 为可激活行设置 pointing-hand cursor。
- 在该 policy 下，每行 GPUI cursor style 被关闭；click、context menu、typed DnD move/drop
  和 drag start 仍留在 GPUI row/section shell。
- retained layer 也会观察 active mouse-drag movement，并在 pointer 离开 retained layer
  bounds 时清理当前 Places drop target。现有 GPUI typed drag handler 保持为 fallback，
  直到 Phase 4。
- `[fika places-event-probe]` 在该 policy 下包含 `pointer=1`。完整 retained-event
  analyzer gate 仍会拒绝它，因为 `retained_hitboxes=0` 且
  `gpui_event_shells=rows+sections`。

2026-06-18 retained targeting 切片：

- `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-targeting` 继续扩展同一个
  sidebar-level retained layer，让 row activation 和 row/section context menu
  targeting 由 retained layer 拥有。
- retained layer 使用已插入的 row/section hitbox 以及 `Hitbox::is_hovered()` 进行
  dispatch，而不是从原始 scroll offset 重新计算 pointer 位置。这与 Dolphin 方向一致：
  viewport event layer 负责目标查找，model/controller 方法继续负责 activation 和 menu state
  change。
- 在该 policy 下，GPUI row `on_click`、row right-click 和 section right-click shell
  被关闭。GPUI row/section shell 仍拥有 typed DnD move/drop，row shell 也仍拥有
  drag-start。
- `[fika places-event-probe]` 包含 `pointer=1 targeting=1`，并且
  `[fika places-interaction-policy]` 包含 `retained_targeting=rows+sections`。
  完整 retained-event analyzer gate 仍会拒绝该 mixed state，因为
  `gpui_event_shells=rows+sections`。

2026-06-18 retained DnD 切片：

- `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-dnd` 继续使用同一个 retained layer，
  并将 typed item、external-path 和 place drag move/drop 的目标查找移到 retained
  `PlacesInteractionGeometry`。
- GPUI 公开 API 目前仍只能通过 `Div::on_drag_move` 和 `Div::on_drop` 取得 typed drag
  payload，因此该切片有意使用一个 sidebar-level GPUI typed drag shell，而不是
  row/section shell。这里与 Dolphin 对齐的是目标查找和状态转换：viewport event layer
  拥有 hit testing，model/controller 方法拥有 drop target 和 drop execution。
- 在该 policy 下，row/section DnD move/drop shell 被关闭。row drag-start shell 仍保留，
  因为 GPUI 仍通过 `Div::on_drag` 启动 app 内部 drag。
- `[fika places-interaction-policy]` 报告 `retained_dnd=rows+sections` 和
  `gpui_event_shells=1`。`[fika places-event-probe]` 报告
  `pointer=1 targeting=1 dnd=1`。完整 retained-event analyzer gate 仍拒绝该状态，
  因为 `gpui_event_shells=1` 且 `drag_shells=rows`。

retained DnD autosmoke 切片：

- `FIKA_AUTOSMOKE_PLACES=dnd` 现在可以在不改写用户 Places 排序、不写 bookmark 的
  前提下验证 retained Places DnD target decision。
- 该 smoke 会采样 path-list drag 经过 row body、row edge、section heading，再采样
  place drag 经过另一个 row。预期 retained decision 分别是 `Place`/`DropMenu`、
  `Insert`/`Copy`、`Insert`/`Copy` 和 `Insert`/`Move`。
- `scripts/analyze-places-perf.sh --require-retained-dnd-autosmoke` 会拒绝缺失
  start/complete marker、缺失采样覆盖、失败采样决策，或没有同时包含 row 和 section
  geometry 的 summary。这给后续 drag-start / GPUI shell 移除切片提供了非破坏性回归守卫；
  真正执行 reorder/drop 的 destructive smoke 仍需隔离配置后再添加。

2026-06-19 retained DnD clear-path autosmoke 切片：

- `FIKA_AUTOSMOKE_PLACES=dnd` 现在还会采样一次位于 retained Places content geometry
  外的 path-list drag。预期 decision 是 `Clear`，cursor 是 `NotAllowed`。
- `scripts/analyze-places-perf.sh --require-retained-dnd-autosmoke` 现在要求
  `path-outside` 样本存在。这避免 smoke 只证明 row/section 的正向 target，却漏掉拖拽中防止
  Places 高亮残留所需的 no-target 路径。
- 这仍然是非破坏性 smoke，不能替代手动 `FIKA_DEBUG_DND=1` bounds trace。GUI trace
  仍然是证明 sidebar typed payload bridge 在 pane-internal drag 中拒绝 bounds 外
  capture-phase drag move，并且只清 Places state 的证据。

retained drag-start source-model 切片：

- 本地 GPUI 源码在 Zed commit
  `69b602c797a62f09318916d24a98c930533fbdc8` 仍然只通过
  `Interactivity::on_drag` / `StatefulInteractiveElement::on_drag` 暴露 typed drag
  启动；retained hitbox 没有公开 typed drag-start API。
- 因此 row shell 仍保留为平台 drag-start 触发器，但 `places/drag.rs` 现在拥有从
  `PlaceSnapshot` 投影 `PlaceDragStartSource` 的逻辑。该投影在安装 GPUI shell 前决定
  path、label、icon、source index、movable flag、export payload 和 preview model。
  shell 安装本身也通过 `install_place_drag_start_shell()` 集中，因此 row construction
  不再拥有 preview creation 或 drag-start payload wiring。
- `[fika places-interaction-policy]` 报告 `drag_start_models=rows`，并且
  `scripts/analyze-places-perf.sh --require-interaction-policy` 会拒绝 drag-start model
  数量与可见 row 数不一致的日志。这在保留平台 shell 的同时保持 Dolphin
  model/controller 边界显式。

retained content-y conversion test 切片：

- `places_content_y_from_viewport_y()` 现在拥有未来 viewport-local y 加 scroll offset
  后进入 `PlacesInteractionGeometry::hit_test_y()` 的转换规则。当前 retained event layer
  位于 scroll content 内，因此传入 zero scroll，但该转换已为未来 viewport-level layer
  显式化。
- 单元覆盖证明非零 scroll 会把 viewport y 映射到预期 row 或 section，并证明
  row/section/content bounds 使用半开区间。这能防止后续移动 event layer 时出现
  row/section target off-by-one 回归。

retained hitbox accounting 切片：

- `retained_probe_hitboxes` 继续报告 opt-in retained policy 插入的 retained layer
  hitbox 数。
- `retained_hitboxes` 现在只在这些 hitbox 承载 retained target delivery 时报告
  rows+sections，也就是 `retained-targeting` 和 `retained-dnd`。probe 和 pointer-only
  policy 仍报告 `retained_hitboxes=0`。
- 完整 retained-event gate 不变：仍要求 `gpui_event_shells=0` 和
  `drag_shells=rows`，因此 mixed retained-targeting 和 retained-dnd 状态仍会被拒绝。

retained interaction policy accounting 切片：

- renderer policy 日志现在会在 `retained-targeting` 和 `retained-dnd` 下报告
  `retained_interaction=rows+sections`，因为这些 policy 中 retained layer 已经实际拥有
  row/section activation、context-menu targeting、DnD target lookup 和 drop dispatch。
- probe 和 pointer-only policy 仍报告 `retained_interaction=0`，因为它们不拥有 target
  delivery。
- custom row visual analyzer gate 现在按所选 event policy 校验 `retained_interaction`，
  不再假设每次 custom chrome/full visual 运行都仍是 GPUI event ownership。完整
  retained-event gate 没有放松：只要 `retained-dnd` 还存在剩余 typed GPUI DnD shell，
  它仍会被拒绝。

retained targeting autosmoke 切片：

- `FIKA_AUTOSMOKE_PLACES=targeting` 现在会输出非变更 retained targeting 采样，覆盖
  activation-row、row context-menu 和 section context-menu target classification。
- 该 smoke 消费 retained event layer 使用的同一份 `PlacesInteractionGeometry`，不会真正
  activate place 或打开菜单。它证明的是 retained event handler 依赖的 target
  classification 层，为后续默认 policy 提升提供回归守卫。
- `scripts/analyze-places-perf.sh --require-retained-targeting-autosmoke` 会拒绝缺失
  marker、失败采样，或没有同时包含 row 和 section 的 summary。

默认 retained-DnD promotion 切片：

- Places event delivery 现在默认使用 `retained-dnd`，也就是当前证据覆盖最强的 mixed
  policy。默认路径移除了 per-row/section GPUI activation、context-menu 和 DnD target
  shell，但仍保留 GPUI 文本/图标渲染、一个 sidebar-level typed DnD payload shell 和 row
  drag-start shell。
- `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` 仍可显式回退到旧 row/section event-shell 路径。
- full retained-event analyzer gate 不变，仍会因为默认 mixed policy 的
  `gpui_event_shells=1` 拒绝它。

retained sidebar leave shell 移除切片：

- 默认 retained-DnD 现在依赖 retained pointer layer 做 active-drag leave clearing，
  不再安装 item、external-path 和 place drag 三个 root sidebar GPUI `on_drag_move`
  leave-clear shell。
- `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` 和 `retained-probe` 仍会安装这些 GPUI
  leave shell，因为它们不拥有 retained pointer movement。
- `[fika places-interaction-policy]` 在 retained-pointer、retained-targeting 和
  retained-DnD 下报告 `gpui_sidebar_leave_shells=0`，在 GPUI/probe fallback policy
  下报告 `3`。analyzer 会拒绝重新引入这些 shell 的 retained-DnD 日志，同时 full
  retained-event gate 仍保持严格，因为 sidebar typed DnD payload shell 还存在。

剩余 shell accounting 拆分切片：

- `[fika places-interaction-policy]` 现在把原本重载的 `gpui_event_shells` 拆成
  `gpui_row_section_event_shells` 和 `gpui_typed_dnd_payload_shells`。
- 默认 retained-DnD 应报告 `gpui_row_section_event_shells=0` 和
  `gpui_typed_dnd_payload_shells=1`：row/section target delivery 已经 retained，
  但 GPUI 仍拥有 sidebar-level typed drag payload 入口。
- GPUI/probe/pointer/targeting fallback 状态仍报告
  `gpui_row_section_event_shells=rows+sections` 和
  `gpui_typed_dnd_payload_shells=0`。
- full retained-event gate 仍保持严格，现在会验证这两个拆分计数都为 0。默认
  retained-DnD mixed state 因此会明确失败在 typed payload shell，而不是一个含糊的
  event-shell 总数。

typed payload bridge 审计：

- 默认 retained-DnD 路径已经从 target delivery 中移除了 row/section GPUI event
  callbacks。`gpui_row_section_event_shells=0` 表示 activation、row/section context-menu
  targeting、item/external-path drag target lookup、place reorder target lookup、drop
  target state 和 drop dispatch 都由 retained Places event layer 与
  `places/interaction.rs` 拥有。
- 剩余的 `gpui_typed_dnd_payload_shells=1` 是一个 sidebar-level typed payload bridge，
  安装点是 `src/ui/places/event_layer.rs::install_places_event_dnd_handlers()`。它存在的原因
  是当前 GPUI 公共 drag/drop API 只能通过 interactive element
  (`Div::on_drag_move` / `Div::on_drop`) 暴露 typed `ItemDrag`、`ExternalPaths` 和
  `PlaceDrag` move/drop payload，而不是通过 retained painter hitbox 暴露。
- 这个 bridge 不能重新膨胀成 row/section ownership。它只能解析 typed payload、读取当前鼠标
  位置，并调用已经拥有 target decision 与 drop execution 的 retained geometry/controller
  路径。
- 只有当 GPUI 暴露 typed retained-hitbox drag move/drop delivery，或 Fika 携带经过审计的
  最小 patch，并且能提供同样 payload 类型时，才能移除此 bridge。替代路径必须保留同窗口 item
  drops、external path drops、place reorder/drop、place-to-pane drag 行为、cursor update、
  leave clearing，以及基于当前位置的 drop menu 放置。
- 移除 gate 是：

```text
scripts/analyze-places-perf.sh --expect-retained-event-policy ...
FIKA_AUTOSMOKE_PLACES=dnd ...
future isolated destructive drop/reorder smoke with a temporary Places config
```

- GPUI 依赖更新后，在改变该 bridge 前应重新搜索本地 GPUI checkout，确认是否已经出现
  retained hitbox drag/drop payload API：

```sh
rg -n "on_drag_move|on_drop|insert_hitbox|DragMoveEvent|DropEvent|ExternalPaths|PlaceDrag|ItemDrag" ~/.cargo/git/checkouts/zed-* src
```

2026-06-19 typed payload API 审计切片：

- 当前 `Cargo.lock` 将 GPUI 解析到 Zed commit
  `69b602c797a62f09318916d24a98c930533fbdc8`。
- 在该 checkout 中，`crates/gpui/src/elements/div.rs:63` 定义
  `DragMoveEvent<T>`，`div.rs:315` 暴露 `Interactivity::on_drag_move<T>()`，
  `div.rs:525` 暴露 `Interactivity::on_drop<T>()`。这些 API 绑定在 interactive
  element hitbox 上，并通过 `cx.active_drag` 接收 typed payload。
- retained hitbox surface 仍然是分开的：
  `crates/gpui/src/window.rs:4243` 暴露 `Window::insert_hitbox()`，
  `window.rs:4360` 暴露 `Window::on_mouse_event<Event: MouseEvent>()`。这两个 API 都没有为
  retained painter hitbox 提供 typed drag payload callback 或 drop payload callback。
- 结论：sidebar typed DnD payload bridge 仍然必要。下一条有效代码路径要么是设计
  retained-hitbox typed drag-move/drop delivery 的 GPUI API/patch，要么继续把 bridge
  限制为一个 sidebar-level shell，同时保持 row/section target delivery retained。

2026-06-19 drag-time target isolation 切片：

- 根本原因：GPUI typed `on_drag_move` handler 在 capture dispatch 阶段运行，且不会自动被
  element bounds 裁剪。retained Places bridge 是 sidebar-level typed shell，因此必须在把
  `event.position.y` 转成 Places local row geometry 之前，显式拒绝 pointer 位于 Places
  layer bounds 之外的 drag-move event。
- 如果缺少该 bounds gate，pane 内部 item drag 在 pointer 的 y 坐标与某个 Places row 重叠时，
  仍可能进入 Places retained-DnD 路径，从而在 pane drag 过程中错误显示 Places drop
  highlight。
- 修复点位于 `install_places_event_dnd_handlers()`：每个 typed drag-move handler 先检查
  `event.bounds.contains(&event.event.position)`。如果为 false，只清 retained Places DnD
  target 和 `place_drop_target`，然后直接返回且不 stop propagation。这里不能调用
  `clear_drag_drop_targets()`，因为同一个 drag frame 里 pane-owned item target 可能正在生效。
- 回归规则：后续任何替换 sidebar typed bridge 的实现都必须保留 target isolation。Pointer 在
  Places 外只清 Places state；pointer 在 Places 内才可以拥有 Places target decision 并 stop
  propagation；pane preview/window drag tracking 继续负责 pane item target。

2026-06-19 pane-ownership defer 切片：

- bounds gate 之后还需要第二层隔离：sidebar-level typed payload bridge 仍可能收到
  capture-phase drag-move event，此时 GPUI 认为 pointer 位于 bridge element 内，但 Fika 的
  pane viewport geometry 已经判断 pointer 位于 pane 内。此时 pane viewport 是权威 drop
  target owner，这与 Dolphin item-view controller ownership 对齐。
- 修复规则是在每个 Places retained-DnD move 把 pointer 转换成 Places row geometry 前，先经过
  app 级 pane viewport guard。如果 pointer 位于任意 pane viewport 内，Places 只清自己的
  retained DnD target 和 `place_drop_target`，输出 `places-dnd-defer-to-pane`，并且不再设置新的
  Places target。`can_drop` 也会在当前鼠标位置位于 pane viewport 内时拒绝，从而避免 stale
  retained Places target 在 drop 阶段获胜。
- 回归规则：未来 retained-hitbox typed drag API 必须保留这个 surface ownership 顺序。对于 pane
  坐标，pane viewport hit testing 优先于 Places sidebar bridge；Places 只能拥有 pane viewport
  之外且 Places event layer 之内的坐标。

## TODO

- [x] 添加 `PlacesEventDeliveryPolicy`，保留显式 `GpuiShells` fallback，当前默认为
  retained-DnD mixed policy，并提供 opt-in `RetainedProbe`。mixed state 必须显式记录在
  日志里，且 probe 日志不能满足 retained-event policy gate。
- [x] 添加 retained sidebar event probe layer，能插入 row/section hitboxes 并报告计数，
  但不改变行为。
- [x] 将 hover/cursor/leave clearing 移到 retained layer。当前状态：默认 retained-DnD
  layer 拥有 pointer cursor ownership 和 active-drag leave clearing。
- [x] 为带 scroll offset 的 content-local 坐标转换、section/row 边界添加单元覆盖。
- [x] 将 activation/context-menu targeting 移到 retained layer。当前状态：
  `retained-targeting` 拥有 row activation 和 row/section context menu targeting，
  默认 retained-DnD policy 也包含这些路径。非变更 targeting autosmoke 覆盖 activation-row、row context-menu 和 section
  context-menu target classification。
- [~] 添加 retained item/external/place drops 的隔离 DnD smoke。当前状态：
  `FIKA_AUTOSMOKE_PLACES=dnd` 在不改变用户 Places 的前提下验证 path-list 和 place drag
  对 row body、row edge、section target 的 retained target decision。它有意不执行
  destructive drop，因此完整隔离 drop/reorder smoke 仍未关闭。
- [x] 将 drag-move/drop delivery 移到 retained layer。当前状态：
  `retained-dnd` 通过 retained sidebar content hitbox 拥有 typed payload move/drop；
  `gpui_typed_dnd_payload_shells=0`。
- [x] 将 Places drag-start source modeling 移出 row shell。当前状态：
  `PlaceDragStartSource` 和 `install_place_drag_start_hitbox()` 位于 `places/drag.rs`，
  且 analyzer 日志要求 `drag_start_models=rows` 和 `drag_shells=0`。
- [x] 在 policy 日志中区分 probe hitbox 与 retained target-delivery hitbox。当前状态：
  retained-targeting 和 retained-dnd 报告 `retained_hitboxes=rows+sections`，probe /
  pointer-only policy 不报告。
- [x] 让 renderer `retained_interaction` 按 event policy 计数。当前状态：
  retained-targeting 和 retained-dnd 报告 rows+sections，probe/pointer 保持 0，并且
  retained-DnD 在 `gpui_event_shells=0` 时通过完整 retained-event policy。
- [x] 添加非变更 retained targeting autosmoke 和 analyzer gate。当前状态：
  `FIKA_AUTOSMOKE_PLACES=targeting` 会在不改变 app state、不打开菜单的前提下证明
  activation-row、context-row 和 context-section target classification。
- [x] 将 Places event delivery 默认提升到 retained-DnD policy。当前状态：
  默认日志显示 `event_policy=retained-dnd`、`retained_hitboxes=rows+sections`、
  `gpui_event_shells=0`、`drag_shells=0` 和 `drag_start_models=rows`；显式 `gpui`
  仍是 fallback。
- [x] 从 retained pointer policy 移除冗余 root sidebar GPUI leave-clear shell。当前状态：
  retained-pointer、retained-targeting 和 retained-DnD 报告
  `gpui_sidebar_leave_shells=0`；GPUI/probe policy 报告 `3`；analyzer 夹具会拒绝重新引入这些
  shell 的 retained-DnD 日志。
- [x] 按边界类型拆分剩余 GPUI event-shell accounting。当前状态：retained-DnD 报告
  `gpui_row_section_event_shells=0` 和 `gpui_typed_dnd_payload_shells=0`；fallback 状态显式报告
  row/section shell；analyzer 夹具会拒绝重新引入 row/section GPUI event shell 的
  retained-DnD 日志。
- [x] 从默认 target delivery 中移除 GPUI row/section event callbacks。当前状态：
  retained-DnD 报告 `gpui_row_section_event_shells=0`；retained activation、menu、DnD
  target ownership、typed payload delivery 和 drag start 都在 retained hitbox 上。
- [x] 添加 Fika GPUI retained-hitbox typed drag-move/drop API 后，移除 sidebar-level
  GPUI typed DnD payload bridge。完整 retained-event analyzer 现在通过 retained-DnD。
- [x] 使用 Fika GPUI retained-hitbox typed drag API 移除 GPUI row drag-start shells。
