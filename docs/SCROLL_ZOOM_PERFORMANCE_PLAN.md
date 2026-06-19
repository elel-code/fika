# Scroll and Zoom Performance Plan

> Current GPUI entry point plus archived Slint-era investigation. The legacy
> Slint notes remain below for historical context; active item-view scroll/zoom
> work must follow the GPUI/Dolphin boundaries in
> `docs/ITEM_VIEW_CUSTOM_PAINT_DESIGN.md`,
> `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`, and
> `docs/ITEM_VIEW_RENDERER_DECISIONS.md`.

## Current GPUI Plan

The current GPUI item-view path treats scroll and zoom as retained state
updates, not as opportunities to rebuild item identity or resolve file roles in
the render frame.

### Dolphin-Aligned Boundaries

- Scroll updates the pane `ViewState`, visible range, slot geometry, retained
  hit testing, and paint snapshots. It must not synchronously scan icon themes,
  probe thumbnails, or read MIME magic.
- Zoom changes item metrics and may invalidate layout/text/image geometry, but
  model roles stay on the role/update side. The frame may use preliminary or
  retained same-source icon snapshots until resolved role data is ready.
- `raw_file_grid_snapshot()` owns the visible/work range. Scheduler projection
  queues metadata roles, thumbnails, and file-icon theme resolve work.
- `VisibleItemSnapshotCache`, paint slots, text shape caches, and GPUI
  `RetainAllImageCache` are the retained-state surfaces that should absorb
  repeated scroll/zoom work.

### Current Fixes

- File icon theme path resolution is no longer done synchronously during
  raw-to-render snapshot conversion. The frame path calls
  `FileIconCache::cached_or_preliminary_icon_for()`. Visible icon warming uses
  Dolphin `updateVisibleIcons()` index order, and background batches resolve
  theme icon paths in Dolphin `indexesToResolve()` visible/read-ahead order.
- When background icon resolve completes, visible item snapshot caches are
  invalidated so preliminary fallback icons are replaced on the next frame.
- Thumbnail and theme-icon image pending/failure states no longer have to drop
  directly to fallback. Compact/Icons and Details keep a pane-local retained
  image map: MIME/theme icons are retained by `iconName`, while thumbnails are
  retained by exact thumbnail path. A zoom-level path change can therefore keep
  painting the previous real MIME icon until GPUI finishes decoding the new
  resource, matching Dolphin's `KStandardItemListWidget::m_pixmap` behavior.
  Fallback is still used when no real image has ever been decoded for that
  semantic source.
- Read-ahead items stay in raw/render snapshots for scheduler projection and
  cache retention, but they no longer enter static visual or image prepaint.
  This matches Dolphin's split where `KItemListView` paints visible widgets and
  `KFileItemModelRolesUpdater::indexesToResolve()` handles read-ahead role
  work outside the paint frame.
- Zoom exact-size theme-icon misses now reuse an already resolved icon path for
  the same file-icon kind and do not enqueue another exact-size path request.
  This mirrors Dolphin's visual-stability behavior: do not replace a real
  visible icon with a fallback marker, or commit a second image identity, just
  because the new zoom level changed the requested icon bounds.
- Active zoom now mirrors Dolphin's ordinary theme-icon paint path. Item layout
  and icon bounds change immediately, while file-icon role/path identity remains
  stable once the same file-icon kind has a resolved theme path. Dolphin's 300ms
  `triggerIconSizeUpdate()` timer is treated as a preview/role-updater boundary,
  not as a delayed second size or path commit for Fika theme icons.
- The image paint layer now applies the same rule after path resolution too:
  if GPUI `RetainAllImageCache::load()` returns pending/error for a new icon
  path, the painter first tries a retained image for the same MIME icon name.
  This avoids the probabilistic fallback flash seen while scrolling or zooming
  image-backed MIME icons.
- Theme icon file decoding is not performed synchronously in GPUI prepaint.
  Decoding stays on GPUI's image-cache path; paint uses retained same-`iconName`
  images to avoid visible blank/marker regression.
- Directory-load MIME icon stability now follows Dolphin's visible-widget
  boundary. Dolphin avoids expensive `KFileItem::iconName()` in
  `KFileItemModel::retrieveData()` when MIME is unknown, but
  `KFileItemModelRolesUpdater::startUpdating()` calls `updateVisibleIcons()`
  and `KFileItemListView::initializeItemListWidget()` fills `iconName` for
  widgets that are actually created. Fika mirrors that by resolving visible
  generic MIME metadata synchronously within a small frame budget before
  queueing background metadata work; read-ahead and offscreen items still use
  the asynchronous role scheduler.

### 2026-06-17 Breakthrough: MIME Icon Load, Zoom, And Scroll Stability

This record captures the root cause and accepted implementation for the recent
`/etc` and zoom stability fix. Keep it as the comparison point before changing
MIME/theme icon rendering again.

Symptoms:

- Loading `/etc` showed a visible blank/placeholder-to-MIME-icon cascade.
- Zoom could show a second icon-size adjustment after the item geometry had
  already changed.
- Initial `/etc` scroll/zoom autosmoke produced intermittent hitches in the
  early custom theme-icon painter before retained readiness/cache promotion.

Root causes:

- The early custom MIME/theme icon painter could enter the first paint before
  GPUI's image cache had decoded the theme icon resource. In the historical
  `/etc` A/B smoke it logged `theme_placeholder=48`, matching the visible
  placeholder cascade. The current default full custom path is guarded by
  retained readiness/cache evidence and must keep `theme_placeholder=0`.
