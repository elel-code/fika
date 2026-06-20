> 本文是 [RETAINED_ICON_IMAGE_CACHE_PLAN.md](RETAINED_ICON_IMAGE_CACHE_PLAN.md)
> 的简体中文翻译。

# Retained MIME/Theme Icon Image Cache 计划

本计划覆盖 Compact/Icons 和 Details 中的普通 MIME/theme 图标，不替换缩略图处理。
缩略图已经使用 thumbnail-path identity 和 custom image layer；普通 theme icon
现在默认使用 retained image model 上的 full custom image layer。Painter 通过
`Window::paint_image` 绘制 `RenderImage`；image 可来自 retained pixmap-key cache、
cache-refresh 阶段的缺失 key SVG rasterization，或非 SVG resource 的 GPUI image cache。
Item 渲染不再保留逐 item 的 GPUI `img()` 子元素。

本计划的目的，是定义并守住这个 full custom 默认所需的 cache 边界。截至
2026-06-20，普通 MIME/theme icon 已移除 GPUI image-element 和 hybrid
readiness-handoff 运行时开关；`gpui_image_element=0` 是普通 pane 的必需状态。

## Dolphin 参考

Dolphin 的普通 item icon 路径是自绘的，但不是每帧 decode：

- `KStandardItemListWidget::paint()` 开头先调用 `triggerCacheRefreshing()`，
  然后才进入真正的 item paint body。
- `triggerCacheRefreshing()` 刷新 widget content、text cache 和 pixmap cache，
  再清 dirty flag。
- `KStandardItemListWidget::updatePixmapCache()` 拥有 widget-local pixmap refresh。
- `KStandardItemListWidget::pixmapForIcon()` 通过 `QPixmapCache` 查 themed pixmap。
- Cache key 包含 icon identity、请求尺寸、device pixel ratio，以及 icon
  mode/state 输入。
- Widget 保留当前 pixmap，直到 content/role/layout 变化需要刷新。

因此 Fika 等价路径是 retained pixmap identity 加自绘。缺失 pixmap key 第一次需要时
可以在 visible cache-refresh 步骤 rasterize SVG/theme 文件，这贴近 Dolphin 同步
`QIcon::pixmap()` cache-refresh 行为；防护点是已加载同 key 真实图像后不能退回
marker placeholder。

## 源码级对照与目标边界

| 层 | Dolphin 源码事实 | GPUI `img()` 源码事实 | Fika 目标 |
|---|---|---|---|
| 调度 | `KFileItemModelRolesUpdater::startUpdating()` 先同步处理 visible icons，再按 `indexesToResolve()` 排 read-ahead；滚动/缩放通过 view timer 合并 | `Img::request_layout()` 每个 element 调 `ImageSource::use_data()`，并在 loading 超过 200ms 后请求 loading element | image work 由 shared RoleUpdater/ImageResolver 批量调度；places/pane 不各自发明调度 |
| 图标生成 | `KStandardItemListWidget::updatePixmapCache()` 从 model data 取 `iconPixmap`，否则用 `iconName` 走 `pixmapForIcon()` | `ImageAssetLoader` 从 `Resource` 读 bytes，decode 到 `RenderImage`；SVG 走 `svg_renderer.render_single_frame()` | Fika 负责自绘 pixmap key、retention、budget |
| cache key | `pixmapForIcon()` key 包含 name、height、DPR、mode | `RetainAllImageCache` 按 `Resource` hash | theme icon 按 `ThemeIconImageKey` 键控；thumbnail 继续按 thumbnail path |
| 绘制 | `drawPixmap()` 只缩放/绘制当前 pixmap；hover 背景由 item widget style cache 绘制 | `Img::paint()` 最终也是 `window.paint_image()` | custom layer 和 `img()` 单次 image primitive 成本应接近；差异来自 element 数量、cache/lifecycle、visible work |
| Places/pane | Dolphin item view 和 Places 都是 model/view/delegate 闭环 | GPUI row/img element 容易把 image 和事件生命周期绑在 UI tree 上 | places 和 pane 必须共用 retained image request/result/outcome 模型，后续优化禁止分叉 |

## 当前 Fika 边界

当前已接受的 renderer policy：

