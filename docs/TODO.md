# Fika TODO: SCTK/wgpu Shell Mainline

本文档是当前任务板。当前可运行应用仍是 GPUI 基线，但长期 UI 主线已经转向
Linux-only、Fika 专用的 `smithay-client-toolkit + calloop + wgpu` shell。新 UI runtime 工作以
`docs/WGPU_SHELL_ROADMAP.md` 为准；GPUI 代码保留为兼容实现、行为基线和必要
fallback。

状态说明：

- `[x]` 已完成
- `[~]` 正在进行或部分完成
- `[ ]` 未开始
- `[!]` 阻塞项或必须先解决的决策

## Hard Rules

- [!] **P0：新增 Fika 专用 SCTK/wgpu shell，并把 GPUI 降级为基线/fallback。** 新 UI 工作必须优先进入独立 `fika-sctk` spike：复用 `fika-core`，使用 `smithay-client-toolkit`/`calloop`/`wayland-client` 直接拥有 Wayland window、event loop、seat、clipboard、popup 和 DnD 边界，使用官方 crates.io `wgpu` 渲染。既不采用 libcosmic/iced widget tree，也不把 winit 作为长期窗口层；现有 `fika-wgpu` 仅作为 renderer/scene 迁移输入。文件视图和 Places 的热路径必须直接拥有 retained geometry、hit-test、paint command、texture/glyph cache 和 DnD routing。GPUI retained renderer 文档只作为历史证据和迁移输入。
- [x] Dolphin 是第一参考目标。目录加载、刷新、删除、rename、undo 后刷新必须先确认 Dolphin 源码执行流，再实现 Fika 对应层。
- [x] 每个 pane 必须有稳定 `PaneId`。所有 lister、watcher、async result、selection、thumbnail、file operation result 都按 `PaneId + generation` 路由。
- [~] 当前主构建路径仍保留 GPUI/core package；新增 shell 应先作为独立实验二进制并与 GPUI 并存。
- [~] GPUI 从 Zed 仓库通过 git 依赖获取，仅用于当前二进制和 fallback；新 shell 依赖策略以 `docs/WGPU_SHELL_ROADMAP.md` 为准，窗口/event 后端以 SCTK/calloop 为准。
- [x] 直接 crates.io 依赖不使用 `*`。版本声明保持最新稳定大版本范围，不锁到 patch/minor。
- [x] 新实现不得把 UI widget identity 当作文件模型 identity。文件身份属于 core model；新 shell 的 slot、hitbox、atlas 和 draw resources 只能消费 core/retained identity。
- [x] 功能提炼与集成：Dolphin 是 UI 行为和文件操作流程的第一参考；cosmic-files 是纯 Rust 系统集成的参考源。两个源码库中提炼的功能统一集成到 `fika-core`，UI 层只做渲染和输入路由。
- [x] Dolphin 分层模型对齐：渲染层不做数据决策，模型层不持有 UI 句柄，交互层不直接操作文件系统。
- [x] 文件拆分：`src/main.rs` 只保留 app 状态编排和跨模块路由。所有功能模块已拆入 `src/core/`（domain logic）和 `src/ui/`（rendering），子职责继续按目录式模块拆分。
- [x] 图标模型已收敛为 Dolphin-style retained image 模型：删除 `ModelEntry.icon_name`、`src/ui/icons/roles.rs`、`RenderImage` 自解码路径；图标由 `FileIconCache` 按 exact `FileIconKind + icon_size` / named icon 缓存，同时同一 `FileIconKind` 已有 resolved path 时跨 zoom 尺寸复用稳定语义。文件视图缩略图和 MIME/theme icon 默认都由 custom image paint layer 通过 `Window::paint_image` 绘制；普通 MIME/theme icon 不再有 GPUI `img()` 或 hybrid readiness handoff 运行时分支。Theme pixmap cache 按 `ThemeIconImageKey`（icon name、size、scale、theme/color/mode 哨兵）键控，像 Dolphin `QPixmapCache` 一样按尺寸区分。Zoom 对齐 Dolphin 普通图标路径：布局立即变化，MIME/theme icon semantic identity 不套用 300ms preview role-size timer，也不因每个 zoom size 提交第二次 path identity。

## Completed Features

以下功能已实现并通过验证。保留摘要记录以备查考，不再维护逐项验收清单。

### 目录加载与模型
`DirectoryLister` → `DirectoryModel` 完整 Dolphin 对齐：`read_dir` 流式发布、2000ms maximum update interval、per-pane request coalescing、fresh/stale cache LRU、`Arc<Vec<Entry>>` 跨 pane 共享。`ListingWorkerState` 按 `(path, mode)` 合并请求，`DirectoryCache` 按 canonical path 缓存。Large directory 保留轻量 path/count summary。

### 右键菜单
完整 Dolphin 对齐：Open/Open in New Pane/Open in New Window、Cut/Copy/Paste、Rename、Move to Trash、Delete Permanently、Properties、Create New 子菜单、Open With（`mimeapps.list` 优先级 + Other Application chooser）、KDE/Fika Service Menu（含 `X-KDE-Submenu` 二级子菜单、TopLevel 提升、条件过滤）、Sort By（含 Trash 专用 Original Path/Deletion Time）、Compress/Extract Ark 集成。Trash 专用菜单：Restore、Delete Permanently、Empty Trash。Places 右键菜单：Open/Edit/Remove/Hide/Empty Trash/Copy Location/Properties，按条目类型启用。菜单定位使用 viewport clamp/flip。