- Fika initially treated Dolphin's 300ms `triggerIconSizeUpdate()` delay as an
  icon-size debounce. Dolphin only delays preview/role-updater work there;
  ordinary `iconName` pixmaps are generated from the widget's current style
  option icon size. Delaying or freezing Fika theme-icon size therefore created
  a visible second-size commit during zoom.
- Visible icon sync duplicated work that was already queued for read-ahead icon
  resolution. The first autosmoke after the renderer split logged
  `icon_sync=28340us` and `total=29451us` on a geometry-change frame.

Implementation:

- MIME/theme icons now default to the retained custom image layer. It reuses
  GPUI `RetainAllImageCache -> RenderImage -> Window::paint_image`, but ordinary
  pane rendering must keep `gpui_image_element=0`. `FIKA_GPUI_THEME_ICONS=1`
  is the paired GPUI `img()` baseline.
- Render conversion uses cached or preliminary icon snapshots only. Theme icon
  path scanning stays in visible icon sync and the background resolve queue, not
  in GPUI prepaint or render conversion.
- Visible icon sync skips requests already queued or pending in
  `FileIconResolveQueue`, preserving Dolphin's visible-first exception without
  redoing read-ahead scans in the scroll frame.
- Zoom commits the current layout icon bounds immediately. MIME/theme icon
  paths stay stable after the same file-icon kind has resolved once, so zoom no
  longer synchronously resolves or queues an exact-size path request. Preview
  and thumbnail role work may still be coalesced, but theme icon geometry must
  not use a delayed second size.
- Directory load resolves visible generic MIME metadata and visible theme icon
  paths within the bounded visible-widget budget before queueing offscreen
  metadata/icon work.

Evidence:

```text
historical custom-theme /etc A/B: theme_placeholder=48, gpui_image_element=0
historical GPUI baseline /etc A/B: theme_placeholder=0,  gpui_image_element=48

before queued/pending skip:
  icon_sync=28340us, geometry-change total=29451us

after queued/pending skip:
  icon_sync=173us, geometry-change max_total=1635us
```

Regression guard:

- For renderer changes, compare the default full custom path against
  `FIKA_GPUI_THEME_ICONS=1` with `scripts/compare-item-image-renderers.sh`.
- For scroll/zoom changes, run
  `FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc`
  and summarize with `scripts/analyze-item-view-perf.sh`.
- If `icon_sync` returns to multi-ms values, inspect visible/read-ahead icon
  queue ownership before changing the renderer.
- If `icon_sync` stays low but the frame is still slow, inspect static visual
  paint, text shaping, or GPUI image-cache behavior instead of blaming MIME
  icon path lookup.

### Open Verification Work

- Collect desktop-session logs for `/etc` initial scroll and ordinary-directory
  initial zoom in Compact and Icons:

  ```sh
  FIKA_PERF_ITEM_VIEW=1 cargo run -- /etc 2>&1 | tee /tmp/fika-etc-scroll.log
  FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads 2>&1 | tee /tmp/fika-downloads-zoom.log
  ```

- In those logs, verify:
  - warm scroll/zoom `convert=` is not dominated by synchronous icon work
  - `[fika item-image]` appears when image-backed items are present
  - no repeated blank thumbnail/icon frame is visible during zoom
  - loading a directory does not show a visible cascade from preliminary MIME
    icons to resolved MIME icons for the initial visible range
  - cold first-frame work is separated from steady scroll/zoom phases
- Keep comparing against Dolphin source before moving more work into custom
  paint. If a GPUI built-in renderer is faster for a surface, keep the retained
  Dolphin-style model/controller boundary and leave that surface on GPUI.

## Archived Slint-Era Investigation

## Scope

用户体感问题集中在主文件视图的滚动和 zoom，不是目录刷新。当前 Dolphin-style slot reuse
架构已经收尾，但还没有真实性能基线；本计划先建立可重复测量，再按数据处理热点。

## Implementation Status

截至本轮实现，代码级优化已落地，真实 workload p95 仍待 GUI 交互采样，不能用主观结论替代。

已完成：

- `src/app/item_view_perf.rs` 增加 `FIKA_PERF_ITEM_VIEW=1` 可关闭日志，默认无输出。
- scroll hot path 保留 Dolphin 同步 scroll-offset boundary；cached viewport 命中时只更新 viewport
  state/range，不进入 `prepare_virtual_view_snapshot_update()`、`sync_pane_view_ui()`、raster 或
  fallback icon。
- zoom hot path 保留 Dolphin transaction 的同步请求边界：zoom level 变化立即推进 pane generation
  和 latest-only prepare request，但 `prepare_virtual_view_snapshot_update()`、entry projection、bounds、
  metadata projection、slot projection 都在 `spawn_blocking` 中完成；UI 线程只 apply 最新 prepared
  result、合并当前 selection/thumbnail cache，并执行必要的 Slint model writes。
- `apply_virtual_view_result()` 不再有 UI 线程 projection fallback；prepared result 缺 projection
  会被记录并丢弃，避免回归成同步投影。
- `split_view.rs` 的 pane chrome 更新和 item-view 更新已拆开：`sync_pane_slot_ui()` 只 patch
  `PaneSurfaceData.pane`，`sync_pane_view_ui()` 只构造一次 `PaneViewData` 并复用到 surface，不再因为
  状态栏、路径栏、focus 等普通 UI 更新重新构造 item-view/raster/fallback icon。