- MIME/theme icon 默认使用 custom image layer。Full custom painting 由 retained
  `ThemeIconImageKey` pixmap key 和有界 cache 支撑；普通 pane 日志必须保持
  `gpui_image_element=0`。
- 可见 SVG theme-icon pixmap-key miss 会在构造 image/details visual layer 前刷新。
  这是 Fika 对 Dolphin `paint() -> triggerCacheRefreshing() -> updatePixmapCache()`
  边界的对应实现：SVG theme icon 的 paint/prepaint body 只消费 retained image。
- Thumbnail image 使用 custom item image layer 和 retained same-thumbnail image fallback。
- 普通 MIME/theme icon 当前没有 runtime renderer switch。GPUI `img()` 子元素和
  hybrid readiness handoff 只属于历史路径。
- 首次 icon path 解析可以使用当前 layout icon size；但同一文件图标类型已有
  resolved theme path 后，zoom 会复用该稳定 path，而不是创建另一个 exact-size path
  request。Fika 不使用延迟的第二次 icon-size 或 path commit。

Retained cache 必须让 custom theme-icon 路径获得 Dolphin 从 widget-local pixmap 和
`QPixmapCache` 中得到的稳定性：exact semantic key 仍区分 size/scale，且 decoded image
不会仅因为共享 source path 就跨不同 pixmap key 复用。

## Cache Key

第一版可接受的 key shape 应显式且保守：

```rust
struct ThemeIconImageKey {
    icon_name: SharedString,
    icon_size_px: u32,
    scale_bits: u32,
    theme_name: SharedString,
    color_scheme: IconColorScheme,
    mode: IconPaintMode,
}
```

说明：

- `icon_name` 是语义 MIME/theme icon 名称，不是解析后的文件路径。
- `icon_size_px` 是当前 mode/zoom 使用的真实 layout icon size。
- `scale_bits` 保留 device-pixel-ratio 差异。
- 一旦 Fika 能观察 theme 和 color scheme，`theme_name` 与 `color_scheme` 必须进入
  key；在此之前使用稳定哨兵值，而不是省略字段。
- 如果 selected/disabled 等状态会改变资源或 tint，`mode` 需要区分这些状态。
- Thumbnail image 不得使用该 key，仍按 thumbnail path 和源 metadata 建 key。

## Stored Value

Retained cache 只应保存可渲染 image state 和状态：

```rust
struct RetainedThemeIconImage {
    key: ThemeIconImageKey,
    resolved_path: Option<PathBuf>,
    image: Option<gpui::Image>,
    load_generation: u64,
    status: ThemeIconImageStatus,
}
```

具体 GPUI image 类型可以调整。关键规则是：refresh 或 decode pending 时，custom painter
可以复用上一次同 key 已加载的真实图像；不能仅因共享 source path 就跨不同 pixmap key
复用 decoded image。

状态值：

- `Loaded`：绘制 retained image。
- `Pending`：如果有 retained same-key image 就绘制它，否则绘制中性 fallback。
- `Failed`：如果有 retained same-key image 就绘制它，否则绘制中性 fallback。
- `StalePath`：新 resolved path 排队期间继续绘制 retained same-key image。

当前 cache 生命周期中，某个 key 一旦加载过真实图像，就不允许再退回文字 marker fallback。

## 所有权

Cache 应归属 file-grid 或 icon UI，而不是 `main.rs`。候选模块边界：

- `src/ui/icons/image_cache.rs`：通用 retained theme-icon image cache。
- `src/ui/file_grid/image_layer.rs`：custom paint 消费 retained image handle。
- `src/ui/file_grid/renderer_policy.rs`：拥有默认 self-painted 策略和 renderer-policy
  stats。

Worker orchestration 保持 visible-first：

1. Raw snapshot 决定 visible 和 read-ahead icon request。
2. `FileIconCache` 用当前 layout size 返回 cached/preliminary icon snapshot。
3. Theme path resolve 继续后台批处理，并 visible-first。
4. Surface build 在 image/details visual prepaint 前，对 visible SVG theme icon 运行
   retained pixmap-key cache refresh。缺失 key 的 SVG rasterization 只允许发生在这个
   refresh 步骤。
5. Image/details visual prepaint 对 SVG theme icon 只读取 retained pixmap-key cache，
   不得在绘制 prepass 中同步扫描或 rasterize SVG theme icon。
