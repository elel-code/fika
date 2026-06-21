> 本文是 [DESIGN.md](DESIGN.md) 的简体中文翻译。

# Fika 设计：GPUI 基线与 winit/wgpu 方向

本文档描述当前可运行 GPUI 基线。它不再是长期 UI 架构目标。新的 shell 架构工作应遵循
`docs/WGPU_SHELL_ROADMAP.md`：基于官方 upstream `winit` master 和官方 crates.io
`wgpu` 的 Linux-focused、Fika 专用 shell。旧 SCTK spike 现在只是实验/参考代码，
不再是目标 window/event backend。

下面的边界仍然重要，因为 GPUI 应用是兼容实现和行为基线。实现边界以根 Cargo package
和 `src/` 源码目录为准；Dolphin 源码执行流仍是目录加载、刷新、model signal 和
current-directory-removed 行为的第一参考。

## 基线目标

- 保持当前 GPUI 应用可用，作为兼容 shell 和行为/性能基线。
- 保持 `fika-core` UI-neutral：core 不依赖 GPUI、窗口句柄或 UI model 类型。
- 每个 pane 都有稳定 identity：`PaneId + generation` 是 lister、watcher、async result 和 UI event 的路由边界。
- 目录变化通过 lister event 进入 `DirectoryModel`，GPUI 层只渲染 snapshot 并派发 action。
- 新 UI runtime 工作面向 winit/wgpu shell；当前二进制所需的功能修复仍可进入 GPUI 基线。
- 新增 UI 功能优先采用现代 Rust 目录式模块（`feature.rs` 入口 + `feature/*.rs` 子职责），`src/main.rs` 只保留 app 状态编排和跨模块路由。

## 非目标

- 不翻译旧 UI 文件。
- 不保留旧 slot、focused-pane fallback 或 reload queue。
- 不一次性复制 Dolphin 的所有 KDE/KIO 后端。当前主线先保住本地目录、pane identity、portal/helper 边界。
- 不在 GPUI render/input 路径中执行阻塞 I/O。
- 除非是保持当前基线可用所必需，否则不把继续 GPUI retained-renderer 工作视为活跃长期架构。

## 参考优先级

1. Dolphin 源码执行流 (`../dolphin`)。
2. Dolphin 类行为使用的 Linux 桌面规范和 service：
   XDG trash、freedesktop thumbnails、MIME apps、service menus、GIO/GVfs、Polkit。
3. 现有的 `fika-core` 模块，前提是它们保留了 Dolphin 风格的执行流。
4. `docs/WGPU_SHELL_ROADMAP.md` 中的新 UI runtime 所有权、性能门和迁移阶段。
5. 维护当前基线时使用 GPUI entity、view、state 和 input 组合惯用法。

## 源码布局

