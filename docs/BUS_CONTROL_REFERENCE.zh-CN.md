> 本文是 [BUS_CONTROL_REFERENCE.md](BUS_CONTROL_REFERENCE.md) 的简体中文翻译。

# 总线控制参考

本文档记录 D-Bus 源码参考和 Fika 共享总线控制层的目标映射。目标是避免在启动器、portal、特权 helper、设备、Ark DnD 和未来 FileManager1 集成中分散调用原始 `zbus::Connection`。

## Dolphin 源码

- `../dolphin/src/main.cpp`：Dolphin 创建 `KDBusService`。会话总线不可用时普通启动仍继续工作；D-Bus 集成不是本地目录浏览的硬依赖。
- `../dolphin/src/dbusinterface.{h,cpp}`：在会话总线上注册 `/org/freedesktop/FileManager1`，请求 `org.freedesktop.FileManager1`，实现 `ShowFolders`/`ShowItems`/`ShowItemProperties`。
- `../dolphin/src/views/draganddrophelper.cpp`：Ark 拖拽读取 `application/x-kde-ark-dndextract-service/path`，调用 `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)`。

## Cosmic Files 源码

- `../cosmic-files/cosmic-files-applet/src/main.rs`：用 zbus 创建阻塞会话总线服务，拥有 `org.freedesktop.FileManager1`。
- `../cosmic-files/src/mounter/`：mounter 架构是 GIO/GVfs 设备发现和 action 路由的 Rust 端参考。

## 当前 Fika 状态

- `src/core/bus.rs`
  - 定义 `BusKind::{Session,System}`、`BusCallTarget`、`BusConfig`、`BusController` 和结构化 `BusError`。
  - 在共享 controller 后惰性缓存会话和系统 `zbus::Connection` 句柄，默认 30s 空闲超时。
  - 刻意不启用 `zbus/tokio` 特性以避免与 GPUI 的 `ashpd`/accessibility zbus 调用冲突。
  - `BusController::proxy()` 返回 owned zbus proxy。
- `src/core/launcher.rs`：`launch_with_systemd_user()` 使用 `BusController::shared()` 获取会话总线。
- `src/core/privilege.rs`：特权文件操作和外部编辑生命周期的客户端 helper 通过 `BusController::shared()` 获取连接。
- `src/core/archive.rs`：解析 Ark DnD service/path MIME 载荷，构建 `ArkDndExtractRequest`，通过共享会话总线 helper 执行。
- `src/core/devices.rs`：主后端为 GIO/GVfs `VolumeMonitor`，不再直接使用 zbus。

## 已完成的实现

- Core bus controller：懒连接缓存、30s 空闲超时、3 次重试的方法调用超时/重试。
- `launch_with_systemd_user()` 迁移到共享会话总线 helper。
- Ark DnD executor 边界。
- Privileged-helper 客户端调用迁移到共享总线连接 helper。

## 剩余工作

1. GPUI/backend 多 MIME 外部拖拽提供接入 core Ark DnD executor。
2. 添加 FileManager1 会话总线注册（`ShowFolders`/`ShowItems`/`ShowItemProperties`）。

## 约束

- 会话总线不可用时本地文件浏览必须继续。
- 系统总线失败必须降级特权操作，不影响 pane 渲染。长运行 D-Bus 操作不得在 GPUI 渲染路径上运行。