### 拖拽 (Drag & Drop)
内部 item/place drag 完整：pane↔pane、pane↔Places 互相拖拽，GPUI `ExternalPaths` 外部 drop，Copy/Move/Link drop menu，目录 drop target 琥珀色高亮，Places 插入线 bookmark insert/reorder，精确 leave 清理，3s lease timeout 兜底。`DragExportPayload`（`text/uri-list` + `text/plain`）已构造，Places drag preview 含 cursor offset 补偿。

### 缩略图
Freedesktop thumbnail spec 完整实现：MD5 URI cache key、`normal/` + `large/` cache path、failure marker（`fail/gnome-thumbnail-factory`）、`Thumb::URI` / `Thumb::MTime` 校验、thumbnailer `.desktop` 注册表 + fallback 命令列表。Dolphin `indexesToResolve()` visible-first scheduling + read-ahead，`ThumbnailScheduler` 管理 pane/generation/item 工作 key。成功写入 `thumbnail_path`，失败写入 `thumbnail_failed`。

### MIME & 应用启动
`shared-mime-info` glob/literal/suffix/magic 检测。`.desktop` 解析（Desktop Entry/Action/MimeType/Exec field codes）、`mimeinfo.cache`、`mimeapps.list` Default/Added/Removed Associations、`type/*` wildcard 匹配。systemd user transient unit 启动。KDE service menu 专用目录扫描、条件过滤。

### 文件操作 & Undo & Trash
Copy/Move/Link/Trash/Create/Rename/Delete primitives → core file ops → affected dirs → pane refresh。Undo serial 防 stale。XDG Trash：`.trashinfo` 读写、Restore（含 Replace Existing 冲突对话框）、Delete Permanently、Empty Trash。`TrashEmptinessMonitor` app-owned 状态。Trash model 按 Deletion Time 排序。

### Places 侧栏
Home/XDG dirs/Trash/Devices/Root/Network sections。User bookmark XBEL 持久化（`fika/places.xbel`）。GIO/GVfs 动态 Removable Devices section，mount/unmount/eject。右键菜单、拖拽重排、Add/Edit draft。Hidden place/section 过滤。

### 状态栏 & 工具栏 & 地址栏
Pane-local selection summary、free-space info、zoom slider、operation progress with Stop。Dolphin breadcrumb + editable text mode、caret navigation、Tab 补全。Pane toolbar：Search/Close filter、Split/Close Pane 按钮。

### Inline Rename
Pane-local draft state、UTF-8 caret、selection range、Shift+←→ 扩展选区、Ctrl+A 全选。扩展名修改琥珀色警示。Tab 连续 rename。Watcher rename/refresh 重定向 draft。空名/重名 inline 错误提示。

### 异步运行时代码
Tokio 多线程 + Compio 专用操作线程。Bounded `mpsc::channel(1)` 提交，`compio::runtime::spawn(...).detach()`。Compio 文件 I/O + `spawn_blocking` 同步 fallback。`OperationRuntime::shared()`。

### D-Bus 总线控制
`BusController`：lazy connection、30s idle timeout、3 次 timeout/retry。Session/System bus proxy。Systemd launcher、privileged-helper、Ark DnD 通过共享 bus helper 路由。不引入 `async-io`。

### 键盘快捷键
Pane-scoped navigation、selection、zoom、filter、clipboard、undo、inline rename。`PaneId` 路由。

### 属性对话框 & 搜索栏 & Filter Bar
多选 metadata rows。Pane-local plain-text/glob filter、case-sensitive toggle、match count、filtered model cache。

### KDE 集成
Ark DnD 解析与 `extractSelectedFilesTo()`。Compress/Extract fallback（`ark --add --changetofirstpath --autofilename zip`/`--batch`）。Service menu 二级子菜单。

## Remaining Work

### SCTK/wgpu Shell 迁移

详细目标和阶段见：

- `docs/WGPU_SHELL_ROADMAP.md`
- `docs/WGPU_SHELL_ROADMAP.zh-CN.md`

