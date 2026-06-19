> 本文是 [ITEM_VIEW_CUSTOM_PAINT_TODO.md](ITEM_VIEW_CUSTOM_PAINT_TODO.md) 的简体中文翻译。

# 条目视图自定义绘制 TODO

这是 GPUI 条目视图自定义绘制迁移的活动任务板。

## P0：准备

- [x] 确认 `KItemListView` widget 复用的 Dolphin 参考边界。
- [x] 保持当前 viewport 调整大小预备和快照缓存行为。
- [x] 记录设计和迁移阶段。
- [x] 在 `file_grid.rs` 中添加简短注释，标记临时交互 shell 与静态绘制边界。

## P1：静态后备视觉画布

- [x] 为非重命名、非缩略图后备图标条目添加静态条目视觉元素。
- [x] 从 `FileIconSnapshot` 绘制后备图标背景和标记。
- [x] 从 `VisibleItemSnapshot` 绘制 Compact/Icons 条目名称行。
- [x] 将缩略图条目保留在当前 `img()` 路径上。
- [x] 将真实主题图标条目保留在当前缓存图标路径上，直到图像绘制所有权被审计。
- [x] 将重命名条目保留在当前编辑器路径上。
- [x] 保留条目拖拽预览和 payload 行为。
- [x] 运行 `cargo fmt`、`cargo check`、`cargo test`、`cargo build`。
- [x] 在此切片后审查用户提供的 `FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads` 日志。

## P2：文本形状缓存

- [x] 定义文本绘制缓存键。
- [x] 为静态条目标签缓存形状行。
- [x] 在视图模式、缩放/字体度量、选择颜色、显示行或重命名状态更改时失效。
- [x] 在 `FIKA_PERF_ITEM_VIEW` 之后埋入缓存命中/未命中计数。
- [x] 验证当文本内容和文本矩形尺寸稳定时，调整大小不会重新塑造未更改的可见条目标签。

## P3：保留绘制 Slot 状态

- [x] 在 `VisibleItemSlotPool` 旁添加 `ItemPaintSlot` 状态。
- [x] 将 `VisibleItemSnapshot` 投影到保留绘制状态。
- [x] 跟踪纯几何与内容更改。
- [x] 在不重建内容的情况下修补选择/放置/悬停视觉状态。
- [x] 在重叠滚动和调整大小期间保持 slot identity 稳定。

## P4：缩略图绘制边界

- [x] 审计 GPUI `img()` 和 `Window::paint_image` 缓存所有权。
- [x] 决定保留图像元素 vs 直接绘制句柄。
- [x] 为文件网格图像条目添加 pane 本地保留图像缓存。
- [x] 按视觉 slot id 键控缩略图/主题图标图像元素。
- [x] 保留 freedesktop 缓存缩略图首帧行为。
- [x] 保留缩略图失败/失效 model 语义。
- [x] 重新审视直接 `Window::paint_image`：P8 使用 GPUI 的公共 `RetainAllImageCache` / `ImageAssetLoader` / `RenderImage` 契约，而不是在 Fika 中重新实现图像解码。

## P5：专用自定义元素

- [x] 如果直接自定义元素提供更好的保留 prepaint 状态，则替换 canvas spike。
- [x] 将绘制计时埋点移入自定义元素。
- [x] 添加围绕几何和内容键失效的测试。

## P6：Pane 级静态视觉层

- [x] 通过一个内容级自定义层绘制静态后备 Compact 和 Icons 视觉。
- [x] 为静态后备条目将条目 slot 保留为透明交互 shell。
- [x] 将缩略图、主题图标和重命名条目保留在其专门的子路径上。
- [x] 添加测试，证明只有后备静态条目进入该层。
- [x] 重新审视缩略图/主题图标保留图像条目是否可以加入 viewport 绘制器：P8 将它们移入由 GPUI 保留图像缓存支持的自定义图像绘制层。

## P7：非重命名基础视觉和图像层

- [x] 在内容级基础视觉层中包含每个非重命名 Compact/Icons 条目。
- [x] 仅为没有缩略图/主题图标路径的条目绘制后备图标标记。
- [x] 将缩略图/主题图标 `img()` 元素移入按保留视觉 slot id 键控的内容级图像层。
- [x] 保持非重命名条目 shell 透明且仅交互。
- [x] 保持重命名条目在当前子子树上。
- [x] 对图像支持的条目跳过后备标记形状和缓存键碎片。
- [x] 重新审视直接 `Window::paint_image`：P8 使用 GPUI 的保留图像缓存契约进行直接绘制，而不添加 Fika 拥有的解码器。

## P8：直接图像绘制层

- [x] 用一个自定义图像绘制元素替换内容级缩略图/主题图标 `img()` 子元素。
- [x] 使用 pane 本地 `RetainAllImageCache` 加上 GPUI `ImageAssetLoader` 进行异步路径/SVG/图像解码。
- [x] 使用 `Window::paint_image` 绘制已加载图像。
- [x] 通过复用保留同 `iconName` 图像（在回退到中性无标记占位符之前）来保持主题图标视觉稳定性。
- [x] 保持缩略图角色成功/失败 model 驱动，同时在挂起图像加载或资源加载失败时仅在已尝试同源保留图像后才绘制条目后备视觉。
- [x] 匹配 `ObjectFit::Contain` 图像边界。
- [x] 添加图像绘制成员资格和后备策略的测试。

## P9：绘制交互 Hitbox

- [x] 审计 GPUI 自定义元素 hitbox 插入以支持悬停和光标。
- [~] 用保留 hitbox 替换非重命名每条目交互 shell：P9a 首先移动悬停/光标；P9b 仅在 GPUI 暴露公共自定义元素拖拽启动 API 或 Fika 携带经过审计的 GPUI patch 后移除拖拽 shell。
- [x] 通过保留条目视觉状态路由非重命名 Compact/Icons 悬停和光标投影。
- [x] 通过保留条目视觉状态路由目录拖拽覆盖投影；条目/行 shell 不再绘制临时 `drag_over` 背景。
- [x] 通过保留行视觉状态路由详情悬停投影；详情行 shell 不再绘制临时悬停背景。
- [x] 通过保留交互层路由详情悬停/光标 hit testing；详情行 shell 不再拥有悬停监听器或光标样式。
- [x] 通过 viewport 级保留 hit testing 路由详情点击/菜单/导航/中键粘贴；详情行 shell 不再拥有鼠标按下处理器或阻止鼠标事件。
- [x] 保留条目/place 拖拽预览光标偏移行为。
- [x] 在 Compact、Icons 和 Details 保留迁移路径中保留 Rust viewport hit testing 用于点击/菜单/放置。
- [x] 为保留 hitbox prepaint/paint 计数和计时添加 P9a 交互层性能日志。
- [x] 在进一步扩展自定义交互之前，将 P9a 性能日志与之前的 GPUI 悬停/光标 shell 路径进行比较；用户 `~/Downloads` 日志显示热调整大小/全屏条目视图转换保持亚毫秒级，而冷模式切换缓存预热保持单独跟踪。

## P10：重命名叠加层边界

- [x] 在重命名启动时保持普通条目背景/文本/图像在内容级层中。
- [x] 将重命名编辑器定位为唯一条目本地叠加子树。
- [x] 保留 caret hit testing、UTF-8 选择、警告/错误助手和 Tab 重命名下一个。
- [x] 验证启动/停止重命名不重建无关条目层内容。

## P11：详情模式绘制路径

- [x] P11a：将详情行投影到保留绘制 slot 中，同时保持现有 GPUI 行子树作为渲染路径。
- [x] P11b：从内容级自定义层绘制行背景、图标和文本单元格，同时最初保留行 shell 作为桥梁。
- [x] P11c：在保留绘制器边界保留保留详情路径/拖拽字段和回收站特定视觉列。
- [x] P11e：将详情行 shell 缩小到剩余的 GPUI 拖拽启动边界；点击、菜单、导航、滚动和中键粘贴 controller 行为现在通过 viewport 保留 hit testing 路由。
- [x] P11f：通过 viewport 级放置处理器路由详情放置分发；详情行 shell 不再拥有每行条目/外部/place 放置处理器。
- [x] P11d：将详情视觉层性能日志拆分为专用的 `[fika details-visual]` 通道，以便在不与 Compact/Icons 静态视觉混合的情况下比较 GPUI 行 shell 成本和自定义绘制成本。
- [x] 在可行的地方与 Compact/Icons 共享图像/文本缓存概念：详情现在使用相同的 GPUI 保留图像缓存路径和一个 pane 本地详情文本形状缓存，具有单独的性能统计。

## P12：剩余边界审计

- [x] 审计本地 GPUI 拖拽 API：GPUI 0.2.2 通过 `Div::on_drag` 暴露拖拽启动，而自定义元素暴露 hitbox 和鼠标监听器但不暴露公共自定义元素拖拽启动钩子。
- [x] 记录剩余的条目本地表面：Compact/Icons 拖拽启动 shell、详情拖拽启动行 shell 和重命名文本编辑叠加层。
- [x] 添加 `docs/ITEM_VIEW_RUNTIME_SMOKE.md`，包含用于 P11e 后验证的运行时 DnD、重命名和性能日志检查清单。
- [x] 添加 `scripts/analyze-item-view-perf.sh` 以总结性能日志并在 P11e 后审查期间强制执行所需的 steady/details/static-visual/interaction 通道和已锻炼的视图模式，包括 Compact/Icons 静态视觉模式覆盖。
- [ ] 在 P11e 之后运行运行时 DnD smoke pass：条目拖拽、条目到目录放置、pane 放置、Places 放置/重排、外部路径放置，以及在 Compact、Icons 和 Details 中的重命名 caret 点击。
- [ ] 在扩展自定义绘制或尝试另一个 shell 移除切片之前，收集 Compact、Icons 和 Details 调整大小/全屏路径的 P11e 后 `FIKA_PERF_ITEM_VIEW=1` 日志。

## P13：渲染器决策门

- [ ] 在每个新的自定义绘制表面之前，识别 Dolphin 风格的 model、layouter、controller/hit-test 和 painter 所有者。
- [ ] 在 GPUI 保持更快或拥有所需平台契约的表面上保持 GPUI 内置元素，同时仍然从保留 model 数据馈送它们。
- [ ] 仅在运行时日志显示中性或更好的稳定行为且迁移保持行为完整的拖放、重命名和选择路径时，才扩展自定义绘制。
- [ ] 对于当前具有 GPUI 路径的每个表面，在将自定义绘制器接受为默认渲染器之前捕获相同场景的 GPUI 基线。
- [ ] 在移除任何现有 GPUI 表面之前，在相关参考文档或 TODO 条目中记录渲染器决策和性能证据。
- [x] 添加 `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 作为当前每表面渲染器决策日志。
- [x] 添加 `scripts/summarize-item-view-renderer-evidence.sh`，以便通过的运行时性能日志产生渲染器决策证据块。
- [x] 将 Compact/Icons 渲染器选择集中到显式的 `ItemRendererPolicy`，使自定义绘制 vs GPUI 表面决策不隐藏在临时布尔值后面。
- [x] 将详情行渲染器选择集中到显式的 `DetailsRowRendererPolicy`，覆盖视觉层、保留交互和 GPUI 拖拽启动 shell 边界。
- [x] 发出 `[fika renderer-policy]` 日志，使运行时性能证据包括自定义绘制、保留交互和 GPUI shell 边界的实际表面计数分布。
- [x] 在标准运行时性能门中要求 Compact、Icons 和 Details 的渲染器策略日志覆盖。
- [x] 将渲染器策略拆分到 `src/ui/file_grid/renderer_policy.rs`，使自定义绘制 vs GPUI 渲染器的决策边界与渲染构造分离。
- [x] 使 `scripts/analyze-item-view-perf.sh` 拒绝不可能的渲染器策略表面计数，因此自定义绘制证据不能声明比记录的条目数量更多的自定义/保留/GPUI 表面。

## P14：完整转换路线图

- [x] 添加 `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`，使当前替换状态、剩余 GPUI 边界和完整转换路线图显式化。
- [ ] 在另一个绘制器扩展之前冻结 Compact、Icons 和 Details 的当前桌面会话运行时证据块。
- [x] 在活动条目拖拽预览重绘后备之后刷新 `FIKA_DEBUG_DND=1` 运行时证据：pane 条目拖到 pane 目录上记录 `active-item-move via=preview ... kind=Some(Directory)` 并在放置前视觉高亮目录。
- [x] 记录 2026-06-17 pane 自拖拽根本原因和验收追踪：GPUI 可以在拖拽启动后停止传递 pane/条目移动回调，因此保留的 `ActiveItemDrag` 目标必须在必要时由预览重绘 tick。
- [x] 沿 Dolphin 风格的 model/投影、controller/hit-test、painter 和 renderer-policy 边界拆分 `src/ui/file_grid.rs`，而不改变行为。
- [x] 将根文件网格渲染表面组合提取到 `src/ui/file_grid/surface.rs`，使 `src/ui/file_grid.rs` 不再是 viewport/层/shell 组装的所有者。
- [x] 将条目视图绘制器性能计数器提取到 `src/ui/file_grid/perf.rs`，使渲染埋点不再由主文件网格表面拥有。
- [x] 将 FikaApp 条目视图性能访问器/记录方法移入 `src/ui/file_grid/perf.rs`。
- [x] 将条目视图性能帧阶段分类移入 `src/ui/file_grid/perf.rs`，使调整大小/模式/内容/视觉埋点不再在 `main.rs` 中定义。
- [x] 将文件网格条目/place/外部拖拽移动和放置处理器提取到 `src/ui/file_grid/dnd.rs`，使 controller 路由不再由主绘制器/渲染表面拥有。
- [x] 将条目拖拽预览渲染和选择计数标签逻辑移入 `src/ui/file_grid/dnd.rs`，使剩余的 GPUI 拖拽启动 shell 边界集中化。
- [x] 将文件网格滚轮、pane 导航和条目鼠标按下 controller 决策提取到 `src/ui/file_grid/controller.rs`。
- [x] 将文件图标解析候选排序移入 `src/ui/file_grid/snapshot/scheduler.rs`，使可见/预读角色工作与元数据和缩略图调度一起投影，而不是在 `snapshot.rs` 中。
- [x] 将原始文件网格快照模型/转换边界提取到 `src/ui/file_grid/snapshot.rs` 子模块，使 model 投影、角色调度和视图模式组合模块化。
- [x] 将缩略图候选和预读投影移入 `src/ui/file_grid/snapshot/thumbnail.rs`，使角色调度决策与原始快照构造分离。
- [x] 将缩略图/预读投影测试移入 `src/ui/file_grid/snapshot/thumbnail.rs`，使快照门面不再导入缩略图私有测试助手。
- [x] 将元数据角色候选投影及其 `RawFileGridSnapshot` 方法实现提取到 `src/ui/file_grid/snapshot/metadata.rs`，使 MIME magic 调度决策与原始快照构造分离。
- [x] 将原始快照 model/投影类型提取到 `src/ui/file_grid/snapshot/types.rs`，使原始数据契约与构造、转换、调度器和范围助手分离。
- [x] 将 Compact/Icons 预读与 Dolphin 的角色更新器边界对齐：不可见工作窗口条目可以复用现有快照内容进行绘制预热，但未缓存的预读条目在渲染转换期间不再触发同步图标/文本内容解析。
- [x] 将文件图标主题路径解析移出渲染转换：可见 Compact/Icons/Details 条目现在在帧中使用缓存/初步图标快照。可见同步图标预热遵循 Dolphin `updateVisibleIcons()` 索引顺序，而后台解析队列遵循 Dolphin `indexesToResolve()` 可见/预读顺序。
- [x] 当后台图标解析结果到达时使可见条目快照缓存失效，以便初步图标被替换而无需在滚动或缩放帧中进行同步主题查找。
- [x] 保持缩略图/主题图标挂起或加载失败帧视觉稳定：首先复用保留的同源真实图像，然后在没有保留图像存在时绘制后备视觉。
- [x] 将缩放图标视觉与 Dolphin 对齐：普通 MIME/主题图标立即根据当前布局图标尺寸解析，匹配 Dolphin `KStandardItemListWidget::pixmapForIcon()`，而主题图标文件仍然不在 prepaint 中同步解码。主题图标图像及其首帧加载占位符现在绘制到相同的当前方形图标框中，以避免挂起小图标然后真实图标尺寸跳跃。
- [x] 将保留条目/详情绘制 slot 状态提取到 `src/ui/file_grid/paint_slots.rs`，使 model 到绘制器快照复用与渲染器构造代码分离。
- [x] 将保留条目/详情交互 hitbox 层提取到 `src/ui/file_grid/interaction.rs`，使悬停/光标 hitbox 和活动条目拖拽窗口跟踪与主绘制器/渲染表面分离。
- [x] 将剩余的跨模块文件网格测试移入 `src/ui/file_grid/tests.rs`，使 `src/ui/file_grid.rs` 仅是模块门面和公共导出边界。
- [ ] 在公共 GPUI 自定义元素拖拽启动支持存在或携带经过审计的 GPUI patch 之前保持剩余拖拽启动 shell。
- [ ] 在自定义文本编辑具有焦点、caret、选择、验证、提交/取消和 IME 的行为覆盖之前保持重命名在 GPUI 叠加层上。
- [x] 将 Places 视为单独的渲染器迁移，具有自己的 GPUI 基线和 DnD/滚动验收门。结果：`docs/PLACES_RENDERER_PLAN.md` 定义了 Dolphin model/view 划分、保留行迁移门、DnD/滚动验收检查以及当前的 `FIKA_PERF_PLACES_VIEW=1` GPUI 基线。

## P15：完整转换执行计划

这是在保留条目视图方向被接受后的活动计划。它将代码库推向完全自定义绘制/复用池所有权，而不假装每个剩余的 GPUI 边界今天都可以安全移除。

- [~] P15a：在 Dolphin 对齐的缩放图标视觉更新后冻结当前桌面会话证据。所需日志：`FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads`、`FIKA_PERF_ITEM_VIEW=1 cargo run -- /etc` 和一个 `FIKA_DEBUG_DND=1` pane 自拖拽追踪。当前状态：`/etc` 缩放/滚动 autosmoke 和 pane 自拖拽 `via=preview` 追踪已记录；完整的 `~/Downloads`/详情/手动 DnD 桌面会话 pass 在另一个 shell 移除或绘制器扩展切片之前仍需要刷新。
- [x] P15b：在扩展或回退任何渲染器表面之前，在 `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 中记录证据摘要。当前证据默认将 MIME/主题图标保留在 GPUI `img()` 元素上，并将剩余的 `/etc` autosmoke 成本识别为静态视觉/文本/基础绘制，而非同步主题图标路径查找。
- [x] P15c：从源而非猜测决定拖拽启动边界：要么确认公共 GPUI 自定义元素拖拽启动 API 存在，要么携带小型经过审计的 GPUI patch，要么将 Compact/Icons 和 Details 拖拽启动 shell 保留为显式平台边界。当前决定：GPUI `0.2.2` 仅通过交互元素暴露类型化拖拽启动，因此 shell 保留为显式平台边界。
- [ ] P15d：如果 P15c 解锁保留拖拽启动，先移除 Compact/Icons 非重命名拖拽 shell，然后移除详情行拖拽 shell。每次移除需要对条目到目录、pane 放置、Places 放置/重排和外部路径放置进行 DnD smoke。
- [~] P15e：在实现之前对保留/自定义行绘制器进行基准测试，与当前 GPUI 侧栏比较。仅当滚动、重排、挂载/回收站/设备行、右键菜单和放置行为中性或更好时才接受 Places 迁移。当前状态：GPUI 侧栏基线和渲染器策略日志存在，且 `FIKA_AUTOSMOKE_PLACES=targets` 覆盖非持久目标/插入投影。`PlacePaintSlotCache` 现在记录保留行/section slot 和 `[fika places-slots]` 统计。该条目后续由 P16dy 收窄：默认现在使用 Dolphin 对齐的 custom chrome layer 绘制 background/drop/insert/trash，同时 GPUI 保留文本、图标、行事件传递、右键菜单、DnD 和拖拽启动 shell。`FIKA_CUSTOM_PLACES_ROWS=1` 保留为 full custom-text 基准路径。`places/interaction.rs` 现在拥有行/section 目标决策，而 GPUI shell 仍提供事件传递和边界。行视觉聚合到一个侧栏级层中，因此 `[fika places-row-visual] rows` 必须匹配策略行计数，而不是每行记录一个 canvas。
- [ ] P15f：在自定义文本编辑计划覆盖焦点、caret hit testing、UTF-8 选择、验证、提交/取消、Tab 重命名下一个和 IME 之前，保持重命名在 GPUI 上。不要在没有该行为矩阵的情况下合并自定义重命名绘制器。
- [ ] P15g：收紧复用池证据。运行时渲染器策略日志应证明普通 Compact/Icons 和 Details 帧没有每条目 GPUI 视觉子元素，只有已知的拖拽启动/重命名边界。
- [ ] P15h：在可以在不改变行为的情况下完成时，将仍存在于 `src/main.rs` 中的任何剩余条目视图编排移入 Dolphin 对齐的文件网格模块。候选边界：图标角色更新调度、文件图标解析队列移交和运行时证据收集助手。已完成：
  - [x] 修剪 `file_grid.rs` 重导出：`src/ui/file_grid.rs` 不再从子模块重新导出私有 surface/details/details_shell/item_shell/types（需要的 crate 使用 `pub(crate)` 子模块路径）。`src/ui.rs` 不再重新导出 `interaction` 或 `renderer_policy` 符号。
  - [x] 将文件图标解析候选排序移入 `src/ui/file_grid/snapshot/scheduler.rs`，使可见/预读角色工作与元数据和缩略图调度一起投影，而不是在 `snapshot.rs` 中。
  - [x] 将缩略图候选和预读投影移入 `src/ui/file_grid/snapshot/thumbnail.rs`，使角色调度决策与原始快照构造分离。
  - [x] 将元数据角色候选投影移入 `src/ui/file_grid/snapshot/metadata.rs`，使 MIME magic 调度决策与原始快照构造分离。
  - [x] 将原始快照类型提取到 `src/ui/file_grid/snapshot/types.rs`，使原始数据契约与构造、转换、调度器和范围助手分离。
  - [x] 将条目视图绘制器性能埋点移入 `src/ui/file_grid/perf.rs`，并将 FikaApp 条目视图性能访问器/记录方法移入同一模块。
  - [x] 将文件网格条目/place/外部拖拽移动和放置处理器移入 `src/ui/file_grid/dnd.rs`，使 controller 路由不再由主绘制器/渲染表面拥有。
  - [x] 将文件网格滚轮、pane 导航和条目鼠标按下 controller 决策移入 `src/ui/file_grid/controller.rs`。
  - [x] 将保留条目/详情绘制 slot 状态移入 `src/ui/file_grid/paint_slots.rs`。
  - [x] 将保留条目/详情交互 hitbox 层移入 `src/ui/file_grid/interaction.rs`。
  - [x] 将渲染器策略决策移入 `src/ui/file_grid/renderer_policy.rs`。
  - [x] 将剩余的跨模块文件网格测试移入 `src/ui/file_grid/tests.rs`，使 `src/ui/file_grid.rs` 仅是模块门面和公共导出边界。
  已完成：文件图标排队/可见/传输中解析状态存在于 `file_grid/icon_work.rs` 中；可见文件图标同步和排队工作移交现在通过 `file_grid/icon_work.rs` 路由；较早的 pane 本地主题图标角色尺寸去抖动已被移除，因为它导致了延迟的第二次缩放调整。运行时证据收集助手保留在 `src/main.rs` 和脚本中。

