# Fika Design

本文档记录 Fika 当前架构、设计边界和后续扩展方向。它的目标不是冻结实现，而是给后续逐项完善提供稳定参照。

交互细节可参考 `docs/DOLPHIN_REFERENCE.md`，其中记录了本地 `./dolphin` 源码链接中与右键菜单、选择、拖拽、视图状态和搜索相关的关键文件。

## Goals

Fika 是一个面向现代 Wayland 桌面的轻量文件管理器原型。当前优先级是：

- 保持 UI 响应，不让目录读取、MIME 探测、文件操作阻塞 Slint 主线程。
- 采用 Dolphin 风格的布局逻辑：顶部路径/工具栏、左侧 Places、右侧列优先图标视图、底部状态栏。
- 使用 Slint `1.16.1`，UI 保持在 `.slint` 文件中，通过 `build.rs` 编译。
- 避免依赖长期未维护的 MIME/XDG 小库；桌面默认应用解析先采用项目内置实现。
- 为后续 portal chooser 和 Polkit helper 保留清晰边界。

## Non-Goals

当前阶段暂不追求：

- 完整替代 Dolphin/Nautilus。
- 支持所有文件系统后端和远程协议。
- 在 UI 线程中直接执行可能阻塞的 I/O。
- 引入大型 GUI 框架或重写 Slint UI 架构。

## Current Architecture

### UI Layer

入口 UI 在 `ui/app.slint`，共享组件已拆分：

- `AppWindow` 是主窗口。
- `ui/models.slint` 定义 `FileEntry` / `PlaceEntry` / `DesktopApp`。
- `ui/widgets.slint` 包含通用按钮、菜单项、popup surface、Places 行和 `FolderGlyph`。
- `ui/top_bar.slint` 负责父目录/Home、路径输入、搜索入口和主题切换；`AppWindow` 只保留动作 callback、路径输入状态和持久化转发。
- `ui/file_tile.slint` 负责主栏文件项显示、选择、右键菜单、双击打开。
- `ui/status_bar.slint` 负责状态文本、外部受保护编辑动作、Undo、chooser 保存名/过滤/choices/确认按钮；`AppWindow` 只保留状态绑定和动作转发。

主栏当前采用列优先布局：

- `rows-per-column` 由可见高度和 `icon-row-height` 计算。
- `x = floor(index / rows-per-column) * icon-cell-width`
- `y = mod(index, rows-per-column) * icon-row-height`

这样更接近 Dolphin 图标视图的排列逻辑。

主栏现在使用轻量虚拟化：

- `entry_count` 只保存当前过滤后的条目数量，用于空状态和横向滚动宽度。
- `virtual_entries` 只包含当前可见列附近的条目，额外保留少量左右 overscan 列；Slint 不再持有完整 `FileEntry` 模型。
- Rust 侧维护轻量可见索引缓存：无搜索/过滤时使用隐式 identity fast path，有搜索/过滤时只保存匹配条目的 `usize` 索引。
- 滚动同步时 Rust 直接通过可见索引缓存克隆当前虚拟范围的条目；不会为了更新可视窗口构造完整过滤结果模型，也不会在每次滚动事件中重复扫描完整目录。
- Slint 侧把虚拟 tile 放进以 `virtual_start_column` 为锚点的局部 layer；tile 坐标只相对当前虚拟窗口增长，避免超大目录产生很大的每项坐标。
- Rust 缓存虚拟范围、行数、列宽和缩略图尺寸；滚动仍落在同一虚拟范围时不重置 Slint model。
- Rust 侧的 `VirtualGridPlan` 统一计算 clamped viewport、scroll max、可见范围、overscan 范围和 Slint 锚点列，防止滚动条、缩略图调度和模型切片各用一套边界规则。
- 过滤、搜索、缩放或窗口尺寸变化导致内容变窄时，Rust 会按同一套列宽规则夹紧横向滚动位置，避免旧 viewport 落在新内容之外造成空白主栏。
- tile 的真实全局索引由 `virtual_start_index + local index` 计算，因此列优先坐标、选择范围、拖拽命中和右键语义仍然基于完整模型。
- 横向滚动、缩放和窗口尺寸变化会重新切片 `virtual_entries`，避免大目录一次性实例化所有 `FileTile`。
- 框选仍按完整可见顺序返回路径，但候选项会先裁剪到选择矩形横向覆盖的列范围；搜索/过滤状态下通过可见索引缓存解析真实条目。
- 缩略图调度按“当前可见列优先，overscan 后置”排序，减少大目录图片预览队列对当前屏幕反馈的拖慢。
- 离屏缩略图完成时只更新 Rust 缓存，不重置 Slint 模型；缩略图所属路径落在当前虚拟切片内时才刷新 `virtual_entries`。

### State Layer

Rust 侧核心状态在 `AppState`：

- `current_dir`: 当前目录。
- `entries`: 当前目录完整条目。
- `places`: 左侧 Places。
- `search_query`: 当前过滤关键字。
- `selected_paths`: 当前选中项。
- `directory_cache`: 已访问目录的内存条目缓存，用于 back/forward 或重复进入时先即时渲染，再后台刷新；缓存使用 LRU 顺序并限制容量，避免长时间浏览时无限保留完整目录列表。
- `view_state_cache`: 每个目录的主栏 viewport 坐标缓存，用于返回目录时恢复滚动位置；缓存使用 LRU 顺序并限制容量，避免长时间浏览时无限保留每个路径的视图状态。
- `thumbnail_cache`: 按路径、mtime 和目标尺寸缓存缩略图像素。
- `thumbnail_failures`: 按路径、mtime 和目标尺寸缓存缩略图失败结果，避免坏图或不支持格式在大目录滚动时反复排队解码；文件修改后 key 变化，会重新尝试。
- 缩略图完成事件只更新成功/失败缓存和 pending 状态，不扫描或改写完整 `entries`；如果结果落在当前虚拟切片内，Slint 模型通过缓存装饰重新同步可见项。
- `operation_queue`: Move/Copy/Link 操作队列，一次只启动一个后台文件操作。
- `load_generation`: `GenerationCounter`，用于丢弃过期目录加载结果。
- `open_generation`: `GenerationCounter`，用于丢弃过期打开状态。
- `search_generation` / `thumbnail_generation`: 分别用于递归搜索与缩略图加载的 stale-result 控制。

Slint UI 只在主线程更新。后台任务不持有 `AppWindow` 或 `Rc<RefCell<AppState>>`。

Rust 代码当前按低耦合职责拆分为嵌套模块：

- `src/app/`: UI 线程共享状态、异步事件/桥接、DnD payload 解析、Places UI 逻辑、主栏几何和选择辅助。
- `src/config/`: CLI 参数、路径归一化、settings 持久化。
- `src/desktop/`: 内置 MIME/default app 解析、Open With 异步桥接。
- `src/fs/`: entries、Places、文件动作、文件操作、递归搜索、缩略图流水线。
- `src/support/`: generation / stale-result helper。

### Devices

侧栏 Devices 现在由 Rust 模型驱动，而不是在 Slint 中硬编码。当前实现优先保持已挂载目录稳定，同时开始接入 UDisks2 设备发现和挂载：