- `ui/app.slint`/`split_view.rs` 移除重复的 `pane_views` model；`PaneViewData` 只嵌在
  `PaneSurfaceData` 中维护，viewport-only 和 view apply 都直接 patch `pane_surfaces`，连续 zoom
  视觉资源复用时用轻量标量字段判断替代 `PaneViewData`/`Image`/`ModelRc` 整结构比较。
- `model_update.rs` slot allocator 返回统计，能记录 reused/extended/inactive rows、content/geometry/
  thumbnail patch、thumbnail image reuse/replace、`set_row_data()` 和 model extend/rebuild；生产 apply
  路径改用后台生成的 `PreparedItemViewSlotProjection`，不再在 UI 线程重建 frame batch/slot projection。
- `PaneView::tile_frame_raster_layer()` 记录 raster cache hit/miss、尺寸、像素数、revision 和 render time；
  没有选中项/drop target 时直接返回 1x1 empty raster，连续 zoom 的 immediate commit 复用旧 raster。
- fallback icon cache 改为小 LRU，key 为 `(width, height, dark, kind)`；`split_view.rs` 只请求当前
  active non-thumbnail slots 实际需要的 media kind，不再每个 zoom level 固定渲染 10 种 icon；raster
  deferred 时只复用缓存 icon，不在 UI 线程渲染新尺寸。
- thumbnail flush 保留 16ms batch；icon-size timer pending 时只把结果写入 Rust state/cache，不插入
  `sync_virtual_entries_for_slot(... schedule=false)`，等 icon-size timer 后按 latest visible slice 刷新。
- thumbnail key 计算随 virtual projection 在 `spawn_blocking` 中完成，UI apply 只用 prepared key 合并
  当前 cache/pending/failure 状态；cached thumbnail 的 RGBA -> `slint::Image` 转换增加 UI-side image
  cache，避免滚动/zoom/flush 重复像素拷贝。
- 本地搜索/过滤的 visible index 构建已从 `apply_filter_for_slot()` 的 UI 线程同步全量扫描，改为
  `spawn_blocking` latest-only prepare；`pane.search_index_generation` 保证旧结果丢弃，pending 期间
  virtual sync 不混用“新 query + 旧 index”，选择/框选/Select All 也不会绕回按新 query 全量重扫。
- 搜索框关闭和 Escape 语义按 Dolphin 源码收敛：关闭按钮只停 debounce timer 并发 close request，
  Rust 统一清理状态；输入框 Escape 有文本先清本地文本并提交空查询，文本为空才关闭搜索栏，且
  `search_query_sync_request` 只在程序化清空时强制回写聚焦中的输入框。
- 搜索输入 focus 路由按 Dolphin 的 focus proxy 边界收敛：搜索栏内部事件不再调用会抢走输入框焦点的
  global `app-focus.focus()`，`focus_request` 会立即并延后一拍重试 focus，避免搜索框打开、debounce
  submit 或筛选变化后失焦。
- search filter popup 按 Dolphin `QToolButton::InstantPopup` + `WidgetMenu` 结构收敛：Filter
  按钮下沿打开紧凑主面板，主面板内部是扁长 selector 控件；点击 selector 或 active chip
  时打开独立下拉列表，等价于 Dolphin 里 `FileTypeSelector(QComboBox)` 和
  `DateSelector(QToolButton::InstantPopup)` 的两级交互，不再把整个 popup 切换成大列表页面。
  主面板和下拉都按窗口可用宽度/高度 clamp，避免横向或纵向溢出。
- `PaneState` 缓存 unfiltered entry summary，目录加载结果携带后台已计算 summary；状态栏和无过滤
  count summary 不再在 UI 线程重新扫描 entries。
- 已补结构测试覆盖 scroll/zoom 同步边界、thumbnail flush gate、fallback icon lazy kinds、slot stats
  热路径形态、本地搜索后台索引和搜索框关闭/Escape 行为。

已通过验证：

- `cargo fmt --check`
- `cargo check`
- `cargo test app::model_update`
- `cargo test app::geometry`
- `cargo test search`
- `cargo test visible_entry_index`
- `cargo test filtered`
- 全量 `cargo test`
- `cargo build --release`

仍待完成：

- 在真实 GUI 中跑 `10k-flat`、`mixed-icons`、`photos`、`split-view` 等 workload，采集 debug/release
  p95、max 和 hotspot breakdown。
- 根据采样结果判断是否需要继续做 frame-level latest-only zoom coalescing、inactive pane 下一帧提交
  或更深的 Slint row 拆分。
- 更新本文档 Phase 6 表格中的实测 p95；当前不得填入猜测值。

## Current Hot Paths

### Scroll

滚轮路径：

```text
SplitPaneView.handle-scroll()
  -> pan-horizontal()
  -> set-viewport-x(raw, smooth=true)
  -> view_changed()
  -> PaneViewSyncScheduler::request()
  -> sync_pane_viewport_for_slot()
  -> sync_virtual_entries_for_slot_with_count_and_cache_policy()
```

当前设计让 logical viewport 立即驱动 scrollbar、hit-test 和 visible slice；`paint-viewport-x`
只负责平滑绘制偏移。即使当前 virtual slice 覆盖目标可见范围，Rust 仍会收到
`view_changed()`，然后通过 cached viewport path 退出。

### Zoom

Ctrl+wheel / toolbar zoom 路径：