## P16：具体完整转换积压

此阶段将已接受的方向转化为可执行的队列。它按风险和证据排序，而非按表面看起来有多自定义绘制。

Places chrome 默认之后的当前执行入口是
`docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md`；本 backlog 需要与其中轨道保持一致。

- [x] P16a：在规划、设计和 TODO 文档中记录完整转换轨道：证据、绘制器、controller、shell 边界、Places 和所有权。
- [x] P16b：在最新的 Dolphin 对齐主题图标绘制边界更改后收集一组新的桌面会话证据：`/etc` 自定义主题 vs 默认日志现在证明默认 MIME/主题图标避免了首帧加载 `theme_placeholder` 变动，且 `FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc` 捕获无人值守缩放/滚动证据。
- [x] P16c：使用该证据更新 `docs/ITEM_VIEW_RENDERER_DECISIONS.md`，包括 `/etc` 缩放/滚动是否仍然显示冷图像加载卡顿或可见占位符到图标切换。当前证据：可见同步停止复制排队的预读图标工作后，`icon_sync` 最大值从 `28340us` 降至 `173us`；剩余的 `/etc` autosmoke 成本是静态视觉文本/基础绘制，而非 MIME/主题图像渲染。
- [x] P16d：如果当前日志无法区分以下情况，则添加或扩展运行时证据工具：首帧加载主题图标占位符、保留同 `iconName` 复用、GPUI 图像缓存解码完成和稳定重绘成本。`[fika item-image]` 现在报告 `theme_loaded`、`theme_decoded`、`theme_retained`、`theme_placeholder`、`thumb_loaded`、`thumb_decoded`、`thumb_retained` 和 `thumb_fallback`；运行时分析器将其总结为 `image_sources`。`FIKA_AUTOSMOKE_ITEM_VIEW` 现在无需手动输入即可练习缩放/滚动，并添加 `[fika autosmoke]` 标记到同一性能日志中。
- [x] P16e：审计本地 GPUI 源码中保留/自定义元素拖拽启动路径。如果没有公共 API 存在，记录确切阻塞并保留条目和详情拖拽启动 shell。结果：GPUI 通过 `Interactivity::on_drag` / `StatefulInteractiveElement::on_drag` 在 `crates/gpui/src/elements/div.rs` 中暴露类型化拖拽启动。自定义元素可以通过 `Window::insert_hitbox()` 插入 hitbox，并通过 `Window::on_mouse_event()` 观察鼠标事件，但没有公共 API 从这些保留 hitbox 启动类型化拖拽，因此条目、详情和 Places 拖拽启动 shell 保留为显式平台边界。2026-06-19 复查：Zed commit `69b602c797a62f09318916d24a98c930533fbdc8` 仍然是同一阻塞。
- [x] P16f：如果选择经过审计的 GPUI patch，设计最小的从保留 hitbox 启动拖拽的 API，同时保留 payload、预览、光标偏移、接受的传输模式和外部放置行为。当前设计：`docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md` Track 4 现在定义了最小 retained typed drag API 拆分，覆盖 drag start（`Window::on_hitbox_drag`）和 typed drag-move/drop payload delivery（`Window::on_hitbox_drag_move`、`Window::can_drop_on_hitbox`、`Window::on_hitbox_drop`），且不需要为了作为拖拽源或目标而重新创建可见 GPUI row/item。
- [x] P16g：将下一个行为保留的条目视图编排边界移出 `src/main.rs`。候选：运行时条目视图性能/证据收集访问器，因为绘制器性能状态已经存在于 `file_grid/perf.rs` 下。已完成：`FIKA_PERF_ITEM_VIEW` 标志和文件网格性能层调用者由 `src/ui/file_grid/perf.rs` 拥有；条目视图性能帧分类和性能状态清理由 `src/ui/file_grid/perf.rs` 拥有；帧状态和绘制器性能统计存储现在位于 `src/ui/file_grid/perf.rs` 中的 `ItemViewPerfState` 后面；条目视图性能摘要发出现在由 `src/ui/file_grid/perf.rs` 拥有；autosmoke 场景解析和操作排序现在位于 `src/ui/file_grid/autosmoke.rs` 中。
- [x] P16h：在更改 Places 渲染之前起草保留 Places 行绘制器设计。设计必须覆盖行组、隐藏 section、设备行、重排/放置插入、右键菜单和侧栏滚动。结果：`docs/PLACES_RENDERER_PLAN.md` 将 Dolphin 的 `DolphinPlacesModel + KFilePlacesView` 划分与 Fika 当前的 `places/model`、`projection`、`sidebar/row`、`drag` 和自定义滚动条模块进行比较，然后将任何保留行绘制器门控于 Places 特定性能日志、运行时 smoke 和渲染器策略证据之后。
- [x] P16i：在更改 GPUI 重命名叠加层之前起草重命名自定义编辑器行为矩阵。它必须覆盖焦点、caret hit testing、UTF-8 选择、验证帮助文本、提交/取消、Tab 重命名下一个和 IME。结果：`docs/RENAME_EDITOR_PLAN.md` 将 Dolphin 的 `DolphinView::renameSelectedItems()`、`KItemListView::editRole()` 和 `KItemListRoleEditor` 路径与 Fika 的 `RenameDraft`、快捷键路由和 GPUI 叠加层进行比较。该矩阵将叠加层保留为默认值，直到 IME、焦点/失焦、鼠标选择、可访问性和运行时 smoke 被覆盖。
- [x] P16j：在 MIME/主题图标闪烁修复之前建立历史图像渲染器基线。使用 `a3f5b0f` 作为预保留/自定义绘制 GPUI `img()` 基线，并使用 `d497593`、`8d1198f`、`36da130` 和 `b0cac9a` 作为转换检查点，以决定回归属于 model/投影、保留 slot 状态、自定义元素绘制还是自定义图像层。该历史基线后来演进为当前的 `FIKA_GPUI_THEME_ICONS=1` 同场景 GPUI image baseline；默认路径已经是 full custom image layer。`scripts/compare-item-image-renderers.sh` 继续标准化配对日志比较。
- [x] P16k：从证据中决定 Compact/Icons 主题图标渲染器。历史阶段先保留 GPUI `img()`，随后经过 prewarm/hybrid handoff、semantic key cache、source-image reuse、app-level prewarm 和 cache budget，默认已推进到 full custom image layer；`FIKA_GPUI_THEME_ICONS=1` 保留为 GPUI baseline。
- [x] P16k1：retained MIME/theme icon image cache 已实现并成为默认路径。cache 以 semantic `ThemeIconImageKey` 为主；当主题、scale factor 或 color scheme 会影响选中路径时使用稳定输入/哨兵；缩略图仍按 thumbnail path 独立保留；普通渲染/prepaint 不做无界同步解码。设计和完成证据记录在 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md`。
- [x] P16k2：默认 full-custom vs GPUI baseline 的成对 runtime evidence 已由后续 runner 和最终 core evidence 覆盖。`scripts/run-retained-renderer-evidence.sh --core --skip-build --prefix fika-core-final-retained-v3` 覆盖 Compact/Icons/Details；item summary 显示 `gpui_image_element=0`、`theme_placeholder=0`、visible `theme_decoded=0`，image max paint `373us`，warm image max paint `363us`。
- [x] P16k2a：在重新考虑默认 custom theme icon 前构建 prewarm/hybrid bridge。`FIKA_PREWARM_THEME_ICONS=1` 会在可见 theme icon 仍由 GPUI `img()` 绘制时预热 retained theme-icon image。2026-06-18 `/tmp/fika-icon-prewarm-etc-p16k2.log` smoke 保持 `max_image_layer=0`、`max_gpui_image_element=64`、`theme_placeholder=0` 和 `paint_count=0`，同时把预热工作暴露为 `theme_prewarm_loaded=598`、`theme_prewarm_decoded=5` 和 `theme_prewarm_pending=118`。这验证了无可见 placeholder 的 bridge。readiness handoff 基础随后实现，并被后续成对证据验证；该路径最终被 semantic cache、source-image reuse、app-level prewarm 和 cache budget 后的 full custom 默认取代。
- [x] P16k3：`docs/ITEM_VIEW_RENDERER_DECISIONS.md` 已重新评估 Compact/Icons MIME/theme icon renderer policy。当前划分为：缩略图和普通 MIME/theme icon 都走 retained/custom image layer，`FIKA_GPUI_THEME_ICONS=1` 仅作为 GPUI baseline，`FIKA_HYBRID_THEME_ICONS=1` 仅作为显式过渡 handoff 路径。
- [x] P16l：在任何保留行绘制器工作之前建立 Places GPUI 侧栏基线。`FIKA_PERF_PLACES_VIEW=1` 现在记录快照时间、侧栏构建时间和 GPUI 行路径的当前渲染器策略表面计数；`docs/PLACES_RENDERER_PLAN.md` 记录了 2026-06-17 桌面会话基线。
- [x] P16m：在任何保留行绘制器工作之前添加非破坏性 Places 运行时 smoke 路径。`FIKA_AUTOSMOKE_PLACES=targets` 现在驱动 place 目标、插入开始、插入结束、清除和快照日志，而不重排或持久化书签。完整的重排/放置变异 smoke 仍然门控于隔离的用户 place 配置或手动审查。
- [x] P16n：在不改变可见渲染的情况下添加保留 Places 绘制 slot 和统计。`PlacePaintSlotCache` 通过稳定的语义 identity 保留 section 标题和 place 行，对设备行优选设备 id，对普通行优选路径/组。`[fika places-slots]` 现在报告当前 GPUI 侧栏的插入/内容/几何/视觉/未更改/已移除 slot 活动。
- [x] P16o：在任何保留 hitbox 或自定义行绘制器工作之前，将 Places 行/section 目标决策提取出 GPUI 行闭包。`places/interaction.rs` 现在返回条目/外部路径放置和 place 重排的共享目标/光标决策。GPUI 行/section shell 仍提供事件传递、边界和拖拽启动。
- [x] P16p：在基准测试自定义行绘制器之前添加 Places 性能/自动 smoke 分析器。`scripts/analyze-places-perf.sh` 现在总结 `[fika places-view]`、`[fika places-sidebar]`、`[fika places-slots]`、`[fika places-renderer-policy]` 和非破坏性 Places autosmoke 标记。`scripts/check-places-perf-analyzer.sh` 覆盖分析器门。
- [x] P16s：在不切换默认渲染器的情况下添加第一个可选 Places 行视觉绘制器。`FIKA_CUSTOM_PLACES_ROWS=1` 自定义绘制行背景、活动/放置视觉状态、标签、回收站标记和插入指示器；默认 Places 行保持 GPUI。分析器支持现在包括 `--expect-custom-row-visual-policy` 和 `[fika places-row-visual]` prepaint/paint 最大值。
- [x] P16t：添加非破坏性 Places 溢出 autosmoke 和滚动条性能证据。`FIKA_AUTOSMOKE_PLACES=overflow` 在附加仅快照测试行时不写入用户 Places 配置，`[fika places-scrollbar]` 报告可见溢出和 `max_scroll_y`，且 `scripts/analyze-places-perf.sh` 现在支持 `--require-overflow-autosmoke`。
- [x] P16u：在考虑默认切换之前，将可选 Places 行视觉绘制器聚合到一个侧栏级层中。根本原因：第一个可选绘制器每行使用一个 canvas，因此溢出 smoke 为 75 个可见行记录了 `places_row_visual_frames=675 max_rows=1`。实现：`places_row_visual_layer` 从侧栏快照流绘制所有行背景、标签、回收站标记和插入指示器，而 GPUI 保留图标、事件传递、右键菜单、DnD 和拖拽启动 shell。证据：`/tmp/fika-places-custom-rows-layer.log` 以 `max_rows=11` 通过了 `--require-autosmoke --expect-custom-row-visual-policy`，且 `/tmp/fika-places-overflow-custom-layer.log` 以 `max_rows=75` 通过了 `--require-overflow-autosmoke --expect-custom-row-visual-policy`。守卫：分析器现在拒绝 `[fika places-row-visual] rows` 与策略行计数不匹配的自定义行视觉策略日志。
- [x] P16v：为可选的 Places 行视觉层添加保留文本形状。根本原因：在行视觉被聚合到一个 canvas 后，可选 prepaint 路径仍然每帧重新塑造每个行标签。实现：`PlacesRowTextShapeCache` 存在于 `FikaApp` 上，并仅对 `FIKA_CUSTOM_PLACES_ROWS=1` 按标签/字体/字体大小/文本颜色缓存行标签。证据/守卫：`FIKA_PERF_PLACES_VIEW=1` 现在发出 `[fika places-row-shape-cache] hits=... misses=... evicted=... entries=...`，且 `scripts/analyze-places-perf.sh --expect-custom-row-visual-policy` 要求可选自定义行日志包含该通道。
- [x] P16w：在不更改行渲染器默认值的情况下添加运行时 Places 面板宽度和可见性状态。顶部工具栏现在有一个 Places 切换按钮，侧栏分割器可以调整面板大小并双击重置，调整大小请求通过现有的 pane 行重测路径流动，以便在宽度更改后重新计算条目视图 viewport。这是有意仅在运行时的；稍后的持久化切片必须通过合并的设置路径保存宽度/可见性，而不是在每个拖拽帧上写入配置。
- [x] P16x：通过窄应用设置 model 持久化 Places 面板宽度和可见性。`src/core/settings.rs` 在 `$XDG_CONFIG_HOME/fika/settings.tsv` 中存储 `places.sidebar.width` 和 `places.sidebar.visible`；启动时在渲染面板之前加载这些值。UI 更改使用生成计数器调度仅最新的 120ms 延迟后台保存，因此重复的侧栏拖拽帧更新内存而无需同步配置写入。
- [x] P16y：在依赖手动侧栏测试之前添加无人值守 Places 面板布局 smoke。`FIKA_AUTOSMOKE_PLACES=layout` 通过与工具栏和分割器相同的应用状态/更新保存路径驱动隐藏、显示、调整大小、重置、恢复和最终设置文件验证。分析器门 `--require-layout-autosmoke` 拒绝缺失操作或最终 `layout-verify-saved ok=false`，因此未来的 Places 渲染器工作可以在比较 GPUI 和可选自定义行策略时证明其未破坏面板布局状态。证据：`/tmp/fika-places-layout.log` 通过了 `--require-layout-autosmoke --expect-current-gpui-policy`，且 `/tmp/fika-places-layout-custom.log` 通过了 `--require-layout-autosmoke --expect-custom-row-visual-policy`。
- [x] P16z：在将行 hitbox 移出 GPUI 之前使 Places 交互边界可度量。`[fika places-interaction-policy]` 报告保留行/section 目标决策计数，与当前 GPUI 事件 shell 和拖拽启动 shell 计数分开。分析器选项 `--require-interaction-policy` 要求行和 section 目标决策匹配可见行/section，同时 `retained_hitboxes=0`、`gpui_event_shells=rows+sections` 和 `drag_shells=rows`；这使当前 Dolphin 对齐的决策层保持显式，而不假装激活、菜单、DnD 事件传递或拖拽启动已经离开 GPUI。证据：`/tmp/fika-places-targets-interaction.log` 通过了 `--require-autosmoke --require-interaction-policy --expect-current-gpui-policy`；`/tmp/fika-places-custom-targets-interaction.log` 通过了 `--require-autosmoke --require-interaction-policy --expect-custom-row-visual-policy`。
- [x] P16aa：在不更改事件传递的情况下添加保留 Places 交互几何投影。`places_interaction_geometry()` 从可选视觉层使用的相同 `PLACE_ROW_HEIGHT` 和 `PLACE_SECTION_HEADING_HEIGHT` 常量投影行和 section y/高度数据。`[fika places-interaction-geometry]` 报告行、section、条目、内容高度、hit-test 采样和投影时间；`--require-interaction-geometry` 要求这些计数匹配渲染器策略。这在保持 `retained_hitboxes=0` 和 GPUI 行/section 事件 shell 显式的同时创建了未来的保留 hit-test 数据边界。证据：`/tmp/fika-places-targets-geometry.log` 通过了 `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy`；`/tmp/fika-places-custom-targets-geometry.log` 通过了 `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy`。
- [x] P16ab：在不更改事件传递的情况下添加保留 Places 几何 hit-test 逻辑。`PlacesInteractionGeometry::hit_test_y()` 将内容本地 y 坐标映射到保留行或 section，行命中复用与现有 GPUI 行 DnD 处理器相同的 `place_drop_zone_for_y()` 边缘/主体规则。这在保持激活、右键菜单、DnD 事件传递和拖拽启动在 GPUI shell 上的同时准备了未来的保留 hitbox 层。证据：`/tmp/fika-places-targets-hit-test.log` 通过了 `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-current-gpui-policy`；`/tmp/fika-places-custom-targets-hit-test.log` 通过了 `--require-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-visual-policy`，两者均带有 `max_hit_tests=2`。
- [x] P16ac：在将行/section 事件传递移出 GPUI shell 之前添加无人值守保留 Places hit-test autosmoke。`FIKA_AUTOSMOKE_PLACES=hit-test` 在插入前、在 place 上和插入后 y 位置采样第一个保留行，采样第一个 section 标题，并发出要求行和 section 都存在才能通过的摘要。`scripts/analyze-places-perf.sh` 现在具有 `--require-hit-test-autosmoke`，且 `scripts/check-places-perf-analyzer.sh` 覆盖有效和无效标记夹具。运行时证据路径记录在 `docs/PLACES_RENDERER_PLAN.md` 中：`/tmp/fika-places-retained-hit-test.log` 通过了当前 GPUI 策略门，且 `/tmp/fika-places-custom-retained-hit-test.log` 通过了可选自定义行视觉策略门。
- [x] P16ad：在保留渲染器边界稳定后润色面向用户的 Places 侧栏布局控件。当前代码已经具有运行时宽度、隐藏/显示、重置、设置持久化和 `FIKA_AUTOSMOKE_PLACES=layout`。Dolphin 将 Places 面板停靠操作暴露为 `show_places_panel` 和 `Qt::Key_F9`；Fika 现在用 F9 Places 切换镜像该行为，而工具栏按钮共享相同的应用级可见性路径。单元覆盖证明了快捷键分类以及切换保留最后侧栏宽度。Pane viewport 重测保持由布局 autosmoke 覆盖；`/tmp/fika-places-f9-layout.log` 通过了 `--require-layout-autosmoke --expect-current-gpui-policy`。
- [x] P16ae：将保留 Places hit-test autosmoke 报告所有权移出 `src/main.rs` 并移入 `src/ui/places/autosmoke.rs`。应用根现在仅提供投影的 `PlaceSnapshot` 列表；Places 拥有保留行/section 采样、预期边缘/主体区域、摘要计算和模块级测试。这在行/section 事件传递离开 GPUI shell 之前保持运行时证据收集与 Places model/controller 边界对齐。证据：`/tmp/fika-places-hit-test-autosmoke-module.log` 通过了 `--require-hit-test-autosmoke --expect-current-gpui-policy`。
- [x] P16af：将 Places autosmoke 快照摘要所有权移出 `src/main.rs` 并移入 `src/ui/places/autosmoke.rs`。Places 模块现在拥有可见行计数、section 转换计数、活动行计数、place 目标计数以及非破坏性运行时 smoke 日志的插入前/后计数。这使目标/溢出/布局证据保持在 Places 投影使用的相同保留快照边界上。证据：`/tmp/fika-places-snapshot-autosmoke-module.log` 通过了 `--require-autosmoke --expect-current-gpui-policy`。
- [x] P16ag：将 Places 布局 autosmoke 报告移出 `src/main.rs`。`src/ui/places/autosmoke.rs` 现在拥有侧栏布局 smoke 状态类型、调整大小目标策略、布局操作日志格式和保存的设置验证报告。应用根仍然变更面板可见性/宽度并读取设置，但不再拥有隐藏、显示、调整大小、重置、恢复或验证的证据/报告逻辑。证据：`/tmp/fika-places-layout-autosmoke-module.log` 通过了 `--require-layout-autosmoke --expect-current-gpui-policy`。
- [x] P16ah：将 Places 放置目标 autosmoke 操作报告移出 `src/main.rs`。`src/ui/places/autosmoke.rs` 现在拥有非破坏性 DropTargets 场景使用的目标路径标签、插入索引操作报告和清除目标操作日志格式。应用根仍然选择并变更目标状态，但 Places 模块拥有分析器消耗的运行时证据标记。证据：`/tmp/fika-places-target-actions-autosmoke-module.log` 通过了 `--require-autosmoke --expect-current-gpui-policy`。
- [x] P16ai：将 DropTargets 首个 place 选择规则移出 `src/main.rs`。`src/ui/places/autosmoke.rs` 现在拥有为非破坏性 place 目标操作选择第一个已挂载 `PlaceSnapshot` 的规则。应用根仍然将所选路径应用于应用状态，但场景 model 不再依赖于对投影 Places 行的应用根迭代。证据：`/tmp/fika-places-first-target-autosmoke-module.log` 通过了 `--require-autosmoke --expect-current-gpui-policy`。
- [x] P16aj：将 Places autosmoke 启动/完成标记格式化移出 `src/main.rs`。`src/ui/places/autosmoke.rs` 现在拥有分析器消耗的稳定场景标记标签，而不是依赖于应用根 `Debug` 格式化。应用根仍然调度场景操作，但标记表面属于 Places autosmoke 模块。证据：`/tmp/fika-places-start-complete-autosmoke-module.log` 通过了 `--require-autosmoke --expect-current-gpui-policy`。
- [x] P16ak：将条目视图 autosmoke 标记格式化移出 `src/main.rs`。`src/ui/file_grid/autosmoke.rs` 现在拥有 `FIKA_AUTOSMOKE_ITEM_VIEW` 的稳定场景标签加上启动/完成、缩放操作和滚动操作标记格式化。应用根仍然将缩放和滚动应用于 pane 状态，但条目视图运行时证据标记属于文件网格 autosmoke 模块。证据：`/tmp/fika-item-view-autosmoke-marker-module.log` 通过了用于 `/etc` 缩放/滚动证据的条目视图分析器门。
- [x] P16al：在分析器中要求条目视图 autosmoke 标记。条目视图性能分析器现在支持 `--require-autosmoke`，并验证 `Zoom`、`Scroll` 和 `ZoomScroll` 场景的 start/complete 标记以及所需 zoom 和 changed scroll 操作。分析器摘要总是包含 `autosmoke:` 行，因此 renderer 证据块能证明日志来自哪个脚本场景。证据：`scripts/check-item-view-perf-analyzer.sh` 覆盖正向 `ZoomScroll` 夹具和缺失 scroll action 的负例夹具。
- [x] P16am：将下一个 Places 迁移边界拆成保留事件传递，而不是把行视觉绘制当作足够。Places 计划现在定义了未来的保留事件策略门，保持 GPUI 拖拽启动 shell 显式，并按 hover/cursor、activation/context-menu targeting、drag-move/drop delivery、最后移除 GPUI 行/section shell 的顺序推进。这避免把可选行视觉绘制器误认为行为完整的保留 Places 行。
- [x] P16an：在更改事件路由之前添加 Places 保留事件传递分析器门。`scripts/analyze-places-perf.sh` 现在支持 `--expect-retained-event-policy`，它接受当前 GPUI 行视觉或聚合的可选自定义视觉层，同时要求 `retained_interaction` 和保留 hitbox 等于 rows+sections、`gpui_event_shells=0`，并保持 drag shells 为 rows。分析器夹具覆盖默认视觉、自定义视觉，以及自定义行视觉仍依赖 GPUI event shell 的负例混合状态。
- [x] P16ao：记录条目视图复用池所有权边界。状态文档现在明确 `VisibleItemSlotPool` 和 `ItemPaintSlotCache` 是 Compact/Icons 可复用条目 identity 的来源，详情绘制状态按 `ItemId` 保留。GPUI id 仅作为 shell/image 表面的消费者存在，不是主要复用机制。未来复用池工作必须保持该边界；如果更改，需要更新保留 slot/paint-slot 测试或运行时 `[fika item-paint-slots]` 证据。
- [x] P16ap：使保留条目 paint-slot 证据可被分析器看见。条目视图分析器现在汇总 `[fika item-paint-slots]` 保留 slot 活动并支持 `--require-paint-slots`；标准运行时日志门使用它，因此 renderer 证据包含 inserted、content、geometry、visual、unchanged、removed 和 entries 最大值。分析器夹具覆盖有效 Compact/Icons/Details paint-slot 日志，以及缺失和空 slot 证据。
- [x] P16aq：使保留条目 renderer-policy 证据受分析器强制。`scripts/analyze-item-view-perf.sh --expect-retained-item-policy` 现在拒绝 renderer-policy 日志，除非每个条目都有保留基础视觉、保留交互和重命名叠加层覆盖所有条目，且剩余 GPUI 拖拽/image 边界在策略计数中保持显式。标准运行时门启用此检查，防止未来把 GPUI shell 误报为已移除或把保留身份退回到 GPUI 子 key。
- [x] P16ar：将原始条目视图快照转换移入文件网格模块。`project_retained_file_grid_snapshot()` 现在拥有从 raw grid snapshot 到 retained render snapshot 的行为保持序列：分配 `VisibleItemSlotPool` slots，通过 `VisibleItemSnapshotCache` 转换，应用 hovered-item 视觉状态，并投影到 `ItemPaintSlotCache`。`src/main.rs` 仍拥有 pane/app 状态存储和图标解析，但不再内联手动连接该保留投影序列。单元覆盖证明了新边界中的 slot 分配、图标请求、paint-slot 插入和 hover 视觉投影。
- [x] P16as：将可见 raw-grid 工作队列移交移入文件网格模块。`queue_raw_file_grid_model_work()` 现在拥有 raw grid snapshot 的 `PaneVisibleWorkKey` 重复工作门，以及 metadata role、thumbnail probe 和 file-icon resolve candidate 队列。`src/main.rs` 保留薄的 pane/app 状态 wrapper 并仍启动后台 worker，但不再内联手动连接三个 scheduler handoff。单元覆盖证明 unchanged work key 会跳过第一次 metadata/icon 工作提交后的重复排队。
- [x] P16at：将保留 hovered-item controller 状态移入文件网格模块。`RetainedHoveredItem` 现在拥有 pane/item hover identity、change detection、pane clearing 和 per-pane lookup，用于保留视觉投影。`src/main.rs` 仍暴露当前 GPUI shell 和保留 hitbox callback 使用的事件入口方法，但状态 model 不再是 app-root 的裸 `Option<(PaneId, ItemId)>`。单元覆盖证明幂等 set、item clear、pane clear 和跨 pane lookup 行为。
- [x] P16au：将保留文件网格 lifecycle cleanup policy 移入文件网格模块。`file_grid/lifecycle.rs` 现在拥有 projection invalidation 与 mode-switch invalidation 分别清理哪些保留 item-view slot、paint slot、snapshot cache、text-shape cache、perf phase/layer stats、hover state、compact width 和 visible work key。`src/main.rs` 仍决定 pane/filter/view-mode 转换何时触发 cleanup，但不再内联重复保留状态清理列表。
- [x] P16av：将可见 metadata role 同步收集移入文件网格模块。`visible_metadata_role_results_for_raw_grid()` 现在拥有 raw grid snapshot 的 visible-candidate 循环、同步 budget cutoff、request filtering 和 metadata role result generation。`src/main.rs` 仍将这些结果应用到 pane model，并在 model role 变化时失效可见快照。单元覆盖证明 zero-budget cutoff 和 visible-only candidate conversion。
- [x] P16aw：将文件网格可见快照 cache 失效策略移入文件网格 lifecycle 模块。`file_grid/lifecycle.rs` 现在拥有 pane-local 和 global visible snapshot cache invalidation，用于 visible icon sync、visible metadata sync 和后台 icon resolve 完成后。`src/main.rs` 仍决定 role/icon 结果何时变化，但这些失效路径不再直接访问 `visible_item_snapshot_caches`。
- [x] P16ax：将保留文件网格投影状态 handoff 移入文件网格模块。`file_grid/retained.rs` 现在拥有 raw-to-retained projection 前后取出并放回 pane-local `VisibleItemSlotPool`、`VisibleItemSnapshotCache` 和 `ItemPaintSlotCache` 状态，包括保留 hovered-item lookup 和 icon snapshot callback。`src/main.rs` 仍决定 pane render 何时需要转换，但不再内联连接 retained slot/cache handoff。
- [x] P16ay：将 app 侧 raw-grid model-work 队列 wrapper 移入文件网格模块。`file_grid/retained.rs` 现在拥有进入 `queue_raw_file_grid_model_work()` 前的薄 pane lookup 和 app-state handoff，而 `src/main.rs` 只消费 metadata/thumbnail/icon 是否排队的布尔值来启动现有 worker。这让 Dolphin 风格的 visible-work dedupe 和角色调度移交保持在文件网格边界后面。
- [x] P16az：将 app 侧 raw file-grid snapshot wrapper 移入文件网格模块。`file_grid/retained.rs` 现在拥有 pane lookup 和 `RawFileGridSnapshotInput` 组装，包括 selection、rename draft、drop-target、filter、source revision 和 compact column-width 状态。`src/main.rs` 仍决定何时需要 snapshot，但不再内联构造 raw file-grid snapshot input。
- [x] P16ba：将 visible metadata sync 应用 wrapper 移入文件网格模块。`file_grid/retained.rs` 现在拥有从 raw grid 收集 visible metadata role results、通过现有 app model result 路径应用结果，并在 visible role 变化时失效 pane visible snapshot cache。后台 metadata worker 仍使用 `src/main.rs` 中共享的 model result application 路径。
- [x] P16bb：将后台 metadata 和 thumbnail 结果应用移入文件网格 retained 边界。`file_grid/retained.rs` 现在拥有将经过 generation 校验的 `MetadataRoleResult` 和 `ThumbnailProbeResult` 批次应用到 pane model 的逻辑，而 `src/main.rs` 只保留 worker 调度、scheduler 完成、继续启动和通知决策。这让 raw-grid 可见同步和后台角色/缩略图结果变更都位于 Dolphin 风格边界中的同一个 retained model 侧。
- [x] P16bc：将文件网格 model-work lifecycle helper 移入 retained 边界。`file_grid/retained.rs` 现在拥有 pane-local metadata-role 和 thumbnail 取消、stale generation 清理，以及 retained 投影使用的文件图标快照查找。`src/main.rs` 仍从 pane load/refresh/close 事件和 worker 调度触发这些动作，但不再拥有 scheduler 清理或图标快照策略。
- [x] P16bd：将 item-view scroll transient state 移入 item-view 模块。`ItemViewScrollState` 现在同时拥有 GPUI scroll handle、布局后短暂以 view 为权威的 frame 计数和 scrollbar-drag 状态。`src/main.rs` 仍负责把 pane `ViewState` 与该 controller 同步，但不再为 item-view scroll lifecycle 携带并行的 `HashMap`/`HashSet` 状态。
- [x] P16be：将 item-view scroll-handle 同步决策逻辑移入 item-view 模块。`ItemViewScrollState` 现在为普通 handle 同步、布局后 view 权威同步和 scrollbar drag 同步返回 `ItemViewScrollSyncAction`。`src/main.rs` 仍负责把最终 scroll 值应用到 pane model，但不再决定哪个 scroll 来源是权威。
- [x] P16bf：将 item-view scrollbar-axis viewport policy 移入 item-view 模块。`ui/item_view.rs` 现在拥有哪些 view mode 使用水平 item-view scrollbar，以及给定 pane width 时的 item viewport width 投影计算。`src/main.rs` 仍提供 pane geometry 并应用 viewport 预热，但不再内联 scrollbar-axis 宽度扣减规则。
- [x] P16bg：将 item-view wheel scroll axis policy 移入 item-view 模块。`ui/item_view.rs` 现在拥有 Compact 如何把滚轮输入映射为水平滚动，以及 Icons/Details 如何保持垂直滚动。`src/main.rs` 仍把结果 delta 应用到 pane model，但不再内联各 view mode 的滚轮轴向映射。
- [x] P16bh：将 item-view view-mode axis-change viewport priming policy 移入 item-view 模块。`ui/item_view.rs` 现在拥有在水平 scrollbar 模式和垂直 scrollbar 模式之间切换时，如何按保留 scrollbar extent 调整缓存的 viewport width/height。`src/main.rs` 仍把结果尺寸写回 pane view 并重置 scroll max。
- [x] P16bi：将 item-view filter-bar viewport-height priming policy 移入 item-view 模块。`ui/item_view.rs` 现在拥有显示或隐藏 filter bar 时如何调整缓存的 item viewport height，并应用 core viewport normalization 规则。`src/main.rs` 仍提供 filter-bar height、写回 pane view height，并保持 scroll handle 的短暂权威状态。
- [x] P16bj：将 item-view window-resize viewport prime policy 移入 item-view 模块。`ui/item_view.rs` 现在拥有 render viewport 尺寸归一化、resize delta 检测，以及把 width/height delta 应用到缓存 item-view extent 的规则。`src/main.rs` 仍更新 pane-row width、根据 split geometry 投影各 pane 的 item width，并写回 pane view 尺寸。
- [x] P16bk：将 item-view layout-change scroll authoritative policy 移入 scroll state。`ItemViewScrollState::preserve_for_layout_change()` 现在拥有缩放或布局变化期间保留 scroll 后，接下来两帧以 view 为权威的交接规则。`src/main.rs` 仍把保留后的 scroll 值写回 pane model，但不再知道这条路径的 frame-count policy。
- [x] P16bl：将 item-view authoritative handle-sync policy 移入 scroll state。`ItemViewScrollState::sync_handle_to_view_authoritatively()` 现在拥有 app 侧 viewport 预热后使用的两帧 view-authoritative 交接规则，例如 filter-bar 可见性变化。`src/main.rs` 仍提供 pane view scroll 值，但不再自己组合 raw handle sync 和 frame-count 标记。
- [x] P16bm：将 item-view bounds-update scroll sync policy 移入 scroll state。`ItemViewScrollState::sync_after_bounds_update()` 现在拥有 viewport bounds 到达后的 scrollbar-drag 分支、普通 handle sync、authoritative tick 和 handle-changed 上报。`src/main.rs` 仍应用返回的 pane-view sync action，但不再自行决定这条 lifecycle 路径。
- [x] P16bn：将 item-view user-scroll handle sync policy 移入 scroll state。`ItemViewScrollState::sync_handle_after_user_scroll()` 现在拥有 wheel 驱动的 pane model scroll 变化后，清理临时 view-authoritative 状态并同步 GPUI scroll handle 的规则。`src/main.rs` 仍应用 pane model scroll，但不再自行组合这些 scroll-state lifecycle 操作。
- [x] P16bo：将 item-view transient-clearing handle sync policy 移入 scroll state。`ItemViewScrollState::sync_handle_to_view_clearing_transients()` 现在拥有 pane loading 保留 model scroll 时，清理 authoritative/scrollbar-drag 临时状态并同步 GPUI handle 的规则。`src/main.rs` 仍提供 pane view scroll 值，但不再自行排序这些 scroll-state 操作。
- [x] P16bp：将 item-view scrollbar-drag sync policy 移入 scroll state。`ItemViewScrollState` 现在拥有 scrollbar drag 更新期间的 authoritative handle sync action，以及 finish-drag 时同时上报 pane-view sync action 和此前是否处于 dragging 的交接规则。`src/main.rs` 仍应用返回的 pane-view sync action，但不再接触这条 lifecycle 路径里的 raw finish/sync 原语。
- [x] P16bq：将 item-view rubber-band drag threshold policy 移入 rubber-band 模块。`ui/rubber_band` 现在拥有激活 pending rubber-band selection 的 Manhattan-distance 启动阈值。`src/main.rs` 仍提供 clamp 后的 content point，并负责启动/更新活动 selection band。
- [x] P16br：将 file-grid viewport window-to-content point policy 移入 viewport 模块。`ui/file_grid/viewport.rs` 现在拥有基于 `PaneViewportGeometry` 和 `ViewState` 把 window position 转换成 scrolled content point，以及 clamp 后 content point 的规则。`src/main.rs` 仍执行 pane lookup，并把这些 point 用于 hit testing、drag target 和 rubber-band selection。
- [x] P16bs：将 file-grid viewport pane hit-testing policy 移入 viewport 模块。`ui/file_grid/viewport.rs` 现在拥有根据 window position 命中 viewport pane 的规则，并保持 `PaneController::pane_ids()` 顺序作为优先级。`src/main.rs` 仍提供当前 pane 顺序和缓存的 viewport geometry，用于跨 pane drag target lookup。
- [x] P16bt：将 pending rubber-band state 移入 rubber-band 模块。`ui/rubber_band` 现在同时拥有 active 和 pending rubber-band 数据模型；`src/main.rs` 仍负责启动、更新、结束这些状态并应用 selection 结果。
- [x] P16bu：将 pending rubber-band activation policy 移入 rubber-band 模块。`PendingRubberBand` 现在通过 `can_activate()` 拥有 pane 匹配和 Dolphin-like Manhattan drag threshold；`src/main.rs` 仍提供 clamp 后的当前 content point，并负责启动/更新 selection。
- [x] P16bv：将 active rubber-band state mutation policy 移入 rubber-band 模块。`RubberBandState` 现在拥有构造、pane ownership 检查和同 pane current-point 更新规则。`src/main.rs` 仍保存 active state、清理 draft、计算相交 item 并应用 selection 变化。
- [x] P16bw：将 rubber-band finish state-clearing policy 移入 rubber-band 模块。`finish_rubber_band_for_pane()` 现在拥有只清理属于目标 pane 的 pending 和 active rubber-band state 的规则。`src/main.rs` 仍决定哪些 lifecycle event 会结束 rubber-band 交互。
- [x] P16bx：将 rubber-band selection activity update policy 移入 rubber-band 模块。`set_rubber_band_selection_activity_for_count()` 现在拥有最新 rubber-band selection count 非零时 pane 才保持 rubber-band-selection active 的规则。`src/main.rs` 仍保存 active pane set 并发出 status text。
- [x] P16by：将 rubber-band selection activity clear/query policy 移入 rubber-band 模块。`clear_rubber_band_selection_activity_for_pane()` 和 `rubber_band_selection_activity_is_active()` 现在拥有生产路径的清理规则，以及带 selected-count 判断的 activity 检查。`src/main.rs` 仍提供 activity set 和 pane selected count。
- [x] P16bz：将 active rubber-band viewport-rect projection 移入 rubber-band 模块。`active_rubber_band_viewport_rect_for_pane()` 现在拥有 pane ownership 检查，以及把 active band 转成渲染用裁剪后 viewport rect 的规则。`src/main.rs` 仍提供 active state snapshot 和当前 pane view。
- [x] P16ca：将 active rubber-band pane ownership query/clear policy 移入 rubber-band 模块。`active_rubber_band_is_for_pane()` 和 `clear_active_rubber_band_for_pane()` 现在拥有生产路径里的 active-band pane 检查和仅清 active 的规则。`src/main.rs` 仍决定哪些 app lifecycle event 请求这类清理。
- [x] P16cb：将 pending rubber-band press state replacement 移入 rubber-band 模块。`press_pending_rubber_band_for_pane()` 现在拥有在 blank-press start 时清理 active band 并安装 pending band 的规则。`src/main.rs` 仍决定 blank press 何时有效。
- [x] P16cc：将 active rubber-band start state replacement 移入 rubber-band 模块。`start_active_rubber_band_for_pane()` 现在拥有 pending drag 激活时清理 pending state 并安装 active band 的规则。`src/main.rs` 仍清理 draft 并应用 selection 更新。
- [x] P16cd：将 active rubber-band update writeback 移入 rubber-band 模块。`update_active_rubber_band_for_pane()` 现在拥有同 pane current point 更新，以及把更新后的 active band 写回 active state slot 的规则。`src/main.rs` 仍使用返回的 band rect 计算 selection。
- [x] P16ce：将 pending rubber-band activation start selection 移入 rubber-band 模块。`pending_rubber_band_activation_start()` 现在拥有判断 pending band 是否可在当前 pane/content point 激活，并返回 active-band startup 所需 start point 的规则。`src/main.rs` 仍提供 clamp 后的当前点，并执行 draft cleanup/selection。
- [x] P16cf：将 file-grid projected hit/intersection query composition 移入 projection 模块。`pane_content_item_hit_at_point()` 和 `pane_model_indexes_intersecting_visual_rect()` 现在拥有构建 pane layout projection、应用 rename-draft visual bounds、并把 filtered layout indexes 映射回 model indexes 的顺序。`src/main.rs` 仍提供 pane/filter/cache 输入，并决定 query result 如何影响 selection、DnD 和 context-menu 行为。
- [x] P16cg：将 item-view scroll sync outcome classification 移入 scroll state。`ItemViewScrollSyncAction::into_outcome()` 现在拥有判断返回的 scroll action 是否携带 pane-view values，以及这些 values 是否不同于当前 view snapshot 的规则。`src/main.rs` 仍负责把返回的 scroll values 写入 pane model。
- [x] P16ch：将 item-view scroll sync view-snapshot API 移入 scroll state。`ItemViewScrollViewSnapshot` 现在承载 handle-sync 和 authoritative-handle sync 路径中的 pane view scroll tuple，`src/main.rs` 在这些生产路径中不再用松散字段传递这些值。
- [x] P16ci：记录后续 MIME/theme icon custom-renderer 工作流。`docs/ITEM_VIEW_RENDERER_DECISIONS.md` 现在记录 retained `(iconName, icon_size)` image-cache 方向、hybrid promotion 选项、禁止同步解码规则，以及替换默认 GPUI `img()` MIME/theme renderer 前必须具备的默认/自定义成对证据门槛。
- [x] P16cj：将 item-view scroll lifecycle snapshot APIs 移入 scroll state。Bounds update、scrollbar-drag finish sync 和 layout-change scroll preservation 现在都有 `ItemViewScrollViewSnapshot` 入口；`src/main.rs` 的生产路径不再用松散字段传递这些 scroll values。
- [x] P16ck：将 item-view handle-to-view snapshot sync APIs 移入 scroll state。Authoritative handle sync、user-scroll handle sync 和 transient-clearing handle sync 现在在生产路径中消费 `ItemViewScrollViewSnapshot`，不再使用松散 scroll 字段。
- [x] P16cl：收窄 item-view scroll tuple helper 可见性。松散字段 scroll helpers 现在只是 scroll-state 实现细节；生产路径和跨模块测试都使用 snapshot API surface。
- [x] P16cm：记录更新后的 GPUI 依赖基线。2026-06-18 的 lockfile 更新将 GPUI 移到 Zed commit `e4f6742a`，当前依赖基线是 Zed commit `69b602c797a62f09318916d24a98c930533fbdc8`；解析后的依赖图不再包含 `async-std`、`async-global-executor` 或旧 Zed `util` crate。这降低了保留 GPUI surface 的依赖重量顾虑，但 renderer 替换决策仍然必须依赖成对运行时证据。
- [x] P16cn：将 item-view scroll sync-action 应用规则移入 scroll state。`ItemViewScrollSyncAction::apply_to_view()` 现在拥有 sync action 何时写入 pane view values，以及该写入是否代表 view change 的判断；`src/main.rs` 只提供 pane model 写入闭包。
- [x] P16co：将 item-view handle-sync action 组合移入 scroll state。`sync_view_from_handle_snapshot()` 和 `sync_view_from_authoritative_handle_snapshot()` 现在拥有 handle action 创建和 view-write 应用；`src/main.rs` 只提供 pane view snapshot 和 pane model 写入闭包。
- [x] P16cp：将 item-view bounds-update 和 scrollbar-finish 的 scroll action 应用移入 scroll state。Bounds 和 drag-finish 路径现在暴露 snapshot API，拥有 action 创建、handle-change 聚合和 view-write 应用；`src/main.rs` 只保留 pane bounds 更新和 pane model 写入闭包。
- [x] P16cq：将 item-view layout-change scroll preservation 写回移入 scroll state。`preserve_layout_scroll_syncing_view_snapshot()` 现在拥有 preserved scroll 计算和 view-write 应用；`src/main.rs` 只提供 pane view snapshot 和 pane model 写入闭包。
- [x] P16cr：将 item-view scroll snapshot tuple 构造移入 item-view 模块。生产路径现在使用 `ItemViewScrollViewSnapshot::from_view_state()`，不再在 `src/main.rs` 手工复制 `scroll_x`、`scroll_y`、`max_scroll_x` 和 `max_scroll_y`。
- [x] P16cs：隐藏内部 item-view scroll sync 计算类型，不再作为跨模块写回协议。公开的 scroll-state 写回回调现在接收 `ItemViewScrollViewSnapshot`，`ItemViewScrollSync` 仅作为 `scroll_state.rs` 内部类型。
- [x] P16ct：收窄 item-view handle-to-view snapshot helper 可见性。`sync_handle_to_view_snapshot()` 现在是 scroll-state 内部 helper；跨模块路径使用 authoritative、user-scroll 或 transient-clearing policy API，而不是 raw handle sync helper。
- [x] P16cu：封装 item-view scroll snapshot 写回。snapshot 字段现在仅在 `scroll_state.rs` 内部可见；`main.rs` 通过 `ItemViewScrollViewSnapshot::apply_scroll_writeback()` 和单一 pane 写回 adapter 写入 pane scroll，不再重复拆开 scroll tuple。
- [x] P16cv：让滚轮滚动变化判断也走 item-view scroll snapshot 协议。`scroll_pane_from_wheel()` 现在比较 pane model scroll 前后的 `ItemViewScrollViewSnapshot`，不再在 `src/main.rs` 手工拼四字段 scroll tuple。
- [x] P16cw：将 item-view scroll snapshot 的 pane 写回 adapter 移入 item-view 模块。`main.rs` 现在只把 `PaneController` 和 `PaneId` 交给 `apply_item_view_scroll_snapshot_to_pane()`，不再拥有拆解 item-view scroll snapshot 的 adapter 逻辑。
- [x] P16cx：将 pane 到 item-view scroll snapshot 的投影移入 item-view 模块。`item_view_scroll_snapshot_for_pane()` 和 `item_view_scroll_snapshot_for_existing_pane()` 现在拥有从 pane `ViewState` 投影到 `ItemViewScrollViewSnapshot` 的规则，`main.rs` 不再保留自己的 pane snapshot helper。
- [x] P16cy：在 `main.rs` 中隐藏直接 item-view scroll snapshot 构造。filter-bar 预热现在使用 `item_view_scroll_snapshot_for_view()`，滚轮滚动使用 `changed_item_view_scroll_snapshot()`，应用侧测试复用 pane snapshot 投影而不是直接构造 `ItemViewScrollViewSnapshot`。
- [x] P16cz：将普通 item-view scroll handle 到 pane 的同步编排移入 item-view facade。`main.rs` 现在把 scroll state、pane controller 和 pane id 交给 `sync_pane_view_from_item_view_scroll_handle()`，不再本地组装 snapshot/writeback 闭包。
- [x] P16da：将 authoritative item-view scroll handle 到 pane 的同步编排移入 item-view facade。scrollbar-drag update 现在通过 `sync_pane_view_from_authoritative_item_view_scroll_handle()` 委托，而不是在 `main.rs` 组装 authoritative handle snapshot/writeback 闭包。
- [x] P16db：将 item-view scrollbar finish 同步编排移入 item-view facade。`finish_item_view_scrollbar_drag()` 现在拥有现有 pane snapshot lookup、缺失 pane 时只结束 drag 的 fallback，以及 pane 写回闭包；`main.rs` 只委托 public action。
- [x] P16dc：将 item-view layout-change scroll preservation 编排移入 item-view facade。缩放/layout 路径现在通过 `preserve_item_view_scroll_for_layout_change()` 委托 preserved-scroll snapshot lookup 和 pane 写回，而不是在 `main.rs` 组装闭包。
- [x] P16dd：将 item-view transient-clearing handle sync 编排移入 item-view facade。保留 pane scroll 的加载过渡现在通过 `sync_item_view_scroll_handle_to_pane_view()` 委托 handle sync 和 transient cleanup，而不是在 `main.rs` 查找 pane snapshot 并直接调用 scroll-state API。
- [x] P16de：将 item-view bounds-update scroll sync 编排移入 item-view facade。`set_pane_viewport_bounds()` 仍通过 pane controller 写 viewport bounds，但后续 handle/action sync 和 pane scroll 写回现在通过 `sync_pane_view_after_item_view_bounds_update()` 执行。
- [x] P16df：将 item-view wheel-scroll 编排移入 item-view facade。`scroll_pane_from_wheel()` 现在通过 `scroll_pane_from_item_view_wheel()` 委托 wheel 轴向映射、pane model scroll、snapshot change detection 和 user-scroll handle sync。
- [x] P16dg：将 item-view authoritative handle-to-view 预热移入 item-view facade。filter-bar viewport 预热现在通过 `sync_item_view_scroll_handle_to_view_authoritatively()` 委托，不再在 `main.rs` 构造 scroll snapshot 并直接调用 scroll-state API。
- [x] P16dh：将 item-view scroll lifecycle 薄入口移入 item-view facade。`main.rs` 现在通过 item-view 函数委托 handle lookup、scrollbar drag start、pane reset 和 pane removal，不再在生产路径直接调用 `ItemViewScrollState` 方法。
- [x] P16di：将 item-view scroll transient 测试查询移入 item-view facade。app 侧测试现在通过 item-view helper 查询 authoritative-scroll 和 scrollbar-dragging 状态，不再从 `main.rs` 直接调用 `ItemViewScrollState` 查询方法。
- [x] P16dj：将 rubber-band 交互状态合并进 rubber-band controller。`main.rs` 现在只持有一个 `RubberBandController`，不再分开持有 pending band、active band 和 selection activity 字段；viewport 与 app 路径通过 controller 方法查询/变更状态，同时保留现有 GPUI drag shell 边界。
- [x] P16dk：将 rubber-band drag-move 的 active/pending 分支移出 viewport shell。GPUI shell 现在只把 drag move 转发给 `move_rubber_band_drag_from_window()`，由 app/controller 边界决定激活 pending band 还是更新 active band。
- [x] P16dl：将 visible file-icon sync handoff 收到 file-grid retained facade 后面。渲染循环现在调用 pane-level `resolve_visible_file_icons_for_raw_grid()` 方法；Dolphin visible-icon sync budget、queue-aware cache sync 和 visible snapshot invalidation 留在 file-grid 模块中，而不是 `main.rs`。
- [x] P16dm：将 file-icon resolve worker 编排移入 file-grid retained facade。批次启动、后台图标解析、queue completion、resolved icon application、visible snapshot invalidation 和继续调度后续批次现在与 file-grid icon work 边界在一起，而不是 `main.rs`。
- [x] P16dn：将 metadata role worker 编排移入 file-grid retained facade。metadata role 批次启动、后台 role 收集、scheduler completion、model result application、继续调度和通知决策现在与 visible metadata sync 放在一起，而不是 `main.rs`。
- [x] P16do：将 thumbnail probe worker 编排移入 file-grid retained facade。thumbnail probe 批次启动、后台缓存探测、scheduler completion、model result application、继续调度和通知决策现在与 thumbnail result application 放在一起，而不是 `main.rs`。
- [x] P16dp：将 visible model work startup 保持在 file-grid retained facade 内。queue 现在返回 typed `QueuedVisibleModelWork` 协议，`main.rs` 只委托 worker 启动，不再拆解 metadata、thumbnail 和 file-icon 三个布尔值。
- [x] P16dq：将 visible metadata resnapshot 编排移入 file-grid retained facade。render loop 现在请求一个已经应用同帧可见 metadata role 结果的 raw grid，并接收更新后的 model data generation，不再从 `main.rs` 重建 raw grid。
- [x] P16dr：将 visible icon sync、model-work queueing 和 queued worker startup 收到一个 file-grid retained facade 入口后面。render loop 保留相同的 icon-sync 与 queue perf 字段，但不再直接串起 metadata、thumbnail 和 icon worker controller 步骤。
- [x] P16ds：将 retained projection frame 组装移入 file-grid retained facade。facade 现在拥有 visible-count 推导、retained slot projection、paint-slot stats 和 item-view perf phase 记录；`main.rs` 只消费 frame 来生成 pane snapshot 和 perf log。
- [x] P16dt：记录 2026-06 依赖更新后的 GPUI 调度依赖边界。设计文档现在说明 `async-std` 和 `async-global-executor` 已移除，但 GPUI/platform async 支持 crate 仍存在；item-view worker 编排应继续留在 file-grid/places facade 后面，而不是回到 `main.rs`。
- [x] P16du：将 raw/work/projection 条目视图渲染管线合并为 pane-level file-grid render frame。`main.rs` 现在以一个 facade 结果接收 file-grid snapshot、item/visible count、slot stats、perf phase 和 timing 字段，不再持有 raw grid 与 model-generation 中间态。
- [x] P16dv：将 item-view perf log 字段映射隐藏到 file-grid render frame 内。`main.rs` 现在只传 pane id、mode 和 pane 总耗时；raw/icon/queue/convert timing、visible count、perf phase 与 slot stats 都封装在 frame 中。
- [x] P16dw：将 same-visible-work-range resize queue invariant 从 app-side 测试移入 file-grid snapshot scheduler 测试。raw snapshot/queue 协议现在由拥有 work key 和 scheduler contract 的模块覆盖，而不是要求 `main.rs` 测试调用低层 file-grid 方法。
- [x] P16dx：推进 Places 自绘层的可见行过滤，但暂不设为默认。根本原因：聚合 Places 行视觉层虽然只用一个 canvas，但 overflow 场景仍在每帧 shape/paint 全部 75 行。实现：`places_row_visual_layer` 在 prepaint 使用 GPUI `Window::content_mask()` 过滤当前滚动裁剪区域，只 shape/paint 可见行；`[fika places-row-visual]` 保留总 `rows` 并新增 `painted`，分析器汇总 `max_painted`。证据：`/tmp/fika-places-custom-targets-visible-rows.log` 通过 targets 自绘策略门，`/tmp/fika-places-custom-overflow-visible-rows.log` 通过 overflow 自绘策略门，overflow 从全量 75 行降为最多 32 个 painted rows，稳态约 `0.6-0.7ms`。仍不默认的原因：首两帧仍存在约 `7-8ms` 字形/文本绘制冷启动尖峰；下一步需要消除或证明它相对 GPUI baseline 中性。
- [x] P16dy：将 Dolphin 对齐的 Places custom chrome 策略设为默认，同时保留 full custom text 为 opt-in。根本原因：Dolphin 高性能 item view 复用可见 widget，并依赖 static text/pixmap cache；Fika 的 full Places canvas text 路径仍有字形/文本冷启动成本。实现：`FIKA_PLACES_ROW_VISUAL_POLICY` 现在支持 `gpui`、默认 `chrome` 和 `full`；chrome 用一个可见行过滤后的 layer 绘制 row background/drop/insert/trash，同时 GPUI 保留文本和图标。分析器新增 `--expect-custom-row-chrome-policy`，跟踪 `text_gpui` 和 `visual_kind`，并拒绝 chrome 路径出现 row shape-cache 日志。证据：`/tmp/fika-places-chrome-targets.log`、`/tmp/fika-places-chrome-overflow.log`、`/tmp/fika-places-chrome-layout.log` 和 `/tmp/fika-places-chrome-hit-test.log` 通过 chrome gate；`/tmp/fika-places-gpui-targets.log` 通过 GPUI fallback gate；`/tmp/fika-places-full-targets.log` 通过 full custom-text gate，但仍保持 opt-in，因为它有 `max_paint=5183us` 和 shape-cache 活动，而 chrome targets 为 `max_paint=83us`、overflow 为 `148us` 且没有 shape-cache channel。
- [x] P16dz0：添加第一个 opt-in full Places 行视觉路径，并用证据证明 GPUI icon 元素可以移除。根本原因：之前 `full` 的命名实际只代表 text-only 自绘，Places 图标仍由 GPUI row 元素渲染。实现：渲染器策略现在区分 `chrome`、`text` 和 `full`；`full` 在现有聚合 sidebar layer 中自绘行文本和矢量 fallback 图标，并发出 `icon_gpui=0`。证据：`/tmp/fika-places-full-icon-targets.log` 通过 `--expect-custom-row-full-policy` 且 `max_icon_gpui=0`；默认 `/tmp/fika-places-chrome-after-full-icon.log` 仍通过 chrome gate。full 路径暂不默认：矢量图标去掉了额外 icon text 成本，但 full text 绘制仍有冷启动尖峰（`max_paint=5669us`），而默认 chrome 行视觉绘制仍是微秒级（`max_paint=63us`）。下一道门：解决 Places 自定义文本冷启动/预热，或继续把 GPUI 文本作为 Dolphin 对齐的默认边界。
- [x] P16dz1：区分 Places full custom-text 的冷 paint 和 warm paint，并移除 opt-in 路径中的每标签 clip layer。根本原因：单看 `max_paint` 会把前两帧 glyph/text paint 冷启动和稳态行绘制混在一起，无法判断路径是根本慢还是冷启动受限。实现：`scripts/analyze-places-perf.sh` 现在在跳过前两个 `[fika places-row-visual]` 帧后报告 `warm_frames`、`max_warm_prepaint` 和 `max_warm_paint`；opt-in Places custom-text painter 直接按最大宽度绘制 `ShapedLine`，不再为每个 label 包一层 `paint_layer`。证据：`/tmp/fika-places-full-direct-text.log` 通过 `--expect-custom-row-full-policy`，`max_icon_gpui=0`、`max_paint=5941us`、`max_warm_paint=667us`；默认 `/tmp/fika-places-chrome-direct-text-check.log` 通过 chrome gate，`max_warm_paint=48us`。结论：full Places visual 仍保持 opt-in；下一步性能目标是前两帧 glyph/text paint，而不是 row model、hit testing、图标绘制或稳态 canvas paint。
- [x] P16dz2：添加显式 Places row-visual paint 晋升门，并记录 Dolphin 对齐的 ownership 规则。根本原因：full Places visual 现在可以移除 GPUI icon/text row 元素，但 `icon_gpui=0` 不足以证明可默认；它还必须证明 cold 和 warm row visual paint 都可接受。这不是等待特殊的 GPUI prewarm API。Dolphin 的缓存行为是应用层设计：稳定 item identity、retained/static text 和 pixmap 状态，以及资源 ready 后才 handoff。实现：`scripts/analyze-places-perf.sh` 支持 `--row-visual-paint-us` 和 `--row-visual-warm-paint-us`；`scripts/check-places-perf-analyzer.sh` 覆盖 warm paint 通过但 cold paint 失败的合成场景。下一道门：实现 Fika 自己拥有的 retained Places text/image handoff，让 full custom rows 在晋升默认前同时通过两个阈值。
- [x] P16dz3：审查 GPUI 高效 `img()` 路径，并记录 custom-image 设计规则。根本原因：GPUI image 性能不是隐藏的同步绘制 primitive，而是 retained resource identity 和延迟 atlas 绘制。实现发现：`img()` 通过 `ImageCache` 解析 `Resource`；`RetainAllImageCache` 以 resource hash 为 key 保存共享后台加载任务或已加载的 `Arc<RenderImage>`，加载完成后通知下一帧；`Window::paint_image` 使用 `(RenderImage.id, frame_index)` 作为 sprite atlas key。后续：custom Places/image 工作必须使用稳定语义 key、retained loaded resources、可见路径不重复 decode/shape replacement，以及 ready-only handoff，才可能超过当前 GPUI image baseline。
- [x] P16dz4：添加第一个 opt-in full custom Places 行 ready-only handoff。根本原因：之前 full 路径会立即隐藏 GPUI text/icon，首个可见 custom 帧就承担 text/glyph 冷 paint，用户能看到切换成本。实现：`FIKA_PLACES_ROW_VISUAL_HANDOFF=1` 在两个 warmup 帧内保留 GPUI text/icon，同时 custom layer 只画 chrome；retained row visual 路径 ready 后再切到 full custom text+icon paint。`scripts/analyze-places-perf.sh` 新增 `--expect-custom-row-handoff-policy` 并汇总 `[fika places-row-handoff]`；分析器 fixture 覆盖 fallback-to-ready 成功和缺少 ready 帧失败。证据：`/tmp/fika-places-full-handoff.log` 通过 `--expect-custom-row-handoff-policy --row-visual-paint-us 1000 --row-visual-warm-paint-us 1000`；fallback 帧 chrome paint 约 `50-59us`，ready full-custom 帧 paint 约 `230-286us`，旧的 5-6ms full 冷 paint 尖峰已不在可见 handoff 路径上。默认晋升前剩余：压低 ready 帧 text-shape prepaint miss（本次 `max_prepaint=1175us`）、补 overflow/layout handoff 证据，并把同样的 stable-key/ready-only 模式扩展到真实 image resource，而不只是在 Places fallback vector icon 上成立。
- [x] P16dz5：使用 handoff warmup 帧预填 retained Places row text shapes，并为 row-visual prepaint 加门。根本原因：P16dz4 已把可见 text/glyph paint 尖峰移出 handoff 路径，但第一个 ready 帧仍在 prepaint 承担 `PlacesRowTextShapeCache` miss。实现：`places_row_visual_layer` 新增 `warm_text_shapes` 输入；`FIKA_PLACES_ROW_VISUAL_HANDOFF=1` fallback 模式下继续显示 GPUI text/icon，只画 chrome，并把可见 label shape 进 app-owned cache 而不绘制。`scripts/analyze-places-perf.sh` 新增 `--row-visual-prepaint-us` 和 `--row-visual-warm-prepaint-us`。证据：`/tmp/fika-places-full-handoff-prewarm.log` 通过 `--expect-custom-row-handoff-policy --row-visual-prepaint-us 300 --row-visual-paint-us 1000 --row-visual-warm-prepaint-us 100 --row-visual-warm-paint-us 1000`，`max_prepaint=113us`、`max_warm_prepaint=54us`、`max_warm_paint=282us`。overflow/layout 也通过 handoff gate：`/tmp/fika-places-full-handoff-overflow.log` 为 `max_painted=29`、`max_warm_prepaint=77us`、`max_warm_paint=1058us`；`/tmp/fika-places-full-handoff-layout.log` 为 `max_warm_prepaint=47us`、`max_warm_paint=282us`。默认晋升前剩余：判断 overflow full-custom text paint 约 1ms 相对 chrome/default 证据是否可接受，然后把同样 ready-only retained-resource 模型扩展到真实 image resource。
- [x] P16dz6：采集 full Places handoff 决策所需的 default-chrome 配对证据。根本原因：P16dz5 后 full handoff 已没有 cold prepaint/paint 尖峰，但晋升默认仍必须和当前默认策略配对比较。证据：默认 chrome 运行 `/tmp/fika-places-chrome-targets-compare.log`、`/tmp/fika-places-chrome-overflow-compare.log` 和 `/tmp/fika-places-chrome-layout-compare.log` 均通过 `--expect-custom-row-chrome-policy`。与 handoff 日志相比，row-visual paint 仍是 chrome 更低：targets `85us` 对 full handoff `282us`，layout `64us` 对 `282us`，overflow 29 个 painted rows 为 `154us` 对 `1058us`。决策：full Places handoff 目前继续 opt-in。这不否定 custom-renderer 方向，因为 chrome row-visual 指标不包含 GPUI text/icon subtree 成本；下一步证据应补 total render/chrome-vs-full accounting，或继续优化 retained custom text paint 后再考虑默认晋升。
- [x] P16dz7：在 Places analyzer 中加入 total render accounting，并采集第一组 chrome-vs-full overflow render 配对。根本原因：只看 row-visual 会显示 full custom text paint 明显高于 chrome，但 chrome row visual layer 本来就不包含 GPUI text/icon subtree。实现：分析器现在解析 `[fika render]` 并支持 `--render-total-us`。证据：同时开启 `FIKA_PERF_ITEM_VIEW=1` 和 `FIKA_PERF_PLACES_VIEW=1` 后，`/tmp/fika-places-chrome-overflow-render.log` 与 `/tmp/fika-places-full-handoff-overflow-render.log` 均通过各自 policy gate。row visual 仍是 chrome 更低（`max_warm_paint=153us` 对 full handoff 本轮 `max_warm_paint=2041us`），但 ready-frame total render 没有按这个差距恶化：chrome 稳态帧到 `2035-4959us`，full handoff ready 帧为 `1463-2130us`。决策：full handoff 在更多重复 total-render 证据前继续 opt-in，但默认晋升指标应同时使用 total render accounting 和 row visual prepaint/paint。
- [x] P16dz：添加 Places chrome 默认之后的全面 retained renderer 路线图。新的 `docs/FULL_RETAINED_RENDERER_ROADMAP.md` 及中文翻译定义当前基线、显式 GPUI bridge、不可违反的 Dolphin 对齐规则，以及六条执行轨道：证据冻结、MIME/theme icon renderer、Places retained event delivery、drag-start 边界、rename editor 和 ownership cleanup。这为后续继续全面转向提供单一规划入口。
- [x] P16ea：添加 retained MIME/theme icon image cache 设计。新的 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md` 及中文翻译定义 Dolphin `QPixmapCache` 对比、保守的 `ThemeIconImageKey`、retained same-key loaded/pending/failed/stale image 状态、所有权边界、默认 vs custom 的配对运行时证据，以及 custom theme-icon 绘制成为默认前必须通过的 TODO gate。
- [x] P16eb：实现 retained MIME/theme icon image cache 基础。`src/ui/icons/image_cache.rs` 现在拥有 `ThemeIconImageKey`、`RetainedThemeIconImageCache` 和 loaded/pending/failed/stale 状态。custom image layer 保持 thumbnail 按 thumbnail path keyed，但 theme/MIME icon 改走包含 size/scale 的 key，Details visual icon 也接入同一路径。根本原因：旧 custom A/B 路径只按 `iconName` retain theme image，缩放时可能在当前尺寸图像加载前复用旧尺寸图像。默认 MIME/theme icon 仍走 GPUI `img()`，直到配对证据证明 custom 路径不差或更优。
- [x] P16ec：添加 item-image 成对 default-promotion gate。`scripts/compare-item-image-renderers.sh --gate-default-promotion` 现在会在 custom 日志包含 theme placeholder、theme decode churn、缺少 custom item-image frame 或 default/custom renderer-policy 证据无效时非零退出。`scripts/check-item-view-perf-analyzer.sh` 覆盖失败和通过的合成对比；真实 `/etc` 和混合目录运行时证据仍属于 P16k2。
- [x] P16ed：在 retained theme image key 落地后采集第一组真实 `/etc` default-vs-custom P16k2 证据。默认日志：`/tmp/fika-icon-default-etc-p16k2.log`；custom 日志：`/tmp/fika-icon-custom-etc-p16k2.log`。default-promotion gate 正确失败，因为 custom 虽然 renderer-policy 证据有效，但仍产生 `theme_placeholder=118` 和 `theme_decoded=5`。这确认下一步架构应先做 prewarm 或 hybrid delivery，而不是马上把普通 MIME/theme icon 全量切到 custom image layer。
- [x] P16ee：添加 opt-in theme-icon prewarm telemetry 和 runtime 证据。`FIKA_PREWARM_THEME_ICONS=1` 会为 GPUI-rendered theme icon 添加不绘制的 image-layer prewarm item，并扩展 `[fika item-image]`：`theme_prewarm_loaded`、`theme_prewarm_decoded`、`theme_prewarm_retained` 和 `theme_prewarm_pending`。`/tmp/fika-icon-prewarm-etc-p16k2.log` 证明该 bridge 保持默认 GPUI renderer policy，且不暴露 custom placeholder（`theme_placeholder=0`、`paint_count=0`），同时预热 retained image。这仍是中间 bridge，不是默认提升。
- [x] P16ef：添加成对 hybrid handoff gate。`scripts/compare-item-image-renderers.sh --gate-hybrid-handoff` 现在会在 candidate 日志没有同时显示 GPUI fallback、prewarm 活动、ready-key image-layer paint，或仍有可见 theme placeholder/decode churn 时失败。`scripts/check-item-view-perf-analyzer.sh` 覆盖通过和失败的合成 hybrid 对比；真实 `/etc` 和混合目录提升证据仍由 P16k2/P16k2a 跟踪。
- [x] P16eg：让 zoom 后的 MIME/theme icon path identity 与 Dolphin 的稳定 `iconName` role 对齐。根本原因：旧 `FileIconCacheKey` 将 `size_px` 纳入 exact key，zoom 后即使同一文件图标类型已经有 resolved path，也会生成新的 exact-size request，可能造成可见帧 path lookup、GPUI image identity 二次提交和体感图标大小跳变。实现：`FileIconCache::resolve_request_for()` 和 `resolve_now_for()` 在同一 `FileIconKind` 已有 resolved path 时直接视为 cached，visible icon sync 看到无 request 就计入 cached 并跳过同步解析；exact key 已解析但无 path 的负结果也视为已完成，防止 negative theme lookup 循环；`find_icon_direct()` 先跳过不存在目录并用一次 metadata 检查文件和长度，降低 theme miss 的系统调用成本。验证：`cargo fmt --check`、`cargo check`、`cargo build`、`cargo test -q`、`scripts/check-item-view-perf-analyzer.sh` 和 `scripts/check-places-perf-analyzer.sh` 通过；当前自动运行环境没有 Wayland compositor，`/etc` runtime autosmoke 触发 GPUI `NoCompositor`，需要在桌面会话刷新真实日志。
- [x] P16eh：添加实现级 Places retained event-delivery 计划。`docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.md` 及中文翻译现在定义 Dolphin 边界、当前 GPUI-shell policy、目标 retained-hitbox policy、sidebar-level event layer、scroll-local 坐标规则、分阶段迁移顺序、analyzer/smoke 要求和 TODO。该计划在 Track 4 之前继续保留 GPUI row drag-start shell，并把下一实现切片定义为 opt-in、无行为变更的 retained hitbox layer。
- [x] P16ei：添加第一段 Places event-delivery policy 实现。`PlacesEventDeliveryPolicy` 现在默认
  `GpuiShells`，并支持 `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-probe`。probe 会在
  renderer/interaction policy 日志中报告 `retained_probe_hitboxes=rows+sections`，同时保持
  `retained_hitboxes=0` 和 `gpui_event_shells=rows+sections`，因此不能满足未来 retained-event
  gate。这记录了 Dolphin 对齐结论：Places 全自绘性能需要 viewport-level event ownership，
  不只是 row chrome paint。
