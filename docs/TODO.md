# Fika TODO: GPUI Rewrite

本文档是当前唯一有效的任务板。旧的 Slint 优化任务已经归档，不再作为未来实现方向。

目标：全面替换为 GPUI，同时以 Dolphin 源码执行流作为第一参考目标。迁移不是把 `.slint`
组件翻译成 GPUI 组件，而是借迁移机会重建 pane、directory lister、model signal、undo 和
view/controller 的边界。

状态说明：

- `[x]` 已完成
- `[~]` 正在进行或部分完成
- `[ ]` 未开始
- `[!]` 阻塞项或必须先解决的决策

## Hard Rules

- [x] 当前 Slint 实现只作为旧实现和可复用 Rust 模块来源，不再继续扩展 UI 胶水。
- [x] Dolphin 是第一参考目标。目录加载、刷新、删除、rename、undo 后刷新必须先确认
  Dolphin 源码执行流，再实现 Fika 对应层。
- [x] 每个 pane 必须有稳定 `PaneId`。所有 lister、watcher、async result、selection、
  thumbnail、file operation result 都按 `PaneId + generation` 路由。
- [x] 不保留 Slint 兼容层，不保留 slot/focused-pane fallback，不保留旧 reload queue。
- [ ] 新实现不得把 UI widget identity 当作文件模型 identity。GPUI view/entity 是渲染层，
  文件身份属于 core model。

## Phase 0: Reference Freeze

- [x] 建立 Dolphin 源码参考清单并写入迁移计划。
  - 验收：`docs/GPUI_DOLPHIN_MIGRATION_PLAN.md` 包含 `DolphinView::loadDirectory()`、
    `KFileItemModel::{loadDirectory, refreshDirectory}`、KDirLister 信号连接、
    `slotItemsAdded`、`slotItemsDeleted`、`slotRefreshItems`、`slotCompleted`、
    `KItemListView::setModel()` 和 current-directory-removed 处理路径。
- [~] 给每个迁移子系统写“Dolphin 对应层”。
  - 验收：目录、view layout、selection/controller、undo、trash、devices、thumbnail
    都能在计划文档里找到 Dolphin 或 Linux desktop reference。
- [x] 冻结 Slint 文档语义。
  - 验收：`docs/DESIGN.md` 描述 GPUI 目标架构；旧 Slint 优化文档标注为归档。

## Phase 1: Repository Shape

- [ ] 拆出 core 与 UI shell 边界。
  - 目标结构：
    - `crates/fika-core`: pane、directory lister、entry model、selection、operations、trash、
      thumbnails、devices、settings。
    - `crates/fika-gpui`: GPUI app、window、pane views、menus、dialogs、input routing。
    - `crates/fika-portal`: chooser / xdp portal bridge，复用 core。
    - `crates/fika-privileged-helper`: 保留现有 helper 边界。
  - 验收：core 不依赖 GPUI、Slint 或 window 类型。
- [ ] 新增 GPUI app skeleton。
  - 验收：能打开窗口，显示 single pane shell，加载一个目录，退出干净。
- [ ] 明确旧 Slint 二进制的生命周期。
  - 验收：旧 `fika` 入口只保留到 GPUI shell 达到功能替代；不新增 Slint 功能。

## Phase 2: Dolphin-like Directory Core

- [ ] 实现 `DirectoryLister`。
  - 输入：`PaneId`、`generation`、directory path、reload flag。
  - 输出：`DirectoryModelEvent`，包括 `LoadingStarted`、`ItemsAdded`、`ItemsDeleted`、
    `ItemsRefreshed`、`ListingCompleted`、`CurrentDirectoryRemoved`、`Error`。
  - 验收：手动 refresh 和 watcher refresh 走同一条 lister event path，不发 UI reload 命令。
- [ ] 实现 `DirectoryModel`。
  - 验收：支持 Dolphin-style add/delete/refresh delta；排序、过滤、trash metadata 更新在 model
    层完成；view 只消费 model signal/snapshot。
- [ ] 实现 pane-scoped watcher。
  - 验收：每个 pane 独立 watcher/debounce/generation；关闭 pane 会 drop watcher；split open/close
    不影响其它 pane。