```text
SplitPaneView.handle-scroll(control=true)
  -> zoom_in()/zoom_out()
  -> AppWindow.icon_zoom_level changed
  -> icon_zoom_layout_changed()
  -> PaneLayoutSyncScheduler::set_icon_zoom_level_now()
  -> apply_visible_pane_zoom_style_options()
  -> apply_pane_zoom_style_option_for_slot()
  -> sync_virtual_entries_for_slot_with_count(... schedule_thumbnails=false, immediate=true)
  -> start_virtual_view_prepare()
  -> tokio::task::spawn_blocking(snapshot + entry/bounds/metadata/slot projection)
  -> AsyncEvent::VirtualViewPrepared
  -> apply_virtual_view_result()
  -> set_pane_virtual_entries()
  -> sync_pane_view_ui()
```

thumbnail/preview role 调度已用 300ms `IconSizeUpdateScheduler` 合并。zoom 的 request/generation
仍同步进入 Rust，但 snapshot、layout projection、metadata projection、bounds projection 和 slot
projection 不在 UI 线程执行；UI 线程 apply 阶段只做 latest result 检查、当前 selection/thumbnail
cache 合并、slot reuse/patch 和 Slint model writes。连续 zoom 的 immediate commit 会 defer tile
raster 和 fallback icon 新尺寸渲染，等 icon-size timer 的最终 refresh 再重建。

### Search / Filter

本地搜索输入路径：

```text
SearchPanel.TextInput changed text
  -> 500ms debounce
  -> search_submitted(slot, query, recursive)
  -> submit_search_for_slot()
  -> apply_filter_for_slot()
  -> start_local_search_index_prepare()
  -> tokio::task::spawn_blocking(prepare_visible_entry_index)
  -> AsyncEvent::LocalSearchIndexPrepared
  -> apply_local_search_index_result()
  -> sync_virtual_entries_for_slot_with_count_and_cache_policy(force_uncached_prepare=true)
```

输入框和 pane search state 仍同步更新 request/generation；O(n) 的 visible index 构建、summary/path
收集和 location group 预计算放到后台。pending 期间保留当前已提交 view，`sync_virtual_entries...`
直接返回，避免“新 query + 旧 visible index”生成错误 slice。选择/框选/Select All 在 pending
状态下按当前已提交 view 工作，不在 UI 线程按新 query 临时重扫。

## Dolphin Source Notes

对照源码来自本地 `/home/yk/Code/dolphin`，commit `2a72145eb`。这些点是本计划的边界：