- [x] P16ej：添加非变更 Places retained event probe layer。opt-in layer 消费
  `PlacesInteractionGeometry`，通过 `Window::insert_hitbox()` 插入 normal row/section
  hitbox，并报告 `[fika places-event-probe]`；它不注册 event handler，也不改变 app state。
  分析器现在有 `--require-event-probe`，证明插入 hitbox 数匹配 `retained_probe_hitboxes`，
  同时 retained-event gate 仍会拒绝该 mixed GPUI-shell policy。
- [x] P16ek：添加第一段 Places retained-pointer event 切片。opt-in
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-pointer` policy 复用 retained event layer
  来设置 row pointer cursor，并在 drag 离开 retained layer bounds 时清理 active Places
  drop target。该 policy 下 row shell cursor styling 被关闭，但 GPUI row/section shell
  仍拥有 click、context menu、typed DnD move/drop 和 drag start。
  `[fika places-event-probe]` 现在对这个 mixed state 报告 `pointer=1`，且 retained-event
  gate 仍会拒绝它。
- [x] P16el：添加 Places retained-targeting event 切片。opt-in
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-targeting` policy 继续使用 sidebar retained
  event layer，但现在 row activation 以及 row/section context menu targeting 从 retained
  row/section hitbox 派发。该 policy 下 GPUI row `on_click`、row right-click 和
  section right-click shell 被关闭。typed DnD move/drop 和 row drag-start 仍留在
  GPUI shell，因此 analyzer 会记录 `retained_targeting=rows+sections` 以及
  `pointer=1 targeting=1`，同时完整 retained-event gate 仍拒绝这个 mixed state。