- [~] Phase 0：新增独立 `fika-sctk` spike，不删除 GPUI binary，并把现有 `fika-wgpu`
  作为 scene/renderer 迁移输入而不是长期窗口层。当前 `fika-sctk` 已通过
  `smithay-client-toolkit` 创建 Wayland xdg-window、用 raw Wayland handle 初始化
  wgpu surface、把 Wayland event queue 接入 calloop `WaylandSource`、读取可选 path
  并输出目录统计；`src/bin/fika-sctk.rs` 已降为 8 行入口，实际代码拆入
  `src/bin/fika_sctk/{options,app,renderer,scene,wayland}.rs`；`SctkScene`
  现在拥有启动目录快照和目录统计，是后续承接 `fika-wgpu` retained scene 的边界。
  后续新增 SCTK shell 能力必须落入这些模块或继续按职责拆分。下一步是把 `fika-wgpu` 的
  `ShellScene`/renderer/input projection 搬到 SCTK/calloop 事件循环下。历史 `fika-wgpu`
  winit-backed spike 只作为迁移源和测试基线，不再承接新的 shell 功能；它已能打开独立窗口、接受可选 path 参数、通过 `fika_core::read_entries_sync`
  读取目录、用 `IconsLayout` 投影 retained geometry，Compact projection 会按每列
  可见名称中的最长项决定列宽，并用 solid quad batch 渲染 path bar、可见 item
  背景和 icon fallback 形状；真实文件名通过
  `cosmic-text` shaping/rasterization 进入 bounded label raster cache，再上传到
  临时 per-frame RGBA text atlas 绘制，当前 shell metrics 会应用 window scale
  factor，默认 Icons 图标保持 48 逻辑 px（1.5x 下为 72 物理 px），baseline text
  metric 为 14px/18px 以贴近 GPUI Fika 视觉尺寸；真实 MIME/theme icon 已按 XDG/GTK/KDE
  theme 解析，PNG/WebP/JPEG/BMP/GIF/ICO 通过 `image` 光栅化，SVG 通过
  `usvg/resvg` 光栅化，visible icon 打包到 per-frame RGBA icon atlas，并保留
  bounded icon raster cache；semantic icon path resolve 已移出 frame path，未缓存
  file-icon role 进入后台 resolver，当前 frame 先绘制 fallback，ready 后自动 redraw
  补齐；新 icon raster 也有每帧预算；pointer move/leave 和左键点击已通过 shell-owned
  retained hit testing 路由，支持 hovered item、单选、Ctrl/Meta toggle selection
  和 Shift range selection，并从同一 slot projection 绘制 hover/selection 状态；
  右键 context targeting 已通过同一 retained hit testing 路由，右键未选 item 会先同步
  selection，右键已选 item 会保留 multi-selection 并更新 focus，右键 content 空白区域会记录
  blank directory target 且不启动 rubber-band；第一版 shell-owned context menu overlay
  已接入 item/blank targets，菜单会 clamp 到窗口内，支持 row hover、Esc/外部点击关闭，
  现在使用不透明浅色 surface，并将 directory item 的 Open、file item 的 Open（通过 GIO default-application URI
  launch）、item Copy Location（通过 shell-owned Wayland text clipboard provider）
  和 item Copy/Cut（通过同一 provider 写入 Fika URI-list text encoding）以及 blank menu 的
  Paste（读取 Wayland text clipboard，解码 Fika/GNOME URI-list text 或 plain text，调用本地
  core transfer/text-paste helper，reload 目录，并在 Cut 成功后清空 clipboard）、Refresh
  和 Select All 分派到现有
  shell navigation/reload/selection path；Properties 会为 item 和 blank-directory targets
  打开轻量 shell-owned metadata overlay；blank menu 的 Create New 会打开最小
  shell-owned modal，支持 folder/file 选择、name capture、校验、真实 create/reload
  并选中新建条目；item Rename 会打开最小 shell-owned modal，支持 name capture、
  校验、真实 rename/reload 并选中重命名后的条目；item Move to Trash 会解析 clicked
  item 或 multi-selection，拒绝 remote paths，调用 core XDG trash handling 并 reload；
  Trash view context menu 会通过 core `TrashViewOperation` 分派 Restore From Trash、
  Delete Permanently 和 Empty Trash，并 reload Trash view；Restore conflict 会打开
  shell-owned confirmation overlay，Replace 会用 replace policy 通过 core restore 后 reload；
  item Cut 和 Paste 会拒绝 remote paths；file item 的最小 shell-owned Open With chooser
  已通过 core `MimeApplicationCache` 和 systemd-user launch plan 接入；Open With
  default-app selection、Open in New Pane、多 MIME `text/uri-list` clipboard export/import 等
  其余 action 先记录 pending 日志；
  文件内容区现在预留并绘制 shell-owned item-view scrollbar，Icons/Details 为右侧竖向
  track，Compact 为底部横向 track，scrollbar track/thumb 使用圆角绘制，并支持
  thumb drag 与 track click-to-drag 更新 retained scroll offset；frame log 输出 `scale=...` 和
  `content_scrollbar=0|1`；Icons/Compact 普通未 hover/未 selection item 不再绘制默认
  highlight/background，Compact label 左对齐且每项高亮宽度按该项文本宽度收缩；
  空白区域左键拖动已通过同一 retained Icons geometry 支持 rubber-band selection，
  普通拖动替换 selection，Shift 追加，Ctrl/Meta 相对按下时的 base selection 做
  toggle；keyboard navigation 已通过同一 retained selection state 处理 Arrow、
  Home/End 和 Page Up/Down，Shift 扩展 range，focus item 会滚入视口，`Ctrl/Meta+A`
  全选当前目录 entries，`Esc` 清空 selection 并取消任何 transient rubber-band；目录激活已走
  shell-owned input path，Enter 打开当前 focus/selected 目录，双击通过 retained
  hit testing 打开目录，Backspace 或 Alt+Up 加载父目录；Alt+Left/Alt+Right
  走同一有界 history stack，`F5` / `Ctrl/Meta+R` 可刷新当前目录且不写 history，并在 entry 仍存在时按名称保留
  selection/focus，history/reload 的 app-level mouse controls 仍待 toolbar 迁移；
  普通新导航只在读取成功后写入 back stack 并清空 forward history，并在加载新 path 时重置 scroll/selection/rubber-band
  transient state、刷新 hover、更新窗口标题且立即 present；初版 projection zoom 已由
  shell-owned retained geometry 驱动，`Ctrl/Meta + +`、`Ctrl/Meta + -` 和 `Ctrl/Meta + 0`
  调整/重置有界 zoom step，`Ctrl/Meta + wheel` 也映射到同一 zoom path，Icons/Compact 更新 item/icon/text slot metrics，Details 更新
  row/icon metrics，scroll 会被 clamp，focus item 保持可见，icon resolver 按 zoom 后的
  slot size 请求 raster；底部最小 shell-owned status bar 已开始绘制在 content pane 内，
  不再跨过 Places sidebar，显示 entries/dirs/files/selected/visible/view/zoom 摘要，并从
  content viewport 和 item hit testing 中排除；最小 shell-owned filter bar 已可用，`Ctrl/Meta+F` 激活，
  字符输入更新 retained plain-text name filter，Backspace 编辑，Enter 保留 pattern/filter 结果但停止继续吃文本，
  Esc 清空并关闭，layout/hit-test/hover/selection/select-all/keyboard navigation 均通过
  filtered model-index projection 路由；dotfile visibility 已进入 shell-owned retained
  projection，默认隐藏 hidden entries，`Ctrl/Meta+H` 可显示，selection 会随可见性切换保留或裁剪，
  app-level Hidden toggle 仍待 toolbar 迁移；最小 shell-owned pane-local location edit mode 已可用，
  `Ctrl/Meta+L`、`Ctrl/Meta+D`、`F6` 或点击 top path bar 激活，首次输入替换当前 path
  draft，Backspace/Delete 通过真实 caret 编辑，Arrow/Home/End 移动 caret，caret x 现在由
  `cosmic-text` shaped glyph layout 测量且 path label no-wrap，点击 path bar 外空白会安全取消 draft
  并恢复当前真实 path，Tab 复用 core `complete_location_input()` 补全，Enter 复用 core
  `resolve_location_input()` 并通过 retained navigation/history path 提交，Esc 取消；
  第一版 shell-owned Places 侧栏已作为顶部与 app-level toolbar 下方 pane 起点对齐的圆角 panel 绘制，通过公开 core API 构建
  Home、已存在的 XDG directories、Trash、Fika user places、primary
  `places-order.xml`、Network root、network bookmarks、Root，以及 app 启动时从 GIO
  snapshot 投影出的 mounted local devices，保留 row geometry，用
  最长路径前缀决定 active place，Places hover 与 item hover 分离，并将左键 place
  navigation 分派到同一 `load_path`/history path；Places sidebar 现在拥有独立 scroll
  offset、clipped row rendering、圆角 active/hover row background、圆角窄 scrollbar
  track/thumb，并支持 sidebar scrollbar thumb drag / track click-to-drag；Places 与 pane
  之间保留 splitter+gap，Places panel 右侧也保留内边距，避免贴紧 pane；directory item 和 blank-directory
  context menu 现在支持 Add to Places，会写回 Fika `places.xbel`、reload sidebar
  projection，并持久化 primary place order；Places 右键现在会创建 shell-owned place
  context target，并打开最小 context menu，分派 Open、Copy Location、Properties，以及
  editable user places 的 Remove；Remove 会写回 Fika `places.xbel`，裁剪对应
  place-order 条目，reload sidebar projection，并清理 stale place context state。
  日志已输出
  `--view icons|compact|details`，默认仍是 Icons baseline，Compact 使用 core
  `CompactLayout`，Details 已有 shell-owned row projection、固定 header 和
  Name/Size/Modified 三列；运行时可用 `1/2/3`、`Ctrl/Meta+1/2/3` 或 fallback `F1/F2/F3` 切换三种模式，
  临时 top-bar mode buttons 已移除以贴近原版 app toolbar，真实 toolbar controls 仍待迁移，
  `--auto-cycle-views` 可每秒自动切换一次以调试 compositor/render。切换会 clamp
  active scroll axis、清理 transient rubber-band state、刷新 hover、更新窗口标题，
  立即输出 `[fika-wgpu] view-mode=...` 日志，并保持短 redraw burst 直到切换后的
  scene 被 present。
  日志已输出 view mode、path、entry count、visible count、thumbnail candidate count、
  retained visible slot active/free/reuse/recycle/allocation counters、
  selected/hover/places/context/context-menu/properties/rubber-band state、
  hit-test/selection/context/context-menu-action/properties/keyboard/rubber-band/view-switch/path-change/open/copy-location/file-clipboard/paste/places/zoom/DnD counters、quad/icon/text/batch count、
  icon/text cache hit/miss/bytes、icon deferred/raster-deferred、layout/icon-resolve/icon-raster/text-raster/render
  reason/time、icon/text atlas bytes 和 `scroll_x` / `scroll_y` offsets；本地目标 desktop
  session 的 `timeout 4s target/debug/fika-wgpu --view icons|compact|details /etc`
  smoke 均已到达 `shell-ready` 和 `frame=1`，输出 `surface-format=Rgba8Unorm srgb=0`
  以及真实 icon/text atlas counters。
  仍待接入 glyph-level cache/atlas retention、真实 Wayland DnD hover/export/drop 执行、手动打开/关闭/交互 smoke，
  以及确认 Phase 0 默认 Compact/Icons 视图。