```text
src/
  lib.rs                         UI-neutral core 模块导出
  main.rs                        GPUI 应用和 chooser shell
  core.rs                        Core 模块重导出
  cli.rs                         CLI 参数解析入口
  cli/
    args.rs                      Chooser 模式 metadata 和 help 解析
  core/
    archive.rs                   Ark DnD 解压和分类
    bus.rs                       D-Bus session/system 总线控制器
    cache.rs                     目录条目缓存（LRU，共享 Arc 载荷）
    clipboard.rs                 URI-list 编解码和 GPUI 往返
    devices.rs                   GIO/GVfs 设备发现入口
    devices/
      actions.rs                 Mount/unmount/eject/safely-remove 操作
    directory.rs                 目录 lister 和 watcher 事件
    entries.rs                   文件条目 metadata 和排序
    file_ops.rs                  文件 transfer/trash/create/rename primitives
    filter.rs                    名称过滤模型（plain-text、glob）
    launcher.rs                  .desktop / mimeapps.list 应用发现
    launcher/
      ark.rs                     Ark 压缩文件 launch plan 构建
      results.rs                 Launch 结果类型
    listing_worker.rs            后台目录读取 worker
    location.rs                  路径解析、breadcrumb、Tab 补全
    metadata.rs                  条目 metadata role 解析
    mime.rs                      shared-mime-info MIME 检测
    model.rs                     目录 model snapshots 和 signals
    network.rs                   GVfs/远程文件系统分类
    operation_runtime.rs         Tokio + Compio 操作运行时 bridge
    operations.rs                操作队列和 undo 边界
    operations/
      tasks.rs                   文件操作任务结果类型
    pane.rs                      Pane identity、state、split/close 路由
    places.rs                    Places model（书签、设备、网络）
    privilege.rs                 特权操作 API surface
    thumbnails.rs                Freedesktop 缩略图 URI 和缓存键
    thumbnails/
      scheduler.rs               Dolphin 风格可见优先缩略图调度
    trash_monitor.rs             App 自管回收站空状态和 watcher
    view.rs                      Compact 布局、viewport 数学、可见范围
  ui.rs                          UI 模块重导出
  ui/
    application_chooser.rs       "Other Application…" 选择器入口
    application_chooser/
      identity.rs                应用选择器条目 identity
      matching.rs                应用去重和搜索匹配
      search.rs                  搜索框 caret、hit-test 和输入
    background_tasks.rs          侧栏后台任务面板
    chooser.rs                   文件选择器模式入口
    chooser/
      state.rs                   选择器状态和 portal metadata 输出
    clipboard.rs                 剪贴板 UI 入口
    clipboard/
      state.rs                   Copy/cut 模式和 GPUI ClipboardItem 状态
      tasks.rs                   Paste 任务结果和进度跟踪
    context_menu.rs              右键菜单 target/action/icon model
    context_menu/
      actions.rs                 Root action 生成和路由
      icons.rs                   右键菜单图标解析
      items.rs                   菜单条目构造和分组
      layout.rs                  菜单尺寸、viewport clamp 和 flip 数学
      overlay.rs                 右键菜单 overlay 渲染
      service.rs                 Service-menu action 分发
    controls.rs                  共享 UI 控件 helper
    drag_drop.rs                 拖放 UI 入口
    drag_drop/
      preview.rs                 拖放预览渲染
      state.rs                   DnD 状态、导出 payload、修饰键到模式映射、target 匹配
    file_grid.rs                 文件网格 UI 入口
    file_grid/
      details.rs                 Details-view 列布局和渲染
      layout.rs                  Compact 列宽缓存和布局组装
      projection.rs              Hit-test 投影和过滤后布局映射
      slots.rs                   可见条目 slot pool（回收元素 ID）
      snapshot.rs                可见条目 snapshot 数据和图标投影
    filter_bar.rs                过滤栏 UI 入口
    filter_bar/
      icon.rs                    过滤模式切换图标
      state.rs                   过滤 snapshot 和过滤后 model 缓存
    icons.rs                     文件/命名图标入口
    icons/
      cache.rs                   FileIconCache、MIME 候选、主题解析
      view.rs                    缓存主题图标渲染 helper
    item_view.rs                 Item-view scroll 所有权
    item_view/
      scroll_bar.rs              Pane-decoupled tracked 横向滚动条
      scroll_state.rs            Per-pane ScrollHandle 映射和 view/handle 同步
    location_bar.rs              地址栏 UI 入口
    location_bar/
      draft.rs                   可编辑地址栏 draft 和 caret 状态
      metrics.rs                 可编辑 metrics、hit-test、滚动数学
    pane.rs                      Pane 外壳 UI 入口
    pane/
      snapshot.rs                Pane 渲染 snapshot
      sort.rs                    Pane sort-status 格式化
      splitter.rs                Splitter drag payload 和比例几何
      toolbar.rs                 Pane header Search/Close、Split、Close Pane 按钮
    place_draft.rs               Places Add/Edit draft 入口
    place_draft/
      overlay.rs                 Draft 对话框和字段渲染
      state.rs                   Draft 状态、字段切换和文本输入
    places.rs                    Places 侧栏 UI 入口
    places/
      devices.rs                 可移动设备 section 替换和排序
      drag.rs                    PlaceDrag payload、预览、drop-zone 数学
      icon_view.rs               Place 图标渲染和降级分类
      model.rs                   Place 条目、分组和图标 snapshots
      projection.rs              Place row snapshot 投影和状态映射
      snapshot.rs                Place 图标和 snapshot 类型
      sidebar.rs                 Places 面板布局和后台任务 slot
      sidebar/
        row.rs                   Place row 视觉结构、点击和右键菜单
        section.rs               Section header 视觉结构和右键菜单
      style.rs                   Row/drop-target/insert-indicator 颜色 helper
      user.rs                    用户书签入口
      user/
        dropped.rs               拖入文件夹添加验证
        edit.rs                  Add/Edit draft 提交和去重
        entry.rs                 用户书签 PlaceEntry 构造
        ordering.rs              插入索引、插入和重排
        persistence.rs           XBEL 持久化投影
        removal.rs               删除结果和可移除门
      visibility.rs              隐藏 place/section 状态过滤
    properties_dialog.rs         Properties 对话框入口
    properties_dialog/
      metadata.rs                文件 metadata 读取和行生成
    rename.rs                    Inline rename 入口
    rename/
      draft.rs                   Pane-local rename draft 状态和 caret
      metrics.rs                 Rename caret hit-test 和文本内缩 metrics
    rubber_band.rs               Rubber-band 框选入口
    rubber_band/
      state.rs                   Rubber-band drag payload 和 rect 投影
    shortcuts.rs                 键盘快捷键分类
    status_bar.rs                状态栏 UI 入口
    status_bar/
      progress.rs                操作进度/busy 视图和 Stop 路由
      space.rs                   文件系统空间信息视图和使用量颜色
      state.rs                   Snapshot、空间信息缓存、进度句柄
      summary.rs                 Pane selection/model 摘要格式化
      zoom.rs                    Zoom track/segment 渲染和 drag 更新
    trash_conflict.rs            回收站还原冲突对话框
  bin/
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser 后端
    fika-privileged-helper.rs    系统总线特权 helper
```

