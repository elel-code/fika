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
- Fika mapping: src/main.rs WgpuState::acquire_surface_frame / render / render_detached_dialog。
- Divergence: 无直接 Dolphin wgpu surface reference；Fika 需要显式处理 wgpu Surface lost/outdated/timeout/validation，但把 main/dialog 的 acquire/recover 合并成单一 frame surface 边界，避免 detached dialog 继续维护独立错误策略。
- Verification: cargo test surface_frame_context_keeps_dialog_suboptimal_recovery_local；cargo check；cargo test；git diff --check。
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
