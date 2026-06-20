# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika 是一个面向 Linux 桌面的 Rust 文件管理器 shell。当前可运行应用是 GPUI
package，围绕 UI-neutral core 和参考 Dolphin 的 directory lister/model 执行流构建。
当前活跃 UI 架构方向已经转为 Linux-only、Fika 专用的 `winit + wgpu` shell；
GPUI 应用在新 shell 被证明之前保留为兼容实现和行为/性能基线。

新 shell 将使用 iced/COSMIC windowing 路径（本地 COSMIC 参考使用的
`pop-os/winit`）和 `wgpu`，但不采用 libcosmic/iced 作为通用 widget tree。
当前二进制和 fallback 路径仍继续使用来自 Zed 仓库的 GPUI。

> [English version](README.md)

## 当前范围

当前切换后的构建包含：

### Core

- UI-neutral directory lister 和 model（`fika-core`，`src/core/`）。
- 后台 listing worker：按 pane 合并请求，fresh/stale 缓存，可取消 `read_dir`。
- 目录缓存：LRU 淘汰，条目预算，分屏 pane 间共享 `Arc<Vec<Entry>>`。
- Pane identity、pane state、split/close 路由（通过稳定 `PaneId`）。
- Pane-local selection，方向键导航，Shift 范围选择，Ctrl/secondary toggle，
  rubber-band 框选。
- 当前目录删除时跳转到最近仍存在的上级目录。
- Navigation history（Back/Forward），按 `PaneId` 隔离。
- Compact file-view 布局：列优先，按列宽度缓存，可见范围投影，hit-test，
  viewport 数学。
- Core 层平滑滚动缓动、惯性跟踪器、retarget 和 scroll clamp。
- Zoom level 映射（Dolphin 风格 0–16 → 16–256 px icon size）。
- 文件操作 primitives：copy、move、link、trash、create、rename、undo。
- Privileged operation API surface（受保护文件系统操作）。
- 路径解析：`~` 展开，绝对/相对路径，breadcrumb segments，文件系统 Tab 补全。
- MIME 类型检测（shared-mime-info globs、后缀、magic bytes）。
  文件图标通过 Dolphin-aligned 扩展名降级链解析
  (`text-x-{ext}` → `text-x-generic` → `unknown`)，实现无闪烁首帧显示。
- 应用启动器：`.desktop` 解析，`mimeapps.list` Default/Added/Removed 关联，
  `XDG_DATA_DIRS` 应用缓存，systemd user transient unit 启动。
- KDE service-menu 解析：`X-KDE-ServiceTypes`、
  `X-KDE-Priority/TopLevel`、`X-KDE-Submenu`，协议和 URL 数量条件。
- Ark 压缩文件集成：分类器，session bus DnD 解压，Compress/Extract
  fallback 命令。
- 剪贴板模型：URI-list 编解码，GPUI `ClipboardItem` 往返，
  primary/clipboard 选择导入，文本粘贴创建。
- 过滤模型：plain-text 和 glob 名匹配，过滤后模型投影，缓存失效。
- 回收站：`$XDG_DATA_HOME/Trash` metadata 读取，还原，永久删除，清空，
  按删除时间排序。
- 缩略图：freedesktop 缩略图 URI，缓存键，缓存命中，失败标记，
  `EntryData` path role。首帧显示使用同步 freedesktop 缓存探测；
  Dolphin 风格的可见优先调度，支持预读。
- GIO/GVfs 设备发现：mount/volume monitor 快照、可移动设备动态
  section、mount/unmount/eject 操作。
- Network/GVfs 远程文件系统分类和 Places Network root。
- D-Bus bus controller：session/system 连接缓存，超时重试 helper，
  owned proxy 创建，结构化 `BusError`。
- COSMIC-style 操作运行时：Tokio multi-thread context 加 dedicated Compio
  操作线程，使用 bounded task submission，并通过 Compio blocking fallback
  承接同步文件操作片段。`OperationId` 身份标识，`Operation` 枚举
  (Transfer/Trash/Rename/Create/Undo)，`OperationController` 支持
  cancel/pause/progress，运行时级 `BTreeMap<OperationId, OperationHandle>`
  跟踪。

### 当前 UI (GPUI 基线)

- Manager 窗口：目录 pane、pane 外壳、toolbar、header。
- 动态分屏（通过快捷键 Split / Close Pane）。
- Pane-local 地址栏：breadcrumb 模式和可编辑文本模式（带光标、水平滚动、
  Tab 补全）。
- Pane-local 状态栏：选中摘要，可用空间信息，zoom slider，以及目录加载
  进度和 Stop。
- 侧栏后台任务面板：active file operations、per-task Stop、最近历史和
  progress，放在 Places 侧栏底部。
