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
- [~] P15e：在实现之前对保留/自定义行绘制器进行基准测试，与当前 GPUI 侧栏比较。仅当滚动、重排、挂载/回收站/设备行、右键菜单和放置行为中性或更好时才接受 Places 迁移。当前状态：GPUI 侧栏基线和渲染器策略日志存在，且 `FIKA_AUTOSMOKE_PLACES=targets` 覆盖非持久目标/插入投影。`PlacePaintSlotCache` 现在记录保留行/section slot 和 `[fika places-slots]` 统计；没有保留/自定义行绘制器是默认值。`FIKA_CUSTOM_PLACES_ROWS=1` 现在为背景、活动/放置状态、标签、回收站标记和插入指示器提供可选的行视觉绘制器，同时保持 GPUI 图标、行事件传递、右键菜单、DnD 和拖拽启动 shell。`places/interaction.rs` 现在拥有行/section 目标决策，而 GPUI shell 仍提供事件传递和边界。可选行视觉现已聚合到一个侧栏级层中，因此 `[fika places-row-visual] rows` 必须匹配策略行计数，而不是每行记录一个 canvas。
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

- [x] P16a：在规划、设计和 TODO 文档中记录完整转换轨道：证据、绘制器、controller、shell 边界、Places 和所有权。
- [x] P16b：在最新的 Dolphin 对齐主题图标绘制边界更改后收集一组新的桌面会话证据：`/etc` 自定义主题 vs 默认日志现在证明默认 MIME/主题图标避免了首帧加载 `theme_placeholder` 变动，且 `FIKA_PERF_ITEM_VIEW=1 FIKA_AUTOSMOKE_ITEM_VIEW=zoom-scroll target/debug/fika /etc` 捕获无人值守缩放/滚动证据。
- [x] P16c：使用该证据更新 `docs/ITEM_VIEW_RENDERER_DECISIONS.md`，包括 `/etc` 缩放/滚动是否仍然显示冷图像加载卡顿或可见占位符到图标切换。当前证据：可见同步停止复制排队的预读图标工作后，`icon_sync` 最大值从 `28340us` 降至 `173us`；剩余的 `/etc` autosmoke 成本是静态视觉文本/基础绘制，而非 MIME/主题图像渲染。
- [x] P16d：如果当前日志无法区分以下情况，则添加或扩展运行时证据工具：首帧加载主题图标占位符、保留同 `iconName` 复用、GPUI 图像缓存解码完成和稳定重绘成本。`[fika item-image]` 现在报告 `theme_loaded`、`theme_decoded`、`theme_retained`、`theme_placeholder`、`thumb_loaded`、`thumb_decoded`、`thumb_retained` 和 `thumb_fallback`；运行时分析器将其总结为 `image_sources`。`FIKA_AUTOSMOKE_ITEM_VIEW` 现在无需手动输入即可练习缩放/滚动，并添加 `[fika autosmoke]` 标记到同一性能日志中。
- [x] P16e：审计本地 GPUI 源码中保留/自定义元素拖拽启动路径。如果没有公共 API 存在，记录确切阻塞并保留条目和详情拖拽启动 shell。结果：Zed 提交 `f16a469` 处的 GPUI `0.2.2` 通过 `Interactivity::on_drag` / `InteractiveElement::on_drag` 在 `crates/gpui/src/elements/div.rs` 中暴露类型化拖拽启动。自定义元素可以通过 `Window::insert_hitbox()` 插入 hitbox，但没有公共 API 从这些保留 hitbox 启动类型化拖拽，因此条目和详情拖拽启动 shell 保留为显式平台边界。
- [ ] P16f：如果选择经过审计的 GPUI patch，设计最小的从保留 hitbox 启动拖拽的 API，同时保留 payload、预览、光标偏移、接受的传输模式和外部放置行为。
- [x] P16g：将下一个行为保留的条目视图编排边界移出 `src/main.rs`。候选：运行时条目视图性能/证据收集访问器，因为绘制器性能状态已经存在于 `file_grid/perf.rs` 下。已完成：`FIKA_PERF_ITEM_VIEW` 标志和文件网格性能层调用者由 `src/ui/file_grid/perf.rs` 拥有；条目视图性能帧分类和性能状态清理由 `src/ui/file_grid/perf.rs` 拥有；帧状态和绘制器性能统计存储现在位于 `src/ui/file_grid/perf.rs` 中的 `ItemViewPerfState` 后面；条目视图性能摘要发出现在由 `src/ui/file_grid/perf.rs` 拥有；autosmoke 场景解析和操作排序现在位于 `src/ui/file_grid/autosmoke.rs` 中。
- [x] P16h：在更改 Places 渲染之前起草保留 Places 行绘制器设计。设计必须覆盖行组、隐藏 section、设备行、重排/放置插入、右键菜单和侧栏滚动。结果：`docs/PLACES_RENDERER_PLAN.md` 将 Dolphin 的 `DolphinPlacesModel + KFilePlacesView` 划分与 Fika 当前的 `places/model`、`projection`、`sidebar/row`、`drag` 和自定义滚动条模块进行比较，然后将任何保留行绘制器门控于 Places 特定性能日志、运行时 smoke 和渲染器策略证据之后。
- [x] P16i：在更改 GPUI 重命名叠加层之前起草重命名自定义编辑器行为矩阵。它必须覆盖焦点、caret hit testing、UTF-8 选择、验证帮助文本、提交/取消、Tab 重命名下一个和 IME。结果：`docs/RENAME_EDITOR_PLAN.md` 将 Dolphin 的 `DolphinView::renameSelectedItems()`、`KItemListView::editRole()` 和 `KItemListRoleEditor` 路径与 Fika 的 `RenameDraft`、快捷键路由和 GPUI 叠加层进行比较。该矩阵将叠加层保留为默认值，直到 IME、焦点/失焦、鼠标选择、可访问性和运行时 smoke 被覆盖。
- [x] P16j：在下一次 MIME/主题图标闪烁修复之前建立历史图像渲染器基线。使用 `a3f5b0f` 作为预保留/自定义绘制 GPUI `img()` 基线，并使用 `d497593`、`8d1198f`、`36da130` 和 `b0cac9a` 作为转换检查点，以决定回归属于 model/投影、保留 slot 状态、自定义元素绘制还是自定义图像层。在更改当前图像渲染器之前将其与 Dolphin `KStandardItemListWidget::updatePixmapCache()` / `pixmapForIcon()` 进行比较。当前代码的 A/B 支持通过 `FIKA_CUSTOM_THEME_ICONS=1` 可用，它保留条目状态但强制 MIME/主题图标通过自定义条目图像层，以便与默认 GPUI 主题图标渲染器进行桌面会话比较。`scripts/compare-item-image-renderers.sh` 现在标准化了配对日志比较，2026-06-17 的 `/etc` smoke 证据记录在 `docs/ITEM_VIEW_RENDERER_DECISIONS.md` 中。
- [x] P16k：从证据中决定 Compact/Icons 主题图标渲染器：默认现在使用 GPUI `img()` 元素处理 MIME/主题图标，并将缩略图保留在自定义图像层上。保持此划分，除非配对的默认 vs `FIKA_CUSTOM_THEME_ICONS=1` 缩放/滚动日志证明自定义主题图标绘制器在没有首帧加载占位符、缩放时 `theme_decoded` 变动或尺寸跳跃的情况下是中性或更好的。
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
- [ ] P16q：在每个 P16 实现切片之后，单独提交并附带相关验证：仅文档切片需要 `git diff --check`；代码切片需要 `cargo fmt`、`cargo check`、`cargo test -q`、`scripts/check-item-view-perf-analyzer.sh`、`scripts/check-places-perf-analyzer.sh` 和 `git diff --check`。
- [x] P16r：记录运行时自测试和突破记录规则。可重复的滚动、缩放、启动图标、调整大小、模式切换和 Places 目标回退应在依赖手动计时之前通过 autosmoke 日志和分析器脚本重现。任何确认的优化突破必须记录症状、Dolphin 比较边界、根本原因、实现、保存的日志/分析器命令和未来回归守卫在拥有的设计或决策文档中。