- 固定包含 `Filesystem`，路径为 `/`。
- 优先解析 `/proc/self/mountinfo`，只显示挂载点位于 `/run/media/$USER`、`/media/$USER`、`/media` 和 `/mnt` 下、且 source / filesystem type 看起来像真实本地设备的路径。`tmpfs`、`proc`、`sysfs` 等伪文件系统会被过滤掉，避免把运行时目录误显示成 Devices。
- 当 mountinfo 不可用时，才回退扫描这些目录下的一级目录。
- 作为增强层，Fika 会 best-effort 查询 system bus 上的 UDisks2 `ObjectManager`，从 `Block` -> `Drive` 关系识别用户可见的外置介质。已插入介质且 `Drive.Removable`、`Drive.MediaRemovable`、`Drive.Ejectable`、`Drive.Optical`、`Drive.ConnectionBus=usb` 任一成立，或 `Drive.MediaCompatibility` 标记为 optical/flash 介质时会列出，这覆盖许多不标记为 removable 的 USB 外置硬盘、读卡器和光学介质；空光驱/空读卡器仍因 `MediaAvailable=false` 被过滤。同时继续尊重 `Block.HintIgnore` / `Block.HintSystem`，避免系统分区进入侧栏。只有带 `org.freedesktop.UDisks2.Filesystem` 接口的 block 才会成为侧栏设备，裸 removable block 会被过滤掉，避免 UI 展示无法 mount/open 的目标。设备模型同时保存显示/打开用的 `path` 和 UDisks2 操作用的 `device_path`：未挂载 filesystem 设备二者通常都是 `/dev/...`；已挂载设备的 `path` 使用第一个 `MountPoints`，`device_path` 继续保留底层 `/dev/...`。当 mountinfo 和 UDisks2 发现同一个挂载点时，mountinfo 行保留显示顺序和标签，但会从 UDisks2 补充底层 `device_path` 和 eject 支持。UDisks2 不可用、超时或返回错误时，不影响 mountinfo 结果。显示名优先使用桌面后端给出的 `Block.HintName`，之后才使用文件系统 label、挂载点名、drive vendor/model 和 raw device path。侧栏 marker 优先使用 UDisks2 drive profile 推导出的 `USB`、`SD`、`CD`，否则回退到 label 首字母；这为后续替换成真实图标保留了稳定的设备语义。
- Devices 发现通过统一 async event bridge 后台运行，`/proc/self/mountinfo` 解析和 UDisks2 system-bus 查询都不会阻塞 UI 线程。`AppState` 保存最近一次设备列表和独立的 `device_generation`；过期的 `DevicesLoaded` 事件会被丢弃。
- 启动后会创建轻量设备 monitor。它订阅 UDisks2 system bus 上 `/org/freedesktop/UDisks2` 命名空间的信号，收到设备对象、属性或挂载状态变化后只向 UI 线程投递 `DevicesChanged`，再由主线程复用现有 `refresh_devices_async()` 刷新流程。为了覆盖桌面后端漏信号、挂载表变化或 UDisks2 不可用的情况，monitor 还会低频比对 Devices 快照；只有快照真实变化时才触发刷新。连续信号会经过 debounce 合并，避免一次插拔造成侧栏多次重建。
- 点击未挂载的 UDisks2 filesystem 设备时，UI 线程只发起后台任务；后台通过 system bus 反查对应 `Block` object 并调用 `org.freedesktop.UDisks2.Filesystem.Mount({})`。成功后刷新 Devices 并打开返回的挂载点；失败信息写入状态栏。
- Devices 行右键菜单复用普通菜单定位和 PopupSurface。Open 只依赖当前行是否 mounted；Mount / Unmount / Eject 则分别由 `DeviceEntry.can_mount`、`can_unmount`、`can_eject` 显式控制。UDisks2 发现出的未挂载 filesystem 会设置 `can_mount`，UDisks2 发现出的已挂载 filesystem 会设置 `can_unmount`，Drive `Ejectable=true` 时设置 `can_eject`；根 Filesystem 行和 mountinfo-only fallback 行保持可打开，但不会显示后端无法执行的 UDisks2 动作。Unmount 调用 `org.freedesktop.UDisks2.Filesystem.Unmount({})`，Eject 调用对应 Drive object 上的 `org.freedesktop.UDisks2.Drive.Eject({})`。这些调用全部通过 Tokio `spawn_blocking()` 离开 UI 线程，完成后刷新 Devices 和当前目录。发起动作前，`AppState` 会按 `device_path` 登记 pending action；同一设备已有 Mount/Unmount/Eject 未完成时，新动作只更新状态栏，不会重复排队 D-Bus 调用。pending action 会叠加到 `DeviceEntry.pending_action`，侧栏行用蓝色 in-progress 状态显示，右键菜单只展示禁用的 Mounting/Unmounting/Ejecting 行。
- UDisks2 方法调用失败时，后端会识别常见 D-Bus error name：busy device、authorization denied / missing polkit agent、already mounted、not mounted、cancelled、timed out。状态栏优先显示可操作 guidance，同时保留原始 error name 和 detail，便于后续真实发行版排查。
- 最近一次设备动作失败会按 `device_path` 记录到 `AppState::device_errors`，`sync_devices()` 刷新侧栏时把该错误叠加到对应 `DeviceEntry.error`。`PlaceButton` 会用红色细边、淡红底和 `!` 标记渲染失败设备；同一设备后续 Mount/Unmount/Eject 成功会清除这个视觉错误状态。
- Unmount/Eject 发起时会把当时的挂载点路径随后台任务一起保存。动作成功后，如果主视图当前目录仍在该挂载点下，Fika 会切回 Home，并清掉 back/forward history 中同一挂载点下的条目。这参考了 Dolphin `setViewsToHomeIfMountPathOpen()` 和 cosmic-files 对已卸载 location 的处理，避免停留或回退到失效路径。
- 设置 `FIKA_DEBUG_DEVICES=1` 启动 Fika 时，会把设备发现和 monitor 诊断输出到 stderr，包括 mountinfo 是否可用、UDisks2 接受的设备、被过滤设备的原因、monitor 刷新原因、单行发现摘要、mountinfo-only / UDisks2-only / merged 行数，以及最终合并后的 Devices 侧栏列表。UDisks2 接受行和最终列表都会打印 marker；发现摘要记录 mountinfo/root-scan 来源、UDisks2 行数、最终 mounted/unmounted 行数和 Mount/Unmount/Eject 能力计数，用于真实 U 盘、外置硬盘、polkit/UDisks2 发行版差异验证。
- `scripts/check-runtime-integration.sh` 的普通运行模式会额外报告 UDisks2 system service 状态、`udisksctl` 可用性、`org.freedesktop.UDisks2` system-bus owner/activation 状态，并调用 ObjectManager 统计 Block / Drive / Filesystem 接口数量。这是只读探测，不会执行 Mount、Unmount 或 Eject，适合打包后在不同发行版上确认 Devices 侧栏的后端条件。
- `fika --diagnose-devices` 是不启动 GUI 的只读诊断入口，会直接运行 Rust 侧 Devices 发现逻辑并输出发现摘要、merge 统计、UDisks2 discovery 错误、label、marker、显示路径、底层 device path、mounted 状态和 Mount/Unmount/Eject 能力。它用于把真实机器上的侧栏输入模型和后端来源直接贴出来排查，不会执行任何设备动作。