- [ ] 实现 current directory removed。
  - 验收：当前目录删除或 rename 后，pane 跳到最近存在 ancestor，符合 Dolphin 的
    `slotCurrentDirectoryRemoved()` 行为。

## Phase 3: GPUI Pane and View

- [ ] 建立 `PaneEntity`。
  - 验收：每个 pane 是独立 GPUI entity，持有 `PaneId`、current directory、history、
    selection、search/filter、view state 和 lister handle。
- [ ] 建立 dynamic pane tree。
  - 验收：split open/close 不复制 UI glue；每个 pane independently renders and receives input。
- [ ] 实现 compact file view。
  - 验收：先支持 Dolphin compact horizontal layout；scroll、hit-test、selection rect 与 model
    index 对齐。
- [ ] 实现 view/controller signal boundary。
  - 验收：model events 进入 view layout/controller；input 产生 controller action；UI 不直接改
    directory model。

## Phase 4: File Operations and Undo

- [ ] 迁移 `FileOperationController` 到 core。
  - 验收：copy/move/link/trash/rename/create/delete 结果只返回 affected dirs / pane ids / undo
    registration，不直接触碰 UI。
- [ ] 迁移 undo serial。
  - 验收：undo start/finish 以 serial 防 stale result；undo 完成后通过 affected panes 的 lister
    refresh，不能手动重建 view。
- [ ] 迁移 trash。
  - 验收：trash `files/` 和 `info/` 变化映射到同一个 model item；restore/permanent delete
    后走 lister event path。

## Phase 5: Interaction Parity

- [ ] Selection。
  - 验收：single、ctrl、shift range、rubber-band、Ctrl+A 都由 pane-local controller 处理。
- [ ] Context menus。
  - 验收：文件、目录、blank、Places、Devices、service-menu 都按 pane id 路由。
- [ ] Drag and drop。
  - 验收：内部 drag payload 不依赖 UI row index；drop target 由 core hit-test 决定。
- [ ] Search/filter。
  - 验收：搜索输入不抢 pane focus；filter 改变只更新 visible model/index，不重启目录 lister。

## Phase 6: Desktop Integration

- [ ] MIME/Open With/service-menu 迁移到 core/desktop module。
  - 验收：无 UI 阻塞 I/O；结果按 pane id 回到 GPUI shell。
- [ ] Devices 迁移。
  - 验收：UDisks2/mountinfo discovery 与 GPUI sidebar 解耦；mount/unmount/eject result 按 affected
    panes 刷新。
- [ ] Thumbnail pipeline 迁移。
  - 验收：thumbnail cache、failure cache、visible-first scheduling 不依赖 Slint image/model 类型。
- [ ] Portal chooser 迁移评估。
  - 验收：chooser 可以复用 GPUI shell 或保留独立二进制，但 core selection/output 共享。

## Phase 7: Cutover

- [ ] GPUI shell 达到旧功能替代门槛。
  - 必须包含：single pane、split pane、directory lister、watch refresh、undo refresh、trash、
    selection、copy/move/trash/rename、open with、context menu、thumbnail、basic devices。
- [ ] 删除 Slint UI 依赖。
  - 验收：`ui/*.slint`、`slint`、`slint-build`、Slint-specific bridge/model/update code 从主构建路径移除。
- [ ] 清理文档。
  - 验收：README、DESIGN、REFERENCE、TODO 均只描述 GPUI 架构；旧 Slint 文档只留在归档说明中。

## Immediate Next Tasks

- [x] 创建 `docs/GPUI_DOLPHIN_MIGRATION_PLAN.md`。
- [ ] 为 GPUI spike 建立单独分支或 crate，不在现有 Slint 回调层继续堆代码。
- [ ] 写 core API 草图：`PaneId`、`DirectoryLister`、`DirectoryModelEvent`、`PaneEntityState`。
- [ ] 做最小 GPUI spike：single pane + directory listing + create/delete external watcher refresh。
- [ ] 跑 split pane spike：两个 pane 打开同一路径，一个 pane refresh 不污染另一个 pane identity。
