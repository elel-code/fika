> 本文是 [ITEM_VIEW_CUSTOM_PAINT_DESIGN.md](ITEM_VIEW_CUSTOM_PAINT_DESIGN.md) 的简体中文翻译。

# GPUI 条目视图自定义绘制设计

> 本文是 [ITEM_VIEW_CUSTOM_PAINT_DESIGN.md](ITEM_VIEW_CUSTOM_PAINT_DESIGN.md) 的简体中文翻译。

> 状态：活跃计划。本文档取代了旧的 Slint slot 复用计划，针对当前 GPUI
> mainline 工作。历史 Slint 笔记保留在
> `docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md`。

## 目标

Fika 条目视图应收敛到 Dolphin 的 `KItemListView` 模型：

- model identity 属于 `DirectoryModel` / `ItemId`
- layout identity 属于 Rust 侧投影和可见 slot 状态
- UI hitbox 是稳定的交互表面
- 静态条目视觉通过自定义绘制实现，而非重建为 GPUI 子元素树
- 缩略图图像由保留的内容级图像层绘制，该层由 GPUI 的图像缓存支持；MIME/主题图标默认使用 GPUI `img()` 元素叠加在保留条目 shell 上，因为该路径目前有更好的首帧加载证据
- 重命名编辑器和拖拽启动可以保持为专门的 GPUI 子路径，直到它们的平台契约可被替换

实际目标不仅仅是更低的延迟。目标是构建一个保留条目视图，其中调整大小、滚动、选择、悬停和元数据更新都能基于缓存数据修补稳定状态并进行绘制。

自定义绘制是一种实现技术，而非架构边界。Fika 必须保持 Dolphin 在 model、layouter、controller/hit testing 和 painter 之间的划分，即使 GPUI 内置元素在特定表面上仍然更快。每次自定义绘制扩展都需要来自 `FIKA_PERF_ITEM_VIEW=1` 日志和渲染/构建计时的性能证据。如果 GPUI 内置路径在某个表面上可测量地更快或更简单，则保留该表面在内置路径上，直到有保留状态或行为需求证明移动它是合理的。

决策规则：

- Dolphin 风格的 model 架构是强制性的；GPUI 元素 identity 不得成为文件条目 model、布局 model 或 controller 状态。
- 自定义绘制是针对保留条目状态的渲染器选择，本身不是目标。
- 如果自定义绘制更慢、更不可靠或更难保持行为完整性，则保持 Dolphin 对齐的保留 model，并使用 GPUI 内置元素渲染该表面，直到有更强的证据或更窄的迁移范围。

当前替换矩阵见 `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.zh-CN.md`。Places chrome 默认
之后的执行入口见 `docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md`。

## 架构契约

迁移是 model 优先的。渲染器选择是刻意可替换的。

| Dolphin 概念 | Fika 所有者 | 约束 |
| --- | --- | --- |
| `KFileItemModel` 角色和条目 identity | `DirectoryModel`、`ItemId`、可见快照 | GPUI 元素不得定义条目 identity 或角色状态。 |
| `KItemListViewLayouter` 几何 | pane 布局投影、可见范围、slot 池 | 布局变更修补保留几何，而非重建业务状态。 |
| `KItemListController` hit testing 和 DnD 状态 | viewport 保留 hit testing 和 `drag_drop` 状态 | 绘制器代码不得决定选择、菜单、放置或传输行为。 |
| `KItemListWidget` 复用 | 视觉 slot 池和保留绘制快照 | Slot id 是可复用的视觉实例，而非 model 索引。 |
| 条目绘制器 | GPUI 内置元素或基于保留快照的自定义 GPUI 绘制器 | 对每个表面使用更快且行为完整的渲染器。 |

渲染器策略：

