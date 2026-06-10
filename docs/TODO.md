# Fika TODO: GPUI Mainline

本文档是当前任务板。仓库已经切到单包 GPUI 主线；后续任务只应进入
`src/` 下的 core modules、GPUI UI modules、`src/main.rs` 和 `src/bin/`。

状态说明：

- `[x]` 已完成
- `[~]` 正在进行或部分完成
- `[ ]` 未开始
- `[!]` 阻塞项或必须先解决的决策

## Hard Rules

- [x] Dolphin 是第一参考目标。目录加载、刷新、删除、rename、undo 后刷新必须先确认 Dolphin 源码执行流，再实现 Fika 对应层。
- [x] 每个 pane 必须有稳定 `PaneId`。所有 lister、watcher、async result、selection、thumbnail、file operation result 都按 `PaneId + generation` 路由。
- [x] 主构建路径只保留 GPUI/core package。
- [x] GPUI 从 Zed 官方仓库通过 git 依赖获取，不写 concrete crate release、branch 或 revision。
- [x] 新实现不得把 UI widget identity 当作文件模型 identity。GPUI view/entity 是渲染层，文件身份属于 core model。
  - 验收：`Entry` 携带 core `ItemId`，`DirectoryModel` 负责分配、索引和 refresh/rename 身份延续；GPUI item id 使用 `ItemId`，pane selection 存储 `ItemId` 并按当前 model 派生 path。
- [x] 功能提炼与集成：Dolphin 是 UI 行为和文件操作流程的第一参考（目录加载、右键菜单、拖拽、缩略图、trash、搜索、地址栏、状态栏）；cosmic-files 是纯 Rust 系统集成的参考源（MIME 识别、UDisks2 设备发现、systemd 进程启动、Network 远程文件系统）。两个源码库中提炼的功能统一集成到 `fika-core`，UI 层只做渲染和输入路由。新增功能必须先确定参考源码路径并写入对应 `docs/*_REFERENCE.md`，再开始实现。
- [x] Dolphin 分层模型对齐：fika 架构必须对齐 Dolphin 的经典分层——`DolphinMainWindow → DolphinViewContainer → DolphinView → KItemListView`（渲染/容器层）对应 fika 的 GPUI window → pane shell → file grid；`KDirLister → KFileItemModel → KFileItemListWidgetInformant`（数据/模型层）对应 fika 的 `DirectoryLister → DirectoryModel → Entry`；`KFileItemActions / DolphinContextMenu`（交互层）对应 fika 的 Context Menu / DragDrop / Keyboard Action。每层职责边界清晰：渲染层不做数据决策，模型层不持有 UI 句柄，交互层不直接操作文件系统。新增模块必须先映射到 Dolphin 对应层，确认职责归属后再实现。

## Completed Cutover

- [x] 建立 Dolphin 源码参考清单。
  - 验收：`docs/GPUI_DOLPHIN_MIGRATION_PLAN.md` 包含 `DolphinView::loadDirectory()`、`KFileItemModel::{loadDirectory, refreshDirectory}`、KDirLister signal、model slot 和 current-directory-removed 处理路径。
- [x] 移除多包 Cargo 布局。
  - 验收：root `Cargo.toml` 是单一 package，并从 `src/` 构建 core library 和三个 binary。
- [x] 建立 UI-neutral core。
  - 验收：`fika-core` 不依赖 GPUI 或 window 类型。
- [x] 新增 GPUI app shell。
  - 验收：`fika` binary 位于 `src/main.rs`，可打开窗口、加载目录、分屏、关闭 pane、刷新目录。
- [x] 实现初版 `DirectoryLister`、`DirectoryModel` 和 pane-scoped watcher。
  - 验收：加载、刷新、watcher event 和 current-directory-removed 都走 core event path。
- [x] 保留 portal/backend 和 privileged-helper 二进制边界。
  - 验收：两个二进制从 root package 构建。
- [x] 清理旧主路径。
  - 验收：root manifest 不再引用旧 UI 构建路径；旧 UI 源目录和构建脚本已从主代码树移除。
- [x] 更新 README、DESIGN 和 REFERENCE。
  - 验收：文档描述当前 GPUI package 和剩余功能缺口。

## Directory Core

- [x] 完善 `DirectoryLister` event 分类。
  - 验收：watcher add/delete/refresh 能稳定映射到 model delta；不能分类时才整目录 reload。
- [x] 对齐 Dolphin `KDirLister` 加载生命周期。
  - 验收：目录 listing 不再每个 request `spawn` 一个线程；GPUI shell 使用单 worker，pending request 和 pending result 都只保留每个 pane 最新项；同一路径和 mode 的 pending listing 请求会合并为一次 `read_dir`，再按各自 `PaneId + generation + request_serial` 发布；watcher drain 和文件操作 affected-dir refresh 会先收集同一轮 `LoadingStarted` 事件再批量入队，只唤醒 worker 一次，减少分屏同目录 reload 被拆成多次读取的概率；`ListingRefreshed` 的 entries 在 worker result queue 中使用共享 `Arc<Vec<Entry>>`，分屏同目录不会在队列里复制整份大目录 entries；应用到 `DirectoryModel` 时不再 `Arc::try_unwrap` 失败后 clone 整个 entries vec，而是为每个 pane 派生轻量 `ModelEntry`；关闭 pane 会从 worker 中删除该 pane 的 pending request、latest request key 和 pending result，in-flight 读取会在取消检查时停止；`read_dir` 循环按 request stale/shutdown 状态可取消，对齐 `KFileItemModel::cancelDirectoryLoading()`/`KDirLister::stop()` 的释放语义，旧的大目录 `Vec<Entry>` 不进入 UI result queue。
