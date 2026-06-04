# Fika 项目参考文档 / Fika Project Reference

> 本文档提供 Fika 项目各部分的详细中英文说明，涵盖架构、模块、数据流和设计决策。
> This document provides detailed Chinese and English descriptions of each part of the Fika project, covering architecture, modules, data flows, and design decisions.

---

## 目录 / Table of Contents

1. [项目概述 / Project Overview](#1-项目概述--project-overview)
2. [架构总览 / Architecture Overview](#2-架构总览--architecture-overview)
3. [构建系统 / Build System](#3-构建系统--build-system)
4. [UI 层 / UI Layer](#4-ui-层--ui-layer)
5. [状态管理层 / State Layer](#5-状态管理层--state-layer)
6. [配置系统 / Configuration System](#6-配置系统--configuration-system)
7. [文件系统模块 / Filesystem Module](#7-文件系统模块--filesystem-module)
8. [桌面集成模块 / Desktop Integration Module](#8-桌面集成模块--desktop-integration-module)
9. [应用协调层 / Application Coordination Layer](#9-应用协调层--application-coordination-layer)
10. [辅助支撑模块 / Support Module](#10-辅助支撑模块--support-module)
11. [二进制入口 / Binary Entry Points](#11-二进制入口--binary-entry-points)
12. [桌面集成数据 / Desktop Integration Data](#12-桌面集成数据--desktop-integration-data)
13. [脚本与工具 / Scripts & Tools](#13-脚本与工具--scripts--tools)
14. [数据流 / Data Flows](#14-数据流--data-flows)
15. [工程规则 / Engineering Rules](#15-工程规则--engineering-rules)

---

## 1. 项目概述 / Project Overview

### 中文

Fika 是一个使用 Rust + Slint 构建的轻量级文件管理器原型，面向现代 Wayland 桌面环境。项目当前聚焦于一个小而可用的核心功能集：

- 浏览本地目录，支持目录历史（前进/后退）和路径直接输入
- 左侧 Places 侧栏（含内置项和用户自定义项）
- 右侧主栏采用 Dolphin 风格的列优先图标视图，支持轻量虚拟化
- 底部状态栏显示操作进度、文件信息和撤销动作
- 明暗主题切换
- 防抖动的目录监控（inotify），自动刷新
- 内置 MIME 类型推断和默认应用程序启动（不依赖 `xdg-open`）
- 异步文件操作队列：复制、移动、链接、回收站、重命名、冲突处理、一步撤销
- 轻量文件选择器模式（`--chooser`），作为 `xdg-desktop-portal` FileChooser 后端
- 缩略图流水线：支持 PNG/JPEG/WebP 的异步生成，含内存和磁盘缓存（符合 freedesktop.org Thumbnail Managing Standard）
- 递归文件搜索
- 受保护文件操作通过独立 D-Bus 特权 helper 执行（Polkit 鉴权）
- 外部编辑器受保护文件编辑（临时副本 + 自动写回）
- Split View 分屏预览

### English

Fika is a lightweight file manager prototype built with Rust + Slint, targeting modern Wayland desktop environments. The current implementation focuses on a small, usable core:

- Browse local directories with history navigation (back/forward) and direct path entry
- Left sidebar with Places (built-in and user-defined entries)
- Right main pane with Dolphin-style column-first icon view and lightweight virtualization
- Bottom status bar showing operation progress, file info, and undo actions
- Light/dark theme switching
- Debounced directory monitoring (inotify) with auto-refresh
- Built-in MIME type detection and default application launching (no `xdg-open` dependency)
- Async file operation queue: copy, move, link, trash, rename, conflict handling, one-step undo
- Lightweight file chooser mode (`--chooser`), serving as an `xdg-desktop-portal` FileChooser backend
- Thumbnail pipeline: async PNG/JPEG/WebP generation with memory and disk caching (freedesktop.org Thumbnail Managing Standard)
- Recursive file search
- Protected file operations via a separate D-Bus privileged helper (Polkit authorization)
- External editor protected file editing (scratch copy + auto writeback)
- Split View pane preview

---

## 2. 架构总览 / Architecture Overview

### 中文

Fika 采用分层架构，核心层次如下：

```
┌─────────────────────────────────────────────────┐
│                  Slint UI 层                      │
│  ui/app.slint → models/widgets/menus/dialogs     │
├─────────────────────────────────────────────────┤
│              Rust 应用协调层 (main.rs)             │
│  app/ → state, pane, events, transfer, selection  │
├──────────────────┬──────────────────────────────┤
│  配置层 config/   │  桌面集成层 desktop/           │
│  args, settings  │  mime_open, clipboard, terminal│
├──────────────────┴──────────────────────────────┤
│              文件系统层 fs/                       │
│  entries, devices, file_ops, search, thumbnails  │
│  privilege (D-Bus helper)                        │
├─────────────────────────────────────────────────┤
│              支撑层 support/                      │
│  generation (过期结果丢弃)                         │
└─────────────────────────────────────────────────┘
```

- **UI 线程**：只做状态更新和轻量计算，不执行阻塞 I/O
- **异步 Tokio 运行时**：处理目录扫描、文件操作、缩略图生成、搜索和 D-Bus 通信
- **跨线程通信**：通过 `mpsc::channel` + 统一 `AsyncEvent` 枚举回到 UI 线程

### English

Fika follows a layered architecture:

```
┌─────────────────────────────────────────────────┐
│                  Slint UI Layer                   │
│  ui/app.slint → models/widgets/menus/dialogs     │
├─────────────────────────────────────────────────┤
│           Rust App Coordination (main.rs)         │
│  app/ → state, pane, events, transfer, selection  │
├──────────────────┬──────────────────────────────┤
│  config/         │  desktop/                      │
│  args, settings  │  mime_open, clipboard, terminal│
├──────────────────┴──────────────────────────────┤
│              fs/ (Filesystem Layer)               │
│  entries, devices, file_ops, search, thumbnails  │
│  privilege (D-Bus helper)                        │
├─────────────────────────────────────────────────┤
│              support/ (Utilities)                 │
│  generation (stale-result discard)                │
└─────────────────────────────────────────────────┘
```

- **UI thread**: state updates and lightweight computation only; no blocking I/O
- **Async Tokio runtime**: directory scanning, file operations, thumbnail generation, search, and D-Bus communication
- **Cross-thread communication**: `mpsc::channel` + unified `AsyncEvent` enum back to the UI thread

---

## 3. 构建系统 / Build System

### `build.rs`

**中文**：编译脚本启用 Slint 的实验性编译器注册表，使项目可以在不要求用户手动设置环境变量的情况下使用 `FlexboxLayout`。然后调用 `slint_build::compile("ui/app.slint")` 编译入口 UI 文件，生成 Rust 绑定代码。`FlexboxLayout` 仅用于局部响应式控件行（如搜索栏），主栏文件视图使用确定性的手写布局。

**English**: The build script enables Slint's experimental compiler registry so `FlexboxLayout` can be used without requiring users to manually set environment variables. It then calls `slint_build::compile("ui/app.slint")` to compile the entry UI file and generate Rust bindings. `FlexboxLayout` is only used for local responsive control rows (e.g., search bar); the main file view uses a deterministic hand-written layout.

### `Cargo.toml`

**中文**：项目为 Rust 2024 edition。关键依赖包括：

| 依赖 | 用途 |
|------|------|
| `slint` (master) | UI 框架，含 `image-default-formats` 特性 |
| `slint-build` (master) | 编译时 Slint 到 Rust 的代码生成 |
| `tokio` | 异步运行时（`fs`, `process`, `rt-multi-thread`, `sync`, `time`） |
| `zbus` 5.16 | D-Bus 客户端/服务端通信 |
| `zbus_polkit` 5.0 | Polkit 授权集成 |
| `notify` 8 | 文件系统事件监控 |
| `image` 0.25 | 图片解码（jpeg, png, webp） |
| `png` 0.18 | PNG 编码（缩略图磁盘缓存写入） |
| `futures-lite` | 轻量异步流处理 |

**English**: The project uses Rust 2024 edition. Key dependencies:

| Dependency | Purpose |
|------------|---------|
| `slint` (master) | UI framework with `image-default-formats` feature |
| `slint-build` (master) | Compile-time Slint-to-Rust code generation |
| `tokio` | Async runtime (`fs`, `process`, `rt-multi-thread`, `sync`, `time`) |
| `zbus` 5.16 | D-Bus client/server communication |
| `zbus_polkit` 5.0 | Polkit authorization integration |
| `notify` 8 | Filesystem event monitoring |
| `image` 0.25 | Image decoding (jpeg, png, webp) |
| `png` 0.18 | PNG encoding (thumbnail disk cache writing) |
| `futures-lite` | Lightweight async stream processing |

---

## 4. UI 层 / UI Layer

### 4.0 文件结构 / File Structure

```
ui/
├── app.slint           # 入口：AppWindow, FilePane, 全局 DndApi/PaneRouting
├── models.slint        # 数据模型：FileEntry, PlaceEntry, DeviceEntry, DesktopApp
├── widgets.slint       # 通用控件：按钮、菜单项、PopupSurface、Places 行
├── split_pane.slint    # 主栏 viewport、pane-level input/DnD、可见 tile primitive
├── menus.slint         # 菜单层：RootContextMenu, TransferMenu, ChildSubmenu, ChooserPopup
├── menu_geometry.slint # 菜单几何纯函数回调
├── menu_lifecycle.slint# 菜单生命周期控制器（延迟关闭 timer）
├── dnd_bridge.slint    # 拖拽类型枚举 DragKind
├── dnd_overlay.slint   # 拖拽覆盖层：ghost, 插入线, 拒绝提示
├── top_bar.slint       # 顶栏：TopBar (搜索/分屏/主题), PathBar (导航/地址)
├── status_bar.slint    # 状态栏：状态文本、外部编辑、撤销、选择器控件
├── search_panel.slint  # 搜索面板：过滤条件（类型、大小、日期）
├── split_pane.slint    # 分屏主栏视图：虚拟化 grid、选择框、view 同步
└── dialogs.slint       # 弹窗：属性、冲突处理、权限确认、文本输入
```

### 4.1 数据模型 / Data Models (`models.slint`)

**中文**：定义四个核心数据结构：

- **`FileEntry`**：文件/目录项的完整描述，包含名称、路径、类型、大小、修改时间、缩略图状态、选择状态，以及当前虚拟切片使用的 Rust item-view render-plan 字段
- **`PlaceEntry`**：侧栏位置条目，含标签、路径、图标标记、是否内置
- **`DeviceEntry`**：设备条目，含挂载状态、挂载/卸载/弹出能力、操作状态和错误信息
- **`DesktopApp`**：桌面应用程序，含 ID、显示名称和是否默认应用标记

**English**: Defines four core data structures:

- **`FileEntry`**: Complete file/directory item descriptor including name, path, kind, size, modified time, thumbnail state, selection state, and the Rust item-view render-plan fields used by the current virtual slice
- **`PlaceEntry`**: Sidebar place entry with label, path, icon marker, and built-in flag
- **`DeviceEntry`**: Device entry with mount status, mount/unmount/eject capabilities, action state, and error info
- **`DesktopApp`**: Desktop application with ID, display name, and default-app marker

### 4.2 主窗口与 Pane 架构 / Main Window & Pane Architecture (`app.slint`)

**中文**：`AppWindow` 是主窗口组件，采用 COSMIC 风格的 shell 分层模型和 slot-based pane 架构：

- 最顶层：一个共享的基础表面 + 独立的窗口级 shell/header 行
- shell 行内：`TopBar` 承载全局搜索、分屏和主题切换（无底部水平分隔线）
- shell 行下：左侧圆角侧栏面板 + 右侧主窗格区域，等高一排
- **Pane 架构**：主窗格区域采用 **slot-based** 模型——`FilePane` 是一个 100% 可复用的独立组件，通过 `PaneSlot`（继承 `FilePane`）将所有 callback 经 `PaneRouting` 全局路由。`PaneSlotSurface` 将 `PaneSlot` 封装为带 slot 编号和焦点状态的容器。Split 打开时，`pane-slot-0` 和 `pane-slot-1` 是两个完全相同的 `PaneSlotSurface` 实例，没有任何功能差异。该设计天然支持扩展到 3 个甚至更多 pane
- 每个 slot 内：`PathBar`（导航/地址栏）→ 搜索过滤条 → 文件 grid → `StatusBar`
- 菜单覆盖层通过 `RootContextMenuLayer`、`TransferMenuLayer`、`ChildSubmenuLayer` 统一挂载
- 弹窗通过 `dialogs.slint` 中定义的组件承载

**English**: `AppWindow` is the main window component, using COSMIC-style shell surface layering and slot-based pane architecture:

- Top level: one shared base surface + separate window-wide shell/header row
- Shell row: `TopBar` hosts global search, split, and theme controls (no bottom divider)
- Below shell: left rounded sidebar panel + right main pane area in one equal-height row
- **Pane architecture**: the main pane area uses a **slot-based** model — `FilePane` is a 100% reusable standalone component. `PaneSlot` (inheriting `FilePane`) routes all callbacks through the `PaneRouting` global. `PaneSlotSurface` wraps `PaneSlot` in a container with a slot number and focus state. When Split is open, `pane-slot-0` and `pane-slot-1` are two identical `PaneSlotSurface` instances with no functional differences. This design naturally supports extending to 3 or more panes
- Inside each slot: `PathBar` (nav/address) → search filter strip → file grid → `StatusBar`
- Menu overlays mounted through `RootContextMenuLayer`, `TransferMenuLayer`, `ChildSubmenuLayer`
- Dialogs hosted through components defined in `dialogs.slint`

### 4.3 虚拟化主栏视图 / Virtualized Main Grid

**中文**：主栏采用列优先布局和轻量虚拟化：

- `rows-per-column` 由可见高度和 `icon-row-height` 计算
- `x = floor(index / rows-per-column) * icon-cell-width`
- `y = mod(index, rows-per-column) * icon-row-height`
- Slint 端只接收 `entry_count`（用于空状态和滚动条宽度）和当前可见 `virtual_entries` 切片
- Rust 端通过 `VirtualGridPlan` 统一计算 clamped viewport、scroll max、可见范围和 overscan 范围
- 横向滚动只克隆需要的虚拟范围条目，避免大目录的性能问题
- 缩放和窗口尺寸变化会 clamp 横向滚动位置，避免旧 viewport 落在新内容之外

**English**: The main pane uses column-first layout and lightweight virtualization:

- `rows-per-column` computed from visible height and `icon-row-height`
- `x = floor(index / rows-per-column) * icon-cell-width`
- `y = mod(index, rows-per-column) * icon-row-height`
- Slint side receives only `entry_count` (for empty state and scrollbar) and visible `virtual_entries` slice
- Rust side uses `VirtualGridPlan` for unified clamped viewport, scroll max, visible range, and overscan
- Horizontal scrolling only clones needed virtual range entries, avoiding large-directory performance issues
- Zoom and resize clamp horizontal scroll position, preventing old viewport from landing outside new content

### 4.4 菜单系统 / Menu System

**中文**：右键菜单系统按 Dolphin/Qt 父子菜单模型模拟：

- **根菜单**：以触发点为锚点，放不下时向左/上翻转，clamp 到窗口安全边距
- **子菜单**：锚定在父菜单行，有不可见 hover bridge 连接，250ms 延迟关闭
- **菜单层**：`RootContextMenuLayer`（文件/Places/Devices/空白区）、`TransferMenuLayer`（拖放操作）、`ChildSubmenuLayer`（Open With / Create New / service-menu 分组）
- **生命周期**：`MenuLifecycleController` 拥有 delayed-close timer 和 hover/show 辅助
- **交互**：Open With 不额外显示标题行，应用列表最多 7 行纵向滚动，最后固定 `Other Applications...`
- **快捷方式提示**：菜单项支持右侧快捷键标注（仅标注已由 `KeyBinding` 处理的快捷键）

**English**: Context menu system modeled after Dolphin/Qt parent-child menu model:

- **Root menus**: anchored at trigger point, flip left/up when no space, clamp to window safe margins
- **Child menus**: anchored to parent row, connected by invisible hover bridge, 250ms delayed close
- **Menu layers**: `RootContextMenuLayer` (file/Places/Devices/blank), `TransferMenuLayer` (drag-drop), `ChildSubmenuLayer` (Open With / Create New / service-menu groups)
- **Lifecycle**: `MenuLifecycleController` owns delayed-close timer and hover/show helpers
- **Interaction**: Open With has no extra title row; app list capped at 7 visible rows with scroll; `Other Applications...` fixed at bottom
- **Shortcut hints**: menu items support right-aligned shortcut annotations (only for `KeyBinding`-handled actions)

### 4.5 拖拽系统 / Drag & Drop System

**中文**：应用内 DnD 使用 Slint master 的 `DragArea`/`DropArea` 内置组件：

- 内部 payload 通过 `data-transfer` 的 `user_data` 承载（`FikaDragInfo::Place/Folder/File`）
- 拖拽时只显示 ghost 预览和插入线，松手时一次性提交
- Places 项之间拖入主栏文件夹 → 弹出 Move/Copy/Link 菜单
- 拖到自身或自身子目录 → 拒绝并显示状态提示
- 外部跨应用 DnD 当前不支持，等待 Slint 稳定跨应用 DnD

**English**: In-app DnD uses Slint master's `DragArea`/`DropArea` built-ins:

- Internal payload carried via `data-transfer` `user_data` (`FikaDragInfo::Place/Folder/File`)
- During drag, only ghost preview and insertion line shown; commit on drop
- Drag Places items to main-pane folders → Move/Copy/Link menu
- Drop on self or descendant → rejected with status message
- External cross-app DnD not currently supported, awaiting stable Slint cross-app DnD

---

## 5. 状态管理层 / State Layer

### 5.1 `AppState` (`src/app/state.rs`)

**中文**：核心状态结构体，持有整个应用的可变状态：

| 字段 | 类型 | 用途 |
|------|------|------|
| `panes` | `PanesState` | Slot-based 多 pane 容器（每个 pane 完全对等） |
| `places` | `Vec<PlaceEntry>` | 左侧 Places 列表 |
| `devices` | `Vec<DeviceEntry>` | 侧栏设备列表 |
| `directory_cache` | `HashMap<PathBuf, Vec<FileEntry>>` | 目录条目 LRU 缓存 |
| `thumbnail_cache` | `HashMap<ThumbnailKey, ThumbnailData>` | 缩略图缓存 |
| `thumbnail_failures` | `HashMap<ThumbnailKey, String>` | 缩略图失败缓存 |
| `operation_queue` | `VecDeque<FileOperationRequest>` | 文件操作队列 |
| `clipboard_paths` | `Vec<PathBuf>` | 内部剪贴板路径 |
| `last_undo` | `Option<FileUndo>` | 最后一步撤销信息 |
| `other_application_apps` | `Vec<DesktopApp>` | Open With 其他应用列表 |
| `pending_privileged_command` | `Option<PrivilegedCommand>` | 待确认的提权操作 |

**English**: Core state struct holding the application's entire mutable state:

| Field | Type | Purpose |
|-------|------|---------|
| `panes` | `PanesState` | Slot-based multi-pane container (all panes are identical peers) |
| `places` | `Vec<PlaceEntry>` | Left sidebar Places list |
| `devices` | `Vec<DeviceEntry>` | Sidebar device list |
| `directory_cache` | `HashMap<PathBuf, Vec<FileEntry>>` | Directory entry LRU cache |
| `thumbnail_cache` | `HashMap<ThumbnailKey, ThumbnailData>` | Thumbnail cache |
| `thumbnail_failures` | `HashMap<ThumbnailKey, String>` | Thumbnail failure cache |
| `operation_queue` | `VecDeque<FileOperationRequest>` | File operation queue |
| `clipboard_paths` | `Vec<PathBuf>` | Internal clipboard paths |
| `last_undo` | `Option<FileUndo>` | Last undo info |
| `other_application_apps` | `Vec<DesktopApp>` | Open With other apps list |
| `pending_privileged_command` | `Option<PrivilegedCommand>` | Pending privileged operation |

### 5.2 `PaneState` — 完全自包含的 Pane 组件 (`src/app/pane.rs`)

**中文**：`PaneState` 是一个完全自包含的 pane 状态封装。无论有多少个 pane 实例，每个实例的结构和能力完全相同，不存在"主 pane"与"副 pane"之分：

- `id`：稳定 pane 标识（`u64`），用于异步结果路由——加载结果、操作完成事件、搜索进度都通过 pane id 回写到正确的 pane
- `current_dir`：当前目录路径
- `entries`：当前完整条目列表
- `history`：`PaneHistory`（back/forward stack，最大深度 64）
- `selection`：选择状态（支持多选、Shift+click 范围选择）
- `search`：搜索状态（query、递归搜索标志、过滤条件、进度）
- `view`：`PaneView`（viewport 位置、虚拟范围/overscan cache、缩略图 pending 状态、每目录 viewport LRU cache）
- 各 `_generation` 计数器（`load_generation`、`open_generation`、`search_generation`、`thumbnail_generation`）：每个 pane 独立持有，用于丢弃该 pane 的过期异步结果

`split_snapshot(id)` 从当前 pane 克隆目录、搜索和 viewport 快照（不复制选区和历史），用于创建新的 pane 实例。

**English**: `PaneState` is a fully self-contained pane component. Every instance has the exact same structure and capabilities, regardless of how many panes exist—there is no "primary" vs "secondary" distinction:

- `id`: stable pane identifier (`u64`) for async result routing — load results, operation completions, and search progress are routed back to the correct pane by id
- `current_dir`: current directory path
- `entries`: current full entry list
- `history`: `PaneHistory` (back/forward stack, max depth 64)
- `selection`: selection state (multi-select, Shift+click range selection)
- `search`: search state (query, recursive flag, filters, progress)
- `view`: `PaneView` (viewport position, virtual range/overscan cache, thumbnail pending state, per-directory viewport LRU cache)
- Generation counters (`load_generation`, `open_generation`, `search_generation`, `thumbnail_generation`): each pane holds its own, used to discard stale async results for that specific pane

`split_snapshot(id)` clones the directory, search, and viewport snapshot from the current pane (without selection and history), used to create a new pane instance.

### 5.3 `PanesState` — Slot-Based 多 Pane 容器 (`src/app/pane.rs`)

**中文**：

`PanesState` 是多个 `PaneState` 实例的容器。当前实现维护两个 slot（因 split view 最多两个 pane），但设计上为任意数量 pane 做好了准备：

- `active`：slot-0 对应的 `PaneState`——这是一个完全自包含的 pane，与 inactive 实例在结构上完全对等
- `inactive`：slot-1 对应的 `Option<PaneState>`，打开 Split 时从 active 的目录、搜索和 viewport 快照创建
- `focused`：`PaneSide` 枚举（`Active`/`Inactive`），标记键盘焦点所在的 slot
- `next_pane_id`：单调递增的 pane id 分配器

关键路由抽象：

- `PaneTarget::Focused`：指向当前键盘焦点所在的 pane——快捷键、菜单和 DnD 通过此路由摆脱 hard-coded active 访问
- `PaneTarget::Id(u64)`：按稳定 id 定位特定 pane——异步结果（目录加载、文件操作、设备挂载刷新）通过此路由回写
- `pane_for_target()` / `pane_mut_for_target()`：统一的目标解析入口

**English**:

`PanesState` is a container for multiple `PaneState` instances. The current implementation maintains two slots (since split view has at most two panes), but the design is ready for an arbitrary number of panes:

- `active`: slot-0's `PaneState` — a fully self-contained pane, structurally identical to the inactive instance
- `inactive`: slot-1's `Option<PaneState>`, created from the active pane's directory, search, and viewport snapshot when Split opens
- `focused`: `PaneSide` enum (`Active`/`Inactive`), marking which slot has keyboard focus
- `next_pane_id`: monotonic pane id allocator

Key routing abstractions:

- `PaneTarget::Focused`: points to the currently keyboard-focused pane — shortcuts, menus, and DnD break free of hard-coded active access through this route
- `PaneTarget::Id(u64)`: locates a specific pane by stable id — async results (directory loading, file operations, device mount refresh) are routed back through this
- `pane_for_target()` / `pane_mut_for_target()`: unified target resolution entry points

---

## 6. 配置系统 / Configuration System

### 6.1 CLI 参数 / CLI Arguments (`src/config/args.rs`)

**中文**：解析三种运行模式：

- `Mode::Manager`：默认文件管理器模式，接受可选起始目录
- `Mode::Chooser`：轻量文件选择器模式，支持 `--chooser-directory`、`--chooser-multiple`、`--chooser-save NAME`、`--chooser-save-files`、`--chooser-title`、`--chooser-accept-label`、`--chooser-filters`、`--chooser-filter-index`、`--chooser-return-filter`、`--chooser-choices`、`--chooser-return-choices`、`--chooser-parent-window`
- `Mode::DeviceDiagnostics`：`--diagnose-devices` 打印设备发现诊断信息

**English**: Parses three run modes:

- `Mode::Manager`: default file manager mode, accepts optional start directory
- `Mode::Chooser`: lightweight file chooser mode with all chooser flags
- `Mode::DeviceDiagnostics`: `--diagnose-devices` prints device discovery diagnostics

### 6.2 持久化设置 / Persistent Settings (`src/config/settings.rs`)

**中文**：基于 TSV 格式的键值对配置，存储在 `$XDG_CONFIG_HOME/fika/settings.tsv`：

| 键 | 类型 | 含义 |
|----|------|------|
| `dark_mode` | `bool` | 暗色模式 |
| `sidebar_width_px` | `f32` | 侧栏宽度（像素） |
| `split_pane_ratio` | `f32` | 分屏比例 |
| `icon_zoom_level` | `i32` | 图标缩放级别（0-4） |
| `window_width_px` | `f32` | 窗口宽度 |
| `window_height_px` | `f32` | 窗口高度 |
| `last_dir` | `PathBuf` | 上次打开的目录 |

损坏的配置值会被忽略，回退到默认值（通过测试覆盖）。

**English**: TSV-format key-value configuration stored at `$XDG_CONFIG_HOME/fika/settings.tsv`:

| Key | Type | Meaning |
|-----|------|---------|
| `dark_mode` | `bool` | Dark mode |
| `sidebar_width_px` | `f32` | Sidebar width (pixels) |
| `split_pane_ratio` | `f32` | Split pane ratio |
| `icon_zoom_level` | `i32` | Icon zoom level (0-4) |
| `window_width_px` | `f32` | Window width |
| `window_height_px` | `f32` | Window height |
| `last_dir` | `PathBuf` | Last opened directory |

Corrupt values are ignored and fall back to defaults (covered by tests).

### 6.3 路径工具 / Path Utilities (`src/config/paths.rs`)

**中文**：

- `home_dir()`：获取用户家目录
- `expand_user_path(path)`：展开 `~` 前缀
- `normalize_start_dir(path)`：规范化起始目录，确保存在且为目录

**English**:

- `home_dir()`: get user home directory
- `expand_user_path(path)`: expand `~` prefix
- `normalize_start_dir(path)`: normalize start directory, ensuring it exists and is a directory

---

## 7. 文件系统模块 / Filesystem Module

### 7.1 目录条目 / Directory Entries (`src/fs/entries.rs`)

**中文**：

- `RawFileEntry`：从文件系统读取的原始条目，包含名称、路径、类型、大小、修改时间等
- `read_entries_async(path)`：在 Tokio `spawn_blocking` 中同步扫描目录
- `read_entries_sync(path)`：直接同步读取并排序（目录优先，然后按名称排序）
- 回收站目录特殊处理：显示原始路径和删除日期，按删除日期降序排列
- `to_file_entry(raw)`：将 `RawFileEntry` 转换为 Slint 的 `FileEntry` 类型

**English**:

- `RawFileEntry`: raw entry read from filesystem with name, path, kind, size, modified time
- `read_entries_async(path)`: sync directory scan inside Tokio `spawn_blocking`
- `read_entries_sync(path)`: direct sync read with sorting (directories first, then by name)
- Trash directory special handling: displays original path and deletion date, sorted by deletion date descending
- `to_file_entry(raw)`: convert `RawFileEntry` to Slint's `FileEntry` type

### 7.2 文件操作 / File Operations (`src/fs/file_ops.rs`)

**中文**：核心文件操作实现（~1777 行），所有操作在后台线程执行：

- **Transfer（传输）**：统一的 copy/move/link 实现
  - `perform_transfer_with_progress_outcome()`：完整的传输流程，支持取消、进度报告
  - 冲突处理策略：`overwrite`（含备份恢复）、`rename`（`copy` 风格后缀）、`keepboth`
  - 自引用/子目录检测：`transfer_target_relation()` 检查 source 和 target 关系
  - Copy 使用 64KB buffer 逐块复制，支持取消标志
  - Move 在同设备上使用 `fs::rename`，跨设备回退到 copy + delete
  - Link 使用 `std::fs::hard_link`
- **Trash（回收站）**：
  - 遵循 freedesktop.org Trash 规范
  - 写入 `.trashinfo` 文件记录原始路径和删除日期
  - 支持 `$XDG_DATA_HOME/Trash` 和分区根目录 `.Trash-$UID`
- **进度报告**：`TransferProgress { bytes_done, bytes_total }`

**English**: Core file operation implementation (~1777 lines), all operations run in background threads:

- **Transfer**: unified copy/move/link implementation
  - `perform_transfer_with_progress_outcome()`: full transfer flow with cancellation and progress reporting
  - Conflict policies: `overwrite` (with backup/restore), `rename` (`copy`-style suffix), `keepboth`
  - Self/descendant detection: `transfer_target_relation()` checks source-target relationship
  - Copy uses 64KB buffer chunking with cancellation support
  - Move uses `fs::rename` on same device, falls back to copy+delete across devices
  - Link uses `std::fs::hard_link`
- **Trash**:
  - Follows freedesktop.org Trash specification
  - Writes `.trashinfo` files recording original path and deletion date
  - Supports `$XDG_DATA_HOME/Trash` and per-partition `.Trash-$UID`
- **Progress reporting**: `TransferProgress { bytes_done, bytes_total }`

### 7.3 设备发现 / Device Discovery (`src/fs/devices.rs`)

**中文**：侧栏 Devices 的 Rust 驱动实现（~2059 行）：

- **固定项**：`Filesystem`，路径为 `/`
- **挂载点发现**：优先解析 `/proc/self/mountinfo`，仅显示 `/run/media/$USER`、`/media/$USER`、`/media`、`/mnt` 下的真实设备；过滤 `tmpfs`、`proc`、`sysfs` 等伪文件系统
- **UDisks2 增强**：通过 system bus 查询 `org.freedesktop.UDisks2`，从 `Block` → `Drive` 关系识别外置介质（USB、removable、optical、flash）
- **合并策略**：mountinfo 结果和 UDisks2 结果按路径去重合并，UDisks2 信息增强 mountinfo 条目的能力标签
- **操作支持**：`mount_device()`、`unmount_device()`、`eject_device()` 通过 UDisks2 D-Bus 接口执行
- **诊断模式**：`device_diagnostics_report()` 生成格式化诊断报告，`--diagnose-devices` 命令行使用

**English**: Rust-driven Devices sidebar implementation (~2059 lines):

- **Fixed item**: `Filesystem` at `/`
- **Mount point discovery**: prefers parsing `/proc/self/mountinfo`, only showing real devices under `/run/media/$USER`, `/media/$USER`, `/media`, `/mnt`; filters pseudo-filesystems (`tmpfs`, `proc`, `sysfs`)
- **UDisks2 enhancement**: queries `org.freedesktop.UDisks2` on system bus, identifies external media via `Block` → `Drive` relationships (USB, removable, optical, flash)
- **Merge strategy**: mountinfo and UDisks2 results merged by path deduplication, UDisks2 info enriches mountinfo entries' capability labels
- **Operations**: `mount_device()`, `unmount_device()`, `eject_device()` via UDisks2 D-Bus interface
- **Diagnostics**: `device_diagnostics_report()` generates formatted diagnostics, used by `--diagnose-devices`

### 7.4 特权操作 / Privileged Operations (`src/fs/privilege.rs`)

**中文**：受保护文件操作的 D-Bus helper 实现（~1750 行）：

- **架构**：GUI 进程始终为普通用户，当需要 root 权限时，通过系统总线 D-Bus 调用独立的 helper 进程
- **D-Bus 接口**：`org.fika.FileManager1.Privileged`，方法包括：
  - `CreateFolder(parent, name)` → `created_path`
  - `CreateFile(parent, name)` → `created_path`
  - `Rename(path, new_name)` → `renamed_path`
  - `Trash(paths)` → `summary`
  - `Transfer(operation, source, target_dir)` → `destination`
- **Polkit 鉴权**：每个方法调用 `org.freedesktop.PolicyKit1.Authority.CheckAuthorization`，action id 为 `org.fika.FileManager.privileged-helper`
- **外部编辑器流程**：
  1. `PrepareExternalEdit(path)` → 创建可写临时副本（scratch），返回 `scratch_path` + `token`
  2. Fika 以普通路径启动编辑器
  3. Helper 监听 scratch 文件变更和 systemd unit 生命周期
  4. `CommitExternalEdit(token)` / `DiscardExternalEdit(token)`
- **空闲退出**：无活跃 scratch token 时，helper 空闲 180 秒后退出
- **开发 fallback**：未安装 system bus service 时通过 `pkexec` 启动 session bus 模式

**English**: D-Bus helper implementation for protected file operations (~1750 lines):

- **Architecture**: GUI process always runs as normal user; when root is needed, a separate helper is invoked via system bus D-Bus
- **D-Bus interface**: `org.fika.FileManager1.Privileged` with methods:
  - `CreateFolder(parent, name)` → `created_path`
  - `CreateFile(parent, name)` → `created_path`
  - `Rename(path, new_name)` → `renamed_path`
  - `Trash(paths)` → `summary`
  - `Transfer(operation, source, target_dir)` → `destination`
- **Polkit authorization**: each method calls `org.freedesktop.PolicyKit1.Authority.CheckAuthorization`, action id `org.fika.FileManager.privileged-helper`
- **External editor flow**:
  1. `PrepareExternalEdit(path)` → creates writable scratch copy, returns `scratch_path` + `token`
  2. Fika launches editor with normal path
  3. Helper monitors scratch file changes and systemd unit lifecycle
  4. `CommitExternalEdit(token)` / `DiscardExternalEdit(token)`
- **Idle exit**: without active scratch tokens, helper exits after 180 seconds idle
- **Dev fallback**: when system bus service not installed, uses `pkexec` for session bus mode

### 7.5 缩略图流水线 / Thumbnail Pipeline (`src/fs/thumbnails.rs`)

**中文**：异步缩略图生成和管理（~1306 行）：

- **缓存键**：`ThumbnailKey { path, modified_secs, size_px, freedesktop_size, freedesktop_cache_filename }`
- **内存缓存**：LRU 缓存，有容量限制，最老条目在超出限制时淘汰
- **失败缓存**：按相同键缓存失败结果，避免坏图反复排队解码
- **freedesktop.org 磁盘缓存**：
  - 计算 canonical `file://` URI → 对应 MD5 PNG 文件名
  - 写入 `~/.cache/thumbnails/{normal,large,x-large,xx-large}/`
  - 失败写入 `fail/fika-$version/` marker
- **外部 thumbnailer**：发现 XDG `.thumbnailer` entry，校验 `TryExec`，匹配 `MimeType`，展开 `Exec` 字段码
- **可见项优先调度**：可见列优先，overscan 后置
- **刷新保持**：同目录 refresh/watcher reload 保留进行中的缩略图任务

**English**: Async thumbnail generation and management (~1306 lines):

- **Cache key**: `ThumbnailKey { path, modified_secs, size_px, freedesktop_size, freedesktop_cache_filename }`
- **Memory cache**: LRU cache with capacity limit; oldest evicted when limit exceeded
- **Failure cache**: caches failures by same key to avoid repeated decode queuing for broken images
- **freedesktop.org disk cache**:
  - Computes canonical `file://` URI → corresponding MD5 PNG filename
  - Writes to `~/.cache/thumbnails/{normal,large,x-large,xx-large}/`
  - Failures write `fail/fika-$version/` markers
- **External thumbnailer**: discovers XDG `.thumbnailer` entries, validates `TryExec`, matches `MimeType`, expands `Exec` field codes
- **Visible-first scheduling**: visible columns first, overscan deferred
- **Refresh preservation**: same-directory refresh/watcher reload preserves in-progress thumbnail jobs

### 7.6 递归搜索 / Recursive Search (`src/fs/search.rs`)

**中文**：

- `search_recursive_with_progress(root, query, cancel, progress)`：异步递归目录搜索
- 使用栈（BFS 风格）：逐个目录读取，匹配条目的文件名（不区分大小写）和文件类型
- 支持 `AtomicBool` 取消标志
- 通过 `SearchProgress { directories_scanned, matches_found }` 实时报告进度
- 可用文件类型、大小范围、修改时间范围等过滤器过滤

**English**:

- `search_recursive_with_progress(root, query, cancel, progress)`: async recursive directory search
- Stack-based (BFS-style): reads directory by directory, matches entry filenames (case-insensitive) and types
- Supports `AtomicBool` cancellation flag
- Real-time progress via `SearchProgress { directories_scanned, matches_found }`
- Filterable by file kind, size range, and modification time range

### 7.7 Places 管理 / Places Management (`src/fs/places.rs`)

**中文**：

- `default_places()`：生成默认 Places 列表（Home、Desktop、Documents、Downloads、Music、Pictures、Videos）
- 用户 Places 持久化到 `$XDG_DATA_HOME/fika/places.tsv`
- 支持重排序（通过拖拽或菜单）、重命名、添加、移除、恢复默认

**English**:

- `default_places()`: generates default Places list (Home, Desktop, Documents, Downloads, Music, Pictures, Videos)
- User Places persisted to `$XDG_DATA_HOME/fika/places.tsv`
- Supports reorder (via drag or menu), rename, add, remove, restore defaults

---

## 8. 桌面集成模块 / Desktop Integration Module

### 8.1 MIME 类型与默认应用 / MIME & Default App (`src/desktop/mime_open.rs`)

**中文**：内置 MIME 检测和默认应用启动（~903 行），不依赖 `xdg-open`：

- `guess_mime_type(path)`：通过扩展名查询共享 MIME 数据库，含 fallback
- `find_default_app(mime_type)`：解析 `mimeapps.list`，遵循 XDG 优先级
- `open_file_with_default_app(path)`：解析 desktop 文件，展开 `Exec=` 字段码（`%f`, `%F`, `%u`, `%U`, `%i`, `%c`, `%k`）
- `list_apps_for_file(path)`：列出文件对应的所有已注册应用，标记默认应用
- 启动的应用通过 `systemd_launch` 纳入 user transient `.scope`

**English**: Built-in MIME detection and default app launching (~903 lines), no `xdg-open` dependency:

- `guess_mime_type(path)`: queries shared MIME database by extension with fallback
- `find_default_app(mime_type)`: parses `mimeapps.list`, follows XDG priority
- `open_file_with_default_app(path)`: resolves desktop file, expands `Exec=` field codes (`%f`, `%F`, `%u`, `%U`, `%i`, `%c`, `%k`)
- `list_apps_for_file(path)`: lists all registered apps for a file, marking the default
- Launched apps are attached to user transient `.scope` via `systemd_launch`

### 8.2 剪贴板 / Clipboard (`src/desktop/clipboard.rs`)

**中文**：

- 桌面剪贴板写入：Cut/Copy 通过内置 Wayland data-control 发布 `x-special/gnome-copied-files`、`text/uri-list` 和 `application/x-kde-cutselection`
- 读取端：通过内置 Wayland data-control 列 MIME 并读取 payload，解析 `x-special/gnome-copied-files`、`text/uri-list`、`application/x-kde-cutselection`
- Paste 入口通过异步事件桥读取桌面剪贴板后再入队传输，不在 UI 线程同步等待 Wayland selection
- 剪贴板读写都不调用外部 clipboard helper 命令
- 非文件剪贴板：按 COSMIC 顺序检测 image → video → text

**English**:

- Desktop clipboard write: Cut/Copy publishes `x-special/gnome-copied-files`, `text/uri-list`, and `application/x-kde-cutselection` through the built-in Wayland data-control owner
- Read side: uses the built-in Wayland data-control reader to list MIME types and read payloads, and parses `x-special/gnome-copied-files`, `text/uri-list`, `application/x-kde-cutselection`
- Paste reads the desktop clipboard through the async event bridge before queueing transfers instead of synchronously waiting for the Wayland selection on the UI thread
- Clipboard read/write does not call external clipboard helper commands
- Non-file clipboard: detects image → video → text in COSMIC order

### 8.3 终端启动 / Terminal Launch (`src/desktop/terminal.rs`)

**中文**：

- 优先级：`$FIKA_TERMINAL` → `$TERMINAL` → `x-scheme-handler/terminal` → 已知终端列表
- 优先使用 CosmicTerm（当存在时）
- 启动终端时纳入 systemd user scope

**English**:

- Priority: `$FIKA_TERMINAL` → `$TERMINAL` → `x-scheme-handler/terminal` → known terminal list
- Prefers CosmicTerm when available
- Terminal launch attached to systemd user scope

### 8.4 Systemd 集成 / Systemd Integration (`src/desktop/systemd_launch.rs`)

**中文**：

- 通过 D-Bus `org.freedesktop.systemd1.Manager.StartTransientUnit` 将启动的应用纳入 user transient `.scope`
- 支持受保护外部编辑的 unit 生命周期跟踪
- systemd 不可用时回退到普通 `Command::spawn`

**English**:

- Attaches launched apps to user transient `.scope` via D-Bus `org.freedesktop.systemd1.Manager.StartTransientUnit`
- Supports unit lifecycle tracking for protected external edits
- Falls back to normal `Command::spawn` when systemd is unavailable

---

## 9. 应用协调层 / Application Coordination Layer

### 9.1 `main.rs` 概览

**中文**：`main.rs`（~5768 行）是应用的中央调度中心，负责：

- 解析 CLI 参数、加载设置
- 初始化 `AppState` 和 `AppWindow`
- 注册 Slint callback（目录导航、文件操作、菜单动作、DnD 事件）
- 启动设备监控器
- 运行异步事件循环：从 channel 读取 `AsyncEvent`，分发到各处理函数

**English**: `main.rs` (~5768 lines) is the central dispatch hub, responsible for:

- Parsing CLI args, loading settings
- Initializing `AppState` and `AppWindow`
- Registering Slint callbacks (directory navigation, file operations, menu actions, DnD events)
- Starting device monitor
- Running async event loop: reads `AsyncEvent` from channel, dispatches to handlers

### 9.2 异步事件 / Async Events (`src/app/events.rs`)

**中文**：统一的异步事件枚举，所有后台任务通过此类型回到 UI 线程：

| 事件 | 含义 |
|------|------|
| `DirectoryLoaded(DirectoryLoadResult)` | 目录扫描完成 |
| `FileOpened(FileOpenResult)` | 文件打开完成 |
| `RecursiveSearchCompleted(RecursiveSearchResult)` | 递归搜索完成 |
| `RecursiveSearchProgress(RecursiveSearchProgress)` | 搜索进度更新 |
| `FileOperationCompleted(FileOperationResult)` | 文件操作完成 |
| `FileOperationProgress(FileOperationProgress)` | 操作进度更新 |
| `FileUndoCompleted(FileUndoResult)` | 撤销完成 |
| `ThumbnailsLoaded(Vec<ThumbnailLoad>)` | 缩略图生成完成 |
| `ExternalEditCompleted(ExternalEditResult)` | 外部编辑完成 |
| `DevicesLoaded(DevicesLoadedResult)` | 设备列表更新 |
| `DeviceMountCompleted(DeviceMountResult)` | 设备挂载完成 |
| `ClipboardUpdated(Vec<PathBuf>, bool)` | 剪贴板刷新完成 |
| `OperationControllerUpdate` | 操作队列状态变更 |

**English**: Unified async event enum; all background tasks return to UI thread through this type:

| Event | Meaning |
|-------|---------|
| `DirectoryLoaded(DirectoryLoadResult)` | Directory scan complete |
| `FileOpened(FileOpenResult)` | File open complete |
| `RecursiveSearchCompleted(RecursiveSearchResult)` | Recursive search complete |
| `RecursiveSearchProgress(RecursiveSearchProgress)` | Search progress update |
| `FileOperationCompleted(FileOperationResult)` | File operation complete |
| `FileOperationProgress(FileOperationProgress)` | Operation progress update |
| `FileUndoCompleted(FileUndoResult)` | Undo complete |
| `ThumbnailsLoaded(Vec<ThumbnailLoad>)` | Thumbnails generated |
| `ExternalEditCompleted(ExternalEditResult)` | External edit complete |
| `DevicesLoaded(DevicesLoadedResult)` | Device list updated |
| `DeviceMountCompleted(DeviceMountResult)` | Device mount complete |
| `ClipboardUpdated(Vec<PathBuf>, bool)` | Clipboard refreshed |
| `OperationControllerUpdate` | Operation queue state change |

### 9.3 操作控制器 / Operation Controller (`src/app/operation_controller.rs`)

**中文**：文件操作队列的集中控制器：

- `enqueue_operation()`：添加入队列
- `start_next_operation()`：启动下一个排队的操作
- `cancel_queued_operations()`：清空未开始队列
- 管理 active id、cancel flag 生命周期
- 生成 queued/start/progress/complete/failed 状态文本
- 完成验证（active-id 校验）、目录缓存失效和刷新决策
- 权限不足时保存 `PrivilegedCommand` 等待用户确认

**English**: Central controller for the file operation queue:

- `enqueue_operation()`: add to queue
- `start_next_operation()`: start next queued operation
- `cancel_queued_operations()`: clear unstarted queue
- Manages active id and cancel flag lifecycle
- Generates queued/start/progress/complete/failed status text
- Completion verification (active-id check), directory cache invalidation, refresh decisions
- Saves `PrivilegedCommand` on permission denial for user confirmation

### 9.4 分屏视图 / Split View (`src/app/split_view.rs`)

**中文**：

- `toggle_split_view()`：打开 Split 时从当前 pane 快照创建第二个 `PaneSlotSurface` 实例（slot-1），两个 slot 是对等的 `PaneSlotSurface` 组件——关闭 Split 时 `close_focused_split_pane()` 关闭焦点所在 slot，将被关闭 pane 的焦点路由给剩余 pane
- `sync_inactive_pane_ui()`：为 slot-1 计算虚拟切片和 viewport，与 slot-0 走完全相同的 `prepare_pane_preview_update()` 管线——没有功能降级
- `pane_viewport_x_from_ui()`：从 pane-local state 读取指定 slot 的自管 viewport 位置
- `set_pane_viewport_ui(slot, viewport_x)` / `set_pane_viewport_ui_if_clamped()`：将自管 viewport 写入正确的 pane slot 并增量刷新对应 row
- `directory_status_text()`：为指定路径查找对应 pane 的加载状态文本

**English**:

- `toggle_split_view()`: opens Split by creating a second `PaneSlotSurface` instance (slot-1) from the current pane's snapshot; both slots are identical `PaneSlotSurface` components — closing Split via `close_focused_split_pane()` removes the focused slot and routes focus to the remaining pane
- `sync_inactive_pane_ui()`: computes virtual slice and viewport for slot-1 through the exact same `prepare_pane_preview_update()` pipeline as slot-0 — no functional degradation
- `pane_viewport_x_from_ui()`: reads the requested slot's self-managed viewport position from pane-local state
- `set_pane_viewport_ui(slot, viewport_x)` / `set_pane_viewport_ui_if_clamped()`: writes the self-managed viewport to the correct pane slot and refreshes only that row
- `directory_status_text()`: looks up the loading status text for a given path's corresponding pane

### 9.5 选择逻辑 / Selection Logic (`src/app/selection.rs`)

**中文**：

- `rebuild_visible_entry_index()`：重建可见条目索引缓存（搜索过滤时使用非 identity 路径）
- `selection_range_paths_filtered()`：范围选择（Shift+click）
- `selection_rect_paths_filtered()`：矩形框选
- `retained_visible_paths()`：过滤后保留的可见路径
- `filtered_entry_count()` / `filtered_entry_paths()`：过滤后条目数量/路径

**English**:

- `rebuild_visible_entry_index()`: rebuild visible entry index cache (non-identity path for search/filters)
- `selection_range_paths_filtered()`: range selection (Shift+click)
- `selection_rect_paths_filtered()`: rectangle selection
- `retained_visible_paths()`: retained visible paths after filtering
- `filtered_entry_count()` / `filtered_entry_paths()`: filtered entry count/paths

### 9.6 虚拟视图 / Virtual View (`src/app/virtual_view.rs`)

**中文**：

- `VirtualGridPlan` 结构体：统一计算 clamped viewport、scroll max、visible range、overscan range、Slint anchor column
- `prepare_virtual_view_snapshot_update()`：用 `VirtualViewSnapshotInput` 在后台纯函数路径计算新虚拟切片，包括 viewport 夹紧、overscan 范围扩展、过滤切片和 location group 标注
- 可见列优先 + overscan 后置的缩略图调度策略
- 虚拟范围不变时跳过 Slint model 重置

**English**:

- `VirtualGridPlan` struct: unified clamped viewport, scroll max, visible/overscan range, Slint anchor column
- `prepare_virtual_view_snapshot_update()`: computes the new virtual slice from `VirtualViewSnapshotInput` on the background pure-function path, including viewport clamping, overscan expansion, filtered slicing, and location-group annotation
- Visible-column-first + overscan-deferred thumbnail scheduling
- Skips Slint model reset when virtual range unchanged

### 9.7 目录加载 / Directory Loading (`src/app/directory_loading.rs`)

**中文**：

- `prepare_directory_load()`：导航加载准备（更新 generation、缓存查找、取消搜索、恢复 view context）
- `prepare_directory_load_for_target()`：同目录刷新准备（保留缩略图 pipeline）
- `directory_entries_match()`：比较两个条目列表是否相同（避免无变化刷新）
- `DirectoryLoadErrorRecovery`：加载失败时的恢复策略

**English**:

- `prepare_directory_load()`: navigation load prep (update generation, cache lookup, cancel search, restore view context)
- `prepare_directory_load_for_target()`: same-directory refresh prep (preserve thumbnail pipeline)
- `directory_entries_match()`: compare two entry lists for equality (avoid no-change refresh)
- `DirectoryLoadErrorRecovery`: recovery strategy on load failure

### 9.8 传输协调 / Transfer Coordination (`src/app/transfer.rs`)

**中文**：

- `prepare_entry_transfer()` / `prepare_main_transfer()` / `prepare_place_transfer()` / `prepare_current_dir_transfer()` / `prepare_inactive_pane_transfer()`：各种 DnD 目标的传输准备
- 自引用/子目录拒绝检测
- `resolve_transfer_conflict()`：冲突对话框处理
- `start_transfer_operation()`：入队实际传输
- `cancel_queued_operations()`：取消排队操作

**English**:

- `prepare_entry_transfer()` / `prepare_main_transfer()` / `prepare_place_transfer()` / `prepare_current_dir_transfer()` / `prepare_inactive_pane_transfer()`: transfer prep for various DnD targets
- Self/descendant rejection detection
- `resolve_transfer_conflict()`: conflict dialog handling
- `start_transfer_operation()`: enqueue actual transfer
- `cancel_queued_operations()`: cancel queued operations

### 9.9 几何计算 / Geometry (`src/app/geometry.rs`)

**中文**：

- `MainGridLayout`：主栏网格布局参数
- `register_menu_geometry_callbacks()`：注册菜单几何纯函数回调
- `SelectionRect`：选择矩形
- `active_main_pane_width()`：计算活跃主栏宽度
- `place_drop_geometry()`：Places 拖拽插入位置计算

**English**:

- `MainGridLayout`: main grid layout parameters
- `register_menu_geometry_callbacks()`: register menu geometry pure callbacks
- `SelectionRect`: selection rectangle
- `active_main_pane_width()`: compute active main pane width
- `place_drop_geometry()`: Places drop insertion geometry calculation

### 9.10 Chooser 选择器 (`src/app/chooser.rs`)

**中文**：

- `ChooserOutputMetadata`：chooser 输出元数据格式
- `parse_chooser_choice_spec()` / `parse_chooser_filter_spec()`：解析 chooser 参数
- `safe_child_path()`：安全子路径验证
- `selected_directory_or_current()`：chooser 保存模式的目录选择

**English**:

- `ChooserOutputMetadata`: chooser output metadata format
- `parse_chooser_choice_spec()` / `parse_chooser_filter_spec()`: parse chooser params
- `safe_child_path()`: safe child path validation
- `selected_directory_or_current()`: directory selection for chooser save mode

---

## 10. 辅助支撑模块 / Support Module

### 10.1 Generation 计数器 (`src/support/generation.rs`)

**中文**：

- `GenerationCounter`：简单的单调递增计数器
- `next()` 递增并返回新值
- `is_current(generation)` 检查传入的 generation 是否等于当前值
- 用于丢弃过期异步结果：每次新导航、新搜索、新缩略图调度时递增，后台任务完成时携带 generation，UI 端根据 `is_current()` 决定是否采纳

**English**:

- `GenerationCounter`: simple monotonic incrementing counter
- `next()` increments and returns new value
- `is_current(generation)` checks if passed generation equals current value
- Used to discard stale async results: incremented on new navigation/search/thumbnail schedule; background tasks carry generation; UI side uses `is_current()` to decide whether to accept

### 10.2 Chooser 辅助 (`src/support/chooser.rs`)

**中文**：为 `fika-xdp-filechooser` 和 `fika --chooser` 提供共享的 chooser 输出解析逻辑。

**English**: Shared chooser output parsing logic for `fika-xdp-filechooser` and `fika --chooser`.

---

## 11. 二进制入口 / Binary Entry Points

### 11.1 `fika` 主二进制 (`src/main.rs`)

**中文**：默认 `default-run`，支持三种模式：

- `Mode::Manager`：标准文件管理器
- `Mode::Chooser`：轻量文件选择器，选择后打印路径到 stdout 并退出
- `Mode::DeviceDiagnostics`：打印设备诊断报告

**English**: Default `default-run`, supports three modes:

- `Mode::Manager`: standard file manager
- `Mode::Chooser`: lightweight file chooser, prints selected path to stdout and exits
- `Mode::DeviceDiagnostics`: prints device diagnostics report

### 11.2 `fika-privileged-helper` (`src/bin/fika-privileged-helper.rs`)

**中文**：独立特权 helper 二进制（45 行入口）：

- `--system-bus`：正式 system bus 模式，通过 D-Bus activation 以 root 启动
- `--session-bus ADDRESS`：开发 fallback，通过 `pkexec` 启动，校验 `PKEXEC_UID`
- 内部调用 `fika::privilege::run_dbus_service(bus)` 启动 D-Bus 服务

**English**: Standalone privileged helper binary (45-line entry):

- `--system-bus`: production system bus mode, started as root via D-Bus activation
- `--session-bus ADDRESS`: dev fallback, started via `pkexec`, validates `PKEXEC_UID`
- Internally calls `fika::privilege::run_dbus_service(bus)` to start D-Bus service

### 11.3 `fika-xdp-filechooser` (`src/bin/fika-xdp-filechooser.rs`)

**中文**：`xdg-desktop-portal` FileChooser 后端二进制（~1920 行）：

- D-Bus 名称：`org.freedesktop.impl.portal.desktop.fika`
- 对象路径：`/org/freedesktop/portal/desktop`
- 接口：`org.freedesktop.impl.portal.FileChooser`
- `OpenFile(handle, app_id, parent_window, title, options)` → 启动 `fika --chooser`，读取 stdout 路径，返回 `file://` URI
- `SaveFile(handle, ...)` → 选择保存路径
- `SaveFiles(handle, ...)` → 选择目标目录 + 文件名
- 支持 portal filter → chooser glob 转换、choice 转发
- `parent_window` 解析和转发（Wayland handle）
- 通过 `Request.Close` signal 处理调用方取消
- 子进程生命周期管理（`kill_on_drop` 兜底）

**English**: `xdg-desktop-portal` FileChooser backend binary (~1920 lines):

- D-Bus name: `org.freedesktop.impl.portal.desktop.fika`
- Object path: `/org/freedesktop/portal/desktop`
- Interface: `org.freedesktop.impl.portal.FileChooser`
- `OpenFile(handle, ...)` → launches `fika --chooser`, reads stdout paths, returns `file://` URIs
- `SaveFile(handle, ...)` → select save path
- `SaveFiles(handle, ...)` → select target directory + filenames
- Portal filter → chooser glob conversion, choice forwarding
- `parent_window` parsing and forwarding (Wayland handle)
- Caller cancellation via `Request.Close` signal
- Child process lifecycle management (`kill_on_drop` fallback)

---

## 12. 桌面集成数据 / Desktop Integration Data

### `data/` 目录

**中文**：包含 Fika 的 D-Bus、Polkit 和 xdg-desktop-portal 集成元数据，分为两个独立子系统：

**子系统一：Privileged Helper**

| 文件 | 安装路径 | 用途 |
|------|----------|------|
| `dbus-1/interfaces/org.fika.FileManager1.Privileged.xml` | `/usr/share/dbus-1/interfaces/` | D-Bus 接口规范文档 |
| `dbus-1/system-services/org.fika.FileManager1.Privileged.service.in` | `/usr/share/dbus-1/system-services/` | 系统总线 activation 模板 |
| `dbus-1/system.d/org.fika.FileManager1.Privileged.conf` | `/etc/dbus-1/system.d/` | 系统总线安全策略 |
| `polkit-1/actions/org.fika.FileManager.policy.in` | `/usr/share/polkit-1/actions/` | Polkit 授权策略模板 |

**子系统二：Portal Backend**

| 文件 | 安装路径 | 用途 |
|------|----------|------|
| `dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service.in` | `/usr/share/dbus-1/services/` | 会话总线 activation 模板 |
| `xdg-desktop-portal/portals/fika.portal` | `/usr/share/xdg-desktop-portal/portals/` | Portal 后端描述符 |

所有 `.in` 模板文件中的 `@bindir@` 占位符由 `install-data.sh` 在安装时替换。

**English**: Contains Fika's D-Bus, Polkit, and xdg-desktop-portal integration metadata, split into two independent subsystems:

**Subsystem 1: Privileged Helper**

| File | Install path | Purpose |
|------|-------------|---------|
| `dbus-1/interfaces/org.fika.FileManager1.Privileged.xml` | `/usr/share/dbus-1/interfaces/` | D-Bus interface specification |
| `dbus-1/system-services/org.fika.FileManager1.Privileged.service.in` | `/usr/share/dbus-1/system-services/` | System bus activation template |
| `dbus-1/system.d/org.fika.FileManager1.Privileged.conf` | `/etc/dbus-1/system.d/` | System bus security policy |
| `polkit-1/actions/org.fika.FileManager.policy.in` | `/usr/share/polkit-1/actions/` | Polkit authorization policy template |

**Subsystem 2: Portal Backend**

| File | Install path | Purpose |
|------|-------------|---------|
| `dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service.in` | `/usr/share/dbus-1/services/` | Session bus activation template |
| `xdg-desktop-portal/portals/fika.portal` | `/usr/share/xdg-desktop-portal/portals/` | Portal backend descriptor |

All `@bindir@` placeholders in `.in` templates are replaced by `install-data.sh` at install time.

---

## 13. 脚本与工具 / Scripts & Tools

### `scripts/install-data.sh`

**中文**：打包安装脚本，展开 service/policy 模板中的 `@bindir@`，安装所有元数据文件。支持 `DESTDIR`、`PREFIX`、`BINDIR`、`DATADIR`、`SYSCONFDIR` 环境变量。

### `scripts/check-install-data.sh`

**中文**：非 root 安装自检脚本，在临时 `DESTDIR` 中验证所有元数据文件位置和内容正确性。

### `scripts/check-runtime-integration.sh`

**中文**：安装后的运行时诊断脚本：

- `--metadata-only`：仅检查 staged package 中的元数据
- 普通模式：输出 OS、session、systemd、xdg-desktop-portal、Polkit agent、UDisks2 诊断摘要
- `--activate-system-helper`：额外通过 D-Bus 激活 privileged helper 并验证
- `--record FILE`：将输出保存到文件

### English

### `scripts/install-data.sh`

Packaging install script, expands `@bindir@` in service/policy templates, installs all metadata files. Supports `DESTDIR`, `PREFIX`, `BINDIR`, `DATADIR`, `SYSCONFDIR` env vars.

### `scripts/check-install-data.sh`

Non-root install self-check script, verifies all metadata file positions and content correctness in a temporary `DESTDIR`.

### `scripts/check-runtime-integration.sh`

Post-install runtime diagnostic script:

- `--metadata-only`: check only staged package metadata
- Normal mode: outputs OS, session, systemd, xdg-desktop-portal, Polkit agent, UDisks2 diagnostic summary
- `--activate-system-helper`: additionally D-Bus-activates privileged helper and validates
- `--record FILE`: save output to file

---

## 14. 数据流 / Data Flows

### 14.1 目录导航流程 / Directory Navigation Flow

**中文**：

```
用户输入路径 / 点击 Places / Back/Forward
    │
    ▼
prepare_directory_load()
    ├─ 更新 load_generation（使旧结果过期）
    ├─ 查找 directory_cache
    ├─ 有缓存 → 立即渲染缓存条目，恢复 viewport
    └─ 无缓存 → 保留旧画面，状态栏显示加载中
    │
    ▼
read_entries_async() → Tokio spawn_blocking
    │
    ▼
AsyncEvent::DirectoryLoaded
    ├─ generation 检查（丢弃过期结果）
    ├─ 转换为 FileEntry，更新目录缓存
    ├─ 更新 Slint 条目模型
    ├─ 计算虚拟切片
    └─ 调度可见缩略图
```

**English**:

```
User enters path / clicks Places / Back/Forward
    │
    ▼
prepare_directory_load()
    ├─ Update load_generation (stale old results)
    ├─ Lookup directory_cache
    ├─ Cache hit → render cached entries immediately, restore viewport
    └─ Cache miss → keep old view, status bar shows loading
    │
    ▼
read_entries_async() → Tokio spawn_blocking
    │
    ▼
AsyncEvent::DirectoryLoaded
    ├─ Generation check (discard stale results)
    ├─ Convert to FileEntry, update directory cache
    ├─ Update Slint entry model
    ├─ Compute virtual slice
    └─ Schedule visible thumbnails
```

### 14.2 文件操作流程 / File Operation Flow

**中文**：

```
用户拖放 / Paste / 菜单操作
    │
    ▼
prepare_*_transfer()
    ├─ 自引用/子目录检查
    ├─ 冲突检测 → 弹出冲突对话框
    └─ 权限检查
    │
    ▼
operation_controller::enqueue_operation()
    │
    ▼
start_next_operation() → Tokio spawn_blocking
    ├─ 进度报告 → AsyncEvent::FileOperationProgress
    └─ 完成 → AsyncEvent::FileOperationCompleted
        ├─ 注册 Undo
        ├─ 刷新受影响目录
        └─ 启动下一个排队操作
```

**English**:

```
User drag-drop / Paste / menu action
    │
    ▼
prepare_*_transfer()
    ├─ Self/descendant check
    ├─ Conflict detection → conflict dialog
    └─ Permission check
    │
    ▼
operation_controller::enqueue_operation()
    │
    ▼
start_next_operation() → Tokio spawn_blocking
    ├─ Progress → AsyncEvent::FileOperationProgress
    └─ Complete → AsyncEvent::FileOperationCompleted
        ├─ Register Undo
        ├─ Refresh affected directories
        └─ Start next queued operation
```

### 14.3 缩略图流程 / Thumbnail Flow

**中文**：

```
虚拟切片更新
    │
    ▼
prioritize_thumbnail_entries() → 可见优先排序
    │
    ▼
thumbnails::spawn_thumbnail_async() → Tokio
    ├─ 检查 freedesktop 磁盘缓存
    ├─ 检查失败 marker
    ├─ 内置解码（PNG/JPEG/WebP）
    └─ 外部 thumbnailer（PDF/SVG/AVIF）
    │
    ▼
AsyncEvent::ThumbnailsLoaded
    ├─ 写入内存缓存（成功/失败）
    ├─ 写入 freedesktop 磁盘缓存
    └─ 刷新可见切片
```

**English**:

```
Virtual slice update
    │
    ▼
prioritize_thumbnail_entries() → visible-first sort
    │
    ▼
thumbnails::spawn_thumbnail_async() → Tokio
    ├─ Check freedesktop disk cache
    ├─ Check failure marker
    ├─ Built-in decode (PNG/JPEG/WebP)
    └─ External thumbnailer (PDF/SVG/AVIF)
    │
    ▼
AsyncEvent::ThumbnailsLoaded
    ├─ Write memory cache (success/failure)
    ├─ Write freedesktop disk cache
    └─ Refresh visible slice
```

### 14.4 特权操作流程 / Privileged Operation Flow

**中文**：

```
文件操作返回权限错误
    │
    ▼
保存 PrivilegedCommand → 弹出确认框
    │
    ▼
用户确认
    │
    ▼
D-Bus system bus → org.fika.FileManager1.Privileged
    ├─ D-Bus daemon 以 root 启动 fika-privileged-helper
    ├─ Helper 调用 Polkit CheckAuthorization
    ├─ 桌面 Polkit agent 弹出密码对话框
    └─ 授权通过 → 执行操作 → 返回结果
```

**English**:

```
File operation returns permission error
    │
    ▼
Save PrivilegedCommand → show confirmation dialog
    │
    ▼
User confirms
    │
    ▼
D-Bus system bus → org.fika.FileManager1.Privileged
    ├─ D-Bus daemon starts fika-privileged-helper as root
    ├─ Helper calls Polkit CheckAuthorization
    ├─ Desktop Polkit agent shows password dialog
    └─ Authorized → execute operation → return result
```

---

## 15. 工程规则 / Engineering Rules

### 中文

1. UI 主线程只做状态更新和轻量计算
2. 后台任务不能直接访问 Slint UI 对象
3. 跨线程数据使用 owned Rust 类型；进入 UI 前再转换成 Slint 类型
4. 每类异步任务都要有 generation 或 cancellation 机制，并通过统一 `AsyncEvent` 回到 UI 线程
5. 新功能优先补 focused tests；UI 行为至少通过 `cargo check` 覆盖 Slint 编译
6. 避免无关重构，按 TODO 阶段逐项推进

### English

1. UI main thread only performs state updates and lightweight computation
2. Background tasks must not directly access Slint UI objects
3. Cross-thread data uses owned Rust types; convert to Slint types only before entering UI
4. Every async task type must have generation or cancellation mechanism, returning via unified `AsyncEvent` to UI thread
5. New features should add focused tests first; UI behavior must at least pass `cargo check` for Slint compilation
6. Avoid unrelated refactoring; follow TODO phases incrementally

---

> 本文档随项目演进持续更新。
> This document is updated continuously as the project evolves.
