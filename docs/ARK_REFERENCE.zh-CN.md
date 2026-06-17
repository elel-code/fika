> 本文是 [ARK_REFERENCE.md](ARK_REFERENCE.md) 的简体中文翻译。

# Ark 集成参考

Fika 的压缩文件集成应首先遵循 Dolphin 的菜单边界，然后决定 action 由 KDE service menu、Ark D-Bus 拖拽提取、Ark 命令行执行还是 Rust fallback 支持。

## Dolphin 源码

- `../dolphin/src/dolphincontextmenu.cpp`：`addAdditionalActions()` 插入分隔符，可选添加 `open_terminal_here`，然后委托 `m_fileItemActions->addActionsTo()`。Compress/Extract 条目通过 KDE service action 路径到达。
- `../dolphin/src/views/draganddrophelper.{h,cpp}`：定义 Ark 拖拽 MIME 类型 `application/x-kde-ark-dndextract-service/path`。`dropUrls()` 先检查 `isArkDndMimeType()` 再处理普通 URL drop。
- `../dolphin/.flatpak-manifest.json`：Dolphin Flatpak 包含 Ark 和压缩库，确认 Dolphin 的压缩 UX 依赖 Ark 可用。

## Dolphin 行为模型

- 右键菜单的 Compress/Extract 不是硬编码的，而是通过 KDE file item action 基础设施贡献的 service/menu action。
- 文件管理器仍需压缩感知的菜单布局：单压缩文件显示 Extract action，单非压缩文件显示 Compress，多选支持多文件 Exec 字段代码或 Fika 提供 fallback。
- 从 Ark 拖出与 service menu 分离：Ark 发布 D-Bus service/object MIME 对；Dolphin 通过 `extractSelectedFilesTo()` 发送目标路径回 Ark。

## 当前 Fika 状态

- Fika 已在 `src/core/launcher.rs` 解析 KDE service menu 文件。右键菜单已渲染 service action。
- `src/core/archive.rs` 提供小型压缩分类器（MIME 优先，扩展名 fallback：`.zip/.tar/.tar.gz/.tgz/.tar.bz2/.tbz2/.tar.xz/.txz/.7z/.rar`）。
- 上下文菜单在无匹配 service-menu action 时暴露内置 `Compress...` fallback（`ark --add --changetofirstpath --autofilename zip`）。
- 单压缩文件暴露 `Extract Here`（`ark --batch --destination <parent> <archive>`）和 `Extract To...`（加 `--dialog`）。
- Ark DnD MIME 解析器产生 `ArkDndExtractPayload` 和 `ArkDndExtractRequest`，通过共享会话总线 helper 执行。

## Fika 实现计划

1. 保持 service-menu 驱动 action 为第一路径。✅
2. 添加 core 中小型压缩分类器。✅
3. 仅 service menu 不提供等效 action 时添加 fallback 右键菜单 action。✅
4. 通过操作/状态基础设施路由执行。✅（Ark 命令行 fallback 与 Open With/service action 使用相同 systemd user transient unit 启动器边界）
5. 添加 Ark DnD 提取支持。core parser/executor 已完成；GPUI/backend 多 MIME offer 路由待处理。
6. 延迟压缩虚拟目录浏览。

## 剩余工作

- 将 Ark 多 MIME DnD offer 从 GPUI/backend 拖拽数据路径接入 core parser/executor。
- 添加不使用 Ark 的系统的 Rust fallback 压缩工作（如需要）。
- 单独设计压缩虚拟目录浏览。
