# 架构 Refactor Roadmap

本文档记录 Fika shell 后续架构拆分路线。目标是为性能优化、动画扩展、独立窗口
dialog、render damage 和异步操作持续演进提供稳定边界。

## 原则

- 每一步只移动一个明确边界，避免把行为改动混进结构调整。
- 保持测试先行的回归 gate：`cargo fmt`、`cargo check`、`cargo test`、
  `git diff --check`。
- 性能相关变更遵循 `docs/PERFORMANCE_ALIGNMENT.zh-CN.md`，需要 Dolphin
  reference 时必须写明本地源码路径和 Fika 映射。
- 新抽象必须减少重复、收缩 `main.rs` 职责，或为动画/render/operation 后续扩展
  提供明确挂载点。
- 不保留长期兼容式双路径；迁移完成后应清理旧 overlay、旧 dirty key、旧事件分支。

## 已完成边界

- 独立 dialog window host：
  `src/shell/dialog_window.rs` 管理 dialog window 创建、同步、关闭、cursor、resize、
  renderer size、scale factor 和 window id 路由。
- Create / Rename / Open With 独立窗口化：
  旧的主窗口 overlay fallback 已移除，dialog 内容变化不再污染主窗口 render dirty /
  damage。
- 动画 runtime 初始边界：
  item reflow 动画从 `main.rs` 移入 `src/shell/animation.rs`，主循环只依赖
  active、deadline、dirty、prune 接口。
- Dialog window 通用事件：
  common close / resize / scale / modifiers 路径已从具体 dialog handler 中抽出。
- Render dirty / damage 三层拆分：
  `src/shell/render/dirty_key.rs` 负责 dirty key，`damage_snapshot.rs` 负责采样
  render 可见状态，`damage_bounds.rs` 负责比较 snapshot 并生成 damage bounds；
  `damage.rs` 仅保留 folder preview 异步结果到 damage rect 的映射。
- Command / Action 分类层初始边界：
  `src/shell/action.rs` 负责 context menu command plan、context menu action dispatch
  和 file keyboard command 的纯分类；`FikaWgpuApp` 暂时保留副作用执行，后续可以
  继续拆执行器。
- Command / Action 执行器第一步：
  `src/app_actions.rs` 承载 context menu 和 file keyboard command 的副作用执行；
  `main.rs` 的 window event handler 保持调用入口，但不再内联这两段长业务分支。
- Command / Action 执行器第二步：
  clipboard export、device action、trash view action、move-to-trash、paste/drop wrapper
  已从 `main.rs` 迁入 `src/app_actions.rs`。
- Command / Action 执行器第三步：
  open file、service menu、Ark extract-and-trash、context open-with、Open With dialog launch
  的副作用执行已迁入 `src/app_actions.rs`；`main.rs` 继续保留 dialog commit 前的校验、
  默认应用更新、async completion 等尚未 request 化的应用层流程。
- Command / Action 执行器模块化：
  `src/app_actions.rs` 收缩为 dispatcher，具体副作用分散到
  `src/app_actions/clipboard.rs`、`device.rs`、`dialog_commit.rs`、`launch.rs`、
  `trash.rs`、`transfer.rs`，避免新的单文件执行器继续膨胀。
- Dialog commit 执行迁移：
  create / rename / Open With dialog commit 的磁盘操作、默认应用更新、状态记录和
  reload/apply 流程已迁入 `src/app_actions/dialog_commit.rs`；`main.rs` 只保留事件入口
  和 dialog window 生命周期 helper。
- Navigation / Places 执行迁移：
  path navigation、reload、location commit、add network folder、add/remove place wrapper
  已迁入 `src/app_actions/navigation.rs` 和 `src/app_actions/places.rs`；`main.rs` 中保留
  `ShellScene` 的路径/places 状态变更 API 和事件路由。
