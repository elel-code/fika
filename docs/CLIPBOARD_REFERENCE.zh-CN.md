> 本文是 [CLIPBOARD_REFERENCE.md](CLIPBOARD_REFERENCE.md) 的简体中文翻译。

# 剪贴板参考

Fika 的剪贴板路径遵循 Dolphin 的文件剪贴板语义，同时使用 GPUI 的公开剪贴板 API。

## Dolphin 源码

- `../dolphin/src/views/dolphinview.cpp`
  - `cutSelectedItemsToClipboard()` 构建选中条目的 `QMimeData`，用
    `KIO::setClipboardDataCut()` 标记为剪切，为 portal 导出 URL，
    然后写入 `QApplication::clipboard()`。
  - `copySelectedItemsToClipboard()` 构建相同的选择 MIME 数据并写入，
    不带剪切标记。
  - `pasteToUrl()` 将 `QApplication::clipboard()->mimeData()` 传递给
    `KIO::paste()`，并监听创建条目和复制 job 信号。
  - `selectionMimeData()` 将选中的 model 索引委托给文件条目 model。
- `../dolphin/src/kitemviews/kfileitemmodel.cpp`
  - `KFileItemModel::createMimeData()` 将选中的条目索引转换为 URL 列表和
    `mostLocalUrl()` 列表。
  - 跳过父目录已包含在 MIME 载荷中的子项。
  - `KUrlMimeData::setUrls()` 是最终的文件列表 MIME 写入器。
- `../dolphin/src/kitemviews/private/kfileitemclipboard.{h,cpp}`
  - 从剪贴板 MIME 数据跟踪活动的剪切集合。
  - 通过 KDE/KIO helper 使用 `application/x-kde-cutselection`。

## GPUI 源码

- `gpui/src/platform.rs`
  - `ClipboardItem` 存储字符串、图像和 `ExternalPaths` 条目。
  - `ClipboardItem::new_string_with_metadata()` 在单个字符串条目上保留
    app-local metadata。
  - `ClipboardEntry::ExternalPaths` 表示平台提供的路径列表。
- `gpui/src/app.rs`
  - `App` 暴露 `read_from_clipboard()` 和 `write_to_clipboard()`。
  - Linux 和 FreeBSD 构建还暴露主选择区读/写 API。
- `gpui_linux/src/linux/wayland/clipboard.rs`
  - Wayland 后端通过 `TEXT_MIME_TYPES` 提供文本 MIME 类型。
  - 将 `FILE_LIST_MIME_TYPE` 定义为 `text/uri-list`，但普通剪贴板读取
    当前仅接受其中暴露的允许文本 MIME 列表。
  - app 可见的发送路径序列化 `ClipboardItem::text()`。
- `gpui_linux/src/linux/wayland/client.rs`
  - 剪贴板和主选择区写入从 GPUI 剪贴板条目创建 Wayland 数据源。
  - 拖放数据提供接受 `text/uri-list` 并将接收到的路径转换为 GPUI `ExternalPaths`。
- `gpui_linux/src/linux/x11/clipboard.rs`
  - 普通剪贴板读取优先使用图像/文本目标，不请求 `text/uri-list` 目标作为
    文件列表 `ExternalPaths` 条目。

## Fika 映射

- Core 文件剪贴板数据位于 `src/core/clipboard.rs`。
- `FileClipboardRole` 镜像 Dolphin 的复制/剪切状态。
- `encode_file_clipboard_text()` 写入文件 URI 列表。剪切载荷包含 Fika metadata
  标记，解码器也接受常见的 `copy`/`cut` 首行标记。
- `decode_file_clipboard_text()` 接受 `file://` URI-list 文本和普通绝对路径。
- `ClipboardState` 在 `src/ui/clipboard.rs` 和 `src/ui/clipboard/state.rs`
  中将 core 载荷桥接到 GPUI `ClipboardItem`。
- 复制和剪切将载荷写入 GPUI 剪贴板，在 Linux/FreeBSD 上也写入主选择区。
- 粘贴首先导入 GPUI 剪贴板，然后在 Linux/FreeBSD 上导入主选择区。
- 中键粘贴仅使用主选择区：空白 pane 空间粘贴到当前目录，
  对目录条目中键点击粘贴到该目录，不回退到普通剪贴板。
- URI-list 载荷作为文件传输粘贴。纯文本载荷作为新 `Pasted Text.txt` 文件粘贴，
  使用与文件创建相同的 keep-both 命名路径。
- 粘贴结果处理为复制/移动的文件记录传输 undo，为粘贴的文本文件记录创建 undo。

## 已知 Dolphin/KDE 剪贴板限制

- Dolphin/KDE 文件复制将选中的 URL 作为剪贴板 MIME 数据发布，包括
  `text/uri-list`，并在 KDE 特定的 MIME metadata 中保持剪切/复制状态。
- 当前 GPUI Linux 剪贴板读取不暴露剪贴板目标列表，也不在普通粘贴路径中
  将 `text/uri-list` 剪贴板数据转换为 `ClipboardEntry::ExternalPaths`。
- 当 Dolphin 文件复制提供同时也暴露纯文本目标时，GPUI 可以将该文本返回
  给 Fika。Fika 然后将其视为普通文本粘贴并创建 `Pasted Text.txt`，
  因为它看不到隐藏的文件列表 MIME 目标。
- 正确修复需要 GPUI Linux 后端工作：在剪贴板粘贴的通用文本之前读取
  `text/uri-list`，将文件 URL 转换为 `ExternalPaths`，并暴露足够的 metadata
  以保持 KDE 剪切/复制状态。Fika 不应在树内修补 vendored GPUI checkout；
  将其作为上游/后端任务，除非有意采用本地 GPUI fork。

## 剩余协议工作

- GPUI 当前的公开剪贴板 API 不允许 Fika 从 app 代码显式发布带有
  `text/uri-list` 和 `text/plain` 的多条目 Wayland 数据源。
- GPUI 的 Linux 剪贴板读取路径当前在文件列表 MIME 类型之前读取图像/文本
  MIME 类型；提供 `text/uri-list` 的对等方在后端支持之前，
  Fika 无法直接将它们作为文件传输导入。
- 拖放路径列表提供现在作为 GPUI `ExternalPaths` 到达，并连线到 Fika 的 pane
  文件操作管线。任意非路径或多 MIME 拖放提供在后端暴露之前，
  仍无法与相同的 `FileClipboardPayload` 模型统一。
