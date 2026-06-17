> 本文是 [GPUI_DOLPHIN_MIGRATION_PLAN.md](GPUI_DOLPHIN_MIGRATION_PLAN.md) 的简体中文翻译。

# GPUI + Dolphin 迁移计划

这是将当前 Slint UI 替换为 GPUI、同时以 Dolphin 作为首个参考目标的详细计划。

> **状态：已完成。** 全部 8 个实现切片已交付至 GPUI
> mainline。Slint 实现已移除。当前代码库遵循此计划的架构：
> UI-neutral 的 `fika-core` 库，位于 `src/ui/` 的 GPUI shell，
> 以及 Dolphin 风格的 directory/model/selection 契约。参见
> `README.md`、`docs/DESIGN.md` 和 `docs/TODO.md` 了解当前状态。
> 本文档作为架构过渡的历史记录予以保留。

## 1. 目标

Fika 将被重建为：

- 面向 directory/model/operation 行为的 UI-neutral Rust core
- 用于窗口、pane、item view、菜单、对话框和输入的 GPUI shell
- 类似 Dolphin 的 directory lister 和 model 信号流

这次迁移是对 UI 架构的重写，并非对 `.slint` 文件的兼容性移植。

## 2. 为什么替换 Slint

当前问题是结构性的：

- 静态的 `.slint` 组件形状迫使使用基于 slot 的 pane 而非自然的 pane 实体
- 动态 pane 状态必须通过 `ModelRc<VecModel<...>>` 投影
- 文件标识、行标识、slot 标识、叠加层标识和 pane 标识必须手动同步
- 回调容易丢失 pane 标识并回退到聚焦 pane
- 目录刷新和 undo 需要胶水代码来弥补缺失的 lister/model 信号边界
- 每次 UI 状态变更都有触发完整 pane 或 sidecar model 重建的风险

GPUI 本身并不解决文件管理器语义，但它让 Fika 能够将 pane、view state 和 model 更新直接表达为 Rust 实体。这消除了当前掩盖执行流错误的静态 UI 胶水层。

## 3. 不可妥协的架构规则

- 实现前必须检查 Dolphin 源码流。
- 每个 pane 都有稳定的 `PaneId`。
- 每个 pane 作用域的异步结果都携带 `PaneId + generation`。
- 每个同代重叠请求都携带请求序列号。
- 目录变更首先进入 `DirectoryLister`，然后进入 `DirectoryModel`，最后进入 view。
- Undo 和文件操作通过 lister/model 事件刷新，而不是手动 UI 重建。
- 关闭 pane 会丢弃其 lister、watcher、待处理的 view 工作和过期结果目标。
- GPUI 路径中不保留任何 Slint 兼容代码。

## 4. Dolphin 源码执行流

### 目录入口点

`../dolphin/src/views/dolphinview.cpp:2337`

```cpp
void DolphinView::loadDirectory(const QUrl &url, bool reload)
{
    if (reload) {
        m_model->refreshDirectory(url);
    } else {
        m_model->loadDirectory(url);
    }
}
```

Fika 目标：

```rust
pane.lister.load_directory(path, LoadMode::Load);
pane.lister.load_directory(path, LoadMode::Reload);
```

手动刷新、watcher 重新扫描以及操作/undo 受影响目录的刷新必须调用 reload 路径。

### Lister 到 Model

`../dolphin/src/kitemviews/kfileitemmodel.cpp:300`

```cpp
connect(m_dirLister, &KCoreDirLister::itemsAdded, this, &KFileItemModel::slotItemsAdded);
connect(m_dirLister, &KCoreDirLister::itemsDeleted, this, &KFileItemModel::slotItemsDeleted);
connect(m_dirLister, &KCoreDirLister::refreshItems, this, &KFileItemModel::slotRefreshItems);
connect(m_dirLister, &KCoreDirLister::listingDirCompleted, this, &KFileItemModel::slotCompleted);
```

Fika 目标：

```rust
enum DirectoryListerEvent {
    ItemsAdded { pane_id: PaneId, generation: Generation, path: PathBuf, entries: Vec<Entry> },
    ItemsDeleted { pane_id: PaneId, generation: Generation, path: PathBuf, paths: Vec<PathBuf> },
    ItemsRefreshed { pane_id: PaneId, generation: Generation, path: PathBuf, pairs: Vec<RefreshPair> },
    ListingRefreshed { pane_id: PaneId, generation: Generation, path: PathBuf, entries: Vec<Entry> },
    ListingCompleted { pane_id: PaneId, generation: Generation, path: PathBuf },
    CurrentDirectoryRemoved { pane_id: PaneId, generation: Generation, path: PathBuf },
    Error { pane_id: PaneId, generation: Generation, path: PathBuf, message: String },
}
```