- View / Split / Drag 执行迁移：
  view mode、hidden visibility、split pane toolbar/context wrapper 已迁入
  `src/app_actions/view.rs`；`load_path_into_pane` 进入 `navigation.rs`；
  trash restore conflict wrapper 进入 `trash.rs`；external drag enter/move/drop/left wrapper
  进入 `src/app_actions/drag.rs`。
- 主窗口输入分支收敛：
  main window keyboard shortcut handling 迁入 `src/app_actions/keyboard.rs`，pointer hover /
  cursor handling 迁入 `src/app_actions/pointer.rs`，mouse wheel zoom/scroll scheduling
  迁入 `src/app_actions/scroll.rs`；`main.rs` 的 `window_event` 对应分支只保留事件路由。
- 主窗口 pointer button action 边界：
  trash conflict、task detail、properties、context/drop menu、toolbar、path bar、places、
  item activation 和 pane selection 的点击语义已迁入 `src/app_actions/pointer.rs`；
  `ApplicationHandler::window_event` 对主窗口点击只做参数转发，刷新/present 继续走
  action outcome 调度。
- Pointer button route / effect 分层：
  `src/app_actions/pointer.rs` 新增 main pointer button intent / left-button route，
  先用只读 hit-test helper 计算 route，再集中应用状态修改和 action outcome；`ShellScene`
  补充 task area、places toggle、scrollbar drag、place target 的只读命中 helper，后续
  可以为 pointer route 单独补 focused tests。
- Pointer button planner 抽取：
  `src/app_actions/pointer_route.rs` 承载 main pointer button intent 和 left-button
  route planner，`pointer.rs` 只负责从 scene 收集 snapshot 并执行 effect；planner 已
  覆盖 modal 优先级、鼠标 back/forward、右键 press/release、左键 toolbar/path-bar/
  menu/selection 以及 release 路径的 focused tests。
- Open With query hit testing 收敛：
  search box 的 pointer hit test 进入 `src/shell/open_with/geometry.rs`，scene 的
  cursor 判断不再直接拼 query rect。
- Action Outcome / Presentation 调度边界：
  `src/app_actions/outcome.rs` 统一承载 action 执行后的 `None`、`Redraw`、`Queue`、
  `Present` 结果；除 `outcome.rs` 外的 `src/app_actions/*` 不再直接调用主窗口
  `request_redraw`、`queue_scene_change` 或 `present_scene_change`，后续动画、
  render damage 和性能策略可以挂到统一表现调度入口。
- Action Outcome 组合器：
  `ShellActionOutcome` 增加 `merge` / `with_redraw_if`，用明确优先级合并 `Redraw`、
  `Queue` 和 `Present`；pointer effect 已开始使用组合式 redraw，后续 async
  completion、动画 timeline 和局部 dirty 可以继续返回 outcome 而不是立即触发表现。

## 下一步队列

### 1. Command / Action 层

目标：把 `FikaWgpuApp` 中的用户命令执行路径拆出，使 window event handler 只负责把
输入转换为 action。

候选拆分：
- context menu / drop menu action dispatcher。
- 文件命令执行器：rename、create、delete、trash、paste、open。
- view 命令执行器：zoom、view mode、hidden、split pane、reload。
- 剪贴板、设备操作、trash、paste、drop 等副作用进一步 request 化，并逐步接入
  async operation dispatcher。
- 将 pointer move / drag route 继续收敛到 snapshot + planner，和 pointer button
  使用同一种 route/effect 边界。
- 将更多 effect 从“立即 apply outcome”推进为“返回 outcome/request，由上层合并后
  apply”，便于 async completion、动画 timeline 和 render damage 共享同一套调度语义。

完成标准：
- `ApplicationHandler::window_event` 中的业务分支减少。
- command 函数不直接依赖 `WindowEvent`。
- 测试仍覆盖现有用户工作流。

### 2. Render Surface / Frame Pipeline

