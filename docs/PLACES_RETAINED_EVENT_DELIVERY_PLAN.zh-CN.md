> 本文是 [PLACES_RETAINED_EVENT_DELIVERY_PLAN.md](PLACES_RETAINED_EVENT_DELIVERY_PLAN.md)
> 的简体中文翻译。

# Places Retained 事件传递计划

本文是 `docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md` 中 Track 3 的实现计划。
它只覆盖事件传递，不改变 row 渲染器策略：Places row chrome 默认自绘；row 文本、
图标、context menu 渲染、DnD preview 创建、typed drag payload delivery 和 drag start
仍保留在 GPUI，除非后续 gate 证明可以替换。

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
- GPUI row shell 只为 drag start 保留，直到存在 retained-hitbox typed-drag API。

## 当前状态

已实现：

- `places_interaction_geometry()` 提供 retained row/section geometry。
- `PlacesInteractionGeometry::hit_test_y()` 提供 retained row/section hit test。
- item/external path drop 和 place reorder 的 retained target-decision helpers。
- 显式 GPUI event-shell fallback、当前 retained-DnD mixed 默认和未来 full retained event
  policy 的 analyzer 支持。
- 显式的 `PlacesEventDeliveryPolicy`。默认现在是 `RetainedDnd`。
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` 保留为显式 fallback。
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe` 只报告未来 retained layer
  需要覆盖的 row/section hitbox 计数；它仍保持 `retained_hitboxes=0` 和
  `gpui_event_shells=rows+sections`。
- 默认 custom row chrome，同时 GPUI 保留文本/图标、一个 sidebar-level typed DnD payload
  shell 和 row drag-start shell；row/section activation、context-menu 与 DnD target
  delivery 由 retained layer 承担。

默认 mixed policy 形状：

```text
event_policy=retained-dnd
retained_hitboxes=rows+sections
retained_interaction=rows+sections
gpui_event_shells=1
drag_shells=rows
drag_start_models=rows
```

Full retained event policy 形状：

```text
retained_hitboxes=rows+sections
retained_interaction=rows+sections
gpui_event_shells=0
drag_shells=rows
drag_start_models=rows
```

`drag_shells=rows` 是有意保留的 GPUI typed drag-start 边界，不代表事件传递失败。
`drag_start_models=rows` 记录 payload、movable flag、export metadata 和 preview model
由 Places drag 模块拥有；row shell 应该只调用 GPUI `on_drag` API。

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

## TODO

- [x] 添加 `PlacesEventDeliveryPolicy`，保留显式 `GpuiShells` fallback，当前默认为
  retained-DnD mixed policy，并提供 opt-in `RetainedProbe`。mixed state 必须显式记录在
  日志里，且 probe 日志不能满足 retained-event policy gate。
- [x] 添加 retained sidebar event probe layer，能插入 row/section hitboxes 并报告计数，
  但不改变行为。
- [~] 将 hover/cursor/leave clearing 移到 retained layer。当前状态：
  `retained-pointer` 将 pointer cursor ownership 和 active-drag leave clearing 移到
  opt-in retained layer 后面；GPUI row/section shell 仍拥有 typed DnD move/drop delivery。
- [x] 为带 scroll offset 的 content-local 坐标转换、section/row 边界添加单元覆盖。
- [~] 将 activation/context-menu targeting 移到 retained layer。当前状态：
  `retained-targeting` 拥有 row activation 和 row/section context menu targeting，
  但 typed DnD move/drop 和 drag-start 仍需要 GPUI shell，因此该 policy 仍保持 opt-in。
  非变更 targeting autosmoke 现在覆盖 activation-row、row context-menu 和 section
  context-menu target classification。
- [~] 添加 retained item/external/place drops 的隔离 DnD smoke。当前状态：
  `FIKA_AUTOSMOKE_PLACES=dnd` 在不改变用户 Places 的前提下验证 path-list 和 place drag
  对 row body、row edge、section target 的 retained target decision。它有意不执行
  destructive drop，因此完整隔离 drop/reorder smoke 仍未关闭。
- [~] 将 drag-move/drop delivery 移到 retained layer。当前状态：
  `retained-dnd` 在一个 sidebar-level GPUI typed drag shell 后面拥有 row/section 目标查找
  和 drop dispatch。剩余 GPUI 边界是 payload delivery 和 drag-start，而不是 per-row/section
  DnD target logic。
- [x] 将 Places drag-start source modeling 移出 row shell。当前状态：
  `PlaceDragStartSource` 和 `install_place_drag_start_shell()` 位于 `places/drag.rs`，
  且 analyzer 日志要求 `drag_start_models=rows`。
- [x] 在 policy 日志中区分 probe hitbox 与 retained target-delivery hitbox。当前状态：
  retained-targeting 和 retained-dnd 报告 `retained_hitboxes=rows+sections`，probe /
  pointer-only policy 不报告。
- [x] 让 renderer `retained_interaction` 按 event policy 计数。当前状态：
  retained-targeting 和 retained-dnd 报告 rows+sections，probe/pointer 保持 0，并且
  `gpui_event_shells=1` 时完整 retained-event policy 仍失败。
- [x] 添加非变更 retained targeting autosmoke 和 analyzer gate。当前状态：
  `FIKA_AUTOSMOKE_PLACES=targeting` 会在不改变 app state、不打开菜单的前提下证明
  activation-row、context-row 和 context-section target classification。
- [x] 将 Places event delivery 默认提升到 retained-DnD mixed policy。当前状态：
  默认日志显示 `event_policy=retained-dnd`、`retained_hitboxes=rows+sections`、
  `gpui_event_shells=1` 和 `drag_start_models=rows`；显式 `gpui` 仍是 fallback。
- [x] 从 retained pointer policy 移除冗余 root sidebar GPUI leave-clear shell。当前状态：
  retained-pointer、retained-targeting 和 retained-DnD 报告
  `gpui_sidebar_leave_shells=0`；GPUI/probe policy 报告 `3`；analyzer 夹具会拒绝重新引入这些
  shell 的 retained-DnD 日志。
- [x] 按边界类型拆分剩余 GPUI event-shell accounting。当前状态：retained-DnD 报告
  `gpui_row_section_event_shells=0` 和 `gpui_typed_dnd_payload_shells=1`；fallback 状态显式报告
  row/section shell；analyzer 夹具会拒绝重新引入 row/section GPUI event shell 的
  retained-DnD 日志。
- [ ] analyzer gates 通过后移除 GPUI row/section event callbacks。
- [ ] Track 4 解决 typed drag start 前，继续保留 GPUI row drag-start shells。
