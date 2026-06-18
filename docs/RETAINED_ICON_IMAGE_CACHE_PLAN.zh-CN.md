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
- Render conversion 按当前 layout icon size 解析 icon snapshot，不使用延迟的第二次
  icon-size commit。

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

## TODO

- [x] 在 file icon snapshot 路径旁添加 `ThemeIconImageKey` 类型。
- [x] 添加 retained same-key image storage，包含 loaded/pending/failed/stale 状态。
- [x] 让 `[fika item-image]` 区分 retained same-key theme image reuse 和 first-load
  placeholder。
- [x] 扩展 `scripts/compare-item-image-renderers.sh` 或 item-view analyzer，使配对
  default/custom 日志在 placeholder churn 或 zoom-time decode burst 时失败。
  `--gate-default-promotion` 现在会在 custom theme placeholder、theme decode
  churn 或 renderer-policy 证据无效时失败。
- [ ] 在配对证据通过且 `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 更新前，保持 GPUI
  `img()` 为默认 MIME/theme icon renderer。