目标：把主窗口和 detached dialog 的 frame acquire、text/icon cache begin-frame、
upload、present、logging 管线抽成共享 render surface 层。

候选拆分：
- surface acquire / recover / validation error。
- text/icon frame builder setup。
- vertex/icon/text upload merge。
- render pass encode / present。

完成标准：
- `WgpuState::render_detached_dialog` 不再手写一整套 frame pipeline。
- 主窗口 render 与 dialog render 共享 recover/present 错误策略。
- 日志仍能区分 main frame 和 dialog frame。

### 3. ShellScene Hit Testing / Layout 边界

目标：把 `ShellScene` 中和具体 UI 区域绑定的 hit testing、rect 计算逐步移入对应模块。

优先模块：
- Open With hit testing / cursor。
- Create / Rename hit testing。
- Places sidebar hit testing。
- Task detail dialog hit testing。

完成标准：
- `ShellScene` 暴露较少的 `*_at_screen_point` 手写入口。
- geometry / hit test 和 paint 使用同一套模块内 rect API。
- 测试从“直接改字段”逐步转向 fixture builder 和模块 API。

### 4. Render Dirty / Damage 后续收敛

目标：在已完成文件拆分基础上，继续收缩跨层依赖和测试直接字段访问。

候选清理：
- 把 `ShellRenderDamageSnapshot` 测试访问字段逐步改为断言 helper。
- 把 folder preview damage rect 映射迁到更明确的文件名。
- 将 `damage_snapshot.rs` 内部的 context/drop menu 采样 helper 与对应 UI 模块共享
  rect API。

完成标准：
- dirty key 不依赖 damage bounds helper。
- snapshot 只采集 render 可见状态。
- bounds 只比较 snapshot 并输出 damage。

### 5. Animation Registry

目标：把 `ShellAnimationRuntime` 从 item reflow 专用 runtime 扩展为可挂载多个 timeline
的 animation registry。

候选动画：
- delete fade / scale。
- surviving item reflow。
- hover / selection transition。
- Places reorder transition。
- dialog enter / exit transition。

完成标准：
- 主循环只查询 animation runtime 的 next deadline 和 dirty value。
- 每种动画有独立 key、生命周期和可测试的 easing。
- render damage 能基于动画 dirty value 做最小 invalidation。

### 6. Async Operation Dispatcher

目标：把 `FikaWgpuApp` 中的 async task spawn、completion drain、task status 更新拆到
operation dispatcher。

候选操作：
- trash / restore / delete permanently / empty trash。
- create / rename privileged fallback。
- paste / transfer。
- open with / set default app。

完成标准：
- `FikaWgpuApp` 只提交 operation request 并应用 completion。
- operation runtime 管理 task id、controller、status 文案和 cancellation。
- Empty Trash 等性能敏感路径保留 compio 优先实现。

### 7. Test Fixture Builder

目标：减少测试直接构造 `ShellScene` 字段导致的迁移成本。

候选 builder：
- `TestShellSceneBuilder`
- pane / entries / places / dialogs / task statuses presets。
- damage snapshot helper。

完成标准：
- 新测试不再手写完整 `ShellScene` 字段。
- 迁移字段时主要改 builder，而不是批量改测试。

## 当前推荐顺序

1. Command / Action 层后续：把 pointer move / drag route 继续收敛到 snapshot +
   planner，并开始把 effect 返回值统一成 action outcome。
2. Async operation dispatcher：优先承接 trash / paste / drop / create / rename 等文件
   操作，把 completion 也映射为 action outcome。
3. Animation registry：将 delete/reflow/hover 等 timeline 挂到 outcome 的 Queue /
   Present 调度后面。
4. Render surface / frame pipeline。
5. ShellScene hit testing 模块化。
6. Test fixture builder 穿插进行。
7. Render dirty / damage 后续收敛穿插进行。

## 每步提交前验证

```bash
cargo fmt
cargo check
cargo test
git diff --check
```
