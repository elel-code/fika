# Fika winit/wgpu Shell 路线图

2026-06-21 决策：Fika UI 主线是官方 upstream `winit` `master` 加官方 crates.io
`wgpu`。默认 `fika` 二进制是当前源码树里唯一的文件管理器 UI runtime。

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

## Dolphin 对齐突破

2026-06-22：当前 shell 架构已经从“逐项即时解析”推进到更接近 Dolphin 的
item-view hot path，这是本轮性能工作的关键突破。

- MIME/icon role 以 role + size 为复用边界，不再按每个文件路径重复承担完整
  theme lookup；这对应 Dolphin `KFileItemModelRolesUpdater` 先解析 role、
  view/widget 再复用 pixmap/text cache 的分层。
- icon read-ahead 从一次性大批量扫描改为持久队列和每帧小预算，方向对齐
  Dolphin 用 event loop 分摊 pending role 的做法。
- text/icon atlas 改为子矩形上传；无 overlay 时不创建 overlay text renderer，
  让普通 compact 滚动帧只承担可见项需要的工作。
- icon theme cache 只保留命中的可渲染资源，不再长期缓存大量 negative full-path
  probe；这把 `/bin` compact 从头滚到底后的 `Private_Dirty` 从约 97.7 MB
  降到约 43.7-45.9 MB，其中 `[anon]` 从约 54.9 MB 降到约 2.9 MB。

这说明当前架构已经比之前更接近 Dolphin：复用单位是文件管理器 role 和视图资源，
昂贵工作进入队列/缓存边界，而不是在 draw path 为每个路径即时构造。剩余重点是继续
把首次可见 exact icon role lookup 从滚动/zoom 帧移出去，避免 compact 模式下新 MIME
批次进入视口时出现尖峰。

## 当前路线

- `src/main.rs` 仍是 shell 入口。
- `src/shell/` 是 shell 模块拆分目标。
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

- `cargo check --locked --bin fika`
- `cargo test --locked --bin fika`
- Icons/Compact/Details、split panes、hidden files、location editing、
  scroll/zoom、context menus、DnD、thumbnails、devices、大目录 runtime smoke。
- Telemetry 覆盖 frame time、layout time、visible slots、cache hits/misses、
  atlas pressure、thumbnails、hit tests 和 DnD state。