这能覆盖常见桌面环境已经自动挂载的 U 盘路径，也能提前显示并挂载部分未挂载 U 盘。这个分层参考了 Dolphin 通过 Solid/KMountPoint 处理真实挂载点、以及 cosmic-files 将设备抽象为 mounter item 再填入侧栏的结构。后续完整设备管理应继续基于 UDisks2 system bus D-Bus，并在真实发行版上验证 UDisks2 / polkit 边界情况。

### Async Runtime

Tokio runtime 在 `main()` 启动时创建，并持有到 `ui.run()` 返回。

当前异步通道：

- 后台任务通过统一 `async_tx/rx` 发送 `AsyncEvent`。
- `AsyncEvent::DirectoryLoaded` 回传目录读取结果。
- `AsyncEvent::FileOpened` 回传文件打开结果。
- `AsyncEvent::ThumbnailLoaded` 回传缩略图解码结果。
- 后台任务发送事件后调用 Slint `Weak::upgrade_in_event_loop()` 唤醒 UI 线程。
- UI 线程通过 `async_results_ready` callback drain channel 并应用事件。

当前已异步化：

- `tokio::fs::read_dir()` 读取目录。
- `tokio::fs::DirEntry::metadata()` 读取 metadata。
- `spawn_blocking()` 执行 MIME 探测、`mimeapps.list` 解析、`.desktop` 解析和默认应用启动。
- `spawn_blocking()` 解码 PNG/JPEG/WebP 缩略图并回传 RGBA 像素。

异步 stale-result 策略：

- 每条异步流水线持有自己的 `GenerationCounter`。
- 启动新任务时调用 `next()` 得到 generation，并随 `AsyncEvent` 返回。
- UI 线程应用结果前调用 `is_current()`；过期结果直接丢弃。

### MIME / Default App

`src/mime_open.rs` 内置默认应用打开逻辑：

- 读取文件头做简单 magic 探测。
- fallback 到扩展名。
- 读取 XDG `mimeapps.list`。
- 解析 `.desktop` 的 `Exec=`, `Name=`, `NoDisplay=`, `Terminal=`。
- 展开 desktop exec field codes。
- 右键文件的 Open With 是 hover 子菜单，候选应用来自当前 MIME 类型的默认应用、`mimeapps.list` 的 `Added Associations`，以及系统 `applications/mimeinfo.cache` 的 `MIME Cache`。
- Open With hover 子菜单根据剩余窗口空间选择向右或向左展开，避免靠近右边缘时被挤压。
- Open With 子菜单最后一项是 `Other Applications...`，会打开应用选择框。
- 应用选择框支持指定 desktop app 打开、一次性自定义命令打开，以及写入用户级 `mimeapps.list` 设置默认应用。
- `Other Applications` 弹窗使用一个对话框级别的 “Set selected application as default” 勾选框；候选列表只负责选择要打开的应用。
- `Open Terminal Here` 参考 cosmic-files 的终端选择方式：保留 `FIKA_TERMINAL` / `TERMINAL` 显式覆盖优先级，然后查询 `xdg-mime query default x-scheme-handler/terminal` 并解析对应 `.desktop`；只有可见且 `Categories` 包含 `TerminalEmulator` 的 entry 会作为桌面终端候选。之后再优先尝试 `com.system76.CosmicTerm.desktop`、其它 `TerminalEmulator` desktop entries 和内置终端可执行文件 fallback。

这个模块目前是同步实现，因此从 UI 调用时必须通过 `spawn_blocking()`。

### Persistence

当前持久化：

- Places 存储在 `$XDG_CONFIG_HOME/fika/places.tsv` 或 `~/.config/fika/places.tsv`。
- window size、dark mode、sidebar width、icon zoom level、last opened directory 存储在 `$XDG_CONFIG_HOME/fika/settings.tsv` 或 `~/.config/fika/settings.tsv`。

## Interaction Model

### Selection

当前支持：

- 单击文件项：单选。
- Ctrl+单击：切换多选。
- Shift+单击：按当前可见顺序选择从锚点到目标的范围。
- Ctrl+Shift+单击：把锚点到目标的范围追加到现有选择。
- Ctrl+A：选择当前过滤后可见的所有项。
- Ctrl+C / Ctrl+X / Ctrl+V：复制、剪切、粘贴文件路径，粘贴目标为当前目录；Ctrl+Z 触发最近一次文件操作 Undo；Delete 将当前选择移动到回收站。上述文件操作快捷键使用 Slint `KeyBinding` 声明，并在路径输入框、搜索框、保存名输入框、右键菜单、传输菜单或对话框活跃时不执行，避免抢走文本编辑快捷键。
- 主栏空白拖拽：显示选择矩形，松手后选择与矩形相交的可见 tile；Ctrl+拖拽会追加到当前选择。
- 主栏采用列优先布局，不提供垂直滚动；普通滚轮和水平滚轮都绑定横向滚动，Ctrl+滚轮缩放图标。左栏是独立纵向滚动区域。主栏空白层、grid 层和 `FileTile` 的滚轮入口都统一转发到 `AppWindow` 的 `handle-main-scroll()`，避免缩放方向、边界夹紧和横向滚动规则在多个组件中漂移。
- Esc：优先关闭上下文菜单；没有菜单时清空选择。
- 点击主栏空白处：取消选中并转移焦点。
- 双击：打开目录或文件。
- F5：刷新当前目录。
- 鼠标 Back/Forward：按目录历史后退/前进。
- 多选右键菜单只暴露已实现的批量安全动作。当前批量菜单显示选中摘要和 `Move Selected to Trash`；`Rename`、`Open With`、`Add to Places` 等单项动作在多选状态下隐藏，直到存在明确的批量语义。单目录菜单中的 `Add to Places` 也会在该路径已存在于 Places 时隐藏，避免显示无效动作。
- 右键菜单按 Dolphin/Qt 的父子菜单模型模拟：根菜单以触发点为首选位置，放不下时向左/上翻转，然后 clamp 到窗口安全边距；子菜单锚定在父菜单项行，同样按可用空间水平翻转并垂直 clamp。父项与子菜单之间有不可见 hover bridge，bridge 高度会跟随实际 clamp 后的子菜单位置，避免从父项斜向移动到子菜单时误触发关闭。普通菜单项 hover 不主动关闭已有子菜单，关闭由父/子 hover leave 后的短延迟处理。Transfer 根菜单、Open With / Create New 子菜单和 hover bridge、chooser-choice 上方锚定 popup 都复用 Rust `PopupPlacement` 几何 helper。
- 菜单外观和通用交互件集中在 `ui/widgets.slint`：`MenuItem` / `MenuSeparator` / `MenuTitle` / `PopupSurface` / `MenuHoverBridge` / `MenuDismissLayer`。具体菜单内容和弹出层承载组件在 `ui/menus.slint`；`RootContextMenuLayer` 统一挂载 file / Places / Devices / blank-area 菜单，并负责根菜单宽高选择、触发点 flip/clamp 定位以及 Open With / Create New 父行锚点计算；`TransferMenuLayer` 统一挂载拖放操作菜单，并封装 Transfer 菜单固定尺寸与坐标转换；`ChildSubmenuLayer` 统一承载 Open With / Create New 子菜单并封装子菜单尺寸、子菜单定位与 hover bridge 定位输入，`ChooserChoicePopupLayer` 统一挂载 chooser choice 弹出菜单并封装锚点定位公式。`ui/app.slint` 保留菜单动作、父子菜单生命周期和对 Rust 菜单几何 callback 的转发。所有右键入口通过 `show-context-menu()` 统一写入 kind、触发坐标、关闭子菜单并停止延迟关闭 timer。Open With / Create New 的父项、hover bridge 和子菜单内容都走同一个 child-submenu hover/timer 入口，chooser-choice popup 保留从按钮上方弹出的语义但边界 clamp 也由 Rust helper 计算。
- 鼠标侧键 Back/Forward 在 winit 层统一处理，先用 Rust `main_pane_bounds` 判断指针是否位于右侧主栏内，命中后阻止事件继续传播；侧栏和顶栏不会触发目录历史导航。几何层会返回明确的 Back/Forward 方向和缩放后的逻辑坐标，日志直接记录该导航意图。这与 cosmic-files 的 mouse area 先检查 cursor bounds、命中后 capture event 的模式一致。