- 当 GPUI 拥有硬平台契约时优先使用 GPUI 内置元素，例如文本编辑、公共拖拽启动，或优于自定义层的图像/缓存路径。
- 仅当保留快照减少了每帧元素工作且 `FIKA_PERF_ITEM_VIEW=1` 证据支持时，才优先使用自定义绘制。对于渲染每一帧时改变形态的文件图标和导航交互，自定义绘制为文件管理器绘制带来了 Dolphin 风格的内聚性。
- 更改渲染器时需要日志证据，而非仅凭架构偏好。两个渲染路径都可以读取相同的 `ItemPaintSlotCache` 和 model/布局投影；只有渲染器所有者不同。因此在得到测量结果之前，没有理由在没有证据的情况下转换表面，也没有理由给任一渲染器增加外观负担。
- 除非替换物在冷加载、缩放、内存和 SVG/主题行为方面全面优于它，否则不要替换 GPUI 自有的图像路径。
- Places 渲染器：GPUI 保持，直到有 Places 特定的保留绘制器计划和基线被捕获。

有疑问时，先对齐所有者，再对齐渲染器。

GPUI 调度依赖说明：

- 2026-06 依赖更新到 Zed/GPUI `e4f6742a` 后，且当前基线位于 Zed/GPUI
  `69b602c797a62f09318916d24a98c930533fbdc8`，`async-std` 和
  `async-global-executor` 已不在 Fika 的依赖树中。这不表示 Linux/平台栈完全没有
  async 支持 crate：`smol` 仍通过 `gpui_linux` 进入，`async-channel` 仍通过
  GPUI/platform/accessibility/zbus 路径出现。
- 条目视图 UI 工作继续使用 GPUI 的 `cx.spawn()` 和 `cx.background_spawn()` 边界，
  worker 编排由对应的 file-grid 或 places facade 拥有。不要因为旧 runtime crate
  消失，就把 item-view worker scheduling 的应用级所有权重新放回 `main.rs`。
- 文件操作和特权/后台任务继续使用 `docs/OPERATION_RUNTIME_REFERENCE.md` 中记录的显式
  operation runtime；这与 GPUI UI-frame 工作是分开的应用 I/O 边界。

## Dolphin 参考

相关 Dolphin 流程：

- `KItemListView::setGeometry()` 更新 layouter 尺寸，然后调用布局。
- `KItemListView::doLayout()` 复用 `KItemListWidget` 实例并更新几何/属性。
- `KItemListViewLayouter::updateVisibleIndexes()` 计算可见索引而不重建 widget。
- `KFileItemModelRolesUpdater::updateVisibleIcons()` 在可能时在绘制前准备可见条目角色。

Fika 等价实现：

- `raw_file_grid_snapshot()` 和 `pane_layout_projection()` 拥有 model/布局投影。
- `src/ui/file_grid/surface.rs` 拥有 viewport/层/shell 组装，而非 model 投影或绘制器内部。
- `VisibleItemSlotPool` 拥有稳定的视觉 slot identity。
- `VisibleItemSnapshotCache` 拥有稳定的每个条目内容。
- 自定义绘制的条目视觉消费快照并绘制 quads/文本/缩略图图像；GPUI `img()` 主题图标仍然消费保留条目快照。

## 当前角色/更新策略

当前 GPUI 路径遵循 Dolphin 的角色更新器划分，而不是在绘制器内部解析文件角色：

- `raw_file_grid_snapshot()` 计算可见几何和有界工作范围。它是可见索引和预读索引的真实来源。
- `queue_visible_model_work_for_raw_grid()` 在渲染转换之前排队元数据角色、缩略图探测和文件图标主题解析。
- `FileIconCache::cached_or_preliminary_icon_for()` 是渲染帧图标路径。在缓存未命中时，它返回内存中的初步/后备快照，不扫描图标主题目录。
- 文件图标主题路径解析通过后台批处理运行，排序方式与 Dolphin 的 `KFileItemModelRolesUpdater::indexesToResolve()` 类似：可见文件、可见目录、之后预读、之前预读。
- 当图标解析结果到达时，可见条目快照缓存被失效，以便下一帧可以用已解析的主题图像替换初步图标。
- 缩略图角色成功/失败保持 model 驱动，图像绘制层仅在已尝试相同缩略图的保留图像后才绘制后备。MIME/主题图标默认使用 GPUI `img()` 元素叠加在保留条目 shell 上；`FIKA_CUSTOM_THEME_ICONS=1` 仍可通过自定义图像层强制主题图标以获取 A/B 证据，但这不是默认渲染器。
- 缩放镜像 Dolphin 的普通图标绘制路径：条目几何立即更改，MIME/主题图标快照根据当前布局图标尺寸解析，正如 `KStandardItemListWidget::pixmapForIcon()` 使用当前 style-option 图标尺寸。Dolphin 的 300ms `triggerIconSizeUpdate()` 计时器是预览/角色更新器边界，不得为 Fika 主题图标创建延迟的第二次尺寸提交。

