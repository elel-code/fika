> 本文是 [CONTEXT_MENU_REFERENCE.md](CONTEXT_MENU_REFERENCE.md) 的简体中文翻译。

# 右键菜单参考

本文档记录 Fika 的 pane 右键菜单行为迁移所用的 Dolphin 源码路径。

## Dolphin 源码

- `../dolphin/src/dolphincontextmenu.{h,cpp}`
  - `DolphinContextMenu` 是用于条目和 viewport 右键菜单的 `QMenu`。构造函数输入区分条目上下文和空白 viewport 上下文。
  - `addAllActions()` 检测上下文并分发到回收站条目、回收站 viewport、普通条目或普通 viewport 菜单构建器。
  - `addViewportContextMenu()` 插入 Create New、当前目录的 Open With、Paste、Add to Places、Sort By、View Mode、additional actions 和 Properties。
  - `addItemContextMenu()` 处理单条目、目录条目、多条目、Open With、默认条目 action、Copy/Move 子菜单等。
  - `addDirectoryItemContextMenu()` 在分隔符之前插入 Create New 子菜单（标题 `Create New`、图标 `list-add`）。
  - `addAdditionalActions()` 调用 `KFileItemActions::addActionsTo(..., MenuActionSource::All)`——这是 Compress/Extract 条目到达的路径。
  - `insertDefaultItemActions()` 插入 Cut、Copy、Copy Location、Paste、Duplicate、Rename、Add to Places、Move to Trash 和 Delete。
- `../dolphin/src/kitemviews/kitemlistcontroller.cpp`
  - 右键在菜单处理前取消活动的 rubber-band 选择。空白区域右键消耗事件，不创建 rubber-band。
- `../dolphin/src/dolphinmainwindow.cpp`
  - `Open in New Window` action 文本和图标 `window-new`。`Create New` 菜单标题和图标 `list-add`。

## Fika 映射

- Dolphin `DolphinContextMenu` → Fika 的 `src/ui/context_menu.rs` + `src/main.rs`（action 执行）
- `context_menu_actions()` 生成 Paste 启用状态，仅在目录条目 target 上添加 Open in New Pane。
- 空白 viewport 菜单暴露 Dolphin-aligned `Sort By`（含 Folders First/Hidden Files Last toggle）和 `View Mode` 子菜单。
- `Create New` 子菜单（`list-add` 图标）包含 Folder（`folder-new`）和 Text File（`document-new`）。
- `Open in New Window` 通过 `launch_with_systemd_user()` 以 systemd transient unit 启动单独 Fika 进程。
- Places 侧栏右键菜单暴露 Add Entry、Show Hidden Places、Hide Section、Open/Edit/Remove/Hide/Copy Location/Properties。
- 设备 Places 右键菜单：未挂载设备禁用 Open 并暴露 Mount；已挂载设备暴露 Unmount、Eject、Safely Remove（仅在 core 快照报告能力时）。
- 回收站右键菜单：空白回收站视图暴露 Empty Trash；条目暴露 Restore/Delete Permanently。排序子菜单含 Name、Original Path、Deletion Time。
- Service menu action：少量 action 直接显示在根菜单；大量时 Compress/Extract/Terminal 等保持提升，其余进入 `More Actions` 子菜单。`X-KDE-Submenu` 渲染为真实嵌套子菜单行。`TopLevel` action 优先提升。
- Ark fallback：无匹配 service action 时提供内置 `Compress...`/`Extract Here`/`Extract To...`。
- Open With 子菜单按 desktop id 去重。`Other Application...` 行打开 GPUI application chooser（`uniform_list`），按需解析可见行图标。Set Default 写回 `mimeapps.list`。
- 根菜单使用鼠标位置作为弹出锚点；子菜单在 viewport 空间不足时向左翻转。子菜单隐藏遵循 Qt 菜单过渡期模型。
- 多选右键菜单仅在所有选中条目匹配且 Exec 支持多路径（`%F`/`%U`）时提供 service action。

## 当前差距

- 在 View Mode 子菜单后实现 Icons 和 Details 视图模式。
- 完成回收站特定冲突处理和 Details 列。
- 完成可移动设备 action。
