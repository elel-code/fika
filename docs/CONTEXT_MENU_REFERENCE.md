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
  - `addViewportContextMenu()` reuses the main window `KNewFileMenu`, calls
    `checkUpToDate()`, sets the working directory to `m_baseUrl`, and adds the
    menu before Open With and Paste.
  - `addItemContextMenu()` handles single item, directory item, multi-item,
    Open With, default item actions, Copy/Move submenus, split-view transfer
    actions, and Properties.
  - `addDirectoryItemContextMenu()` inserts Open in New Tab, Open in New Window,
    Open in Split View, Open With, then creates a `DolphinNewFileMenu`, sets its
    working directory to the clicked directory URL, titles it `Create New`, sets
    icon `list-add`, adds it as a submenu, and only then inserts a separator.
  - `addOpenWithActions()` calls
    `KFileItemActions::insertOpenWithActionsTo(...)`; this is Dolphin's Open
    With application path.
  - `addAdditionalActions()` calls
    `KFileItemActions::addActionsTo(..., KFileItemActions::MenuActionSource::All, ...)`;
    this is Dolphin's service/additional-action path. Ordinary application
    desktop-file `Actions=` are not promoted into the root service menu by
    scanning `applications/`.
  - `insertDefaultItemActions()` inserts Cut, Copy, Copy Location, Paste,
    Duplicate, Rename, Add to Places, Move to Trash, and Delete.
  - `createPasteAction()` chooses paste destination from a selected directory
    when appropriate; otherwise it targets the viewport base directory.
- `../dolphin/src/dolphinmainwindow.cpp`
  - `DolphinMainWindow::openInNewWindow()` opens the current URL when nothing is
    selected, or the single selected folder URL when exactly one item is
    selected, by calling `Dolphin::openNewWindow(...)`.
  - The `open_in_new_window` action text is `Open in New Window` and its theme
    icon is `window-new`.
  - `setupActions()` constructs the shared `DolphinNewFileMenu`, titles it
    `Create New`, and assigns theme icon `list-add`.
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
- `src/core/launcher.rs`
  - Open With application discovery reads XDG application desktop directories
    and `mimeapps.list`.
  - Service menu discovery is deliberately separate and only reads dedicated
    service menu roots: `$XDG_DATA_HOME/fika/servicemenus` or
    `~/.local/share/fika/servicemenus`, plus `kio/servicemenus`,
    `kservices5/ServiceMenus`, and `konqueror/servicemenus` under each XDG data
    root.
  - Application desktop-file `Actions=` remain application metadata. They are
    not converted into service menu rows, so application actions such as Zed
    workspaces or Nautilus new-window entries cannot appear in Fika's blank
    service menu.
- `src/ui/file_grid.rs` and `src/ui/file_grid/snapshot.rs`
  - `file_grid()` renders only `VisibleItemSnapshot` values provided by pane
    snapshots. The item snapshot type lives in `src/ui/file_grid/snapshot.rs`.
  - The item hitbox is the child positioned at `visual_rect`, not the full item
    slot.
  - Blank press, blank right-click, and rubber-band drag are attached to the
    viewport; item left press, item right-click, and item drag are attached to
    the item visual rect. Item visual rects are mouse occlusion hitboxes so
    single-click selection cannot leak into blank viewport handlers underneath.
  - When a context menu or modal overlay is open, the file grid receives
    `mouse_overlay_active` and does not apply item hover styling. This keeps
    menu overlays from visually highlighting items below them even on GPUI paths
    where hover invalidation lags behind the occlusion hitbox update.
