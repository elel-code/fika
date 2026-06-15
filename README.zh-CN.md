# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika 是一个面向 Linux 桌面的 Rust 文件管理器 shell。当前实现是 GPUI
package，围绕 UI-neutral core 和参考 Dolphin 的 directory lister/model 执行流构建。

GPUI 依赖来自 Zed 官方仓库：`https://github.com/zed-industries/zed`。manifest
没有把 GPUI 固定到 crates.io 包发布、具体分支、具体提交或具体数字版本。

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
- Core 层平滑滚动、kinetic tracker、retarget 和 scroll clamp。
- Zoom level 映射（Dolphin 风格 0–16 → 16–256 px icon size）。
- 文件操作 primitives：copy、move、link、trash、create、rename、undo。
- Privileged operation API surface（受保护文件系统操作）。
- 路径解析：`~` 展开，绝对/相对路径，breadcrumb segments，文件系统 Tab 补全。
- MIME 类型检测（shared-mime-info globs、后缀、magic bytes）。
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
  `EntryData` path role。
- GIO/GVfs 设备发现：mount/volume monitor 快照、Removable Devices 动态
  section、mount/unmount/eject 操作。
- Network/GVfs 远程文件系统分类和 Places Network root。
- D-Bus bus controller：session/system 连接缓存，超时重试 helper，
  owned proxy 创建，结构化 `BusError`。
- COSMIC-style 操作运行时：Tokio multi-thread context 加 dedicated Compio
  操作线程，使用 bounded task submission，并通过 Compio blocking fallback
  承接同步文件操作片段。

### UI (GPUI)

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

- `fika` — 主 GPUI 应用和 chooser shell。
- `fika-xdp-filechooser` — XDG Desktop Portal FileChooser 后端。
- `fika-privileged-helper` — 受保护操作的系统总线 helper。
- `data/` 下 D-Bus service 文件、Polkit policy 和 portal metadata。

旧 UI 实现已经从主代码树移除。GPUI package 中不存在的能力都应视为后续实现任务，而不是当前功能。

## 布局