根 `Cargo.toml` 是单一 package。它从 `src/lib.rs`（通过 `src/core.rs`）暴露 `fika_core` library，并从 `src/main.rs` 和 `src/bin/` 构建 `fika`、`fika-xdp-filechooser` 和 `fika-privileged-helper` 二进制。GPUI 来自 Zed 官方仓库，使用 git 依赖，没有数字 crate 发布版本固定。

## Core Model

### Pane

`PaneState` 是 core 对象，不是 UI slot。它拥有：

- `PaneId`
- `generation`
- `current_dir`
- `DirectoryModel`
- `DirectoryLister`
- watcher state

打开或关闭分屏 pane 会创建或丢弃 pane state。它不能克隆全局 UI state 或共享 watcher state。

### Directory Lister

lister 镜像 Dolphin 的 `KDirLister -> KFileItemModel` 边界。

输入：

- load directory
- reload current directory
- watcher refresh
- current-directory-removed detection

输出：

- `LoadingStarted`
- `ItemsAdded`
- `ItemsDeleted`
- `ItemsRefreshed`
- `ListingCompleted`
- `CurrentDirectoryRemoved`
- `Error`

所有输出都携带 `PaneId`、`generation` 和路径上下文，以便拒绝过时事件。

### Directory Model

`DirectoryModel` 拥有条目并发出 model signals：

- 在 `LoadingStarted` 时保持上一个列表可见
- 仅当当前请求交付新的 `ListingRefreshed` 时才重置/替换
- 插入条目范围
- 删除条目范围
- 刷新条目范围
- 报告加载/错误状态

GPUI pane 消费 snapshots 和 signals。它不判断文件系统事件是 add、delete、refresh 还是 full reload。导航期间它会取消瞬时交互，但在新列表就绪之前保留旧 model/layout，匹配 Dolphin 的无空白帧加载行为。

路径和条目 ID 查找使用 Dolphin 风格的惰性块索引。仅限 role 的更新（如 thumbnail/MIME 解析，或排序后条目 identity 未变的重载）在不重建这些索引的情况下更新 metadata。

### Listing Worker 和 Cache

`ListingWorkerState` 是按 app 全局的单例，接收以 `(path, mode)` 为键的列表请求。显示同一目录的多个 pane 的请求被合并到单个 `read_dir` 中。结果以 `Arc<Vec<Entry>>` 共享，并以 pane-local `ModelEntry` identity 重新定向到每个请求 pane。

