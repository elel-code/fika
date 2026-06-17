> 本文是 [MIME_LAUNCHER_REFERENCE.md](MIME_LAUNCHER_REFERENCE.md) 的简体中文翻译。

# MIME 和启动器参考

本文档记录 Fika 的 MIME 识别、图标选择、Open With 菜单和进程启动路径的源码参考。

## Dolphin 参考

- `../dolphin/src/kitemviews/kfileitemmodel.cpp`
  - `KFileItemModel::createItemDataList()` 仅当按类型排序需要稳定顺序时同步解析 MIME 类型。
  - `retrieveData()` 仅在 model 路径上存储快速 role。MIME 类型未知时避免调用 `KFileItem::iconName()`。
  - MIME 类型已知时，图标数据来自 `item.iconName()`。若主题无该图标，回退到 `QMimeDatabase().mimeTypeForName(item.mimetype()).genericIconName()`。
- `../dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`
  - MIME 注释、图标名称、权限、缩略图等昂贵 role 在快速列表路径之外解析。
- `../dolphin/src/dolphincontextmenu.cpp`：右键菜单填充委托文件特定的 Open With 和 service menu action 给 KDE action 基础设施。

## Cosmic Files 参考

- `../cosmic-files/src/mime_icon.rs`：使用 shared-mime-info 进行 MIME 猜测和图标名称查找。按 `(mime, size)` 缓存 MIME 图标句柄。
- `../cosmic-files/src/mime_app.rs`：从 `.desktop` 条目和 `mimeapps.list` 构建 MIME 应用缓存。独立跟踪默认应用和附加关联。

## Fika 映射

- Core MIME 解析在 `src/core/mime.rs`：读取 shared-mime-info `globs2`、`icons`、`generic-icons` 和 MIME XML 图标声明。
- 条目构造在 `src/core/entries.rs`：目录列表将文件名/glob MIME 数据存储在 `EntryData` 上。`src/core/mime/roles.rs` 镜像 Dolphin 的昂贵 role 拆分。
- UI 图标选择在 `src/ui/icons.rs` 和 `src/ui/icons/cache.rs`：按 MIME/文件类型和图标大小缓存。候选顺序镜像 Dolphin。
- Core 启动器和应用发现在 `src/core/launcher.rs`：
  - 解析 `.desktop` 记录、`MimeType=`、`Exec=`、`Actions=` 等。
  - `launch_with_systemd_user()` 通过 session-bus helper 以 systemd transient units 启动。
  - 从专用 XDG service menu 目录发现 service menu `.desktop` 文件。
  - 支持 `X-KDE-ServiceTypes`、`X-KDE-Submenu`、协议/URL 数量条件、`TopLevel` 优先级。
  - 解析 `mimeinfo.cache` 和 `mimeapps.list`。应用排序：default → added → cached/declared → removed 过滤。
  - 构建 `DesktopLaunchPlan` 并转换为 systemd user transient units。
- Open With UI 集成在 `src/main.rs`：条目右键菜单存储 `MimeApplication` 值。"Other Application..." 对话框列出所有应用并支持 Set Default 写回。
- Service menu action 执行使用与 Open With 相同的路径。

## 剩余工作

- 添加需要 KIO 或授权上下文的 KDE 高级条件。
