# 性能对齐原则

Fika 的性能工作以 Dolphin 为第一参考。本机 Dolphin 源码位于
`/home/yk/Code/fika/reference/dolphin`，它是文件管理器性能架构、行为保持型优化和
回归 gate 的第一参考。

## 硬规则

每一次性能优化，或任何会影响性能边界的调整，都必须在变更完成前给出明确的
Dolphin reference。

有效 reference 必须包含：

- 本地 Dolphin 文件路径，以及相关 class、function 或数据流；
- Dolphin 中被复制、改写或明确不复制的行为/性能边界；
- Fika 中对应的模块或代码路径；
- 如果 Fika 因 `winit/wgpu` shell 需要偏离 Dolphin，要写明原因；
- 本次变更使用的验证命令、日志、benchmark 或 smoke gate。

如果 Dolphin 没有直接对应实现，必须明确写出“无直接 Dolphin reference”，并给出
最接近的 Dolphin reference 和只能部分参考的原因。

## Reference 格式

性能说明、commit message、PR 描述或实现总结里使用这个结构：

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating
- Dolphin boundary: 可见项优先于后台 role work。
- Fika mapping: src/shell/... 或 src/core/...
- Divergence: ...
- Verification: ...
```

## 常用参考入口

- item model、refresh、filtering、sorting 和 role storage：
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kfileitemmodel.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kfileitemmodel.h`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kfileitemmodelsortalgorithm.h`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kfileitemmodelfilter.cpp`。
- metadata role、preview scheduling、visible index priority、异步 role 解析、
  directory size counting 和 MIME/Baloo role 更新：
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kdirectorycontentscounter.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kbaloorolesprovider.cpp`。
- 可见项 virtualization、widget reuse、scroll/layout 边界、column sizing、
  rubber-band 和 item view geometry：
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.h`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp`。
- item painting、icon/pixmap handling、text caching、role text layout 和
  selection/hover visuals：
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistwidget.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/kitemviews/kstandarditemlistwidget.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/views/dolphinfileitemlistwidget.cpp`。
- Dolphin view integration 和 mode-specific behavior：
  `/home/yk/Code/fika/reference/dolphin/src/views/dolphinview.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/views/dolphinitemlistview.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/views/viewmodecontroller.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/views/viewproperties.cpp`。
- Places 行为和设备侧边栏集成：
  `/home/yk/Code/fika/reference/dolphin/src/panels/places/placespanel.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/dolphinplacesmodelsingleton.cpp`。
- Dialog 生命周期、modal parent、尺寸 hint 和 Open With 初始尺寸：
  `/home/yk/Code/fika/reference/dolphin/src/dolphinmainwindow.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/views/dolphinview.cpp`、
  `/home/yk/Code/fika/reference/dolphin/src/panels/folders/folderspanel.cpp`、
  `/home/yk/Code/fika/reference/kio/src/widgets/kopenwithdialog.cpp`、
  `/home/yk/Code/fika/reference/kio/src/widgets/widgetsopenwithhandler.cpp`。

## 可继续推进的性能方向

- Model 增量变更：参考 `KFileItemModel` 的稳定 index / item identity / inserted /
  removed range，把 delete、trash、reload 从 full reset 继续推进为 range diff。
- 可见项优先 role 更新：参考 `KFileItemModelRolesUpdater`，将 MIME、图标、缩略图、
  folder preview、metadata role 分成 visible priority 与 background queue。
- View virtualization：参考 `KItemListView`、`KItemListWidget` 和 layouter，继续收缩
  visible slot pool、scroll range、hover/selection dirty 与 widget reuse 的边界。
- Layout / size hint cache：参考 `KItemListSizeHintResolver`，为 details / compact /
  icons 模式缓存文本自然宽度、列宽和 item rect，减少滚动和重排时的重复 shaping。
- 删除动画和批量 remove：参考 Dolphin model range removal，并结合 Nautilus 的连续
  splice 思路，先保留 stable item id，再让 surviving items 只做 reflow timeline。
- Render pipeline：将主窗口和 detached dialog 的 surface acquire、text/icon begin-frame、
  upload、present 合并到共享 frame surface 层，为 damage 和多窗口性能日志提供统一入口。

## 近期对齐记录