- `src/main.rs`
  - `item_at_content_point()` performs model hit testing and then filters by
    `visual_rect`.
  - `start_rubber_band_from_blank()` refuses to start if the pointer is inside
    an item visual rect, and blank left press clears the current selection
    before entering rubber-band state so blank-click clear does not depend on a
    later synthesized click event.
  - Window-coordinate blank press/click/right-click must map into the measured
    file viewport before it can clear selection, start rubber-band, or open a
    blank menu. Fika stores the full viewport window rect, not just its origin,
    so scrollbar space, pane chrome, and other non-viewport regions are ignored
    instead of being treated as blank content.
  - Rubber-band drag updates are clamped to the current viewport rect. Pane
    snapshots pass a viewport-local, already-clipped overlay rectangle to
    `file_grid()`, so the UI does not subtract scroll a second time and the
    selection box cannot extend under the scrollbar.
  - Rubber-band selection records a pane-local selection origin. While that
    origin is active, a context-menu press only opens the selection/item menu
    when the press lands on an already-selected item visual rect. A right press
    on blank viewport space or on an unselected item clears the rubber-band
    selection and does not reuse it for a menu.
  - The rubber-band context hit-test uses the item visual/core rect, not the
    whole compact-view item cell. A right press in padding around a selected
    item is treated as outside selected content and clears the rubber-band
    selection without opening a menu.
  - `show_blank_context_menu_if_blank()` only opens the blank menu after blank
    hit testing.
  - `show_item_context_menu()` focuses the pane, selects the item when needed,
    cancels rubber-band state, and opens the item menu.
  - `context_menu_overlay()` is the current GPUI overlay implementation. The
    full-window layer is a mouse occlusion hitbox, dismisses outside mouse-downs
    during the capture phase, and stops mouse move, wheel, left-click and
    right-click propagation, so menu hover/click cannot pass through to file
    items underneath. Root, submenu, and nested submenu panels also stop mouse
    move propagation. Modal overlays follow the same event barrier rule.
  - `src/ui/context_menu.rs` owns the pure context-menu model and cascade layer:
    `ContextMenuTarget`, `ContextMenuAction`, `ContextMenuItem`,
    `ContextMenuIcon`, `ContextMenuSubmenu`, root/submenu/nested-submenu action
    generation, Open With de-duplication, service-menu promotion and grouping,
    Ark fallback visibility, open submenu state, overlay rects, viewport
    clamp/flip, menu placement math, icon snapshot collection, and GPUI
    overlay/row rendering. `src/main.rs` keeps app state construction and action
    execution.
  - `context_menu_actions()` generates Paste enabled state from the internal
    clipboard, adds Open in New Pane only for directory item targets, and keeps
    Copy Location on single item targets only. It appends Properties to blank,
    single-item, and multi-item menus, matching Dolphin's final properties
    action placement. Blank menus carry the current directory as an
    `inode/directory` service-menu target, so service-provided entries such as
    terminal actions appear with their service icons instead of being hard-coded
    as built-ins. Menu rows have a stable leading icon slot; common file, place,
    trash, sort/view, clipboard and service actions resolve system theme icons
    first and use compact markers only as a fallback instead of rendering
    all-text rows. Blank viewport and directory item menus expose a Dolphin-like
    `Create New` submenu instead of a flat `New Folder` row. The submenu contains
    Folder and Text File entries today; blank menus create in the pane current
    directory, while directory-item menus create inside the clicked directory.
    The `Create New` submenu resolves `list-add`, folders resolve `folder-new`,
    text files resolve `document-new`, and Open in New Window resolves
    `window-new`. Root menu generation also marks Dolphin-style visual groups:
    blank menus separate create/paste/service actions/sort-view/
    select-refresh/properties groups, while item menus separate open/create,
    clipboard/paste, service actions, rename/delete, and properties groups.
  - `ContextMenuSubmenu` mirrors Qt `QMenu` child menus for first-level
    cascading items. `context_menu_overlay()` opens the submenu on hover or
    click, positions it at the parent row, and flips it to the left when there
    is not enough viewport space to the right. Root menus use the mouse position
    as the popup anchor: they keep the menu top-left at the cursor when it fits,
    flip to `anchor - menu_size` on an overflowing axis when there is room before
    the cursor, and only clamp as a last resort for very small viewports. Root
    menus and submenus share
    the same viewport layout calculation for narrow panes, capped height, and
    scrollable overflow instead of letting overlays be clipped by pane or window
    edges. Submenu hide
    follows the Qt menu grace-period model: leaving a
    root or submenu container schedules a delayed hide, entering either
    container cancels it, and stale delayed hides are ignored through an
    app-local generation counter. Individual rows only open or retarget
    submenus; they do not close the menu tree on hover loss.
  - Blank viewport menus expose Dolphin-aligned `Sort By` and `View Mode`
    submenu entries. Sort actions route through pane-local
    `PaneController` sort methods into `DirectoryModel` sorting; each pane
    remembers its own Dolphin-style preferred order per sort role. The Sort By
    submenu includes Folders First and Hidden Files Last pane-local toggles,
    matching Dolphin's `folders_first` and `hidden_last` actions. Future
    Icons/Details view modes remain disabled; Compact is the active view mode.
  - `run_context_menu_action()` routes Open in New Pane through the same
    pane-splitting path as keyboard split actions, then loads the target
    directory into the new pane.
  - `run_context_menu_action()` routes Open in New Window through
    `current_executable_launch_plan()` and `launch_with_systemd_user()`. This
    starts a separate Fika process for the target directory through a systemd
    user transient unit, matching the launcher boundary used by Open With and
    service menu actions rather than spawning a child process directly. System
    service-menu rows labelled Open in New Window/Tab/Pane, Open New
    Window/Tab/Pane, Open in Window, or Open Window are filtered out so similarly
    named actions cannot masquerade as Fika's own new-window action.
  - Fika no longer adds a built-in `Open Terminal Here` item. Terminal entries
    are expected to come from dedicated service menu files, and those actions
    execute through the same systemd user transient unit launcher path as Open
    With.
  - `run_context_menu_action()` writes Copy Location through GPUI's
    `ClipboardItem`/`write_to_clipboard` API.
  - `run_context_menu_action()` routes Paste on a single directory target into
    that directory; blank and non-directory targets paste into the current pane
    directory, following Dolphin `createPasteAction()` destination selection.
  - Places sidebar context menus follow Dolphin `PlacesPanel` on top of
    `KFilePlacesView`: blank sidebar space exposes Add Entry and Show Hidden
    Places; section headings expose Hide Section; normal places expose Open,
    Open in New Pane, Open in New Window, Edit Entry, Remove Entry, Hide, Copy
    Location, and Properties; built-in places keep Edit/Remove disabled; user
    bookmarks keep Edit/Remove enabled; Trash places expose Open, Open in New
    Pane, Open in New Window, Empty Trash, Hide, Copy Location, and Properties.
    Place and section hiding are view state only: they filter the sidebar
    snapshot without deleting
    `PlaceEntry` values or rewriting `user-places.xbel`. User bookmarks are
    persisted through
    `src/core/places.rs` using a KDE/Dolphin-style `user-places.xbel` bookmark
    file under `$XDG_DATA_HOME` with `~/.local/share` fallback; built-in paths
    keep priority over persisted bookmarks. Places rows render theme-resolved
    semantic icons (`user-home`, `folder-download`, `user-trash`,
    `drive-harddisk`, and related fallbacks) in a fixed icon slot; when no
    theme icon exists, the fallback is a small drawn place glyph rather than a
    repeated text marker such as `H`, `Doc`, or `Down`.
  - Trash context menus follow Dolphin's trash branch: blank trash view menus
    expose Empty Trash, trash item menus expose Restore to Former Location and
    Delete Permanently, and Restore is enabled only when the trash metadata can
    resolve an original target. Trash blank menus use a Trash-specific Sort By
    submenu for Name, Original Path, and Deletion Time, matching Dolphin's
    Trash Details roles `text`, `path`, and `deletiontime`.
  - Single item context menus expose small or common service action sets directly
    in the root menu. When many service actions match, important labels such as
    Compress, Extract, Terminal, Send To, Copy To and Move To remain promoted and
    the remaining actions move into a `More Actions` submenu. KDE service menus
    with `X-KDE-Priority=TopLevel` are also promoted to the root menu, while
    protocol, URL-count, and executable-presence conditions are filtered in the
    core launcher before the UI sees the action. `X-KDE-Submenu` labels render
    as real nested submenu rows inside `More Actions`; actions with a KDE
    submenu are kept nested even when the service set is small, unless the
    action explicitly requests `TopLevel`. The actions come only from dedicated
    KDE/Fika service menu desktop files with
    `X-KDE-ServiceTypes=KonqPopupMenu/Plugin` or `KFileItemAction/Plugin`;
    ordinary application `.desktop Actions=` are not a service menu source.
    Action and service menu `Icon=` values are preserved as named theme icons
    and rendered in the menu row icon slot before falling back to compact
    markers. Matching service actions are deduplicated by submenu and display
    label after condition filtering, with `TopLevel` actions taking priority
    over normal duplicates. Execution goes through the same systemd transient
    unit launcher path as Open With. When
    Ark service menus are missing, built-in archive fallbacks fill only the
    equivalent gaps: non-archive files/directories and multi-selections get
    `Compress...`, while a single recognized archive file gets `Extract Here`
    and `Extract To...`; these fallback rows are suppressed as soon as matching
    Compress/Extract service actions exist.
  - Open With's "Other Application..." row opens a GPUI application chooser
    backed by the core launcher cache. The Open With submenu deduplicates by
    desktop id and display name before rendering, so default and added
    associations cannot show the same application twice. Open With submenu rows
    and application chooser rows both use the `.desktop Icon=` value as a named
    theme icon when available, falling back to the generic application icon only
    when the launcher cache has no icon. The chooser body is a GPUI
    `uniform_list` with a persistent `UniformListScrollHandle`, and application
    icon resolution is limited to the visible row range instead of scanning every
    desktop application before the dialog appears. Choosing an application reuses
    the same `DesktopLaunchPlan` and `launch_with_systemd_user()` path as direct
    Open With rows. When a MIME type is known, chooser rows also expose Set
    Default: the action updates the user `mimeapps.list`, reloads launcher
    associations, and refreshes the chooser's Default badge without launching the
    application.
  - Multi-selection context menus use the same service-action promotion rule
    only when the core launcher finds actions that match every selected item and
    support multi-path Exec field codes (`%F`/`%U`). Execution passes the
    pane-local selected path list through the service menu launch plan.
  - `properties_for_path()` and `properties_for_selection()` build the current
    GPUI Properties dialog data from `symlink_metadata()` only. Directory sizes
    are not recursively scanned on the UI path.

## Current Gap List

- Implement Icons and Details view modes behind the existing View Mode submenu.
- Open With execution, Other Application execution, default-app updates, and
  service menu action execution are now driven by core launcher data and the
  systemd launcher path.
- Add remaining multi-selection differences such as batch rename and all-folder
  batch helpers. Built-in Compress and single-archive Extract fallbacks are now
  present when no matching service menu exists.
- Complete Trash-specific conflict handling and Details columns.
- Complete removable device actions.