- [x] P16em：添加 Places retained-DnD event 切片。opt-in
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=retained-dnd` policy 使用一个 sidebar-level GPUI
  typed drag shell，因为 GPUI 通过 `Div::on_drag_move` / `Div::on_drop` 暴露 app 内部
  typed drag payload。该 policy 下 row/section DnD move/drop shell 被关闭，retained
  `PlacesInteractionGeometry` 拥有 item、external-path 和 place drag 的 row/section
  target lookup。row drag-start 仍留在 GPUI shell。analyzer 记录
  `retained_dnd=rows+sections`、`gpui_event_shells=1` 以及
  `pointer=1 targeting=1 dnd=1`；完整 retained-event gate 仍拒绝这个 mixed state。
- [x] P16en：添加非破坏性 Places retained DnD autosmoke。
  `FIKA_AUTOSMOKE_PLACES=dnd` 场景现在会记录 path-list drag 经过 row body、row edge、
  section heading，以及 place drag 经过另一个 row 的 retained target-decision 采样。
  `scripts/analyze-places-perf.sh` 支持 `--require-retained-dnd-autosmoke`，
  `scripts/check-places-perf-analyzer.sh` 覆盖有效 marker 和失败 decision 的无效夹具。
  这证明了 Dolphin 风格 retained geometry/controller decision 边界，同时不改变用户
  Places 排序。证据：`/tmp/fika-places-retained-dnd.log` 通过
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`。
- [x] P16eo：将 Places drag-start source modeling 移出 row shell。GPUI 平台边界仍然
  需要 row `Div::on_drag`，但 `src/ui/places/drag.rs` 现在拥有从 `PlaceSnapshot` 投影
  `PlaceDragStartSource` 的逻辑，包括 path、label、icon、source index、movable flag、
  export payload 和 preview model。`[fika places-interaction-policy]` 现在报告
  `drag_start_models=rows`，Places analyzer 会拒绝 model 数量不匹配可见 row 数的
  interaction 日志。这在保留 drag-start shell 的同时让 Dolphin 风格 source model 边界
  保持显式。证据：`/tmp/fika-places-drag-start-model.log` 通过
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`，
  且 `max_drag_start_models=11`。
- [x] P16ep：集中剩余的 Places GPUI drag-start shell installer。row construction
  现在调用 `src/ui/places/drag.rs` 中的 `install_place_drag_start_shell()`，而不是内联安装
  `Div::on_drag` 和构造 `PlaceDragPreview`。这让平台 shell 保持显式，同时 payload
  projection、preview construction 和 GPUI drag-start wiring 共享同一个 drag 模块边界。
  证据：`/tmp/fika-places-drag-start-model.log` 通过
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`。
- [x] P16eq：添加 retained Places content-y conversion 和边界测试。
  `places_content_y_from_viewport_y()` 现在集中 viewport-local y 加 scroll offset 后进入
  retained hit testing 的转换规则，单元覆盖证明非零 scroll 会映射到预期 row/section，
  同时 row、section 和 content bounds 保持半开区间。这能防止未来 viewport-level event
  layer 不再位于 scroll content 内时回退 drop/activation 目标。
