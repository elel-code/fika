> 本文是 [PLACES_RENDERER_PLAN.md](PLACES_RENDERER_PLAN.md) 的简体中文翻译。

# Places 渲染器计划

本计划仅覆盖 Places/侧栏 surface。不改变当前 item-view 渲染器决策：item-view
MIME/主题图标继续使用 GPUI `img()`，除非有证据证明自定义绘制器的性能持平或更优。

## Dolphin 参考

Dolphin 的 Places 路径不是通用的 item-view 克隆：

- `src/dolphinplacesmodelsingleton.cpp` 定义 `DolphinPlacesModel` 为薄
  `KFilePlacesModel` 特化。Dolphin 保持 model 为权威层，仅添加 Trash 装饰、
  panel-lock 分组行为、Ark DnD MIME 接受（视图层）以及 Ark drop 拒绝（model 边界）。
- `src/panels/places/placespanel.cpp` 使用 `KFilePlacesView` 作为视图。该 panel
  启用 drop-on-place、禁用 auto-resize items、持久化图标大小、在 `dragMoveEvent`
  期间拒绝不可写 place drop 目标、将 URL drop 委托给 `DragAndDropHelper::dropUrls`、
  连接设备拆卸信号，并注入 Dolphin 特定的右键菜单操作。

Fika 遵循 Dolphin 对齐的规则是：将 Places model/排序/设备语义保留在渲染器外部，并
在行为门明确之前将行渲染器视为可替换。

对于高性能自绘，Dolphin 的 item-view 实现同样是边界规则，而不是“每帧全部
canvas 重画”的依据：

- `src/kitemviews/kitemlistview.cpp` 只为可见 index 创建 widget，回收不可见的
  `KItemListWidget`，并更新 widget 属性，而不是重建整个 view tree。
- `src/kitemviews/kitemlistwidget.cpp` 和
  `src/kitemviews/kstandarditemlistwidget.cpp` 使用 content、layout、role 的
  dirty flag。只有缓存的 widget 状态变脏时才刷新 paint 工作。
- `KStandardItemListWidget::TextInfo` 使用带 aggressive caching 的
  `QStaticText`，因此文本 layout/raster 不会在每次 paint 时重复执行。
- 图标 pixmap 通过 `QPixmapCache` 按 icon identity、size、device pixel ratio
  和 mode 建 key。

Fika 对应的规则是：先迁移 row chrome；在 Fika 拥有可证明持平或更优的
retained/static text 与 image cache 之前，文本和图标继续留在最快的缓存渲染路径上。

## 当前 Fika 边界

当前所有权已接近 Dolphin 的划分方式：

- Model/order/device rows：`src/ui/places/model.rs` 加上 `src/ui/places/user/*`。
  主 Places 排序通过 `place_order_path` 持久化。
- Snapshot projection：`src/ui/places/projection.rs` 将 active、hidden、
  drop-target、insert-indicator、trash、device 和 icon 状态映射到
  `PlaceSnapshot`。
- GPUI row shell：`src/ui/places/sidebar/row.rs` 构建行视觉、右键菜单路由、
  激活、drag start 以及行级 DnD shell 接线。
- DnD 几何和预览：`src/ui/places/drag.rs` 拥有插入区域、重排索引、导出载荷
  以及光标偏移补偿的预览布局。
- Sidebar scroll：`src/ui/places/sidebar.rs` 拥有 GPUI 滚动容器和当前
  custom scrollbar canvas/hitbox。
- Row rendering policy 现在是三态：`FIKA_PLACES_ROW_VISUAL_POLICY=gpui`
  保留旧 GPUI row renderer；默认 `chrome` 策略在一个 sidebar-level
  自定义层中绘制 row background、active/drop 状态、trash 标记和插入指示器，
  同时保留 GPUI 文本和图标；`FIKA_CUSTOM_PLACES_ROWS=1` / `full` 保留完整
  自绘文本基准路径。

## 提议的 Retained 设计

不要一步替换 GPUI Places row renderer。目标设计是一个 retained Places row
surface，与 file-grid 保持相同的分离方式：

- `places/paint_slots.rs`：保留 `PlacePaintSlot` 和 section-heading slot。
  Place slot key 应按语义标识稳定，设备行优先使用 device id，普通 place 使用
  path/group。Slot 统计应区分 inserted、content changed、geometry changed、
  visual changed、unchanged 和 removed 行。