当前导航模型：

- `navigate_to()` 会把当前目录压入 back stack，并清空 forward stack。
- refresh 和 watcher reload 不进入历史。
- mouse Back/Forward 使用 back/forward stack，不等同于“上一级”。
- 未缓存目录导航不会立即清空旧主栏，而是保留旧画面并显示轻量 loading 遮罩；新目录结果到达后再原子替换，避免短暂白屏闪烁。
- back/forward 不在 UI 线程同步 `stat` 历史目标，避免慢盘或网络挂载阻塞事件循环。
- 已访问目录会先从 `directory_cache` 即时显示，再启动异步刷新，兼顾“快”和新鲜度。
- 目录切换前会记录当前主栏滚动位置，进入已访问目录时恢复对应 viewport，减少 back/forward 后的视觉上下文丢失。
- 鼠标 Back/Forward 只在右侧主栏范围触发，避免顶栏、侧栏或分隔条上的操作意外改变目录历史。

调试：

- 导航和异步流水线日志默认静默。
- 设置 `FIKA_DEBUG_NAV=1` 可重新启用 `[fika nav]` 诊断输出。

### Places Drag

当前 Places 拖拽使用 Slint master `DragArea` / `DropArea` 和 `data-transfer` 的“预览 + 插入线 + 松手提交”模型：

- 拖动时不实时修改 Rust 模型；DropArea hover 只更新 ghost 和插入槽位。
- 只显示 ghost 和插入槽位。
- 松手后通过 path 找到源 Places 项，再调用内部 reorder 逻辑一次性提交。

这个模型应继续用于后续列表拖拽，避免边拖边重建模型导致抖动。

Slint `DragArea` / `DropArea` 引入策略：

- 主栏 tile 通过 `DndApi.make-drag-folder()` / `DndApi.make-drag-file()` 构造内部 `data-transfer` user data，不再用自定义 MIME 字符串承载内部路径。
- Places 项通过 `DndApi.make-drag-place()` 构造内部 `data-transfer` user data。
- Places sidebar、主栏空白和主栏目录 tile 已使用 `DropArea` 接收主栏条目、Places 项和外部本地路径 drop，并根据 `DropEvent` 的 data-transfer kind 决定 reorder、add place 或弹出 Move / Copy / Link 菜单。
- 拖到 Places 缝隙时显示插入线；拖到 Places 项或主栏目录时高亮目标项，颜色与普通选中态区分。
- Places 插入线、拖拽 ghost、拒绝提示和主栏拒绝提示统一由 `ui/dnd_overlay.slint` 的 `DragOverlayLayer` 承载；`ui/app.slint` 只保留 hover/drop 状态和动作转发。
- Places DropArea 的 hover target、gap/item 判定和插入 slot 通过 Rust `PlaceDropGeometry` 计算；winit fallback 复用同一 helper，避免滚动后的视觉反馈和松手提交规则漂移。
- Places DropArea 已接受 `text/uri-list` / `text/plain` payload，并按缝隙插入目录 Place；主栏 DropArea 也接受同类外部本地路径 payload，普通文件和目录都会进入 transfer 菜单；平台 `DroppedFile` 事件仍作为 fallback 保留。
- 保留当前 ghost、插入线和松手提交模型；`DropArea.contains-drag` 只驱动 hover/slot 视觉，不直接修改 Rust 模型。
- 外部文件管理器拖入的 `file://` / text-uri-list 优先尝试 Slint DropArea 路径；winit `DroppedFile` 桥接继续覆盖平台只发送文件 drop 事件的情况。

### Internal Drag And Transfer

当前内部拖拽语义：

- 主栏文件夹拖到 Places 项之间的缝隙：按缝隙位置插入为新的 Place。
- 主栏文件夹拖到 Places 项本体：弹出 Move / Copy / Link 菜单，目标为该 Place。
- 主栏普通文件拖到 Places 项本体：弹出 Move / Copy / Link 菜单，目标为该 Place。
- 主栏普通文件拖到 Places 项之间的缝隙：不作为 Place 添加，避免把不可打开为目录的文件写入 Places。
- 主栏文件夹拖到主栏文件夹：弹出 Move / Copy / Link 菜单，目标为该文件夹。
- 主栏文件夹拖到主栏空白处：弹出 Move / Copy / Link 菜单，目标为当前目录。
- 主栏普通文件拖到主栏文件夹或空白处：弹出 Move / Copy / Link 菜单，目标分别为该文件夹或当前目录。
- Places 项拖到主栏空白处：弹出 Move / Copy / Link 菜单，目标为当前目录。
- Places 项拖到主栏文件夹：根据松手坐标和列优先网格几何计算目标 tile；如果命中目录则以该目录为目标，否则退回当前目录。
- 拖到自身或自身子目录不会打开 transfer 菜单；Rust 侧准备菜单和执行操作都会拒绝这类目标并在状态栏提示。
- Drop transfer 菜单提供 Move / Copy / Link / Cancel，Cancel 只关闭菜单不入队操作。

Move / Copy / Link 通过 Tokio `spawn_blocking()` 执行真实文件系统操作，完成后回到 UI 线程更新状态。操作会先检查目标目录中是否已有同名条目；存在冲突时弹出对话框，由用户选择 Overwrite、Keep Both、Rename 或 Skip，之后才进入 `operation_queue`。Paste 会复用同一套 transfer 接受/拒绝规则，只统计真正被接受的传输；Cut 剪贴板只在至少一个 move 被接受后清空，拒绝项不会误报为已排队。队列一次启动一个任务；copy 和跨文件系统 move 会按字节汇报进度。用户可以取消尚未开始的排队任务，也可以取消正在复制的活动任务。当前目录受影响时会触发刷新。权限不足时，UI 会保存待执行命令并弹出确认框；确认后通过受限 D-Bus helper 重试，GUI 进程本身不做 root 写入。

### External Drag And Drop

外部文件管理器拖入 Fika 有两类目标：