- [~] 完善 `DirectoryModel`。
  - 已完成：目录条目有 stable `ItemId`；`Entry` 不保存完整 path，只保存 name/raw metadata，完整 path 由 `DirectoryModel` 的目录根按需派生；`Entry` 已拆成共享不可变 `EntryData` 载荷和 pane-local `ModelEntry { ItemId, Entry }`，同一 listing 结果应用到多个 pane 时共享底层文件名、trash 文本和 metadata payload；文件名和 trash 文本 payload 使用 `Arc<str>`，同一 listing 结果被多个 pane 应用时不会深拷贝每个文件名字符串；visible item snapshot 复用共享 name，右键菜单 target 和 content hit-test 不再携带未使用的 name 副本；lazy path index 的 key 复用 `Entry.name` 的 `Arc<str>`，按需索引时不再复制一份 `OsString` 文件名；full reload 对同 path 保持身份；watcher rename/refresh 对 old path 延续身份；path index 按 Dolphin `KFileItemModel::index(QUrl)` 的思路按需分块扩展，不在加载时为全目录 eager 构造 HashMap。
  - 剩余验收：支持过滤和更完整的 trash metadata/model 映射。
- [~] 实现目录缓存（Directory Cache）。
  - 参考：Dolphin `KDirectoryListerCache` 跨 view 共享 `KDirLister` 实例的缓存策略；`KFileItemModel` 已加载条目保持和不必要的 reload 避免。
  - 已完成：后台 listing worker 已按路径/mode 合并同一批 pending 请求，合并结果共享 entries 载荷；watcher/operation refresh 路径批量调度同一轮 listing request；`Entry` 文本字段使用共享 `Arc<str>`，减少分屏同目录时 pane-local `ItemId` 重建造成的文件名深拷贝；`DirectoryModel` 现在只为 pane 分配轻量 `ModelEntry` 身份，底层 `EntryData` 在 split pane 和 retargeted listing 之间共享；pane load/reload/back/forward/close 会清理该 pane 的 visible slot pool、compact column width cache、viewport origin、rubber-band/context menu/rename state 和 properties modal，避免旧大目录的 UI 布局缓存继续留存；这先解决分屏同目录时重复 `read_dir`、worker result queue 重复大 `Vec<Entry>`、entries clone 中重复字符串分配和切目录后旧列宽 metrics 滞留的问题。
  - 验收：`fika-core` 新增 `src/core/cache.rs`，实现全局目录缓存层（per-window 或 per-app 作用域）；缓存键为规范化目录路径，缓存值为 `DirectoryModel` 的条目快照 + 加载时间戳 + 缓存状态标记（fresh / stale）；同一目录被多个 pane 打开时共享缓存条目，避免重复 `read_dir` 系统调用；新 pane 首次导航到已缓存目录时立即从缓存渲染（instant display），后台通过 watcher 异步检查 freshness 并按需增量更新；watcher event 到达时将对应路径的缓存标记为 stale，当前显示该目录的 pane 触发增量刷新而非全量 reload；缓存 LRU 淘汰策略：默认最多缓存 64 个目录，超过后淘汰最久未访问的条目；大目录（>10,000 条目）缓存特殊处理，可选仅缓存条目元数据摘要或其 `ItemId` 索引结构而不持有完整条目列表；缓存命中/未命中统计用于性能调优日志；pane 关闭时其 `DirectoryModel` 写回缓存（如果缓存中该路径不存在或版本更旧）。
  - 注意：缓存是性能优化，不改变 `PaneId + generation` 路由语义。缓存命中后 pane 依然分配独立 generation，后续 lister event 仍按 `PaneId + generation` 路由。
- [x] 实现 current-directory-removed。
  - 验收：当前目录删除或 rename 后，pane 跳到最近存在 ancestor，符合 Dolphin 的 `slotCurrentDirectoryRemoved()` 行为。
- [x] 为 directory core 增加覆盖。
  - 验收：包含 stale generation、split pane、同目录双 pane、current-directory-removed、watcher refresh、同路径 listing batch、批量事件 request 提取、共享 entries retarget 和关闭 pane 取消 listing worker 状态测试。

## GPUI Pane and View

- [x] 建立 GPUI pane shell。
  - 验收：pane toolbar action 全部按 `PaneId` 路由；完整 pane 外壳已抽到 `src/ui/pane.rs`，主渲染只按 pane snapshot 数量实例化同一个可复用组件；pane 内不再放操作按钮，split/close 等操作改由 pane-local keyboard action 路由；关闭 pane 会清理可见 item slot pool、layout cache、viewport origin、rubber-band state、context menu/rename state 和后台 listing worker 状态；pane load/reload/back/forward 同样清理该 pane 的 transient UI cache，避免旧目录布局结果跨目录滞留。
