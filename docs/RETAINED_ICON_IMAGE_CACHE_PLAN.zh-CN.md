> 本文是 [RETAINED_ICON_IMAGE_CACHE_PLAN.md](RETAINED_ICON_IMAGE_CACHE_PLAN.md)
> 的简体中文翻译。

# Retained MIME/Theme Icon Image Cache 计划

本计划覆盖 Compact/Icons 和 Details 中的普通 MIME/theme 图标，不替换缩略图处理。
缩略图已经使用 thumbnail-path identity 和 custom image layer；普通 theme icon
现在默认使用 retained image model 上的 full custom image layer。Painter 仍使用
GPUI 的 `RetainAllImageCache -> RenderImage -> Window::paint_image` 后端，但 item
渲染不再保留逐 item 的 GPUI `img()` 子元素。

本计划的目的，是定义并守住这个 full custom 默认所需的 cache 边界。
`FIKA_GPUI_THEME_ICONS=1` 是明确的 GPUI baseline，
`FIKA_HYBRID_THEME_ICONS=1` 只保留为过渡 readiness-handoff 路径和回归调查入口。

## Dolphin 参考

Dolphin 的普通 item icon 路径是自绘的，但不是每帧 decode：

- `KStandardItemListWidget::updatePixmapCache()` 拥有 widget-local pixmap refresh。
- `KStandardItemListWidget::pixmapForIcon()` 通过 `QPixmapCache` 查 themed pixmap。
- Cache key 包含 icon identity、请求尺寸、device pixel ratio，以及 icon
  mode/state 输入。
- Widget 保留当前 pixmap，直到 content/role/layout 变化需要刷新。

因此 Fika 等价路径必须是 retained image identity 加 GPUI image cache-backed
decode，而不是在 prepaint 解码 SVG/theme 文件，也不是已加载同 key 真实图像后又退回
marker placeholder。

## 源码级对照与目标边界

| 层 | Dolphin 源码事实 | GPUI `img()` 源码事实 | Fika 目标 |
|---|---|---|---|
| 调度 | `KFileItemModelRolesUpdater::startUpdating()` 先同步处理 visible icons，再按 `indexesToResolve()` 排 read-ahead；滚动/缩放通过 view timer 合并 | `Img::request_layout()` 每个 element 调 `ImageSource::use_data()`，并在 loading 超过 200ms 后请求 loading element | image work 由 shared RoleUpdater/ImageResolver 批量调度；places/pane 不各自发明调度 |
| 图标生成 | `KStandardItemListWidget::updatePixmapCache()` 从 model data 取 `iconPixmap`，否则用 `iconName` 走 `pixmapForIcon()` | `ImageAssetLoader` 从 `Resource` 读 bytes，decode 到 `RenderImage`；SVG 走 `svg_renderer.render_single_frame()` | GPUI decode backend 保留；Fika 负责 semantic key、readiness、retention、budget |
| cache key | `pixmapForIcon()` key 包含 name、height、DPR、mode | `RetainAllImageCache` 按 `Resource` hash | theme icon 以 semantic key 为主，resolved path 只做 loaded image 复用；thumbnail 继续按 thumbnail path |
| 绘制 | `drawPixmap()` 只缩放/绘制当前 pixmap；hover 背景由 item widget style cache 绘制 | `Img::paint()` 最终也是 `window.paint_image()` | custom layer 和 `img()` 单次 image primitive 成本应接近；差异来自 element 数量、cache/lifecycle、visible work |
| Places/pane | Dolphin item view 和 Places 都是 model/view/delegate 闭环 | GPUI row/img element 容易把 image 和事件生命周期绑在 UI tree 上 | places 和 pane 必须共用 retained image request/result/outcome 模型，后续优化禁止分叉 |

## 当前 Fika 边界

当前已接受的 renderer policy：

- MIME/theme icon 默认使用 custom image layer。Full custom painting 由 retained
  semantic image key 和 GPUI retained image/decode 后端支撑；普通 pane 日志必须保持
  `gpui_image_element=0`。