### Icons layout height cache

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp
- Symbol: KItemListSizeHintResolver::sizeHint / itemsChanged / clearCache / updateCache
- Dolphin boundary: item size hint 独立缓存，只有 item 插入、删除、移动、role 改变或显式 clear 时才重新解析。
- Fika mapping: src/shell/pane_layout.rs IconsLayoutHeightCache；src/main.rs ShellScene::pane_icons_layout / invalidate_layout_caches。
- Divergence: Dolphin 以 model range 精确失效；Fika 当前目录模型仍以 pane 级 reload/filter 为主，因此先按 pane + layout metric key 缓存 icons 文本高度，后续 model diff 落地后再缩小到 range 级失效。
- Verification: cargo test icons_layout_height_cache_reuses_name_measurements_while_scrolling；cargo check；cargo test；git diff --check。
```

### Render surface acquire boundary

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: View paint 入口只处理 view/widget 绘制，窗口系统的 backing surface 与 expose/recover 由 Qt 图形栈统一承担。
- Fika mapping: src/main.rs WgpuState::acquire_surface_frame / begin_surface_frame_encoding / submit_surface_frame / render / render_detached_dialog。
- Divergence: 无直接 Dolphin wgpu surface reference；Fika 需要显式处理 wgpu Surface lost/outdated/timeout/validation、texture view/encoder 创建和 submit/present/frame counter，但把 main/dialog 的 acquire/recover/encode setup/present 合并成单一 frame surface 边界，避免 detached dialog 继续维护独立错误策略。
- Verification: cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；cargo check；cargo test；git diff --check。
```

### Detached dialog frame pipeline

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: view paint 入口只负责把已经准备好的 view/widget 内容交给 painter，窗口 backing surface、缓存 begin/end 和 expose/present 生命周期由 Qt 图形栈统一承载。
- Fika mapping: src/shell/render/frame.rs::prepare_dialog_frame；src/main.rs::WgpuState::render_detached_dialog / encode_detached_dialog_pass。
- Divergence: 无直接 Dolphin dialog+wGPU reference；Fika 的 detached dialog 仍需要显式维护 text/icon atlas、async icon result drain、vertex upload、swash cache trim 和 render pass encode，但这些阶段从具体 dialog window handler 中抽到共享 DialogFrame 边界，避免 Open With 搜索结果变化继续复制一整套上传/重绘管线。
- Verification: cargo check；cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；git diff --check。
```

### Main SceneFrame upload and retained encode

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::paint
- Dolphin boundary: paint 阶段聚合 view/item/widget 绘制，局部 repaint 区域由 view/update 体系传入，具体 backing surface 复制和窗口 present 由 Qt 图形栈承担。
- Fika mapping: src/shell/render/frame.rs::SceneFrame::upload_quads；src/main.rs::WgpuState::encode_retained_scene_pass / encode_retained_present_pass。
- Divergence: 无直接 Dolphin retained wGPU reference；Fika 需要显式维护 retained texture、damage scissor、quad/text/icon GPU buffer upload 和 surface present，因此把 quad upload stats 收进 SceneFrame，把 retained scene encode 与 present-copy encode 收成两个固定阶段，后续动画 dirty 和局部 damage 可以在同一 frame encode 边界扩展。
- Verification: cargo check；cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；cargo test；git diff --check。
```

### SceneFrame work-pending boundary

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating / resolveNextPendingRoles
- Dolphin boundary: expensive roles、icons 和 previews 的待处理状态集中在 roles updater，visible range 改变后统一决定继续异步更新，而不是由 paint/event handler 分散判断。
- Fika mapping: src/shell/render/frame.rs::SceneFrame::work_pending / SceneFrameWorkPending；src/main.rs::WgpuState::render。
- Divergence: Dolphin 的 pending work 由 Qt/KIO job 和 model updater 驱动；Fika 目前仍有 metadata role worker、icon resolver、icon raster worker、thumbnail worker、folder preview role queue 和 text atlas miss 多个队列，因此先在 SceneFrame 层合并这些“是否需要下一帧”的信号，后续再把 visible-priority queue 和动画 dirty 接入同一入口。
- Verification: cargo check；cargo test；git diff --check。
```

### Dirty key projection reuse

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / slotItemsChanged / m_visibleItems
- Dolphin boundary: item view layout 维护一份 visible widget/item 集合，role 变化、paint 和局部更新复用该可见集合，而不是在每个判断点重新计算可见项。
- Fika mapping: src/shell/render/dirty_key.rs::ShellRenderDirtyKey::*_with_projections；src/shell/render/damage_snapshot.rs::ShellRenderDamageSnapshot::from_scene；src/main.rs::WgpuState::render。
- Divergence: Dolphin 的可见集合是长期 `m_visibleItems` widget map；Fika 当前 frame 仍以临时 `ShellPaneProjection` 表达可见项，因此本次先让 dirty key 和 damage snapshot 复用同一 frame projections，避免 details visible hash 和 folder preview dirty hash 重复触发布局。
- Verification: cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo check；cargo test；git diff --check。
```

