> 本文是 [SCROLL_ZOOM_PERFORMANCE_PLAN.md](SCROLL_ZOOM_PERFORMANCE_PLAN.md) 的简体中文翻译。

# 滚动与缩放性能计划

> 当前 GPUI 入口点，以及已归档的 Slint 时代调查。旧的 Slint 笔记保留在下方作为历史参考；活跃的 item-view 滚动/缩放工作必须遵循 `docs/ITEM_VIEW_CUSTOM_PAINT_DESIGN.md`、
> `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md` 和
> `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 中确定的 GPUI/Dolphin 边界。

## 当前 GPUI 计划

当前 GPUI item-view 路径将滚动和缩放视为 retained state 更新，而不是在渲染帧中重建 item identity 或解析文件角色的机会。

### Dolphin 对齐边界

- 滚动更新 pane `ViewState`、可见范围、slot 几何、retained hit testing 和 paint snapshot。它不得同步扫描图标主题、探测缩略图或读取 MIME magic。
- 缩放会改变 item 指标，并可能使 layout/文本/图片几何失效，但 model 角色保持在 role/update 侧。在解析后的角色数据就绪之前，帧可以使用初步的或 retained 同源图标 snapshot。
- `raw_file_grid_snapshot()` 拥有可见/工作范围。调度器投影队列负责 metadata 角色、缩略图和文件图标主题解析工作。
- `VisibleItemSnapshotCache`、paint slot、文本形状缓存和 GPUI `RetainAllImageCache` 是应吸收重复滚动/缩放工作的 retained-state 层。

### 当前修复

- 文件图标主题路径解析不再在 raw-to-render snapshot 转换期间同步进行。帧路径调用 `FileIconCache::cached_or_preliminary_icon_for()`。可见图标预热使用 Dolphin `updateVisibleIcons()` 索引顺序，后台 batch 按 Dolphin `indexesToResolve()` 可见/预读顺序解析主题图标路径。
- 当后台图标解析完成时，可见 item snapshot 缓存被失效，以便在下一帧替换初步 fallback 图标。
- 缩略图和主题图标图片的 pending/failure 状态不再必须直接降级到 fallback。Compact/Icons 和 Details 维护一个 pane 本地 retained 图片映射：MIME/主题图标按 `iconName` 保留，缩略图按精确缩略图路径保留。因此缩放级别的路径变化可以在 GPUI 解码新资源期间继续绘制前一张真实的 MIME 图标，匹配 Dolphin 的 `KStandardItemListWidget::m_pixmap` 行为。当从未对该语义源解码过真实图片时，仍使用 fallback。
- 预读条目保留在 raw/render snapshot 中用于调度器投影和缓存保留，但它们不再进入静态视觉或图片 prepaint。这匹配 Dolphin 的拆分：`KItemListView` 绘制可见 widget，`KFileItemModelRolesUpdater::indexesToResolve()` 处理 paint 帧外的预读角色工作。
- 缩放精确尺寸主题图标未命中时，复用同文件图标类型已经解析出的稳定 theme path，并且不再为新尺寸排队另一个 exact-size path 请求。这镜像了 Dolphin 的视觉稳定性行为：不要仅因为新缩放级别改变了图标边界，就把真实可见图标替换为 fallback 标记或提交第二个 image identity。
- 活动缩放现在镜像 Dolphin 的普通主题图标 paint 路径。Item layout 和 icon bounds 立即变更；一旦同一文件图标类型已有 resolved theme path，文件图标 role/path identity 保持稳定。Dolphin 的 300ms `triggerIconSizeUpdate()` timer 被视为 preview/role-updater 边界，而不是 Fika 主题图标的延迟第二次尺寸或路径提交。
- 图片 paint 层现在在路径解析后也应用相同规则：如果 GPUI `RetainAllImageCache::load()` 返回新图标路径的 pending/error，painter 首先尝试同 MIME 图标名称的 retained 图片。这避免了滚动或缩放图片支持的 MIME 图标时出现的概率性 fallback 闪烁。
- 主题图标文件解码不在 GPUI prepaint 中同步执行。解码保持在 GPUI 的 image-cache 路径上；paint 使用 retained 同 `iconName` 图片以避免可见的空白/标记回退。
- 目录加载 MIME 图标稳定性现在遵循 Dolphin 的 visible-widget 边界。Dolphin 在 MIME 未知时避免在 `KFileItemModel::retrieveData()` 中进行昂贵的 `KFileItem::iconName()` 调用，但 `KFileItemModelRolesUpdater::startUpdating()` 调用 `updateVisibleIcons()`，且 `KFileItemListView::initializeItemListWidget()` 为实际创建的 widget 填充 `iconName`。Fika 镜像此行为：在排队后台 metadata 工作之前，在较小帧预算内同步解析可见的通用 MIME metadata；预读和离屏条目仍使用异步角色调度器。

### 2026-06-17 突破：MIME 图标加载、缩放和滚动稳定性

此记录捕获了近期 `/etc` 和缩放稳定性修复的根本原因和已接受的实现。在再次更改 MIME/主题图标渲染之前，将其保留为比较基准。

症状：

- 加载 `/etc` 显示了可见的空白/占位符到 MIME 图标的级联过渡。
- 缩放可能在 item 几何已经改变后显示第二次图标尺寸调整。
- 即使 MIME/主题图标已从自定义图片 painter 中移出，初始 `/etc` 滚动/缩放 autosmoke 仍产生间歇性卡顿。

根本原因：

- 自定义 MIME/主题图标 painter 可能在 GPUI 图片缓存解码主题图标资源之前进入首次 paint。在 `/etc` A/B smoke 中记录了 `theme_placeholder=48`，匹配可见的占位符级联。
- Fika 最初将 Dolphin 的 300ms `triggerIconSizeUpdate()` 延迟视为图标尺寸防抖。Dolphin 仅在那里延迟 preview/role-updater 工作；普通的 `iconName` pixmap 是从 widget 当前 style option 图标尺寸生成的。因此延迟或冻结 Fika 主题图标尺寸在缩放期间产生了可见的第二次尺寸提交。
- 可见图标同步重复了已为预读图标解析排队的工作。渲染器拆分后的第一次 autosmoke 在 geometry-change 帧上记录了 `icon_sync=28340us` 和 `total=29451us`。

实现：

- MIME/主题图标现在默认使用 GPUI `img()` 元素而非 retained item shell。自定义主题图标图片 painter 仅通过 `FIKA_CUSTOM_THEME_ICONS=1` 保持可用，用于配对的 A/B 证据。
- 渲染转换仅使用缓存或初步图标 snapshot。主题图标路径扫描保持在可见图标同步和后台解析队列中，不在 GPUI prepaint 或渲染转换中。
- 可见图标同步跳过已在 `FileIconResolveQueue` 中排队或 pending 的请求，保留 Dolphin 的可见优先例外而不在滚动帧中重做预读扫描。
- 缩放立即提交当前 layout 图标边界；MIME/主题图标 path 在同一文件图标类型首次解析后保持稳定，不再因新缩放尺寸同步或排队 exact-size path 请求。Preview/thumbnail role 工作可能仍然合并，但主题图标几何不得使用延迟的第二次尺寸。
- 目录加载在排队离屏 metadata/图标工作之前，在有界的 visible-widget 预算内解析可见通用 MIME metadata 和可见主题图标路径。

证据：

```text
custom-theme /etc A/B: theme_placeholder=48, gpui_image_element=0
default /etc A/B:      theme_placeholder=0,  gpui_image_element=48