- `KItemListView::setScrollOffset()` 会 clamp 负 offset，offset 未变直接返回；offset 变化后同时
  更新 layouter 和 animation，然后无条件同步 `doLayout(NoAnimation)`。源码注释明确说 scroll
  offset 必须同步 layout，否则 smooth scrolling 会抖。
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:167`
- `KItemListViewLayouter::setScrollOffset()` 只设置 `m_visibleIndexesDirty = true`；
  `updateVisibleIndexes()` 用 row offsets 二分计算 first/last visible index。也就是说 Dolphin
  scroll 是同步进入 layout，但 hot work 收敛到 visible-index 更新和 widget reuse，不是重建全模型。
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:149`
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:543`
- `KItemListView::doLayout()` 在 scroll 时拿 first/last visible index，回收完全不可见的
  `KItemListWidget`，对仍可见 widget 只更新 position/size/icon size。`KItemListViewLayouter::itemRect()`
  在 horizontal orientation 下把逻辑垂直方向转置成物理横向滚动，并直接从 item rect 里减
  `m_scrollOffset`。
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:1861`
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:220`
- `KItemListSmoothScroller::scrollContentsBy()` 动画的是目标对象的 `scrollOffset` property；
  连续 wheel 会调整新的 animation start/end，scrollbar press/release 和 maximum 变化会影响动画状态。
  Fika 的 `paint-viewport-x` 只是 Slint 适配层；Rust logical viewport 不能长期落后。
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:81`
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:160`
- `KItemListSmoothScroller::requestScrollBarUpdate()` 在 animation running 且 maximum 未变时不更新
  scrollbar；maximum 改变说明内容变化，会停止动画并立即更新。Fika 的 `scroll-max-x`、slice
  geometry 或 relayout 变化也必须停止 smooth paint offset。
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:142`
- `DolphinItemListView::setZoomLevel()` clamp level 后立即 `updateGridSize()`；
  compact layout 的公式是 `itemWidth = padding * 4 + iconSize + fontMetrics.height() * 5`，
  `itemHeight = padding * 2 + max(iconSize, textLines * lineSpacing)`，并用
  `beginTransaction(); setStyleOption(option); setItemSize(...); endTransaction();` 合并成一次 layout。
  - `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp:34`
  - `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp:176`
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:665`
- `KItemListView::setStyleOption()` 会更新所有 visible widgets 的 style、清 size-hint cache、
  mark layouter dirty 并 layout；`setItemSize()` 会清 size-hint cache 并 layout。由于
  Dolphin 用 transaction 包住 zoom 的 style + size 变更，最终只在 `endTransaction()` 做一次 layout。
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:874`
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:916`
- `KFileItemModelRolesUpdater::setIconSize()` 在 preview shown 时清 finished items 并
  `startUpdating()`；`startUpdating()` 先同步更新 visible icons，再按 `indexesToResolve()`
  启动 preview job 或 0ms async role resolving。`indexesToResolve()` 顺序是可见文件、可见目录、
  向后 read-ahead、向前 read-ahead、末页、首页，再补到 `ResolveAllItemsLimit`。
  - `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:142`
  - `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:181`
  - `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:887`
  - `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:1430`
- `Search::Bar::keyPressEvent()` 对 Escape 的处理是：搜索输入为空则隐藏 search bar，不为空则只清
  search term；`FilterBar::keyPressEvent()` 同样是文本为空发 closeRequest，不为空只 clear input。
  `FilterBar` 的 close button 只发 closeRequest，输入框 textChanged 只发 filterChanged，不在控件
  内部拆散 Rust/容器状态清理。
  `Search::Bar`/`FilterBar` 都把 focus proxy 设置到输入框；Search filter button 是
  `QToolButton::InstantPopup` + `setMenu(m_popup)`，不是单独抢焦点后再显示大偏移面板。
  `WidgetMenu::showEvent()` 在非自发 show 后把焦点交给内部 widget。
  `Search::Popup::init()` 用 `QGridLayout` 放置高级过滤控件；`FileTypeSelector` 继承
  `QComboBox`，`DateSelector` 是 `QToolButton::InstantPopup` + `KDatePickerPopup`，点击内部
  selector 应打开自身下拉，而不是替换整个 popup 内容。
  - `/home/yk/Code/dolphin/src/search/bar.cpp:48`
  - `/home/yk/Code/dolphin/src/search/bar.cpp:70`
  - `/home/yk/Code/dolphin/src/search/bar.cpp:82`
  - `/home/yk/Code/dolphin/src/search/widgetmenu.cpp:66`
  - `/home/yk/Code/dolphin/src/search/popup.cpp:201`
  - `/home/yk/Code/dolphin/src/search/selectors/filetypeselector.cpp:18`
  - `/home/yk/Code/dolphin/src/search/selectors/dateselector.cpp:22`
  - `/home/yk/Code/dolphin/src/search/selectors/dateselector.cpp:30`
  - `/home/yk/Code/dolphin/src/search/bar.cpp:279`
  - `/home/yk/Code/dolphin/src/filterbar/filterbar.cpp:46`
  - `/home/yk/Code/dolphin/src/filterbar/filterbar.cpp:39`
  - `/home/yk/Code/dolphin/src/filterbar/filterbar.cpp:192`

结论：Fika 的 scroll 和 zoom 都不能走长延迟。scroll 应对齐 Dolphin 的 synchronous
scroll-offset boundary：logical viewport 立即进入 Rust，hot work 只更新 visible range、
slot position 和必要的新入/离开 slots。zoom 应对齐 Dolphin 的 transaction boundary，但要注意
Dolphin 同步 `doLayout()` 的主体是 visible widget reuse/position update，不是重新做完整模型投影、
raster 或 icon 渲染。Fika 保留同步 request/generation 边界，heavy snapshot/projection/slot projection
走 latest-only 后台 prepare；UI 线程只 apply 最新 visible slice、合并当前 thumbnail/selection 状态并
执行 Slint row writes。search/filter 也保留输入和状态同步边界，但 visible index/summary/group
构建走 latest-only 后台 prepare；搜索框自身只处理本地输入、timer 和 close/Escape 信号，不拆散
Rust 侧状态清理。preview/thumbnail role 更新继续独立合并并按 visible range 排序。

## Concrete Source Mapping

| Area | Dolphin source | Dolphin behavior | Fika source | Fika constraint / planned check |
|------|----------------|------------------|-------------|----------------------------------|
| Scroll input to offset | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:167-185` | `setScrollOffset()` clamps, exits if unchanged, updates layouter + animation, then synchronously `doLayout(NoAnimation)` | `ui/split_pane.slint:111-121`, `src/main.rs:181-199`, `src/main.rs:5220-5246` | Keep logical viewport synchronous. Instrument no-op/clamped scroll, cached scroll, prepare scroll; do not move scroll layout to a timer. |
| Scroll dirty state | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:149-154` | `setScrollOffset()` only changes `m_scrollOffset` and marks `m_visibleIndexesDirty` | `src/main.rs:4397-4430`, `src/main.rs:4518-4527` | Cached scroll should only update viewport state or focused `entry_count` if required; no raster/fallback icon/slot model work on cache hit. |
| Visible range calculation | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:543-600` | `updateVisibleIndexes()` returns early if not dirty and uses row offsets binary search for first/last visible item | `src/main.rs:4206-4213`, `src/main.rs:4420-4422` | Measure `virtual_plan()` and cache-cover checks separately; if p95 is high, optimize visible range math before touching rendering. |
| Horizontal compact projection | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:220-234` | horizontal orientation transposes logical vertical flow and subtracts `m_scrollOffset` in item rect | `ui/split_pane.slint:172-182`, `ui/split_pane.slint:428-484` | `paint-viewport-x` may animate visual offset, but Rust viewport and item hit-test coordinates must stay logical and current. |
| Visible widget reuse | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:1861-1922` | `doLayout()` uses first/last visible index, recycles invisible widgets, and creates only missing visible widgets | `src/app/model_update.rs:304-473`, `src/app/model_update.rs:494-523` | Slot allocator stats must report reused slots, inactive slots, extended slots and changed rows. Still-visible items must keep slot id on scroll. |
| Smooth scroll animation | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:81-128` | animation changes target `scrollOffset`; interrupted wheel adjusts start/end to avoid skipped range | `ui/split_pane.slint:109-130`, `ui/split_pane.slint:180-182` | Keep `paint-viewport-x` as visual-only adaptation. Do not let Rust logical viewport lag behind smooth paint animation. |
| Scrollbar maximum changes | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:142-150` | if maximum changes during animation, content changed; stop animation and update immediately | `ui/split_pane.slint:231-265`, `ui/split_pane.slint:267-269` | `scroll-max-x`, rows-per-column, width, virtual slice geometry and relayout must stop smooth paint and commit current slice. |
| Zoom level input | `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp:34-66` | `setZoomLevel()` clamps level, exits if unchanged, updates icon/preview size, calls `updateGridSize()` immediately | `ui/split_pane.slint:189-198`, `ui/app.slint:1367-1369`, `src/main.rs:301-306` | Keep zoom layout immediate. Instrument repeated same-level zoom no-op and event count. |
| Compact zoom geometry | `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp:176-254` | compact item size uses padding/icon/font metrics; style option + item size are applied together | `src/app/item_view_metrics.rs:23-50`, `src/app/split_view.rs:432-450`, `src/main.rs:5157-5180` | Single zoom event should compute render/layout metrics once per visible pane and commit once. |
| Zoom transaction | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:665-684` | `beginTransaction()` suppresses intermediate layouts; `endTransaction()` runs one final `doLayout()` | `src/main.rs:5157-5180`, `src/main.rs:4175-4350`, `src/main.rs:4474-4625` | Phase 2 must verify one zoom event -> one `sync_virtual_entries_for_slot...` and one `sync_pane_view_ui()` per visible pane. |
| Style/item size side effects | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:874-912`, `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:916-953` | `setItemSize()` and `setStyleOption()` clear size-hint cache, update visible widgets, mark layouter dirty and layout | `src/app/split_view.rs:353-429`, `src/app/split_view.rs:453-505` | Zoom-side `PaneViewData` construction must be measured: metrics, raster, fallback icons and slot model separately. |
| Icon-size roles update | `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:142-153`, `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:887-970` | icon size change clears finished previews, synchronously updates visible icons under timeout, then starts preview/role work | `src/main.rs:206-238`, `src/main.rs:5183-5217`, `src/app/file_item_roles_updater.rs:19-21`, `src/app/file_item_roles_updater.rs:37-49`, `src/app/file_item_roles_updater.rs:123-143` | Keep 300ms `IconSizeUpdateScheduler` limited to thumbnail/preview roles. It must not gate layout. |
| Search focus / filter popup | `/home/yk/Code/dolphin/src/search/bar.cpp:48-82`, `/home/yk/Code/dolphin/src/search/widgetmenu.cpp:66-70`, `/home/yk/Code/dolphin/src/search/popup.cpp:201-239`, `/home/yk/Code/dolphin/src/search/selectors/filetypeselector.cpp:18-83`, `/home/yk/Code/dolphin/src/search/selectors/dateselector.cpp:22-41` | search/filter bars proxy focus to input; filter button uses Qt menu popup under the button; popup embeds flat selector controls whose own combo/menu opens on click | `ui/app.slint:1210-1229`, `ui/search_panel.slint:468-742`, `ui/app.slint:2331-2370` | Search-internal routes must not call global focus scope; Filter opens a compact panel below the button; selector/chip clicks open a separate dropdown below the selector instead of switching the whole popup into a list page. |
| Search input Escape/close | `/home/yk/Code/dolphin/src/search/bar.cpp:279-289`, `/home/yk/Code/dolphin/src/filterbar/filterbar.cpp:192-200` | Escape clears non-empty input and closes only when empty; close button emits close request | `ui/search_panel.slint:370-434`, `ui/app.slint:1210-1214`, `src/main.rs:3404-3499` | SearchPanel owns local input/timer only. Rust owns close/clear state. Programmatic query clear must use `search_query_sync_request` rather than overwriting active typing on every pane update. |
| Local search/filter index | Dolphin search/filter state is committed by view container/model, while item view layout still operates on visible model/indexes | Avoid doing full model filtering inside item-view scroll/zoom paths | `src/main.rs:5006-5177`, `src/app/selection.rs:195-286`, `src/app/events.rs:66-75` | `apply_filter_for_slot()` must start latest-only background visible-index prepare. UI thread must not scan all entries on each debounced search input or filter chip change. |