### SceneFrame projection reuse

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::doLayout / updateVisibleItems / paint
- Dolphin boundary: layout 阶段维护的可见 item/widget 集合会被 paint、role update 和局部更新复用；paint 不再为同一帧重新计算可见集合。
- Fika mapping: src/main.rs::ShellScene::prepare_frame_projection_layouts / update_visible_slot_pools_for_projection_layouts / pane_projections_from_layouts / WgpuState::render / prewarm_scene_caches / ShellScene::build_frame；src/shell/render/frame.rs::SceneFrameProjections / prepare_scene_frame。
- Divergence: Dolphin 的可见集合是长期 widget map；Fika 仍使用每帧临时 `ShellPaneProjection`，但现在先用一次 layout 产出 prepared projection layouts，visible slot pool 直接消费这份 layout 的可见路径，随后 dirty key、damage、metadata/icon/text prewarm 和 SceneFrame paint 共用同一组 projections，避免 visible slot 更新和主帧 build 阶段分别重跑 layout/projection。
- Verification: cargo fmt；cargo check；cargo test prepared_pane_projections_match_direct_projection；cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo test；git diff --check。
```

### Visible slot assignment fused with projection layouts

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/kitemlistview.cpp
- Symbol: KItemListView::updateVisibleItems / m_visibleItems
- Dolphin boundary: visible item/widget 集合在可见项更新阶段分配和复用 widget identity，paint 阶段直接使用已经维护好的 visible item，不再为每个 item 重新查找 identity。
- Fika mapping: src/main.rs::ShellScene::update_visible_slot_pools_for_projection_layouts；src/shell/pane.rs::ShellVisibleItemSlotPool::update_visible_item_slots / ShellVisibleSlotItem；src/main.rs::ShellScene::pane_projection_from_prepared。
- Divergence: Dolphin 以 widget 对象长期承载 identity；Fika 仍使用 path keyed visible slot pool。现在 slot pool 直接消费 prepared projection layout 中的 borrowed path，并通过 `ShellVisibleSlotItem` 把 slot id 写回 prepared visible item；已有可见项在同一次 hash lookup 中拿到 slot id，新出现的 item 只在分配 slot 后补一次 lookup，随后立即释放 prepared visible item 的临时 `PathBuf`。同时 projection layout 改为用 `ShellLayout::for_each_visible_item` 直接填充 prepared items，不再先物化一份 `Vec<ItemLayout>`，最终 projection 构建时优先使用已分配 slot id，降低 retained visible item 的路径克隆、全量二次 slot hash lookup 和同帧峰值内存。
- Verification: cargo fmt；cargo check；cargo test prepared_pane_projections_match_direct_projection；cargo test；git diff --check。
```

### Layout size-hint cache bounded memory

```text
Dolphin reference:
- Source: /home/yk/Code/fika/reference/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp
- Symbol: KItemListSizeHintResolver::updateCache / clearCache / itemsInserted / itemsRemoved / itemsMoved / itemsChanged
- Dolphin boundary: size hint cache 是 view/model 维度的一份 logicalHeightHintCache；model 结构变化时就地更新或清空，不会为每次尺寸/缩放变化长期保留多份整目录高度数组。
- Fika mapping: src/shell/pane_layout.rs::BoundedLayoutCache / CompactLayoutCache / IconsLayoutHeightCache；src/main.rs::ShellScene::pane_compact_layout / pane_icons_layout。
- Divergence: Dolphin 的 size hint resolver 绑定 Qt item view 和 model 生命周期；Fika 仍按 pane、item_count、尺寸、缩放等 key 缓存 compact text widths、column widths 和 icons item heights。现在这两类 layout cache 使用 8-entry LRU 上限并保留 pane invalidation，避免窗口尺寸/缩放反复变化后把多份大目录 `Arc<[f32]>` 常驻内存。
- Verification: cargo fmt；cargo check；cargo test bounded_layout_cache_prunes_least_recently_used_entry；cargo test prepared_pane_projections_match_direct_projection；cargo test pane_visible_slot_pools_are_addressed_by_pane_id；cargo test render_dirty_key_with_projections_matches_scene_lookup；cargo test；git diff --check。
```

## Review 检查项

- 变更是否包含本地 Dolphin 文件路径和 symbol？
- 实现是否保持 Dolphin 的 model data、role resolution、view layout、painting
  分层边界；如果没有，是否写明偏离原因？
- 验证是否覆盖 reference 对应的用户可见路径，例如 scrolling、sorting、refresh、
  thumbnails、Places 或 DnD？
- 新增 cache、queue 或 retained resource 是否有边界和失效策略，并与 Dolphin
  reference 或明确的 Fika 边界一致？
- 如果声称性能提升，是否附上 benchmark、smoke 或日志结果？
