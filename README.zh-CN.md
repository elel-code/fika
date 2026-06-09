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

- GPUI 管理器窗口和目录 pane。
- 带稳定 `PaneId` 路由的动态分屏。
- `fika-core` 中的 UI-neutral directory lister 和 model。
- pane-scoped reload 与文件系统 watcher refresh。
- 当前目录被删除时跳转到最近仍存在的上级目录。
- pane-local selection、导航快捷键、移入回收站和 undo refresh。
- 最小 GPUI chooser 模式，输出选中路径和 portal 元数据。
- XDG Desktop Portal FileChooser 后端二进制。
- 系统总线特权 helper 二进制边界。

旧 UI 实现已经从主代码树移除。GPUI package 中不存在的能力都应视为后续实现任务，而不是当前功能。

## 布局

```text
src/
  lib.rs                     UI-neutral core module exports
  directory.rs               Directory lister 和 watcher event 分类
  entries.rs                 文件条目 metadata 和排序输入
  model.rs                   Directory model snapshots 和 model signals
  pane.rs                    Pane identity、pane state、split/close 路由
  operations.rs              Operation queue 和 undo payloads
  file_ops.rs                文件 transfer/trash/create/rename primitives
  privilege.rs               特权操作 API surface
  main.rs                    主 `fika` GPUI 应用和 chooser shell
  bin/fika-xdp-filechooser.rs
                             XDG Desktop Portal FileChooser 后端
  bin/fika-privileged-helper.rs
                             受保护操作的系统总线 helper
```

根 manifest 是单一 Cargo package。它从 `src/lib.rs` 暴露 `fika_core` library，
并从 `src/main.rs` 和 `src/bin/` 构建 `fika`、`fika-xdp-filechooser` 和
`fika-privileged-helper` 二进制。

## 构建

前置条件：

- 支持 Rust 2024 edition 的工具链。
- GPUI 和 zbus 所需的 Linux 桌面开发库。
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

- [docs/DESIGN.md](docs/DESIGN.md) - 当前 GPUI/core 架构。
- [docs/TODO.md](docs/TODO.md) - 剩余实现任务。
- [docs/REFERENCE.md](docs/REFERENCE.md) - Dolphin 与 Fika 参考索引。
- [docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md) - 原始切换计划。

## 许可证

[MIT](LICENSE)
