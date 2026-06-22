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
- text cache 改为 alpha mask 复用，颜色进入 text vertex/shader；text atlas 改为
  持久 R8 atlas。这样同一标签不同颜色共用一份 mask，`/bin` compact 滚到底后
  3096 个 label cache 约 9.1 MB，后续帧 `text_atlas_reused` 稳定命中。这更接近
  Dolphin `QStaticText::AggressiveCaching` 的“文本形状缓存 + 绘制资源复用”边界。
- text/icon atlas 改为子矩形上传；无 overlay 时不创建 overlay text renderer，
  让普通 compact 滚动帧只承担可见项需要的工作。
- icon theme cache 只保留命中的可渲染资源，不再长期缓存大量 negative full-path
  probe；这把 `/bin` compact 从头滚到底后的 `Private_Dirty` 从约 97.7 MB
  降到约 43.7-45.9 MB，其中 `[anon]` 从约 54.9 MB 降到约 2.9 MB。
- 后续推进把 visible exact icon role lookup 从所有 UI 预热/绘制帧移到
  pending resolver 路径；普通帧只读 exact cache 或显示 role fallback，避免滚动中
  theme lookup 回到 draw path。
- zoom/scroll 帧的 SVG icon raster miss 进入后台 worker；UI 帧优先使用 exact
  cache、相邻尺寸 cache、role-raster cache 或 generic role fallback，不再在普通
  redraw 中同步 raster SVG。这对应 Dolphin 按 role/pixmap cache 复用，而不是缩放
  时让图标短暂空掉或把 SVG raster 放回 draw path。
- icon resolver 的 pending 请求现在区分 visible/deferred 优先级，worker 会把可见
  role 请求提升到 deferred read-ahead 前处理。这让当前 viewport 的 role work
  优先于后台 warmup，更接近 Dolphin 由 event loop 分摊 pending role 的边界。
- core MIME metadata role 调度也具备同样的 visible/deferred 边界：可见 metadata
  work 会先于 deferred background work 成批处理，同一个 key 的 deferred 请求可以
  被提升为 visible，visible snapshot 也不会误删 deferred background 请求。
- winit/wgpu shell 现在已经在 prewarm/render 中使用这个 metadata 边界：可见
  MIME metadata candidates 会先于 deferred read-ahead drain，旧结果通过 pane、
  path、entry index、size 和 modified time 做保护性写回。

这说明当前架构已经比之前更接近 Dolphin：复用单位是文件管理器 role 和视图资源，
昂贵工作进入队列/缓存边界，而不是在 draw path 为每个路径即时构造。最新 debug
实测中，`/bin` compact 从头滚到底并停留末尾的 `Private_Dirty` 为 45.5 MB，
`autosmoke-scroll render_us_p50/p95/max` 约 2.17/3.78/5.94 ms，`icon_raster_us_max=0`；
`/etc` compact 快速滚动 `render_us_p95` 约 3.9 ms；compact 快速 zoom
`render_us_p95` 约 4.5 ms，`icon_raster_us_max=0`。小目录快速滚到未命中的尾部
MIME role 现在已有真实 desktop-session gate：
`scripts/run-retained-renderer-evidence.sh --metadata-tail-scroll`。当前 Icons
fixture evidence 显示 startup metadata visible/deferred queue
（`visible_total=44`、`deferred_total=128`）和 autosmoke-scroll metadata drain
（`results_total=32`、`applied_total=32`），同时 `icon_raster_us_max=0`、
`max_new_scroll_y=1693.0`。

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
- 外部文件 DnD import 已通过 winit 文件拖放事件接入；`text/uri-list` export
  和缺失的 Wayland-specific 支持后续补齐。

### Phase 5：验证

- `cargo check --locked --bin fika`
- `cargo test --locked --bin fika`
- Icons/Compact/Details、split panes、hidden files、location editing、
  scroll/zoom、context menus、DnD、thumbnails、devices、大目录 runtime smoke。
  小目录 MIME role tail-scroll 通过
  `scripts/run-retained-renderer-evidence.sh --metadata-tail-scroll` 独立 gate
  覆盖，后续应并入更完整的 runtime matrix。
- Telemetry 覆盖 frame time、layout time、visible slots、cache hits/misses、
  atlas pressure、thumbnails、metadata role prewarm/drain、hit tests 和 DnD state。