- [ ] 实现每个 pane 的地址栏（Location Bar）。
  - 参考：Dolphin `DolphinUrlNavigator` / `KUrlNavigator` 的 breadcrumb 模式和可编辑文本模式。
  - 验收：每个 pane 顶部有独立地址栏，显示当前目录的 breadcrumb 路径（如 `Home > user > Documents > project`）；每个 breadcrumb 段可点击，点击后导航到对应目录；点击 breadcrumb 右侧空白区域或按 Ctrl+L 切换到可编辑文本模式，直接输入路径后回车导航；文本模式提供路径自动补全（基于文件系统）；路径变更后 breadcrumb 同步更新；地址栏按 `PaneId` 隔离，分屏各自独立；导航通过地址栏的操作计入 pane-local navigation history。
- [x] 建立 dynamic split pane。
  - 验收：split open/close 不复制全局 UI state；每个 pane 独立加载目录；1、3、4 个 pane 都走同一个 pane 组件路径。
- [x] 接入 pane-local navigation history。
  - 验收：Back/Forward 通过 `PaneId` 路由，切换 focused pane 不会改变历史事件目标。
- [~] 建立 chooser shell。
  - 验收：支持文件/目录选择、multi-select 输出、filter/choice metadata 输出。
- [x] 实现 pane-local selection controller。
  - 验收：single select、Ctrl/secondary toggle、Shift range、Ctrl/secondary+A、select all、clear selection、方向键移动、Shift+方向键范围选择、chooser multi-select、model change pruning 和 GPUI rubber-band selection 都进入 `fika-core::PaneState`；selection 内部存储 core `ItemId`，rename/refresh 后选择跟随同一 model item；select-all 使用 compact all-selected 状态和 exclusion list，不为大目录分配全量 selected id。
- [x] 实现 Dolphin compact file view。
  - 验收：core compact layout、model-index hit-test、selection rect、rubber-band overlay、GPUI item rendering 使用 `src/core/view.rs` 的布局结果；文件网格已抽到 `src/ui/file_grid.rs`；普通滚轮驱动 pane-local 横向 scroll state；条目按列优先 `index / rows_per_column`、`index % rows_per_column` 布局；每列宽度由该列可显示 item 的最长 name 单独决定，并通过 pane-local `CompactColumnMetrics` cache 复用；snapshot 只投影 `CompactLayout::visible_items()` 返回的可见条目，不再 clone 全目录 entries；visible range 计算按可见列/行生成，不按 model 总数扫描；GPUI tile identity 使用 pane-local reusable slot pool，离屏 slot 回收后给新进入 viewport 的 item 复用，core `ItemId` 仍只作为文件模型身份；GPUI element id 使用 pane id + slot id，避免分屏 key 冲突；recycled slot pool 上限为 100，对齐 Dolphin `KItemListCreatorBase::pushRecycleableWidget()`；目录单击只选择，双击才进入；item 本体不画边框，hover 只显示非选中浅底色，不复用 selected 高亮；item 的 selection/highlight/hitbox 使用自身 `visual_rect`，宽度由 icon + name text 决定；点击空白区域清空 selection；rubber-band 只从空白区域开始，item 上 drag 进入 item drag source；文件/目录图标按目录和扩展名缓存；横向 scrollbar/handle visual 与 pane-local scroll state 同步，drag 更新 pane-local scroll。
- [ ] 实现滚动条平滑插值滚动。
  - 参考：Dolphin `QScroller` / `QPropertyAnimation` 驱动的平滑滚动和 kinetic scrolling（惯性滚动）实现；`KItemListView` 中 scrollbar drag 和滚轮事件的动画过渡。
  - 验收：滚轮滚动时 scroll offset 使用缓出（ease-out）插值而非瞬时跳跃，过渡时长约 150–200ms；scrollbar 拖拽释放后保留惯性动量（kinetic scrolling），动量按摩擦系数逐渐衰减；大目录下平滑滚动不丢帧，插值计算在渲染帧回调中完成；pane 目录切换时取消当前滚动动画并从 offset 0 开始；纵向滚动（列表模式/详情模式预留）同样走插值路径；滚动动画使用 `f32` 亚像素精度，渲染时 round 到物理像素。
- [ ] 实现 pane-local zoom（缩放）。
  - 参考：Dolphin `DolphinView::zoomIn()` / `zoomOut()` / `zoomReset()` 和 `KItemListView::setZoomLevel(int)`，zoom level 影响图标大小和 compact view 网格布局。
  - 验收：每个 pane 有独立 zoom level，按 `PaneId` 隔离；Ctrl+Plus 放大 / Ctrl+Minus 缩小 / Ctrl+0 重置，zoom 快捷键按 focused `PaneId` 路由；zoom level 直接影响 compact view 的 icon size、列宽（column width）和行高（row height），`CompactColumnMetrics` 在 zoom 变更时失效重建；icon size 范围对齐 Dolphin（约 16px–256px，默认随系统字体 scale）；zoom level 持久化到 pane state，新建 pane 继承当前 focus pane 的 zoom level；状态栏或 toolbar 显示 zoom slider（可选 UI，首版可仅快捷键）；zoom 变更不触发热重载目录，仅更新 rendering layout。
