> 本文是 [REFERENCE.md](REFERENCE.md) 的简体中文翻译。

# Fika 参考索引

本文档是当前 GPUI package 的参考索引。实现前先查 Dolphin 源码和本仓库 core 边界，不使用记忆或猜测替代源码。

## 首要参考：Dolphin

本地 Dolphin 源码在 `../dolphin`。

### 目录加载与刷新

- `../dolphin/src/views/dolphinview.cpp:2337`
  - `DolphinView::loadDirectory(const QUrl &url, bool reload)`
  - `reload == true` 调用 `m_model->refreshDirectory(url)`
  - `reload == false` 调用 `m_model->loadDirectory(url)`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:349`
  - `KFileItemModel::loadDirectory()` 调用 `m_dirLister->openUrl(url)`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:354`
  - `KFileItemModel::refreshDirectory()` 调用 `m_dirLister->openUrl(url, KDirLister::Reload)`
  - 发出 `directoryRefreshing()`

### KDirLister 到 Model 信号

- `../dolphin/src/kitemviews/kfileitemmodel.cpp:300`
  - `itemsAdded -> slotItemsAdded`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:301`
  - `itemsDeleted -> slotItemsDeleted`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:302`
  - `refreshItems -> slotRefreshItems`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:308`
  - `listingDirCompleted -> slotCompleted`

### Model Slots

- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1359`
  - `slotCompleted()` — 分发待定插入条目，展开待定目录，发出加载完成
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1399`
  - `slotItemsAdded()` — 创建条目数据，处理过滤，排队待定插入，发出变更的父目录
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1506`
  - `slotItemsDeleted()` — 检测当前目录移除，删除条目范围，更新过滤条目
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1577`
  - `slotRefreshItems()` — 更新旧/新条目对，保留展开 metadata，发出变更的条目范围

### 视图消费 Model 信号

- `../dolphin/src/kitemviews/kitemlistview.cpp:1812`
  - `KItemListView::setModel()` — 将 model 信号连接到 `slotItemsChanged`、
    `slotItemsInserted`、`slotItemsRemoved`、`slotItemsMoved`、分组/排序变更

### 当前目录被删除

- `../dolphin/src/dolphinviewcontainer.cpp:1088`
  - `DolphinViewContainer::slotCurrentDirectoryRemoved()` — 本地路径移至最近存在的上级目录并显示警告

## 当前 Fika 概念映射

| Dolphin 概念 | Fika 模块 |
| --- | --- |
| `DolphinView` | `src/ui/pane.rs`（pane 外壳） |
| `DolphinViewContainer` | `src/main.rs`（app-state 路由） |
| `DolphinStatusBar` | `src/ui/status_bar.rs` |
| `DolphinUrlNavigator` / `KUrlNavigator` | `src/ui/location_bar.rs` |
| `KDirLister` | `src/core/directory.rs` |
| `KFileItemModel` | `src/core/model.rs` |
| `KItemListView` | 布局在 `src/core/view.rs` / `src/ui/file_grid/layout.rs`，保留渲染在 `src/ui/file_grid/` |
| `KItemListSmoothScroller` | 记录于 `docs/SMOOTH_SCROLL_REFERENCE.md`；已从活动代码移除 |
| `KDirectoryListerCache` | `src/core/cache.rs` |
| `KItemListCreatorBase`（slot 复用） | `src/ui/file_grid/slots.rs` 和 `src/ui/file_grid/paint_slots.rs` |
| `KItemListSizeHintResolver`（列宽） | `src/ui/file_grid/layout.rs` |
| pane 身份 / 分屏 | `src/core/pane.rs` |
| 导航历史 | `src/core/pane.rs` |
| `KFileItemActions` / `DolphinContextMenu` | `src/ui/context_menu.rs` |
| `KFilePlacesModel` / `PlacesPanel` | `src/core/places.rs` → `src/ui/places.rs` |
| 拖拽源 / 放置目标 | `src/ui/drag_drop.rs` |
| rubber-band 框选 | `src/ui/rubber_band.rs` |
| 搜索 / 过滤 | `src/core/filter.rs` → `src/ui/filter_bar.rs` |
| 缩放（`DolphinView::setZoomLevel`） | `src/core/pane.rs`（ViewState），`src/ui/status_bar.rs`（滑块） |
| 文件操作 primitives | `src/core/file_ops.rs` |
| undo（`KIO::undo`） | `src/core/operations.rs` |
| 回收站（`TrashBase`、`DolphinTrash`） | `src/core/file_ops.rs`（回收站 primitives） |
| MIME 检测 / `KMimeTypeResolver` | `src/core/mime.rs` |
| Open With / `mimeapps.list` | `src/core/launcher.rs` |
| KDE service menus | `src/core/launcher.rs`（service-menu 解析器） |
| Ark / `kerfuffle` | `src/core/archive.rs` + `src/core/launcher/ark.rs` |
| 应用启动 / `KProcessRunner` | `src/core/launcher.rs`（systemd transient units） |
| 剪贴板（`KIO::paste`） | `src/core/clipboard.rs` → `src/ui/clipboard.rs` |
| `KFileItemModelRolesUpdater`（metadata/icon/thumbnail roles） | `src/ui/file_grid/snapshot.rs`，`src/ui/icons/cache.rs`，`src/core/thumbnails.rs` |
| GIO/GVfs 设备 / `Solid::Device` | `src/core/devices.rs` → `src/ui/places.rs` |
| 网络 / `KFilePlacesModel` remote | `src/core/network.rs` → `src/ui/places.rs` |
| D-Bus / `KDirNotify` / `FileManager1` | `src/core/bus.rs` |
| inline rename（`DolphinView::renameSelectedItems`） | `src/ui/rename.rs` |
| 特权操作 API | `src/core/privilege.rs` |
| portal FileChooser 后端 | `src/bin/fika-xdp-filechooser.rs` |
| 系统总线 helper | `src/bin/fika-privileged-helper.rs` |
| listing worker | `src/core/listing_worker.rs` |
| 应用内 chooser shell | `src/ui/chooser.rs` |
| 路径解析 / breadcrumb | `src/core/location.rs` |
| properties 对话框 | `src/ui/properties_dialog.rs` |
| application chooser | `src/ui/application_chooser.rs` |
| 图标缓存 / 主题解析 | `src/ui/icons.rs` + `src/ui/icons/cache.rs` |
| 键盘快捷键分类 | `src/ui/shortcuts.rs` |
| 条目 metadata role 解析 | `src/core/metadata.rs` |
| 操作运行时（Tokio + Compio） | `src/core/operation_runtime.rs` |
| 回收站空状态监视器 | `src/core/trash_monitor.rs` |
| 缩略图调度器 | `src/core/thumbnails/scheduler.rs` |
| 后台任务面板 | `src/ui/background_tasks.rs` |
| CLI 参数解析 | `src/cli.rs` + `src/cli/args.rs` |
| 回收站冲突对话框 | `src/ui/trash_conflict.rs` |
| details-view 列 | `src/ui/file_grid/details.rs` |
| file-grid hit-test 投影 | `src/ui/file_grid/projection.rs` 和保留交互在 `src/ui/file_grid/interaction.rs` |

## Cargo 边界

- 根 `Cargo.toml` 是单一 Cargo package。
- `src/lib.rs` 作为 `fika_core` library 通过 `src/core.rs` 暴露，无 GPUI 依赖。
- `src/main.rs` 包含 `fika` 二进制源码。
- GPUI 依赖从 `https://github.com/zed-industries/zed` 通过 `git` 包依赖获取。
- 不将任何 GPUI 依赖固定到具体的 crate release、branch 或 revision。
- 直接 crates.io 依赖使用宽 semver 范围（如 `tokio = "1"`，`zbus = "5"`，`notify = "8"`）。