## 验收门

- [~] 重命名、选择、右键菜单、条目 DnD、places DnD 和外部放置路径无行为回退：单元覆盖现在包括一个跨 Compact、Icons 和 Details 的保留行为矩阵，用于应用侧 hit testing、选择、条目菜单、重命名 draft 路由、条目拖拽源状态、外部路径归一化/放置目标菜单，以及条目/place 放置目标移交。在每次 shell 移除或绘制器扩展切片后，保持此部分直到完整的 `cargo test` 和运行时 DnD smoke 都被刷新。
- [x] `cargo test` 保持绿色。
- [~] 性能日志显示调整大小稳定路径对条目快照转换保持亚毫秒级，没有新的大型 `file-grid build` 回退，Compact/Icons 自定义视觉成本通过 `[fika static-item-visual]` 可见，存在图像支持的图标/缩略图时图像绘制成本通过 `[fika item-image]` 可见，条目图像源计数显示帧是否使用了解码主题图标、保留同 `iconName` 图像、首帧加载占位符或缩略图后备，聚合自定义绘制成本被汇总，详情自定义视觉/文本形状成本通过 `[fika details-visual]` 和 `[fika details-shape-cache]` 分开可见。滚动/缩放证据还应显示，在第一帧切换到初步图标后，冷主题图标工作不再出现为同步渲染转换尖峰。当前 `/etc` autosmoke 满足 Compact/Icons 缩放-滚动图标同步部分；详情和完整 DnD 运行时 smoke 仍需要桌面会话刷新。
- [x] 冷模式切换成本与调整大小成本分开跟踪：`[fika item-view]` 现在包括 `phase=initial|mode-switch|content-change|geometry-change|visual-change|steady`，具有单元覆盖证明模式切换不被分类为调整大小/几何更改。
- [ ] 任何自定义绘制扩展保持 Dolphin 的 model/controller/painter 划分，并且仅当在该表面上性能中性或优于 GPUI 内置路径时才保留。
- [ ] 如果自定义绘制表面在性能或行为完整性上输给 GPUI 内置元素，保持 Dolphin 对齐的保留 model，但将该表面保留在 GPUI 渲染器上，直到迁移可以被收窄或被证明合理。
- [x] 自定义绘制路径由非重命名 Compact 和 Icons 基础/图像视觉使用。
- [x] 非重命名 Compact/Icons 条目在 P9a 之后不再需要每条目 GPUI 视觉子元素；临时拖拽 shell 保持直到 P9b。
