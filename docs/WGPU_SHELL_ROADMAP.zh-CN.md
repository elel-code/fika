> 本文是 [WGPU_SHELL_ROADMAP.md](WGPU_SHELL_ROADMAP.md) 的简体中文版本。

# Fika winit/wgpu Shell 路线图

本文档是 Fika 当前活跃 UI 方向。GPUI 应用保留为兼容实现和行为基线，新 Linux
专用 shell 经过验证后再提升为默认。新的 UI 架构工作应面向 Fika 专用的
`winit + wgpu` runtime，而不是继续扩展 GPUI element-tree 迁移。

目标不是采用另一个通用 widget toolkit。Fika 应借用 iced/COSMIC 生态正在验证的
Linux windowing 栈，然后围绕自己的 retained 数据建立窄口径文件管理器 renderer、
scene model、input router 和 cache policy。

## 决策

Fika 是 Linux 独占应用。这样就失去了在热路径保留跨平台 UI framework 的主要理由。
文件视图、Places 侧栏、selection、hover、drag/drop routing、zoom、thumbnail、
icon/text cache 都足够专用；继续通过 GPUI 模仿 Dolphin 的成本已经高于拥有一套
专用 runtime。

新的 shell 应使用：

- iced/COSMIC 栈中的 `winit`，而不是随意选择上游 windowing 依赖。本地 COSMIC
  参考通过 `pop-os/winit` tag `cosmic-0.14` 解析 `winit`。
- 官方 crates.io `wgpu` 作为 render backend。COSMIC 解析到的 `wgpu` 版本可作为兼容性
  参考，但 Fika 应直接依赖上游 `wgpu`，而不是继承 framework 或 editor fork。
- 现有 Fika core modules：listing、operations、thumbnails、MIME、Places、
  devices、trash、portal 和 privileged-helper 行为。
- 现有 retained file-grid 和 Places model 作为迁移输入，而不是作为 GPUI 专属设计约束。

不要把主 shell 做成 libcosmic/iced widget tree。它们对 Linux windowing、Wayland、
DnD、clipboard、text 和 theme integration 很有参考价值，但 Fika 的主 UI 应是专用
文件管理器 surface。

选择 iced/COSMIC 这条 `winit` 路径是有意为之。对 Fika 的目标环境来说，它比单独跟随
上游 `winit` 更有价值，因为它被真实 Linux 桌面应用持续验证，并承载 iced/libcosmic
runtime 所需的集成假设：Wayland window 和 popup 行为、clipboard 和 drag/drop 管线、
raw-window-handle/wgpu surface 集成，以及桌面会话边界情况。Fika 应复用这层经过验证的
windowing layer，同时避开其上方的通用 widget tree。

## 为什么它有机会超过 GPUI 和 cosmic-files

Fika 的问题比通用桌面 UI framework 更窄：

- 文件网格可以按可见 slot 做少量 GPU batch，而不是构建大量独立 row/item widget。
- Layout、hit testing、paint command generation 和 input routing 可以共享同一份
  retained geometry projection。
- Scroll 和 zoom 可以先更新 viewport state，再把 thumbnail、icon、text-shape 和
  glyph 等昂贵工作按 visible-first 预算推进。
- MIME/theme icon、thumbnail 和 glyph atlas 可以按文件管理器语义键控，而不是按
  widget/image handle 生命周期键控。
- Places、Compact、Icons 和 Details 可以共享 slot、dirty-state、cache 和 hit-test
  primitive。
- Linux-only clipboard、URI-list、Wayland DnD、portal、GIO/GVfs 和 XDG 行为可以保持
  窄实现并直接测试。

代价是显式所有权。Fika 必须拥有 frame scheduling、GPU resources、text cache policy、
focus、IME 边界、popup、clipboard、DnD 和 accessibility 规划。这个代价可以接受，
因为这些部分都可以围绕 Fika 的文件管理器工作流实现，而不需要服务通用 toolkit。

## 架构目标