`DirectoryCache` 仅存储以规范路径为键的新鲜列表结果。在 `Load` 请求时，缓存返回缓存的 `ListingRefreshed + ListingCompleted` 对，无需排队后台 `read_dir`。条目通过 LRU 淘汰，有按目录和总条目预算。`Reload`、watcher 失效和目录指纹不匹配会立即丢弃缓存载荷，而不是保留过时条目。

超出缓存条目预算的大型目录仅以轻量级路径/计数摘要跟踪。它们从不保留 `Entry` 载荷，但可通过 listing-worker debug snapshot 可见。`FIKA_DEBUG_CACHE=1`、`FIKA_DEBUG_NAV=1` 或 `FIKA_PERF_ITEM_VIEW=1` 在应用运行时打印缓存命中/未命中、失效、淘汰、待处理工作、已缓存条目和跳过大目录统计信息。

### 路径解析

`src/core/location.rs` 提供由启动参数解析器、地址栏输入提交、Places Add/Edit 和 Tab 补全使用的路径规范化：

- `expand_user_path()` — `~` 展开
- `normalize_start_dir()` — 绝对路径解析，以 home 为 fallback
- `resolve_location_input()` — 绝对、相对和 `~` 输入
- `complete_location_input()` — 文件系统 Tab 补全
- `breadcrumb_segments()` — breadcrumb segment model
- `home_dir()` — `$HOME` 查找

### MIME 检测

`src/core/mime.rs` 读取系统 shared-mime-info 数据库（`globs2`、`icons`、`generic-icons`）并提供：

- 字面文件名匹配（最高优先级）
- 多后缀匹配
- 仅扩展名匹配
- 通用 magic-byte 嗅探（仅作为 `application/octet-stream` 的 fallback）
- MIME 特定图标和通用图标查找

### 应用启动器

`src/core/launcher.rs` 解析 `.desktop` 文件（`Desktop Entry`、`Desktop Action`、`MimeType=`、`Exec` 字段代码）和 `mimeapps.list`（Default、Added、Removed Associations）。Open With 应用列表按 `mimeapps.list` 优先级排序，过滤掉已移除的关联。`.desktop` `MimeType=` 通配符（例如 `image/*`）在父 MIME fallback 之前被识别。应用启动计划通过 session bus `StartTransientUnit()` 以 systemd user transient unit 执行。

### Service Menu

`src/core/launcher.rs` 还扫描专用的 KDE/Fika service-menu 目录，并解析 `Type=Service` desktop 文件及其 `X-KDE-ServiceTypes=`。条件包括 `X-KDE-Protocols`、`X-KDE-RequiredNumberOfUrls`、`X-KDE-ShowIfExecutable`、`X-KDE-Priority=TopLevel` 和 `X-KDE-Submenu`，均在 core 中评估。TopLevel action 提升到根右键菜单；`X-KDE-Submenu` action 在 "More Actions" 下渲染为嵌套子菜单。

### 回收站

`src/core/file_ops.rs` 实现 XDG Trash：

- `trash_file()` — 唯一回收站名称、带有 `Path=` 和 `DeletionDate=` 的 `.trashinfo`、移动到 `files/`
- `restore_file()` — 读取 `.trashinfo`、移回、覆盖冲突对话框
- `delete_permanently()` — 删除 `files/` 条目和对应的 `info/`
- `empty_trash()` — 清除所有 `files/` 和孤立 `info/` 条目

回收站 model 条目携带 `trash_original_path` 和 `trash_deletion_time`，model 支持 `TrashOriginalPath` 和 `TrashDeletionTime` 排序角色。

### 缩略图

`src/core/thumbnails.rs` 实现 freedesktop 缩略图规范：

- 从文件路径和修改时间推导缩略图 URI
- 缓存键生成和缓存命中检查
- 失败标记处理
- 可见条目的 pane-local `ModelEntry.thumbnail_path` 预览 role
- Dolphin `KFileItemModelRolesUpdater::indexesToResolve()` 风格可见优先预读用于预览 role 更新
- 轻量级 thumbnail/MIME scheduler key，保持 pane/generation/item identity 和路径/MIME hash，在有界请求/结果载荷之外不保留完整路径字符串

