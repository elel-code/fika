# Fika TODO: upstream winit/wgpu Mainline

本文档是当前任务板。2026-06-21 起，新 UI runtime 主线改为 Linux-focused、
Fika 专用的 **official upstream `winit` master + crates.io `wgpu`** shell。
GPUI 应用继续作为兼容实现和行为/性能基线；`fika-sctk` 降级为实验/参考后端，
不再承接新的 shell 行为。

状态说明：

- `[x]` 已完成
- `[~]` 正在进行或部分完成
- `[ ]` 未开始
- `[!]` 阻塞项或必须先解决的决策

## Hard Rules

- [!] **P0：winit/wgpu 是新 shell 主线。** 新 UI runtime 工作优先进入
  `fika-wgpu` / `src/bin/fika_wgpu/`。窗口/event/scale/redraw 边界使用官方 upstream
  `winit` master；渲染使用官方 crates.io `wgpu`。不再使用 Pop!_OS/COSMIC 或 iced
  fork 的 winit 作为主线依赖。
- [x] Dolphin 是第一参考目标。目录加载、刷新、删除、rename、undo 后刷新必须先确认
  Dolphin 源码执行流，再实现 Fika 对应层。
- [x] `fika-core` 必须保持 UI-neutral。core 不依赖 GPUI、winit、SCTK、Wayland object、
  `wgpu`、raw window handle 或 renderer resource。
- [x] 每个 pane 必须有稳定 `PaneId`。所有 lister、watcher、async result、selection、
  thumbnail、file operation result 都按 `PaneId + generation` 路由。
- [~] 当前可运行默认应用仍保留 GPUI/core package；新 shell 先作为独立二进制并与
  GPUI 并存。
- [~] `fika-sctk` 仅保持为实验/参考后端。除非后续明确重新决策，不继续迁入新功能。
- [x] 直接 crates.io 依赖不使用 `*`。版本声明保持最新稳定大版本范围，不锁到 patch/minor。
- [x] 新实现不得把 UI widget identity 当作文件模型 identity。文件身份属于 core model；
  shell 的 slot、hitbox、atlas 和 draw resources 只能消费 core/retained identity。
- [x] 功能提炼与集成：Dolphin 是 UI 行为和文件操作流程的第一参考；cosmic-files 是
  纯 Rust 系统集成的参考源。两个源码库中提炼的功能统一集成到 `fika-core`，UI 层只做
  渲染和输入路由。

## Current Route

详细目标和阶段见：

- `docs/WGPU_SHELL_ROADMAP.md`
- `docs/WGPU_SHELL_ROADMAP.zh-CN.md`

当前依赖策略：

```toml
winit = { git = "https://github.com/rust-windowing/winit.git", branch = "master" }
wgpu = "29"
```

当前源码边界：

- `src/bin/fika-wgpu.rs`：winit/wgpu prototype，现在是新 shell 迁移来源。
- `src/bin/fika_wgpu/`：winit shell 模块拆分目标目录。
- `src/bin/fika-sctk.rs`、`src/bin/fika_sctk/`：实验/参考后端，只保持可编译和必要修复。
- `src/main.rs`、`src/ui/`：GPUI fallback 和行为基线。
- `src/core/`：UI-neutral domain logic，继续承接可复用能力。

## winit/wgpu Shell Work

- [x] Phase 0a：依赖从 Pop!_OS/COSMIC `winit` tag 切到官方 upstream
  `rust-windowing/winit` `master` 分支，并刷新 `Cargo.lock`。
- [x] Phase 0b：撤销未完成的 SCTK dialog 半迁移，避免 `fika-sctk` 因半成品引用阻塞构建。
- [~] Phase 0c：文档路线切换到 winit/wgpu 主线，SCTK 标记为 archived/experimental。
- [ ] Phase 1：用 upstream winit master 重新验证 `fika-wgpu` 构建和 runtime smoke：
  `/etc`、repo root、large-dir、Icons/Compact/Details、split pane、scrollbar、rubber-band、
  context menu、location caret、DPI。
- [~] Phase 2：拆分 `src/bin/fika-wgpu.rs` 单文件。已抽出
  `src/bin/fika_wgpu/clipboard.rs`（Wayland clipboard wrapper）和
  `src/bin/fika_wgpu/location.rs`（`PathHistory`、`LocationDraft`、UTF-8 cursor helper）、
  `src/bin/fika_wgpu/selection.rs`（selection、keyboard navigation、rubber-band state）、
  `src/bin/fika_wgpu/pane.rs`（pane kind/state/view/projection、scroll metrics、
  visible-slot pool）。
  下一步继续抽 app/window/event loop、renderer、pane layout、Places、context menu、
  dialogs、icons、thumbnails、text、DnD、telemetry。
- [ ] Phase 3：把 pane 做成可复用组件，并持续对齐 Dolphin 架构：visible-slot
  virtualization、slot pool、retained geometry、filtered projection、selection/rubber-band、
  scroll/zoom、rename/filter/DnD 共用边界。
- [ ] Phase 4：接入系统能力：Open With、service menu 图标和子菜单、clipboard、
  create/rename/file transfer/trash/properties、thumbnail worker、devices、Places 动态数据。
- [ ] Phase 5：外部 DnD 基本可用。优先完成 `text/uri-list` export/import、内部 pane/place
  drop target、Copy/Move/Link drop menu、drag preview/hover。
- [ ] Phase 6：主线化 gate。只有在构建、测试、runtime smoke 和 telemetry 证明
  `fika-wgpu` 比 GPUI baseline 更适合作为默认入口后，才能把默认运行目标切过去。

## GPUI Baseline

GPUI retained item-view、Places、DnD、thumbnail、MIME、Trash、Undo、location bar、
status bar、service menu、devices、network 等能力仍是当前稳定行为基线。后续 winit
迁移应复用其中已经下沉到 `fika-core` 的 domain 能力；未下沉的行为要先抽象边界，再接入
winit shell。

保留参考：

- `docs/DESIGN.md`
- `docs/ITEM_VIEW_RENDERER_DECISIONS.md`
- `docs/ITEM_VIEW_RUNTIME_SMOKE.md`
- `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md`

## Known Pending Areas

- [~] 外部 MIME 拖出：`DragExportPayload`（`text/uri-list` + `text/plain`）已构造；
  winit shell 需要实现实际 data offer/export。
- [~] 外部 MIME 拖入：Ark service/path MIME parser 和 executor 已有 core 支持；
  winit shell 需要接入多 MIME data offer。
- [ ] KDE Service Menu 高级条件：`X-KDE-Require=`、`X-KDE-ShowIfRunning=` 等。
- [ ] Trash 多存储聚合：Dolphin/KIO 的 `trash:/` 多 storage 聚合和 removable storage
  `.Trash-$uid`。
- [ ] Accessibility：winit/wgpu shell 需要单独规划可访问性边界。
