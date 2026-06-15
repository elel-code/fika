# Drag and Drop Reference

Fika's drag and drop path follows Dolphin's item-view controller model:
selection and hit-testing belong to the item controller, file URLs belong to the
model, and file operations are launched only after a drop target is resolved.

## Dolphin Source

- `../dolphin/src/kitemviews/kitemlistcontroller.cpp`
  - `mouseMoveEvent()` starts an item drag only when the pressed position was on
    an item, the left button is still held, and movement exceeds Qt's drag
    threshold.
  - Before starting a drag, Dolphin ensures the pressed item is selected. If the
    item was already selected, the whole current selection is dragged.
  - `startDragging()` reads `m_selectionManager->selectedItems()`, asks the model
    for mime data, exports URLs to the portal, creates a `QDrag`, assigns the
    drag pixmap and hotspot, and executes Move/Copy/Link with Copy as the
    default action.
  - `dragEnterEvent()` clears `DragAndDropHelper`'s URL match cache.
  - `dragLeaveEvent()` stops auto activation, disables auto-scroll, hides the
    drop indicator, and unhovers the current drop widget.
  - `dragMoveEvent()` maps the pointer into item-view coordinates, updates the
    hovered item or insertion indicator, decides the effective target directory,
    and accepts or ignores the proposed action through
    `DragAndDropHelper::urlListMatchesUrl()` and `model->supportsDropping()`.
  - `dropEvent()` stops drag-time state, checks insertion-position drops first,
    then emits an item drop for either the receiving item or the blank viewport.
- `../dolphin/src/kitemviews/kfileitemmodel.cpp`
  - `createMimeData()` converts selected model indexes to URL lists, skips child
    entries whose parent is already included, and calls `KUrlMimeData::setUrls()`
    with both original URLs and most-local URLs.
  - `supportsDropping()` accepts the root directory when the index is `-1`, or a
    concrete model item otherwise, and delegates the final decision to
    `DragAndDropHelper::supportsDropping()`.
- `../dolphin/src/kitemviews/kfileitemlistview.cpp`
  - `createDragPixmap()` builds a preview pixmap from multiple selected items,
    using a compact grid when more than one item is dragged.
- `../dolphin/src/kitemviews/kitemlistview.cpp`
  - `dragEnterEvent()` accepts the drag and enables auto-scroll.
  - `dragMoveEvent()` updates the tracked mouse position and starts the
    auto-scroll timer.
  - `dragLeaveEvent()` and `dropEvent()` both disable auto-scroll.
  - `showDropIndicator()` uses visible item widgets to decide between dropping
    onto a directory-capable item and inserting between items; `hideDropIndicator()`
    clears the indicator.
- `../dolphin/src/views/dolphinview.cpp`
  - `slotItemDropEvent()` maps a drop on an item to either that directory's URL
    or the current view URL.
  - `dropUrls()` applies Dolphin-specific `KIO::DropJobFlags`, calls
    `DragAndDropHelper::dropUrls()`, connects operation results, and refreshes
    created items through KIO job signals.
- `../dolphin/src/views/draganddrophelper.cpp`
  - `dropUrls()` rejects no-op drops where the source URL list already matches
    the destination, handles Ark drag MIME types, and otherwise launches a
    `KIO::drop()` job.
  - `supportsDropping()` accepts writable directories, desktop files, and local
    executable files.
  - `updateDropAction()` sets IgnoreAction for no-op or unsupported targets and
    otherwise accepts the proposed action.
- `../dolphin/src/panels/places/placespanel.cpp`
  - `dragMoveEvent()` rejects non-writable place targets for external drags, but
    still allows internal place reordering.
  - `slotUrlsDropped()` delegates file drops to `DragAndDropHelper::dropUrls()`
    and reports non-cancelled job errors.
- `../dolphin/src/dolphintabbar.cpp` and
  `../dolphin/src/dolphintabwidget.cpp`
  - Tab drag enter/move/drop accept URL drags and retarget drops to the
    corresponding tab's current view URL.

## Fika Mapping

- Dolphin `KItemListController::mouseMoveEvent()` -> GPUI item `on_drag` on the
  item `visual_rect`, while blank-area drags remain owned by the viewport-level
  rubber-band handler.
- Dolphin "pressed selected item drags the whole selection" -> Fika item drag
  preview uses pane-local selection count when the dragged item is selected, and
  falls back to a single-item preview for unselected items.
- Dolphin `KFileItemModel::createMimeData()` -> Fika `DragExportPayload` is
  built from `PaneController::selected_paths()` for selected item drags, prunes
  child paths when a parent directory is already exported, and encodes
  `text/uri-list` through the same URI-list path used by clipboard operations.
- Dolphin `DragAndDropHelper::supportsDropping()` -> Fika drop target checks
  accept current directory blank space, directory items, breadcrumb segments and
  Places rows when their resolved directory is valid, and reject no-op drops
  onto their own source directory or descendant directory.
- Dolphin `KItemListView::showDropIndicator()` -> Fika pane drop target keeps
  directory-hover highlight separate from insertion indicators and from normal
  selection highlight. GPUI `on_drag_move` handlers must check their own bounds:
  capture-phase drag move is not a reliable hover test by itself.
- Dolphin drag/drop transient state -> `src/ui/drag_drop.rs` as the module
  entry and `src/ui/drag_drop/state.rs` as the directory-style child module for
  transfer modes, item/place drag payloads, drop target state and target helper
  queries.