6. Retained image cache 记录 loaded same-key image，并暴露给 custom painter。

Prepaint 路径不得扫描 icon theme。只有 role/cache 层已为请求的 pixmap key 解析出具体
theme icon path 后，才允许 SVG rasterization。

## 运行时证据

更改 image renderer 或 cache policy 前，采集默认 self-painted 日志并验证当前
Dolphin 对齐不变量：

```bash
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-full-etc.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-full-downloads.log 2>&1
```

默认 full custom 路径只有在 analyzer output 证明以下事项后才能继续接受：

- key 加载过一次后没有稳态 `theme_placeholder` 抖动；
- 没有 zoom-time `theme_decoded` burst 导致可见二次尺寸跳变；
- 不存在已加载 same-key icon 退回 marker 或 blank 的帧；
- `icon_sync`、queue 和 convert phase 保持在当前 retained evidence 范围内；
- image paint 成本通过 `[fika item-image]` 可见，并保持在 static visual 预算内。

分阶段 GPUI/hybrid readiness-handoff 路径只属于历史记录。当前 runtime evidence 不得为
普通 MIME/theme icon 使用 image-renderer env switch。

2026-06-20 cold cache-refresh 对齐证据：

- 已对照本地 Dolphin 源码：
  `/home/yk/Code/dolphin/src/kitemviews/kstandarditemlistwidget.cpp`。
  相关函数是 `KStandardItemListWidget::paint()`、`triggerCacheRefreshing()`、
  `updatePixmapCache()` 和 `pixmapForIcon()`。
- 旧 Fika SVG cold 路径：
  `/tmp/fika-dolphin-icon-cache-zoom-scroll.log` 显示
  `item-image max_prepaint=7437us`，并在 image prepaint 路径中出现
  `theme_decoded=5`。
- 当前 Dolphin-style cache-refresh 路径：
  `/tmp/fika-dolphin-cold-cache-refresh-zoom-scroll.log` 显示
  `item-image max_prepaint=156us`、`theme_loaded=0`、`theme_decoded=0`、
  `theme_retained=550`、`theme_placeholder=0`，且 renderer policy 保持
  `max_gpui_image_element=0`。
- 同一批 cold work 现在单独体现在
  `image_cache_refresh_frames=12 requested=550 retained=545 loaded=5 decoded=5
  max_total=8904us`；这有意归属 cache-refresh/build 边界，而不是 image
  prepaint/draw 边界。
- 内存按“从左侧滚动到最右侧后”采样：
  `/tmp/fika-dolphin-cold-cache-refresh-scroll-end-memory-rerun.log` 和
  `/tmp/fika-dolphin-cold-cache-refresh-scroll-end-memory-rerun.mem` 显示
  `scroll_x=2303.5 max_scroll_x=2303.5`、`RSS=295372 kB`、
  `Private_Dirty=60328 kB`。该 run 的 analyzer 仍保持 `gpui_image_element=0`、
  `theme_placeholder=0`，且 `image_sources theme_retained=286`。

2026-06-18 当前 `/etc` 证据：

- 默认日志：`/tmp/fika-icon-default-etc-p16k2.log`。
- Custom 日志：`/tmp/fika-icon-custom-etc-p16k2.log`。
- 对比命令：
  `scripts/compare-item-image-renderers.sh --gate-default-promotion
  /tmp/fika-icon-custom-etc-p16k2.log
  /tmp/fika-icon-default-etc-p16k2.log`。
- 结果：gate 失败。Custom 已经通过 image layer 绘制，但仍有
  `theme_placeholder=118` 和 `theme_decoded=5`；默认 GPUI `img()` 在
  `[fika item-image]` 中没有 theme placeholder/decode churn。
- 决策：暂不提升 custom theme-icon renderer 为默认。下一步架构必须避免首帧/新尺寸
  placeholder，较可能的方向是在 visible MIME/theme icon 离开 GPUI `img()` 前先 warm
  retained image。

2026-06-18 opt-in prewarm 证据：

- Prewarm 日志：`/tmp/fika-icon-prewarm-etc-p16k2.log`，使用
  `FIKA_PREWARM_THEME_ICONS=1` 采集。