- [x] P16er：区分 retained probe hitbox 和 retained target-delivery hitbox。
  `retained_probe_hitboxes` 继续表示插入的 retained layer hitbox 数，而
  `retained_hitboxes` 现在只在 `retained-targeting` 和 `retained-dnd` 这种 row/section
  hitbox 实际派发 target 时变成 rows+sections。完整 retained-event gate 不变，在
  `gpui_event_shells=0` 前仍会拒绝这些 mixed state。证据：
  `/tmp/fika-places-hitbox-accounting.log` 通过
  `--require-retained-dnd-autosmoke --require-interaction-policy --require-interaction-geometry --expect-custom-row-chrome-policy`，
  且 `max_retained_hitboxes=13`；`--expect-retained-event-policy` 仍按预期失败。
- [x] P16es：让 Places renderer retained-interaction accounting 按 event policy 计数。
  `PlacesEventDeliveryPolicy::retained_interaction()` 现在在 `retained-targeting` 和
  `retained-dnd` 下报告 rows+sections，因为 retained event layer 在这些 policy 中实际拥有
  row/section target delivery；probe 和 pointer-only policy 继续报告 0。Places analyzer
  会按这个 event-policy-aware 计数验证 custom chrome/full visual policy，但完整
  retained-event gate 在 `gpui_event_shells=0` 前仍拒绝 `retained-dnd`。