- [ ] 实现状态栏（Status Bar）。
  - 参考：Dolphin `DolphinStatusBar` 选中条目信息、可用空间、zoom slider 和进度指示。
  - 验收：窗口底部有全局状态栏（非 per-pane）；左侧显示当前 focused pane 的选中条目数量和总大小（如 "3 items (14.2 MiB)"）；右侧显示当前 focused pane 所在分区的可用空间（如 "Free space: 23.4 GiB"）；中间嵌入 zoom slider（水平滑块），拖动时实时更新 focused pane 的 zoom level；大文件复制/移动操作进行时显示进度条和取消按钮；状态栏信息随 focus pane 切换实时更新；状态栏高度紧凑（单行 24–28px）。
- [~] 实现 keyboard shortcuts。
  - 已完成：方向键、Shift+方向键、Ctrl/secondary+A、Ctrl/secondary+C/X/V、Ctrl/secondary+Shift+N、F2 rename、Escape、F5、F3、Backspace、Alt+Left、Alt+Right、Delete、Ctrl/secondary+W 和 Ctrl/secondary+Z 都按 focused `PaneId` 路由到 pane-local action。
  - 剩余验收：后续新增交互继续按 pane-local action 路由。
- [ ] 实现每个 pane 自己的搜索框。
  - 参考：Dolphin 搜索栏（`DolphinSearchBox` / `KUrlNavigator` 中的 filter/search 切换）。
  - 验收：每个 pane 有独立搜索框（inline filter bar），输入实时过滤当前目录条目；支持名称过滤和基本通配符；搜索框清空后恢复完整目录视图；搜索状态按 `PaneId` 隔离，分屏互不影响；激活搜索不影响 selection 和 navigation history。
- [ ] 实现 Wayland 下的粘贴/复制操作协议。
  - 参考：Wayland `wl_data_device_manager` / `wl-clipboard` / `smithay-clipboard` 生态。
  - 验收：Ctrl+C 将选中文件路径写入 Wayland 剪贴板（`text/uri-list` 和 `text/plain`）；Ctrl+V 从剪贴板读取文件路径或文本，触发 paste file operation；支持 primary selection（中键粘贴）和 clipboard selection 两种 Wayland data device；拖拽过程中的 data offer 也走同一协议栈。

## Context Menu（右键菜单）

> **全局参考**：Dolphin 右键菜单实现路径 `../dolphin/src/dolphincontextmenu.cpp`、
> `DolphinContextMenu`、`KFileItemActions`、`DolphinMainWindow` 中的 context menu event 处理。
> 子菜单定位和延迟消失参考 `QMenu::popup()`、`QMenu::setHideDelay()` 和 Dolphin 的
> `DolphinContextMenu::open()` / `DolphinContextMenu::showEvent()`。

- [~] 建立 Dolphin 右键菜单源码执行流参考清单。
  - 已完成：新增 `docs/CONTEXT_MENU_REFERENCE.md`，记录 `DolphinContextMenu::{addAllActions, addViewportContextMenu, addItemContextMenu, createPasteAction}` 和 `KItemListController` 右键/空白区事件边界。
  - 剩余验收：补齐子菜单级联定位、hide delay 和 Places/Trash 专用 context menu 的完整执行流。
- [~] 实现基础右键菜单（空白区域）。
  - 已完成：pane 空白区域右键弹出 GPUI overlay menu；包含 New Folder、Paste、Select All、Refresh、Properties；Paste 按内部 clipboard 状态启用/禁用；点击外部或 Esc 关闭；空白右键不启动 rubber-band；Properties 只读取当前目录自身 metadata，不递归扫描。
  - 参考：Dolphin 在空白目录区域右键弹出 `DolphinContextMenu`（包含 Paste、Sort By、View Mode、Properties 等）。
  - 剩余验收：补 Sort By、View 子菜单、打开终端和完整 Dolphin-like action grouping。
- [~] 实现文件/目录右键菜单。
  - 已完成：item core 区域右键弹出菜单；未选中 item 先按 `PaneId` 选中；菜单包含 Open/Open With、Rename、Copy、Copy Location、Cut、Move to Trash、Properties；Copy Location 使用 GPUI clipboard 写入真实系统剪贴板；目录 item 增加 Open in New Pane；单目录右键 Paste 目标为该目录，和 Dolphin `createPasteAction()` 一致；右键 item 停止 rubber-band。
  - 参考：Dolphin 选中单文件时右键菜单包含 Open With、Cut/Copy、Rename、Move to Trash、Properties 等；选中目录时额外包含 Open in New Tab/Window。
  - 剩余验收：补 Open With 子菜单、Open in New Window、multi-select 差异菜单和按文件类型动态 action state。
- [~] 实现多选右键菜单。
  - 已完成：右键目标属于多选时，菜单生成批量 Copy、Cut、Move to Trash、Properties，不再显示单文件/单目录专属 Open、Open With、Rename 或 Open in New Pane；Properties 汇总数量、类型计数和非目录文件大小，不递归扫描目录。
  - 参考：Dolphin 多选时右键菜单不包含单文件专属项（如 Open With），只显示批量操作。
  - 剩余验收：补 Compress 和“全是目录”的批量专属操作。
- [ ] 实现子菜单定位。
  - 参考：Dolphin 使用 `QMenu::popup()` 时传入 `QPoint` 指定弹出位置，子菜单（Open With、Sort By 等）由 Qt 自动处理级联定位；Dolphin 不对子菜单做自定义偏移。
  - 验收：子菜单（Open With、Sort By、Create New 等）在父菜单项右侧弹出，不超出窗口边界；窗口靠右边缘时子菜单自动翻转到左侧；多级子菜单级联展开位置正确。