这意味着滚动和缩放帧绝不能同步执行主题图标路径查找、MIME magic 读取、缩略图探测、主题图标文件解码或大型图像工作。它们可以消费已投影的 model 角色、缓存图像、保留形状数据、保留同源图像和初步视觉后备。

## 架构边界

### Model 层

由 core 和 snapshot 代码拥有：

- `ItemId`
- 路径、文件类型、MIME、缩略图角色
- 选择/放置状态
- 重命名 draft 状态
- 布局矩形和可见条目范围

此层不得依赖 GPUI 元素 identity。

### Slot 层

由 `src/ui/file_grid` 拥有：

- 可见条目的稳定 slot id
- 从 `ItemId` 到 slot 的映射
- 保留绘制内容
- 保留视觉状态（选择、放置目标、悬停）
- 可选的文本形状缓存
- 可选的后备图标绘制缓存

Slot id 不是 model 索引。它是可复用的视觉实例 id。

### 绘制层

自定义绘制的静态条目视觉应绘制：

- 条目背景、悬停/选择/放置色调
- 后备图标背景和标记
- 条目名称文本行
- 未来的元数据叠加层
- 渲染器策略路径为自定义图像层的缩略图/图像 quads

绘制层可以使用：

- `Window::paint_quad`
- `WindowTextSystem::shape_line`
- `ShapedLine::paint`
- `Window::paint_image`，使用由 pane 本地 `RetainAllImageCache` 加载的 GPUI `RenderImage` 值

绘制层不得：

- 执行文件系统 I/O
- 解析 MIME
- 分配每帧业务 identity
- 决定选择或 DnD 行为

### 交互层

暂时为每个可见条目保留一个 GPUI `Div` 用于：

- 稳定的 `id(("item-slot", slot_id))`
- 非重命名拖拽源，因为 GPUI 缺少公共自定义元素拖拽启动 API
- 重命名悬停/光标/输入，直到重命名移动到叠加层边界

Viewport 级 hit testing 对于普通点击、右键菜单、中键点击、rubber-band 框选和放置目标路由保持权威。

Pane 内部条目拖拽悬停不由 GPUI 每元素 `on_drag_move` 拥有。运行时证据显示条目自拖拽可以成功启动，但后续元素拖拽移动回调不会被传递。因此，保留交互层在存在 `ActiveItemDrag` 时安装窗口鼠标跟踪器。该跟踪器将当前窗口位置路由通过保留 pane hit-test，并更新由 Places 到 pane 和外部路径放置使用的相同 `ItemDropTarget` 状态。当平台/后端在同一窗口活动条目拖拽期间不传递底层移动回调时，GPUI 拖拽预览重绘使用当前窗口鼠标位置运行相同的保留 hit-test 更新。GPUI 条目 shell 仅负责拖拽启动和预览所有权。

#### 同窗口条目拖拽悬停根本原因

2026-06-17 运行时追踪隔离了 pane 内部悬停缺失：