- [x] P16et：添加非变更 retained Places targeting autosmoke。
  `FIKA_AUTOSMOKE_PLACES=targeting` 场景现在会从 `PlacesInteractionGeometry` 采样
  retained activation-row、row context-menu 和 section context-menu target
  classification，不会 activate place，也不会打开菜单。`scripts/analyze-places-perf.sh`
  现在支持 `--require-retained-targeting-autosmoke`，并会在任何 retained-targeting 默认提升前拒绝缺失或失败的 targeting 采样。
- [x] P16eu：将 Places event delivery 默认提升到 retained-DnD mixed policy。
  `places_event_delivery_policy()` 现在默认回退到 `RetainedDnd`，同时
  `FIKA_PLACES_EVENT_DELIVERY_POLICY=gpui` 仍保留为显式 GPUI row/section event-shell
  fallback。默认日志应显示 `event_policy=retained-dnd`、
  `retained_hitboxes=rows+sections`、`gpui_event_shells=1` 和
  `drag_start_models=rows`；完整 retained-event analyzer gate 在移除 sidebar typed DnD
  shell 前仍应失败。
- [x] P16ev：从 retained pointer policy 移除冗余 root sidebar GPUI leave-clear
  shell。retained event layer 已经会在 active drag 离开自身 bounds 时清理 active
  Places drop target，因此 retained-pointer、retained-targeting 和 retained-DnD 不再安装
  item、external-path 和 place 三个 root sidebar `on_drag_move` leave handler。GPUI 和
  probe policy 继续保留这三个 fallback shell。interaction policy 日志现在报告
  `gpui_sidebar_leave_shells`，analyzer 会拒绝重新引入这些 shell 的 retained-DnD
  日志，同时不放松完整 retained-event gate。
- [x] P16ew：将剩余 Places GPUI event-shell accounting 拆成 row/section event shell
  和 sidebar typed DnD payload shell。interaction policy 日志现在除了总
  `gpui_event_shells` 外，还报告 `gpui_row_section_event_shells` 和
  `gpui_typed_dnd_payload_shells`。默认 retained-DnD 必须显示
  `gpui_row_section_event_shells=0` 和 `gpui_typed_dnd_payload_shells=1`，证明
  row/section target delivery 已经 retained，而 typed payload 入口仍是 GPUI 平台边界。
  完整 retained-event gate 仍要求两个拆分计数都为 0。
- [x] P16ex：依赖更新后重新审计 GPUI drag-start API。当前 GPUI `0.2.2` 位于 Zed
  `69b602c797a62f09318916d24a98c930533fbdc8`，类型化拖拽启动仍只通过
  interactive element 暴露，而不是 retained painter hitbox。Track 4 现在记录了移除
  Compact/Icons、Details 或 Places drag-start shell 前所需的最小审计 patch/API 形状：
  payload、preview entity、cursor offset、transfer modes、cancel、同窗口 drop dispatch
  和 external drop 行为都必须保留，并且不能为了作为拖拽源而重新创建可见 GPUI row。
- [x] P16ey：添加 Track 1 retained-renderer evidence checklist。新的
  `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.md` 和中文翻译定义了桌面会话命令、
  `/tmp` 日志名、analyzer gate、image A/B gate、Places retained-DnD 期望、手动
  DnD/rename smoke 提醒，以及提升 custom renderer 或移除 GPUI bridge 前必须遵守的记录规则。
- [x] P16ez：添加 retained-renderer evidence runner。新的
  `scripts/run-retained-renderer-evidence.sh` 自动化 core Track 1 item 和 Places
  桌面会话采集，运行对应 analyzer gate，并验证当前 Places full-retained gate 在 typed
  DnD payload shell 移除前仍按预期失败。MIME/theme icon A/B 证据位于 `--icons` 后面，
  因此当前尚不可提升的 custom icon 路径不会阻塞每次基线冻结。
- [x] P16fa：记录跨 surface 的 Dolphin retained-renderer 对齐契约。
  `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.zh-CN.md` 说明为什么全自绘理论上仍然有效，
  为什么当前差距是 model/cache/event 闭环不完整而不是 GPUI 天然更快的证明，并明确在移除
  GPUI bridge 或把 custom renderer 提升为默认前，model、layout、role-readiness、
  painter、controller 和 analyzer gate 必须满足哪些条件。
- [x] P16fb：将 retained-renderer evidence runner 拆出更窄的 item-only 和
  Places-only 模式。`scripts/run-retained-renderer-evidence.sh --items-only` 现在只采集
  item-view Track 1 日志；`--places-only` 只采集 Places targets/overflow/layout/hit-test/
  targeting/DnD 日志，并且仍会验证完整 retained-event gate 按预期失败。默认 `--core`
  行为保持为 item 加 Places。
- [x] P16fc：添加 hybrid MIME/theme icon 证据 runner 模式。
  `scripts/run-retained-renderer-evidence.sh --hybrid-icons` 会为 `/etc` 和混合用户目录采集
  default 与 `FIKA_HYBRID_THEME_ICONS=1` 的 zoom-scroll 配对日志，然后运行
  `scripts/compare-item-image-renderers.sh --gate-hybrid-handoff`。这让下一步 image
  readiness 工作可以被度量，而不需要把当前尚不可提升的 full custom icon 路径强行通过
  `--gate-default-promotion`。
- [x] P16fd：让 retained-renderer evidence runner 的选择语义显式化。
  脚本现在只在没有传入任何选择参数时默认启用 core item+Places 采集，因此单独
  `--hybrid-icons` 只运行 hybrid icon handoff 证据，而 `--core --hybrid-icons`
  仍会同时运行两组。
- [x] P16fe：采集 `/etc` 和混合用户目录的成对 hybrid MIME/theme icon 证据。
  `scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-icons-20260619`
  在 `/etc` 和 Downloads 都通过了 `--gate-hybrid-handoff`，并且
  `theme_placeholder=0`、visible `theme_decoded=0`。结果支持了中间 handoff 步骤；
  后续 full custom 工作已经取代 hybrid 成为默认。
- [x] P16ff：添加严格的 hybrid icon 默认提升 gate。
  `scripts/compare-item-image-renderers.sh --gate-hybrid-default-promotion`
  现在在 handoff gate 之上，用显式容差比较 `icon_sync`、item-view phase max total、
  static visual prepaint/paint 和 image paint 与 GPUI baseline。2026-06-19 的 `/etc` 和
  Downloads hybrid 日志都通过了这个更严格的 gate，因此下一段代码切片当时可以尝试默认
  hybrid renderer policy；该路径后来被 full custom 默认取代。
- [x] P16fg：将普通 MIME/theme icon 默认切到 hybrid renderer，作为 full custom 前的中间态。
  `FIKA_GPUI_THEME_ICONS=1` 现在强制旧 GPUI `img()` baseline；默认路径会让尚未 ready 的
  theme-icon key 继续走 GPUI，并把 ready key 交给 retained custom image layer。证据
  runner 现在用 `FIKA_GPUI_THEME_ICONS=1` 采集 baseline 日志，而默认候选不再需要 hybrid env。
  证据：`scripts/run-retained-renderer-evidence.sh --hybrid-icons --skip-build --prefix fika-hybrid-default-20260619`
  在 `/etc` 和 Downloads 都通过了 `--gate-hybrid-default-promotion`。
- [x] P16fh：默认 hybrid 中间态切换后同步顶层 roadmap 和状态文档。
  `docs/FULL_RETAINED_RENDERER_ROADMAP.md`、`docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.md`
  和 `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.md` 当时把 MIME/theme icon 状态描述为
  hybrid-by-default，并明确 `FIKA_GPUI_THEME_ICONS=1` 是 baseline override；后续文档已更新为 full custom 默认。
- [x] P16fi：记录剩余 Places typed DnD payload bridge 边界。
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.zh-CN.md` 现在区分默认 row/section event
  callback 移除与仍然需要的 sidebar typed payload bridge。默认 retained-DnD 必须显示
  `gpui_row_section_event_shells=0` 和 `gpui_typed_dnd_payload_shells=1`。后者在 retained
  hitbox 能传递 typed `ItemDrag`、`ExternalPaths` 和 `PlaceDrag` move/drop payload，并且完整
  retained-event analyzer 加隔离 DnD smoke 通过之前，仍是经过审计的 GPUI API 边界。
- [x] P16fj：依赖更新后重新审计 GPUI typed drag-move/drop delivery。当前
  `Cargo.lock` 将 GPUI 解析到 Zed `69b602c797a62f09318916d24a98c930533fbdc8`；
  `DragMoveEvent<T>`、`Interactivity::on_drag_move<T>()` 和
  `Interactivity::on_drop<T>()` 仍是 interactive-element API，而
  `Window::insert_hitbox()` 和 `Window::on_mouse_event<Event: MouseEvent>()` 仍没有为
  retained painter hitbox 暴露 typed drag payload。这确认 Places sidebar typed payload
  bridge 仍是 API 边界，不是可直接移除的 row/section shell debt。
- [x] P16fk：将 Track 4 扩展为 retained typed drag API 设计。roadmap 现在把 drag
  start 和 typed drag-move/drop payload delivery 视为同一组 GPUI 边界。最小 patch 形状拆成
  retained hitbox drag source 注册和 retained hitbox drag target callback，二者都以 retained
  `HitboxId` 为 key，并明确禁止为了替代 shell ownership 而重新创建可见 GPUI row/item。
- [x] P16fl：记录拖拽过程中 pane/Places target isolation 回归。
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.zh-CN.md` 现在说明 GPUI typed
  `on_drag_move` capture handler 不会自动被 element bounds 裁剪。retained Places typed
  payload bridge 必须对 move event 做 bounds gate，pointer 离开 Places 时只清 Places state，
  并避免清掉由 pane preview/window drag tracking 拥有的 pane item target。
- [x] P16fm：将 Places drag-bounds debug trace 加入 evidence checklist。
  `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.zh-CN.md` 现在说明手动
  `FIKA_DEBUG_DND=1` pane-drag trace 可以包含 `places-dnd-leave`；这行日志证明 Places
  typed bridge 拒绝了 bounds 外的 capture drag move，并且当 pointer 位于 pane 内时，它不应伴随
  持续的 Places 高亮残留。
- [x] P16fn：给 retained Places DnD autosmoke 增加 no-target clear 路径。
  `FIKA_AUTOSMOKE_PLACES=dnd` 现在会输出一个预期为 `Clear`/`NotAllowed` 的
  `path-outside` 样本，并且 `scripts/analyze-places-perf.sh
  --require-retained-dnd-autosmoke` 会强制要求它存在。这在无人值守 smoke 中守住拖拽中的
  target isolation；手动 `places-dnd-leave` trace 仍然是 GUI bounds 证据。
- [x] P16fo：在 retained-DnD 默认后同步顶层 Places 状态。
  `docs/FULL_RETAINED_RENDERER_ROADMAP.zh-CN.md`、
  `docs/ITEM_VIEW_CUSTOM_PAINT_STATUS.zh-CN.md` 和
  `docs/DOLPHIN_RETAINED_RENDERER_ALIGNMENT.zh-CN.md` 现在把 Places 描述为 custom row
  chrome 加 retained row/section target delivery；仍在 GPUI 上的只剩 text/icons、
  sidebar typed payload bridge 和 row drag-start shell。完整 retained Places gate 仍要求移除
  typed payload 和 drag-start 边界。
- [x] P16fp：在 hybrid icon 默认后同步 image decisions。
  `docs/ITEM_VIEW_RENDERER_DECISIONS.zh-CN.md` 和
  `docs/RETAINED_ICON_IMAGE_CACHE_PLAN.zh-CN.md` 现在把 hybrid 视为当前默认，
  `FIKA_GPUI_THEME_ICONS=1` 视为旧 GPUI baseline，`FIKA_CUSTOM_THEME_ICONS=1` 视为
  full-custom 压力路径。剩余 image TODO 是保持 hybrid 默认，直到未来 full-custom 运行能在
  `/etc` 和混合目录中击败 hybrid/default gate，且没有 placeholder、zoom-decode、
  image-paint 或 renderer-policy 回归。
- [x] P16fq：将 item-image 比较输出从 default log 改为 baseline log。
  `scripts/compare-item-image-renderers.sh` 现在把第二个日志称为 `BASELINE_LOG`，
  这符合当前 hybrid 默认工作流：由 `FIKA_GPUI_THEME_ICONS=1` 提供 GPUI image-element
  baseline。
- [x] P16fr：记录 active drag 期间 pane/Places drag-target ownership。
  `docs/PLACES_RETAINED_EVENT_DELIVERY_PLAN.zh-CN.md`、
  `docs/DRAG_DROP_REFERENCE.zh-CN.md` 和
  `docs/RETAINED_RENDERER_EVIDENCE_CHECKLIST.zh-CN.md` 现在说明 retained Places typed
  payload bridge 在 pointer 位于 pane viewport 内时必须 defer 给 pane ownership。
  必需运行时 trace 是 `places-dnd-defer-to-pane`，且 pane retained hit testing 已拥有 item
  drop target 时，不允许残留 Places 高亮。
- [x] P16fs：将 item-view autosmoke runner ownership 移入 file-grid facade。
  `src/ui/file_grid/autosmoke.rs` 现在拥有 `FIKA_AUTOSMOKE_ITEM_VIEW` 的异步 action loop 和
  marker emission，`src/main.rs` 只读取选中的 scenario 并触发 facade。这让运行时证据收集跟随
  item-view controller/perf surface，而不是把 action orchestration 留在 app root。
- [x] P16ft：将 Places autosmoke runner 和 action application 移入 Places facade。
  `src/ui/places/autosmoke.rs` 现在拥有 `FIKA_AUTOSMOKE_PLACES` 的异步 loop、target/layout
  action dispatch、settings verification marker emission，以及 retained targeting/DnD smoke
  调用。`src/main.rs` 只读取选中的 scenario 并触发 facade，因此 Places 运行时证据收集跟随
  Places projection 和 interaction 模块。