```text
core model -> retained UI model -> scene projection -> GPU command batches
          \-> input/hit-test routing -> file-manager actions
```

Core 保持 UI-neutral。它不能依赖 `winit`、`wgpu`、window handle 或 renderer resource。

Shell 拥有：

- Window lifecycle 和 event-loop integration。
- Pane、Places、overlay、popup 和 chooser scene state。
- File slot、Details row、Places row、scrollbar、rubber-band selection、splitter
  和 context target 的 retained geometry。
- Hit testing 和 pointer/keyboard routing。
- Draw command generation、batching、clipping、transform 和 invalidation。
- Icon、thumbnail、mask 和 UI asset 的 texture atlas。
- 通过成熟 text crates 集成 text shaping/raster cache。不要自己实现 Unicode shaping、
  bidi、fallback 或 IME 文本编辑。
- 用 shell-native frame、cache、atlas、batch 和 hit-test counters 替代 GPUI
  renderer-policy logs。

## 迁移阶段

### Phase 0：Shell Spike

新增独立实验二进制，暂定 `fika-wgpu`，不删除 GPUI binary。它应打开窗口、初始化
`wgpu`、驱动现有 directory listing model，并用最小 Compact view 渲染 `/etc`。

当前 checkpoint：

- `src/bin/fika-wgpu.rs` 已作为独立 binary 存在。
- 接受可选 path 参数，默认使用当前目录。
- 通过 `fika_core::read_entries_sync` 读取目录 entries。
- 通过现有 `IconsLayout` retained geometry 投影条目；Compact 由 shell-owned projection
  按每一列可见名称中的最长项决定列宽。
- 渲染顶部 path bar、可见 item 背景、active XDG icon theme 可解析时的真实文件/文件夹
  theme icon、miss 时的 fallback 文件/文件夹 icon 形状，以及真实可见文件名。文字通过
  `cosmic-text` 做 shaping/rasterization，再上传临时 per-frame RGBA atlas，由一个
  `wgpu` textured quad batch 绘制。wgpu shell 现在会在 layout/rasterization 前应用
  window scale factor，所以默认 Icons 图标仍是 48 逻辑 px（例如 1.5x scale 下为 72
  物理 px），14px/18px baseline text metric 也更接近当前 GPUI Fika 的视觉尺寸。
- 为可见文件名/path text 保留 bounded persistent label raster cache，按 text、size 和
  color 键控。per-frame atlas 现在打包 cached label raster，不再每次 redraw 都重新
  shape/rasterize 所有可见 label。
- 从 XDG、GTK 和 KDE theme settings 解析 MIME/theme icon；PNG/WebP/JPEG/BMP/GIF/ICO
  通过 `image` 光栅化，SVG 通过 `usvg/resvg` 光栅化；可见 icon 打包到 per-frame RGBA
  icon atlas，并按 theme icon file path 和 size 保留 bounded persistent icon raster cache。
- 鼠标滚轮更新 retained viewport state。文件内容区现在会预留并绘制 shell-owned
  item-view scrollbar：Icons/Details 使用右侧竖向 track，Compact 使用底部横向 track。
  frame log 会输出 `content_scrollbar=0|1`；scrollbar drag/click 交互仍是后续工作。
- 实验 binary 支持 `--view icons|compact|details`。Icons 仍是默认 baseline；
  Compact 使用 core `CompactLayout`；Details 现在有 shell-owned row projection、
  固定 header，以及 Name/Size/Modified 三列。Icons 和 Compact 现在只在 hover 或 selection
  时绘制 item highlight/background，普通未悬停项不再像被预高亮。Compact label 左对齐，
  每个 Compact item 的高亮宽度按该项自己的文本宽度收缩，而不是填满整列。同一组模式也可用 top-bar `Icons /
  Compact / Details` 按钮、`1/2/3`、`Ctrl/Meta+1/2/3` 或 fallback `F1/F2/F3`
  在运行时切换；`--auto-cycle-views` 会每秒自动切换一次，用于在没有输入的情况下
  调试 compositor/render。切换时会 clamp 当前 scroll axis、清理 transient
  rubber-band state、从 retained geometry 刷新 hover、更新窗口标题，立即输出
  `[fika-wgpu] view-mode=...` 日志，并保持一个短 redraw burst，直到切换后的 scene
  被 present。Top bar active segment 和全宽 mode color stripe 让当前 projection
  直接可见，即使目录内容在不同 mode 下看起来接近。