- [ ] 实现子菜单延迟消失（hide delay）。
  - 参考：Dolphin 使用 `QMenu` 默认 hide delay（约 300ms），鼠标短暂离开菜单区域不会关闭；子菜单之间移动时有 grace period。
  - 验收：鼠标在父菜单和子菜单之间移动时有 ~300ms 过渡窗口，菜单不立即关闭；鼠标直接从父菜单项滑入子菜单不会触发菜单消失；鼠标完全离开整个菜单树（父+子）后延迟关闭。
- [ ] 实现 Places 侧栏右键菜单。
  - 参考：Dolphin Places 面板右键菜单（`DolphinPlacesModel` 的 context menu），包含 Add Entry、Edit、Remove、Hide Section 等。
  - 验收：侧栏空白区域右键可添加新书签；侧栏已有条目右键可编辑/移除/重命名；侧栏 section（如 Removable Devices）有独立的上下文操作。
- [ ] 实现 Trash 视图右键菜单。
  - 参考：Dolphin trash 目录右键菜单包含 Empty Trash、Restore、Delete Permanently。
  - 验收：在 trash 视图中右键文件增加 Restore 选项；右键空白区域增加 Empty Trash 选项；无 Restore 目标时 Restore 置灰。

## Drag and Drop（拖拽）

> **全局参考**：Dolphin 拖拽实现路径 `../dolphin/src/dolphinview.cpp` 中的
> drag 和 drop event handler（`startDrag()`、`dropEvent()`、`dragEnterEvent()`、
> `dragMoveEvent()`、`dragLeaveEvent()`）；`KItemListView` 中的拖拽 widget 创建；
> Places 面板拖拽（`../dolphin/src/places/` 下的 model/view drag support）。

- [ ] 建立 Dolphin 拖拽源码执行流参考清单。
  - 验收：`docs/` 下新建 `DRAG_DROP_REFERENCE.md`，记录 Dolphin view drag start/move/enter/leave/drop 完整执行路径、`QDrag` 对象构造、mime data 填充、drop action 判断和 Places panel drop 处理。
- [~] 实现 pane item 拖拽源（drag source）。
  - 已完成：item `visual_rect` 拥有独立 GPUI drag source 和基础拖拽预览，item 上拖拽不再触发空白 rubber-band。
  - 参考：Dolphin `DolphinView::startDrag()` 创建 `QDrag`，设置 pixmap、mime data（`text/uri-list`），支持 `MoveAction` / `CopyAction` / `LinkAction`。
  - 剩余验收：补 selected item count preview、`text/uri-list` MIME data、内部/外部 copy/move/link action 和 Ctrl 切换。
- [ ] 实现 pane item 拖拽目标（drop target）。
  - 参考：Dolphin `DolphinView::dropEvent()` 判断 drop action，对目录执行 move/copy/link 操作。
  - 验收：拖拽文件到目录上时目录高亮显示（与普通 hover 不同的高亮颜色）；拖拽到空白区域时执行 copy/move 到当前目录；拖拽到 pane toolbar（路径栏）时导航到对应目录后 drop；区分内部拖拽 move 和外部拖拽 copy。
- [ ] 实现 Places 侧栏 item 拖拽源。
  - 参考：Dolphin Places panel 允许拖出 bookmark 到外部。
  - 验收：侧栏中的 places 条目可拖拽到 pane 中打开；侧栏条目拖拽到外部应用时传递文件路径（如果对应目录存在）。
- [ ] 实现 Places 侧栏 drop target。
  - 参考：Dolphin Places panel 接受文件/目录拖入创建新 bookmark，根据 drop 位置区分：拖到已有条目上 → 复制到该目录；拖到条目之间 → 插入新 bookmark。
  - 验收：从 pane 拖拽目录到侧栏条目之间时插入新 places bookmark；从 pane 拖拽目录到侧栏已有条目上时执行 copy/move 到该目标目录；侧栏根据 drop 位置显示不同高亮：插入位置显示插入线（insertion indicator），目标目录显示背景高亮；两种高亮使用不同颜色区分。
- [ ] 实现 pane 到侧栏、侧栏到 pane 的互相拖拽。
  - 验收：从 pane 拖到侧栏 → 行为如上（插入 bookmark 或 copy 到目标目录）；从侧栏拖到 pane → 导航到该目录。
- [ ] 实现拖拽过程中的视觉反馈。
  - 参考：Dolphin 拖拽悬停在目录上时该目录条目显示高亮背景；拖拽悬停在侧栏条目之间时显示插入指示线。
  - 验收：pane 中目录 drop target 高亮颜色与 selected 高亮颜色明确区分（如蓝色 selected vs 绿色 drop target）；侧栏插入线为 2px 粗线，颜色与系统强调色一致；拖拽离开区域后高亮立即清除；拖拽过程中光标样式随 drop action 变化（Move → 箭头+小方块，Copy → 箭头+加号，Link → 箭头+链接图标）。

## File Operations and Undo

- [~] 迁移 file operation primitives 到 core。
  - 已完成：create file/folder、rename、move-to-trash 和内部 Copy/Cut/Paste 都通过 GPUI 后台任务调用 core file operation primitives，并返回 affected dirs / undo payload。
  - 验收：copy/move/link/trash/rename/create/delete 结果只返回 affected dirs / pane ids / undo registration，不直接触碰 UI。