- [~] Phase 1：Compact、Icons 和 Details scene projection 已开始接入。`/etc` 已可通过
  `--view` 在三种模式下渲染首帧；Compact 走 core `CompactLayout`，Details 走 shell-owned
  row projection。滚动、hover、keyboard navigation、directory activation/history navigation、
  runtime mode switching、projection zoom、reload、location editing、filtering、hidden-file visibility、selection 和全选/清空快捷键已通过 shared
  `ShellLayout` abstraction 走 retained geometry；
  primary/split pane 现在开始通过 `ShellPaneProjection` 共享 pane view、geometry、visible
  item、item painter、scroll metrics 和 path-keyed visible slot pool/reuse list；split pane
  scrollbar/滚轮路由现在会更新目标 pane，frame log 不再为了 content scrollbar telemetry 重新计算一次主 pane layout；
  glyph-level text zoom policy、`~/Downloads` smoke、手动交互 smoke
  和更完整 Details column/metadata parity 仍待完成。
- [~] Phase 2：把 Phase 0 初版 icon atlas 提升为预算化 semantic icon work，并实现 thumbnail texture retention、text shaping cache、glyph atlas policy 和 eviction telemetry。当前 semantic icon path resolve 已按 Dolphin role updater 思路移到后台 worker，frame 只消费 ready 结果并用 fallback 占位，new raster 每帧预算化；thumbnail candidates 已从可见 retained pane items 投影并输出 telemetry，第一版 wgpu thumbnail worker 已在 frame 外复用 core freedesktop cache/failure marker/thumbnailer registry 并把 ready PNG 光栅化进 icon atlas path；visible thumbnail 请求会优先于 deferred work，且每帧会按 Dolphin 顺序排入有界 read-ahead queue，ready read-ahead raster 会先进有上限的 resolver cache，日志输出 `thumb_read_ahead` 和 `thumb_ready` 计数。model role writeback、长期 thumbnail atlas retention、glyph policy 和 eviction telemetry 仍待完成。Cold glyph/icon/thumbnail work 必须 visible-first 且预算化。
- [~] Phase 3：把剩余 pointer routing、context target selection、directory hover、Places hover 和 drag/drop target lookup 移到 shell-owned hit testing。当前 file view pane item/blank 右键 context target selection、菜单 row hit testing、Places row hover/left navigation/right-click target/sidebar scrolling 和最小 place menu（含 editable user place Remove）已由 wgpu shell-owned；mounted/unmounted devices metadata 已从 GIO snapshot 投影进 Places，并能生成 Mount/Unmount/Eject/Safely Remove context rows，device operation 执行仍待接 core；第一版 `ShellDropTarget` lookup 已可区分 primary/split pane item、pane blank、place row 和 Places blank，用同一 `ShellPaneView`/pane geometry 路径命中。主 pane item drag 已有内部 drag session，超过阈值后更新 retained drop hover，release 时为有效 pane/place target 生成 retained Copy request；真实 Wayland DnD hover/drop/export、执行该请求、directory hover、device monitor/actions、place edit/hide/add action dispatch 仍待迁移。
- [~] Phase 4：实现 Places、toolbar、location bar、filter bar、status bar、context menus、dialogs 和 chooser mode，使常见文件管理器工作流不需要启动 GPUI shell。当前已有独立 app-level toolbar（移除临时 Back/Forward/Reload/Hidden/view-mode 鼠标按钮，仅保留原版 Places toggle 形态，相关 keyboard commands 仍可用；Places toggle 现在可实际隐藏/恢复 sidebar）、从 toolbar 下方带 margin 开始的 pane、顶部与 pane 起点对齐且补齐原版 title/row/icon 尺寸的圆角 shell-owned Places panel/左键 navigation/独立 sidebar scrolling/最小 Open-Copy Location-Properties-Remove row menu、mounted/unmounted device rows 和 Mount/Unmount/Eject/Safely Remove 菜单入口、Places splitter 拖拽改宽并有 `ColResize` cursor 提示、split-pane divider 拖拽改宽并有相同 cursor 提示、文件内容区 item-view scrollbar 显示/scroll offset 同步、内容区与 Places scrollbar 圆角 track/thumb 和 drag/click-to-drag 交互、pane-local 底部 status bar、窄实现 filter bar、28px pane-local location edit mode、不透明浅色 item/blank context menu overlay（原版 196px 宽度、28px row、4px vertical padding、8px viewport margin、edge flip/clamp 定位、18px icon slot、shadow、row separators、几何 fallback 图标，overlay quads/text 独立顶层 pass，避免底层 item text 透出，并用同一 padding-aware row hit-test 驱动 hover/点击）、最小 properties metadata overlay、最小 Create New modal、最小 Rename modal、最小 Move to Trash dispatch、最小 Trash view Restore/Delete Permanently/Empty Trash dispatch、最小 Trash restore conflict Replace overlay、file Open 默认应用分发、最小 Open With chooser/systemd-user launch plan 分发、item Copy Location Wayland text clipboard 分发、item Copy/Cut 的 Fika URI-list text clipboard 导出，以及 blank Paste 的本地文件/纯文本执行；Open directory、Open file、Open With chooser、Copy、Cut、Paste、Copy Location、Refresh、Select All、Properties、最小 Create New、最小 Rename、最小 Move to Trash、最小 Trash view actions 已接入 dispatch。更完整 Places actions/device monitor/device action execution/DnD、toolbar、完整 location/filter/create-name/rename-name/application-search 文本边界、多 MIME `text/uri-list` clipboard export/import、remote paste、更完整 Trash multi-conflict handling、undo、更完整 properties、完整 inline rename、完整 Create New 子菜单/模板、Open With default-app selection、new-pane actions、dialogs 和 chooser mode 仍待迁移。
  当前增量：blank context menu 已接入 Show/Hide Hidden Files 和 Split View；
  directory/place 的 Open in New Pane 会加载右侧 split pane 的真实目录内容；file-view
  item hover/selection 改为圆角高亮；location bar 改为白底、细边框、leading folder
  glyph、active focus ring 和垂直居中的真实 caret，并支持 Arrow/Home/End/Delete 光标编辑；
  Places 与 pane 之间加入明确间距，pane 硬蓝/灰外框弱化为更贴近背景的细分隔线；Places sidebar 可通过 toolbar toggle 隐藏并释放 pane 宽度，也可拖拽 splitter 调整宽度；Places splitter 命中区已向 Places 内侧扩宽，且 Places scrollbar 在重叠区域优先于 splitter resize；地址栏 hover 显示 text cursor，Places splitter 和 split-pane divider hover/drag 均有 `ColResize` cursor 提示；Trash place 在为空时不再显示蓝色状态圆点。
  Open With 直接应用子菜单、KDE/Fika service-menu TopLevel/More Actions/`X-KDE-Submenu` 子菜单和 service action systemd-user launch plan 已接入 wgpu context menu；application/service `Icon=` 现在会按 named theme icon 异步解析到 wgpu overlay icon pass，未解析成功时仍回退到紧凑 glyph。Properties/Create/Rename/Open With/Trash conflict overlay geometry 与 hit-test 现在按 window DPI factor 缩放。Thumbnail 工作已推进到 frame 外 core cache/thumbnailer probe、visible-priority dispatch、有界 Dolphin-order read-ahead queue、有界 resolver ready cache、background raster 和 mtime-keyed failure handling；完整 Dolphin/KIO PreviewJob cancellation、model-role writeback、长期 atlas retention 仍待完成。
  当前 split pane 在交互焦点和 file operations 上仍是最小可见骨架，但 divider resize、content scrollbar 和滚轮路由已接入；pane 可复用化继续推进：
  主 pane 与右侧 pane 现在可投影为同一个 `ShellPaneView`，并通过共享 `pane_layout(...)`
  生成 Icons/Compact/Details layout；主/右 pane geometry 与 item hit-test 已抽到共享路径，primary/split pane item paint 已共用 `ShellPaneProjection` + generic pane item painter，scrollbar metrics 和 visible slot pool/reuse 也从同一 projection/metrics 路径读取。下一步必须继续把 pane focus 和 file-operation routing
  接入同一个可复用 pane component，而不是继续堆 split-only 特例。