## Measurement First

不要先猜。新增 `FIKA_PERF_ITEM_VIEW=1` 后输出结构化单行日志，并在 1s 窗口或退出时打印
summary。所有日志必须可关闭，默认零成本或接近零成本。

需要记录：

- input: scroll event count、zoom event count、slot、zoom level、viewport-x、window width。
- sync entry: `PaneViewSyncScheduler::request()` 次数、re-entrant skip 次数。
- virtual sync: cached hit / prepare / deferred / stale result 次数，immediate vs async。
- prepare: `prepare_virtual_view_snapshot_update()` 总耗时，拆分 layout/cache/snapshot/metadata。
- apply: `apply_virtual_view_result()` 总耗时，拆分 entry projection、thumbnail decoration、
  metadata projection、bounds generation、`set_pane_virtual_entries()`、`sync_pane_view_ui()`。
- slot pool: active slot 数、patched rows、inactive rows、extended rows、thumbnail image reuse /
  replace 次数、`set_row_data()` 次数。
- raster: cache hit / miss、render time、raster width/height/pixels、revision bump reason。
- fallback icons: cache hit / miss、rendered icon kind count、render time。
- thumbnail flush: flush batch size、触发 `sync_virtual_entries_for_slot(... schedule=false)` 次数，
  与 zoom pending 重叠次数。