- [x] P16fu：将 Places autosmoke sidebar layout mutation helper 移入 Places facade。
  `src/ui/places/autosmoke.rs` 现在拥有 layout 证据使用的 smoke-only 宽度/可见性更新路径，
  `src/main.rs` 保留常规 sidebar 命令和 settings 持久化调度作为 app coordination。这减少了
  app root 对 Places 证据机制的了解，同时不改变持久化 sidebar 行为。
- [x] P16fv：将常规 Places sidebar 宽度/可见性命令移入 Places sidebar facade。
  `src/ui/places/sidebar.rs` 现在拥有 sidebar layout state 的切换、重置、拖拽调整、
  clamp 和 app-settings 保存移交。`src/main.rs` 仍保存 app-level 持久化字段，并从快捷键/
  render closure 调用 facade，但不再拥有 sidebar mutation policy。
- [x] P16fw：将 Places snapshot projection 编排移入 Places projection facade。
  `src/ui/places/projection.rs` 现在拥有面向 app 的 `place_snapshots()` 方法，包括 active-place
  投影、隐藏 place 过滤、autosmoke 额外行、paint-slot projection 和 Places snapshot perf
  日志。`src/main.rs` 仍在 render 时请求 snapshots，但不再直接串联 Places projection 内部或
  evidence emission。
- [x] P16fx：将面向 app 的 removable-device Places 更新方法移入 Places devices facade。
  `src/ui/places/devices.rs` 现在拥有 `finish_device_refresh()`、`apply_device_snapshot()` 和
  `replace_removable_device_places()` app 方法，`src/main.rs` 保留 monitor scheduling/draining
  loop。低层 device-section replacement helper 不再通过顶层 Places facade 重新导出。
- [x] P16fy：将面向 app 的 hidden Places visibility 命令移入 Places visibility facade。
  `src/ui/places/visibility.rs` 现在拥有 `hide_place()`、`hide_place_section()` 和
  `show_hidden_places()` app 方法及状态栏更新；低层 visibility helpers 保持在 Places
  visibility 模块内部，不再通过顶层 Places facade 重新导出。
- [x] P16fz：将面向 app 的 user-place persistence wrapper 移入 Places user facade。
  `src/ui/places/user.rs` 现在拥有 `FikaApp` 上的 `user_places()` 和 `save_user_places()`，
  包括用户书签导出和 primary place order 持久化。`src/main.rs` 仍从 add/edit/remove/reorder
  路径调用保存 facade，但不再拥有 persistence wiring。
- [x] P16gaa：将面向 app 的 user-place removal 移入 Places user facade。
  `src/ui/places/user.rs` 现在拥有 `FikaApp` 上的 `remove_place()`，包括 removable-place
  校验、draft 清理、hidden-place 清理、持久化和状态栏更新。低层 `remove_user_place()`
  helper 保持在 user-place 模块内部，不再通过 `ui::places` 重新导出。
- [x] P16gab：将面向 app 的 dropped-folder place insertion 移入 Places user facade。
  `src/ui/places/user.rs` 现在拥有 `FikaApp` 上的 `insert_place_from_dropped_paths()`，
  包括校验、用户 place 插入、持久化和状态栏更新。低层 dropped helper 保持在 user-place
  模块内部，不再通过 `ui::places` 重新导出。
- [x] P16gac：将面向 app 的 user-place reorder 和 insert-index helper 移入 Places user
  facade。`src/ui/places/user.rs` 现在拥有 `FikaApp` 上的
  `move_user_place_to_insert_index()` 和 `user_place_insert_index()`，包括状态映射和持久化。
  低层 ordering result enum 和函数保持在 user-place 模块内部。
- [x] P16gad：将面向 app 的 place-draft commit 移入 Places user facade。
  `src/ui/places/user.rs` 现在拥有 `FikaApp` 上的 `commit_place_draft()`，包括 draft 取出、
  当前目录查找、校验结果映射、持久化和状态栏更新。低层 `commit_user_place_draft()` helper
  保持在 user-place edit 模块内部。
- [x] P16gae：将面向 app 的 add/edit place draft startup 移入 Places user facade。
  `src/ui/places/user.rs` 现在拥有 `FikaApp` 上的 `start_add_place()` 和 `start_edit_place()`，
  包括 pane focus、冲突 draft 清理、默认 label 投影、editable-place 查找、draft 创建和状态栏更新。
  `src/main.rs` 暂时保留 network-drive draft startup，因为该路径有单独的 network-auth 语义。
- [x] P16gaf：将面向 app 的 Places drop-target 状态移入 Places drag facade。
  `src/ui/places/drag.rs` 现在拥有 `PlaceDropTarget` 的设置、查询、清理和 pane viewport ownership 清理；
  通用 item/path-list drop-target 状态仍留在 Places 外部。
- [x] P16gag：将面向 app 的 Places drop 执行逻辑移入 Places drag facade。
  `src/ui/places/drag.rs` 现在拥有将 place drag、item drag 和 external paths 放到 Places target
  或插入位置的流程；通用 pane 和文件传输 helper 仍留在 Places 外部。
- [x] P16gah：将面向 app 的 Places activation 和设备操作移入 Places devices facade。
  `src/ui/places/devices.rs` 现在拥有打开已挂载 place、挂载未挂载设备 place，以及完成
  mount/unmount/eject 后台操作的流程。
- [x] P16gai：将面向 app 的 Places context-menu target 投影移入 Places sidebar facade。
  `src/ui/places/sidebar.rs` 现在拥有将 place、section 和 sidebar 空白区域交互转换成
  `ContextMenuTarget` 的流程。
- [x] P16gaj：将面向 app 的 place-draft lifecycle wrapper 移入 place-draft facade。
  `src/ui/place_draft.rs` 现在拥有 `PlaceDraft` 的 pane-scoped 清理、dismiss 和 focus 切换；
  draft 创建和提交仍留在对应的 Places user/network 路径中。
- [x] P16gak：记录 Places full-row visual handoff 的突破和默认提升阻塞。突破点不是
  原始 full custom paint，而是 ready-only handoff 加 `PlacesRowTextShapeCache` 预热：
  warmup 帧继续显示 GPUI text/icons，资源 ready 后才切到 retained full row painting。
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-full-handoff-runner-20260619` 的证据显示 full targets warm row paint 为
  `379us`，overflow 在 75 行/29 painted rows 下 warm row paint 为 `1090us`，layout
  warm row paint 为 `724us`；同一组 analyze-only runner 通过 row-visual gate。默认提升
  仍被阻塞，因为 targets full 运行首帧 `[fika render] total` 仍达到 `27268us`，所以剩余工作是
  首帧 owner accounting 和 total-render 波动，而不是 cold row visual paint 本身。roadmap、
  renderer decisions 和 evidence checklist 现在要求在修改 Places full-row visual 默认值前采集
  `--places-full-handoff` A/B 证据。
- [x] P16gal：添加 Places full handoff 首帧 render owner accounting。之前 full targets
  total 不透明的根因是 `[fika render]` 缺少 owner：analyzer 只有宽泛 render phase 的 max
  字段，所以 2026-06-19 full targets 日志在 max total 帧显示 `17285us` residual。
  实现：`[fika render]` 现在输出 `window_setup`、`chrome_inputs` 和 `overlays`；
  `scripts/analyze-places-perf.sh` 现在输出 `render_at_max_total`，包含同一帧 owner、
  `accounted`、`residual`、`max_accounted` 和 `max_residual`。证据：
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-full-owner-20260619` 通过了所有 full handoff gate。新 owner 行把 residual
  降到 `4-5us`，并确认 `chrome_inputs` 是首帧主要 owner：chrome targets `7846us`、
  full targets `7817us`、chrome overflow `8768us`、full overflow `7832us`、
  chrome layout `7824us`、full layout `8638us`。这把下一步优化目标从 row visual paint
  收窄到 toolbar/chrome icon/input preparation。
- [x] P16gam：将 `chrome_inputs` 拆成 state 和 icon owner。上一轮 owner accounting
  将整帧尖峰收窄到 `chrome_inputs`，但它仍混合了廉价 render state projection 与
  toolbar/chrome 控件的同步命名图标解析。实现：`[fika render]` 现在输出
  `chrome_state` 和 `chrome_icons`；`scripts/analyze-places-perf.sh` 继续把
  `chrome_inputs` 作为归一化总和，因此旧日志仍可解析。证据：
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-chrome-split-20260619` 通过所有 full handoff gate，并显示 `chrome_state`
  只有 `2-7us`，而 `chrome_icons` 主导 max-total 帧：chrome targets `8380us`、
  full targets `8360us`、chrome overflow `14626us`、full overflow `10708us`、
  chrome layout `11679us`、full layout `9101us`。下一步优化目标已经明确为首帧
  named toolbar/chrome icon resolution。
- [x] P16gan：在首帧 render 前预热固定 chrome 图标 snapshot。根因：默认 chrome 和
  full handoff 两条路径都会在第一帧为 toolbar/sidebar 固定控件同步解析 named icon
  snapshot，所以剩余的 full-path 尖峰不是 row visual painting。实现：
  `FikaApp::new()` 现在会在替换设备 place 之前调用 `prewarm_chrome_icon_cache()`，
  将 filter toggle、pane split/close 和 Places panel 图标 snapshot 解析进共享
  file-icon cache。证据：
  `scripts/run-retained-renderer-evidence.sh --places-full-handoff --skip-build --prefix
  fika-places-chrome-prewarm-20260619` 通过所有 full handoff gate。`chrome_icons`
  最大 owner 从 split run 的 targets `8380us`/`8360us`、overflow
  `14626us`/`10708us`、layout `11679us`/`9101us` 降到 chrome targets
  `12us`、full targets `6us`、chrome overflow `10us`、full overflow `9us`、
  chrome layout `7us`、full layout `7us`。这是 full 路径的实质突破，因为它移除了
  共享的首帧 chrome icon 尖峰；默认提升仍要看后续 row-visual/root/pane
  total-render 的重复证据，而不是只看这个 owner。
- [x] P16gao：移除 Places full visual handoff 后的空 GPUI row spacer。根因：
  full handoff 达到 `text_gpui=0` 和 `icon_gpui=0` 后，每个 retained Places row
  shell 仍然携带一个空的 `flex_1` child，只用于撑开 shell 宽度。实现：custom chrome
  row shell 现在用固定行高加 `w_full()` 保持 hitbox 宽度，ready full row 不再构造
  spacer 子树。证据：`/tmp/fika-places-full-overflow-no-spacer.log` 通过 full handoff
  overflow gate，并将前一轮 prewarm overflow 最大值从 `max_total=4760us`、
  `max_pane_elements=1603us`、`max_root=2008us` 降到 `max_total=3813us`、
  `max_pane_elements=1191us`、`max_root=1583us`。这是增量优化，还不是默认提升决策；
  但它更接近 Dolphin 风格 retained row：custom painter 已拥有文本和图标输出后，
  GPUI row shell 不再保留纯视觉占位 child。
- [x] P16gap：跳过 Places custom visual layer 中普通行的冗余背景填充。根因：
  full/custom Places painter 会把每个普通行都填成和 sidebar 背景相同的 `0xf8f9fb`，
  因而 retained painter 会为没有 active/drop 状态的行提交不必要的圆角 quad。实现：
  `paint_place_row_visual()` 现在只为 active 或 drop-target row 绘制背景和边框；
  普通行透出 sidebar 背景，文本、图标、Trash 标记和插入指示器保持不变。证据：
  `/tmp/fika-places-full-overflow-skip-plain-bg.log` 通过 full handoff overflow gate，
  `places_row_visual max_paint=828us`、`max_warm_paint=828us`，低于最近同类
  full-overflow run 的约 `1.1-1.3ms`。这是直接的 Dolphin 风格 retained paint 优化：
  只绘制有状态的 row chrome，而不是为每个 item 重画静态父背景。
- [x] P16gaq：将 Places retained event hitbox 过滤到可见 content mask。根因：
  retained event layer 在 overflow 场景中仍会为所有 Places row 和 section 插入 hitbox，
  但 pointer、click 和 context-menu delivery 只需要当前 viewport 的 hitbox。DnD
  move/drop 仍继续使用完整 interaction geometry。实现：
  `places_event_probe_prepaint()` 现在会先用 `Window::content_mask()` 和 row/section
  y 范围求交，再调用 `Window::insert_hitbox()`；analyzer 将
  `[fika places-event-probe] rows/sections` 视为可见 hitbox 计数，而 renderer-policy
  计数仍表示 retained projection 容量。证据：
  `/tmp/fika-places-full-overflow-visible-hitboxes.log` 通过带
  `--require-event-probe` 的 full handoff overflow gate；overflow event hitboxes
  从 retained projection 容量 `78` 降到可见集合 `32`，event paint 保持
  `max_paint=52us`。这让 Places 进一步离开 per-row GPUI/event 工作，转向
  viewport-owned retained hit testing。
- [x] P16gar：在 sidebar visual 和 event layer 之间共享 Places snapshot。根因：
  `places_sidebar()` 会分别为 visual layer 和 event layer clone 一整份
  `Vec<PlaceSnapshot>`，然后再消费原 vector 构建剩余 row shell。实现：sidebar 现在将
  输入 vector 移入 `Arc<[PlaceSnapshot]>`；visual layer 和 event layer 共享同一份
  snapshot slice，row-shell 构建只为仍然交给 GPUI drag-start 边界的单行 clone。证据：
  `/tmp/fika-places-full-overflow-shared-snapshots.log` 通过带
  `--require-event-probe` 的 full handoff overflow gate，`max_build=1112us`、
  `max_total=3198us`，event probe `max_paint=15us`。这让 Places 更接近 retained
  model/painter 拆分：一份投影 snapshot 同时喂给所有 viewport layer，而不是按 surface
  重复 clone。
- [x] P16gas：将 Places 默认推进到 full retained visual，并用 retained image cache
  替换 row 内 GPUI 图标元素。根因：仅把默认从 chrome 推到 text 仍会把 Places 图标留在
  GPUI `img()` 子元素里，不满足 Dolphin 风格 model/controller/painter 拆分。实现：
  默认 `places_row_visual_policy()` 现在为 `CustomFull`；Places visual layer 拥有
  keyed `PlacesIconImageCache`，内部使用 GPUI `RetainAllImageCache` 加
  `window.paint_image()` 绘制真实主题图标，pending/failed 时保留稳定 fallback，不再用
  fallback marker 冒充已完成 image 路径。证据：
  `/tmp/fika-places-default-full-targets-scale.log` 通过
  `--expect-custom-row-full-policy`，显示 `visual_kinds=full`、`text_gpui=0`、
  `icon_gpui=0`、`max_total=2247us`、warm row paint `395us`；
  `/tmp/fika-places-default-full-overflow.log` 通过 overflow full gate，75 行策略下
  viewport event hitbox 裁到 `32`，`max_total=2162us`、warm row paint `655us`。冷帧
  row paint 仍有首次 image atlas/text paint 成本（targets `5179us`、overflow
  `8263us`），下一步要继续预热或平摊冷帧，但默认 Places 行文本和图标已经不再依赖
  GPUI 子元素。
- [x] P16gat：把 Places full handoff 经验应用到 pane MIME/theme icon。根因：
  直接把 `FIKA_CUSTOM_THEME_ICONS=1` full-custom 压力路径作为冷启动默认仍会暴露
  首次加载 custom image placeholder 和 decode completion churn
  （`/tmp/fika-pane-full-custom-etc.log`：`theme_placeholder=52`、visible
  `theme_decoded=5`）。当时实现：默认 hybrid renderer 使用可见集合级 handoff。
  当前可见集合中任意 theme-icon key 未 ready 时，所有可见 theme icons 都继续使用
  GPUI `img()`，item image layer 只预热 retained images；当这组 key 全部 ready 后，
  所有可见 theme icons 同一批切到 retained custom image layer。该阶段后来被
  semantic cache、source-image reuse、app-level prewarm 和 cache budget 后的 full custom
  默认取代。证据：
  `/tmp/fika-pane-cohort-default-downloads.log` 相对
  `/tmp/fika-pane-cohort-gpui-downloads.log` 通过
  `--gate-hybrid-default-promotion`，且 `theme_placeholder=0`、visible
  `theme_decoded=0`。`/tmp/fika-pane-cohort-default-etc-r2.log` 也保持这些 image
  稳定指标干净，但完整 promotion gate 仍因 `/etc` 的 icon-sync/content-change
  波动失败，因此下一步 pane image 目标是降低 `/etc` `icon_sync` 成本，而不是继续调整
  placeholder 行为。
- [x] P16gau：移除 pane `icon_sync` 中同 kind 图标 cache 扫描，并扩大后台 icon
  resolve batch。根因：可见集合级 handoff 后，`/etc` 仍出现 7-13ms `icon_sync` 帧，
  即使大多数 candidates 被统计为 cached；原因是
  `FileIconCache::cached_icon_for_kind()` 为了找到可复用的 resolved theme path，会对每个
  可见 candidate 扫描 exact-size cache。实现：`FileIconCache` 现在为 pathful
  `FileIconKind` 结果维护 `resolved_by_kind` 索引，同时保留 exact-size 和 negative
  exact cache entries；file icon 后台 resolve batch 提高到 128 个请求，让 bounded
  read-ahead 更可能在 resize/scroll 让这些 item 进入可见区域前完成。证据：
  `/tmp/fika-icon-batch128-default-etc.log` 相对
  `/tmp/fika-icon-batch128-gpui-etc.log` 通过
  `--gate-hybrid-default-promotion`，candidate `icon_sync=103us`、
  `theme_placeholder=0`、visible `theme_decoded=0`；
  `/tmp/fika-icon-batch128-default-downloads-r2.log` 相对
  `/tmp/fika-icon-batch128-gpui-downloads-r2.log` 通过同一 gate。
- [x] P16gav：将 Places section heading label 移入默认 full visual layer。根因：
  Places row 和 row icon 默认 full custom 后，group heading 仍使用 GPUI text child，
  因此默认 full policy 还不能真实报告完整的 Places 文本视觉所有权。实现：
  `places_row_visual_layer` 现在从与 row 相同的 snapshot 投影 section heading，通过
  `PlacesRowTextShapeCache` prepaint 可见 heading label，并在 sidebar visual canvas
  中绘制；`group_heading` 保留 section targeting/DnD shell，但当 custom visual text
  启用时不再挂载 label child。证据：
  `/tmp/fika-places-section-full-targets.log` 和
  `/tmp/fika-places-section-full-overflow.log` 已以 `section_gpui=0` 通过
  `--expect-custom-row-full-policy`；targets warm row paint 为 `247us`，overflow
  将 visible event hitboxes 裁到 `32`，warm row paint 为 `785us`。
- [x] P16gaw：移除 pane per-directory GPUI drag-move shell。根因：目录 item/row
  drop hover 仍通过透明 GPUI shell 正向断言，但 retained window-position hit testing
  已经能为 item、external-path 和 Place 拖拽解析 pane/目录目标。实现：Compact/Icons
  item shell 和 Details row shell 不再安装 `install_directory_drop_target_shell`；
  `file_grid/dnd.rs` 中的 helper 和 `directory-shell-hit` handler 已移除。
  Renderer-policy 日志现在报告 `retained_directory_drop_target` 和
  `gpui_directory_drop_shell`，且
  `scripts/analyze-item-view-perf.sh --expect-retained-item-policy` 会拒绝任何非零
  GPUI directory drop shell 计数。剩余 GPUI item/row shell 仅是 typed drag-start
  边界和 rename overlay。证据：`/tmp/fika-item-retained-directory-drop.log` 通过
  item-view autosmoke、renderer-policy 和 interaction gate，且报告
  `max_retained_directory_drop_target=60`、`max_gpui_directory_drop_shell=0`。
- [x] P16gax：将 Details header 移入 custom Details visual layer。根因：
  Details row 已经 custom paint，但 header 背景、列分隔线和标题仍是 GPUI `Div`/text
  child，Details 模式里还残留静态 GPUI 视觉 surface。实现：
  `details_visual_layer_view()` 现在携带 header projection，通过
  `DetailsTextShapeCache` prepaint header label，并在与 Details row 相同的 canvas 中绘制
  header；`details_shell.rs` 不再构建 GPUI `details_header()` 子树。
  Renderer-policy 日志现在暴露 `details_header_visual_layer` 和 `gpui_details_header`，
  `--expect-retained-item-policy` 会拒绝 GPUI Details header。剩余后续：补专门的
  Details-mode runtime autosmoke，让这个 surface 拥有与 Compact zoom/scroll 相同强度的
  运行时证据。
- [x] P16gay：添加专门的 Details-mode item-view runtime autosmoke gate。根因：
  retained item-view smoke 只覆盖默认 Compact 路径，因此 Details header 迁入 visual
  layer 后，Details custom paint 回归仍可能通过标准运行时证据。实现：
  `FIKA_AUTOSMOKE_ITEM_VIEW=details-zoom-scroll` 现在会先把 active pane 切到 Details，
  再运行 zoom/scroll action；item-view analyzer 识别 `DetailsZoomScroll` 并要求
  `view-details` marker；retained renderer evidence 脚本会用 `--require-details`、
  `--require-modes Details`、`--require-renderer-policy-modes Details` 和
  `--expect-retained-item-policy` 跑 Details gate。这把 Details 视觉所有权变成后续 pane
  painter 工作可重复的运行时证据。
- [x] P16gaz：在 zoom handoff 中复用已 ready 的 MIME/theme icon resource path。根因：
  pane image handoff 只按 exact-size key 判断 ready。Zoom 时，同一个已加载
  `Resource::Path` 如果对应新的 size/scale key，仍可能短暂退回 GPUI，或在 custom
  visible paint 中被统计为新的 first-ready decode，从而产生 Dolphin 对照中要避免的第二次
  icon identity 调整。实现：`ThemeIconImageReadiness` 现在同时记录 ready semantic key
  和 ready resource path；visible-cohort handoff 接受 exact key ready 或 resource path
  ready 的可见图标。Retained theme icon cache 也按 path 建立 loaded image 索引，同一路径的
  新 size key 会被视为 retained reuse，而不是 first-ready decode。证据：
  `/tmp/fika-path-ready-hybrid-downloads.log` 相对
  `/tmp/fika-path-ready-gpui-downloads.log` 通过
  `--gate-hybrid-default-promotion`，且 `theme_placeholder=0`、visible
  `theme_decoded=0`。`/tmp/fika-path-ready-hybrid-etc-r2.log` 通过 handoff 部分并移除
  visible decode churn（`theme_decoded=0`），但完整 default promotion 仍因 `/etc`
  icon-sync/content-change 方差失败；该失败点不在 image handoff 路径。
- [x] P16gba：用 Dolphin 风格 key-size full path 取代 pane path-ready 方案。根因：
  Dolphin 的 `KStandardItemListWidget::pixmapForIcon()` 按 `iconName + iconHeight +
  devicePixelRatio + mode` 查 `QPixmapCache`；path 只是 icon theme resolver 的资源入口，
  不应该成为 MIME/theme icon ready/cache 主 key。实现：pane MIME/theme icons 默认 full
  custom image layer；`ThemeIconImageReadiness` 只记录 `ThemeIconImageKey`；
  `RetainedThemeIconImageCache` 不再通过 `images_by_path` 跨 size 复用旧 image；
  `FileIconCache` 不再跨 size 复用 pathful kind snapshot，并新增 `MIME + size` 复用；
  SVG theme icons 冷 key 通过 GPUI `svg_renderer` 同步生成 `RenderImage`，随后仍走
  `Window::paint_image`/sprite atlas。证据：`/tmp/fika-full-syncsvg-custom-etc.log`
  相对 `/tmp/fika-full-syncsvg-gpui-etc.log` 报告 `max_image_layer=64`、
  `max_gpui_image_element=0`、`theme_placeholder=0`、`theme_retained=497`，且
  content-change 与 `icon_sync` 低于 GPUI baseline；Downloads full log 报告
  `max_image_layer=32`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_retained=543`，initial total 低于 GPUI baseline。
