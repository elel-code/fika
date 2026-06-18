> 本文是 [DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md](DOLPHIN_RETAINED_RENDERER_ALIGNMENT.md)
> 的简体中文翻译。

# Dolphin Retained Renderer 对齐

本文定义 Fika 中“Dolphin 对齐的自绘”到底意味着什么。它回答一个反复出现的问题：
全自绘理论上可以达到预期性能，但前提是周围的 model、cache、painter 和事件传递架构也
一起 Dolphin 化。单纯把一个 GPUI element 换成 custom painter 并不充分。

## 目标

长期目标是 Dolphin 风格的 retained view：

- 稳定 model 拥有 item/place identity、排序、selection 和 role。
- viewport 级 controller 拥有 hit testing、hover、drop targeting 和 activation
  dispatch。
- retained layout projection 拥有可见几何和 slot 复用。
- painter 只消费投影状态和缓存资产；它不解析文件 role、不扫描 icon theme、不解码 image、
  不决定 DnD 语义。
- runtime analyzer 决定 custom renderer 是否可以成为默认。

这与 GPUI 并不冲突。GPUI 可以继续作为 windowing、text、image decode 或 typed drag
bridge，同时 Fika 拥有 Dolphin 风格的 model/controller/painter 状态。只有当等价 retained
行为和性能证据都存在时，bridge 才应该消失。

## 剩余差距的根因

剩余差距并不证明 GPUI built-in rendering 天然比 retained custom renderer 快。它说明某些
surface 还没有形成完整的 Dolphin 风格闭环。

| Surface | 当前缺口 | Dolphin 对齐要求 |
| --- | --- | --- |
| MIME/theme icons | Custom image painting 可能暴露首帧 placeholder 或 ready-state 抖动。 | 使用语义 icon identity 加 size/scale/theme keyed retained image readiness。某个 key 一旦有真实图像，zoom/scroll frame 不得退回 blank 或 marker placeholder。 |
| Zoom | 任何延迟的 icon identity 或 image-size commit 都会造成可见二次跳变。 | 几何立即变化；昂贵 role/image 工作延迟或预热，绘制使用当前尺寸的稳定 icon identity。 |
| `/etc` 这类大目录 | 如果转换同步解析过多 visible role/icon work，会出现 spike。 | 保持 visible-first 有界 role work，缓存已解析 visible role，read-ahead work 不得进入 paint/convert 路径。 |
| Places | Row chrome 已自绘，但 text/icons 和部分 typed DnD bridge state 仍是 GPUI。 | 在声明完整 Places retained 行为前，event delivery 和 hit testing 必须是 viewport-level retained state。Drag start 是单独的平台边界。 |
| Drag start | GPUI 公开 API 仍通过 interactive element 暴露 typed drag start。 | 要么保留最小 shell，要么先添加/审计 retained-hitbox typed drag-start API 再移除。 |
| Rename | GPUI editor 仍提供 focus、caret、selection 和 IME 行为。 | Retained rename 必须先通过完整 editor 行为矩阵，才能默认使用。 |

## Fika 必须匹配的 Dolphin 契约

### Model And Identity

- Item identity 是 `ItemId` 加 pane-local retained slot，不是 GPUI element key。
- Place identity 是 Places slot cache 使用的投影视义 identity，不是 row element identity。
- Renderer policy 必须从 model/layout/readiness state 推导，并在作为证据时记录日志。
- Sorting、Places order、device state、selection 和 drop semantics 永远不归 painter
  所有。

### Layout And Slot Reuse

- Scroll 和 zoom 更新 layout geometry，但不重建逻辑 item state。
- Visible slot pool 在重叠滚动范围中保持 identity 稳定。
- 新进入可见区的 row/item 可以分配 slot；未变更的可见 row/item 应是 visual 或 geometry
  update，而不是 content rebuild。
- Layout projection 是 paint rect 和 hit-test rect 的唯一来源。

### Role And Asset Readiness

- Visible work 可以优先；read-ahead work 必须留在 render path 之外。
- MIME/theme icon renderer 提升要求 painter 将要绘制的精确
  `(icon_name, icon_size_px, scale, theme/color-mode)` key 已 ready。
- Thumbnail retention 继续按 thumbnail/source path keyed，而不是按 theme icon identity。
- Image decode 可以继续使用 GPUI `RetainAllImageCache`；custom rendering 不意味着必须自写
  decoder。

### Painter

- Painter 只能消费 retained snapshot、text shape 和 retained image handle。
- Painter 不能在 prepaint/paint 中同步扫描 icon theme、读取 thumbnail、检查 MIME magic
  或排队 model role 变更。
- Fallback visual 只允许出现在同 key 真实图像存在之前。同 key 图像存在后，pending/failed
  refresh 必须保留最后真实图像。

### Controller And Event Delivery

- Viewport-level hit testing 拥有 hover、cursor、activation target、context-menu target
  和 drop target selection。
- 一旦 policy 声明完整 retained events，Places row/section shell 不允许再被计为 retained
  event delivery。
- Typed drag start 与 event delivery 分开追踪，因为当前 GPUI 把它暴露为平台 bridge。

## 默认提升规则

Renderer 只有在以下条件全部满足时才能成为默认：

- Dolphin owner split 已记录：model、layout、controller/hit-test、painter、cache 和剩余
  bridge。
- GPUI baseline 和 custom 候选在相同目录、viewport action 和 mode 下比较。
- Analyzer gate 通过，且没有放宽已有检查。
- 日志没有用户可见 placeholder churn、blank first frame、icon-size 二次跳变、paint-time
  同步 decode 或 event-shell 回归。
- 相关 decision 文档记录根因、实现边界、Dolphin 对比和 `/tmp` 证据路径。

如果 custom paint 在某个 surface 上输给 GPUI，就保留 retained model/controller state，
继续让该 surface 使用 GPUI renderer。当 bridge 是显式且有证据支持时，这仍然是
Dolphin 对齐。

## 执行顺序

1. 用 `scripts/run-retained-renderer-evidence.sh --core` 冻结证据。
2. 在更改默认 icon renderer 前，完成 MIME/theme icon hybrid 证据。
3. 在声明完整 Places retained 行为前，完成 Places retained event delivery。
4. 依赖更新后重新审计 GPUI typed drag-start 支持。
5. Rename 只有在 editor 行为矩阵具有测试或 runtime smoke 覆盖后才转换。
6. 每个已接受切片都更新相关 plan/TODO，并单独提交。