- `item-start` 被发出，因此 GPUI 拖拽启动 shell 创建了 `ItemDragPayload` 并且 Fika 填充了 `ActiveItemDrag`。
- 失败的构建中没有 `active-item-move via=window` 或 viewport `on_drag_move::<ItemDrag>` 路径跟随，因此保留 hit-test 状态在光标移动到 pane 条目上时未被刷新。
- 添加预览重绘后备后，同一次拖拽产生了连续的 `active-item-move via=preview` 行。目标从 `kind=Some(Pane)` 更改为目录保留几何处的 `kind=Some(Directory)`，证明 hit testing 和放置目标状态在有可靠的活动拖拽 tick 到达时是正确的。

因此具体原因不是过时的条目几何、目录拒绝或放置目标绘制。它是事件源：在同窗口 GPUI 条目拖拽期间，底层的 pane/条目拖拽移动回调可能在拖拽启动后不被传递。拖拽预览仍然被重绘以跟随指针，因此它目前是 pane 自拖拽悬停的稳定运行时 tick，直到 Fika 在自定义元素中拥有拖拽启动或 GPUI 暴露更强的活动拖拽回调。

重命名条目保留现有编辑器子树。在阶段 8 之前，缩略图和主题图标条目使用 pane 本地图像缓存下的 slot 稳定保留 `img()` 元素。阶段 8 将缩略图移动到自定义绘制层之后，而 MIME/主题图标现在默认回到 GPUI `img()` 元素，因为自定义主题图标层在 `/etc` 日志中显示了首帧加载占位符抖动。

当前 Compact/Icons 条目 shell 位于 `src/ui/file_grid/item_shell.rs`，不再包含每条目静态文本视觉子元素。它们是透明的拖拽启动/重命名边界；基础视觉和缩略图由内容级自定义绘制层拥有，MIME/主题图标 `img()` 子元素是覆盖保留条目状态的显式渲染器策略桥梁。

## 迁移阶段

### 阶段 0：基线和文档

- 记录当前计划和验收标准。
- 将性能日志保留在 `FIKA_PERF_ITEM_VIEW=1` 之后。
- 保留当前针对拖拽、重命名、viewport 调整大小、快照缓存的测试。

### 阶段 1：静态后备视觉画布

用自定义绘制视觉元素替换非重命名后备图标静态视觉子元素：

- 后备图标 + 文本一起绘制
- 真实主题图标路径保持为缓存图标子路径，直到图像绘制所有权被审计
- 缩略图路径保持为 `img()` 子路径
- 重命名路径保持为编辑器子树
- 每条目拖拽表面保持为一个 `Div`

验收标准：

- `cargo test` 通过
- Compact/Icons 后备静态条目的可见行为不变
- `file-grid build` 稳定路径不应在用户性能日志中回退

### 阶段 2：文本形状缓存

将图标/compact 条目文本形状移入 pane 本地缓存，键为：

- `ItemId`
- 显示的行
- 选择/文本颜色
- 宽度/高度
- 视图模式
- 字体大小和行高

验收标准：

- 相同可见条目调整大小时复用形状文本
- 模式切换冷路径与调整大小分开测量
- 文本缓存在重命名、缩放、字体/样式更改时失效

### 阶段 3：绘制 Slot 状态

引入显式的保留 slot 绘制状态：

- `ItemPaintSlot`
- `ItemPaintContent`
- `ItemPaintGeometry`
- `ItemPaintVisualState`
- `ItemPaintSlotCache`

渲染函数应在构建 GPUI 元素之前将可见快照投影到 slot 绘制状态中。

验收标准：

- 稳定的可见条目在调整大小/滚动重叠期间保持 slot id
- 选择/放置更改仅修补受影响 slot 的状态
- 悬停进入/离开修补视觉状态而不更改保留内容
- 目录本地插入/删除不重建不相关的内容缓存

### 阶段 4：缩略图/图像绘制集成

在图像所有权明确后替换缩略图 `img()` 子树：