- `places/interaction.rs`：保留行 hitbox，用于激活、右键菜单、drop 目标查找、
  插入区域和 hover/cursor。Drag start 保持 GPUI shell，直到 GPUI drag-start
  边界发生变化。
- `places/visual.rs`：从 retained snapshot 绘制行背景、active/drop 状态、
  标签、trash 标记和插入指示器。图标渲染仍是独立的渲染器策略决策；如果 GPUI
  主题图标元素比自定义图像绘制更快或更稳定，则可以保留。
- `places/renderer_policy.rs`：记录自定义绘制行数量、GPUI 图标元素、retained
  interaction hitbox、drag-start shell、section heading 以及 scrollbar surface。
  这与 item-view 渲染器策略日志保持一致。
- `places/perf.rs`：添加 `FIKA_PERF_PLACES_VIEW=1` 计时，用于 snapshot
  projection、slot projection、row visual paint、icon rendering 和 scrollbar
  绘制。分析器脚本 `scripts/analyze-places-perf.sh` 对所有 perf/策略/交互/几何/
  autosmoke 字段执行结构化检查。

## 实现步骤

1. 添加 `FIKA_PERF_PLACES_VIEW=1` 和 `scripts/analyze-places-perf.sh`，构建
   当前 GPUI 侧栏基线：构建时间、行数、图标数、渲染器策略字段、scrollbar 帧。
   当前实现已具备这些功能。
2. 添加保留 slot 缓存和 autosmoke 基础设施。Autosmoke 必须可重复且无人值守，
   以便未来的渲染器决策有可复现的证据。
   当前实现使用 `FIKA_AUTOSMOKE_PLACES=targets` 进行安全的、非持久化的
   target-projection smoke。它设置 place target、start/end insert target、
   清除 target，并在每一步后记录 snapshot 计数。它有意不重排或添加书签。
3. 添加 retained paint slot 和统计，而不改变可见渲染。确认主排序持久化和
   hidden-section projection 仍通过单元测试。
   当前实现在 app 状态中维护 `PlacePaintSlotCache`，并输出
   `[fika places-slots]` 日志，包含 row/section 条目以及
   inserted/content/geometry/visual/unchanged/removed 计数。它不改变 GPUI
   row renderer。
4. 将 hover/drop hit testing 移至 retained Places interaction，同时保持
   GPUI drag-start shell。验证 item-to-place、place-to-pane、external
   path-to-place 和 reorder target。
   当前实现由 `places/interaction.rs` 负责 item/external path drop 和
   place reorder 的 row/section 目标决策。GPUI row 和 section shell
   仍提供事件传递和边界，因此在行 hitbox 移出 GPUI 之前
   `retained_interaction=0` 仍然正确。
   `[fika places-interaction-policy]` 是显式的桥接日志：目标决策已 retained，
   而激活、右键菜单、drag/drop 事件传递和 drag start 仍通过 GPUI 事件 shell 路由。
   `[fika places-interaction-geometry]` 是配套的 retained 几何投影。
   在 GPUI shell 可被替换为 retained hitbox 之前，它必须与 row/section 计数匹配。
5. 在 renderer policy 背后添加自定义行视觉绘制器，并与 GPUI row 路径对比滚动
   和 DnD。当前实现有三种策略：`gpui` 保留 fallback row renderer；默认
   `chrome` 自定义绘制 row background、active/drop 状态、trash 标记和插入指示器，
   同时保留 GPUI 文本/图标/事件 shell；`FIKA_CUSTOM_PLACES_ROWS=1` / `full`
   保留 full custom-text 基准路径。
6. 仅当 retained row painter 行为完整且性能持平或更优时才继续扩展到 chrome
   之外。否则保留 Dolphin 对齐的 model/projection，文本、图标和事件传递继续
   使用 GPUI。

## 运行时证据规则

Places 变更遵循与 item-view 变更相同的无人值守证据规则：可复现行为必须由
`FIKA_AUTOSMOKE_PLACES` 或新的隔离运行时 smoke 驱动，然后渲染器决策才能依赖
该证据。当前 `targets` smoke 有意为非破坏性的，因此重排/drop 持久化仍然需要
隔离的用户 place 配置或手动审查，直到存在对应的测试 fixture。

每个 Places 优化突破必须记录在本计划或同一 slice 的 renderer-decision 文档中。
记录应包含：用户可见症状、用于比较的 Dolphin Places 源代码边界、Fika 中的根因、
实现变更、保存的日志/分析器命令，以及未来 Places 工作必须运行的回归守卫。

