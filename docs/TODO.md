# Fika TODO: GPUI Mainline

本文档是当前任务板。仓库已切到单包 GPUI 主线；后续任务只应进入
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
- [x] 直接 crates.io 依赖不使用 `*`。版本声明保持最新稳定大版本范围，不锁到 patch/minor。
- [x] 新实现不得把 UI widget identity 当作文件模型 identity。GPUI view/entity 是渲染层，文件身份属于 core model。
- [x] 功能提炼与集成：Dolphin 是 UI 行为和文件操作流程的第一参考；cosmic-files 是纯 Rust 系统集成的参考源。两个源码库中提炼的功能统一集成到 `fika-core`，UI 层只做渲染和输入路由。
- [x] Dolphin 分层模型对齐：渲染层不做数据决策，模型层不持有 UI 句柄，交互层不直接操作文件系统。
- [x] 文件拆分：`src/main.rs` 只保留 app 状态编排和跨模块路由。所有功能模块已拆入 `src/core/`（domain logic）和 `src/ui/`（rendering），子职责继续按目录式模块拆分。
- [x] 图标模型已收敛为按需路径缓存：删除 `ModelEntry.icon_name`、`src/ui/icons/roles.rs`、`RenderImage` 自解码路径；图标由 `FileIconCache` 按 exact `FileIconKind + icon_size` / named icon 缓存，同时同一 `FileIconKind` 已有 resolved path 时跨 zoom 尺寸复用该稳定 path。文件视图图片策略按证据拆分：缩略图由 custom image paint layer 通过 `Window::paint_image` 绘制并复用 GPUI `RetainAllImageCache` 解码结果，MIME/theme icon 默认由 GPUI `img()` 渲染，仍由 retained item snapshot 和 stable file-icon path 驱动。`FIKA_CUSTOM_THEME_ICONS=1` 可强制 theme icon 回到 custom image layer 做 A/B。Zoom 对齐 Dolphin 普通图标路径：布局立即变化，MIME/theme icon path identity 不套用 300ms preview role-size timer，也不因每个 zoom size 提交第二次 path identity。

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

### Item View 自绘 / Dolphin retained item 对齐

详细设计和迁移任务见：

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
- [x] Phase 7：所有非 rename Compact/Icons item 的背景/文字进入 content-level 自绘 layer，thumbnail image 进入独立 content-level image layer，item shell 只保留交互/drag-start/rename 和明确的 GPUI theme-icon renderer bridge。
- [x] Phase 8：thumbnail image layer 改为 custom paint element，复用 GPUI `RetainAllImageCache`/`ImageAssetLoader`，用 `Window::paint_image` 直接绘制；MIME/theme icon 默认保留 GPUI `img()`，custom theme-icon paint 只作为 A/B 开关。
- [~] Phase 9：custom element hitbox 迁移分两步。P9a 已开始把非 rename Compact/Icons hover/cursor 放进 retained hitbox layer；P9b 删除 drag shell 需等待 GPUI 公开 custom-element drag-start API 或引入可审计 GPUI patch。每一步都必须保留 Dolphin model/controller/painter 分层，并用 perf logs 证明不劣于 GPUI built-in 路径。
- [x] Phase 10：rename 只保留 overlay editor，普通背景/文字/图片继续走 content-level layer。
- [x] Phase 11：Details row 已进入 retained paint slot，背景/图标/文字已转入 content-level custom visual layer；click/menu/navigation/scroll/middle-paste 和 drop dispatch 已走 viewport retained hit testing/drop handlers，row shell 只剩 GPUI drag-start 边界。继续移除 row shell 或扩大自绘前必须用 perf 证明不劣于 GPUI built-in 路径。
- [~] Phase 12：剩余边界已审计：GPUI 0.2.2 公开 drag-start 仍绑定 `Div::on_drag`，custom element 只有 hitbox/mouse event 路径；P9b 需要公开 custom-element drag-start API 或可审计 GPUI patch。runtime DnD/perf 验证清单见 `docs/ITEM_VIEW_RUNTIME_SMOKE.md`，下一步先执行 smoke 和 post-P11e perf log 收集。
- [~] Phase 13：renderer decision gate 已建立：每个 surface 先记录 Dolphin-style model/layout/controller/painter owner，再由 runtime perf 和行为证据决定保留 custom paint 还是 GPUI built-in renderer。当前决策见 `docs/ITEM_VIEW_RENDERER_DECISIONS.md`。
- [~] Phase 14：深入解决 MIME/theme icon 全自绘前，先实现 Dolphin-style retained icon image cache。设计见 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md`。目标是按 `(iconName, icon_size_px)`（必要时加 theme/scale/color-scheme）保留真实图像，刷新期间复用同 key 已加载图像，禁止 prepaint 同步解码，并用默认 GPUI `img()` vs custom renderer 的成对 `/etc` 和混合目录 autosmoke 证明没有首帧 placeholder、缩放二次跳变、`theme_decoded` burst 或 `icon_sync` 回归。通过前，普通 MIME/theme icon 默认继续走 GPUI `img()`。
- [~] Phase 15：全面转向执行计划已写入 `docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md#p15-full-transition-execution-plan`，Places chrome 默认之后的总路线图见 `docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md`。下一步不是盲目删除所有 GPUI 边界，而是先冻结当前 runtime 证据，再按 MIME/theme icon、Places retained event delivery、drag-start、rename、ownership cleanup 等证据门槛逐段迁移。2026-06-18 更新：Places event delivery 已有显式 `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe` 计数入口和非变更 retained sidebar hitbox layer；`retained-pointer` 进一步把 pointer cursor 和 active-drag leave clearing 放到 opt-in retained layer。下一步是迁 activation/context-menu targeting 或继续收窄 typed DnD move/drop delivery。

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
- [x] **P7 — file-grid 根级 image cache 清理**：thumbnail 已由 custom image paint layer 负责；旧 root-level `image_cache(retain_all(...))` provider 不再作为所有 item image 的隐式边界。MIME/theme icon GPUI `img()` 是显式 renderer-policy path。
- [x] **P8 — Dolphin icon visual stability 对齐**：theme icon 不在 GPUI prepaint 同步读文件/解码；默认 GPUI `img()` 路径由 retained item snapshot 和当前 layout icon size 的 `FileIconCache` 路径驱动。Custom theme-icon paint 仅通过 `FIKA_CUSTOM_THEME_ICONS=1` 做对比。Zoom 期间普通 MIME/theme icon 使用当前 layout icon size，避免 300ms 后二次调整；Dolphin 300ms timer 只作为未来 preview/role work 的参考边界。

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
