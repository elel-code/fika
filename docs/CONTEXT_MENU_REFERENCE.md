# Context Menu Reference

This document records the Dolphin source paths used while migrating Fika's pane
context menu behavior. Dolphin remains the source of truth for action grouping,
item-vs-blank event boundaries, and later submenu behavior.

## Dolphin Sources

- `../dolphin/src/dolphincontextmenu.h`
  - `DolphinContextMenu` is a `QMenu` for both item and viewport context menus.
  - Constructor inputs distinguish item context from blank viewport context:
    `fileInfo`, `selectedItems`, `baseUrl`, and `KFileItemActions`.
  - The class owns helper paths for trash, item, viewport, Open With, paste, and
    additional service actions.
- `../dolphin/src/dolphincontextmenu.cpp`
  - `DolphinContextMenu::addAllActions()` detects context and dispatches to
    trash item, trash viewport, normal item, or normal viewport menu builders.
  - `addViewportContextMenu()` inserts Create New, Open With for the current
    directory, Paste, Add to Places, Sort By, View Mode, additional actions, and
    Properties.
  - `addItemContextMenu()` handles single item, directory item, multi-item,
    Open With, default item actions, Copy/Move submenus, split-view transfer
    actions, and Properties.
  - `insertDefaultItemActions()` inserts Cut, Copy, Copy Location, Paste,
    Duplicate, Rename, Add to Places, Move to Trash, and Delete.
  - `createPasteAction()` chooses paste destination from a selected directory
    when appropriate; otherwise it targets the viewport base directory.
- `../dolphin/src/kitemviews/kitemlistcontroller.cpp`
  - Right-click cancels active rubber-band selection before menu handling.
  - Blank-region right-click consumes the event and does not create a rubber
    band.
  - Hit testing separates the row hover region from `selectionRectCore()`, so
    clicks outside icon/text core are treated as empty row space when the view
    does not highlight the entire row.
  - Ctrl-left-click inside `selectionRectCore()` toggles selection; outside the
    core it can start rubber-band selection.
- `../dolphin/src/kitemviews/kstandarditemlistwidget.cpp`
  - `textRect()`, `textFocusRect()`, `selectionRectFull()`, and
    `selectionRectCore()` define the visual and interactive item core.
  - In compact layout, width hints are based on icon size, padding, and required
    text width.

## Fika Mapping

- `src/core/view.rs`
  - `ItemLayout::visual_rect` maps to Dolphin's item core selection area for
    compact view interaction.
  - `CompactLayout::item_with_required_text_width()` narrows the visual rect
    according to the current entry name width.
- `src/ui/file_grid.rs`
  - `file_grid()` renders only `VisibleItemSnapshot` values provided by pane
    snapshots.
  - The item hitbox is the child positioned at `visual_rect`, not the full item
    slot.
  - Blank click, blank right-click, and rubber-band drag are attached to the
    viewport; item click, item right-click, and item drag are attached to the
    item visual rect.
- `src/main.rs`
  - `item_at_content_point()` performs model hit testing and then filters by
    `visual_rect`.
  - `start_rubber_band_from_blank()` refuses to start if the pointer is inside
    an item visual rect.
  - `show_blank_context_menu_if_blank()` only opens the blank menu after blank
    hit testing.
  - `show_item_context_menu()` focuses the pane, selects the item when needed,
    cancels rubber-band state, and opens the item menu.
  - `context_menu_overlay()` is the current GPUI overlay implementation.
  - `context_menu_actions()` generates Paste enabled state from the internal
    clipboard, adds Open in New Pane only for directory item targets, and keeps
    Copy Location on single item targets only. It appends Properties to blank,
    single-item, and multi-item menus, matching Dolphin's final properties
    action placement.
  - `run_context_menu_action()` routes Open in New Pane through the same
    pane-splitting path as keyboard split actions, then loads the target
    directory into the new pane.
  - `run_context_menu_action()` writes Copy Location through GPUI's
    `ClipboardItem`/`write_to_clipboard` API.
  - `run_context_menu_action()` routes Paste on a single directory target into
    that directory; blank and non-directory targets paste into the current pane
    directory, following Dolphin `createPasteAction()` destination selection.
  - `properties_for_path()` and `properties_for_selection()` build the current
    GPUI Properties dialog data from `symlink_metadata()` only. Directory sizes
    are not recursively scanned on the UI path.

## Current Gap List

- Add Dolphin-equivalent Sort By and View Mode submenus.
- Add Open With submenu populated by MIME/application data.
- Add Open in New Window.
- Add remaining multi-selection differences such as Compress and batch rename.
- Add Trash-specific Restore, Empty Trash, and Delete Permanently actions.
- Add Places context menus.
- Add submenu positioning and delayed hide behavior comparable to Qt menus.