- GPUI 的路径/URI `ImageSource` 加载器保持 crate-private，因此直接 `Window::paint_image` 需要 Fika 拥有文件读取、图像格式检测、解码、失效和渲染图像生命周期。
- 当前边界为每个 pane 保留一个自定义图像绘制层。该层在内部拥有 pane 本地 `RetainAllImageCache` 状态，而不是依赖文件网格根 `image_cache(retain_all(...))` 提供者为子 `img()` 元素。
- 直接图像绘制仍可复用 GPUI 的公共 `RetainAllImageCache`、`ImageAssetLoader`、`RenderImage` 和 `Window::paint_image` API。Fika 仅在 GPUI 的缓存契约被证明不足时才重新实现解码/失效。

验收标准：

- 缓存的缩略图仍在第一个相关帧上显示
- 缩略图失败和失效保持 model 驱动
- 绘制中无同步图像解码
- 图像缓存状态是 pane 本地的，按语义图像源键控，而非按瞬态 GPUI 子元素顺序

### 阶段 5：自定义元素

如果需要，用专用的自定义 GPUI 元素替换 `canvas` spike：

- 显式的 layout/prepaint/paint 状态
- 可选的 hitbox 插入，用于未来每条目交互整合
- 针对形状/绘制/缓存命中计数的直接测量

当前边界：

- 静态后备视觉使用 `StaticItemVisualLayerElement` 而非 `gpui::canvas`
- 该层拥有 prepaint/paint 状态并报告 pane 本地聚合计时
- 条目交互仍保留在外部 shell 上，而绘制器边界正在迁移

验收标准：

- 除交互 shell 外，没有普通静态条目子元素树
- 自定义元素拥有所有静态条目绘制
- 测试覆盖几何数学和缓存失效

### 阶段 6：Pane 级静态视觉层

将静态后备条目绘制从每条目元素提升到 Compact 和 Icons 的一个内容级层：

- 从保留的 `ItemPaintSnapshot` 值构建过滤后的静态绘制列表
- 在一个自定义元素中绘制所有非重命名、非缩略图、非主题图标的后备条目
- 将每个条目 slot 保留为透明的交互和拖拽 shell
- 将缩略图图像、主题图标渲染器和重命名路径保留为专门的子路径

验收标准：

- 静态后备 Compact 和 Icons 视觉不再为每个条目分配一个自定义元素
- 选择/悬停/放置视觉更改通过保留条目绘制状态投影到层中
- 图像和重命名条目继续使用其现有路径
- 测试证明只有后备静态条目进入该层

### 阶段 7：非重命名基础视觉和图像层

将所有非重命名 Compact 和 Icons 基础视觉移入内容级层：

- 自定义视觉层绘制每个非重命名条目的背景和文本
- 后备图标标记绘制仅在视觉层中为没有缩略图或主题图标路径的条目保留
- 缩略图图像元素在此阶段位于一个内容级图像层中，按保留视觉 slot id 键控；阶段 8 将该缩略图层替换为直接自定义图像绘制，而主题图标渲染仍为渲染器策略决策
- 每个非重命名条目 slot 保持为透明的交互/拖拽 shell
- 重命名条目保持当前子子树和编辑器行为

验收标准：

- 非重命名缩略图/主题图标条目不再构建每条目文本/背景子元素树
- 图像渲染与基础条目视觉绘制分离
- 对于有图像支持的条目，跳过后备标记形状
- 测试证明视觉层和图像层成员资格保持正确分离

### 阶段 8：直接图像绘制层

用自定义绘制元素替换内容级缩略图 `img()` 层；主题图标仅可为 A/B 证据使用此路径：

- 继续使用 GPUI 的 `ImageAssetLoader` 和 pane 本地 `RetainAllImageCache` 进行缩略图路径加载、图像解码和渲染图像生命周期
- 对于默认 MIME/主题图标，保留 GPUI `img()` 子元素叠加在保留条目 shell 上；如果启用 `FIKA_CUSTOM_THEME_ICONS=1`，保持相同的 GPUI 图像缓存解码路径并按 `iconName` 保留以进行比较
- 从自定义层使用 `Window::paint_image` 绘制已加载图像
- 在图像层中保留缩略图后备标记绘制；自定义主题 A/B 模式仅在相同图标图像从未加载或加载失败且没有保留的同图标图像时使用中性无标记占位符
- 保持缩略图失败 model 驱动；缺失的缩略图渲染图像不在绘制中合成文件图标