before queued/pending skip:
  icon_sync=28340us, geometry-change total=29451us

after queued/pending skip:
  icon_sync=173us, geometry-change max_total=1635us
```

回退防护：

- 对于渲染器更改，使用 `scripts/compare-item-image-renderers.sh` 比较默认和 `FIKA_CUSTOM_THEME_ICONS=1` 日志。
- 对于滚动/缩放更改，运行
  `FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc`
  并使用 `scripts/analyze-item-view-perf.sh` 汇总。
- 如果 `icon_sync` 回到多毫秒值，在更改渲染器之前检查可见/预读图标队列所有权。
- 如果 `icon_sync` 保持低位但帧仍然缓慢，检查静态视觉 paint、文本塑形或 GPUI image-cache 行为，而不是归咎于 MIME 图标路径查找。

### 待完成的验证工作

- 收集 `/etc` 初始滚动和普通目录在 Compact 和 Icons 中初始缩放的桌面会话日志：

  ```sh
  FIKA_PERF_ITEM_VIEW=1 cargo run -- /etc 2>&1 | tee /tmp/fika-etc-scroll.log
  FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads 2>&1 | tee /tmp/fika-downloads-zoom.log
  ```

- 在这些日志中验证：
  - 热滚动/缩放 `convert=` 不被同步图标工作主导
  - 存在图片支持条目时出现 `[fika item-image]`
  - 缩放期间无重复空白缩略图/图标帧可见
  - 加载目录时初始可见范围不显示从初步 MIME 图标到已解析 MIME 图标的可见级联
  - 冷首帧工作与稳态滚动/缩放阶段分离
- 在将更多工作移入自定义 paint 之前持续对比 Dolphin 源码。如果 GPUI 内置渲染器对某层更快，保留 retained Dolphin 风格 model/controller 边界并将该层留在 GPUI 上。

## 已归档的 Slint 时代调查

## 范围

用户体感问题集中在主文件视图的滚动和 zoom，不是目录刷新。当前 Dolphin-style slot reuse
架构已经收尾，但还没有真实性能基线；本计划先建立可重复测量，再按数据处理热点。

## 实现状态

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
- `ui/app.slint`/`split_view.rs` 移除重复的 `pane_views` model；`PaneViewData` 只在
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
  virtual sync 不混用"新 query + 旧 index"，选择/框选/Select All 也不会绕回按新 query 全量重扫。
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

## 当前热路径

### 滚动

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
`view_changed()`，然后通过 cached viewport 路径尽早返回。

### 缩放

缩放输入路径：

```text
StatusBar.zoom-slider-changed()
  -> PaneView.zoom-level-changed()
  -> set-zoom-level(raw)
  -> SplitPaneView.apply-zoom-level()
  -> Rust: handle_zoom_level_changed()
  -> icon_zoom_layout_changed()
  -> PaneViewSyncScheduler::request()
  -> sync_pane_viewport_for_slot()
  -> sync_virtual_entries_for_slot_with_count_and_cache_policy()
