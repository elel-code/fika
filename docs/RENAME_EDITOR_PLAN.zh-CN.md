> 本文是 [RENAME_EDITOR_PLAN.md](RENAME_EDITOR_PLAN.md) 的简体中文翻译。

# 重命名编辑器计划

重命名是一个文本编辑平台边界。在此行为矩阵行为完成之前，不要用自定义绘制器替换 GPUI overlay。

## Dolphin 参考

Dolphin 使用两条路径：

- `DolphinView::renameSelectedItems()` 仅在启用内联重命名且选中单个条目时启动内联重命名。它将条目滚动到可见区域，然后调用 `KItemListView::editRole(index, "text")`。
- `KItemListView::editRole()` 将该条目标记为当前项，并将编辑器委托给可见的 `KStandardItemListWidget`。
- `KItemListRoleEditor` 是一个 `KTextEdit`。它拥有文本编辑行为：
  Escape 取消，Enter 提交，Tab/Down 可提交后编辑下一项，Backtab/Up 可编辑前一项，FocusOut 除弹出窗口焦点外均提交，Home/End 和 Left/Right 选择折叠遵循 `QTextCursor`，编辑器在文本变化时自动调整尺寸。
- 多条目重命名或禁用的内联重命名使用 `KIO::RenameFileDialog`。

Dolphin 的重要经验是：内联重命名不仅仅是绘制的文本。它是一个真正的文本编辑器，具有焦点、选择、键盘操作、验证交接和平台输入行为。

## 当前 Fika 边界

Fika 目前将重命名作为 GPUI overlay 保持：

- Model 状态位于 `src/ui/rename/draft.rs` 中，作为 `RenameDraft`。
- 键盘分类位于 `src/ui/shortcuts.rs` 中，作为 `RenameInputAction`。
- 几何和渲染位于 `src/ui/file_grid/rename_overlay.rs` 中。
- App 编排在 `src/main.rs` 中，启动 draft、将点击位置映射到 caret、
  验证提交、支持特权重命名、支持 Tab 重命名下一项，并在文件监视器
  事件重命名底层条目时重新定位 draft。

这作为平台边界是可以接受的。它应该在 renderer-policy 日志中显式保留为 GPUI overlay surface。

## 行为矩阵

| 行为 | 当前 Fika | 切换到自定义编辑器前的要求 |
| --- | --- | --- |
| 单条目内联启动 | 由 `start_rename_in_pane()` 和选择检查覆盖。 | 保持条目滚动/可见性语义和聚焦 pane 所有权。 |
| 多条目重命名 | 不等同于 Dolphin；Fika 要求一个条目。 | 在声称 Dolphin 等效之前，要么保持单条目行为，要么设计对话框等效方案。 |
| 初始词干选择 | 由 `RenameDraft::new()` 覆盖。 | 保留扩展名/词干选择和 UTF-8 边界。 |
| 文本插入/删除 | 覆盖 key-char、Backspace、Delete。 | 在替换 GPUI 文本处理前添加合成感知文本输入。 |
| IME/合成 | 自定义 model 未覆盖。GPUI 仅接收 key chars。 | 必需。需要合成 start/update/commit/cancel 和标记文本渲染。 |
| Caret 鼠标命中测试 | 由布局投影和 `rename_caret_for_local_x()` 覆盖。 | 在滚动、缩放、Details 布局和长名称宽度扩展后保持正确。 |
| 文本选择 | 覆盖 Shift+Home/End/Left/Right 和全选。 | 如果自定义编辑器拥有输入，添加鼠标拖拽选择和平台级词/行选择决策。 |
| FocusOut 提交/取消 | 当前不是完整的 text-widget 合约。 | 在自定义编辑器之前定义 FocusOut 行为和弹出窗口/右键菜单异常。 |
| Escape 取消 | 已覆盖。 | 保留而不在失焦时意外提交。 |
| Enter 提交 | 已覆盖。 | 保留验证和异步操作交接。 |
| Tab 重命名下一项 | 覆盖正向重命名下一项。 | 添加 Backtab/前一项决策，或记录有意不同的行为。 |
| Up/Down 链式编辑 | 未实现。 | 决定是否需要 Dolphin Up/Down 链式编辑。 |
| 扩展名警告 | 由 `RenameDraft::extension_warning()` 覆盖。 | 保持警告位置和无警告目录行为。 |
| 验证错误 | 覆盖空名称、远程重命名、父目录缺失和目标冲突。 | 在所有条目模式中保持辅助文本而不布局重叠。 |
| 特权重命名 | 由特权 draft 路径覆盖。 | 保持 action label、状态和待定重命名下一项的清除。 |
| Watcher 重定位 | 由 draft 重定位测试覆盖。 | 保持 draft 文本/caret/选择/错误，同时原始路径变化。 |
| 无障碍访问 | 自定义绘制器未覆盖。 | 在替换真正的文本可编辑 overlay 之前必需。 |

## 迁移顺序

1. 保持 GPUI 重命名 overlay 作为默认渲染器。
2. 添加 renderer-policy 证据，证明当 draft 激活时重命名 overlay 计数可见。
3. 如果仍然需要自定义编辑器，首先设计一个覆盖 IME、焦点、鼠标选择、剪贴板、
   无障碍访问和链式编辑行为的输入 model。
4. 仅在该行为 model 有测试后才能在选择加入标志后实现自定义编辑器。
5. 与 GPUI overlay 比较正确性和响应性。如果自定义编辑器不完整或更慢，
   则保持 GPUI overlay。

## 验收门

- 单元测试覆盖 UTF-8 移动、选择、删除、扩展名警告、验证错误、
  重命名下一项、watcher 重定位和特权重命名。
- 运行时冒烟测试覆盖启动重命名、点击 caret、编辑、Escape、Enter、
  Tab 重命名下一项、错误辅助文本、编辑时滚动/缩放，以及 watcher 重定位后的重命名。
- 自定义编辑器冒烟测试必须在成为默认之前包含桌面会话中的 IME/合成。
- 如果任何自定义重命名绘制器使文本输入行为退化，即使视觉绘制路径更快，
  也不能被接受。