## 当前基准 Smoke

2026-06-17 桌面会话命令：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 target/debug/fika /etc > /tmp/fika-places-baseline.log 2>&1
scripts/analyze-places-perf.sh --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-baseline.log
```

当前 GPUI 侧栏日志为 `source=11 visible=11 sections=2`，其中
`rows=11 sections=2 elements=13`。重复冷首次 snapshot 约 `4.3ms`；
稳态 snapshot 帧约 `58-133us`。侧栏行构建通常为 `185-270us`，偶有帧
约 `0.5-0.6ms`。渲染器策略日志显示预期的当前状态：`row_gpui=11`、
`row_visual_layer=0`、`icon_gpui=11`、`retained_interaction=0`、
`drag_shell=11`、`section_gpui=2` 以及 `scrollbar_canvas=1`。

retained slot cache 落地后，同一 perf 运行也输出 `[fika places-slots]`。
对于默认 `/etc` 侧栏，首次投影有 `rows=11 sections=2 entries=13 inserted=13`；
稳态帧应转为 `unchanged=13`，在 2026-06-17 桌面会话上观察到的投影时间约
`21-46us`。target-projection smoke 应显示 drop 或 insert 状态的 visual 变化，
而没有 content 或 geometry 抖动。

## Current Autosmoke

2026-06-17 桌面会话命令：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-targets.log
```

预期标记：

```text
[fika autosmoke] places start scenario=DropTargets
[fika autosmoke] places action=target-first-place ... changed=true
[fika autosmoke] places snapshot=after-place-target ... place_targets=1
[fika autosmoke] places action=target-insert-start index=0 changed=true
[fika autosmoke] places snapshot=after-insert-start ... insert_before=1
[fika autosmoke] places action=target-insert-end ... changed=true
[fika autosmoke] places snapshot=after-insert-end ... insert_after=1
[fika autosmoke] places action=clear-targets changed=true
[fika autosmoke] places snapshot=after-clear ... place_targets=0 insert_before=0 insert_after=0
[fika autosmoke] places complete scenario=DropTargets
```

此 smoke 有意为非破坏性的。后续 Places smoke 只有在能够以隔离的用户 place
配置或显式测试 fixture 运行时，才能覆盖实际的重排/drop 持久化。

当前 GPUI 基线的分析器摘要应包含：

```text
places_slots_frames=... max_inserted=13 max_content=0 max_geometry=0 max_visual=2 max_unchanged=13 max_removed=0
places_renderer_policy_frames=... max_row_gpui=11 max_row_visual_layer=0 max_icon_gpui=11 max_retained_interaction=0 max_drag_shell=11
places_interaction_policy_frames=... max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0 max_gpui_event_shells=13 max_drag_shells=11
places_interaction_geometry_frames=... max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2
places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
```

2026-06-18 interaction policy 证据：

```text
/tmp/fika-places-targets-interaction.log:
  places_interaction_policy_frames=10 max_rows=11 max_sections=2 max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0 max_gpui_event_shells=13 max_drag_shells=11
/tmp/fika-places-custom-targets-interaction.log:
  places_interaction_policy_frames=14 max_rows=11 max_sections=2 max_row_target_decisions=11 max_section_target_decisions=2 max_retained_hitboxes=0
  max_row_gpui=0 max_row_visual_layer=11
/tmp/fika-places-hit-test-autosmoke.log:
  places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2
  places_interaction_geometry_frames=15 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=6us
  max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-custom-retained-hit-test.log:
  places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2
  places_interaction_geometry_frames=10 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=15us
  max_row_gpui=0 max_row_visual_layer=11
/tmp/fika-places-hit-test-autosmoke-module.log:
  places_hit_test_autosmoke start=1 complete=1 row_before=1 row_body=1 row_after=1 section=1 summary=1 max_rows=11 max_sections=2
  places_interaction_geometry_frames=11 max_rows=11 max_sections=2 max_entries=13 max_content_height=378.0 max_hit_tests=2 max_project=4us
  max_row_gpui=11 max_row_visual_layer=0
```

## Overflow Autosmoke