```

当前设计把 zoom level change 视为 immediate sync request，推进 `pane.generation` 并启动
latest-only prepare。300ms `IconSizeUpdateScheduler` timer 只负责 thumbnail/preview roles；
icon geometry 和 theme-icon size 已在 layout 中立即提交。

### 关键调用拆分

- `sync_pane_viewport_for_slot()` 是 scroll 和 zoom 的单一 Rust 入口；它区分 cached/no-op、
  prepare 和 deferred 场景。
- `prepare_virtual_view_snapshot_update()` 在 `spawn_blocking` 中执行 layout projection、
  snapshot 组装、bounds、metadata projection 和 slot projection（生产路径下），
  返回 `PreparedVirtualViewResult`。
- `apply_virtual_view_result()` 在 UI 线程合并 prepared result 与当前 selection/thumbnail 状态并执行
  model writes。它不再做同步 projection fallback。
- `sync_pane_view_ui()` 构造 `PaneViewData`（metrics、raster、fallback icons）并 patch
  `pane_surfaces`。zoom 路径中每 visible pane 最多调用一次。
- `sync_pane_slot_ui()` 只 patch `PaneSurfaceData.pane`（chrome），与 view sync 分开。

## Dolphin 源码审计

滚动边界：

- `KItemListView::setScrollOffset()` 做 clamp，未变化立即退出，设置 `m_scrollOffset`，
  mark `m_visibleIndexesDirty`，更新 layouter，更新 smooth scroller target，然后
  synchronously `doLayout(NoAnimation)`。没有 timer，没有 async dispatch。
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:167`
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:149`
- `KItemListViewLayouter::updateVisibleIndexes()` 在非 dirty 时立即返回；用 row offsets
  binary search 计算 first/last visible item。
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:543`
- Horizontal compact projection transposes logical vertical flow，在 item rect 中减去
  `m_scrollOffset`。
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:220`
- `doLayout()` 用 first/last visible index，回收不可见 widget，只创建缺少的可见 widget。
  - `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:1861`
- `KItemListSmoothScroller` 在动画期间内容变化时停止动画并立即更新。
  - `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:142`

缩放边界：

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

## 源码映射表

| 区域 | Dolphin 源码 | Dolphin 行为 | Fika 源码 | Fika 约束 / 计划检查项 |
|------|----------------|------------------|-------------|----------------------------------|
| 滚动输入到偏移 | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:167-185` | `setScrollOffset()` 做 clamp，未变化退出，更新 layouter + animation，然后同步 `doLayout(NoAnimation)` | `ui/split_pane.slint:111-121`、`src/main.rs:181-199`、`src/main.rs:5220-5246` | 保持 logical viewport 同步。仪表化 no-op/clamped scroll、cached scroll、prepare scroll；不把滚动 layout 移到 timer。 |
| 滚动脏状态 | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:149-154` | `setScrollOffset()` 只改 `m_scrollOffset` 并 mark `m_visibleIndexesDirty` | `src/main.rs:4397-4430`、`src/main.rs:4518-4527` | Cached scroll 应只更新 viewport state 或必要的 focused `entry_count`；缓存命中不碰 raster/fallback icon/slot model。 |
| 可见范围计算 | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:543-600` | `updateVisibleIndexes()` 非 dirty 时立即返回，用 row offsets binary search 计算首尾可见项 | `src/main.rs:4206-4213`、`src/main.rs:4420-4422` | 分别测量 `virtual_plan()` 和 cache-cover 检查；如果 p95 高，在动渲染之前先优化可见范围数学。 |
| 横向 compact 投影 | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp:220-234` | 横向方向转置逻辑纵向流，在 item rect 中减去 `m_scrollOffset` | `ui/split_pane.slint:172-182`、`ui/split_pane.slint:428-484` | `paint-viewport-x` 可以动画化视觉偏移，但 Rust viewport 和 item hit-test 坐标必须保持逻辑和当前。 |
| 可见 widget 复用 | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:1861-1922` | `doLayout()` 用首尾可见索引，回收不可见 widget，只创建缺失的可见 widget | `src/app/model_update.rs:304-473`、`src/app/model_update.rs:494-523` | Slot allocator 统计必须报告 reused slots、inactive slots、extended slots 和 changed rows。滚动时仍可见 item 必须保持 slot id。 |
| 平滑滚动动画 | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:81-128` | 动画改变目标 `scrollOffset`；被中断的滚轮调整起止以避免跳过范围 | `ui/split_pane.slint:109-130`、`ui/split_pane.slint:180-182` | 保持 `paint-viewport-x` 仅作为视觉适配。不让 Rust logical viewport 落后于平滑 paint 动画。 |
| 滚动条最大值变化 | `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp:142-150` | 动画期间最大值变化说明内容已变更；停止动画并立即更新 | `ui/split_pane.slint:231-265`、`ui/split_pane.slint:267-269` | `scroll-max-x`、每列行数、宽度、virtual slice 几何和重排必须停止平滑 paint 并提交当前 slice。 |
| 缩放级别输入 | `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp:34-66` | `setZoomLevel()` 做 clamp，未变化退出，更新图标/preview 尺寸，立即调用 `updateGridSize()` | `ui/split_pane.slint:189-198`、`ui/app.slint:1367-1369`、`src/main.rs:301-306` | 保持 zoom layout 立即。仪表化重复同级别 zoom no-op 和事件计数。 |
| Compact 缩放几何 | `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp:176-254` | compact item size 使用 padding/icon/font metrics；style option + item size 一起应用 | `src/app/item_view_metrics.rs:23-50`、`src/app/split_view.rs:432-450`、`src/main.rs:5157-5180` | 单次 zoom event 应每个 visible pane 计算一次 render/layout metrics 并提交一次。 |
| 缩放事务 | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:665-684` | `beginTransaction()` 抑制中间 layout；`endTransaction()` 运行一次最终 `doLayout()` | `src/main.rs:5157-5180`、`src/main.rs:4175-4350`、`src/main.rs:4474-4625` | Phase 2 必须验证一次 zoom event → 每个 visible pane 一次 `sync_virtual_entries_for_slot...` 和一次 `sync_pane_view_ui()`。 |
| 样式/Item 尺寸副作用 | `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:874-912`、`/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp:916-953` | `setItemSize()` 和 `setStyleOption()` 清 size-hint cache、更新可见 widget、mark layouter dirty 并 layout | `src/app/split_view.rs:353-429`、`src/app/split_view.rs:453-505` | Zoom 侧 `PaneViewData` 构造必须分别测量：metrics、raster、fallback icons 和 slot model。 |
| 图标尺寸角色更新 | `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:142-153`、`/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp:887-970` | 图标尺寸变化清 finished previews，在超时内同步更新可见图标，然后启动 preview/role 工作 | `src/main.rs:206-238`、`src/main.rs:5183-5217`、`src/app/file_item_roles_updater.rs:19-21`、`src/app/file_item_roles_updater.rs:37-49`、`src/app/file_item_roles_updater.rs:123-143` | 保持 300ms `IconSizeUpdateScheduler` 仅限于 thumbnail/preview roles。它不得 gate layout。 |
| 搜索焦点 / 过滤弹窗 | `/home/yk/Code/dolphin/src/search/bar.cpp:48-82`、`/home/yk/Code/dolphin/src/search/widgetmenu.cpp:66-70`、`/home/yk/Code/dolphin/src/search/popup.cpp:201-239`、`/home/yk/Code/dolphin/src/search/selectors/filetypeselector.cpp:18-83`、`/home/yk/Code/dolphin/src/search/selectors/dateselector.cpp:22-41` | 搜索/过滤栏把 focus proxy 设到输入框；过滤按钮用 Qt menu popup 在按钮下方弹出；popup 内嵌扁长 selector 控件，其自身 combo/menu 点击打开 | `ui/app.slint:1210-1229`、`ui/search_panel.slint:468-742`、`ui/app.slint:2331-2370` | 搜索内部路由不得调用全局 focus scope；Filter 在按钮下方打开紧凑面板；selector/chip 点击在 selector 下方打开独立下拉，不把整个 popup 切换成列表页。 |
| 搜索输入 Escape/关闭 | `/home/yk/Code/dolphin/src/search/bar.cpp:279-289`、`/home/yk/Code/dolphin/src/filterbar/filterbar.cpp:192-200` | Escape 清非空输入，仅当输入为空时关闭；关闭按钮发 close request | `ui/search_panel.slint:370-434`、`ui/app.slint:1210-1214`、`src/main.rs:3404-3499` | SearchPanel 只拥有本地输入/timer。Rust 拥有 close/clear 状态。程序化清空查询必须使用 `search_query_sync_request` 而非在每个 pane 更新时覆盖正在输入的内容。 |
| 本地搜索/过滤索引 | Dolphin 搜索/过滤状态由 view container/model 提交，而 item view layout 仍操作可见 model/indexes | 避免在 item-view 滚动/缩放路径内做完整 model 过滤 | `src/main.rs:5006-5177`、`src/app/selection.rs:195-286`、`src/app/events.rs:66-75` | `apply_filter_for_slot()` 必须启动 latest-only 后台 visible-index prepare。UI 线程不得在每个 debounced 搜索输入或过滤 chip 变化时扫描所有条目。 |