- Pointer move/leave 和左键点击现在通过 shell-owned retained hit testing 路由。Spike
  按 model index 跟踪 hovered item、单选、Ctrl/Meta toggle selection 和 Shift range
  selection，并从同一 slot projection 绘制 hover/selection 状态。
- 右键 context targeting 现在也通过 shell-owned retained hit testing 路由。右键未选中的
  item 会先把 selection 同步到该 item；右键已选中的 item 会保留 multi-selection，同时把
  focus 移到点击的 model index；右键 content 空白区域会记录 blank directory target，
  且不会启动 rubber-band selection。shell 现在保存轻量 context target snapshot，为
  item/blank target 打开 clamp 后的 shell-owned context menu overlay，更新 row hover，
  支持 Esc 或外部点击关闭，菜单 surface 已从早期半透明深色改为不透明浅色，并将 directory item 的 Open、file item 的 Open（通过 GIO
  default-application URI launch）、file item 的最小 shell-owned Open With chooser（使用
  core `MimeApplicationCache` 和 systemd-user launch plan）、item Copy Location（通过
  shell-owned Wayland text clipboard provider）、item Copy/Cut（通过同一 provider 写入 Fika URI-list text encoding）
  以及 blank menu Paste（读取 Wayland text clipboard，解码 Fika/GNOME URI-list text 或
  plain text，调用本地 core transfer/text-paste helper，reload 目录，并在 Cut 成功后清空
  clipboard）和 Refresh、
  Select All 分派到现有 shell navigation/reload/selection path；其余 pending action 会记录
  日志，并输出 context target/menu counters。Properties 现在会为 item 和 blank-directory
  target 打开轻量 shell-owned metadata overlay。Blank-menu Create New 现在会打开
  shell-owned modal，支持 folder/file 选择、plain text name capture、校验、真实
  `create_dir` / `create_new` 文件系统动作、reload，并选中新建条目。Item Rename 现在会
  打开最小 shell-owned modal，支持 plain text name capture、校验、真实 `rename`、
  reload，并选中重命名后的条目。Directory item 和 blank-directory context menu 现在
  支持 Add to Places，会写入 Fika `places.xbel`、重建 sidebar projection，并持久化
  primary place order。Move to Trash 现在会把 context target 解析为点击条目或当前
  multi-selection，显式拒绝 remote paths，调用 core XDG trash handling，reload
  pane，并清理 stale context state。Trash view context menu 现在会通过 core
  `TrashViewOperation` path 分派 Restore From Trash、Delete Permanently 和 Empty
  Trash，随后 reload Trash view 并清理 stale context/selection state。Restore conflict
  现在会打开 shell-owned confirmation overlay；Replace 会用 replace policy 重新通过 core
  `TrashViewOperation` restore，然后 reload Trash。Cut 和 Paste 会显式拒绝 remote paths。
  Open With default-application selection、多 MIME `text/uri-list` clipboard
  export/import、更完整 multi-conflict handling、undo、更完整 properties、完整 inline
  rename、完整 Create New 子菜单/模板和 new-pane dispatch 仍留到 Phase 4。