主题文件图标通过 `FileIconCache`（`src/ui/icons/cache.rs`）按需解析。缓存以 `FileIconKind + icon_size` 和命名图标为键，从条目的 MIME 类型、扩展名和文件种类解析主题路径。图标不作为持久 role 写回 model；model role 回写路径（`ModelEntry.icon_name` 和 `src/ui/icons/roles.rs`）已被移除。可见 snapshot 构造按 Dolphin `updateVisibleIcons()` 索引顺序预热当前可见范围的图标缓存；后台/预读候选遵循 Dolphin `KFileItemModelRolesUpdater::indexesToResolve()` 顺序。文件网格渲染使用由 GPUI `RetainAllImageCache` 和 `Window::paint_image` 支持的自定义图像绘制层，因此文件网格根不再拥有 GPUI `img()` 子元素或根 `retain_all` 图像缓存提供者。

### 设备

`src/core/devices.rs` 使用 GIO/GVfs `VolumeMonitor` 作为设备后端，订阅 mount/volume add、remove 和 change signals。`src/core/devices/actions.rs` 提供 mount/unmount/eject 异步操作派发，带有进度、成功和错误消息。可移动设备投影到 Places 的动态 "Removable Devices" section 中，与用户书签持久化隔离。

### 网络

`src/core/network.rs` 分类文件系统类型（GVfs、remote、FUSE）并解析 Network 根路径。Places 侧栏包含一个 Network section，由活跃远程挂载填充。

### 总线控制

`src/core/bus.rs` 提供统一的 D-Bus 抽象：

- `BusKind`（Session / System）
- `BusController`：惰性连接、空闲超时（30s）、方法调用超时/重试（3 次尝试）
- 结构化 `BusError`：含 service name、method name 和错误详情
- Session 和 system bus 的 owned proxy 创建
- 路由：systemd transient units、Portal 注册、特权 helper 操作和 Ark DnD 解压

### 异步运行时架构

Fika 使用双运行时设计：

- `tokio` — 多线程运行时用于通用异步：D-Bus、进程启动、网络、watcher 回调
- `compio` — 基于 completion 的文件 I/O（Linux 上 `io_uring`，polling fallback）

运行时相互独立，不共享 futures。跨运行时数据传输使用 channel。Fika 专为 Linux 设计；`compio` 配置为 `polling` 驱动（计划：为 Linux 原生 completion I/O 启用 `io-uring`）。

## GPUI 层

GPUI shell 拥有：

- 通过 `gpui_platform::application()` 创建窗口
- pane toolbar action
- 按 `PaneId` 进行 split/close/focus 路由
- 目录条目渲染（compact file grid，带 slot-pool 虚拟化）
- 滚动条、rubber-band 和 overlay 渲染
- 地址栏（breadcrumb + 可编辑文本模式）
- 状态栏（摘要、空间信息、zoom slider、进度条）
- 过滤栏（plain-text/glob 切换、匹配计数）
- Places 侧栏（书签、设备、网络）
- 右键菜单（target/action model、Open With、service menus、嵌套子菜单）
- 拖放（条目/place 源、目录/pane 目标、外部路径）
- 剪贴板交互（Copy/Cut/Paste 带进度、primary-selection 粘贴）
- inline rename，包括 pane-local draft state 和 watcher-rename 重定向
- properties 对话框
- 应用选择器（"Other Application…"，带 `uniform_list`）
- watcher polling 交接进入 core events
- pane-local selection、导航快捷键和 manager action
- 返回受影响目录的后台文件操作任务
- chooser 路径输出和 portal metadata 输出

渲染是有意保持薄的。功能工作应先将领域逻辑移入 `fika-core`，然后通过 GPUI action 暴露。

### 关键 UI 组件

#### 文件网格 (`src/ui/file_grid/`)

文件网格现遵循 Dolphin 风格的保留管线，而非每条目 GPUI 可视树。模块外观在 `src/ui/file_grid.rs`，而实现位于 `src/ui/file_grid/` 下的专项模块中。虚拟化和渲染按保留层拆分：