- Slint model writes: pane slot/view/surface row writes、item slot row writes、model extend/remove。

输出示例：

```text
[fika perf] zoom slot=0 level=7 panes=1 prepare_ms=3.4 apply_ms=5.8 raster_ms=1.1 slot_rows=84 patched=84 icons=10 model_writes=87
[fika perf] scroll slot=0 cached=true viewport=1840 range=72..168 sync_ms=0.18 model_writes=0
```

## Test Workloads

1. `10k-flat`: `/tmp/fika-perf-10k`，10000 个普通文件，无 thumbnail。
2. `mixed-icons`: 10000 个混合扩展名文件，覆盖 file/image/video/audio/archive/pdf/text/code/executable
   fallback kind。
3. `photos`: 500-2000 张图片，thumbnail cache 冷/热各跑一次。
4. `recursive-search`: 500+ 带 group/location 的搜索结果，打开 show-location metadata。
5. `split-view`: 左右 pane 都可见，分别测试 focused-only scroll 和全局 zoom。
6. `end-boundary`: 大目录末尾快速滚动、快速 zoom，确认没有空白和重复重建。

每个 workload 记录 debug build 和 release build。体感判断必须绑定日志：描述卡顿时同时给出
对应的 p95、max 和热点 breakdown。

## Phase 0: Instrumentation

目标：一轮工作内拿到可信 baseline。

- 增加 `src/app/item_view_perf.rs` 或等价模块，提供轻量 timer/counter。
- 在 scroll 和 zoom 入口生成 event id，贯穿 Rust hot path。
- 给 `model_update.rs` 的 slot allocator 返回统计，而不是只返回 bool。
- 给 `PaneView::tile_frame_raster_layer()` 记录 cache hit/miss 和 render time。
- 给 fallback icon cache 记录 miss 时实际渲染的 icon kind 数。
- 给 thumbnail flush 记录是否发生在 icon-size timer pending 期间。
- 增加结构测试，确保 perf 开关默认不打印、不改变 hot path 语义。

验收：

- `FIKA_PERF_ITEM_VIEW=1 target/debug/fika <dir>` 能输出 scroll/zoom breakdown。
- 无开关时 `cargo test` 不依赖时间，也不产生 stderr 噪声。

## Phase 1: Dolphin Scroll Offset Boundary

假设：滚动卡顿不是因为同步进入 Rust 本身，而是 Fika 的 synchronous scroll path 做了超过
Dolphin `setScrollOffset() + updateVisibleIndexes() + widget reuse` 边界的工作，例如完整
`PaneViewData` 构造、raster/fallback icon 路径、slot row patch 或 Slint model write。

方案：

- 保留 logical viewport 同步进入 Rust，不把滚动 layout 延迟到 timer。
- 仪表化 `sync_pane_viewport_for_slot()`，把 scroll 分成 Dolphin 对应的几类：
  - offset unchanged / clamped no-op。
  - current virtual slice covers visible window：只更新 viewport state，不构造 `PaneViewData`，不碰
    raster/fallback icon/slot model。
  - visible range changed but overlap 高：只复用/patch changed slots，仍可见 slots 保持 slot id。
  - range jump / relayout / scroll-max changed：停止 smooth paint offset，完整提交当前 slice。
- 审计 cached scroll path，确保命中缓存时不调用 `sync_pane_view_ui()`，不生成
  `pane_slot_tile_frame_raster()`，不渲染 fallback icons。
- 如果高频 cached scroll 的 FFI/state 开销仍是主因，再评估把 Slint 回调拆成
  `viewport_changed(slot, viewport_x)` 和 `view_slice_changed(slot)`；这个拆分只能作为 no-op
  fast path，不能让 Rust logical viewport 长期落后。
- `scroll-max-x`、virtual slice 起点/宽度、rows-per-column 或 pane width 变化时必须调用
  `stop-smooth-scroll()`，对齐 Dolphin maximum 改变时停止 animation 的处理。

验收：

- 当前 slice 覆盖范围内连续滚动时，`prepare_virtual_view_snapshot_update()` 次数为 0，
  `sync_pane_view_ui()` 次数为 0，raster/icon cache miss 为 0。
- visible range 只滑动一列时，仍可见 item 保持 slot id，只 patch 新入/离开和必要坐标 slots。
- cached scroll p95 明显低于 baseline，且 hit-test、选择框、右键命中坐标不回归。

## Phase 2: Dolphin Zoom Transaction Boundary

假设：zoom 卡顿主要来自 Fika 在一次 zoom level 变更中做了超过 Dolphin transaction boundary
需要的工作，例如重复 view sync、重复 raster/icon cache miss、thumbnail flush 插入、或 split
view 两个 pane 同帧全量提交。

方案：

- 对齐 Dolphin `beginTransaction()/endTransaction()`：一次 zoom 只允许一次 visible layout commit。
- 审计 `icon_zoom_layout_changed()` 到 `sync_pane_view_ui()`，确认没有在同一 zoom event 中重复
  `sync_virtual_entries_for_slot_with_count()` 或重复完整 `PaneViewData` 构造。
- 保留 zoom layout 立即提交；不能把 layout 延迟到 300ms thumbnail timer。
- 如果连续 Ctrl+wheel 仍在一帧内产生多次 layout 且 perf breakdown 证明它是主因，再评估
  0ms event-loop post 或 8-16ms single-shot latest-only `ZoomLayoutScheduler`。该 scheduler
  只能合并同一帧内的 latest zoom，不能引入可感知延迟。
