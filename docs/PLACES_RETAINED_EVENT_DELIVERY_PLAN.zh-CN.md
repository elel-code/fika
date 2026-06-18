> 本文是 [PLACES_RETAINED_EVENT_DELIVERY_PLAN.md](PLACES_RETAINED_EVENT_DELIVERY_PLAN.md)
> 的简体中文翻译。

# Places Retained 事件传递计划

本文是 `docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md` 中 Track 3 的实现计划。
它只覆盖事件传递，不改变当前渲染器策略：Places row chrome 默认自绘；row 文本、
图标、context menu 渲染、DnD preview 创建和 drag start 仍保留在 GPUI，除非后续 gate
证明可以替换。

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
- 当前 GPUI event shell 和未来 retained event policy 的 analyzer 支持。
- 默认 custom row chrome，同时 GPUI 保留文本/图标/event shell。

当前 policy 形状：

```text
retained_hitboxes=0
gpui_event_shells=rows+sections
drag_shells=rows
```

默认提升前的目标 policy 形状：

```text
retained_hitboxes=rows+sections
gpui_event_shells=0
drag_shells=rows
```

`drag_shells=rows` 是有意保留的 GPUI typed drag-start 边界，不代表事件传递失败。

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

## TODO

- [ ] 添加 `PlacesEventDeliveryPolicy`，默认 `GpuiShells`，opt-in `RetainedHitboxes`。
  mixed state 必须显式记录在日志里。
- [ ] 添加 retained sidebar event layer，能插入 row/section hitboxes 并报告计数，
  但不改变行为。
- [ ] 为带 scroll offset 的 content-local 坐标转换、section/row 边界添加单元覆盖。
- [ ] 将 hover/cursor/leave clearing 移到 retained layer。
- [ ] 将 activation/context-menu targeting 移到 retained layer。
- [ ] 添加 retained item/external/place drops 的隔离 DnD smoke。
- [ ] 将 drag-move/drop delivery 移到 retained layer。
- [ ] analyzer gates 通过后移除 GPUI row/section event callbacks。
- [ ] Track 4 解决 typed drag start 前，继续保留 GPUI row drag-start shells。