- Slint `DropArea` 内部拖拽通过 `DropEvent.data.user_data()` 区分 place / folder / file；外部拖拽通过 `DropEvent.data.fetch_plaintext()` 读取 `text/uri-list` / `text/plain` 风格 payload。Rust 负责外部 payload 解析、source label 生成和路径归一化；Slint payload 和 fallback 都复用同一套 drop path normalization，支持注释行、多行 uri-list 的首个有效本地条目、`file:///...` / `file://localhost/...`、远程 file URI 拒绝和百分号解码。
- 拖到 Places 缝隙时，外部目录按 hover slot 添加为 Place；非目录会通过状态栏提示，避免把普通文件写入 Places。
- 拖到主栏文件夹时，普通文件和目录都会弹出 Move / Copy / Link 菜单，目标为该文件夹；拖到主栏空白处则以当前目录为目标。主栏外部 drop 和内部 drop 复用 `main_drop_allowed()`、目标文件夹高亮和 self/subdirectory 拒绝规则。
- winit 事件 hook 作为 fallback 记录最后的鼠标位置，并在 `WindowEvent::DroppedFile` 时把路径、逻辑坐标和 `winit DroppedFile fallback` source 送回统一 `AsyncEvent`。fallback 的 Places drop 使用 Rust `PlaceDropGeometry`，基于 Slint 同步出来的列表起点和行距统一计算 target item、gap 和 insertion slot；外部文件 drop 强制按 gap 插入，避免 sidebar 滚动后和插入线视觉位置漂移。
- 成功添加外部 Places 后状态栏会显示处理来源，例如 `Slint DropArea text/uri-list` 或 `winit DroppedFile fallback`，用于后续真实桌面测试判断何时可以移除 fallback。
- winit fallback 的 source/mime 常量、禁用开关解析、启动诊断摘要、DnD trace 格式化和拒绝原因 debug tag 映射集中在 `src/app/dnd.rs`；`main.rs` 只保留实际 winit 事件接入、debug 开关判断和 UI wakeup。
- 设置 `FIKA_DISABLE_WINIT_DROP_FALLBACK=1` 启动 Fika 时，只会禁用 winit `DroppedFile` 旁路，保留 Slint `DropArea`。这用于真实桌面测试中确认外部文件管理器是否稳定发送 `text/uri-list` / `text/plain` 到 Slint。
- 设置 `FIKA_DEBUG_DND=1` 启动 Fika 时，会先把 DnD 启动配置输出到 stderr，包括 `slint_droparea=primary`、Slint DropArea 接受的 payload 类型、winit `DroppedFile` fallback 是否启用以及 `winit_fallback_role=compat`。之后每次 Places / main-pane drop 都会输出 backend、`role=slint-primary` 或 `role=winit-fallback`、phase、data-transfer kind 或外部 payload 类型、解析验证结果、逻辑坐标和目标几何；解析验证结果会标出 `external-local-path path=...`、`internal-drag` 或 `rejected reason=...`，拒绝 tag 包括 `unsupported-mime`、`empty-payload`、`no-local-file-path`、`self-target` 和 `descendant-target`。实际 drop 解析失败时状态栏也会显示对应原因。这个开关同时覆盖 Slint `DropArea` 和 winit `DroppedFile` fallback，方便对比真实桌面环境到底走了哪条路径。
- fallback 的移除标准不是代码路径存在与否，而是 `FIKA_DISABLE_WINIT_DROP_FALLBACK=1 FIKA_DEBUG_DND=1` 下，从目标桌面文件管理器拖入 Places 和 main-pane 都能稳定看到 `role=slint-primary` 且 `validation=external-local-path` 的 Slint DropArea 日志；在此之前 winit fallback 保留为兼容旁路。
- 如果 fallback drop 坐标位于 Places sidebar 内，Rust 根据 `PlaceDropGeometry` 的插入 slot 复用 `add_place_at_slot()`。
- 项目依赖 Slint master 后不再需要 `SLINT_ENABLE_EXPERIMENTAL_FEATURES` 来编译 `DragArea` / `DropArea`；winit hook 仅作为鼠标侧键和外部 `DroppedFile` 兼容 fallback。

### Places Management

Places 分为内置项和用户项：

- 内置项来自默认 Home/Desktop/Documents/Downloads 等位置。
- 用户项来自 `Add to Places`，可重命名、移除、拖拽排序。
- Places 右键菜单对用户项显示 Rename、Remove、Open in New Window。
- Open in New Window 通过当前可执行文件打开对应路径，并优先纳入 systemd user transient scope；成功创建 scope 时记录 unit 名称，systemd 不可用时仍打开窗口并在状态栏显示非致命诊断。
- 内置项不暴露 Rename/Remove，避免误删默认入口。
- Restore Defaults 会恢复默认 Places，并写回 `places.tsv`；入口在 Places 空白区域右键菜单中。该空白菜单还会在当前目录尚未存在于 Places 时显示 Add Current Folder，复用普通目录的 Add to Places 后端。

### File Actions

当前支持：

- 新建文件夹：从主栏空白右键菜单 `Create New > Folder` 打开命名对话框，在当前目录创建；重名时自动生成 `copy` 后缀。
- 重命名：文件/文件夹右键菜单打开对话框，只接受单级名称，拒绝路径分隔符。
- Duplicate Here：右键单项后在同一父目录中排队执行 copy。
- Copy Location：把当前条目的绝对路径写入 Wayland 桌面文本剪贴板，使用 `wl-copy`。
- Properties：显示名称、路径、类型、大小和修改时间。
- 内部 Cut / Copy / Paste：单项和多选菜单可以暂存路径；主栏空白和目录项菜单在有内部剪贴板内容时显示 Paste，实际执行复用 async move/copy 队列。
- 桌面剪贴板：Cut / Copy 同时尝试写入 `x-special/gnome-copied-files`，payload 第一行为 `cut` 或 `copy`，后续为 `file://` URI。Copy 在该 MIME 发布失败时会回退到 `text/uri-list`，提高与只理解 URI list 的桌面组件互操作性；Cut 不回退，因为 URI list 本身没有 move/cut 语义。打开右键菜单或执行 Paste 时会尝试读取 `x-special/gnome-copied-files`，失败后退到 `text/uri-list`；导入 URI list 时额外读取 Dolphin/KDE 使用的 `application/x-kde-cutselection`，首字节为 `1` 时按 cut/move 处理。读取成功后复用现有 async move/copy 队列。内部和导入的剪贴板路径会按原顺序去重，避免同一源路径被重复 Paste。右键菜单刷新和 Paste 入队前都会过滤已经不存在的剪贴板路径，全部失效时清空内部剪贴板并隐藏 Paste。
- 冲突处理：通过 drop、Paste 或内部 transfer 菜单发起的 copy/move/link，如果目标名称已存在，会先要求用户选择 Overwrite、Keep Both、Rename 或 Skip。Apply-to-remaining 只应用于 Skip、Keep Both 和 Overwrite；Rename 始终只处理当前冲突，避免把一个手写目标名复用到不相关的后续冲突。Overwrite 会先把旧目标移动到同目录临时备份；操作失败时尝试恢复旧目标，因此文件和目录覆盖走同一套路径。
- Undo：成功的 copy/link 会在状态栏提供一次性 Undo，执行时删除刚创建的目标；成功的 move 会尝试把目标移回原路径。Overwrite 操作会把被替换的旧目标保留为当前 Undo 条目的临时备份；撤销 copy/link overwrite 时删除新目标并恢复旧目标，撤销 move overwrite 时先把移动来的目标移回原路径再恢复旧目标。新的 Undo 条目替换旧条目时会清理旧 overwrite 备份。Undo 失败时会恢复同一个 Undo 入口以便修正阻塞条件后重试；如果失败结果返回前已经产生了新的 Undo 条目，则保留新的 Undo，不用旧失败结果覆盖当前状态。
- 移到回收站：右键菜单支持单项或多选项移动到 XDG Trash；多选菜单只调用批量 trash 路径，写入 `files/` 和对应 `.trashinfo`。
- 错误汇总：批量移动到回收站会汇总成功数量和失败原因，显示在状态栏。