验收标准：

- 非重命名缩略图条目不再分配每图像 `img()` 元素
- 缩略图图像加载仍然异步发生并在完成时通知 pane
- 默认主题图标使用 GPUI `img()` 元素；自定义主题 A/B 运行不得将已加载的同 `iconName` 图像替换为标记、空白矩形或无关后备
- 已加载图像边界匹配 GPUI `ObjectFit::Contain`
- 图像缓存状态保持 pane 本地，并随 pane/层释放

### 阶段 9：绘制交互 Hitbox

分两步将条目交互移出每条目 `Div` shell，匹配当前 GPUI 公共 API 边界。

#### 阶段 9a：保留悬停/光标 Hitbox

通过内容级自定义元素路由非重命名 Compact/Icons 悬停和光标：

- 自定义元素为每个可见条目视觉矩形插入一个稳定 hitbox
- 悬停和光标通过保留 slot 表路由
- 每条目 shell 仅作为 GPUI 拖拽源边界保留
- viewport hit testing 保持为点击/菜单/放置行为的真实来源
- 拖拽预览偏移继续使用 GPUI 的光标偏移，独立于条目几何

验收标准：

- 非重命名 Compact/Icons 悬停/光标不再需要每条目悬停处理器或光标样式
- 悬停/选择/放置视觉通过保留视觉状态投影
- 目录拖拽覆盖色调从保留放置目标状态绘制，而非瞬态 shell `drag_over` 样式
- 条目拖拽 payload 和预览行为保持不变
- 性能日志不显示新的稳定渲染/构建回退；冷模式切换缓存预热与调整大小/全屏稳定路径分开跟踪
- P9a 性能证据不是移除拖拽 shell 的许可；P9b 仍需要公共 GPUI 拖拽启动 API 或经过审计的 GPUI patch

#### 阶段 9b：拖拽源 Hitbox

仅在 GPUI 暴露公共自定义元素拖拽启动 API 或 Fika 携带小型经过审计的 GPUI patch 后，移除剩余的非重命名每条目拖拽 shell：

- 拖拽源从保留 hitbox 启动
- Compact/Icons 非重命名条目完全不分配每条目元素
- 内部条目 DnD、pane DnD、Places DnD 和外部放置行为保持不变

### 阶段 10：重命名叠加层边界

在文本输入与条目绘制分离之前，保持重命名为唯一条目本地子路径：

- 选中条目的普通基础视觉仍由层绘制
- 重命名条目的缩略图图像仍由图像层绘制；主题图标图像遵循当前渲染器策略
- 编辑器、caret、选择高亮、警告/错误助手和点击 caret hit testing 保留在现有重命名子树中
- 重命名子树作为叠加层定位，而非作为默认条目视觉路径

验收标准：

- 启动/停止重命名不重建无关条目视觉/图像层
- 重命名 caret 和 UTF-8 选择测试保持绿色
- Tab 重命名下一个保持 model 顺序和 pane 本地 draft 状态

### 阶段 11：详情模式绘制路径

在 Compact/Icons 完全保留后，将详情行移到相同 model：

