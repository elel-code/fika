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
- [x] 图标模型已收敛为按需路径缓存：删除 `ModelEntry.icon_name`、`src/ui/icons/roles.rs`、`RenderImage` 自解码路径；图标由 `FileIconCache` 按 `FileIconKind + icon_size` / named icon 缓存，GPUI `img(path).with_fallback()` 懒加载。

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
Ark DnD 解析与 `extractSelectedFilesTo()`。Compress/Extract fallback（`ark --add`/`--batch`）。Service menu 二级子菜单。

## Remaining Work

### GPUI Backend / External MIME Drag (阻塞)
- [~] 外部 MIME 拖出：`DragExportPayload`（`text/uri-list` + `text/plain`）已构造，但 GPUI/Wayland backend 尚未提供从 app 内部 drag source 向外部应用发布 MIME 的 API。待 backend 支持后接入。
- [~] 外部 MIME 拖入：Ark service/path MIME（`application/x-kde-ark-dndextract-service/path`）已就绪 core parser 和 executor，但 GPUI/backend 多 MIME data offer API 仍待支持。当前普通文件路径拖入通过 `ExternalPaths` 工作。
- [~] Move 专用 drag cursor/icon 仍需 GPUI/backend 暴露对应 cursor style。

### Network 网络文件系统
- [~] Backend 边界决策：GVfs/GIO、KIOFuse 或二者兼容的小型抽象层。`src/core/network.rs` 已完成 URL scheme 解析、`NetworkLocation` 模型、`NetworkAuth`、GVfs filesystem type 分类。
- [ ] Saved network bookmarks 和 Add Network Drive UI。
- [ ] 认证交互、取消、结构化错误报告。
- [ ] `DirectoryLister` 集成 network scan，无 pane 闪烁。
- [ ] Remote/GVfs metadata 降级（MIME、thumbnail、size、watcher）。
- [ ] Remote 位置的文件操作和 DnD 语义。

### KDE Service Menu 高级条件
- [ ] 依赖 KIO/权限上下文的 `X-KDE-*` 高级条件（如 `X-KDE-Require=`、`X-KDE-ShowIfRunning=` 等）。

### Trash 多存储聚合
- [ ] Dolphin/KIO 的 `trash:/` 多存储聚合（removable storage `.Trash-$uid`）。
- [ ] Removable storage trash 可访问性刷新。

### 交互细化
- [ ] 真实运行中 inline rename 端到端视觉验收。
- [~] 设备操作（mount/unmount/eject）的 Polkit 交互和用户取消流程仍需端到端验证。
- [ ] View Mode 下的 Icons/Details 视图切换（当前只有 Compact 主视图和 Details 列视图）。

### 双运行时对齐（COSMIC Files）

Fika 的 `operation_runtime.rs` 在 Tokio+Compio 线程边界层面对齐 COSMIC Files，
但在操作抽象层差距很大。详见 `docs/OPERATION_RUNTIME_REFERENCE.md`。

- [ ] **Phase 1.1** — 启用 `io-uring`：`Cargo.toml` 中 `compio` features 从 `polling` 切换到 `io-uring`。
- [ ] **Phase 1.2** — 引入 `OperationId(u64)`：`submit()` 返回 operation id，runtime 层获得操作级身份。
- [ ] **Phase 1.3** — 非 panic 错误路径：替换 `.expect()` 为 `Result` 传播，runtime shutdown 可被 GPUI 层优雅处理。
- [ ] **Phase 2.1** — 定义 `Operation` enum：统一 Transfer/Trash/Rename/Create/Undo/TrashView 提交路径。
- [ ] **Phase 2.2** — 添加 `OperationController`：统一的 cancel/progress/pause 状态，替换 `AtomicBool` + `Arc<Mutex<TransferProgress>>`。
- [ ] **Phase 2.3** — Runtime 级操作跟踪：`BTreeMap<OperationId, (Operation, OperationController)>` 移入 `OperationRuntime`，GPUI 层只查询不自行维护 `active_background_tasks`。
- [ ] **Phase 3.1** — 递归复制模块：`src/core/operations/recursive.rs` 使用 Compio async API 做目录遍历和文件复制。
- [ ] **Phase 3.2** — GIO fallback：GVfs 远程文件通过 `spawn_blocking` 路由 GIO `File::copy()`。

### 验证与测试
- [ ] 端到端测试：多 pane 同时访问同一目录的并发安全。
- [ ] 端到端测试：D-Bus session bus 不可用时的降级行为。