- [~] 迁移 undo serial。
  - 已完成：create file/folder、rename、move-to-trash 和内部 Copy/Cut/Paste 会记录 core undo payload 和受影响目录；Undo 取最新 serial，恢复后通过 affected panes 的 lister refresh。
  - 验收：undo start/finish 以 serial 防 stale result；undo 完成后通过 affected panes 的 lister refresh。
- [ ] 实现完整的 Trash 功能和视图。
  - 参考：Dolphin trash 实现 `../dolphin/src/trash/`、`TrashBase`、`DolphinTrash`；XDG trash spec（`freedesktop.org/wiki/Specifications/trash-spec/`）；trash 目录结构 `$XDG_DATA_HOME/Trash/files/` 和 `$XDG_DATA_HOME/Trash/info/`。
  - 验收：
    - Trash 目录作为特殊虚拟目录加载，`DirectoryModel` 可展示 `files/` 下所有被删除文件及其原始路径（从 `info/` 中 `.trashinfo` 文件读取）。
    - `Entry` 携带 trash metadata：原始路径（`orig_path`）、删除时间（`deletion_date`），在 trash 视图中作为额外列或 tooltip 显示。
    - trash 视图右键菜单包含 Restore（恢复文件到原始路径）和 Delete Permanently（清空回收站/永久删除）。
    - 普通目录中 Delete 键执行 move-to-trash（通过 `FileOps::trash_file()`），undo 后恢复。
    - Empty Trash 操作清空 `files/` 和 `info/`，完成后触发 pane 刷新。
    - trash `files/` 和 `info/` 的外部变化（watcher event）映射到同一个 model item 的 trash metadata 更新。
    - Places 侧栏 Trash 条目显示非空状态图标（有/无内容两种状态），右键包含 Empty Trash。
  - 详细验收：
    - trash 创建：`trash_file()` 生成唯一 trash 文件名（`path_basename.trashinfo` 对应），写入 `.trashinfo`（包含 `[Trash Info]`、`Path=`、`DeletionDate=`），移动原文件到 `files/`。
    - trash 恢复：`restore_file()` 读取 `.trashinfo` 获取原始路径，将文件移回，若原始路径已存在则弹出覆盖确认对话框，成功后清理 trash 中残留的 `files/` 和 `info/` 条目。
    - trash 永久删除：`delete_permanently()` 直接删除 `files/` 中文件和对应 `info/` 条目，不可撤销。
    - trash 视图排序：支持按 Name、Original Path、Deletion Date 排序，model 层提供对应的 `SortRole`。
    - trash 状态变更通过 lister event 路径通知所有相关 pane，走 `PaneId + generation` 路由。

## Desktop Integration

- [ ] D-Bus 总线控制（Bus Control）。
  - 参考：cosmic-files 中 `zbus` Connection 管理（session bus + system bus 统一生命周期）；Dolphin 的 `KDirNotify` / `org.freedesktop.FileManager1` D-Bus 接口；`fika-privileged-helper` 现有的 system bus 连接模式。
  - 验收：`fika-core` 新增 `src/core/bus.rs`，统一管理系统总线（system bus）和会话总线（session bus）的 `zbus::Connection`；连接按需延迟建立，空闲超时后自动断开（默认 30s）；D-Bus 方法调用支持超时重试（默认 3 次，间隔递进）；UDisks2 设备信号（`InterfacesAdded`/`InterfacesRemoved`/`PropertiesChanged`）通过统一 bus 层订阅和分发到 `devices.rs`；systemd `StartTransientUnit` 调用通过统一 bus 层路由到 `launcher.rs`；Portal `org.freedesktop.impl.portal.FileChooser` 接口通过 session bus 注册，走统一 bus 层管理；`privileged-helper` 的 system bus 服务注册和 Polkit 授权检查复用同一 bus 层；D-Bus 错误统一转换为 `fika-core` 结构化错误类型（`BusError`），包含服务名、方法名和错误详情。
- [ ] MIME 类型自我识别。
  - 参考：cosmic-files `src/mime.rs` / `src/mime_types.rs` 的 MIME 识别实现（不依赖 KIO / GLib GVfs，纯 Rust 实现）；`xdg-mime` / `shared-mime-info` 数据库；`mime_guess` / `tree_magic` / `infer` crate 生态。
  - 验收：`fika-core` 新增 `src/core/mime.rs`，通过文件扩展名和 magic bytes（文件头）双重识别 MIME 类型；支持从系统 `shared-mime-info` 数据库（`/usr/share/mime/`）加载 MIME 映射；`Entry` 增加 `mime_type: Option<String>` 字段；目录加载完成后按需批量识别（不阻塞首屏渲染），结果通过 model refresh event 回填。
- [ ] 通过 systemd 创建子进程（Open With / 应用启动）。
  - 参考：cosmic-files 中 `process.rs` / `exec.rs` 使用 systemd `busctl` / `org.freedesktop.systemd1` 启动应用进程；`zbus` crate 的 systemd Manager interface。
  - 验收：`fika-core` 新增 `src/core/launcher.rs`，通过 D-Bus 调用 `org.freedesktop.systemd1.Manager.StartTransientUnit()` 启动应用进程；`Open With` action 接受 desktop file path / MIME type，查找关联的 `.desktop` 文件，通过 launcher 启动；进程生命周期由 systemd user instance 管理，fika 不持有子进程句柄；启动失败时返回结构化错误（找不到应用、权限不足等）。