## 工程检查项

实现文件视图任务前：

1. 找到行为的 Dolphin 源码路径。
2. 将行为状态放入 `fika-core`，除非纯粹是视觉层面。
3. 确保 pane-scoped 异步结果携带 `PaneId + generation`。
4. 为共享行为添加 stale-result 和 split-pane 覆盖。
5. 在 core 边界稳定后才接入 GPUI 渲染或输入。

添加新 UI 功能前：

1. 将功能映射到对应的 Dolphin 层（render / model / interaction）。
2. 将新模块放入 `src/core/`（领域逻辑）或 `src/ui/`（渲染）。
3. 对具有多个内部职责的功能优先使用目录模块（`feature.rs` + `feature/*.rs`）。
4. 不要向 `src/main.rs` 添加大型行为块。
5. 在实现前将 Dolphin 参考路径写入 `docs/*_REFERENCE.md` 文档。

## 参考文档目录

### 架构与规划
- [DESIGN.md](DESIGN.md) — 当前 GPUI/core 架构
- [TODO.md](TODO.md) — 剩余任务板
- [ITEM_VIEW_CUSTOM_PAINT_DESIGN.md](ITEM_VIEW_CUSTOM_PAINT_DESIGN.md) — 活跃的保留 item-view 架构
- [ITEM_VIEW_CUSTOM_PAINT_TODO.md](ITEM_VIEW_CUSTOM_PAINT_TODO.md) — 活跃的 item-view custom-paint 任务板
- [ITEM_VIEW_RENDERER_DECISIONS.md](ITEM_VIEW_RENDERER_DECISIONS.md) — 各 surface 渲染器选择和门
- [ITEM_VIEW_RUNTIME_SMOKE.md](ITEM_VIEW_RUNTIME_SMOKE.md) — 运行时 DnD、rename 和 perf-log 冒烟检查表
- [GPUI_DOLPHIN_MIGRATION_PLAN.md](GPUI_DOLPHIN_MIGRATION_PLAN.md) — 原始切换计划
- [DOLPHIN_ITEM_SLOT_REUSE_PLAN.md](DOLPHIN_ITEM_SLOT_REUSE_PLAN.md) — 已归档的 slot 复用笔记
- [SCROLL_ZOOM_PERFORMANCE_PLAN.md](SCROLL_ZOOM_PERFORMANCE_PLAN.md) — 已归档的滚动/缩放笔记
- [OPTIMIZATION.md](OPTIMIZATION.md) — 已归档的优化笔记
- [BUG_ANALYSIS_BLANK_DIRECTORY.md](BUG_ANALYSIS_BLANK_DIRECTORY.md) — 空白目录 bug 分析
- [BUG_ANALYSIS_SCROLLBAR_DRAG.md](BUG_ANALYSIS_SCROLLBAR_DRAG.md) — 滚动条拖拽回退 bug 分析

