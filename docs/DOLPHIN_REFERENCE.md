# Dolphin Interaction Reference

本文件记录本仓库中 `./dolphin -> /home/yk/Code/dolphin` 的源码参照点，后续打磨 Fika 交互时优先查这些文件。

## Context Menus

Reference files:

- `dolphin/src/dolphincontextmenu.cpp`
- `dolphin/src/dolphincontextmenu.h`
- `dolphin/src/views/dolphinviewactionhandler.cpp`
- `dolphin/src/selectionmode/bottombarcontentscontainer.cpp`

Useful interaction rules:

- Context is split first: trash, item, or viewport. Fika should keep separate code paths for item menus, blank-area menus, and future trash/search menus.
- Item menus are based on both the clicked item and the complete selection.
- `Add to Places` is single-directory-only and hidden if the place already exists.
- `Copy Location` is effectively single-item-only.
- Multi-selection menus still allow batch-safe actions such as copy, cut, rename, trash, properties, and Open With when the backend supports them.
- Selection-mode actions reuse the same context menu and filter duplicate or too-dangerous actions before placing actions in the bottom bar.
- Parent and child menus are positioned by the native menu engine from the parent action rectangle. The child menu is anchored to the hovered row, opens to the side with available space, and clamps to the screen work area. Mouse-triggered menus use the global pointer position; keyboard-triggered item menus use the selected item's context rectangle bottom-right.
- Cancellation is contextual: activating an action closes the menu tree, clicking outside or pressing Escape closes it, and moving from a submenu parent to its child uses the toolkit grace period. Hovering ordinary menu items should not itself close an already open submenu; only leaving the submenu parent/child relationship should start submenu cancellation.

Current Fika mapping:

- Fika hides `Rename`, `Open With`, `Open Folder With`, and `Add to Places` in multi-selection until those batch backends have clear semantics.
- Multi-selection currently exposes implemented batch-safe actions: Cut, Copy, and Move Selected to Trash. Paste is exposed from the current-folder and folder-item menus when the internal clipboard has paths.
- A future batch menu can follow Dolphin further by adding batch rename, Open With for multiple items, properties, and selection actions as each backend exists.
- Fika implements custom Slint menus, so it must emulate Qt's menu behavior explicitly: root-menu trigger points are resolved by `RootContextMenuLayer`, child submenu open state and row anchors are owned by the `MenuLifecycle` global, Open With, Create New, and service-menu groups share one active child-menu placement/bridge property set, horizontal placement flips at the window edge, and delayed close timers are driven only by submenu parent/child leave events.
- Submenu rows now use a separate arrow indicator instead of baking `>` into the label. Open With, Create New, and service-menu groups also have invisible hover bridges between the parent row and child menu, matching the feel of Qt's grace area when moving the pointer diagonally into a submenu.
- Root menu, child menu, transfer menu, and chooser-choice popup placement now share explicit Rust-side popup geometry: context roots and Transfer popups use QMenu-style preferred/flip/clamp rules, Open With / Create New / service-menu group child menus and bridges anchor to the parent row, and chooser-choice popups keep their above-button anchor while clamping to safe margins.
- Service-menu actions follow Dolphin's user-configurable direction at a smaller scope: discovered actions are filtered by a persisted policy that can either show all non-disabled actions or only explicitly checked actions, while the configuration popup is backed by the full current match set so hidden rows remain recoverable.
- Viewport context menu ordering is closer to Dolphin: Create New is first, Open Folder With follows, then Open Terminal Here. The internal drop menu includes an explicit Cancel action like common file-operation menus.

## Selection

Reference files:

- `dolphin/src/kitemviews/kitemlistselectionmanager.cpp`
- `dolphin/src/kitemviews/kitemlistselectionmanager.h`
- `dolphin/src/tests/kitemlistselectionmanagertest.cpp`

Useful interaction rules:

- Track current item separately from selected items.
- Keep an anchor item for range selection.
- Anchored selection is transient while the current item moves, then committed into the selected set.
- Selection changes emit both current and previous selection, which makes downstream panels and actions easier to update.

Current Fika mapping:

- Fika already has `selection_anchor`, Ctrl toggle, Shift range, Ctrl+Shift append, Ctrl+A, and rectangle selection.
- Pure filtering, visible retention, range selection, rectangle selection, and append-unique behavior are now isolated in `src/app/selection.rs`; `main.rs` keeps UI callback orchestration and selection UI synchronization.