这些动作通过 `file_actions.rs` 进入 Tokio `spawn_blocking()`，完成后回到 UI 线程刷新当前目录。权限不足时和 transfer 操作走同一套提权确认：普通用户态尝试失败后显示管理员授权提示，用户确认后才调用 polkit helper。正式路径是 system bus D-Bus activation 启动 `fika-privileged-helper --system-bus`，helper 对每个方法调用 polkit authority；开发 checkout 没有安装 system bus service 时，会退回 `pkexec --disable-internal-agent fika-privileged-helper --session-bus ...`。

### Search

当前支持两种搜索：

- 默认搜索只过滤当前目录已加载条目。
- 勾选 `Search subfolders` 后，提交搜索会异步递归扫描当前目录。
- 搜索栏提供 Type、Modified、Size 三个紧凑过滤器；过滤器可在空查询时单独作用于当前目录，也会作用于递归搜索返回的结果。
- 搜索 strip 的布局集中在 `ui/search_panel.slint` 的 `SearchPanel`。搜索过滤状态 helper、递归搜索取消 token 处理和状态栏文案集中在 `src/app/search_ui.rs`；`ui/app.slint` 只保留查询/过滤状态，`main.rs` 只保留后端 callback 转发、异步搜索启动和主栏高度计算。

递归搜索使用独立 `search_generation` 和一个协作取消标记。修改查询、清空查询、按下 Cancel 或切换目录会请求当前搜索尽快中断并使旧搜索结果失效；过期结果和过期进度事件回到 UI 线程后直接丢弃。递归搜索进行时 UI 设置 `search_loading`，主栏空状态显示搜索中而不是误报无匹配；后台任务会周期性回传已扫描目录数和已匹配结果数，状态栏据此显示实时进度。有效结果或取消/切目录会清除 loading 状态；用户主动取消时，状态栏会包含最近一次扫描进度。递归搜索完成后，如果 Type / Modified / Size 过滤器隐藏了部分匹配项，状态栏会明确显示可见数量是过滤后的结果数。递归结果按父目录位置排序，并在每个位置组的第一个 tile 上显示组名；每个结果仍显示父目录位置，便于区分同名文件。

切换 `Search subfolders` 时，如果已有查询，会立即按新模式重新提交搜索。

`FileEntry` 同时保存展示用的 `size` / `modified` 字符串、递归搜索分组用的 `group` / `location`，以及过滤用的 `size_bytes` / `modified_age_days`。这样搜索过滤不依赖格式化字符串解析，递归搜索、本地过滤和大目录虚拟化切片都走同一套可见索引逻辑。

### Thumbnails

缩略图流水线当前覆盖 PNG/JPEG/WebP：

- `FileEntry` 包含 `thumbnail_state` 和 `thumbnail`，失败或未完成时继续显示通用文件图标。
- 当前可见顺序的前若干项会先被调度，符合列优先图标视图的首屏优先策略。
- 后台任务用 `image` crate 解码并缩放到当前 zoom 对应尺寸，回传 RGBA 像素给 UI 线程构造 Slint `Image`。
- 缓存 key 为路径、mtime 和目标尺寸；重复访问或刷新同一目录时可复用。
- 缩略图缓存有固定条目上限，插入新缩略图时刷新顺序并淘汰最旧条目，避免长时间浏览大目录后内存无限增长。

### State Persistence

当前保存到 `$XDG_CONFIG_HOME/fika/settings.tsv` 或 `~/.config/fika/settings.tsv`：

- dark mode。
- sidebar width。
- icon zoom level。
- last opened directory。
- window width / height。

设置在启动时加载；损坏或无法解析的值会回退到默认值。状态在对应 UI 操作发生时保存，目录切换时同步保存 last opened directory。
关闭窗口时也会保存当前窗口尺寸，启动时按最小尺寸约束恢复。

## Planned Subsystems

### Chooser Output Contract

`fika --chooser [START_DIR]` 后续作为 portal backend 的 UI 前端时，stdout 合约固定如下：

- 成功选择：每行输出一个本地绝对路径，UTF-8 文本，末尾带 `\n`。
- 多选结果：按用户选择顺序输出多行。
- portal backend 启动 chooser 时可以 opt-in 请求元数据行：`FIKA_CHOOSER_FILTER\t<index>` 和 `FIKA_CHOOSER_CHOICE\t<id>\t<selected>`。这些行只在 `--chooser-return-filter` / `--chooser-return-choices` 下输出，普通 chooser 调用仍只输出路径。
- 用户取消：不输出路径，进程以非零状态退出。
- 内部错误：stderr 输出人类可读错误，进程以非零状态退出。
- stdout 不输出日志、状态文案或调试信息；调试信息只能走 stderr。

当前 chooser mode 使用双击和底部确认区两种选择语义：双击目录继续进入目录，双击普通文件会输出该文件路径并退出；底部确认区用于目录选择、保存路径选择和多选确认。stdout 合约必须保持稳定；任何调试输出都只能写 stderr。

Chooser 的纯数据逻辑集中在 `src/app/chooser.rs`：portal filter / choice 参数解析、choice 选中项更新、stdout metadata 生成、保存文件名安全校验和“选中目录否则当前目录”的目标目录选择都在该模块测试。`main.rs` 只保留 chooser UI 同步、用户确认流程，以及真正写 stdout 并退出进程的边界。

### Directory Monitoring

目标：

- 当前目录发生文件新增、删除、重命名、metadata 变化时自动刷新。
- 快速连续事件需要 debounce。
- 切换目录时旧 watcher 必须停止或失效。
- 刷新应保留仍然存在且仍然可见的选中项；导航到新目录才重置选择。

当前实现：

- 使用 `notify`。
- 每次 `load_directory()` 会重建当前目录的 non-recursive watcher，并丢弃旧 watcher。
- watcher 忽略 non-mutating access events。
- watcher 事件 debounce 200ms 后异步重读当前目录。
- debounce 后复用 `AsyncEvent::DirectoryLoaded` 回到 UI 线程。
- 复用 `load_generation` 丢弃过期结果。
- `DirectoryLoadResult::preserve_view` 区分导航加载和刷新加载。F5 与 watcher reload 保留当前过滤和选择交集，导航加载重置过滤和选择。

后续：

- 根据文件操作队列减少本进程引起的重复刷新。

### Locale Workaround

Slint `1.16.1` 的文本栈在部分系统 locale 或 CJK 文本路径下会通过 Parley / ICU4X 请求 CJK complex segmentation。`icu_segmenter 2.2.0` 在缺少 `ja` 模型时会记录 `No segmentation model for language: ja` warning，并继续 fallback。项目通过 `[patch.crates-io]` vendor 了 `icu_segmenter 2.2.0` 的最小补丁，只移除这条非致命 warning，不改变分段 fallback 行为。

应用启动时仍在创建 Slint 窗口前强制设置 `LC_ALL` / `LANG` 等变量为 `C.UTF-8`，作为 Slint 1.16.1 下的保守 workaround。升级 Slint 或 ICU4X 后应重新评估并尽量移除该 patch。

### Thumbnail Pipeline

目标：

- 图片/PDF/视频先显示通用图标，再异步替换缩略图。
- 滚动和切目录时不能阻塞 UI。

建议：

