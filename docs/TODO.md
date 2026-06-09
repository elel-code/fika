# Fika TODO: GPUI Mainline

本文档是当前任务板。仓库已经切到单包 GPUI 主线；后续任务只应进入
`src/` 下的 core modules、`src/main.rs` 和 `src/bin/`。

状态说明：

- `[x]` 已完成
- `[~]` 正在进行或部分完成
- `[ ]` 未开始
- `[!]` 阻塞项或必须先解决的决策

## Hard Rules

- [x] Dolphin 是第一参考目标。目录加载、刷新、删除、rename、undo 后刷新必须先确认 Dolphin 源码执行流，再实现 Fika 对应层。
- [x] 每个 pane 必须有稳定 `PaneId`。所有 lister、watcher、async result、selection、thumbnail、file operation result 都按 `PaneId + generation` 路由。
- [x] 主构建路径只保留 GPUI/core package。
- [x] GPUI 从 Zed 官方仓库通过 git 依赖获取，不写 concrete crate release、branch 或 revision。
- [ ] 新实现不得把 UI widget identity 当作文件模型 identity。GPUI view/entity 是渲染层，文件身份属于 core model。

## Completed Cutover

- [x] 建立 Dolphin 源码参考清单。
  - 验收：`docs/GPUI_DOLPHIN_MIGRATION_PLAN.md` 包含 `DolphinView::loadDirectory()`、`KFileItemModel::{loadDirectory, refreshDirectory}`、KDirLister signal、model slot 和 current-directory-removed 处理路径。
- [x] 移除多包 Cargo 布局。
  - 验收：root `Cargo.toml` 是单一 package，并从 `src/` 构建 core library 和三个 binary。
- [x] 建立 UI-neutral core。
  - 验收：`fika-core` 不依赖 GPUI 或 window 类型。
- [x] 新增 GPUI app shell。
  - 验收：`fika` binary 位于 `src/main.rs`，可打开窗口、加载目录、分屏、关闭 pane、刷新目录。
- [x] 实现初版 `DirectoryLister`、`DirectoryModel` 和 pane-scoped watcher。
  - 验收：加载、刷新、watcher event 和 current-directory-removed 都走 core event path。
- [x] 保留 portal/backend 和 privileged-helper 二进制边界。
  - 验收：两个二进制从 root package 构建。
- [x] 清理旧主路径。
  - 验收：root manifest 不再引用旧 UI 构建路径；旧 UI 源目录和构建脚本已从主代码树移除。
- [x] 更新 README、DESIGN 和 REFERENCE。
  - 验收：文档描述当前 GPUI package 和剩余功能缺口。

## Directory Core

- [x] 完善 `DirectoryLister` event 分类。
  - 验收：watcher add/delete/refresh 能稳定映射到 model delta；不能分类时才整目录 reload。
- [~] 完善 `DirectoryModel`。
  - 验收：支持排序、过滤、trash metadata，并保留 stable item identity。
- [x] 实现 current-directory-removed。
  - 验收：当前目录删除或 rename 后，pane 跳到最近存在 ancestor，符合 Dolphin 的 `slotCurrentDirectoryRemoved()` 行为。
- [x] 为 directory core 增加覆盖。
  - 验收：包含 stale generation、split pane、同目录双 pane、current-directory-removed、watcher refresh 测试。

## GPUI Pane and View

- [x] 建立 GPUI pane shell。
  - 验收：pane toolbar action 全部按 `PaneId` 路由。
- [x] 建立 dynamic split pane。
  - 验收：split open/close 不复制全局 UI state；每个 pane 独立加载目录。
- [x] 接入 pane-local navigation history。
  - 验收：Back/Forward 通过 `PaneId` 路由，切换 focused pane 不会改变历史事件目标。
- [~] 建立 chooser shell。
  - 验收：支持文件/目录选择、multi-select 输出、filter/choice metadata 输出。
- [x] 实现 pane-local selection controller。
  - 验收：single select、Ctrl/secondary toggle、Shift range、Ctrl/secondary+A、select all、clear selection、方向键移动、Shift+方向键范围选择、chooser multi-select、model change pruning 和 GPUI rubber-band selection 都进入 `fika-core::PaneState`。
- [~] 实现 Dolphin compact file view。
  - 已完成：core compact layout、model-index hit-test、selection rect、rubber-band overlay 和 GPUI item rendering 使用 `src/core/view.rs` 的布局结果。
  - 剩余验收：scroll handle 同步、可见区虚拟化。
- [~] 实现 keyboard shortcuts。
  - 已完成：方向键、Shift+方向键、Ctrl/secondary+A、Ctrl/secondary+C/X/V、Ctrl/secondary+Shift+N、F2 rename、Escape、F5、Backspace、Alt+Left、Alt+Right、Delete 和 Ctrl/secondary+Z 都按 focused `PaneId` 路由到 pane-local action。
  - 剩余验收：后续新增交互继续按 pane-local action 路由。

## File Operations and Undo

- [~] 迁移 file operation primitives 到 core。
  - 已完成：create file/folder、rename、move-to-trash 和内部 Copy/Cut/Paste 都通过 GPUI 后台任务调用 core file operation primitives，并返回 affected dirs / undo payload。
  - 验收：copy/move/link/trash/rename/create/delete 结果只返回 affected dirs / pane ids / undo registration，不直接触碰 UI。
- [~] 迁移 undo serial。
  - 已完成：create file/folder、rename、move-to-trash 和内部 Copy/Cut/Paste 会记录 core undo payload 和受影响目录；Undo 取最新 serial，恢复后通过 affected panes 的 lister refresh。
  - 验收：undo start/finish 以 serial 防 stale result；undo 完成后通过 affected panes 的 lister refresh。
- [ ] 迁移 trash view。
  - 验收：trash `files/` 和 `info/` 变化映射到同一个 model item；restore/permanent delete 后走 lister event path。

## Desktop Integration

- [ ] MIME/Open With/service-menu 迁移到 core/desktop module。
  - 验收：无 UI 阻塞 I/O；结果按 pane id 回到 GPUI shell。
- [ ] Devices 迁移。
  - 验收：UDisks2/mountinfo discovery 与 GPUI sidebar 解耦；mount/unmount/eject result 按 affected panes 刷新。
- [ ] Thumbnail pipeline 迁移。
  - 验收：thumbnail cache、failure cache、visible-first scheduling 不依赖 UI image/model 类型。
- [~] Portal chooser。
  - 验收：portal backend 调用 GPUI chooser shell，并共享 core selection/output 常量。

## Documentation and Checks

- [x] README 只描述当前 GPUI package。
- [x] DESIGN 只描述当前 GPUI/core 架构。
- [x] REFERENCE 路径指向 `src/...`。
- [ ] 为 core 和 GPUI shell 补齐任务级测试。
- [ ] 持续运行：
  - `cargo fmt --all`
  - `cargo test`
  - `cargo check`