- [ ] Phase 5：同场景证据证明行为对齐，且 frame cost 比 GPUI Fika 和相关 cosmic-files 基线更好或更可预测后，再把新 shell 提升为默认。默认化前必须有：`/etc`、`~/Downloads`、large-dir、split-pane 和 hidden-file smoke；GPUI fallback binary/launch path 保留；主线 binary 命名/desktop file/CLI default 选择清楚；scroll/zoom/context/location/Places/DnD 交互 smoke 有记录；性能日志至少覆盖 cold icon/text、steady scroll、view switch 和 split pane。

### GPUI Item View 自绘 / Dolphin retained item 对齐（历史基线）

以下工作已形成当前 GPUI 基线和性能证据。它们不再是长期 UI 主线；迁移新 shell 时应复用
其中的 retained model、cache 语义和 smoke 经验。

历史设计和迁移任务见：

- `docs/ITEM_VIEW_CUSTOM_PAINT_DESIGN.md`
- `docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md`
- `docs/ITEM_VIEW_RENDERER_DECISIONS.md`
- `docs/ITEM_VIEW_RUNTIME_SMOKE.md`

- [x] Phase 1：非重命名、非缩略图 item 的静态视觉转向自绘，保留当前交互 shell。
- [x] Phase 2：静态文本 shaping cache，resize 时复用已成形文本。
- [x] Phase 3：显式 retained paint slot state，区分 geometry-only/content/visual changes。
- [x] Phase 4：缩略图/图片绘制边界收敛到 retained image path。
- [x] Phase 5：从 `canvas` spike 升级到 dedicated GPUI custom element。
- [x] Phase 6：静态 fallback Compact/Icons item 上提到 content-level 自绘 layer，item shell 仅保留交互。
- [x] Phase 7：所有非 rename Compact/Icons item 的背景/文字进入 content-level 自绘 layer，thumbnail image 进入独立 content-level image layer，item shell 只保留交互/drag-start/rename。
- [x] Phase 8：thumbnail image layer 改为 custom paint element，复用 GPUI `RetainAllImageCache`/`ImageAssetLoader`，用 `Window::paint_image` 直接绘制；MIME/theme icon 已继续推进到默认 full custom image layer，并删除旧 GPUI `img()` renderer bridge。
- [x] Phase 9：custom element hitbox 迁移完成。非 rename Compact/Icons hover/cursor、drop hit testing 和 drag start 均走 retained hitbox/controller 路径；drag start 通过 Fika GPUI fork 的 hitbox typed DnD API 注册，不再需要 per-item GPUI `Div::on_drag` shell。每一步都必须保留 Dolphin model/controller/painter 分层，并用 perf logs 证明不劣于 GPUI built-in 路径。
- [x] Phase 10：rename 只保留 overlay editor，普通背景/文字/图片继续走 content-level layer。
- [x] Phase 11：Details row 已进入 retained paint slot，背景/图标/文字已转入 content-level custom visual layer；click/menu/navigation/scroll/middle-paste、drop dispatch 和 drag start 已走 viewport retained hit testing/drop handlers；row 不再需要 GPUI drag-start shell。继续扩大自绘前必须用 perf 证明不劣于 GPUI built-in 路径。
- [x] Phase 12：剩余 DnD 边界已通过 Fika GPUI fork 解决。fork 分支 `fika/gpui-hitbox-dnd` 暴露 `Window::on_hitbox_drag*`/`on_hitbox_drop`，Fika pin 到 `572d53326f722e5634647b2276c42069d6b5b63d`；runtime DnD/perf 验证清单见 `docs/ITEM_VIEW_RUNTIME_SMOKE.md`。
- [~] Phase 13：renderer decision gate 已建立：每个 surface 先记录 Dolphin-style model/layout/controller/painter owner，再由 runtime perf 和行为证据决定保留 custom paint 还是 GPUI built-in renderer。当前决策见 `docs/ITEM_VIEW_RENDERER_DECISIONS.md`。
- [x] Phase 14：Dolphin-style retained icon image cache 已成为默认 MIME/theme icon 路径。设计见 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md`。当前按 semantic `ThemeIconImageKey`（icon name、size、scale、theme/color/mode 哨兵）管理 cache 和预算；visible-cohort readiness handoff、source-image decoded reuse 和 GPUI `img()` renderer bridge 已删除。普通 pane evidence 必须保持 `gpui_image_element=0`。
- [x] Phase 14a：pane/Details/Places 已共用 retained image request/load 模型；Places 专属 image cache 壳已移除，thumbnail retained fallback 已改为有界 LRU，Dolphin role-updater read-ahead 顺序已集中到 `ui::retained::work_order`；pane/Places 文字 shape cache/stat 与 slot delta stats 已收敛到 retained 公共层，direct thumbnail/theme image load helper 已私有化，后续 image 使用方必须走 `RetainedImageRequest`。
- [x] Phase 15：全面转向 Dolphin retained model 的核心迁移已完成并有 core evidence。文件视图和 Places 现在共用 retained image request/load、bounded image cache、visible-first work order、shape cache/stat 和 slot delta 语义；Compact/Icons、Details 与 Places 默认视觉路径均为 retained/custom，Places 默认 full row visual 和 retained-DnD target delivery。最终证据 `scripts/run-retained-renderer-evidence.sh --core --skip-build --prefix fika-core-final-retained-v3` 已通过：item 覆盖 Compact/Icons/Details，Places 覆盖 targets/overflow/layout/hit-test/targeting/dnd。
- [x] Phase 16：DnD shell 全面移除完成。Fika 维护专用 GPUI fork/branch `fika/gpui-hitbox-dnd`，Fika pin `gpui`/`gpui_platform` 到 `572d53326f722e5634647b2276c42069d6b5b63d`；Compact/Icons、Details、Places drag start 使用 retained hitbox typed DnD，Places typed move/drop 使用 retained sidebar content hitbox。Analyzer gate 要求 `gpui_drag_shell=0`、`drag_shells=0`、`gpui_event_shells=0`、`gpui_typed_dnd_payload_shells=0`，并且 Places DnD 通过 `--expect-retained-event-policy`。证据 `scripts/run-retained-renderer-evidence.sh --core --skip-build --prefix fika-full-retained-hitbox-dnd-v2` 已通过，item 日志 `max_gpui_drag_shell=0`，Places full retained-event gate 通过且 GPUI DnD shell 计数全为 0。剩余明确 GPUI 边界是 rename editor overlay，以及 GPUI/backend 尚未提供的外部 MIME drag/export 能力。

### GPUI Backend / External MIME Drag (阻塞)
- [~] 外部 MIME 拖出：`DragExportPayload`（`text/uri-list` + `text/plain`）已构造，但 GPUI/Wayland backend 尚未提供从 app 内部 drag source 向外部应用发布 MIME 的 API。待 backend 支持后接入。
- [~] 外部 MIME 拖入：Ark service/path MIME（`application/x-kde-ark-dndextract-service/path`）已就绪 core parser 和 executor，但 GPUI/backend 多 MIME data offer API 仍待支持。当前普通文件路径拖入通过 `ExternalPaths` 工作。
- [~] Move 专用 drag cursor/icon 仍需 GPUI/backend 暴露对应 cursor style。

### Network 网络文件系统
- [x] Backend 边界决策：GVfs/GIO 后端。`src/core/network.rs` 支持 URL scheme 解析、`NetworkLocation` 模型、`NetworkAuth`、GVfs filesystem type 分类、`network:///` root 枚举和 remote URI listing。
- [x] Saved network bookmarks 和 Add Network Drive UI。
- [x] 认证交互、取消、结构化错误报告。GVfs scan cancellation、结构化 auth/GIO error、in-app credential prompt、内存凭据重试已接入。
- [x] `DirectoryLister` 集成 network scan，无 pane 闪烁。
- [x] Remote/GVfs metadata 降级（MIME、thumbnail、size、watcher）。
- [x] Remote 位置的文件操作和 DnD 语义：remote URI 可导航/复制，local file ops、DnD transfer、trash/rename/create/paste 和 privileged helper 会显式拒绝 remote path。