## 先测量

不要先猜。新增 `FIKA_PERF_ITEM_VIEW=1` 后输出结构化单行日志，并在 1s 窗口或退出时打印
summary。所有日志必须可关闭，默认零成本或接近零成本。

需要记录：

- input：scroll event count、zoom event count、slot、zoom level、viewport-x、window width。
- sync entry：`PaneViewSyncScheduler::request()` 次数、re-entrant skip 次数。
- virtual sync：cached hit / prepare / deferred / stale result 次数，immediate vs async。
- prepare：`prepare_virtual_view_snapshot_update()` 总耗时，拆分 layout/cache/snapshot/metadata。
- apply：`apply_virtual_view_result()` 总耗时，拆分 entry projection、thumbnail decoration、
  metadata projection、bounds generation、`set_pane_virtual_entries()`、`sync_pane_view_ui()`。
- slot pool：active slot 数、patched rows、inactive rows、extended rows、thumbnail image reuse /
  replace 次数、`set_row_data()` 次数。
- raster：cache hit / miss、render time、raster width/height/pixels、revision bump reason。
- fallback icons：cache hit / miss、rendered icon kind count、render time。
- thumbnail flush：flush batch size、触发 `sync_virtual_entries_for_slot(... schedule=false)` 次数，
  与 zoom pending 重叠次数。