1. **布局数学** (`src/core/view.rs`、`src/ui/file_grid/layout.rs`)：Compact/Icons/Details 几何从 pane `ViewState` 和 Dolphin 尺寸规则推导。
2. **原始 snapshot 和 role 调度** (`src/ui/file_grid/snapshot/`)：原始可见/工作范围一次性投影，然后 metadata、thumbnail 和 file-icon 解析工作在绘制路径之外排队。
3. **Slot 和绘制状态** (`src/ui/file_grid/slots.rs`、`src/ui/file_grid/paint_slots.rs`)：可见可视 identity 和保留条目/details 绘制器内容在仅几何变化时复用。
4. **自定义可视/图像绘制器** (`static_visual.rs`、`image_layer.rs`、`details_visual.rs`)：背景、标签、fallback 图标、主题图标和缩略图从保留 snapshot 绘制。主题图标文件不在 GPUI prepaint 中同步解码；图像解码保持在 GPUI 的 `RetainAllImageCache` 路径上，带有同源保留 fallback。
5. **交互和平台边界** (`interaction.rs`、`dnd.rs`、`item_shell.rs`、`details_shell.rs`、`rename_overlay.rs`)：hover/cursor、click/menu/drop hit testing、typed drag start 和活跃条目拖拽 hover 均走 retained hitbox/controller 路径。GPUI 条目/行 DnD shell 计数必须保持为 0；rename 仍为 GPUI 文本编辑 overlay，直到该平台契约可被替换。

活跃 inline rename draft 向 compact 列 metrics 添加 pane-local text-width override。Snapshot 生成、条目 hit-testing、rubber-band 可视交集和 rename caret 放置均消费同一扩展布局，因此长 draft 名称可以扩宽编辑器而不使鼠标几何不同步。

Inline rename 文本编辑在 `src/ui/rename/draft.rs` 中遵循常规文本字段选择语义：初始选择覆盖文件主干，纯 Left/Right 将现有选择折叠到其起点/终点，Shift+Left/Right 和 Shift+Home/End 从当前锚点扩展选择，Ctrl/Secondary+A 选择完整 draft 名称（含扩展名）。文件网格渲染器将 inline 编辑器在视觉上绑定到文件名行：仅稳定名称行接收文本字段边框/背景、选择高亮和 caret，而种类/错误/扩展名警告辅助文本保持在下方现有辅助行中。

#### Pane 滚动

之前的 `src/ui/scrollbar.rs`、`src/ui/item_view_container/*` 和 `src/core/scroll.rs` 路径已删除。当前活跃的 compact-view 滚动路径在 Fika 内复现了 Zed 的 `ScrollHandle` scrollbar model：

- `src/ui/pane.rs` 仅组合 pane chrome、文件网格和状态栏；它不携带 scrollbar drag 状态。
- `src/main.rs` 每个 `PaneId` 持有一个 `gpui::ScrollHandle`，在 pane 被移除时删除 handle，并在目录/布局重置时重置它。
- `src/ui/file_grid/viewport.rs` 使条目 viewport 成为 tracked scroll container，使用 `track_scroll()` 和 `overflow_x_scroll()`。它不再通过 `-ViewState.scroll_x` 手动平移内容 div。
- `src/ui/item_view/scroll_bar.rs` 是该 tracked viewport 的同级 overlay，而非可滚动内容的子元素。它从 `ScrollHandle::bounds()`、`max_offset()` 和 `offset()` 计算 thumb 几何，然后使用与 Zed 相同的负偏移约定通过 `ScrollHandle::set_offset()` 写回 drag/track-click 变化。
- 滚轮输入使用与 scrollbar drag 相同的 pane-local offset 路径。Compact 视图将滚轮输入映射到 Dolphin 的横向滚动方向；icons/details 将滚轮输入映射为垂直方向。当前代码中没有活跃的 smooth-scroller tick 任务或 kinetic gesture state。

滚轮输入首先按视图模式映射：compact 视图使用 Dolphin 的横向方向，icons/details 使用垂直滚动。Ctrl/secondary 滚轮保持 pane-local zoom。Scrollbar thumb/track 交互保持直接，不启动 kinetic release。

#### 地址栏 (`src/ui/location_bar.rs`)

两种模式：

- **Breadcrumb 模式**：使用 GPUI 声明式 `div` 元素渲染；每个 segment 可点击进行导航。
- **可编辑模式**：使用 `canvas()` 进行文本渲染，带 caret 和横向滚动；Tab 补全通过 core `complete_location_input()` 查询文件系统。