## Drag And Drop

Reference files:

- `dolphin/src/views/draganddrophelper.cpp`
- `dolphin/src/views/draganddrophelper.h`
- `dolphin/src/tests/draganddrophelpertest.cpp`
- `dolphin/src/panels/places/placespanel.cpp`

Useful interaction rules:

- Drop targets are validated before accepting drag motion.
- Dropping onto itself is ignored.
- Valid file drop destinations are writable directories, desktop files, or executable local files.
- The final operation menu is delegated to a common drop job so Move / Copy / Link / Cancel behavior is consistent.

Current Fika mapping:

- Fika already has Move / Copy / Link transfer menus for main-view folders, current directory, and Places targets.
- Transfer preparation and execution now reject self-drop and folder-into-descendant targets before showing or running Move / Copy / Link.
- Clearer unsupported-target feedback can still be expanded as more non-folder target types become valid.

## View And Menu Placement

Reference files:

- `dolphin/src/kitemviews/kstandarditemlistview.cpp`
- `dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp`
- `dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp`
- `dolphin/src/views/dolphinitemlistview.cpp`

Useful interaction rules:

- Dolphin's horizontal column-first file arrangement is the compact item layout path: `KStandardItemListView::setItemLayout()` sets horizontal scroll orientation for `CompactLayout`, while the layouter transposes the logical grid and `itemRect()` rotates it back into physical horizontal coordinates.
- Compact rows show the icon and file name in one horizontal item, not an icon-centered tile with the name underneath.
- View state and selection should be preserved across non-navigational refreshes.
- Menus and hover submenus should be contextual, not global: the submenu anchor is the parent item, not the root menu.
- Smoothness comes from preserving old visible state until new data is ready, then replacing atomically.

Current Fika mapping:

- The main pane follows the horizontal compact direction: `rows_per_column` comes from visible height, entry index maps to `column = index / rows_per_column` and `row = index % rows_per_column`, and ordinary item render tokens place media on the left with the file name on the right.
- Fika already preserves old view during uncached navigation, caches directory entries, and remembers per-directory horizontal scroll.
- Top/header layout is split between `TopBar` for global search/split/theme controls and `PathBar` for main-pane Back/Forward plus path editing, keeping chrome layout details out of `AppWindow`.
- Status bar and chooser footer controls now live in `StatusBar`, keeping bottom-row actions out of `AppWindow`.
- Submenu positioning has been changed to anchor to the actual parent menu item and avoid window edges.
- File item, viewport, Open With, Create New, Transfer, Places, Devices, and Places blank-area menu content have been split into `ui/menus.slint`.
- Root file / Places / Devices / blank-area menu hosting and root popup placement now go through `RootContextMenuLayer`, while delayed-close timers and child-submenu hover transitions live in `MenuLifecycleController`; `AppWindow` keeps action wiring and business-state updates.
- Transfer operation menus and chooser filter/choice popups now use `TransferMenuLayer`, `ChooserOptionPopupLayer`, and `ChooserChoicePopupLayer`, keeping repeated popup shell, fixed sizing, root-menu flip/clamp placement, and anchored positioning out of `AppWindow` and transfer target logic.
- Places/main-pane drag feedback now goes through `DragOverlayLayer`, keeping ghost, insertion-line, and rejection visuals out of `AppWindow`.
- Root context placement, Transfer placement, Open With / Create New / service-menu group child submenu placement, hover bridge geometry, and chooser-choice popup clamping share Rust-side popup geometry helpers.

## Search

Reference files:

- `dolphin/src/search/bar.cpp`
- `dolphin/src/search/widgetmenu.cpp`

Useful interaction rules:

- Search state is exposed as a real toolbar/panel state, not a modal.
- Search options belong near the search field and are visible while searching.
- Search strip layout now lives in `SearchPanel`, so future Dolphin-like filters can be added without expanding `AppWindow`.

Current Fika mapping:

- Fika already opens a Dolphin-like search strip from the top bar and supports recursive search.
- Recursive search results are grouped by parent location without inserting separate rows, so the current column-first virtualized grid remains stable.
- Further work should add deeper filter UI parity only when the filtering backend exists.
