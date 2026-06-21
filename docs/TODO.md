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

- [~] Phase 0：`fika-sctk` 是唯一继续迁入新 shell 能力的目标。现状：
  `src/bin/fika-sctk.rs` 仍是薄入口；实际实现拆入
  `src/bin/fika_sctk/{options,app,renderer,scene,pane,wayland,metrics,quad,text}.rs`。
  它通过 `smithay-client-toolkit`/`wayland-client` 创建 xdg-window，用 raw Wayland
  handle 初始化官方 crates.io `wgpu` surface，并通过 calloop `WaylandSource`
  驱动 Wayland event queue。`fika-wgpu` 只保留为历史迁移输入和必要编译基线，
  后续不得继续承接新 shell 行为。
- [~] Phase 1：目录场景已经迁到 SCTK 基线。`SctkScene` 读取
  `fika_core::read_entries_sync` 快照，持有 `ViewMode`、scroll、hover、selection，
  用 core `IconsLayout`/`CompactLayout` 和 shell-owned Details rows 投影
  `icons|compact|details` 三种 view。renderer 已从 clear-frame 升级为上传/绘制
  quad batches，并绘制 app 背景、Places 面板、pane、地址栏、item fallback icon、
  hover/selection 圆角高亮、content scrollbar、status bar 和 Details header。
  Wayland pointer capability 已接入 hover、左键单选和滚轮 scroll，全部走同一
  retained hit-test/scroll path。
- [~] Phase 2：真实文本渲染已进入 SCTK。新增 `src/bin/fika_sctk/text.rs`，
  使用 `cosmic-text` shaping/rasterization，把地址栏 path、Places 标题/行、status
  summary、Icons/Compact 文件名、Details header/name/size/modified label 打包进
  per-frame RGBA text atlas，并在 quad pass 后用 textured quad pass 绘制。`/etc`
  三种 view smoke 均已到达 `shell-ready`/`frame=1`，并输出非零
  `text_labels/text_quads/text_atlas` counters。HiDPI 路径已修正：SCTK app 会按
  Wayland integer `wl_surface.set_buffer_scale` 配置物理 `wgpu` surface，text raster、
  text screen rect 和 quad rect 都进入同一物理 surface 坐标系；之前 quad rect 已按
  scale 放大但 NDC 分母仍用逻辑尺寸，导致 1.5/2x 下整体偏移。exact fractional
  viewport 路径先保留为 `FIKA_SCTK_EXACT_FRACTIONAL_VIEWPORT=1` 实验入口，默认不启用。
  下一步要把该第一版 per-frame atlas
  提升为 retained glyph/label cache，补 eviction telemetry，避免 steady scroll 反复
  rasterize 可见 label。
- [~] Phase 3：继续 pane 可复用化。当前增量已新增
  `src/bin/fika_sctk/pane.rs`，把 pane path/view/entries/dir_count、scroll、hover、
  selection、Icons/Compact/Details layout projection、item painter、content scrollbar、
  status bar、pane-local location bar、hit-test、left-click selection 和 wheel scroll
  从 app scene 中抽到 `SctkPane`。`SctkScene` 现在只负责 app 背景、toolbar band、
  Places panel 以及把 pointer/scroll 路由到 active pane geometry。当前 `--split`
  会打开同一路径的右侧 pane，`--split-path PATH` 会打开指定路径；primary/split
  pane 已共用 `SctkPane`，首帧会输出 `split_pane=1 active_pane=...` telemetry，
  鼠标 hover、左键 selection 和带坐标的 wheel scroll 会路由到命中的 pane。SCTK
  keyboard seat 已接入，`F1/F2/F3`、`1/2/3`、方向键、Home/End、PageUp/PageDown、
  Enter、Esc、`Ctrl/Meta+H`、`Ctrl/Meta+L` 和 `F5`/`Ctrl/Meta+R` 会通过 `SceneCommand` 路由到
  active pane；pane 内部已拥有 hidden-file 可见索引缓存、runtime view switching、
  keyboard navigation、目录激活、reload 和 pane-local location edit。地址栏现在可通过
  `Ctrl/Meta+L` 或点击 active pane 的 location bar 聚焦，支持 UTF-8 文本插入、
  Backspace/Delete、方向/Home/End、Enter 提交路径导航、Esc/外部点击取消并恢复当前路径。
  分屏命令也已进入 scene 层：`F4` 或
  `Ctrl/Meta+Shift+S` 会打开/关闭复用同一 `SctkPane` 的 split pane。active pane
  已绘制轻量焦点标记，content scrollbar thumb/track drag 已走 scene pointer capture，
  release/leave 会清理 capture。pane selection 已从单选焦点扩展为可复用的
  `selected_entries` 集合；`Ctrl/Meta+A` 只选择 active pane 的可见条目，空白 primary
  press 会进入 rubber-band pointer capture，并在 Icons/Compact/Details 当前投影上更新
  多选集合和半透明 overlay，为后续 DnD source/export 复用同一 selection 边界。下一步继续补
  toolbar split UI、filter edit、file-operation routing、IME/text-selection 和 DnD data-device。
- [ ] Phase 4：迁入资产和系统集成热路径。MIME/theme icon atlas、Dolphin-style
  visible-first icon resolve、thumbnail worker/read-ahead、device/Places 动态数据、
  context menu、Open With、service menu、clipboard、Trash actions、file operations、
  dialogs 和 chooser mode 仍未接到 SCTK shell。迁移时优先复用已有 core 能力，
  UI 层只负责 retained geometry、hit-test、overlay state 和 GPU batches。
- [ ] Phase 5：Wayland DnD 和主线化。需要完成 SCTK data-device export/drop、
  internal pane/place drop target、copy/move/link drop menu、drag preview/hover、
  外部 `text/uri-list` 互操作；然后用 `/etc`、`~/Downloads`、large-dir、
  split-pane、hidden-file、thumbnail、context menu、DnD、scroll/zoom/location smoke
  证明行为稳定，再决定把 `fika-sctk` 提升为默认。GPUI 保留为兼容 fallback，
  `fika-wgpu` 只保留到迁移输入不再需要时删除。

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