```text
src/
  lib.rs                         UI-neutral core 模块导出
  main.rs                        GPUI 应用和 chooser shell
  core.rs                        Core 模块重导出
  core/archive.rs                Ark DnD 解压和分类
  core/bus.rs                    D-Bus session/system 总线控制器
  core/cache.rs                  目录条目缓存（LRU，按 pane）
  core/clipboard.rs              URI-list 编解码和 GPUI 往返
  core/devices.rs                GIO/GVfs 设备发现入口
  core/devices/actions.rs        Mount/unmount/eject/safely-remove 操作
  core/directory.rs              目录 lister 和 watcher 事件
  core/entries.rs                文件条目 metadata 和排序
  core/file_ops.rs               文件 transfer/trash/create/rename primitives
  core/filter.rs                 名称过滤模型（plain-text、glob）
  core/launcher.rs               .desktop / mimeapps.list 应用发现
  core/launcher/ark.rs           Ark 压缩文件 launch plan 构建
  core/launcher/results.rs       Launch 结果类型
  core/listing_worker.rs         后台目录读取 worker
  core/location.rs               路径解析、breadcrumb、Tab 补全
  core/mime.rs                   shared-mime-info MIME 检测
  core/model.rs                  目录 model snapshots 和 signals
  core/network.rs                GVfs/远程文件系统分类
  core/operations.rs             操作队列和 undo 边界
  core/operations/tasks.rs       文件操作任务结果类型
  core/operation_runtime.rs       Tokio + Compio 操作运行时 bridge
  core/pane.rs                   Pane identity、state、split/close 路由
  core/places.rs                 Places model（书签、设备、网络）
  core/privilege.rs              特权操作 API surface
  core/thumbnails.rs             Freedesktop 缩略图 URI 和缓存键
  core/view.rs                   Compact 布局、viewport 数学、可见范围
  ui.rs                          UI 模块重导出
  ui/application_chooser.rs      "Other Application…" 选择器入口
  ui/application_chooser/
    identity.rs                  应用选择器条目 identity
  ui/background_tasks.rs         侧栏后台任务面板
  ui/chooser.rs                  文件选择器模式入口
  ui/chooser/state.rs            选择器状态和 portal metadata 输出
  ui/clipboard.rs                剪贴板 UI 入口
  ui/clipboard/state.rs          Copy/cut 模式和 GPUI ClipboardItem 状态
  ui/context_menu.rs             右键菜单 target/action/icon model
  ui/controls.rs                 共享 UI 控件 helper
  ui/drag_drop.rs                拖放 UI 入口
  ui/drag_drop/state.rs          DnD 状态、路径归一化、target 匹配
  ui/file_grid.rs                文件网格 UI 入口
  ui/file_grid/layout.rs         Compact 列宽缓存和布局组装
  ui/file_grid/slots.rs          可见条目 slot pool（回收 ID）
  ui/file_grid/snapshot.rs       可见条目 snapshot 数据
  ui/filter_bar.rs               过滤栏 UI 入口
  ui/filter_bar/state.rs         过滤 snapshot 和过滤后 model 缓存
  ui/icons.rs                    文件/命名图标入口
  ui/icons/cache.rs              FileIconCache、MIME 候选、主题解析
  ui/item_view.rs                item-view scroll ownership
  ui/item_view/scroll_bar.rs     tracked 横向滚动条
  ui/item_view/scroll_state.rs   per-pane scroll handle 状态
  ui/location_bar.rs             地址栏 UI 入口
  ui/location_bar/draft.rs       可编辑地址栏 draft 和 caret 状态
  ui/location_bar/metrics.rs     可编辑 metrics、hit-test、滚动数学
  ui/pane.rs                     Pane 外壳 UI 入口
  ui/pane/snapshot.rs            Pane 渲染 snapshot
  ui/pane/splitter.rs            Splitter drag payload 和比例几何
  ui/place_draft.rs              Places Add/Edit draft 状态
  ui/places.rs                   Places 侧栏 UI 入口
  ui/places/sidebar.rs           Places 面板布局和后台任务 slot
  ui/places/model.rs             Place 条目、分组、图标 snapshot
  ui/places/snapshot.rs          Place 图标和 snapshot 类型
  ui/properties_dialog.rs        Properties 对话框入口
  ui/properties_dialog/
    metadata.rs                  文件 metadata 读取和行生成
  ui/rename.rs                   Inline rename 入口
  ui/rename/draft.rs             Pane-local rename draft 状态
  ui/rubber_band.rs              Rubber-band 框选入口
  ui/rubber_band/state.rs        Rubber-band drag payload 和 rect 投影
  ui/shortcuts.rs                键盘快捷键分类
  ui/status_bar.rs               状态栏 UI 入口
  ui/status_bar/state.rs         Snapshot、空间信息缓存、进度句柄
  ui/status_bar/summary.rs       Pane selection/model 摘要格式化
  src/bin/
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

- [docs/DESIGN.md](docs/DESIGN.md) — 当前 GPUI/core 架构和子系统边界。
- [docs/TODO.md](docs/TODO.md) — 剩余实现任务和活跃阻塞项。
- [docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md) — 从旧 UI 到 GPUI 的原始切换计划。
- [docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md](docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md) — 已归档的 slot 复用设计笔记。
- [docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md](docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md) — 已归档的滚动/缩放性能计划。
- [docs/OPTIMIZATION.md](docs/OPTIMIZATION.md) — 已归档的优化笔记。
- [docs/BUG_ANALYSIS_BLANK_DIRECTORY.md](docs/BUG_ANALYSIS_BLANK_DIRECTORY.md) — 空白目录 bug 分析。

### Dolphin / Fika 参考

- [docs/REFERENCE.md](docs/REFERENCE.md) — Dolphin 到 Fika 概念映射和工程检查项。
- [docs/LOCATION_BAR_REFERENCE.md](docs/LOCATION_BAR_REFERENCE.md) — Dolphin `KUrlNavigator` breadcrumb 和可编辑模式。
- [docs/ZOOM_REFERENCE.md](docs/ZOOM_REFERENCE.md) — Dolphin zoom level、图标尺寸映射和网格更新。
- [docs/STATUS_BAR_REFERENCE.md](docs/STATUS_BAR_REFERENCE.md) — Dolphin `DolphinStatusBar` 信息显示和 zoom slider。
- [docs/SMOOTH_SCROLL_REFERENCE.md](docs/SMOOTH_SCROLL_REFERENCE.md) — Dolphin `QScroller` 平滑/惯性滚动。
- [docs/SEARCH_REFERENCE.md](docs/SEARCH_REFERENCE.md) — Dolphin 搜索框和 KIO 搜索集成。

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
