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
- Dolphin `KFileItemModel::createMimeData()` -> future Fika drag payload should
  be built from `PaneController::selected_paths()` and encoded with the same
  `FileClipboardPayload` URI-list path used by clipboard operations.
- Dolphin `DragAndDropHelper::supportsDropping()` -> future Fika drop target
  checks should accept current directory blank space, writable directories,
  desktop files and local executables, and reject no-op drops onto their own
  source directory.
- Dolphin `KItemListView::showDropIndicator()` -> Fika pane drop target keeps
  directory-hover highlight separate from insertion indicators and from normal
  selection highlight. GPUI `on_drag_move` handlers must check their own bounds:
  capture-phase drag move is not a reliable hover test by itself.
- Dolphin `PlacesPanel::slotUrlsDropped()` -> future Fika Places drop should
  distinguish drop-on-place file operations from drop-between-place bookmark
  insertion.

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
- Pane, directory item and Places row drop targets carry the current
  `FileTransferMode`, recomputed from `Window::modifiers()` on every drag move
  and again on drop. No modifier or secondary/Ctrl means Copy, Shift means Move,
  and Shift+secondary/Ctrl or Alt means Link. This applies to internal item
  drags and GPUI `ExternalPaths` drags.
- Directory item, pane background and Places row drop targets use action-specific
  colors while hovered: green for Copy, amber for Move, and purple for Link.
  This visual state is separate from selection, hover and active place state.
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
- Internal `PlaceDrag` can reorder editable/removable user bookmarks by dropping
  on a Places insertion line. Reorder targets are clamped to the persisted user
  bookmark block, built-in Home/Trash/Root and future device places are refused,
  and successful moves are written back to `user-places.xbel`.
- External Wayland/X11 drag MIME publication is not complete yet. GPUI's current
  app-level drag value is sufficient for internal drop targets, but system MIME
  data still needs a backend path capable of publishing `text/uri-list` and
  `text/plain` together.

## Remaining Work

- Complete external item and blank viewport drop targets for backend MIME data
  offers that do not yet arrive as GPUI `ExternalPaths`.
- Integrate external drag MIME data with the same URI-list encoder used by
  clipboard operations.
- Feed Ark DnD service/path MIME offers into the core parser/executor instead
  of ordinary copy/move/link when that payload is present.
