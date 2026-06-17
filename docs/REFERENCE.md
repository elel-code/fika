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
| `DolphinView` | `src/ui/pane.rs` (pane shell) |
| `DolphinViewContainer` | `src/main.rs` (app-state routing) |
| `DolphinStatusBar` | `src/ui/status_bar.rs` |
| `DolphinUrlNavigator` / `KUrlNavigator` | `src/ui/location_bar.rs` |
| `KDirLister` | `src/core/directory.rs` |
| `KFileItemModel` | `src/core/model.rs` |
| `KItemListView` | layout in `src/core/view.rs` / `src/ui/file_grid/layout.rs`, retained rendering in `src/ui/file_grid/` |
| `KItemListSmoothScroller` | documented in `docs/SMOOTH_SCROLL_REFERENCE.md`; removed from active code |
| `KDirectoryListerCache` | `src/core/cache.rs` |
| `KItemListCreatorBase` (slot reuse) | `src/ui/file_grid/slots.rs` and `src/ui/file_grid/paint_slots.rs` |
| `KItemListSizeHintResolver` (column width) | `src/ui/file_grid/layout.rs` |
| pane identity / split | `src/core/pane.rs` |
| navigation history | `src/core/pane.rs` |
| `KFileItemActions` / `DolphinContextMenu` | `src/ui/context_menu.rs` |
| `KFilePlacesModel` / `PlacesPanel` | `src/core/places.rs` → `src/ui/places.rs` |
| drag source / drop target | `src/ui/drag_drop.rs` |
| rubber-band selection | `src/ui/rubber_band.rs` |
| search / filter | `src/core/filter.rs` → `src/ui/filter_bar.rs` |
| zoom (`DolphinView::setZoomLevel`) | `src/core/pane.rs` (ViewState), `src/ui/status_bar.rs` (slider) |
| file operation primitives | `src/core/file_ops.rs` |
| undo (`KIO::undo`) | `src/core/operations.rs` |
| trash (`TrashBase`, `DolphinTrash`) | `src/core/file_ops.rs` (trash primitives) |
| MIME detection / `KMimeTypeResolver` | `src/core/mime.rs` |
| Open With / `mimeapps.list` | `src/core/launcher.rs` |
| KDE service menus | `src/core/launcher.rs` (service-menu parser) |
| Ark / `kerfuffle` | `src/core/archive.rs` + `src/core/launcher/ark.rs` |
| application launch / `KProcessRunner` | `src/core/launcher.rs` (systemd transient units) |
| clipboard (`KIO::paste`) | `src/core/clipboard.rs` → `src/ui/clipboard.rs` |
| `KFileItemModelRolesUpdater` (metadata/icon/thumbnail roles) | `src/ui/file_grid/snapshot/scheduler.rs`, `src/ui/icons/cache.rs`, `src/core/thumbnails.rs` |
| GIO/GVfs devices / `Solid::Device` | `src/core/devices.rs` → `src/ui/places.rs` |
| Network / `KFilePlacesModel` remote | `src/core/network.rs` → `src/ui/places.rs` |
| D-Bus / `KDirNotify` / `FileManager1` | `src/core/bus.rs` |
| inline rename (`DolphinView::renameSelectedItems`) | `src/ui/rename.rs` |
| privileged operation API | `src/core/privilege.rs` |
| portal FileChooser backend | `src/bin/fika-xdp-filechooser.rs` |
| system-bus helper | `src/bin/fika-privileged-helper.rs` |
| listing worker | `src/core/listing_worker.rs` |
| in-app chooser shell | `src/ui/chooser.rs` |
| path resolution / breadcrumb | `src/core/location.rs` |
| properties dialog | `src/ui/properties_dialog.rs` |
| application chooser | `src/ui/application_chooser.rs` |
| icon cache / theme resolution | `src/ui/icons.rs` + `src/ui/icons/cache.rs` |
| keyboard shortcut classification | `src/ui/shortcuts.rs` |
| entry metadata role resolution | `src/core/metadata.rs` |
| operation runtime (Tokio + Compio) | `src/core/operation_runtime.rs` |
| Trash emptiness monitor | `src/core/trash_monitor.rs` |
| thumbnail scheduler | `src/core/thumbnails/scheduler.rs` |
| background task panel | `src/ui/background_tasks.rs` |
| CLI argument parsing | `src/cli.rs` + `src/cli/args.rs` |
| trash conflict dialog | `src/ui/trash_conflict.rs` |
| details-view columns | `src/ui/file_grid/details.rs` |
| file-grid hit-test projection | `src/ui/file_grid/projection.rs` and retained interaction in `src/ui/file_grid/interaction.rs` |

## Cargo Boundaries

- Root `Cargo.toml` is a single Cargo package.
- `src/lib.rs` is exposed as the `fika_core` library via `src/core.rs` and has no GPUI dependency.
- `src/main.rs` contains the `fika` binary source.
- GPUI dependencies come from `https://github.com/zed-industries/zed` through `git` package dependencies.
- No GPUI dependency is pinned to a concrete crate release, branch, or revision.
- Direct crates.io dependencies use wide semver ranges (e.g., `tokio = "1"`, `zbus = "5"`, `notify = "8"`).