对于 Places 滚动/溢出证据，运行：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-default.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-overflow-default.log
```

`FIKA_AUTOSMOKE_PLACES=overflow` 在 snapshot 层追加 64 行非持久化测试行。它不
写入用户 Places 配置，也不修改 `self.places`。预期证据为 `visible=75`，一个额外的
`Autosmoke` section，`[fika places-scrollbar] visible=1` 和 `max_scroll_y>0`。

2026-06-17 默认 GPUI overflow 证据：

```text
places_sidebar_frames=7 max_rows=75 max_sections=3 max_elements=78 max_build=3083us
places_renderer_policy_frames=7 max_row_gpui=75 max_row_visual_layer=0 max_icon_gpui=75
places_scrollbar_frames=7 max_visible=1 max_scroll_y=1909.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

## Layout Autosmoke

对于 Places panel 宽度/可见性和设置持久化证据，使用隔离的配置目录运行：

```bash
XDG_CONFIG_HOME=/tmp/fika-places-layout-config \
  timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=layout \
  target/debug/fika /etc > /tmp/fika-places-layout.log 2>&1
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-layout.log
```

对于 opt-in 行视觉策略，添加 `FIKA_CUSTOM_PLACES_ROWS=1` 并切换分析器策略：

```bash
XDG_CONFIG_HOME=/tmp/fika-places-layout-custom-config \
  timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 \
  FIKA_AUTOSMOKE_PLACES=layout target/debug/fika /etc \
  > /tmp/fika-places-layout-custom.log 2>&1
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-layout-custom.log
```

`FIKA_AUTOSMOKE_PLACES=layout` 不改变用户 Places 排序。它捕获启动 panel 状态，
隐藏侧栏，再次显示，调整大小，重置为默认宽度，恢复捕获的启动状态，并通过读取
`$XDG_CONFIG_HOME/fika/settings.tsv` 验证合并的设置写入。

预期标记：

```text
[fika autosmoke] places start scenario=Layout
[fika autosmoke] places action=layout-hide ... visible=false changed=true
[fika autosmoke] places action=layout-show ... visible=true changed=true
[fika autosmoke] places action=layout-resize ... changed=true
[fika autosmoke] places action=layout-reset ... changed=true
[fika autosmoke] places action=layout-restore ...
[fika autosmoke] places action=layout-verify-saved ... ok=true
[fika autosmoke] places complete scenario=Layout
```

分析器摘要应包含：

```text
places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
```

2026-06-18 证据：

```text
/tmp/fika-places-layout.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  max_row_gpui=11 max_row_visual_layer=0
/tmp/fika-places-layout-custom.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  max_row_gpui=0 max_row_visual_layer=11
  places_row_visual_frames=8 max_rows=11
/tmp/fika-places-f9-layout.log:
  places_layout_autosmoke start=1 complete=1 initial=1 hide=1 show=1 resize=1 reset=1 restore=1 verify_saved=1
  max_row_gpui=11 max_row_visual_layer=0
```

## Opt-In Row Visual Smoke

自定义 Places 行视觉路径是实验性的，在达到或超过 GPUI 行基线之前必须保持
opt-in。运行方式：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-rows.log
```

对于 overflow 对比，切换场景和分析器门：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom.log
```

预期策略形态：

```text
places_renderer_policy_frames=... max_row_gpui=0 max_row_visual_layer=11 max_icon_gpui=11 max_retained_interaction=0 max_drag_shell=11
places_row_visual_frames=... max_rows=11 max_prepaint=...us max_paint=...us
```

`max_rows` 必须与渲染器策略行数匹配。当前 opt-in 实现通过一个侧栏级 visual
layer 绘制行背景、active/drop 状态、标签、trash 标记和插入指示器，而 GPUI
仍拥有图标、行事件传递、右键菜单、DnD 和 drag-start shell。分析器拒绝退回为
每行一个 canvas 的自定义行视觉日志。

2026-06-17 首次 opt-in 桌面会话证据：

```text
default: places_sidebar max_build=631us, max_row_gpui=11, max_row_visual_layer=0
custom: places_sidebar max_build=547us, max_row_gpui=0, max_row_visual_layer=11
custom: places_row_visual_frames=110 max_rows=1 max_prepaint=148us max_paint=921us
```

opt-in 路径通过了非破坏性的 target/insert/clear autosmoke 并证明了渲染器策略
分离，但尚未默认就绪。高 per-row `max_paint` 来自首次冷帧；同一日志中后续行
通常每次绘制约 `14-33us`。在替换默认 GPUI row renderer 之前，需收集滚动/DnD
行为证据，并决定是否将 per-row canvas 开销合并为 retained sidebar visual layer。

2026-06-17 opt-in overflow 证据：

