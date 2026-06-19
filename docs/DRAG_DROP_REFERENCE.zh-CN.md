> 本文是 [DRAG_DROP_REFERENCE.md](DRAG_DROP_REFERENCE.md) 的简体中文翻译。

# 拖放参考

Fika 的拖放路径遵循 Dolphin 的 item-view controller 模型：选择和 hit-testing 属于条目控制器，文件 URL 属于 model，文件操作仅在放置目标解析后才启动。

## Dolphin 源码

- `../dolphin/src/kitemviews/kitemlistcontroller.cpp`
  - `mouseMoveEvent()` 仅在按下位置在条目上、左键仍按住、移动超过 Qt 拖拽阈值时启动条目拖拽。
  - `startDragging()` 读取选中条目，向 model 请求 mime 数据，导出 URL 到 portal，创建 `QDrag`，以 Copy 为默认 action 执行 Move/Copy/Link。
  - `dragEnterEvent()` 清除 URL 匹配缓存。`dragLeaveEvent()` 停止自动激活和自动滚动。`dragMoveEvent()` 更新悬停条目。`dropEvent()` 停止拖拽状态并发出条目或空白 viewport drop。
- `../dolphin/src/kitemviews/kfileitemmodel.cpp`
  - `createMimeData()` 将选中 model 索引转换为 URL 列表，跳过父目录已包含的子项。
- `../dolphin/src/views/dolphinview.cpp`
  - `dropUrls()` 应用 `KIO::DropJobFlags`，调用 `DragAndDropHelper::dropUrls()`。
- `../dolphin/src/views/draganddrophelper.cpp`
  - `dropUrls()` 拒绝无操作 self-drop，处理 Ark 拖拽 MIME 类型，否则启动 `KIO::drop()` job。
- `../dolphin/src/panels/places/placespanel.cpp`
  - `dragMoveEvent()` 拒绝外部拖拽的非可写 place 目标，仍允许内部 place 重排。

## Fika 映射

- Dolphin `KItemListController` → Fika 交互层：rubber-band/选择/拖拽入口在 `src/ui/rubber_band.rs` 和 `src/ui/drag_drop.rs`。
- Dolphin item drag → GPUI `on_drag` 在 `src/ui/file_grid/interaction.rs` 上，将条目模型数据打包为 `ActiveItemDrag`。
- 内部拖拽载荷：`FileDragPayload` 携带选中条目路径和 MIME 类型。GPUI `ExternalPaths` drop 通过相同目标解析路径接入。
- DnD 状态在 `src/ui/drag_drop/state.rs`：hit-test 值、目标类型、悬停可视化、放置操作菜单。
- 放置目标解析：空白 pane → 当前目录；目录条目 → 该目录；面包屑段 → 该路径；Places 行和插入线。
- 放置操作菜单支持 Copy、Move、Link、Cancel，带 `DropOperationMenu` 覆盖层。
- 目录 drop 目标和 Places 行使用专用 drop-target 样式；Places 插入使用独立行指示器。
- PlaceDrag 可重排主 Places 条目，不触发文件操作。重排目标限制在主 Places 块内。
- Ark 拖拽提取 MIME 解析在 core 中作为 `ark_dnd_extract_payload()`。需要 GPUI/backend 多 MIME offer 路径才能到达此 executor。

## 当前实现状态

稳定行为：内部 item/place 拖拽（pane↔pane、pane↔Places），GPUI `ExternalPaths` 外部 drop，Copy/Move/Link drop menu，目录 drop target 琥珀色高亮，Places 插入线 bookmark insert/reorder，精确 leave 清理，3s lease timeout 兜底。

同窗口拖拽期间，pane viewport ownership 优先。如果 retained Places DnD layer 收到一个 drag move，但该 window position 位于任意 pane viewport 内，它必须只清自己的 Places target，在 `FIKA_DEBUG_DND=1` 下输出 `places-dnd-defer-to-pane`，并让 pane retained hit-test 保持或更新 item drop target。stale Places target 不允许对 pane 坐标通过 `can_drop`。

正在处理：`DragExportPayload`（`text/uri-list` + `text/plain`）已构造，但 GPUI/backend 尚未提供从 app 内部 drag source 向外部应用发布 MIME 的 API。

## 剩余工作

- 添加超出 GPUI `ExternalPaths` 的任意外部 MIME offer 后端路径（包括多 MIME offer）。
- 将 `DragExportPayload` 接入未来 GPUI/backend drag-source MIME 发布 API。
- 将 Ark DnD service/path MIME offer 接入 core parser/executor。