`LoadingStarted` 是一个请求/生命周期信号，而非可视化 model 重置。Fika
在当前请求 pending 期间保持前一个 `DirectoryModel` 和 pane 布局可见，只取消瞬时 UI 交互，
并在匹配的 `ListingRefreshed` 到达时替换 model。这遵循 Dolphin 在目录切换期间的实际无空白帧行为，
并避免了异步 listing 慢于 UI 帧时的闪烁。

### Model 到 View

`../dolphin/src/kitemviews/kitemlistview.cpp:1812` 将 model 连接到 item view slot，处理 item 变更、插入、移除、移动、分组和排序。

Fika 目标：

```rust
enum DirectoryModelSignal {
    ItemsInserted(ItemRangeList),
    ItemsRemoved(ItemRangeList),
    ItemsChanged(ItemRangeList, ChangedRoles),
    ItemsMoved(MoveList),
    GroupsChanged,
    SortChanged,
    ModelReset,
}
```

GPUI view 订阅 pane-local 的 model 信号。它们不解析文件系统 watcher 事件。

### 当前目录删除

`../dolphin/src/dolphinviewcontainer.cpp:1088` 将被删除的本地当前目录跳转到最近仍存在的上级目录。

Fika 目标：

- lister 检测当前目录删除
- pane 验证 `PaneId + generation + path`
- pane 导航到最近仍存在的上级目录
- 在该 pane 中显示消息

## 5. 目标模块设计

### `fika-core::pane`

拥有：

- `PaneId`
- `PaneGeneration`
- `PaneState`
- pane 历史
- pane 选择
- pane view 选项

公开 API：

```rust
pub struct PaneId(u64);

pub struct PaneState {
    pub id: PaneId,
    pub generation: Generation,
    pub current_dir: PathBuf,
    pub model: DirectoryModel,
    pub selection: SelectionState,
    pub view: ViewState,
}
```

### `fika-core::directory`

拥有：

- `DirectoryLister`
- watcher 抽象
- lister 事件分类
- 完整 reload 回退
- 当前目录删除检测

Watcher 不能应用 UI 变更。它只提供 lister 事件。

### `fika-core::model`

拥有：

- `DirectoryModel`
- 条目存储
- path-index 查找
- 排序/过滤
- 回收站元数据
- model 信号

### `fika-core::operations`

拥有：

- 文件操作队列
- 操作进度
- undo 注册
- undo 序列号
- 受影响目录计算

### `fika-gpui`

拥有：

- `FikaApp`
- `MainWindow`
- `PaneEntity`
- `ItemViewEntity`
- 菜单/对话框
- 输入路由
- GPUI 图像/文本渲染

GPUI 实体消费 core 事件并将控制器操作提交回 core。

## 6. Pane 身份契约

必需的不变量：

- `PaneId` 分配一次，进程生命周期内永不重用。
- 分屏打开创建新的 `PaneId`。
- 分屏关闭会丢弃已关闭 pane 的 watcher/lister。
- 焦点变更不会改变 pane 身份。
- 异步结果应用永远不使用聚焦 pane 作为回退。
- 两个 pane 显示同一路径时，仍然拥有独立的 generation、selection、scroll、lister 和 watcher 状态。

测试：

- 目录读取进行中时关闭 pane -> 结果被忽略
- 对同一路径分屏两个 pane -> 外部创建通过各自事件更新两者
- undo 完成时将焦点切换到其他 pane -> 原始受影响的 pane 刷新
- 对非活跃 pane 手动刷新 -> 非活跃 pane 更新，聚焦 pane 不更新

## 7. 目录刷新设计

### Load

1. 用户将 pane 导航到某路径。
2. Pane 递增 generation。
3. Pane 启动 lister load。
4. Lister 发出 loading started。
5. Lister 扫描条目。
6. Model 接收 items/listing。
7. View 接收 model 信号。
8. Lister 发出 completed。

### Refresh

1. 手动 F5、watcher 重新扫描、操作结果或 undo 结果调用 pane lister reload。
2. Lister 在可能的情况下产生条目增量。
3. 如果事件无法分类，lister 发出 `ListingRefreshed`。
4. Model 对比当前条目并发出 insert/delete/change 信号。
5. View 更新可见条目布局。

### Watcher 增量

映射：

- create -> `ItemsAdded`
- remove -> `ItemsDeleted`
- rename both -> `ItemsRefreshed`（携带 old/new 对）
- modify metadata/data -> `ItemsRefreshed`
- rescan/no path/unclassified -> `ListingRefreshed`
- watched root removed -> `CurrentDirectoryRemoved`

## 8. GPUI 渲染计划

### 首选 View 模式

从 Dolphin compact 水平布局开始：

- 行垂直填充
- 列水平推进
- 普通滚轮水平滚动
- item layout 拥有图标矩形和文本矩形
- model 索引不是 GPUI 行索引

### View 分层

推荐的 GPUI view 组合：

- pane chrome view
- search/filter view
- item viewport view
- selection overlay
- 右键菜单层
- 对话框层