- [ ] Open With / Service Menu 完整实现。
  - 参考：Dolphin 的 `KFileItemActions` 和 service menu（`.desktop` 文件的 `Actions=` key）；`xdg-mime` 的 `mimeapps.list` 关联。
  - 验收：右键菜单 Open With 子菜单动态列出可打开该 MIME 类型的应用（按 `mimeapps.list` 优先级排序）；顶部显示默认应用，底部显示 "Other Application..." 选项弹出应用选择列表；Service Menu 根据 desktop file `Actions=` 和 `X-KDE-ServiceTypes=` 动态生成额外操作项（如 "Send To"、"Compress"）；service menu action 执行通过 systemd launcher 启动对应进程。
- [ ] Devices 设备识别（U 盘等）。
  - 参考：cosmic-files `src/mount.rs` / `src/device.rs` 的设备发现和挂载实现（UDisks2 D-Bus API、`/proc/mounts` / `mountinfo` 解析）；Dolphin 的 `DeviceNotifier` 和 `KFilePlacesModel` 设备集成。
  - 验收：`fika-core` 新增 `src/core/devices.rs`，监听 UDisks2 D-Bus 信号（`InterfacesAdded`/`InterfacesRemoved`/`PropertiesChanged`）发现块设备；解析 `/proc/self/mountinfo` 获取挂载点映射；`DeviceInfo` 结构包含 device path、mount point、filesystem type、label、容量、removable flag；设备插入/拔出事件通过 core event channel 通知 UI；Places 侧栏动态显示/移除 Removable Devices section。
- [ ] 设备挂载/卸载/eject 操作。
  - 参考：cosmic-files 中通过 UDisks2 `Filesystem.Mount()` / `Filesystem.Unmount()` / `Drive.Eject()` 执行挂载操作；需要 Polkit 授权的操作走 privileged helper。
  - 验收：Places 侧栏设备条目点击时自动挂载（如未挂载）并导航到挂载点；右键菜单包含 Unmount / Eject / Safely Remove；卸载操作完成后 Places 条目仍然显示但状态变为 unmounted；挂载/卸载失败时显示错误通知；需要特权的操作（如格式化、eject）通过 `fika-privileged-helper` D-Bus 服务执行。
- [ ] Network 网络文件系统支持。
  - 参考：cosmic-files `src/network.rs` / `src/remote.rs` 的远程文件系统挂载实现（SMB/CIFS、FTP、SFTP、WebDAV 等协议）；Dolphin 的 `KIO` 远程 URL 支持（`smb://`、`sftp://`、`ftp://`、`fish://`、`nfs://` 等）。
  - 验收：`fika-core` 新增 `src/core/network.rs`，支持解析远程 URL scheme（`smb://`、`sftp://`、`ftp://`、`nfs://`）并建立连接；通过系统挂载工具（如 `mount.cifs`、`sshfs`、`curlftpfs`）或 GVfs FUSE 挂载点访问远程文件系统；远程目录的 `DirectoryLister` 复用本地加载路径，但 lister 内部针对网络延迟做节流（减少 watcher 频率、批量加载）；连接失败时显示结构化错误（认证失败、超时、主机不可达）；Places 侧栏增加 Network 入口，展开后显示可用网络位置和已保存的远程书签；远程连接支持用户名/密码和 SSH key 认证；远程文件操作（复制/移动/删除）复用 core `FileOps` 路径，由底层挂载点透明处理。
- [ ] Thumbnail 缩略图管线完整实现。
  - 参考：Dolphin 缩略图实现 `../dolphin/src/kitemviews/` 中的 `KFileItemModelRolesUpdater` 和 thumbnail role；freedesktop thumbnail spec（`specifications.freedesktop.org/thumbnail-spec/`）；Dolphin 的 thumbnail cache、failure cache、visible-first scheduling 逻辑。
  - 验收：
    - `fika-core` 新增 `src/core/thumbnails.rs`，实现 freedesktop thumbnail cache 读/写（`~/.cache/thumbnails/normal/` 和 `large/`）；`Entry` 增加 `thumbnail_path: Option<PathBuf>` 字段。
    - thumbnail cache 优先命中：先查 `normal/`（128x128），再查 `large/`（256x256），命中直接在 UI 显示。
    - failure cache：对无法生成缩略图的文件（如损坏的图片）记录到 `~/.cache/thumbnails/fail/gnome-thumbnail-factory/` 同名 PNG（按 freedesktop spec），避免重复尝试。
    - visible-first scheduling：缩略图生成请求按可视区域优先级排序，viewport 内 item 优先，viewport 外延迟；目录跳转时取消所有 pending thumbnail 请求。
    - thumbnail 生成走外部 thumbnailer 进程（如 `tumbler`、`ffmpegthumbnailer`）或内置图片解码，不阻塞 UI。
    - 大目录下 thumbnail 生成限制并发数（默认 4），通过 semaphore 控制。
    - 优化：缩略图懒加载，item 滚入 viewport 后才请求生成；离开 viewport 且未完成的请求取消。
    - `DirectoryModel` 不直接持有缩略图像素数据，只存储 thumbnail path；GPUI 渲染层按需加载图片。