## Engineering Checks

Before implementing a file-view task:

1. Find the Dolphin source path for the behavior.
2. Put behavior state in `fika-core` unless it is purely visual.
3. Ensure pane-scoped async results carry `PaneId + generation`.
4. Add stale-result and split-pane coverage for shared behavior.
5. Wire GPUI rendering or input after the core boundary is stable.

Before adding a new UI feature:

1. Map the feature to the corresponding Dolphin layer (render / model / interaction).
2. Place new modules in `src/core/` (domain logic) or `src/ui/` (rendering).
3. Prefer directory modules (`feature.rs` + `feature/*.rs`) for features with multiple internal responsibilities.
4. Do not add large behavior blocks to `src/main.rs`.
5. Write the Dolphin reference paths into a `docs/*_REFERENCE.md` document before implementation.

## Reference Document Catalog

### Architecture and Planning
- [DESIGN.md](DESIGN.md) — Current GPUI/core architecture
- [TODO.md](TODO.md) — Remaining task board
- [ITEM_VIEW_CUSTOM_PAINT_DESIGN.md](ITEM_VIEW_CUSTOM_PAINT_DESIGN.md) — Active retained item-view architecture
- [ITEM_VIEW_CUSTOM_PAINT_TODO.md](ITEM_VIEW_CUSTOM_PAINT_TODO.md) — Active item-view custom-paint task board
- [ITEM_VIEW_RENDERER_DECISIONS.md](ITEM_VIEW_RENDERER_DECISIONS.md) — Per-surface renderer choices and gates
- [ITEM_VIEW_RUNTIME_SMOKE.md](ITEM_VIEW_RUNTIME_SMOKE.md) — Runtime DnD, rename, and perf-log smoke checklist
- [GPUI_DOLPHIN_MIGRATION_PLAN.md](GPUI_DOLPHIN_MIGRATION_PLAN.md) — Original cutover plan
- [DOLPHIN_ITEM_SLOT_REUSE_PLAN.md](DOLPHIN_ITEM_SLOT_REUSE_PLAN.md) — Archived slot-reuse notes
- [SCROLL_ZOOM_PERFORMANCE_PLAN.md](SCROLL_ZOOM_PERFORMANCE_PLAN.md) — Archived scroll/zoom notes
- [OPTIMIZATION.md](OPTIMIZATION.md) — Archived optimization notes
- [BUG_ANALYSIS_BLANK_DIRECTORY.md](BUG_ANALYSIS_BLANK_DIRECTORY.md) — Blank-directory bug analysis
- [BUG_ANALYSIS_SCROLLBAR_DRAG.md](BUG_ANALYSIS_SCROLLBAR_DRAG.md) — Scrollbar drag-regression bug analysis

### Dolphin / Fika Reference
- [LOCATION_BAR_REFERENCE.md](LOCATION_BAR_REFERENCE.md) — `KUrlNavigator` breadcrumb and editable modes
- [ZOOM_REFERENCE.md](ZOOM_REFERENCE.md) — Zoom level, icon-size mapping, grid update
- [STATUS_BAR_REFERENCE.md](STATUS_BAR_REFERENCE.md) — `DolphinStatusBar` info display and zoom slider
- [SMOOTH_SCROLL_REFERENCE.md](SMOOTH_SCROLL_REFERENCE.md) — `QScroller` smooth/kinetic scrolling
- [SEARCH_REFERENCE.md](SEARCH_REFERENCE.md) — Search box and KIO search integration

### Interaction Reference
- [CONTEXT_MENU_REFERENCE.md](CONTEXT_MENU_REFERENCE.md) — Context menu complete execution flow
- [DRAG_DROP_REFERENCE.md](DRAG_DROP_REFERENCE.md) — Drag-and-drop execution flow
- [CLIPBOARD_REFERENCE.md](CLIPBOARD_REFERENCE.md) — Dolphin/KIO file clipboard

### System Integration Reference
- [MIME_LAUNCHER_REFERENCE.md](MIME_LAUNCHER_REFERENCE.md) — MIME detection and application launching
- [DEVICES_REFERENCE.md](DEVICES_REFERENCE.md) — GIO/GVfs device discovery and mount operations
- [TRASH_REFERENCE.md](TRASH_REFERENCE.md) — XDG Trash spec and Dolphin implementation
- [THUMBNAIL_REFERENCE.md](THUMBNAIL_REFERENCE.md) — Freedesktop thumbnail specification
- [NETWORK_REFERENCE.md](NETWORK_REFERENCE.md) — GVfs remote filesystem classification
- [BUS_CONTROL_REFERENCE.md](BUS_CONTROL_REFERENCE.md) — D-Bus bus control and routing
- [ARK_REFERENCE.md](ARK_REFERENCE.md) — Ark/kerfuffle archive integration