- Pane-local 过滤栏：plain-text/glob 切换，大小写切换，匹配计数，关闭按钮。
- Places 侧栏：Home、XDG user dirs、Trash、removable devices、Root、
  Network；用户 bookmark 持久化（`user-places.xbel`）；右键菜单（Open、
  Open in New Pane、Add、Edit、Remove、Copy Location、Properties、
  Empty Trash）；圆角样式和主题图标。
- Compact file grid：可见条目虚拟化，slot-pool 元素复用（上限 100），
  GPU 合成滚动平移。
- 横向滚动条：live canvas bounds，paint-phase capture-move 跟踪，
  reserve-area measured-track fallback，handle-grab 偏移保持。
- Rubber-band 框选：viewport-local 投影，drag clamp，排除滚动条/pane-chrome
  区域。
- 右键菜单：target/action/item/icon model；root、submenu 和 nested
  submenu 渲染；service-menu 分组；Open With 动态子菜单；Ark fallback
  分组；viewport clamp/flip 定位。
- Open With "Other Application…" 选择器：`uniform_list` 虚拟列表，
  可见图标范围，Set Default 写回 `mimeapps.list`。
- 拖放：item/place drag source，directory/item/blank/pane drop target，
  `.desktop` 应用 DnD，通过 GPUI `ExternalPaths` 接收外部文件拖入，
  Copy/Move/Link drop menu 和 hover 反馈，Places bookmark 插入和重排。
- Inline rename：pane-local draft 状态，文本输入，Enter/Escape 提交/取消。
- Properties 对话框：单路径和多选 metadata 行。
- 剪贴板交互：内部 Copy/Cut/Paste 通过后台任务面板展示进度并支持 undo；
  中键 primary-selection 粘贴。
- Chooser shell：文件/目录选择，多选，filter/choice/portal metadata 输出。
- 键盘快捷键：pane-scoped 导航、选择、缩放、过滤、剪贴板、undo 和文本输入
  分类。

### 二进制与集成

- `fika` — 当前 GPUI 应用和 chooser shell。
- `fika-xdp-filechooser` — XDG Desktop Portal FileChooser 后端。
- `fika-privileged-helper` — 受保护操作的系统总线 helper。
- `data/` 下 D-Bus service 文件、Polkit policy 和 portal metadata。

旧 UI 实现已经从主代码树移除。新的 UI runtime 工作应面向 winit/wgpu shell 路线图，
而不是继续把 GPUI 扩展为长期 framework 依赖。

## 布局

