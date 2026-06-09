# Fika Reference Index

本文档是当前 GPUI package 的参考索引。实现前先查 Dolphin 源码和本仓库 core 边界，不使用记忆或猜测替代源码。

## Primary Reference: Dolphin

本地 Dolphin 源码在 `../dolphin`。

### Directory Load and Refresh

- `../dolphin/src/views/dolphinview.cpp:2337`
  - `DolphinView::loadDirectory(const QUrl &url, bool reload)`
  - `reload == true` calls `m_model->refreshDirectory(url)`
  - `reload == false` calls `m_model->loadDirectory(url)`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:349`
  - `KFileItemModel::loadDirectory()` calls `m_dirLister->openUrl(url)`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:354`
  - `KFileItemModel::refreshDirectory()` calls `m_dirLister->openUrl(url, KDirLister::Reload)`
  - emits `directoryRefreshing()`

### KDirLister to Model Signals

- `../dolphin/src/kitemviews/kfileitemmodel.cpp:300`
  - `itemsAdded -> slotItemsAdded`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:301`
  - `itemsDeleted -> slotItemsDeleted`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:302`
  - `refreshItems -> slotRefreshItems`
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:308`
  - `listingDirCompleted -> slotCompleted`

### Model Slots

- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1359`
  - `slotCompleted()`
  - dispatches pending inserted items, expands pending directories, emits loading completed
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1399`
  - `slotItemsAdded()`
  - creates item data, handles filters, queues pending insert, emits changed parent directories
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1506`
  - `slotItemsDeleted()`
  - detects current directory removal, removes item ranges, updates filtered items
- `../dolphin/src/kitemviews/kfileitemmodel.cpp:1577`
  - `slotRefreshItems()`
  - updates old/new item pairs, preserves expansion metadata, emits changed item ranges

### View Consumption of Model Signals

- `../dolphin/src/kitemviews/kitemlistview.cpp:1812`
  - `KItemListView::setModel()`
  - connects model signals to `slotItemsChanged`, `slotItemsInserted`, `slotItemsRemoved`,
    `slotItemsMoved`, group/sort changes

### Current Directory Removed

- `../dolphin/src/dolphinviewcontainer.cpp:1088`
  - `DolphinViewContainer::slotCurrentDirectoryRemoved()`
  - local path moves to nearest existing ancestor and shows warning

## Current Fika Concepts

| Dolphin concept | Fika module |
| --- | --- |
| `DolphinView` | `src/main.rs` GPUI pane shell |
| `KDirLister` | `src/core/directory.rs` |
| `KFileItemModel` | `src/core/model.rs` |
| `KItemListView` | layout in `src/core/view.rs`, GPUI rendering in `src/main.rs` |
| pane identity | `src/core/pane.rs` |
| file operation primitives | `src/core/file_ops.rs` |
| privileged operation API | `src/core/privilege.rs` |
| portal FileChooser backend | `src/bin/fika-xdp-filechooser.rs` |
| system-bus helper | `src/bin/fika-privileged-helper.rs` |

## Cargo Boundaries

- Root `Cargo.toml` is a single Cargo package.
- `src/lib.rs` is exposed as the `fika_core` library and has no GPUI dependency.
- `src/main.rs` contains the `fika` binary source.
- GPUI dependencies come from `https://github.com/zed-industries/zed` through `git` package dependencies.
- No GPUI dependency is pinned to a concrete crate release, branch, or revision.

## Engineering Checks

Before implementing a file-view task:

1. Find the Dolphin source path for the behavior.
2. Put behavior state in `fika-core` unless it is purely visual.
3. Ensure pane-scoped async results carry `PaneId + generation`.
4. Add stale-result and split-pane coverage for shared behavior.
5. Wire GPUI rendering or input after the core boundary is stable.

## Documents

- `docs/DESIGN.md` - current GPUI/core architecture
- `docs/TODO.md` - remaining task board
- `docs/GPUI_DOLPHIN_MIGRATION_PLAN.md` - original cutover plan
- `docs/OPTIMIZATION.md` - archived optimization notes
- `docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md` - archived scroll/zoom notes
- `docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md` - archived slot-reuse notes
