# Fika Reference Index

本文档是迁移期参考索引。旧的 Rust + Slint 架构说明已经失效；未来目标见
`docs/DESIGN.md` 和 `docs/GPUI_DOLPHIN_MIGRATION_PLAN.md`。

## Primary Reference: Dolphin

本地 Dolphin 源码在 `../dolphin`。实现前必须先查源码，不使用记忆或猜测替代。

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

## Target Fika Concepts

| Dolphin concept | Fika target |
| --- | --- |
| `DolphinView` | GPUI pane entity + view shell |
| `KDirLister` | `DirectoryLister` |
| `KFileItemModel` | `DirectoryModel` |
| `KItemListView` | GPUI item view entity |
| `KItemListViewLayouter` | Rust item-view layouter |
| `KItemListController` | Pane-local controller |
| `KFileItemModelRolesUpdater` | thumbnail/metadata role scheduler |

## Current Fika Code Worth Reusing

These modules may be mined during the GPUI rewrite, but they must be moved behind UI-neutral interfaces.

- `src/fs/file_ops.rs`
  - transfer, trash, undo primitives
- `src/fs/entries.rs`
  - local directory entry reading and trash metadata decoration
- `src/fs/thumbnails.rs`
  - freedesktop thumbnail cache and thumbnailer discovery
- `src/fs/devices.rs`
  - mountinfo/UDisks2 discovery and diagnostics
- `src/desktop/mime_open.rs`
  - MIME/default app lookup
- `src/desktop/service_menu.rs`
  - KDE/Fika service-menu parsing
- `src/app/operation_controller.rs`
  - operation queue summaries, undo serial policy, affected-directory routing
- `src/support/generation.rs`
  - stale result helper

Do not reuse these as future architecture:

- `ui/*.slint`
- Slint `ModelRc` / `VecModel` projection layers
- slot/focused-pane callback routing
- old directory reload queues
- Slint-specific DnD and menu lifecycle globals

## Migration Documents

- `docs/TODO.md`
  - active task board
- `docs/DESIGN.md`
  - GPUI target architecture
- `docs/GPUI_DOLPHIN_MIGRATION_PLAN.md`
  - full replacement plan
- `docs/OPTIMIZATION.md`
  - archived Slint optimization history
- `docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md`
  - archived Slint scroll/zoom investigation
- `docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md`
  - archived Slint slot-reuse investigation

## Engineering Checks

Before implementing a GPUI migration task:

1. Find the Dolphin source path for the behavior.
2. Write the Fika core type boundary.
3. Ensure every async result has `PaneId + generation`.
4. Write a stale-result test.
5. Write a split-pane test.
6. Only then wire GPUI rendering or input.