- Slint model writes：pane slot/view/surface row writes、item slot row writes、model extend/remove。

输出示例：

```text
[fika perf] zoom slot=0 level=7 panes=1 prepare_ms=3.4 apply_ms=5.8 raster_ms=1.1 slot_rows=84 patched=84 icons=10 model_writes=87
[fika perf] scroll slot=0 cached=true viewport=1840 range=72..168 sync_ms=0.18 model_writes=0
```

## 测试 Workload

1. `10k-flat`：`/tmp/fika-perf-10k`，10000 个普通文件，无 thumbnail。
2. `mixed-icons`：10000 个混合扩展名文件，覆盖 file/image/video/audio/archive/pdf/text/code/executable
   fallback kind。
3. `photos`：500-2000 张图片，thumbnail cache 冷/热各跑一次。
4. `recursive-search`：500+ 带 group/location 的搜索结果，打开 show-location metadata。
5. `split-view`：左右 pane 都可见，分别测试 focused-only scroll 和全局 zoom。
6. `end-boundary`：大目录末尾快速滚动、快速 zoom，确认没有空白和重复重建。

每个 workload 记录 debug build 和 release build。体感判断必须绑定日志：描述卡顿时同时给出
对应的 p95、max 和热点 breakdown。

## Phase 0：仪表化

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

## Phase 1：Dolphin 滚动偏移边界

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

## Phase 2：Dolphin 缩放事务边界

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

## Phase 3：缩放 Raster 和 Fallback Icon 成本

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

## Phase 4：Slot Patch 和投影成本

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

## Phase 5：缩放期间缩略图隔离

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

## Phase 6：验证和收尾

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

## 风险

- 过度合并 zoom 会让 UI 变得迟钝；frame-level latest-only 可以接受，300ms layout 延迟不可接受。
- scroll callback split 不能让 Rust viewport 状态长期滞后，否则 hit-test、selection rectangle、context menu
  会错位。
- raster defer 只能作为连续 zoom 的短暂策略；最终 raster 必须按最新 revision 重建。
- fallback icon LRU 不能无界增长，zoom level 0..16、dark/light 和 media kind 已足够限制 key 空间。
- 如果 Slint `Text` primitive 本身是 zoom p95 主因，文字 raster 只能作为实测后的独立实验，不纳入
  默认路径。

## 非目标

- 不重做 Details/Icons layout。
- 不恢复旧 Slint-facing item/bounds/thumbnail model。
- 不用主观体感替代 baseline；性能结论必须能从日志或 profiler 复现。