```text
places_sidebar_frames=9 max_rows=75 max_sections=3 max_elements=78 max_build=3968us
places_renderer_policy_frames=9 max_row_gpui=0 max_row_visual_layer=75 max_icon_gpui=75
places_row_visual_frames=675 max_rows=1 max_prepaint=249us max_paint=951us
places_scrollbar_frames=9 max_visible=1 max_scroll_y=1684.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

这确认了首个 opt-in 视觉绘制器在 overflow 下可工作，但也显示了每行一个 canvas
的预期代价（此 5s smoke 中有 `675` 个 row-visual 帧）。这为下一个渲染器 slice
提供了证据：在考虑默认切换之前，将 Places 行视觉聚合为一个 retained sidebar
layer。

## Aggregated opt-in row visual 证据

2026-06-17 聚合 opt-in 行视觉证据：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows-layer.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-rows-layer.log

timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom-layer.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom-layer.log
```

Targets 摘要：

```text
places_sidebar_frames=7 max_rows=11 max_sections=2 max_elements=13 max_build=938us
places_renderer_policy_frames=7 max_row_gpui=0 max_row_visual_layer=11 max_icon_gpui=11
places_row_visual_frames=7 max_rows=11 max_prepaint=1515us max_paint=7570us
places_autosmoke target=1 insert_start=1 insert_end=1 clear=1 snapshots=1,1,1,1,1
```

Overflow 摘要：

```text
places_sidebar_frames=11 max_rows=75 max_sections=3 max_elements=78 max_build=3289us
places_renderer_policy_frames=11 max_row_gpui=0 max_row_visual_layer=75 max_icon_gpui=75
places_row_visual_frames=11 max_rows=75 max_prepaint=12610us max_paint=16108us
places_scrollbar_frames=11 max_visible=1 max_scroll_y=1663.0
places_overflow_autosmoke start=1 complete=1 snapshot=1 max_visible=75
```

首次 opt-in overflow 代价的根因是结构性的：每行拥有自己的 canvas，因此一个包含
75 行的侧栏帧会产生 75 次 row-visual prepaint/paint 过程。Dolphin 的
`KFilePlacesView` 保持 model/view 分离，不让行渲染拥有 Places 排序或设备语义，
因此 Fika 可以在不改变 Places 行为的情况下合并行级视觉。实现将行视觉移入一个
绝对定位的侧栏层，该层从相同的 `PlaceSnapshot` 流计算 section-heading 偏移。
回归守卫是 `--expect-custom-row-visual-policy`，它现在要求
`places_row_visual max_rows == places_renderer_policy max_rows`，并拒绝旧的
per-row `rows=1` 形态。

## Row text shape-cache 证据

opt-in 行视觉的下一个代价是文本形状计算，而非 Places model 工作。聚合层在每次
prepaint 过程中仍然为每行标签重新计算形状，即使相同的 `PlaceSnapshot` 标签、
字体和视觉文本颜色是稳定的。Fika 现在以 app 级 `PlacesRowTextShapeCache`
镜像 item-view text-cache 模式，以 label/font/font-size/color 为键。该缓存
仅由 `FIKA_CUSTOM_PLACES_ROWS=1` / `full` 使用；默认 chrome 策略将文本保留在
GPUI 上，且不得发出该 channel。
运行时日志包含：

```text
[fika places-row-shape-cache] hits=... misses=... evicted=... entries=...
```

`--expect-custom-row-visual-policy` 要求 opt-in 自定义行路径有此 shape-cache
channel，因此未来的 Places 行绘制器变更不能在没有运行时证据的情况下悄然退回
逐帧行标签形状计算。

2026-06-18 opt-in row text shape-cache 证据：

```bash
timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-custom-rows-shape-cache.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-custom-rows-shape-cache.log

timeout 5s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-overflow-custom-shape-cache.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-overflow-custom-shape-cache.log
```

Targets 摘要：

```text
places_row_visual_frames=11 max_rows=11 max_prepaint=1139us max_paint=5175us
places_row_shape_cache_frames=11 max_hits=11 max_misses=11 max_evicted=0 max_entries=11
```

Overflow 摘要：

```text
places_row_visual_frames=6 max_rows=75 max_prepaint=9578us max_paint=8794us
places_row_shape_cache_frames=6 max_hits=75 max_misses=75 max_evicted=0 max_entries=75
```

