# Fika winit/wgpu Shell 路线图

本文档是 Fika 当前 UI 主路线。

2026-06-21 决策：新 shell 主线改为 **官方 upstream `winit` master +
官方 crates.io `wgpu`**。GPUI 应用继续作为兼容实现和行为基线；SCTK
后端不再作为主路线，只保留为实验/参考后端。除非之后明确重新决策，否则不要继续把新
shell 行为迁入 `fika-sctk`。

## 决策

Fika 是 Linux-only，但这不等于必须直接维护所有 Wayland protocol 边界。近期 SCTK
spike 暴露的问题集中在首帧显示、输入坐标、DPI、popup、DnD 等 shell 层，而不是
file-manager scene 本身。继续把这些底层细节全部揽到 Fika 内部，会拖慢主线迁移。

主线应使用：

- `https://github.com/rust-windowing/winit.git` 的 `master` 分支，也就是官方
  upstream。主线不使用 Pop!_OS/COSMIC 或 iced 维护 fork。
- 官方 crates.io `wgpu` 作为渲染后端。
- 现有 Fika core 模块负责目录加载、文件操作、缩略图、MIME/Open With、Places、
  devices、Trash、portal 和 privileged helper。
- 现有 GPUI retained-model 工作和 `fika-wgpu` spike 作为迁移输入，但不能继续保留
  单文件巨型 renderer 结构。

这里使用 `winit` 是把它当作成熟 shell shim，不是为了跨平台目标。Fika 仍然可以保持
Linux-focused，同时让 upstream winit 负责窗口、事件、scale 和 surface 生命周期。

## 架构目标

```text
fika-core
  -> retained file-manager model
  -> winit shell state
  -> wgpu scene projection and GPU batches
  -> input/hit-test routing back to file-manager actions
```

Core 必须保持 UI-neutral，不能依赖 GPUI、winit、SCTK、Wayland protocol object、
`wgpu`、raw window handle 或 renderer resource。

winit/wgpu shell 负责：

- window lifecycle 和 event loop。
- surface resize、scale factor、redraw scheduling、frame pacing。
- pane、Places、overlay、context menu、dialog、chooser scene state。
- file slot、Details row、Places row、splitter、scrollbar、rubber-band、
  drag target 和 context target 的 retained geometry。
- hit-test、pointer/keyboard routing。
- draw command generation、batching、clipping、transform、invalidation。
- MIME/theme icon、thumbnail、text 和 UI asset 的 texture atlas/cache。
- frame/layout/hit-test/visible-slot/cache/atlas/thumbnail/DnD telemetry。

## 源码方向

当前状态：

- `src/bin/fika-wgpu.rs` 是 winit/wgpu prototype，现在作为新 shell 迁移来源。
- `src/bin/fika_wgpu/` 已经有少量模块拆分起点。
- `src/bin/fika-sctk.rs` 和 `src/bin/fika_sctk/` 只保留为实验/参考代码；尽量保持可编译，
  但停止承接新行为。
- `src/main.rs` 继续作为 GPUI baseline，直到 winit shell 通过主线化门槛。

预期整理：

- 把 `src/bin/fika-wgpu.rs` 拆到 `src/bin/fika_wgpu/` 的职责模块中。
- 先拆 app/window/event loop、renderer、scene、pane、Places、context menu、
  dialogs、icons、thumbnails、text、DnD 和 telemetry，再添加大功能。
- shell-only state 留在 winit shell；可复用 file-manager 行为进入 `fika-core`
  或 UI-neutral 共享模块。

## 迁移阶段

### Phase 0：路线切换

- `Cargo.toml` 使用 upstream `winit` master。
- 文档不再把 SCTK 写成唯一 shell 目标。
- SCTK 保留为实验/参考后端。
- 验证现有 winit shell 能在 upstream 分支上构建。

### Phase 1：稳定 winit shell

- `fika-wgpu` 作为 active new-shell binary。
- 重新确认启动显示、DPI、pointer 坐标、keyboard routing、scrollbar、rubber-band、
  context menu 和 location caret。
- `/etc`、repo root、大目录、split-pane 都作为 smoke 目标。
- 用 shell-native telemetry 跟踪问题，而不是继续依赖 GPUI renderer counters。

### Phase 2：拆分单文件

- 当前首要目标是 pane 复用。Primary pane 和 split pane 必须共享
  `ShellPaneState`、pane view/projection、scroll metrics、slot pool、layout adapter，
  后续继续收敛 input/action routing 边界。
- 第一批拆分已落地：`src/bin/fika_wgpu/clipboard.rs` 负责 shell clipboard wrapper；
  `src/bin/fika_wgpu/location.rs` 负责 `PathHistory`、`LocationDraft` 和地址栏编辑使用的
  UTF-8 cursor normalization；`src/bin/fika_wgpu/selection.rs` 负责 selection state、
  keyboard navigation action、click context 和 rubber-band state；
  `src/bin/fika_wgpu/pane.rs` 负责 pane kind/state/view/projection data、scroll metrics、
  split metrics 和 visible-slot pool；`src/bin/fika_wgpu/pane_layout.rs` 负责 shell layout
  enum、Compact/Details layout adapter 和 keyboard navigation target calculation。
- `ShellScene` 现在用 `primary_pane: ShellPaneState` 存储主 pane；primary/split pane
  通过同一组 `pane_state` / `pane_state_mut` 状态访问边界工作，不再保留 primary-only
  的 path/view/entries/filter/scroll 散落字段。
- 抽出 app/window/event loop、renderer、scene、pane、Places、context menu、
  dialogs、icons、thumbnails、text、DnD、telemetry 模块。
- 拆分时尽量少改行为，方便定位 regression。
- 只有当代码 shell-neutral 时，才考虑和 SCTK 共享。

### Phase 3：对齐 Dolphin pane 架构

- pane projection 对齐 Dolphin 的 model/controller/view 分层。
- hot path 使用 visible-slot virtualization、slot pool 和 retained geometry。
- Compact/Icons/Details 共享 selection、hit-test、scroll、zoom、rename、filter、DnD
  边界。
- icon/thumbnail/text 必须 visible-first，不能阻塞输入。

### Phase 4：系统集成

- 接入 Open With、service menu submenu、clipboard、file transfer、create、rename、
  trash、properties、thumbnails、devices、Places dynamic data。
- 外部 DnD 优先使用 winit 可提供的 surface/platform 能力；必要时只补窄的 Linux-specific
  支持。
- remote/GVfs 行为必须显式：不支持的本地 file operation 要安全失败。

### Phase 5：主线化

只有证据证明 winit shell 比 GPUI baseline 更适合默认入口时，才能提升为默认：

- `cargo check --locked --bin fika-wgpu`
- `cargo test --locked --bin fika-wgpu`
- Icons/Compact/Details、split pane、hidden files、location edit、scroll/zoom、
  context menu、DnD、thumbnail、devices、大目录 runtime smoke
- telemetry 证明 frame/layout/cache 表现不差于当前 baseline
- 文档更新为 `fika-wgpu` 不再是实验入口

## 依赖策略

主线 winit 依赖：

```toml
winit = { git = "https://github.com/rust-windowing/winit.git", branch = "master" }
```

- `wgpu` 继续使用官方 crates.io 依赖。
- 除非未来明确修改本路线，不再 pin 到 COSMIC/iced fork。
- 在 winit shell 足够完整可评估前，不新增第二套 window/event backend abstraction。
