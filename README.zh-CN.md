# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika 是一个面向 Linux 桌面的 Rust 文件管理器。当前 UI 主线是默认的 `fika`
二进制，也就是 Fika 专用的 `winit + wgpu` shell；之前的 UI runtime 已经从
源码树移除。

> [English version](README.md)

## 当前 Runtime

- `fika` 是默认运行目标，也是当前源码树里唯一的文件管理器 UI。
- `winit` 当前跟随官方上游的 `master` 分支，以使用跨平台 DnD API。
- `wgpu` 使用官方 crates.io 版本。
- `fika-core` 保持 UI-neutral，负责文件系统和领域行为。
- 剪贴板集成直接使用 Wayland `wl_data_device`；粘贴不再 shell out 到
  `wl-paste`、`wl-copy` 或 `xclip`。
- Portal 与 privileged helper 继续作为独立集成二进制保留。

## 源码布局

```text
src/
  lib.rs                         UI-neutral core 导出
  main.rs                        winit/wgpu shell 入口
  core.rs                        Core 模块重导出
  cli.rs                         共享 CLI 解析入口
  cli/
    args.rs                      Manager/chooser 参数解析
  core/                          Directory、pane、operations、launcher、
                                 Places、devices、thumbnails、trash、D-Bus
  shell/                         已拆出的 shell 模块
  bin/
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser 后端
    fika-privileged-helper.rs    特权操作 D-Bus helper
```

## 构建与运行

```bash
cargo run --bin fika -- --view compact /etc
cargo test --bin fika
scripts/check-rust-file-lines.sh
```

每个 Rust 源文件严格限制为最多 1000 行。门禁不设历史豁免，合并变更前必须通过。

因为 `default-run` 已经是 `fika`，也可以直接运行：

```bash
cargo run -- /etc
```

## 架构要点

- Pane state 按稳定 pane identity 路由，并通过可复用 pane container 存储；
  分屏 pane 走同一套 state/projection/slot-pool 路径。
- 热路径 item view 使用 retained + virtualization：visible-slot 复用、投影缓存、
  text/icon atlas 缓存和显式 scroll metrics。
- Shell 热路径使用 MIME/icon role 按 role + size 复用、read-ahead 队列化、
  atlas 子矩形上传，并收紧 icon theme cache 边界。
- 文件管理器语义以 Dolphin 为第一参考；shell 层负责渲染、hit-test、DPI、输入路由、
  overlay 和 telemetry。

## 参考文档

- [docs/DEVICES_REFERENCE.zh-CN.md](docs/DEVICES_REFERENCE.zh-CN.md) —
  设备和 Places 行为。
- [docs/NETWORK_REFERENCE.zh-CN.md](docs/NETWORK_REFERENCE.zh-CN.md) —
  网络位置行为。
- [docs/PERFORMANCE_ALIGNMENT.zh-CN.md](docs/PERFORMANCE_ALIGNMENT.zh-CN.md) —
  Dolphin-first 性能参考原则。
- [docs/TRASH_REFERENCE.zh-CN.md](docs/TRASH_REFERENCE.zh-CN.md) —
  回收站行为。
