# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://blog.rust-lang.org/2024/02/08/Rust-1.76.0.html)

一个面向现代 Wayland 桌面的轻量文件管理器。当前实现使用 Rust +
[Slint](https://slint.dev)，但项目的活跃目标已经切换为全面 GPUI 重写，并以 Dolphin
作为第一行为参考。

**当前状态：** 迁移规划阶段。Slint 应用是旧实现和可复用 Rust 模块来源；新的 UI 架构工作应以
[docs/TODO.md](docs/TODO.md) 和
[docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md) 为准。

> [English version](README.md)

## 功能

### 文件浏览

- 浏览本地目录，支持面包屑导航和路径直接输入
- 目录历史：前进/后退，鼠标侧键导航
- 左侧 Places 侧栏（内置项 + 用户自定义，支持拖拽排序、重命名、新窗口打开）
- Devices 侧栏：通过 UDisks2 发现存储设备，支持挂载/卸载/弹出，包含挂起和错误状态
- 防抖动目录监控（inotify），自动刷新
- 轻量虚拟化主视图：横向列优先、Dolphin 风格的 compact 布局，大目录下保持低资源占用
- Split View 分屏：同时预览两个目录，可交换焦点、独立滚动、可拖拽调整分栏比例

### 搜索

- 实时递归搜索，支持进度报告和取消
- 文件类型过滤器：文件夹、文档、图片、音频、视频
- 搜索结果按相对位置分组，按路径排序
- 搜索栏带可展开的过滤芯片，使用作用域 FlexboxLayout 实现自适应换行

### 文件操作

- 异步文件操作队列：复制、移动、链接、回收站、重命名
- 冲突处理与一步撤销
- 内部拖放传输菜单（移动/复制/链接）
- 框选与多选
- 剪贴板集成（Ctrl+C/X/V）
- 右键菜单创建新文件夹 / 新文件
- 原地复制、复制路径、重命名
- 从 Wayland 剪贴板粘贴图片、视频和文本内容为文件

### 回收站管理

- 移至回收站（Delete 键），含每文件 `.trashinfo` 元数据
- 从回收站还原：从 `.trashinfo` 读取原始位置
- 永久删除（仅在回收站中可用，需确认）
- 清空回收站（含确认对话框）
- 回收站视图显示原始位置和删除日期，按最新删除排序
- 回收站目录监控：同时监视 `files/` 和 `info/` 目录的外部变更

### UI / UX

- 明暗主题切换
- 可调整大小的侧栏和分屏比例（持久化保存）
- 受保护最小窗口尺寸，防止内容溢出
- COSMIC 风格的 shell 表面分层，Dolphin 风格的 compact 主栏文件视图
- Ctrl+滚轮缩放图标
- 右键上下文菜单：文件、文件夹、空白区域和 Places 项

### 桌面集成

- 内置 MIME 类型推断和默认应用程序启动（不依赖 `xdg-open`）
- Open With 菜单：默认应用、已添加关联、已缓存关联
- Open With 子菜单和应用选择器对话框（"Other Applications…"）
- 设置默认应用程序（写入用户级 `mimeapps.list`）
- 通过 systemd 用户 scope 启动应用程序，追踪进程生命周期

### 服务菜单

- 右键菜单加载用户安装的服务菜单 `.desktop` 项
- 发现 Fika 自有 `fika/servicemenus` 和 KDE/Dolphin `kio/servicemenus` 目录
- MIME 类型和多选过滤
- 无 Shell 的 Exec 字段代码展开（`%f`、`%F`、`%u`、`%U`、`%d`、`%n`）
- 子菜单分组和顶层动作排序

### 缩略图

- 异步缩略图生成：内置支持 PNG / JPEG / WebP
- 内存 LRU 缓存 + 磁盘缓存（符合 [freedesktop.org Thumbnail Managing Standard](https://specifications.freedesktop.org/thumbnail-spec/)）
- 外部 thumbnailer 支持：自动发现 XDG `.thumbnailer` 条目，处理 PDF / SVG / AVIF 等格式
- 失败缓存：避免坏图或非支持格式重复排队解码
- 可见优先调度：视口内缩略图优先于屏幕外项目生成

### 文件选择器 / Portal

- 轻量文件选择器模式 (`--chooser`)，可作为 `xdg-desktop-portal` FileChooser 后端
- `fika-xdp-filechooser` 二进制：暴露 `org.freedesktop.impl.portal.FileChooser` D-Bus 接口
- 独立于 GNOME/KDE/COSMIC/GTK portal 后端

### 安全

- GUI 进程意图非特权化
- 受保护操作通过独立的系统总线 D-Bus helper (`fika-privileged-helper`) 执行
- 按方法进行 Polkit 鉴权
- 受保护外部编辑器：临时副本 + 通过 systemd unit 生命周期监控自动写回

### 状态持久化

- 窗口尺寸、侧栏宽度、分屏比例
- 暗色模式偏好
- 图标缩放级别
- 上次打开的目录
- 设置存储在 `$XDG_CONFIG_HOME/fika/settings.tsv`

### 迁移方向

后续目标是 GPUI + UI-neutral Rust core。目录加载、刷新、undo、split pane identity
和 model signal 的第一参考是 Dolphin 的 `DolphinView → KDirLister → KFileItemModel →
KItemListView` 执行流。

详见：

- [docs/TODO.md](docs/TODO.md)
- [docs/DESIGN.md](docs/DESIGN.md)
- [docs/REFERENCE.md](docs/REFERENCE.md)
- [docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md)

## 前置条件

- Rust 1.76+（2024 edition）
- Linux 系统（Wayland）
- Slint 编译依赖：CMake、pkg-config、fontconfig、libxkbcommon

Arch Linux:

```sh
sudo pacman -S cmake pkgconf fontconfig libxkbcommon
```

## 快速开始

```sh
# 构建
cargo build --release

# 以文件管理器模式运行
cargo run

# 以文件选择器模式运行
cargo run -- --chooser ~/Downloads

# 诊断设备发现（不启动 GUI）
cargo run -- --diagnose-devices

# 查看完整 CLI 帮助
cargo run -- --help
```

## CLI 参考

```
fika [选项] [起始目录]
```

### 模式

| 选项 | 模式 | 说明 |
|------|------|------|
| *(默认)* | 管理器 | 标准文件管理器窗口 |
| `--chooser` | 选择器 | 文件选择器模式，选中的路径打印到 stdout |
| `--diagnose-devices` | 诊断 | 打印设备发现信息，不启动 GUI |

### 选择器模式选项

| 选项 | 说明 |
|------|------|
| `--chooser-directory` | 仅选择目录 |
| `--chooser-multiple` | 允许多选 |
| `--chooser-save <name>` | 保存文件对话框模式 |
| `--chooser-save-files <names>` | 保存文件 + 预设文件名（换行分隔） |
| `--chooser-title <text>` | 自定义窗口标题 |
| `--chooser-accept-label <text>` | 自定义确认按钮文本 |
| `--chooser-filters <filters>` | 文件过滤器（换行分隔，格式：`名称\n模式` 交替） |
| `--chooser-filter-index <n>` | 默认选中的过滤器索引 |
| `--chooser-return-filter` | 输出选中的过滤器索引 |
| `--chooser-choices <choices>` | 附加选择控件（换行分隔，格式：`id\nlabel\nvalue` 三元组） |
| `--chooser-return-choices` | 输出选择控件状态 |
| `--chooser-parent-window <handle>` | 父窗口句柄（用于 portal 嵌入） |

选择器模式下，选中文件后按 Choose 将路径打印到 stdout 并退出。如果使用了 `--chooser-return-filter` 或 `--chooser-return-choices`，额外元数据会以 `FIKA_CHOOSER_FILTER\t` 和 `FIKA_CHOOSER_CHOICE\t` 前缀输出。

## 键盘快捷键

| 快捷键 | 操作 |
|--------|------|
| `Ctrl + C` | 复制选中文件到剪贴板 |
| `Ctrl + X` | 剪切选中文件到剪贴板 |
| `Ctrl + V` | 粘贴文件到当前目录 |
| `Ctrl + A` | 全选可见文件 |
| `Ctrl + F` | 打开搜索 |
| `Ctrl + Z` | 撤销上次文件操作 |
| `Delete` | 将选中文件移至回收站（回收站内禁用） |
| `F5` | 刷新当前目录 |
| `Escape` | 清除选择 / 关闭弹窗 / 退出搜索 |
| `Ctrl + 滚轮` | 缩放图标大小 |
| `鼠标后退键` | 后退到上一目录 |

文件操作快捷键（Ctrl+C/X/V/Z/Delete）在搜索框、保存文件名输入框或弹窗打开时会被阻止，防止误操作。

## 桌面集成安装

打包安装会将 D-Bus 服务文件、Polkit 策略和 portal 元数据部署到系统目录。

### 安装数据文件

```sh
sudo PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
```

### 分阶段测试（无需 root）

```sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika \
  scripts/check-runtime-integration.sh --metadata-only
```

### 验证运行时集成

安装后运行：

```sh
scripts/check-runtime-integration.sh
```

该脚本验证系统总线 helper、Polkit 策略和 portal 后端元数据是否正确安装，并打印运行时环境摘要（发行版、桌面环境、`portals.conf` 位置）。添加 `--activate-system-helper` 可确认 D-Bus 激活：

```sh
scripts/check-runtime-integration.sh --activate-system-helper
```

### Portal 后端配置

安装 `fika.portal` 仅注册后端，不会使 Fika 成为激活的 FileChooser。要试用 Fika 后端，需在 `portals.conf` 中显式配置。参考 `docs/examples/fika-portals.conf` 中的示例，将其放入对应的用户或系统 `portals.conf` 文件中。

## 环境变量

### 自定义

| 变量 | 说明 | 示例 |
|------|------|------|
| `FIKA_ICON_THEME` | 覆盖图标主题 | `FIKA_ICON_THEME=Papirus` |
| `FIKA_GUI` | 覆盖 portal 后端前端可执行文件路径 | 调试用 |
| `FIKA_PRIVILEGED_HELPER` | 覆盖特权 helper 可执行文件路径 | 调试用 |

### 调试

| 变量 | 说明 |
|------|------|
| `FIKA_DEBUG_DEVICES=1` | 打印设备发现和监控诊断信息 |
| `FIKA_DEBUG_DND=1` | 打印拖放调试信息 |
| `FIKA_DEBUG_PORTAL=1` | 打印 portal 调试信息 |
| `FIKA_DEBUG_NAV=1` | 打印导航调试信息 |
| `FIKA_DEBUG_PRIVILEGE=1` | 打印特权操作调试信息 |

## 架构

```
src/
├── main.rs          入口点，Slint UI 回调实现
├── lib.rs           库根
├── config/          CLI 参数解析、路径、设置持久化、服务菜单策略
├── app/             UI 线程共享状态、异步事件桥接、目录加载、DnD、
│                    Places、主视图虚拟化、选择、缩略图流水线、分屏、
│                    搜索 UI、上下文菜单路由、文件选择器和设备监控
├── desktop/         内置 MIME/默认应用解析、Open With、应用选择器、
│                    Wayland 剪贴板、图标查找、服务菜单发现与启动、
│                    systemd 用户 scope 集成
├── fs/              文件条目、文件操作、设备发现（UDisks2 + mountinfo）、
│                    Places 后端、递归搜索、缩略图、特权操作
├── support/         选择器输出、世代计数器
└── bin/
    ├── fika-privileged-helper.rs   系统总线 D-Bus 特权 helper
    └── fika-xdp-filechooser.rs     XDG Desktop Portal FileChooser 后端
```

GUI 进程故意非特权化。受保护的文件操作通过系统总线 D-Bus helper 执行，按方法进行 Polkit 鉴权。

详细设计文档见：
- [docs/DESIGN.md](docs/DESIGN.md) — 架构与子系统设计
- [docs/TODO.md](docs/TODO.md) — 实现路线图与验收标准
- [docs/REFERENCE.md](docs/REFERENCE.md) — 中英文详细参考文档
- [docs/OPTIMIZATION.md](docs/OPTIMIZATION.md) — 性能优化方向

## 许可证

[MIT](LICENSE)