- [x] P16gbb：将 pane theme `RenderImage` cache 提升到 app/global owner，并在 snapshot
  构建期间预热可见 `ThemeIconImageKey`。根因：Dolphin 风格 key-size full path 之后，
  retained `RenderImage` cache 仍属于 image-layer element，因此冷 SVG decode 虽然不再造成
  placeholder，但仍发生在 element prepaint。实现：`FikaApp` 现在拥有 theme
  `RenderImage` cache；`PaneSnapshot` 构建时从 `FileGridRenderSnapshot` 收集可见
  custom-theme key，按 `ThemeIconImageKey` 去重，通过 GPUI `svg_renderer` 同步生成 SVG
  `RenderImage`，标记语义 key ready，并把刷新后的 readiness snapshot 交给 pane
  rendering。单独的 prewarm element 已移除，因此 file-grid surface 只消费 readiness 并绘制
  retained image。证据：`/tmp/fika-early-prewarm-custom-etc.log` 报告
  `max_image_layer=64`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_decoded=0`、`theme_prewarm_decoded=0`、`theme_retained=454`、
  `item-image max_prepaint=166us`；`/tmp/fika-early-prewarm-custom-downloads.log`
  报告 `max_image_layer=32`、`max_gpui_image_element=0`、`theme_placeholder=0`、
  `theme_decoded=0`、`theme_prewarm_decoded=0`、`theme_retained=187`、
  `item-image max_prepaint=315us`。
- [x] P16gbc：降低 `/etc` 冷启动/content-change 的 `icon_sync` 方差。根因：剩余
  `/etc` spike 是两个冷的可见语义 icon resolve，不是 image paint。`.pwd.lock`
  (`application/octet-stream`) 在 theme lookup 中约 28ms，`.updated`
  (`text/plain`) 约 2ms；read-ahead preliminary key 在滚动后变成真实 MIME key，旧 work
  覆盖不到。实现：启动时独立后台预热常见语义 file-icon key，并优先处理默认 48px size 与
  邻近 zoom level，然后补全剩余 size。预热表包含 directory，以及常见 text、binary、
  archive、office、image、video、audio 和 PDF MIME key。预热通过
  `finish_resolve_results` 写入 `FileIconCache`，但不占用 `FileIconResolveQueue` cover
  key；因为 queue-covered visible items 会让首个 `/etc` 内容帧临时失去 image layer。
  证据：`/tmp/fika-common-icon-prewarm-detached-etc.log` 和
  `/tmp/fika-common-icon-prewarm-expanded-etc.log` 不再出现 scroll-time
  `application/octet-stream` 或 `text/plain` sync resolve；扩展表后
  `icon_sync max_total=241us`，`max_resolved=1` 只剩初始 directory key，
  `max_image_layer=64`、`max_gpui_image_element=0`、`theme_placeholder=0`。Downloads
  仍显示首个可见 `application/java-archive` 的竞态，因此混合目录 initial MIME prewarm
  仍是后续工作，不作为本次已修复项。
- [x] P16gbe：移除混合目录首个可见 MIME icon 竞态。根因：detached common prewarm 可能输给
  第一个 visible `icon_sync`，而 MIME theme lookup miss 没有写入 `MIME + size` 语义索引；
  因此预热过的 `application/java-archive` miss 不能保护可见 `.jar` 条目。实现：app 初始化时、
  第一个 pane load 前，同步解析默认 48px 常见语义 MIME 表；剩余 zoom size 继续由 detached
  prewarm 补齐；并把 pathless MIME 结果写入 `FileIconCache::resolved_by_mime`。复用 MIME entry
  时会按当前 file kind 重新计算 fallback marker/颜色，所以 `.jar` 仍显示 `JAR`，且不会重复
  theme lookup。证据：`/tmp/fika-common-icon-sync48-downloads.log` 报告 `max_resolved=0`，
  没有 `[fika icon-sync-resolve]` 行，`icon_sync max_total=235us`、
  `max_gpui_image_element=0`、`theme_placeholder=0`；
  `/tmp/fika-common-icon-sync48-etc.log` 报告 `max_resolved=0`、
  `icon_sync max_total=33us`。
- [x] P16gbd：将 pane SVG theme-image retention 对齐 GPUI `img()` internals。根因：
  Fika full custom 路径已经使用 `Window::paint_image`，但 retained theme image cache 只按
  `ThemeIconImageKey` 索引，所以 zoom 产生新 size key 时，同一个 scalable SVG source 仍可能
  重复 materialize。GPUI `img(Resource::Path(svg))` 是为 resource 创建一个
  `Arc<RenderImage>`，再按 paint bounds 缩放。实现：`RetainedThemeIconImageCache` 现在额外维护
  `source path -> RenderImage` 索引。Readiness 仍按 `ThemeIconImageKey` 保持 size/scale-aware，
  但新的 semantic key 可以直接从 retained source image 记录，不再重新读取/渲染 SVG。source
  复用在 `[fika item-image]` 中记为 retained，而不是 decoded。证据：
  `/tmp/fika-svg-source-retain-etc.log` 报告 `theme_decoded=0`、`theme_retained=982`、
  `theme_placeholder=0`、`max_gpui_image_element=0`、`item-image max_prepaint=480us`；
  `/tmp/fika-svg-source-retain-downloads.log` 报告 `theme_decoded=0`、`theme_retained=702`、
  `theme_placeholder=0`、`max_gpui_image_element=0`、`item-image max_prepaint=788us`。
- [x] P16gbd1：为 pane retained theme icon image cache 加 Dolphin/Qt QPixmapCache 式预算。根因：
  Dolphin `pixmapForIcon()` 使用 `name + size + dpr + mode` 的 `QPixmapCache` key，并依赖
  全局 pixmap cache 预算淘汰；GPUI `img()` 也有 element/global image cache 生命周期。
  Fika full custom path 直接持有 `Arc<RenderImage>`，之前 `RetainedThemeIconImageCache`
  和 source-path map 没有上限，长期访问大量目录会只增不裁剪。实现：cache hit 会刷新
  generation；`prune_to_budget()` 按 retained `RenderImage` frame bytes 做 10MB 预算淘汰，
  而不是按 entry 数量粗略限制；最后一个引用某 source path 的 key 被裁剪时释放
  `source path -> RenderImage`。有 `Window` 的 paint path 还会同步
  `RetainAllImageCache::remove(Resource::Path)` 和 `cx.drop_image(image, Some(window))`，
  避免 GPUI resource cache 或 atlas 继续持有。验收：由用户侧用 debug `/etc` USS/私有占用
  测量确认；不要用 RSS、release build 或当前 GPUI fallback 替代该证据。
- [x] P16gbd2：将同样的 QPixmapCache 预算释放策略应用到 Places full row icon path。根因：
  Places 拥有独立 `PlacesIconImageCache`，之前同样会通过 `RetainAllImageCache` 和
  retained source map 长期持有 sidebar icon image。实现：Places 先按
  `ThemeIconImageKey` find，再按 source path 复用，最后才 load；每次 load/insert 后按 10MB
  frame-byte budget 淘汰 LRU semantic key；最后一个 source 引用释放时同步
  `RetainAllImageCache::remove(Resource::Path)` 与 `cx.drop_image`。
- [~] P16gbf：在 image/icon ownership 稳定后，降低剩余 pane custom visual/text paint 冷帧方差。
  当前 `/etc` 和 Downloads 日志中 image 与 icon-sync 已在预算内，但
  `[fika static-item-visual]` 仍可能出现多毫秒级 cold prepaint/paint。继续对照 Dolphin item
  text/pixmap caches 和 GPUI text shaping，判断下一步是 retained text-shape/source prewarm、
  收紧 paint invalidation，还是引入更接近 Dolphin 的可见 widget/state pool。
- [x] P16gbf1：收紧 pane static text/visual 在 Icons zoom 中的复用。根因：
  static item text shape cache 以前按 `item_id` 以及 Icons 居中标签的 paint-only text bounds
  建 key，所以 zoom/resize 即使实际 shaped label lines 不变也可能 miss。实现：
  `StaticItemTextShapeCacheKey` 移除 item identity；Center/Icons 标签在已计算分行后不再把
  text rect 宽高作为 key 维度；没有 fallback marker 时不再把 marker line height 放进 key；
  普通未选中/未悬停条目不再提交透明 background quad；新增
  `FIKA_AUTOSMOKE_ITEM_VIEW=icons-zoom-scroll`。证据：
  `/tmp/fika-full-icons-keyed-etc.log` 覆盖 `modes: Icons,Compact`，
  `max_gpui_image_element=0`、`theme_placeholder=0`、`theme_decoded=0`；初次切入
  Icons 后，zoom 帧出现 `hits=24 misses=0`、`hits=28 misses=0`、`hits=40 misses=0`，
  重复 zoom 的 `[fika static-item-visual]` prepaint 降到 93-254us。
- [~] P16gbf2：移除 Icons/Compact full custom visual paint 剩余的首次进入冷文本/glyph
  尖峰。当前证据显示下一个根因：Downloads 仍有稳定的首次 Icons 切换 cold shape 尖峰
  （`/tmp/fika-full-icons-keyed-downloads-r2.log`，`hits=1 misses=39`，
  `static-item-visual prepaint=52840us`）和第一次 text paint 尖峰（`paint=17698us`），
  尽管 image/icon 路径已经干净。下一切片应加入 Dolphin 风格 retained text warmup/state
  pool，让目标模式 label shape 和 glyph paint 在 handoff 前被预热，类似 Places row text
  handoff 模型，而不是把全部 cold shaping 放进第一个可见 custom visual frame。
- [x] P16gbf2a：加入第一版 pane alternate-mode static text warmup。pane render frame
  现在用本地临时 slot/cache 状态投影目标 Compact/Icons 模式快照，将其传给 file-grid surface，
  并在可见层之前挂载 warm-only static visual layer。warm layer 只向 pane-local
  `StaticItemTextShapeCache` 写入 shaped text，不绘制，也不记录为可见 static-visual timing。
  它使用独立 `ElementId`，避免与可见 static visual layer 的 GPUI retained identity 碰撞。
  ID 修复后的证据：`/tmp/fika-compare-pane-full-etc-r3.log` 保持
  `max_gpui_image_element=0`、`theme_placeholder=0`、visible `theme_decoded=0`，
  并把 `/etc` 可见 static prepaint 控制到 `2996us`；配对
  `FIKA_GPUI_THEME_ICONS=1` baseline 为 `2938us`。Downloads 仍未完成：
  `/tmp/fika-compare-pane-full-downloads-r3.log` 仍显示
  `static_visual max_prepaint=16866us max_paint=17580us`，接近 GPUI image baseline
  的文本成本。下一步应处理 glyph/text paint retention 或 ready-only handoff，而不是继续改
  image renderer policy。
- [ ] P16q：在每个 P16 实现切片之后，单独提交并附带相关验证：仅文档切片需要 `git diff --check`；代码切片需要 `cargo fmt`、`cargo check`、`cargo test -q`、`scripts/check-item-view-perf-analyzer.sh`、`scripts/check-places-perf-analyzer.sh` 和 `git diff --check`。
- [x] P16r：记录运行时自测试和突破记录规则。可重复的滚动、缩放、启动图标、调整大小、模式切换和 Places 目标回退应在依赖手动计时之前通过 autosmoke 日志和分析器脚本重现。任何确认的优化突破必须记录症状、Dolphin 比较边界、根本原因、实现、保存的日志/分析器命令和未来回归守卫在拥有的设计或决策文档中。

## 验收门

- [~] 重命名、选择、右键菜单、条目 DnD、places DnD 和外部放置路径无行为回退：单元覆盖现在包括一个跨 Compact、Icons 和 Details 的保留行为矩阵，用于应用侧 hit testing、选择、条目菜单、重命名 draft 路由、条目拖拽源状态、外部路径归一化/放置目标菜单，以及条目/place 放置目标移交。在每次 shell 移除或绘制器扩展切片后，保持此部分直到完整的 `cargo test` 和运行时 DnD smoke 都被刷新。
- [x] `cargo test` 保持绿色。
- [~] 性能日志显示调整大小稳定路径对条目快照转换保持亚毫秒级，没有新的大型 `file-grid build` 回退，Compact/Icons 自定义视觉成本通过 `[fika static-item-visual]` 可见，存在图像支持的图标/缩略图时图像绘制成本通过 `[fika item-image]` 可见，条目图像源计数显示帧是否使用了解码主题图标、保留同 `iconName` 图像、首帧加载占位符或缩略图后备，聚合自定义绘制成本被汇总，详情自定义视觉/文本形状成本通过 `[fika details-visual]` 和 `[fika details-shape-cache]` 分开可见。滚动/缩放证据还应显示，在第一帧切换到初步图标后，冷主题图标工作不再出现为同步渲染转换尖峰。当前 `/etc` autosmoke 满足 Compact/Icons 缩放-滚动图标同步部分，`details-zoom-scroll` 已覆盖 Details visual/header 运行时证据；完整 DnD runtime smoke 仍需要桌面会话刷新。
- [x] 冷模式切换成本与调整大小成本分开跟踪：`[fika item-view]` 现在包括 `phase=initial|mode-switch|content-change|geometry-change|visual-change|steady`，具有单元覆盖证明模式切换不被分类为调整大小/几何更改。
- [ ] 任何自定义绘制扩展保持 Dolphin 的 model/controller/painter 划分，并且仅当在该表面上性能中性或优于 GPUI 内置路径时才保留。
- [ ] 如果自定义绘制表面在性能或行为完整性上输给 GPUI 内置元素，保持 Dolphin 对齐的保留 model，但将该表面保留在 GPUI 渲染器上，直到迁移可以被收窄或被证明合理。
- [x] 自定义绘制路径由非重命名 Compact 和 Icons 基础/图像视觉使用。
- [x] 非重命名 Compact/Icons 条目在 P9a 之后不再需要每条目 GPUI 视觉子元素；临时拖拽 shell 保持直到 P9b。
