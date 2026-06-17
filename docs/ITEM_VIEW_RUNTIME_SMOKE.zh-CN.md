> 本文是 [ITEM_VIEW_RUNTIME_SMOKE.md](ITEM_VIEW_RUNTIME_SMOKE.md) 的简体中文翻译。

# Item View 运行时冒烟测试

本检查表验证 retained/custom-painted item view 迁移后单元测试无法完全覆盖的运行时行为。在任何移除 item/row shell handler、扩展 custom painter 或更改拖放路由的切片后运行。

## 自动化规则

对于每个可重复的性能或视觉稳定性报告，首先判断 app 是否可以在无人工干预下驱动该场景。如果可以，添加或复用 autosmoke 模式，将日志保存在 `/tmp` 下，让 analyzer 输出首个通过/失败信号。

默认循环：
1. 使用代表性目录重现（如 `/etc` 用于密集 MIME 图标切换，`~/Downloads` 用于混合用户文件）
2. `cargo build` 后运行匹配的 perf flag 和 autosmoke 命令
3. 运行匹配的 analyzer 脚本，在更改架构或渲染器代码前检查 phase maxima 和 renderer-policy 计数
4. 与拥有相同行为的本地 Dolphin 源码路径比较
5. 仅在相关验证命令通过且证据记录在相应设计/决策/计划文档中后提交切片

## 拖放检查

对于每种视图模式：
- 拖拽文件条目并确认预览跟随光标
- 在可见 pane 目录上悬停时确认目录高亮显示
- `FIKA_DEBUG_DND=1` 应发出 `active-item-move ... kind=Some(Directory)`
- `item-start` 无后续 active move 行说明 item-drag hover 路径未运行
- pane 条目 self-drag 通过 preview repaint 驱动 retained hit-test（GPUI 可能在 drag start 后停止向底层 pane/item 元素发送 move callback）

## 重命名检查

Compact/Icons：启动重命名，点击编辑器内部确认 caret 移到点击位置，编辑非 ASCII 文本，Tab 触发 rename-next。
Details：从 Details 行启动重命名，确认行视觉保留同时 rename overlay 接收输入。

## Perf 日志审查

启用 `FIKA_PERF_ITEM_VIEW=1`，执行：冷启动 → Compact/Icons/Details 切换 → 窗口 resize → fullscreen 切换。预期 Cold 帧显示缓存预热成本；Warm 帧 sub-millisecond。

`/etc` 当前基线：`item_view_stage_max: raw=602us icon_sync=173us queue=336us convert=426us`，`max_visible=64`。

自动化采样：`FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll` 在 app 稳定后自动应用 zoom in/out 和 scroll 动作。

## 决策门

不移除剩余 drag-start shells 除非 GPUI 暴露公开 custom-element drag-start API 或 Fika 携带经审计的 GPUI patch。如果 custom-painted surface 在稳态 perf 或行为完整性上落后于 GPUI 内置组件，保持 Dolphin-aligned retained model 并将该 surface 留在 GPUI 渲染器上。