- Thumbnail image 使用 custom item image layer 和 retained same-thumbnail image fallback。
- `FIKA_CUSTOM_THEME_ICONS=1` 现在等价于默认 full-custom 方向，只在测试需要显式表达意图时保留。
- `FIKA_GPUI_THEME_ICONS=1` 强制旧 GPUI `img()` baseline。
- `FIKA_HYBRID_THEME_ICONS=1` 强制 staged handoff policy，用于历史对比。
- 首次 icon path 解析可以使用当前 layout icon size；但同一文件图标类型已有
  resolved theme path 后，zoom 会复用该稳定 path，而不是创建另一个 exact-size path
  request。Fika 不使用延迟的第二次 icon-size 或 path commit。

Retained cache 必须让 custom theme-icon 路径获得 Dolphin 从 widget-local pixmap 和
`QPixmapCache` 中得到的稳定性：exact semantic key 仍区分 size/scale，但 zoom 为同一文件
创建新 size key 时，已加载的 resource path 可以被复用。

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
可以复用上一次同 key 或同 resource 已加载的真实图像。

状态值：

- `Loaded`：绘制 retained image。
- `Pending`：如果有 retained same-key 或 same-resource image 就绘制它，否则绘制中性
  fallback。
- `Failed`：如果有 retained same-key 或 same-resource image 就绘制它，否则绘制中性
  fallback。
- `StalePath`：新 resolved path 排队期间继续绘制 retained same-key image。

当前 cache 生命周期中，某个 key 一旦加载过真实图像，就不允许再退回文字 marker fallback。

## 所有权

Cache 应归属 file-grid 或 icon UI，而不是 `main.rs`。候选模块边界：

- `src/ui/icons/image_cache.rs`：通用 retained theme-icon image cache。
- `src/ui/file_grid/image_layer.rs`：custom paint 消费 retained image handle。
- `src/ui/file_grid/renderer_policy.rs`：拥有默认 full-custom 策略以及显式
  GPUI/hybrid/custom override flag。

Worker orchestration 保持 visible-first：

1. Raw snapshot 决定 visible 和 read-ahead icon request。
2. `FileIconCache` 用当前 layout size 返回 cached/preliminary icon snapshot。
3. Theme path resolve 继续后台批处理，并 visible-first。
4. Image decode/load 继续由 GPUI image cache 支撑。
5. Retained image cache 记录 loaded same-key 和 same-resource image，并暴露给 custom
   painter。

Prepaint 路径不得扫描 icon theme、直接读取 SVG/PNG 文件或同步 decode image data。

## 运行时证据

更改 image renderer 或 cache policy 前，采集默认 full-custom 与 GPUI-baseline 成对日志：

```bash
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-full-etc.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-gpui-etc.log 2>&1

FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-full-downloads.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_GPUI_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-gpui-downloads.log 2>&1
```

默认 full custom 路径只有在 analyzer output 证明以下事项后才能继续接受：

- key 加载过一次后没有稳态 `theme_placeholder` 抖动；
- 没有 zoom-time `theme_decoded` burst 导致可见二次尺寸跳变；
- 不存在已加载 same-key icon 退回 marker 或 blank 的帧；
- `icon_sync`、queue 和 convert phase 保持在当前 GPUI baseline 内；
- image paint 成本通过 `[fika item-image]` 可见，并保持在 static visual 预算内。

分阶段 hybrid readiness handoff 证据仍可用以下命令采集：

```bash
scripts/run-retained-renderer-evidence.sh --hybrid-icons
```

该 runner 使用 `FIKA_HYBRID_THEME_ICONS=1` 和 `--gate-hybrid-handoff` 对比。
它是 transitional handoff 路径的回归工具，不是架构默认值。

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
- Hybrid 成为 readiness-handoff 的默认提升中间步骤。可见 MIME/theme icon 当时会继续走
  GPUI `img()`，直到当前精确 `(iconName, icon_size_px, scale)` key 或 resolved
  resource path ready；ready key/resource 再 handoff 到 custom image painter。该路径现在
  已被 full custom 默认取代，但仍可通过 `FIKA_HYBRID_THEME_ICONS=1` 使用。