- 普通 theme icon 的 renderer policy 仍停留在 GPUI：
  `max_image_layer=0`，`max_gpui_image_element=64`。
- item-image layer 没有绘制 theme placeholder：
  `theme_placeholder=0`，`paint_count=0`，`max_paint=9us`。
- Prewarm 活动单独可见：
  `theme_prewarm_loaded=598`，`theme_prewarm_decoded=5`，
  `theme_prewarm_pending=118`。
- 决策：prewarm 保持 opt-in。它证明 retained image 可以在不暴露可见 placeholder
  fallback 的情况下预热，但仍不足以让 custom theme-icon painting 成为默认；下一切片需要
  使用 warmed readiness 判断可见 icon 何时可以离开 GPUI `img()`。

2026-06-18 hybrid readiness handoff 基础：

- `FikaApp` 现在拥有 app-level `ThemeIconImageReadiness` cache，key 与 retained image
  storage 使用同一套包含 size/scale 的 `ThemeIconImageKey`。
- custom image layer 只有在 GPUI `RetainAllImageCache` 返回真实 `RenderImage` 后才把
  theme key 标记为 ready；pending placeholder 不会污染 readiness。
- `PaneSnapshot`/`FileGridProps` 传递轻量 readiness snapshot，因此 renderer-policy
  统计、item shell 和 image layer 使用同一份决策输入。
- Hybrid 当时成为 readiness-handoff 的默认提升中间步骤。可见 MIME/theme icon 当时会继续走
  GPUI `img()`，直到当前精确 `(iconName, icon_size_px, scale)` key 或 resolved
  resource path ready；ready key/resource 再 handoff 到 custom image painter。该运行时分支已在
  2026-06-20 删除。
- 当时 `FIKA_HYBRID_THEME_ICONS=0` 会关闭 hybrid handoff，`FIKA_GPUI_THEME_ICONS=1` 会强制旧
  GPUI baseline 以便采集配对证据。

2026-06-18 hybrid `/etc` smoke 证据：

- 默认日志：`/tmp/fika-etc-zoom-scroll.log`。
- Hybrid 日志：`/tmp/fika-icon-hybrid-etc-readiness.log`，使用
  `FIKA_HYBRID_THEME_ICONS=1` 采集。
- 默认路径保持不变：`max_image_layer=0`、`max_gpui_image_element=64`，普通 theme
  icon 没有 `[fika item-image]` frame。
- Hybrid 完成 staged handoff：早期 frame 仍停留在 GPUI，同时 prewarm 报告
  `theme_prewarm_pending=118`；后续 frame 将 ready key 交给 image layer 绘制，
  `theme_loaded=396`、`theme_placeholder=0`、`theme_decoded=0`、`max_paint=383us`。
- `scripts/compare-item-image-renderers.sh --gate-hybrid-handoff
  /tmp/fika-icon-hybrid-etc-readiness.log /tmp/fika-etc-zoom-scroll.log` 已通过
  handoff-specific gate。
- 决策：readiness handoff 在机制上成立，并在这次 `/etc` smoke 中避免了可见
  placeholder/decode churn，但仍不是默认提升。该运行在滚动到新的 `/etc` 可见条目时仍有约
  24ms 的 visible-item `icon_sync` spike，混合用户目录证据也还没有补齐。

2026-06-19 path-ready handoff 更新：

- `ThemeIconImageReadiness` 现在同时记录 ready size/scale-aware semantic key 和 ready
  `Resource::Path`。
- `RetainedThemeIconImageCache` 除了按 `ThemeIconImageKey` 索引，也按 resolved path 索引
  loaded image。若 zoom 为同一个 loaded path 创建新的 size key，custom painter 会把它视为
  retained reuse，而不是新的 first-ready decode。
- 证据：`/tmp/fika-path-ready-hybrid-downloads.log` 相对
  `/tmp/fika-path-ready-gpui-downloads.log` 通过
  `--gate-hybrid-default-promotion`，且 `theme_placeholder=0`、visible
  `theme_decoded=0`。`/tmp/fika-path-ready-hybrid-etc-r2.log` 通过 handoff 部分并移除
  visible decode churn（`theme_decoded=0`），但完整 default promotion 仍因 `/etc`
  icon-sync/content-change 方差失败；该失败点不在 image handoff 路径。

2026-06-19 成对 hybrid 证据：