- [~] Places 侧栏完善。
  - 已完成：GPUI shell 增加 Dolphin-like Places sidebar，入口包括 Home、XDG user dirs、Trash 和 Root；active place 按当前 pane 路径派生；点击 place 通过 focused pane 加载目标目录；侧栏容器和条目改为圆角样式。
  - 验收：后续继续对齐 Dolphin `PlacesPanel` / `KFilePlacesModel`，补齐 bookmarks、devices、trash state 和异步设备操作。
- [~] Portal chooser。
  - 验收：portal backend 调用 GPUI chooser shell，并共享 core selection/output 常量。

## Documentation and Checks

- [x] README 只描述当前 GPUI package。
- [x] DESIGN 只描述当前 GPUI/core 架构。
- [x] REFERENCE 路径指向 `src/...`。
- [ ] 为新增模块新建 Dolphin/cosmic-files 源码参考清单文档：
  - `docs/CONTEXT_MENU_REFERENCE.md` - Dolphin 右键菜单完整执行流
  - `docs/DRAG_DROP_REFERENCE.md` - Dolphin 拖拽完整执行流
  - `docs/THUMBNAIL_REFERENCE.md` - Dolphin 缩略图管线和 freedesktop spec
  - `docs/MIME_LAUNCHER_REFERENCE.md` - cosmic-files MIME 识别和 systemd 进程启动
  - `docs/DEVICES_REFERENCE.md` - cosmic-files UDisks2 设备发现和挂载
  - `docs/TRASH_REFERENCE.md` - Dolphin trash 实现和 XDG trash spec
  - `docs/SEARCH_REFERENCE.md` - Dolphin 搜索框实现
  - `docs/LOCATION_BAR_REFERENCE.md` - Dolphin 地址栏（`KUrlNavigator`）breadcrumb 和文本模式
  - `docs/STATUS_BAR_REFERENCE.md` - Dolphin 状态栏（`DolphinStatusBar`）信息显示和 zoom slider
  - `docs/NETWORK_REFERENCE.md` - cosmic-files/Dolphin 远程文件系统挂载和协议支持
  - `docs/SMOOTH_SCROLL_REFERENCE.md` - Dolphin 平滑滚动（`QScroller`）和 kinetic scrolling 实现
  - `docs/BUS_CONTROL_REFERENCE.md` - D-Bus 总线控制：zbus 连接管理、UDisks2/systemd/Portal 路由
- [~] 为 core 和 GPUI shell 补齐任务级测试。
  - 已完成：core `ItemId` 稳定身份、rename/refresh 后 selection 跟随、cancellable directory listing、per-pane coalesced listing worker、compact select-all/exclusion、column-first compact layout、visible item virtualization、large-directory visible range bound、pane-local reusable visible item slot pool、recycled slot cap、horizontal scrollbar layout、pane-local scroll clamp、compact item `visual_rect` 按 required text width 收窄、右键菜单 action 生成覆盖 Paste enabled state、目录 Open in New Pane、单目录 Paste、Properties 和多选批量菜单。
  - 剩余：trash 操作和视图测试、缩略图管线测试、设备发现/挂载测试、MIME 识别测试、拖拽测试、右键菜单 action 测试、搜索过滤测试、Wayland clipboard 测试。
- [ ] 持续性能优化。
  - 参考：现有性能问题见 `docs/OPTIMIZATION.md`（存档）和 `docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md`（存档）；Dolphin 的性能优化策略（lazy icon loading、`KItemListCreatorBase` slot reuse、大目录分批渲染）；cosmic-files 的异步加载和缓存策略。
  - 验收：
    - 启动性能：冷启动到首帧渲染 < 500ms，热启动（缓存命中）< 200ms；使用 `tracing` / `tracy` 做启动阶段计时。
    - 大目录性能：100,000 条目目录从 `read_dir` 完成到首帧渲染 < 100ms（利用可见条目虚拟化和 compact layout）；滚动帧率保持 60fps（利用 slot pool 复用和 lazy icon loading）。
    - 内存占用：空闲状态（单 pane、空目录）< 50 MiB RSS；100,000 条目目录 < 200 MiB RSS（利用条目元数据紧凑存储 + `CompactLayout::visible_items()` 投影）。
    - 缩略图性能：缩略图生成不阻塞 UI 线程；大目录下 thumbnail 并发数由 semaphore 限制（默认 4），剩余排队按 viewport 优先级调度。
    - I/O 优化：`read_dir` 批量获取后按需惰性 `stat`（先不获取 size/mime，滚动到 viewport 时按需补全）；watcher 事件合并去抖（debounce 100ms 内的同类事件）。
    - 跨 pane 缓存共享：目录缓存命中避免重复 `read_dir`（见 Directory Cache 条目）。
    - 性能回归检测：CI 中加入 benchmark gate（`cargo bench` 对比基线），大目录加载和滚动帧率不低于前次 release。
    - 性能剖析：定期使用 `perf` / `flamegraph` 对关键路径（目录加载、渲染、滚动、文件操作）做热点分析，结果记录到 `docs/perf/`。
- [x] 持续运行：
  - `cargo fmt --all`
  - `cargo test`
  - `cargo check`
  - `cargo build --release`
  - `timeout 4s target/release/fika`