- 第一版 shell-owned Places 侧栏现在作为圆角浅色 panel 绘制，顶部与右侧 pane-local
  顶栏下方的 content/body 区起点对齐。它通过公开 core API 构建 Home、已存在的 XDG directories、Trash、Fika user places、primary
  `places-order.xml`、Network root、network bookmarks 和 Root，保留 row geometry，用最长路径前缀决定 active place，Places
  hover 与 item hover 分离，拥有独立 sidebar scroll offset、clipped row rendering 和窄
  scrollbar thumb，active/hover row 会绘制圆角背景，并将左键 place navigation 分派到与文件视图相同的
  `load_path`/history path。Places 右键现在会创建 shell-owned place context target，
  并打开最小 context menu，分派 Open、Copy Location、Properties，以及 editable user
  places 的 Remove。Remove 会写回 Fika `places.xbel`，裁剪对应 place-order 条目，
  reload sidebar projection，并清理 stale place context state。动态 devices、更完整
  Places actions（sidebar add/edit/hide 和 Trash actions）、DnD/drop targets 和 resize
  仍留到 Phase 4。
- 空白区域左键拖动现在通过同一 retained Icons geometry 执行 rubber-band selection。
  普通拖动替换 selection，Shift 追加，Ctrl/Meta 会相对按下时的 base selection 做
  toggle，并用 clipped GPU overlay 绘制框选矩形。
- Keyboard navigation 现在通过同一 retained selection state 处理 Arrow、Home/End 和
  Page Up/Down。Shift 会扩展当前 range，focus item 会滚入视口。`Ctrl/Meta+A`
  会全选当前目录 entries，`Esc` 会清空 selection 并取消任何 transient rubber-band 操作。
- 目录激活现在也留在 shell-owned input path 内：Enter 打开当前 focus/selected
  目录，双击通过 retained hit testing 解析并打开目录，Backspace 或 Alt+Up 加载父目录。
  Top bar 也提供 shell-owned Back/Forward/Up 控制，Alt+Left 和 Alt+Right 映射到同一
  history stack。Top-bar Reload 控制以及 `F5` / `Ctrl/Meta+R` 会刷新当前目录，不写入
  history，并在 entry 仍存在时按名称保留 selection/focus。加载新 path 复用
  `read_entries_sync`，普通导航会写入有界 back stack，
  且只在成功的新导航后清空 forward history；随后重置 scroll/selection/rubber-band
  transient state，从 retained geometry 刷新 hover，更新窗口标题，并通过与 view
  switching 相同的 redraw burst present 新 scene。
- 初版 view zoom 也由 shell-owned retained geometry 驱动。`Ctrl/Meta + +`、
  `Ctrl/Meta + -` 和 `Ctrl/Meta + 0` 会调整或重置有界 zoom step。Icons 和 Compact
  会更新 item/icon/text slot metrics，Details 会更新 row 和 icon metrics，scroll 会被
  clamp，focus item 会保持可见，icon resolver 现在会按 zoom 后的 slot size 请求 raster。
  glyph-level text sizing 和长期 glyph atlas policy 仍留到 Phase 2。
- shell 现在会在 compositor 提供时优先选择 non-sRGB surface format，因为 UI 颜色以及
  icon/text atlas 已按显示字节空间生成；这避免了之前 sRGB target 二次提亮造成的灰浅感。
- 底部最小 shell-owned status bar 已开始绘制在 content pane 内，不再跨过 Places
  sidebar。它汇总 entry、directory、file、selection、visible item、view mode 和 zoom 状态，
  会预留 content viewport 高度，并从 item hit testing 中排除。
- 最小 shell-owned filter bar 已可用，快捷键为 `Ctrl/Meta+F`。字符输入会更新 retained
  plain-text name filter，Backspace 编辑 pattern，Enter 保留 pattern/filter 结果但停止继续吃文本，
  Esc 清空并关闭 filter。Layout、hit testing、hover、selection、select-all 和 keyboard
  navigation 都会通过 filtered model-index projection 路由。完整 IME/caret/selection
  文本编辑边界仍留到 Phase 4。
- 最小 shell-owned pane-local location edit mode 已可用，可通过 `Ctrl/Meta+L`、`Ctrl/Meta+D`、
  `F6` 或点击顶部 path bar 激活。它复用 core `resolve_location_input` 和
  `complete_location_input`：首次输入会替换当前 path draft，Backspace 编辑 draft，Tab
  补全 filesystem path，Enter 通过 retained navigation/history path 提交，Esc 取消。
  caret movement、selection editing 和 IME 仍留到 Phase 4 文本边界。