- `FIKA_HYBRID_THEME_ICONS=0` 会关闭 hybrid handoff，`FIKA_GPUI_THEME_ICONS=1` 会强制旧
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
  它支持后续代码切片把默认 renderer policy 改成 hybrid，同时对尚未 ready 的 icon key 继续保留
  GPUI fallback。

2026-06-19 默认 hybrid 代码切片证据：

- Runner：`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-default-20260619`。
- Candidate 日志使用默认 renderer policy，不设置 `FIKA_HYBRID_THEME_ICONS`；baseline 日志使用
  `FIKA_GPUI_THEME_ICONS=1`。
- `/etc` 日志：
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-default-etc.log` 和
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-etc.log`。
- Downloads 日志：
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-default-downloads.log` 和
  `/tmp/fika-hybrid-default-20260619-icon-hybrid-downloads.log`。
- 两组都通过了 `--gate-hybrid-default-promotion`，且 `theme_placeholder=0`、visible
  `theme_decoded=0`。

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
- [x] 添加 app-level readiness handoff 基础。renderer policy 现在可以接收 size/scale
  aware 的 `theme_icon_ready` 输入，`FIKA_HYBRID_THEME_ICONS=1` 使用该输入，同时不改变默认
  GPUI `img()` 路径。
- [x] 采集 `/etc` 和混合用户目录的 default-vs-hybrid 成对运行时证据。Hybrid 必须保持
  `theme_placeholder=0`，避免 zoom-time `theme_decoded` burst，并证明 ready-key custom
  painting 不慢于默认 GPUI image element 路径，才能考虑任何默认提升。
  2026-06-18 `/etc` 证据已通过 placeholder/decode 部分，但由于 `icon_sync` spike 和混合目录运行仍需跟进，尚未达到完整提升标准。首选 runner：
  `scripts/run-retained-renderer-evidence.sh --hybrid-icons`。
- [x] 在切换默认 renderer 前添加更严格的 hybrid 默认提升 gate。当前
  `--gate-hybrid-handoff` 证明 GPUI fallback、prewarm、ready-key handoff 和没有可见
  placeholder/decode churn；默认切换还需要为 item-view phase maxima、image paint、
  static visual variance 和 renderer-policy 分布定义明确性能阈值。
- [x] 只有在代码切片继续让 `/etc` 和混合用户目录通过同一 gate 后，才把默认 MIME/theme
  icon renderer policy 改成 hybrid。
- [x] 在 hybrid 默认切换后更新 renderer decisions。
  `docs/ITEM_VIEW_RENDERER_DECISIONS.zh-CN.md` 现在记录默认 hybrid policy、
  `FIKA_GPUI_THEME_ICONS=1` 作为旧 GPUI baseline，以及 `FIKA_CUSTOM_THEME_ICONS=1`
  作为 full custom 压力路径。
- [x] 统一 pane/Details/Places 的 retained image request/load/ready 模型，并移除 Places
  专属 image cache 壳。
- [x] 将 thumbnail retained fallback 改为有界 LRU，并把 Dolphin role-updater read-ahead
  顺序集中到 `ui::retained::work_order`。
- [x] 在 pane 与 Places 之间共享 retained text shape cache/stat 机制和 retained slot delta
  stats。
- [x] 将 thumbnail/theme retained image 的直接加载入口收束到 `RetainedImageRequest` 后面。
- [ ] 在未来 full-custom 运行能在 `/etc` 和混合用户目录中击败 hybrid/default gate，且没有
  placeholder churn、zoom-time decode burst、image-paint 回归或 renderer-policy 漂移前，
  保持 hybrid MIME/theme icon renderer 为默认。