- 为 entries 增加稳定 key，例如 path + modified + size。
- 维护缩略图缓存目录。
- 每次目录加载产生 `thumbnail_generation`。
- 只为当前虚拟可视切片及 overscan 调度缩略图；视口或缩放变化导致旧 generation 结果过期时，只清理匹配同一 path+key 的 pending 记录，避免旧任务阻塞后续重新调度。
- 后台按可见项优先生成。
- 结果通过 channel 回 UI。

### File Operations

当前支持：

- New Folder / Rename / Trash / Copy / Move / Link。
- Drop transfer 操作进入 `OperationQueue`，一次执行一个后台任务。
- Copy 和跨文件系统 Move 会按字节汇报进度到状态栏。
- `Cancel Operations` 会清空未开始的队列，并对活动 copy/move 设置取消标志。
- 批量 Trash 会汇总错误。

权限不足的受保护写入当前通过受限 D-Bus helper 执行：普通 GUI 进程记录 `PrivilegedCommand`，用户确认后优先通过 system bus 调用 `org.fika.FileManager1.Privileged`。打包安装时，D-Bus activation 会以 root 启动 `fika-privileged-helper --system-bus`，helper 再调用 polkit authority 对每个方法做授权。GUI 不做 root 写入，也不向 helper 传 shell 命令或任意 argv。

system bus 形态使用 `data/dbus-1/system-services/org.fika.FileManager1.Privileged.service.in` 和 `data/dbus-1/system.d/org.fika.FileManager1.Privileged.conf` 安装激活与总线权限；`data/polkit-1/actions/org.fika.FileManager.policy.in` 定义稳定 action id `org.fika.FileManager.privileged-helper`。每个 D-Bus 方法都调用 `org.freedesktop.PolicyKit1.Authority.CheckAuthorization`，认证 UI 仍由桌面 polkit agent 提供。Polkit authority 不可用、授权检查失败和用户拒绝的诊断都会带 action id 和 policy 文件安装提示，避免把缺策略、无 agent 和用户拒绝混成无上下文错误。没有活跃 scratch 编辑 token 时，helper 空闲一段时间后退出。设置 `FIKA_DEBUG_PRIVILEGE=1` 时，helper 会输出 startup/exit 生命周期摘要，包含 system/session 模式、bus 连接来源、授权主体、空闲时长和活跃 external-edit token 数。

`scripts/install-data.sh` 是当前打包入口：它展开 service / policy 模板中的 `@bindir@`，并安装 system bus service、D-Bus policy、polkit action、interface XML、portal backend service 和 portal descriptor。脚本支持 `DESTDIR`、`PREFIX`、`BINDIR`、`DATADIR` 和 `SYSCONFDIR`，便于本机安装和 distro packaging 共用。`scripts/check-install-data.sh` 会用临时 `DESTDIR` 做非 root 安装自检，验证所有 metadata 文件位置、模板展开结果、root system-bus activation、D-Bus send policy、导出的 privileged methods、polkit 默认授权策略、polkit 认证提示文案、portal backend metadata、`UseIn=fika` 描述符，以及安装产物不含 `@bindir@` / `example.invalid` 这类占位内容。

`scripts/check-runtime-integration.sh` 是安装后的诊断入口。`--metadata-only` 可用于 `DESTDIR` staged package 检查；普通模式会先输出 OS、session、systemd user、xdg-desktop-portal、polkit agent、UDisks2 和 D-Bus/polkit 诊断工具摘要，再检查 Devices 的 UDisks2 ObjectManager 可见性，尝试运行 `fika --diagnose-devices` 打印 Fika 实际侧栏输入模型，输出外部 DnD fallback-removal 的验证命令和必须观察到的 `role=slint-primary` / `validation=external-local-path` 日志字段，并继续检查 helper / portal backend 可执行文件、system/session bus activatable name、已安装 polkit action 和当前 XDP FileChooser backend selection；`--activate-system-helper` 会额外通过 D-Bus introspection 激活并检查 `org.fika.FileManager1.Privileged`，但不会调用 CreateFolder / Transfer 等任何受保护文件操作方法。`--record FILE` 会把 stdout/stderr 连同报告头写入文件，适合在不同发行版和桌面环境上保存可比较的验证记录。

脚本自检只能证明安装产物内容一致，不能替代真实系统验证。发行版包或本机安装后还需要在目标桌面环境中确认：system bus 能激活 `org.fika.FileManager1.Privileged`，polkit agent 能弹出 `org.fika.FileManager.privileged-helper` 的认证，授权后 protected operation 成功返回，拒绝授权时 GUI 显示带 action id 的错误，portal backend 能被 xdg-desktop-portal 枚举并启动 `fika-xdp-filechooser`，并且最高优先级的 `portals.conf` 明确把 `org.freedesktop.impl.portal.FileChooser` 指向 `fika` 或将 `fika` 放入无显式 FileChooser override 的 `default` backend 列表。

`pkexec` 现在只是未安装 system bus service 时的开发 fallback：GUI 启动 `pkexec --disable-internal-agent fika-privileged-helper --session-bus ...`，helper 校验 D-Bus 调用者 uid 与 `PKEXEC_UID` 一致。这个 fallback 不作为正式打包路径。

这个流程不引入用户可见的 `admin://` 协议。Dolphin 的 `admin://` + `kio-fuse` 是为 KIO 生态和外部 POSIX 应用兼容服务的通用桥；Fika 可以做得更窄也更可控：

- GUI 始终是普通用户进程。
- 受限 helper 提供 D-Bus 接口 `org.fika.FileManager1.Privileged`，只暴露固定文件操作：CreateFolder / Rename / Trash / Transfer。
- 正式 helper 由 system bus activation 启动，并在每个 D-Bus 方法做 polkit authority 检查。
- helper 不接受 shell 命令，不接受任意 argv 执行，只接受结构化路径和操作枚举。
- D-Bus 接口草案在 `data/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml`。

外部编辑器写回不需要 `admin://`。优化后的流程是“临时工作副本 + D-Bus 提交”：

1. Fika 检测目标文件不可由当前用户直接写入。
2. 用户选择外部编辑器时，Fika 请求 helper `PrepareExternalEdit(path)`。
3. helper 经 polkit 授权后读取 protected 文件，生成用户可读写的 scratch 文件，例如 `/run/user/$UID/fika-edit/<token>/<name>`，返回 scratch path 和 token。
4. Fika 用普通路径启动 Zed / VS Code / 其他编辑器。外部编辑器完全不提权，也不需要理解虚拟协议。
5. Helper 持有 scratch token 并监听 scratch 文件变更；用户在编辑器里保存时，helper 自动校验并写回 protected 文件。因此 Fika GUI 可以在外部编辑器启动后关闭，写回不依赖 GUI 存活。
6. Fika 在状态栏提供 “Save Back / Discard” 操作作为显式兜底和清理入口；`Save Back` 调用 `CommitExternalEdit(token, scratch_path)`，`DiscardExternalEdit(token)` 清理 scratch。
7. `DiscardExternalEdit(token)` 清理 scratch。

