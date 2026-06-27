# 性能对齐原则

Fika 的性能工作以 Dolphin 为第一参考。本机 Dolphin 源码位于
`/home/yk/Code/dolphin`，它是文件管理器性能架构、行为保持型优化和回归 gate
的第一参考。

## 硬规则

每一次性能优化，或任何会影响性能边界的调整，都必须在变更完成前给出明确的
Dolphin reference。

有效 reference 必须包含：

- 本地 Dolphin 文件路径，以及相关 class、function 或数据流；
- Dolphin 中被复制、改写或明确不复制的行为/性能边界；
- Fika 中对应的模块或代码路径；
- 如果 Fika 因 `winit/wgpu` shell 需要偏离 Dolphin，要写明原因；
- 本次变更使用的验证命令、日志、benchmark 或 smoke gate。

如果 Dolphin 没有直接对应实现，必须明确写出“无直接 Dolphin reference”，并给出
最接近的 Dolphin reference 和只能部分参考的原因。

## Reference 格式

性能说明、commit message、PR 描述或实现总结里使用这个结构：

```text
Dolphin reference:
- Source: /home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating
- Dolphin boundary: 可见项优先于后台 role work。
- Fika mapping: src/shell/... 或 src/core/...
- Divergence: ...
- Verification: ...
```

## 常用参考入口

- item model、refresh、filtering、sorting 和 role storage：
  `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodel.cpp`、
  `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodel.h`、
  `/home/yk/Code/dolphin/src/kitemviews/private/kfileitemmodelsortalgorithm.h`、
  `/home/yk/Code/dolphin/src/kitemviews/private/kfileitemmodelfilter.cpp`。
- metadata role、preview scheduling、visible index priority、异步 role 解析、
  directory size counting 和 MIME/Baloo role 更新：
  `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`、
  `/home/yk/Code/dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`、
  `/home/yk/Code/dolphin/src/kitemviews/private/kdirectorycontentscounter.cpp`、
  `/home/yk/Code/dolphin/src/kitemviews/private/kbaloorolesprovider.cpp`。
- 可见项 virtualization、widget reuse、scroll/layout 边界、column sizing、
  rubber-band 和 item view geometry：
  `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.cpp`、
  `/home/yk/Code/dolphin/src/kitemviews/kitemlistview.h`、
  `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp`、
  `/home/yk/Code/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp`。
- item painting、icon/pixmap handling、text caching、role text layout 和
  selection/hover visuals：
  `/home/yk/Code/dolphin/src/kitemviews/kitemlistwidget.cpp`、
  `/home/yk/Code/dolphin/src/kitemviews/kstandarditemlistwidget.cpp`、
  `/home/yk/Code/dolphin/src/views/dolphinfileitemlistwidget.cpp`。
- Dolphin view integration 和 mode-specific behavior：
  `/home/yk/Code/dolphin/src/views/dolphinview.cpp`、
  `/home/yk/Code/dolphin/src/views/dolphinitemlistview.cpp`、
  `/home/yk/Code/dolphin/src/views/viewmodecontroller.cpp`、
  `/home/yk/Code/dolphin/src/views/viewproperties.cpp`。
- Places 行为和设备侧边栏集成：
  `/home/yk/Code/dolphin/src/panels/places/placespanel.cpp`、
  `/home/yk/Code/dolphin/src/dolphinplacesmodelsingleton.cpp`。

## Review 检查项

- 变更是否包含本地 Dolphin 文件路径和 symbol？
- 实现是否保持 Dolphin 的 model data、role resolution、view layout、painting
  分层边界；如果没有，是否写明偏离原因？
- 验证是否覆盖 reference 对应的用户可见路径，例如 scrolling、sorting、refresh、
  thumbnails、Places 或 DnD？
- 新增 cache、queue 或 retained resource 是否有边界和失效策略，并与 Dolphin
  reference 或明确的 Fika 边界一致？
- 如果声称性能提升，是否附上 benchmark、smoke 或日志结果？