#### 状态栏 (`src/ui/status_bar.rs`)

每 pane 状态栏显示：

- 选择摘要（条目数量和总大小）
- 当前目录文件系统的可用空间
- Zoom slider（可拖拽横向轨道）
- 带 Stop 按钮的进度条（用于文件操作和目录加载）

#### Places 侧栏 (`src/ui/places.rs`)

Sections：Home、XDG user dirs、Trash、Removable Devices、Root、Network。

- 用户书签持久化到 Fika 自己的 `$XDG_DATA_HOME/fika/places.xbel`。
- Device sections 由 GIO/GVfs volume-monitor signals 动态填充。
- 右键菜单支持 Open、Open in New Pane、Add/Edit/Remove bookmark、Copy Location、Properties 和 Empty Trash。
- 拖放：从 Places 拖到 pane 会导航；将路径列表拖到 Places 会插入书签或执行文件操作；在侧栏内拖动 Places 仅重排 Places 条目。

#### 右键菜单 (`src/ui/context_menu.rs`)

生成 Dolphin 风格右键菜单，包含：

- Root actions（Open、Open in New Pane、Cut/Copy/Paste、Rename、Move to Trash、Delete Permanently、Properties、Compress/Extract）
- Create New 子菜单
- Open With 动态子菜单（按 `mimeapps.list` 优先级排序，带 "Other Application…" 选择器）
- Service-menu actions（来自 KDE/Fika service 目录）
- Sort By 子菜单（含回收站专用角色）
- 带 viewport clamp 和 flip 的菜单定位

#### 拖放 (`src/ui/drag_drop.rs`)

支持：

- 内部条目拖拽（pane 到 pane、pane 到 Places、Places 到 pane）
- 准备好的外部条目/place drag payloads（`text/uri-list` 和 `text/plain`）
- 外部文件拖入（`ExternalPaths`）
- 修饰键模式切换（无修饰键 = Copy、Shift = Move、Shift+Ctrl = Link）
- 颜色编码 drop target（Copy 绿色、Move 琥珀色、Link 紫色）
- Places 书签重排的插入指示器

## 异步和过时结果策略

每个 pane-scoped 异步结果必须包含：

- `PaneId`
- `generation`
- 源路径或操作 ID

应用路径：

1. 接收事件。
2. 按 `PaneId` 解析 pane。
3. 检查 generation 和路径。
4. 应用于 core model。
5. 通知 GPUI view。

任何 pane-scoped 异步结果都不能按 focused pane 应用。

## Undo 和文件操作策略

文件操作属于 core。UI action 应产生操作请求；操作完成应返回受影响目录，并触发显示这些目录的 pane 进行 lister 刷新。

Undo 遵循相同规则：先进行文件系统更改，再刷新受影响 pane，不在 UI 层手动重建 item-view。

## 历史文档

GPUI retained item-view 基线证据跟踪在
`docs/ITEM_VIEW_CUSTOM_PAINT_DESIGN.md`、
`docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md`、
`docs/ITEM_VIEW_RENDERER_DECISIONS.md` 和
`docs/ITEM_VIEW_RUNTIME_SMOKE.md` 中。

已归档的优化文档（`docs/OPTIMIZATION.md`、
`docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md`、
`docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md`）描述了早期规划阶段，
应仅为行为说明和设计历史而阅读。新的架构工作应从
`docs/WGPU_SHELL_ROADMAP.zh-CN.md` 开始。

## 验收定义

GPUI 架构在以下情况下可视为达标：

- 单 pane 和分屏 pane 正确响应外部文件系统变更进行刷新
- 关闭一个 pane 会丢弃其 lister/watcher，且不能接收过时结果
- 显示同一目录的两个 pane 具有独立的 generation 和 watcher state
- current-directory-removed 使用最近仍存在的上级目录 fallback
- portal 和 privileged-helper 二进制从根 package 构建
- 主构建不依赖已移除的 UI 实现
- 所有文件操作报告受影响目录并支持 undo
- 右键菜单、拖放、剪贴板和键盘快捷键均按 `PaneId` 路由