### Dolphin / Fika 参考
- [LOCATION_BAR_REFERENCE.md](LOCATION_BAR_REFERENCE.md) — `KUrlNavigator` breadcrumb 和可编辑模式
- [ZOOM_REFERENCE.md](ZOOM_REFERENCE.md) — 缩放级别、图标尺寸映射、网格更新
- [STATUS_BAR_REFERENCE.md](STATUS_BAR_REFERENCE.md) — `DolphinStatusBar` 信息显示和缩放滑块
- [SMOOTH_SCROLL_REFERENCE.md](SMOOTH_SCROLL_REFERENCE.md) — `QScroller` 平滑/惯性滚动
- [SEARCH_REFERENCE.md](SEARCH_REFERENCE.md) — 搜索框和 KIO 搜索集成

### 交互参考
- [CONTEXT_MENU_REFERENCE.md](CONTEXT_MENU_REFERENCE.md) — 右键菜单完整执行流
- [DRAG_DROP_REFERENCE.md](DRAG_DROP_REFERENCE.md) — 拖放执行流
- [CLIPBOARD_REFERENCE.md](CLIPBOARD_REFERENCE.md) — Dolphin/KIO 文件剪贴板

### 系统集成参考
- [MIME_LAUNCHER_REFERENCE.md](MIME_LAUNCHER_REFERENCE.md) — MIME 检测和应用启动
- [DEVICES_REFERENCE.md](DEVICES_REFERENCE.md) — GIO/GVfs 设备发现和挂载操作
- [TRASH_REFERENCE.md](TRASH_REFERENCE.md) — XDG Trash 规范和 Dolphin 实现
- [THUMBNAIL_REFERENCE.md](THUMBNAIL_REFERENCE.md) — Freedesktop 缩略图规范
- [NETWORK_REFERENCE.md](NETWORK_REFERENCE.md) — GVfs 远程文件系统分类
- [BUS_CONTROL_REFERENCE.md](BUS_CONTROL_REFERENCE.md) — D-Bus 总线控制和路由
- [ARK_REFERENCE.md](ARK_REFERENCE.md) — Ark/kerfuffle 压缩文件集成