### KDE Service Menu 高级条件
- [ ] 依赖 KIO/权限上下文的 `X-KDE-*` 高级条件（如 `X-KDE-Require=`、`X-KDE-ShowIfRunning=` 等）。

### Trash 多存储聚合
- [ ] Dolphin/KIO 的 `trash:/` 多存储聚合（removable storage `.Trash-$uid`）。
- [ ] Removable storage trash 可访问性刷新。

### 交互细化
- [ ] 真实运行中 inline rename 端到端视觉验收。
- [~] 设备操作（mount/unmount/eject）的 Polkit 交互和用户取消流程仍需端到端验证。
- [ ] View Mode 下的 Icons/Details 视图切换（当前只有 Compact 主视图和 Details 列视图）。

### 图标与缩略图性能对齐（Dolphin）

详细分析见 `docs/ICON_THUMBNAIL_PERFORMANCE_ANALYSIS.md`。

观察到两个视觉跳变：
1. 文本文件首次渲染时先显示 `unknown` 齿轮图标，1-3 帧后跳变为正确文本图标。
2. 已缓存的缩略图首帧未显示，异步探测完成后跳变替换文件图标。

- [x] **P0 — PreliminaryFile 扩展名智能回退**：`FileIconKind::PreliminaryFile` 使用扩展名驱动的智能候选（`text-x-{ext}` → `text-x-generic` → `unknown`），让未完成 magic MIME 的文件先显示稳定的初级图标。
- [x] **P1 — metadata 异步批量化**：`METADATA_ROLE_BATCH_SIZE` 为 16，降低 magic MIME 解析的异步往返次数。
- [x] **P2 — 缩略图角色调度和 read-ahead**：thumbnail probe 继续走 scheduler，按 Dolphin visible/read-ahead 顺序调度，成功/失败写回 model role。
- [x] **P3 — 图标 theme path 后台 resolve**：渲染帧只调用 `FileIconCache::cached_or_preliminary_icon_for()`，cache miss 返回无 I/O 的 preliminary/fallback snapshot；`RawFileGridSnapshot::queue_file_icon_resolve_candidates()` 投影 Dolphin visible/read-ahead 顺序，`FileIconResolveQueue` 管理后台 theme path resolve 的 queued/seen/in-flight 状态。
- [x] **P4 — zoom 缩略图 fallback 稳定性**：thumbnail 图片 pending 或 load failure 时由 image paint layer 绘制 item fallback，避免 zoom 期间出现空白图标 rect。
- [x] **P5 — visible MIME icon 首帧稳定性**：对齐 Dolphin `updateVisibleIcons()` + `pixmapForIcon()`，目录加载和 zoom 时在 snapshot 转换前用 Dolphin `MaxBlockTimeout = 200ms` 同步解析 visible item 的 theme icon path；read-ahead/offscreen icon 仍走后台队列。
- [x] **P6 — theme icon fallback marker 去除**：theme icon 图片尚未加载或加载失败且没有 retained same-`iconName` 图片时，只使用中性无文字占位，不再显示 `TXT/IMG/FILE` 等 MIME marker。
- [x] **P7 — file-grid 根级 image cache 清理**：thumbnail 和 MIME/theme icon 均由 custom image paint layer 负责；旧 root-level `image_cache(retain_all(...))` provider 不再作为所有 item image 的隐式边界，普通 MIME/theme icon 不再有 GPUI `img()` renderer bridge。
- [x] **P8 — Dolphin icon visual stability 对齐**：theme icon pixmap cache 按 `ThemeIconImageKey` 键控，self-painted layer 可为缺失 key 同步 rasterize SVG。Zoom 期间普通 MIME/theme icon 使用当前 layout icon size，避免 300ms 后二次调整；Dolphin 300ms timer 只作为 preview/role work 的参考边界。