这个模型避免了 Dolphin+kio-fuse 的 FUSE 挂载层，但保留“编辑器 Ctrl+S 后写回”的核心体验。Fika 当前通过 D-Bus `org.freedesktop.systemd1.Manager.StartTransientUnit` 把默认 Open / Open With / custom command 启动出的 child PID 纳入 user transient `.scope`；systemd user D-Bus 不可用时会保留普通 spawn 行为并把非致命诊断返回 UI 状态栏。受保护外部编辑会把 token 和 `.scope` unit 通过 `AssociateExternalEditUnit` 交给 helper。helper 使用传入的 session bus 地址订阅 systemd user unit 的 `ActiveState` 属性变化，unit 结束后做一次最终写回并清理 scratch；如果订阅不可用则退回保守轮询。没有 unit 的 token 会在固定 TTL 后做最终写回并过期。这样 scratch 清理和 helper 退出已经不依赖 GUI 进程；普通非保护 Open/Open With/custom command/Open Terminal Here 的状态栏会显示 transient unit 名称，或显示应用已启动但 systemd scope 不可用的诊断。

当前不引入 FUSE 层。scratch/writeback 已覆盖核心体验：helper 监听 scratch 保存并自动写回、GUI 可关闭、编辑器进程结束后按 systemd unit 做最终写回和清理、无 unit 时有 TTL 兜底，且测试覆盖 commit、discard、多次保存、原文件外部变更拒绝和过期清理。只有当真实外部编辑器工作流证明“普通路径 scratch”不足时，才重新评估 FUSE 或其他透明挂载方案。

### Portal Chooser

目标：

- `fika --chooser` 成为未来 `org.freedesktop.impl.portal.FileChooser` backend 的 UI 前端。
- 后续集成 XDP / `xdg-desktop-portal`，通过 `zbus` 提供 portal backend 原型。

当前实现：

- `fika-xdp-filechooser` 是独立 portal backend binary。
- D-Bus name: `org.freedesktop.impl.portal.desktop.fika`。
- Object path: `/org/freedesktop/portal/desktop`。
- Interface: `org.freedesktop.impl.portal.FileChooser`。
- Fika backend 只实现标准 FileChooser backend，不调用、不包装、也不依赖 GNOME/KDE/COSMIC/GTK portal backend。其它 portal interface 仍由当前桌面配置中的其它 backend 提供。
- `data/xdg-desktop-portal/portals/fika.portal` 只是描述符：`UseIn=fika` 让 Fika 作为独立 backend 被枚举，但不会自动替换当前桌面的 FileChooser。实际是否使用 Fika 由 `xdg-desktop-portal` 的最高优先级 `portals.conf` 决定。
- `docs/examples/fika-portals.conf` 提供手动 opt-in 示例：`[preferred] org.freedesktop.impl.portal.FileChooser=fika`。项目默认不安装这个配置，避免打包后意外接管现有桌面 FileChooser。
- `OpenFile(handle, app_id, parent_window, title, options)` 启动 `fika --chooser [current_folder]`，读取 stdout 的本地绝对路径列表，并返回 `results["uris"] = as`。
- `OpenFile` 支持 `options["directory"]` 和 `options["multiple"]`，分别映射到 `--chooser-directory` 和 `--chooser-multiple`。
- `SaveFile` 支持 `current_folder`、`current_file` 和 `current_name`，通过 `--chooser-save NAME` 在当前目录下选择保存路径。
- `SaveFiles` 支持 `current_folder` 和 `files`，通过 `--chooser-save-files` 先选择目标目录，再按 portal 传入的文件名返回完整目标 URI 列表。
- portal `title` 会传给 chooser 窗口标题，`accept_label` 会传给 chooser 底部确认按钮；portal glob `filters` 会转换成 chooser 底部的过滤按钮，常见 MIME filter（例如 `image/png`、`image/*`、`text/plain`、`text/*`、PDF、JSON、XML 和常见压缩包）会保守转换成扩展名 glob。`current_filter` 会在匹配到可表达的 chooser filter 时选择初始过滤器，用户切换后的当前过滤器会随结果以原始 portal filter 返回。空白 portal filter label 会映射成稳定的 chooser label（例如 `Filter 1`），但结果仍回传原始 portal filter。未知 MIME-only filter 不会显示成空过滤器，因为当前 chooser UI 只能表达 glob 过滤。portal `choices` 会转换成 chooser 底部的选择控件，点击后展开该 choice 的候选菜单，并随结果返回用户当前选择。
- `wayland:` `parent_window` 会通过 `--chooser-parent-window` 传给 `fika --chooser` 并保存在 chooser 状态中；空值、格式错误或未知 scheme 会被 backend 丢弃。设置 `FIKA_DEBUG_PORTAL=1` 时，backend 会打印 `parent_window` 解析结果，chooser 进程也会打印收到的 handle。当前 Slint 集成尚未把已保存 Wayland handle 绑定为原生 transient parent，因此两侧诊断都会显式报告 `parent_binding=metadata-only`、`parent_binding_reason=slint-parent-token-binding-unavailable` 和 `native_transient=false`。
- 设置 `FIKA_DEBUG_PORTAL=1` 时，backend 还会为每个 OpenFile / SaveFile / SaveFiles 请求打印一行请求摘要，包含 request handle、起始目录、选择/保存模式、portal/chooser filter 数量、MIME 转换 filter 数量、隐藏的未知 filter 数量、初始 filter、parent-window 转发状态、parent binding 状态，并继续显式标记 `native_transient=false`。chooser future 结束时还会记录收尾原因：选择成功、用户取消、空输出、portal request Close、Close stream 结束或异常失败，方便验证 filter 映射、取消清理和子进程生命周期。
- 返回 URI 统一为 `file://`，路径中的空格和非 ASCII 字节按百分号编码；backend 保留 chooser 输出的所选路径本身，不在返回前把符号链接解析成目标路径。
- 用户关闭 chooser 时，`fika --chooser` 以专用取消码退出，backend 返回 response `1`；chooser 成功退出但无路径输出也按取消处理。其它非零退出会返回 D-Bus error，并带上 exit status 和 stderr，避免把崩溃或启动后异常静默伪装成用户取消。
- backend 在每个 FileChooser 请求期间订阅对应 request handle 上的 `org.freedesktop.impl.portal.Request.Close` signal。Close 先到时 backend 返回 response `1`，并 drop 掉正在等待的 chooser 进程；`fika --chooser` 同时以 `kill_on_drop` 启动，所以 request Close、backend future 被取消或连接断开时，未完成的 chooser 子进程都会随 drop 被终止，避免留下孤儿选择器窗口。
- `options["current_folder"]` 会作为 chooser 起始目录。
- `data/dbus-1/services/org.freedesktop.impl.portal.desktop.fika.service.in` 提供 D-Bus activation 模板。
- `data/xdg-desktop-portal/portals/fika.portal` 提供 xdg-desktop-portal backend 描述文件；active backend selection 由 `portals.conf` 单独控制，并由 `scripts/check-runtime-integration.sh` 的普通模式报告。

后续：

- 将已保存的 Wayland `parent_window` 接入具体窗口后端，处理原生 transient 关系。

## Engineering Rules

- UI 主线程只做状态更新和轻量计算。
- 后台任务不能直接访问 Slint UI 对象。
- 跨线程数据使用 owned Rust 类型；进入 UI 前再转换成 Slint 类型。
- 每类异步任务都要有 generation 或 cancellation 机制，并通过统一 `AsyncEvent` 回到 UI 线程。
- 新功能优先补 focused tests；UI 行为至少通过 `cargo check` 覆盖 Slint 编译。
- 避免无关重构，按 TODO 阶段逐项推进。
