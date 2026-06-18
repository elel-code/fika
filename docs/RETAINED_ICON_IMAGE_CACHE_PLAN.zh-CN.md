> 本文是 [RETAINED_ICON_IMAGE_CACHE_PLAN.md](RETAINED_ICON_IMAGE_CACHE_PLAN.md)
> 的简体中文翻译。

# Retained MIME/Theme Icon Image Cache 计划

本计划覆盖 Compact/Icons 和 Details 中的普通 MIME/theme 图标，不替换缩略图处理。
缩略图已经使用 thumbnail-path identity 和 custom image layer；普通 theme icon
当前默认继续走 GPUI `img()` element，因为该路径目前有更好的首帧证据。

本计划的目的，是定义 `FIKA_CUSTOM_THEME_ICONS=1` 或未来 custom icon renderer
成为默认值之前必须具备的 cache 边界。

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

## 当前 Fika 边界

当前已接受的 renderer policy：

- MIME/theme icon 默认使用 GPUI `img()`，叠加在 retained item shell 上。
- Thumbnail image 使用 custom item image layer 和 retained same-thumbnail image fallback。
- `FIKA_CUSTOM_THEME_ICONS=1` 只为 A/B 证据强制 theme icon 走 custom image layer。
- 首次 icon path 解析可以使用当前 layout icon size；但同一文件图标类型已有
  resolved theme path 后，zoom 会复用该稳定 path，而不是创建另一个 exact-size path
  request。Fika 不使用延迟的第二次 icon-size 或 path commit。

缺失的是 retained same-theme-icon image cache，它让 custom theme-icon 路径获得
Dolphin 从 widget-local pixmap 和 `QPixmapCache` 中得到的稳定性。

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
可以复用上一次同 key 已加载的真实图像。

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
- `src/ui/file_grid/renderer_policy.rs`：在证据改变前保持默认 GPUI image-element 策略。

Worker orchestration 保持 visible-first：

1. Raw snapshot 决定 visible 和 read-ahead icon request。
2. `FileIconCache` 用当前 layout size 返回 cached/preliminary icon snapshot。
3. Theme path resolve 继续后台批处理，并 visible-first。
4. Image decode/load 继续由 GPUI image cache 支撑。
5. Retained image cache 记录 loaded same-key image，并暴露给 custom painter。

Prepaint 路径不得扫描 icon theme、直接读取 SVG/PNG 文件或同步 decode image data。

## 运行时证据

更改默认 renderer 前，采集配对日志：

```bash
FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-default-etc.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc > /tmp/fika-icon-custom-etc.log 2>&1

FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-default-downloads.log 2>&1
FIKA_PERF_ITEM_VIEW=1 FIKA_CUSTOM_THEME_ICONS=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika ~/Downloads > /tmp/fika-icon-custom-downloads.log 2>&1
```

未来 custom 路径只有在 analyzer output 证明以下事项后才可接受：

- key 加载过一次后没有稳态 `theme_placeholder` 抖动；
- 没有 zoom-time `theme_decoded` burst 导致可见二次尺寸跳变；
- 不存在已加载 same-key icon 退回 marker 或 blank 的帧；
- `icon_sync`、queue 和 convert phase 保持在当前 GPUI baseline 内；
- image paint 成本通过 `[fika item-image]` 可见，并保持在 static visual 预算内。

分阶段 hybrid readiness handoff 证据可以用以下命令采集：

```bash
scripts/run-retained-renderer-evidence.sh --hybrid-icons
```

这是下一步 MIME/theme icon 工作的首选 runner，因为它使用
`FIKA_HYBRID_THEME_ICONS=1` 和 `--gate-hybrid-handoff` 对比，而不是把当前仍不可提升的
full custom icon 路径强行通过 `--gate-default-promotion`。

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
- `FIKA_HYBRID_THEME_ICONS=1` 是 opt-in。该模式下，可见 MIME/theme icon 会继续走
  GPUI `img()`，直到当前精确 `(iconName, icon_size_px, scale)` key ready；ready key
  才允许 handoff 到 custom image painter。
- 默认行为不变：普通 MIME/theme icon 仍走 GPUI `img()`，除非显式设置
  `FIKA_CUSTOM_THEME_ICONS=1` 或 `FIKA_HYBRID_THEME_ICONS=1`。

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
- [ ] 只有在代码切片继续让 `/etc` 和混合用户目录通过同一 gate 后，才把默认 MIME/theme
  icon renderer policy 改成 hybrid。
- [ ] 在配对证据通过且 `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 更新前，保持 GPUI
  `img()` 为默认 MIME/theme icon renderer。