### 双运行时对齐（COSMIC Files）

Fika 的 `operation_runtime.rs` 在 Tokio+Compio 线程边界层面对齐 COSMIC Files。
详见 `docs/OPERATION_RUNTIME_REFERENCE.md`。

- [x] **Phase 1.1** — 启用 `io-uring`：`Cargo.toml` 中 `compio` features 从 `polling` 切换到 `io-uring`。
- [x] **Phase 1.2** — 引入 `OperationId(u64)`：`submit()` 返回 operation id，runtime 层获得操作级身份。（`src/core/operation_runtime.rs:18`）
- [x] **Phase 1.3** — 非 panic 错误路径：替换 `.expect()` 为 `Result` 传播，runtime shutdown 可被 GPUI 层优雅处理。
- [x] **Phase 2.1** — 定义 `Operation` enum：统一 Transfer/Trash/Rename/Create/Undo/TrashView/External 提交路径。（`src/core/operations.rs:39-73`）
- [x] **Phase 2.2** — 添加 `OperationController`：统一的 cancel/progress/pause 状态，替换 `AtomicBool` + `Arc<Mutex<TransferProgress>>`。（`src/core/operation_runtime.rs:48-105`）
- [x] **Phase 2.3** — Runtime 级操作跟踪：`BTreeMap<OperationId, OperationHandle>` 移入 `OperationRuntime`，GPUI 层只查询不自行维护 `active_background_tasks`。（`src/core/operation_runtime.rs:139`）
- [x] **Phase 3.1** — 递归复制：`src/core/file_ops.rs` 使用 Compio async API 做目录创建、文件复制和递归分发；Compio 缺口（目录枚举、metadata/readlink/remove）通过 runtime blocking pool fallback。
- [x] **Phase 3.2** — GIO fallback：GVfs 远程文件通过 `run_operation_blocking()` 路由 GIO `File::copy()`。

### 验证与测试
- [ ] 端到端测试：多 pane 同时访问同一目录的并发安全。
- [ ] 端到端测试：D-Bus session bus 不可用时的降级行为。