```text
src/
  lib.rs                         UI-neutral core 模块导出
  main.rs                        当前 GPUI 应用和 chooser shell
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
    entries.rs                   文件条目 metadata 和排序输入
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
      search.rs                  搜索框光标、hit-test 和输入
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
      state.rs                   DnD 状态、路径归一化、target 匹配
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
    item_view.rs                 Item-view scroll ownership
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

根 manifest 是单一 Cargo package。它从 `src/lib.rs`（通过 `src/core.rs`）
暴露 `fika_core` library，并从 `src/main.rs` 和 `src/bin/` 构建 `fika`、
`fika-xdp-filechooser` 和 `fika-privileged-helper` 二进制。

## 构建

前置条件：

- 支持 Rust 2024 edition 的工具链。
- GPUI、GIO/GVfs 和 zbus 所需的 Linux 桌面开发库。
- Cargo 首次获取 Zed 仓库依赖时需要网络访问。

构建和运行：

```sh
cargo build
cargo run -- /path/to/start
```

运行 chooser shell：

```sh
cargo run -- --chooser ~/Downloads
cargo run -- --chooser-directory --chooser-multiple ~/Downloads
```

运行检查：

```sh
cargo fmt --all
cargo test
cargo check
```

## CLI

```text
fika [options] [start-directory]
```

| 选项 | 说明 |
| --- | --- |
| `--chooser` | 以文件选择器模式启动。 |
| `--chooser-directory` | 选择目录而不是文件。 |
| `--chooser-multiple` | 确认前选择多个路径。 |
| `--chooser-title <text>` | 设置 chooser 窗口标题。 |
| `--chooser-accept-label <text>` | 设置 chooser 动作标签。 |
| `--chooser-filter-index <n>` | 将 `n` 作为选中过滤器元数据返回。 |
| `--chooser-return-filter` | 在路径前输出过滤器元数据。 |
| `--chooser-choices <list>` | 保留 portal choice 元数据。 |
| `--chooser-return-choices` | 在路径前输出 choice 元数据。 |
| `--chooser-parent-window <handle>` | 接受 portal 父窗口参数。 |
| `-h`, `--help` | 输出帮助。 |

chooser 会把路径输出到 stdout。按需输出的元数据行位于路径之前，前缀为
`FIKA_CHOOSER_FILTER` 和 `FIKA_CHOOSER_CHOICE`。

## 桌面集成

打包安装会把 D-Bus service、Polkit policy 和 portal metadata 与二进制一起部署。

```sh
sudo PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
scripts/check-runtime-integration.sh
```

安装 `fika.portal` 只会注册后端。要让它成为激活的 FileChooser 后端，需要通过
`xdg-desktop-portal` 配置显式启用。示例见
[docs/examples/fika-portals.conf](docs/examples/fika-portals.conf)。

## 文档

### 架构与规划

- [docs/WGPU_SHELL_ROADMAP.md](docs/WGPU_SHELL_ROADMAP.md) — 活跃 Linux-only winit/wgpu shell 目标、阶段和性能门。
- [docs/DESIGN.md](docs/DESIGN.md) — 当前 GPUI/core 基线架构和子系统边界。
- [docs/TODO.md](docs/TODO.md) — 剩余实现任务和活跃阻塞项。
- [docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md](docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md) — 已归档的 slot 复用设计笔记。
- [docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md](docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md) — 已归档的滚动/缩放性能计划。
- [docs/OPTIMIZATION.md](docs/OPTIMIZATION.md) — 已归档的优化笔记。
- [docs/BUG_ANALYSIS_BLANK_DIRECTORY.md](docs/BUG_ANALYSIS_BLANK_DIRECTORY.md) — 空白目录 bug 分析。
- [docs/BUG_ANALYSIS_SCROLLBAR_DRAG.md](docs/BUG_ANALYSIS_SCROLLBAR_DRAG.md) — 滚动条拖拽回退 bug 分析。

### Dolphin / Fika 参考

- [docs/REFERENCE.md](docs/REFERENCE.md) — Dolphin 到 Fika 概念映射和工程检查项。
- [docs/LOCATION_BAR_REFERENCE.md](docs/LOCATION_BAR_REFERENCE.md) — Dolphin `KUrlNavigator` breadcrumb 和可编辑模式。
- [docs/ZOOM_REFERENCE.md](docs/ZOOM_REFERENCE.md) — Dolphin zoom level、图标尺寸映射和网格更新。
- [docs/STATUS_BAR_REFERENCE.md](docs/STATUS_BAR_REFERENCE.md) — Dolphin `DolphinStatusBar` 信息显示和 zoom slider。
- [docs/SMOOTH_SCROLL_REFERENCE.md](docs/SMOOTH_SCROLL_REFERENCE.md) — Dolphin `QScroller` 平滑/惯性滚动。
- [docs/SEARCH_REFERENCE.md](docs/SEARCH_REFERENCE.md) — Dolphin 搜索框和 KIO 搜索集成。
- [docs/ICON_THUMBNAIL_PERFORMANCE_ANALYSIS.md](docs/ICON_THUMBNAIL_PERFORMANCE_ANALYSIS.md) — 图标/缩略图加载性能分析和 Dolphin 对齐。

### 交互参考

- [docs/CONTEXT_MENU_REFERENCE.md](docs/CONTEXT_MENU_REFERENCE.md) — Dolphin 右键菜单完整执行流。
- [docs/DRAG_DROP_REFERENCE.md](docs/DRAG_DROP_REFERENCE.md) — Dolphin 拖放执行流。
- [docs/CLIPBOARD_REFERENCE.md](docs/CLIPBOARD_REFERENCE.md) — Dolphin / KIO 文件剪贴板和 GPUI 往返。

### 系统集成参考

- [docs/MIME_LAUNCHER_REFERENCE.md](docs/MIME_LAUNCHER_REFERENCE.md) — MIME 检测、应用启动、systemd。
- [docs/DEVICES_REFERENCE.md](docs/DEVICES_REFERENCE.md) — GIO/GVfs 设备发现、mount/unmount/eject。
- [docs/TRASH_REFERENCE.md](docs/TRASH_REFERENCE.md) — XDG Trash 规范和 Dolphin 回收站实现。
- [docs/THUMBNAIL_REFERENCE.md](docs/THUMBNAIL_REFERENCE.md) — Freedesktop 缩略图规范和管线。
- [docs/NETWORK_REFERENCE.md](docs/NETWORK_REFERENCE.md) — GVfs 远程文件系统分类和挂载。
- [docs/BUS_CONTROL_REFERENCE.md](docs/BUS_CONTROL_REFERENCE.md) — D-Bus 总线控制、zbus 连接、systemd/Portal 路由。
- [docs/OPERATION_RUNTIME_REFERENCE.md](docs/OPERATION_RUNTIME_REFERENCE.md) — COSMIC-style Tokio + Compio 操作运行时。
- [docs/ARK_REFERENCE.md](docs/ARK_REFERENCE.md) — Ark/kerfuffle 压缩文件集成和 D-Bus 接口。

## 许可证

[MIT](LICENSE)