- Dolphin `PlacesPanel::slotUrlsDropped()` -> Fika Places drop distinguishes
  path-list drop-on-place file operations from drop-between-place bookmark
  insertion, while internal `PlaceDrag` in the Places sidebar is restricted to
  insert/reorder targets.

## Current Fika State

- Item drag is already attached to each rendered item's `visual_rect`, so
  dragging an item does not start blank-area rubber-band selection. The item
  visual rect is also a mouse occlusion hitbox, and item left press handles
  single selection before stopping propagation to the viewport.
- Blank left press clears the current selection and arms rubber-band selection
  only after the content hit-test confirms that the pointer is not inside an
  item visual rect.
- Drag preview now reflects the current pane selection count for selected items,
  matching Dolphin's "drag selected item means drag selected set" behavior.
- Pane-background drop target updates only when the pointer is inside the file
  viewport and the content hit-test says the pointer is on blank space. Directory
  item, file item, Places row and Places heading targets update only when the
  pointer is inside their GPUI bounds, so the active drop target tracks the
  actual endpoint in real time instead of being overwritten by sibling handlers.
- Pane blank space, directory item, breadcrumb segment, Places row and Places
  section drag handlers also clear only the target they own when a later
  drag-move event reports the pointer outside their bounds. This mirrors
  Dolphin's `dragLeaveEvent()` cleanup at the target level and prevents old
  pane tint, directory highlight or Places insertion lines from waiting for the
  lease timeout when the pointer has already moved elsewhere.
- Pane, directory item, breadcrumb segment and Places row drop targets resolve
  the destination first, then show a DropOperation context menu. The final file
  operation comes from the menu action (`Copy`, `Move` or `Link`); hover cursor
  feedback distinguishes valid drop-menu targets, bookmark insert/reorder
  targets, and invalid targets while a drag is moving.
- `src/ui/drag_drop.rs` now owns the DnD UI module boundary, while
  `src/ui/drag_drop/state.rs` owns `FileTransferMode`, `ItemDragPayload`,
  `ActiveItemDrag`, `DragExportPayload`, `ItemDropTarget`, `PlaceDropTarget`,
  path normalization, cursor style mapping, no-op drop rejection and drop target
  matching helpers. `main.rs` still performs the app-level operation routing
  after a target is resolved.
- Internal item drags and GPUI `ExternalPaths` drags normalize source path lists
  at ingress. Duplicate paths are removed and child paths are pruned when their
  parent directory is already present, matching the export payload path pruning.
- Item drop validation is shared by internal and external path-list drops. It
  rejects empty source lists, non-directory targets, dropping a source onto
  itself, and dropping a directory into itself or one of its descendants before
  a hover target is advertised or a final drop operation menu is shown.
- Item drags now carry a prepared external export payload alongside the internal
  GPUI drag value. The payload contains the resolved path list, `text/uri-list`
  and `text/plain` data. Places drags prepare the same payload only when the
  place path exists and is a directory.
- GPUI `ExternalPaths` drops are wired through the same target resolution path
  for file-grid blank space, directory items, breadcrumb segments, Places rows
  and Places insertion lines. Places insertion accepts only one new existing
  directory, while drop-on-place targets still require the place to resolve to a
  valid mounted directory.
- Directory item, pane background and Places row drop targets use dedicated
  drop-target styling while hovered, and Places insertion uses a separate line
  indicator. This visual state is separate from selection, hover and active
  place state; the operation itself is still selected from the drop menu.
- Drop target lease timeout remains as a fallback for drag cancellation or
  platform/backend paths that stop producing drag-move events; it is no longer
  the primary cleanup path for ordinary target leave/target switch.
- Ark drag-extract MIME parsing exists in core as
  `ark_dnd_extract_payload()`. It requires both
  `application/x-kde-ark-dndextract-service` and
  `application/x-kde-ark-dndextract-path`, validates the D-Bus service/object
  path pair, and returns an `ArkDndExtractPayload`. `ArkDndExtractRequest` then
  combines that payload with an absolute destination directory and
  `execute_ark_dnd_extract_with_bus()` calls
  `org.kde.ark.DndExtract.extractSelectedFilesTo(destination)` through the
  shared session bus helper. The UI/backend still needs a multi-MIME external
  offer path before this executor can be reached from a real Ark drag.
- Internal `PlaceDrag` can reorder primary Places entries, including active and
  built-in Home/Trash-style entries, by dropping on a Places insertion line or
  over another primary row. It does not trigger drop-on-place file operations
  inside the Places sidebar. Reorder targets are clamped to the primary Places
  block before grouped sections such as Network or removable devices. The
  persisted XBEL projection still writes only editable/removable user bookmarks
  to Fika's own `fika/places.xbel`, so dynamic/grouped entries do not leak into
  the saved Places file.
- External Wayland/X11 drag MIME publication is not complete yet. GPUI's current
  app-level drag value is sufficient for internal drop targets and GPUI
  `ExternalPaths` is sufficient for ordinary path-list drops into Fika. Fika now
  prepares the exact `text/uri-list` + `text/plain` payload required for
  dragging out, but system MIME data still needs a backend path capable of
  publishing that payload to other applications.

## Remaining Work

- Add a backend path for arbitrary external MIME offers beyond GPUI
  `ExternalPaths`, including multi-MIME offers that cannot be represented as a
  plain file path list.
- Wire `DragExportPayload` into the future GPUI/backend drag-source MIME
  publication API.
- Feed Ark DnD service/path MIME offers into the core parser/executor instead
  of ordinary copy/move/link when that payload is present.