- Runner：`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-icons-20260619`。
- `/etc` 日志：
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-default-etc.log` 和
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-etc.log`。
- Downloads 日志：
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-default-downloads.log` 和
  `/tmp/fika-hybrid-icons-20260619-icon-hybrid-downloads.log`。
- 两组比较都通过了 `scripts/compare-item-image-renderers.sh
  --gate-hybrid-handoff` 以及更严格的 `--gate-hybrid-default-promotion`。
- `/etc` hybrid 显示 `renderer_state=hybrid-readiness-handoff`、
  `theme_loaded=444`、`theme_placeholder=0`、`theme_decoded=0`、
  `theme_prewarm_pending=52`、`max_paint=504us`；默认对照保持
  `max_image_layer=0`、`max_gpui_image_element=64`。
- Downloads hybrid 显示 `renderer_state=hybrid-readiness-handoff`、
  `theme_loaded=310`、`theme_placeholder=0`、`theme_decoded=0`、
  `theme_prewarm_pending=44`、`max_paint=378us`。
- 决策：成对证据补齐了之前缺失的混合目录部分，并通过了显式 hybrid 默认提升 gate。
  它支持了后续代码切片；最终默认值在 retained semantic cache、prewarm 和预算控制落地后
  继续推进到 full custom。

2026-06-19 默认 handoff 代码切片证据：

- Runner：`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-default-20260619`。
- Candidate 日志使用当时的默认 renderer policy，不设置 `FIKA_HYBRID_THEME_ICONS`；
  baseline 日志使用 `FIKA_GPUI_THEME_ICONS=1`。
- `/etc` 日志：
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-default-etc.log` 和
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-etc.log`。
- Downloads 日志：
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-default-downloads.log` 和
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-downloads.log`。
- 两组都通过了 `--gate-hybrid-default-promotion`，且 `theme_placeholder=0`、visible
  `theme_decoded=0`。

2026-06-19 默认 full-custom 完成证据：

- Full custom 在该切片成为 pane MIME/theme icon 默认 renderer。彼时的 GPUI baseline 和
  hybrid transition override 已在 2026-06-20 删除。
- 最终 core evidence：
  `scripts/run-retained-renderer-evidence.sh --core --skip-build --prefix
  fika-core-final-retained-v3`。
- Item 证据覆盖 Compact、Icons 和 Details：
  `/tmp/fika-core-final-retained-v3-item-etc-zoom-scroll.log`、
  `/tmp/fika-core-final-retained-v3-item-etc-icons-zoom-scroll.log`、
  `/tmp/fika-core-final-retained-v3-item-etc-details-zoom-scroll.log`。
- Analyzer summary：modes 为 `Details,Icons,Compact`，warm steady max total
  `1108us`，file-grid max build `1344us`，image max paint `373us`，warm image
  max paint `363us`，warm static visual max paint `2546us`，warm custom/details
  visual max paint `3309us`。
- Renderer policy 证明 image transition 已完成：
  `gpui_image_element=0`、`gpui_directory_drop_shell=0`、`gpui_details_header=0`。

2026-06-19 Dolphin model consolidation：

- `RetainedImageRequest` 现在统一表达 thumbnail 和 theme-icon image work，pane image layer、
  Details visual 和 Places full row visual 都通过
  `RetainedImageLayerState::load_request_or_retained_with_outcome()` 加载。
- `RetainedImageLoad` 现在携带 `RetainedImageReady`。ready 只在真实 image 存在时产生，
  pending/missing 不会污染 readiness；pane、Details、Places 都消费同一 load-ready 语义。
- Places 删除专属 `PlacesIconImageCache` 壳，sidebar keyed state 直接持有
  `RetainedImageLayerState`，与 pane custom element state 使用同一 retained image/cache 模型。
- thumbnail retained fallback 已变成有界 LRU。cache 超预算时同时清理 GPUI
  `RetainAllImageCache` resource 和 `RenderImage`，保持 Dolphin 式 memory ownership。
- Dolphin read-ahead 索引顺序移入 `ui::retained::work_order`；
  `fika_core::thumbnail_read_ahead_indexes` 已移除，避免 thumbnail deferred work 和 file-icon
  resolve 拥有两套 visible/read-ahead 规则。
- `RetainedShapeCache` 和 `RetainedSlotStats` 现在覆盖 pane 与 Places 的 retained projection。
  text shape cache 仍保留 surface-specific key，但 cache/stat 机制共用。
- thumbnail/theme 的直接 load helper 已私有化；新的 image 使用方必须构造
  `RetainedImageRequest`，确保 thumbnail/theme readiness、outcome logging 和 retained
  fallback 都走同一路径。

## TODO

- [x] 在 file icon snapshot 路径旁添加 `ThemeIconImageKey` 类型。
- [x] 添加 retained same-key image storage，包含 loaded/pending/failed/stale 状态。
- [x] 让 `[fika item-image]` 区分 retained same-key theme image reuse 和 first-load
  placeholder。
- [x] 扩展 `scripts/compare-item-image-renderers.sh` 或 item-view analyzer，使配对
  default/custom 日志在 placeholder churn 或 zoom-time decode burst 时失败。
  `--gate-default-promotion` 现在会在 custom theme placeholder、theme decode
  churn 或 renderer-policy 证据无效时失败。
- [x] 为 hybrid handoff 证据扩展成对比较 gate。`--gate-hybrid-handoff` 现在要求存在
  GPUI fallback、theme prewarm 活动、ready-key image-layer paint，且没有可见 theme
  placeholder/decode churn。
- [x] 添加 opt-in theme-icon prewarm 证据。`FIKA_PREWARM_THEME_ICONS=1` 会为
  GPUI-rendered theme icon 创建不绘制的 image layer，并报告 `theme_prewarm_*`
  计数，同时不增加 `theme_placeholder`。
- [x] 添加 app-level readiness handoff 基础。当时 renderer policy 可以接收 size/scale
  aware 的 `theme_icon_ready` 输入，hybrid 路径使用该输入，同时不改变默认 GPUI `img()` 路径。
- [x] 采集 `/etc` 和混合用户目录的 default-vs-hybrid 成对运行时证据。Hybrid 必须保持
  `theme_placeholder=0`，避免 zoom-time `theme_decoded` burst，并证明 ready-key custom
  painting 不慢于默认 GPUI image element 路径，才能考虑任何默认提升。
  2026-06-18 `/etc` 证据已通过 placeholder/decode 部分，但由于 `icon_sync` spike 和混合目录运行仍需跟进，尚未达到完整提升标准。该 runner 后来随运行时分支删除而退役。
- [x] 在切换 renderer policy 前添加更严格的 handoff/default-promotion gate。
  该 gate 证明 GPUI fallback、prewarm、ready-key handoff、没有可见 placeholder/decode
  churn、明确性能阈值和 renderer-policy 分布。
- [x] 以 hybrid readiness handoff 作为中间提升步骤，并在 retained semantic cache
  和 cache budget 通过证据后，将默认 MIME/theme icon renderer 推进到 full custom。
- [x] 在 full-custom 默认切换后更新 renderer decisions。
  `docs/ITEM_VIEW_RENDERER_DECISIONS.zh-CN.md` 现在记录默认 self-painted policy，
  以及普通 MIME/theme icon 的 GPUI image-element/hybrid runtime switch 已删除。
- [x] 统一 pane/Details/Places 的 retained image request/load 模型，并移除 Places
  专属 image cache 壳。
- [x] 将 thumbnail retained fallback 改为有界 LRU，并把 Dolphin role-updater read-ahead
  顺序集中到 `ui::retained::work_order`。
- [x] 在 pane 与 Places 之间共享 retained text shape cache/stat 机制和 retained slot delta
  stats。
- [x] 将 thumbnail/theme retained image 的直接加载入口收束到 `RetainedImageRequest` 后面。
- [x] 在最终 core evidence gate 下保持 full-custom MIME/theme icon renderer 为默认。
  未来变更必须继续保持 `gpui_image_element=0`、placeholder churn、zoom-time decode
  burst、image-paint 回归和 renderer-policy 漂移为零。
- [x] 将可见 SVG theme-icon cold load 从 image/details visual prepaint 移到
  Dolphin-style cache-refresh/build 边界。当前证据必须用
  `[fika item-image-cache-refresh]` 记录 cold decode work，并让默认 self-painted 路径的
  `[fika item-image] theme_decoded=0`。