- split view 下先测 focused pane 和 inactive pane 各自成本；只有 inactive pane 明确超预算时，
  才考虑把 inactive pane 的 zoom commit 延到下一帧。
- 300ms `IconSizeUpdateScheduler` 继续只负责 thumbnail/preview roles，不提前调度。

验收：

- 单次 zoom event 中每个 visible pane 最多一次 layout commit 和一次 `sync_pane_view_ui()`。
- 单次 zoom 立即可见；不能恢复旧的长延迟空白。
- 连续 Ctrl+wheel 的 latest-only 合并只有在 baseline 证明需要时才启用，且 focused pane p95
  优于 baseline。

## Phase 3: Zoom Raster and Fallback Icon Cost

假设：zoom 每档都会改变 raster/input signature 和 fallback icon size，导致 tile raster miss 和一次性
渲染 10 种 fallback icon。

方案：

- fallback icon cache 从 single-entry 改为小 LRU，key 为 `(width, height, dark, kind)` 或
  `(width, height, dark)` 多签名。
- 只渲染当前 active slots 实际使用的 media kinds；普通目录通常不需要一次渲染 10 种 icon。
- zoom 正在连续输入时，评估使用 `PaneView::set_raster_updates_deferred(true)` 复用上一张 selection/drop
  raster；zoom idle 后重建最终 raster。
- raster defer 只能影响 selection/drop base layer，不能影响 title/fallback/thumbnail 的最终位置。

验收：

- zoom 期间 fallback icon miss 的 render kind count 从固定 10 降为实际使用 kind 数。
- raster render time 在连续 zoom 中不再成为 p95 主因，且选中背景/drop target 不出现长期错位。

## Phase 4: Slot Patch and Projection Cost

假设：zoom 时 visible item 内容没变，但 geometry 变了，导致所有 active slots `set_row_data()`；
同时 Rust 侧存在重复 token/projection 构造成本。

候选改动：

- 生产路径已把 frame batch/slot projection 放到后台 prepare；后续若数据证明仍有问题，再让
  `update_pane_item_view_entries_model_with_slot_projections()` 只构造一次 `ItemViewRowToken`，同时用于
  raster token diff 和最终 `view.virtual_entry_tokens`。
- 记录 `bounds_changed` 的原因，避免在可证明 range/layout 未变时做整 Vec bounds 比较。
- 如果 `set_row_data()` 成为 zoom 主成本，评估把 slot row 拆成 content row + geometry row，或把
  zoom 派生几何进一步移到 pane-level 公式，只保留真正 per-item 的 `x/text_width`。
- 不为了理论清晰引入复杂拆分；只有 perf breakdown 证明 Slint row patch 是主因才执行。

验收：

- slot allocator 日志能区分 content patch、geometry-only patch、thumbnail patch。
- zoom geometry-only patch p95 低于 baseline；thumbnail image 不因 zoom 重复替换。

## Phase 5: Thumbnail Isolation During Zoom

假设：图片目录 zoom 卡顿可能来自 thumbnail flush 与 zoom layout 同时发生，flush 又触发
`sync_virtual_entries_for_slot(... schedule=false)`。

方案：

- icon-size timer pending 或 zoom layout pending 时，thumbnail results 先写 Rust cache/pending state，
  UI slot patch 延后到当前 zoom frame 结束。
- flush scheduler 仍保留 16ms batch，但需要记录被 zoom gate 合并的数量。
- zoom idle 后按 latest visible slice patch thumbnails；离屏结果只进 cache，不触发 view sync。

验收：

- `photos` workload 中连续 zoom 时 thumbnail flush 不再插入额外 visible sync。
- zoom 停止后 thumbnail 能在下一批 flush 正常出现。

## Phase 6: Verification and Closeout

每个 phase 完成后更新本文档的结果表：

| Workload | Baseline p95 | After p95 | Max | Main Hotspot Before | Main Hotspot After |
|----------|--------------|-----------|-----|---------------------|--------------------|
| 10k-flat scroll | TBD | TBD | TBD | TBD | TBD |
| 10k-flat zoom | TBD | TBD | TBD | TBD | TBD |
| mixed-icons zoom | TBD | TBD | TBD | TBD | TBD |
| photos zoom cold | TBD | TBD | TBD | TBD | TBD |
| split-view zoom | TBD | TBD | TBD | TBD | TBD |

必须跑：

- `cargo fmt --check`
- `cargo check`
- `cargo test app::model_update`
- `cargo test app::geometry`
- 全量 `cargo test`
- 至少一次 release build 手工滚动/zoom smoke test

## Risks

- 过度 coalesce zoom 会让 UI 变得迟钝；frame-level latest-only 可以接受，300ms layout 延迟不可接受。
- scroll callback split 不能让 Rust viewport 状态长期滞后，否则 hit-test、selection rectangle、context menu
  会错位。
- raster defer 只能作为连续 zoom 的短暂策略；最终 raster 必须按最新 revision 重建。
- fallback icon LRU 不能无界增长，zoom level 0..16、dark/light 和 media kind 已足够限制 key 空间。
- 如果 Slint `Text` primitive 本身是 zoom p95 主因，文字 raster 只能作为实测后的独立实验，不纳入
  默认路径。

## Non-goals

- 不重做 Details/Icons layout。
- 不恢复旧 Slint-facing item/bounds/thumbnail model。
- 不用主观体感替代 baseline；性能结论必须能从日志或 profiler 复现。