- P11a 将可见详情行投影到保留的 `DetailsPaintSlot` 状态，并从保留内容/几何/视觉快照馈送现有 GPUI 行子树。这仅是一个桥梁；它不声称自定义绘制胜利。
- P11b 将行背景、图标和文本单元格移入内容级自定义视觉层。行 shell 仅作为 GPUI 拖拽启动边界保留，直到拖拽启动可以在不丢失行为或性能证据的情况下安全移动。
- P11c 保持保留详情行数据显式：路径、目录标志、名称/图标、选择计数和放置目标状态从保留行快照投影并受测试覆盖。行 shell 仅消费拖拽启动字段。回收站独有列也投影到视觉层单元格中。
- P11d 为详情视觉层提供专用性能通道，以便自定义绘制扩展可以独立于 Compact/Icons 静态视觉进行评判。
- 详情文本形状使用 pane 本地缓存，按文本和文本样式键控；其命中/未命中/淘汰统计与行视觉 prepaint/paint 分开报告。
- 行背景、文本单元格和图标从保留行快照绘制
- 列调整大小/排序/放置 hit testing 保持 model 驱动
- 点击、菜单、导航、滚动和中键粘贴行为通过 viewport 的保留 hit testing 路由，而非行本地鼠标处理器
- 详情中的内联重命名使用与 Compact/Icons 相同的叠加层边界

验收标准：

- P11a 证明行内容在仅几何更改中复用，且选择/放置更改是视觉状态修补。
- P11b 证明详情行视觉投影到绘制器数据中，不再构建每单元格 GPUI 视觉子元素。
- P11c 证明回收站视觉列在绘制器迁移中存活，且保留详情行数据仍携带剩余拖拽启动边界所需的字段。
- P11d 在移除任何剩余行 shell 行为之前，保持详情绘制计时可通过 `[fika details-visual]` 归因，详情文本缓存活动可通过 `[fika details-shape-cache]` 归因。
- 详情稳定渲染不再为每个可见条目构建一个视觉行子树
- 选择、右键菜单、拖放和回收站列保持行为
- Compact/Icons 和详情在可行的地方共享 slot/图像/文本缓存概念

## 当前剩余边界

在 P11e 之后，普通点击、右键菜单、导航、滚动、悬停、光标和中键粘贴行为通过保留 model/布局数据路由，而非行本地处理器。

剩余的条目本地表面是有意的：

- Compact/Icons 非重命名条目 shell：仅 GPUI `Div::on_drag` 拖拽启动边界。它们不携带 GPUI `img()` 或静态文本视觉子元素。它们的视觉、图像、悬停/光标、点击/菜单/放置 hit testing 和拖拽覆盖状态是保留/绘制器驱动的。
- 详情行 shell：`src/ui/file_grid/details_shell.rs` 仅拥有 GPUI `Div::on_drag` 拖拽启动边界。行视觉、放置分发和行悬停/点击/菜单/导航是保留/绘制器或 viewport 驱动的。
- 重命名叠加层：用于 caret hit testing、选择、警告/错误助手文本和光标文本行为的文本编辑边界。

本地 GPUI 0.2.2 通过 `Div::on_drag` 暴露拖拽启动，而自定义元素暴露 `Window::insert_hitbox` 加上 `Window::on_mouse_event` 用于鼠标 hit testing。因此 P9b 仍然被阻塞，除非有公共自定义元素拖拽启动 API 或小型经过审计的 GPUI patch。在此之前，移除这些最后的拖拽 shell 将冒着回退 DnD 行为的风险，而非改进保留架构。

## 不变量

- 点击/菜单/放置行为继续使用 Rust hit testing。
- 拖拽源 payload 保持路径和选择计数正确。
- 重命名编辑器保持完全交互和 UTF-8 安全。
- 缩略图角色调度保持可见优先和生成保护。
- 当投影 viewport 宽度已匹配测量边界时，窗口调整大小不需要第二次通知。
- Places 和条目拖拽预览在不同模式和条目尺寸间保持光标稳定。

## 非目标

- 不在第一个静态绘制切片中重写详情模式。
- 不在 GPUI 的公共 `RetainAllImageCache` 和 `ImageAssetLoader` 仍然足够时重新实现缩略图解码/缓存所有权。主题图标文件不得在 GPUI prepaint 中同步解码。
- 不重新引入 Compact/Icons 条目本地 `img()` 或静态文本子元素，除非测量到的 GPUI 基线证明该路径优于保留内容级绘制器。
- 不引入新的应用级 ECS 或场景图。
- 不将文件管理器决策移入 GPUI 绘制代码。
