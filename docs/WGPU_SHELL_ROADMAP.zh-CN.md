# Fika winit/wgpu Shell 路线图

2026-06-21 决策：Fika UI 主线是官方 upstream `winit` `master` 加官方 crates.io
`wgpu`。`fika-wgpu` 是默认运行目标，也是当前源码树里唯一的文件管理器 UI runtime。

## 架构目标

```text
fika-core
  -> retained file-manager model
  -> reusable pane shell state
  -> wgpu scene projection and batches
  -> input/hit-test routing back to file-manager actions
```

Core 必须保持 UI-neutral。Shell 负责窗口生命周期、scale、redraw scheduling、
retained geometry、hit-test、overlay/menu/dialog state、texture atlas、
thumbnail/text/icon scheduling 和 telemetry。

## 当前路线

- `src/bin/fika-wgpu.rs` 仍是 shell 入口。
- `src/bin/fika_wgpu/` 是 shell 模块拆分目标。
- `src/core/` 负责可复用文件管理器行为。
- `src/bin/fika-xdp-filechooser.rs` 和 `src/bin/fika-privileged-helper.rs`
  继续作为集成二进制保留。

依赖策略：

```toml
winit = { git = "https://github.com/rust-windowing/winit.git", branch = "master" }
wgpu = "29"
```

## 阶段

### Phase 1：Pane 复用

- Pane state 通过可复用 pane container 存储。
- Selection、hover、context target、scrollbar、location/filter state、
  keyboard navigation、rubber-band 和 DnD 全部按 `ShellPaneId` 路由。
- 分屏 pane 与第一个 pane 保持视觉和行为一致。

### Phase 2：拆分 Shell

- 抽出 app/window/event loop、renderer、scene assembly、pane rendering、
  Places、context menu、dialogs、icons、thumbnails、text、DnD 和 telemetry。
- 拆分时尽量少改行为，方便定位 regression。

### Phase 3：Dolphin-style Hot Path

- 热路径保持 visible-slot virtualization、reusable slot pool、retained geometry
  和 cached projection。
- Compact/Icons/Details 共享 selection、hit-test、scroll、zoom、rename、filter、
  DnD 边界。
- Icon、thumbnail、text 工作必须 visible-first，不能阻塞 pointer input。

### Phase 4：系统集成

- 接入 Open With、service-menu 图标/子菜单、clipboard、file transfer、create、
  rename、trash、properties、thumbnails、devices、Places dynamic data 和 portal
  chooser 行为。
- 外部 DnD 在必要处补窄的 Linux-specific 支持。

### Phase 5：验证

- `cargo check --locked --bin fika-wgpu`
- `cargo test --locked --bin fika-wgpu`
- Icons/Compact/Details、split panes、hidden files、location editing、
  scroll/zoom、context menus、DnD、thumbnails、devices、大目录 runtime smoke。
- Telemetry 覆盖 frame time、layout time、visible slots、cache hits/misses、
  atlas pressure、thumbnails、hit tests 和 DnD state。