- Dotfile 可见性现在也由 shell-owned retained projection 管理。默认不显示 hidden
  entries；`Ctrl/Meta+H` 或 top-bar `Hidden` toggle 会显示它们。切换可见性时 selection
  会通过同一 projection 保留或裁剪。
- `[fika-wgpu]` 日志包含 view mode、window/UI scale、path、entry count、visible item count、quad count、draw
  batch count、Places count/hover/change/scroll counters、selected count、hovered item index、active rubber-band state、
  context target kind、context menu state、properties overlay state、hit-test/selection/keyboard navigation/rubber-band/view-switch/path-change/open/copy-location/file-clipboard/paste/reload/location/filter/hidden counters、zoom percent
  和 zoom-change counters、icon count、icon cache hit/miss count、icon cache bytes、icon atlas bytes、
  icon resolve/raster time、text label count、text cache hit/miss count、text cache bytes、text atlas bytes、
  render reason、layout time、text raster time、render time 和 `scroll_x` / `scroll_y`
  offsets。
- 本地目标 desktop session 中，`timeout 4s target/debug/fika-wgpu --view
  icons|compact|details /etc` smoke 已到达 `shell-ready`，并在 Vulkan 上输出
  `surface-format=Rgba8Unorm srgb=0`、`frame=1` 以及真实 icon/text atlas counters。
  自动 smoke 的 timeout exit 符合预期。

Phase 0 仍待完成：glyph-level cache/atlas retention、手动打开/关闭/交互 smoke
证据、DnD targeting，以及初始默认使用 Compact 还是 Icons 的最终选择。

验收：

- [x] 不改变现有 GPUI app 的构建。
- [~] 能在目标 Linux desktop session 中打开窗口，并在自动 timeout smoke 中到达首个
  rendered frame。手动关闭和交互 smoke 仍待完成。
- [~] 能渲染可见目录 slot、可用时的真实 theme icons、miss 时的 fallback icons，以及
  通过 texture atlas 绘制的真实文件名。
- [~] 基本 pointer hover、鼠标 selection、keyboard navigation、全选/清空快捷键、右键
  context target selection 和 rubber-band selection 已通过 retained geometry 路由。DnD
  targeting 仍待完成。
- [~] 输出 frame timing、visible range、draw-command counters、临时 text-atlas counters
  和 icon/text atlas counters、retained hit-test counters、bounded icon/label-cache
  counters；glyph-level 以及 thumbnail atlas counters 会在对应 resource retention 层接入后开始。

### Phase 1：文件视图核心对齐

从现有 Fika model 实现 Compact、Icons 和 Details scene projection。

验收：

- [~] `/etc` 已可通过 `--view` 在 Compact、Icons 和 Details 中渲染；`~/Downloads`
  和手动交互 smoke 仍待完成。
- [~] 初版 projection 的 scroll、hover、keyboard navigation、runtime mode switching、
  projection zoom、reload、location editing、filtering、hidden-file visibility、selection 和全选/清空快捷键走
  retained geometry。glyph-level text zoom policy 仍待完成。
- [~] Icons、Compact 和 Details 的 layout/hit-test/paint 已共享同一 shell layout
  abstraction。
- Steady render pass 不执行同步 theme scan、MIME magic read、thumbnail decode 或
  text shaping。

### Phase 2：Cache 和 Text Pipeline

把 Phase 0 的初版 icon atlas 提升为预算化 semantic icon work，然后加入 thumbnail
texture retention、text shaping cache、glyph atlas policy 和 eviction telemetry。

验收：

- Zoom 不会让已加载的同语义 icon 失效，除非 size/DPI 需要新的 raster。
- Cold glyph/icon work 按 visible-first 预算推进。
- 已缓存 thumbnail 在首个合格 frame 显示。
- Cache logs 显示 hit/miss/evict/bytes 和每帧 compute time。

### Phase 3：交互和 DnD