Item viewport 应当是一个 Rust 拥有的可见布局，而非静态列表 widget。model 和 layouter 决定可见索引；GPUI 渲染这些条目。

## 9. 迁移阶段

### 阶段 A：探测

交付物：

- 独立的 GPUI 二进制或 crate
- 单个 pane
- 加载本地目录
- 显示名称
- 外部 create/delete 刷新 view

验收标准：

- GPUI crate 无 Slint 依赖
- 目录事件携带 `PaneId + generation`
- 测试覆盖过期读取结果和 watcher 更新

### 阶段 B：Core 提取

交付物：

- `fika-core` crate
- 移入 file ops、entries、generation、operation controller
- 在需要的地方提供 UI-neutral 的 image/cache 类型

验收标准：

- core 在没有 Slint 或 GPUI 的情况下构建
- 旧的 Slint 二进制仅在保留于旧路径时仍可编译
- 新测试直接针对 core 运行

### 阶段 C：Directory Model 对等

交付物：

- `DirectoryLister`
- `DirectoryModel`
- Dolphin 风格的 model 信号

验收标准：

- add/delete/rename/modify 测试
- 完整 reload 回退测试
- 当前目录删除测试
- 分屏 pane 相同路径测试

### 阶段 D：GPUI Pane Shell

交付物：

- 动态 pane 树
- 分屏打开/关闭
- 焦点路由
- 地址栏
- 状态栏

验收标准：

- 每个 pane 操作通过 `PaneId` 解析
- 非活跃 pane 刷新正常工作
- 关闭 pane 丢弃 watcher/lister

### 阶段 E：Item View 和选择

交付物：

- compact 布局
- 滚动
- hit-test
- 单选/Ctrl/Shift/rubber-band 选择

验收标准：

- 选择测试是 core/controller 测试
- UI 渲染可替换
- 大目录保持响应

### 阶段 F：操作和 Undo

交付物：

- copy/move/link/trash/rename/create
- 带序列号的 undo
- 受影响 pane 刷新

验收标准：

- undo 永远不会应用过期的序列号
- undo 刷新不调用手动 UI 重建
- 文件操作完成刷新所有受影响的 pane

### 阶段 G：功能恢复

交付物：

- 右键菜单
- service 菜单
- open with
- 缩略图
- 设备
- 递归搜索
- portal chooser

验收标准：

- 每个功能通过 pane/core 契约路由
- 无聚焦 pane 回退
- 无 UI 线程阻塞 I/O

### 阶段 H：切换

交付物：

- GPUI 应用成为主要 `fika`
- Slint UI 从主构建中移除
- 文档更新

验收标准：

- `Cargo.toml` 不再依赖 `slint` / `slint-build`
- `ui/*.slint` 已删除或归档在构建之外
- README 描述 GPUI 架构

## 10. 测试策略

必需的测试类别：

- directory model delta 的 core 单元测试
- watcher 分类测试
- 过期 generation 测试
- undo 序列号测试
- 分屏 pane 身份测试
- 操作后的 lister refresh 测试
- 应用启动和基本 pane 渲染的 GPUI smoke 测试

手动 smoke 用例：

- 在当前 pane 中外部创建文件
- 在当前 pane 中外部删除文件
- 外部重命名文件
- 删除当前目录
- 对同一路径分屏两个 pane 并外部修改路径
- undo copy/move/trash/rename
- 异步读取进行中时关闭 pane

## 11. 切换标准

GPUI 重写仅在以下条件满足后替换 Slint 应用：

- 目录刷新正确，无需分屏切换 workaround
- undo 刷新正确，无需手动 pane 重建
- 分屏 pane 身份稳定
- 回收站元数据变更更新可见的回收站 model
- 缩略图和 service 菜单在 UI 线程之外工作
- portal chooser 路径已确定
- 旧的 Slint 依赖可以移除

## 12. 风险

- GPUI text/layout primitives 可能需要自定义的 item-view 渲染工作。
- 文件管理器语义仍然需要自定义的 lister/model 逻辑；GPUI 只消除了 UI 胶水层压力。
- Portal chooser 嵌入可能需要与主 GPUI shell 不同的独立策略。
- 部分 Slint 时代的测试断言的是实现形状而非行为，必须被替换。

## 13. 首个实现切片

按以下顺序实现：

1. `PaneId`、`Generation`、`DirectoryListerEvent`、`DirectoryModelSignal`。
2. UI-neutral 的目录条目类型。
3. 带 add/delete/refresh/full listing API 的 `DirectoryModel`。
4. 用于本地目录 load/reload 的 `DirectoryLister`。
5. watcher 分类提供 lister 事件。
6. GPUI 单 pane shell。
7. GPUI 分屏 pane shell。
8. 通过 lister reload 实现 undo refresh。

在 directory lister 和分屏 pane 身份契约被证明之前，不要迁移菜单、缩略图、设备或 portal。