最大值包含冷首次帧，其中每个可见行标签都是缓存未命中。同一 overflow 日志随后
稳定在 `hits=75 misses=0`，行视觉 prepaint 约 `148-176us`；重复的行标签形状
计算开销已从稳态 opt-in Places 帧中移除。

2026-06-18 Dolphin 对齐的 Places chrome 策略更新：

之前的 full custom row visual layer 不足以成为默认值，因为它将文本移入 GPUI
canvas 绘制，并重新引入字体/字形冷启动尖峰。Dolphin 保持 item widget
visible-only，并使用 static text 与 pixmap cache；Fika 目前还没有等价的公开
static-text raster cache 可供 Places canvas 使用。因此默认策略收窄为 custom
chrome 路径：

- `FIKA_PLACES_ROW_VISUAL_POLICY=chrome` 是默认值。
- 自定义层绘制 row background、active/drop border、插入指示器和 trash 状态。
- GPUI 仍绘制行文本和主题图标，因此 chrome 日志中必须没有 row shape-cache
  channel。
- `FIKA_PLACES_ROW_VISUAL_POLICY=gpui` 保留为基线 fallback。
- `FIKA_CUSTOM_PLACES_ROWS=1` 保留为 full custom-text 基准路径。

运行时证据：

```bash
timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-chrome-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-targets.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_AUTOSMOKE_PLACES=overflow target/debug/fika /etc > /tmp/fika-places-chrome-overflow.log 2>&1
scripts/analyze-places-perf.sh --require-overflow-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-overflow.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_PLACES_ROW_VISUAL_POLICY=gpui FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-gpui-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy /tmp/fika-places-gpui-targets.log

timeout 6s env FIKA_PERF_PLACES_VIEW=1 FIKA_CUSTOM_PLACES_ROWS=1 FIKA_AUTOSMOKE_PLACES=targets target/debug/fika /etc > /tmp/fika-places-full-targets.log 2>&1
scripts/analyze-places-perf.sh --require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy /tmp/fika-places-full-targets.log
```

默认 chrome targets 摘要：

```text
places_renderer_policy_frames=10 max_rows=11 max_row_gpui=0 max_row_visual_layer=11 max_text_gpui=11 visual_kinds=chrome
places_row_visual_frames=10 max_rows=11 max_painted=11 max_prepaint=23us max_paint=83us
places_row_shape_cache_frames=0
```

默认 chrome overflow 摘要：

```text
places_renderer_policy_frames=6 max_rows=75 max_row_gpui=0 max_row_visual_layer=75 max_text_gpui=75 visual_kinds=chrome
places_row_visual_frames=6 max_rows=75 max_painted=29 max_prepaint=28us max_paint=148us
places_row_shape_cache_frames=0
```

full custom-text 对比：

```text
places_renderer_policy_frames=10 max_rows=11 max_row_gpui=0 max_row_visual_layer=11 max_text_gpui=0 visual_kinds=full
places_row_visual_frames=10 max_rows=11 max_painted=11 max_prepaint=1046us max_paint=5183us
places_row_shape_cache_frames=10 max_hits=11 max_misses=11 max_entries=11
```

额外默认 chrome 守卫已通过：

```bash
scripts/analyze-places-perf.sh --require-layout-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-layout.log
scripts/analyze-places-perf.sh --require-hit-test-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy /tmp/fika-places-chrome-hit-test.log
```

## 验收门

- 主 Places 排序跨重启持久化，动态设备刷新不重写用户排序。
- 隐藏 places 和隐藏 sections 保持仅投影状态。
- Drop-on-place 一致拒绝不可写/网络目标，符合当前规则，同时内部重排仍允许
  主 places。
- 右键菜单继续区分空白侧栏、section header、普通 place、可编辑/可移除书签、
  回收站和设备行。
- 运行时 smoke 覆盖行激活、重排 insert-before/after、item drop 到 place、
  外部路径 drop 到 place、place drag 到 pane 目录、设备拆卸操作可见性以及
  侧栏离开清除。
- 滚动/绘制证据显示相对当前 GPUI 侧栏基线无退化。无法达到 GPUI 水平的自定义
  Places 绘制器必须保持在 opt-in flag 之后或被移除。
- 侧栏宽度/可见性变更重新测量 pane 视口，而不重置 pane 内容、滚动、选择、
  Places 排序或当前渲染器策略。宽度/可见性的持久化必须保持 latest-only/coalesced；
  未来的设置添加不应在每个拖拽帧都写入配置文件。