把剩余 pointer routing、context target selection、directory hover、Places hover 和
drag/drop target lookup 移到 shell-owned hit testing。

验收：

- [~] Pane item/blank 右键 context target selection 以及第一版 shell-owned context menu
  overlay 已进入 file view。Places row hover、左键 navigation、右键 context targets
  和最小 Open/Copy Location/Properties/Remove place menu 已由 shell-owned hit testing
  处理。Device/place edit/hide/add action dispatch 和 DnD target lookup 仍待完成。
- Pane item 到 pane directory、pane item 到 Places、Places 到 pane、external path drop
  和 URI-list clipboard path 由自动或隔离 smoke 覆盖。
- DnD hover 不依赖 per-row 或 per-item widget callback。
- Drag cursor/action state 遵循 Copy/Move/Link 语义。

### Phase 4：Chrome、Overlays 和 Chooser

实现可用 shell 所需外围 UI：Places、toolbar、location bar、filter bar、status bar、
context menus、dialogs 和 chooser mode。

当前 checkpoint：第一批 chrome slice 包含顶部与 pane content/body 区对齐的圆角 shell-owned Places panel、
左键 navigation 和最小 Open/Copy Location/Properties/Remove row context menu、pane-local 底部 status bar、
`Ctrl/Meta+F` 最小 filter bar、
`Ctrl/Meta+L`/`Ctrl/Meta+D`/`F6` pane-local 最小 location edit mode，以及用于 file-view
item/blank 右键的不透明浅色 context menu overlay。Properties 会为 item 和 blank-directory
targets 打开最小 metadata overlay。Create New 会为 blank-directory targets 打开最小
shell-owned modal，并执行真实 folder/file 创建、reload 和选中新建条目。Rename 会为
item targets 打开最小 shell-owned modal，并执行真实 filesystem rename、reload 和选中
重命名后的条目。Move to Trash 会通过 core trash operations 处理 item 或 selected item
targets，并在执行文件系统修改前拒绝 remote paths。Filter、location、create-name 和
rename-name 文本编辑暂时保持窄实现，完整 IME/caret/selection 文本边界仍待迁移；
context menu dispatch 当前覆盖 Open directory、Refresh、Select All、Properties、最小
Create New、最小 Rename、最小 Move to Trash、Trash view Restore/Delete Permanently/Empty
Trash、Copy/Cut/Copy Location 和 Paste；最小 Places row Open/Copy
Location/Properties/Remove menu 也已接入；更完整 Places actions/devices/DnD、更完整
Trash conflict handling、undo、更完整 properties、完整 inline rename、完整 Create New
子菜单/模板、Open With default-app selection 和 new-pane actions 仍待完成。

验收：

- 常见文件管理器工作流不需要启动 GPUI shell。
- Rename、location、filter 和 application search 的文本编辑边界有明确的
  IME/caret/selection 覆盖。
- Portal file chooser 输出保持与现有后端兼容。

### Phase 5：默认提升

只有在同场景证据证明行为对齐，并且 frame cost 比 GPUI Fika 和相关 cosmic-files
基线更好或更可预测后，才能把新 shell 提升为默认。

验收：

- 提升窗口期内 GPUI 保留为 fallback。
- `/etc`、`~/Downloads`、大型本地目录、混合 thumbnail 目录、removable devices、
  trash 和 network roots 有 smoke 覆盖。
- 性能门覆盖 frame build time、GPU submission count、draw batches、texture bytes、
  glyph/icon/thumbnail cache behavior 和 input latency。

## 文档策略

GPUI retained-renderer 文档现在是历史证据和迁移输入，不再是活跃架构目标。

保留：

- Dolphin 行为参考。
- Core/system integration 参考。
- 能提供基线数字或行为覆盖的 GPUI performance evidence。

删除或重写：

- 唯一目的为从旧 UI 迁移到 GPUI 的已完成计划。
- 把“继续 GPUI retained migration”描述为活跃未来方向的文档。
- 当 evidence 已汇总到本路线图或 shell 专项实现笔记后，删除重复 TODO slice。
