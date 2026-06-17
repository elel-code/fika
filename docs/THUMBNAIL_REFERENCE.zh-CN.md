> 本文是 [THUMBNAIL_REFERENCE.md](THUMBNAIL_REFERENCE.md) 的简体中文翻译。

# 缩略图参考

本文档记录 Fika 缩略图管线的 Dolphin 和 freedesktop.org 参考。缩略图工作应先进入 core 调度/缓存代码；GPUI 应仅渲染可见条目的已解析图像路径。

## Dolphin 源码

- `../dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`
  - 与基础文件 model 分离更新昂贵的条目 role。按可见索引优先调度预览/缩略图工作，然后扩展到其余 model。
  - 当目录、图标大小或可见范围变化时取消过时的预览 job。
- `../dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`
  - 将预览 role 状态排除在视图 widget 身份之外。跟踪请求 generation 防止过期结果修改当前 model。
- `../dolphin/src/kitemviews/kfileitemlistwidget.cpp`
  - 渲染已解析的预览像素图或普通文件图标。widget 消费 model role，不拥有缩略图生成。

## Freedesktop 缩略图规范

- 缓存根：`$XDG_CACHE_HOME/thumbnails/` 或 `~/.cache/thumbnails/`。
- 普通缩略图位于 `normal/`，最大 128x128。大型缩略图位于 `large/`，最大 256x256。
- 失败标记位于 `fail/gnome-thumbnail-factory/`。
- 缓存文件名：`md5(uri).png`，uri 为规范文件 URI。
- 失败标记使后续扫描跳过同一文件，直到 metadata 变化使请求失效。

## Fika 映射

- `src/core/thumbnails.rs`
  - 使用百分号编码从绝对路径构建 freedesktop 文件 URI。计算 MD5 缓存键。
  - 从缩略图缓存根解析 `normal/`、`large/` 和失败缓存路径。
  - 先检查 `normal/` 再 `large/`。缓存命中仅当 PNG `tEXt` metadata 具有预期的 `Thumb::URI` 时信任；基于路径的查找还需 `Thumb::MTime` 匹配源文件 mtime。
  - 在 `fail/gnome-thumbnail-factory/` 下记录失败标记，带上 `Thumb::URI` 和 `Thumb::MTime`。
  - 缓存未命中时读取 freedesktop thumbnailer `.desktop` 文件，匹配 `MimeType=`，展开 `Exec=` 字段代码，运行安装的 thumbnailer。无注册条目匹配时回退到内置命令列表。
- `src/core/model.rs`：`ModelEntry` 携带 `thumbnail_path: Option<PathBuf>` 作为 pane-local 预览 role。
- `src/main.rs` 和 `src/ui/file_grid/snapshot.rs`：Pane snapshots 将普通文件缩略图 role 复制到 `VisibleItemSnapshot::thumbnail_path`。可见普通文件无缩略图角色时通过 `ThumbnailRequestQueue` 排队。
- `src/core/thumbnails/scheduler.rs`：拥有 `ThumbnailScheduler` 等 UI-neutral 调度支持，包含请求队列、可见集合状态、活动批次取消、Dolphin 风格 read-ahead 索引计算等。可见优先调度，最多四个并行请求。

## 剩余工作

- 添加宿主安装 thumbnailer 的真实系统端到端覆盖；确定性本地 thumbnailer 路径和失败标记行为已在 core 测试中覆盖。
