# Fika Design: GPUI Architecture

本文档描述当前 GPUI 主线架构。实现边界以根 Cargo package 和 `src/` 源码目录为准；Dolphin 源码执行流仍是目录加载、刷新、model signal 和 current-directory-removed 行为的第一参考。

## Goals

- 用 GPUI 承载窗口、pane、输入路由和渲染。
- 保持 `fika-core` UI-neutral：core 不依赖 GPUI、窗口句柄或 UI model 类型。
- 每个 pane 都有稳定 identity：`PaneId + generation` 是 lister、watcher、async result 和 UI event 的路由边界。
- 目录变化通过 lister event 进入 `DirectoryModel`，GPUI 层只渲染 snapshot 并派发 action。
- 旧 UI 主路径不再存在；新功能只进入 GPUI/core 主路径。
- 新增 UI 功能优先采用现代 Rust 目录式模块（`feature.rs` 入口 + `feature/*.rs` 子职责），`src/main.rs` 只保留 app 状态编排和跨模块路由。

## Non-Goals

- 不翻译旧 UI 文件。
- 不保留旧 slot、focused-pane fallback 或 reload queue。
- 不一次性复制 Dolphin 的所有 KDE/KIO 后端。当前主线先保住本地目录、pane identity、portal/helper 边界。
- 不在 GPUI render/input 路径中执行阻塞 I/O。

## Reference Priority

1. Dolphin source execution flow (`../dolphin`).
2. Linux desktop specifications and services used by Dolphin-like behavior:
   XDG trash, freedesktop thumbnails, MIME apps, service menus, UDisks2, Polkit.
3. Existing `fika-core` modules when they preserve the Dolphin-style flow.
4. GPUI idioms for entity, view, state, and input composition.

## Source Layout

```text
src/
  lib.rs                         UI-neutral core module exports
  main.rs                        GPUI application and chooser shell
  core.rs                        Core module re-exports
  core/
    archive.rs                   Ark DnD extraction and classification
    bus.rs                       D-Bus session/system bus controller
    cache.rs                     Directory entry cache (LRU, shared Arc payloads)
    clipboard.rs                 URI-list encode/decode and GPUI round-trip
    directory.rs                 Directory lister and watcher events
    entries.rs                   File entry metadata and sorting
    file_ops.rs                  File transfer/trash/create/rename primitives
    filter.rs                    Name filter model (plain-text, glob)
    launcher.rs                  .desktop / mimeapps.list application discovery
    listing_worker.rs            Background directory-read worker
    location.rs                  Path resolution, breadcrumbs, tab-completion
    mime.rs                      MIME detection via shared-mime-info
    model.rs                     Directory model snapshots and signals
    network.rs                   GVfs/remote filesystem classification
    operations.rs                Operation queue and undo boundary
    pane.rs                      Pane identity, state, split/close routing
    places.rs                    Places model (bookmarks, devices, network)
    privilege.rs                 Privileged operation API surface
    scroll.rs                    Smooth-scroll easing and kinetic tracker
    thumbnails.rs                Freedesktop thumbnail URI and cache keys
    view.rs                      Compact layout, viewport math, visible range
    devices.rs                   UDisks2 device discovery (entry point)
    devices/
      actions.rs                 Mount/unmount/eject/safely-remove operations
    launcher/
      ark.rs                     Ark archive launch plan construction
      results.rs                 Launch result types
    operations/
      tasks.rs                   File operation task result types
  ui.rs                          UI module re-exports
  ui/
    application_chooser.rs       "Other Application…" chooser
    chooser.rs                   File chooser mode
    clipboard.rs                 Clipboard interaction
    context_menu.rs              Context menu target/action/icon model
    controls.rs                  Shared UI control helpers
    drag_drop.rs                 Drag-and-drop
    file_grid.rs                 File grid and visible-item virtualization
    filter_bar.rs                Filter bar
    icons.rs                     File/named icon resolution
    location_bar.rs              Location bar (breadcrumb + editable)
    pane.rs                      Pane shell
    places.rs                    Places sidebar
    properties_dialog.rs         Properties dialog
    rename.rs                    Inline rename
    rubber_band.rs               Rubber-band selection
    scrollbar.rs                 Horizontal scrollbar entry point
    shortcuts.rs                 Keyboard shortcut classification
    status_bar.rs                Status bar
    place_draft.rs               Places Add/Edit draft
    application_chooser/
      identity.rs                Application chooser item identity
      search.rs                  Application chooser search caret and hit-test
    chooser/
      state.rs                   Chooser state and portal metadata output
    clipboard/
      state.rs                   Copy/cut mode and GPUI ClipboardItem state
    drag_drop/
      state.rs                   DnD state, export payloads, modifier-to-mode, target matching
    file_grid/
      layout.rs                  Compact column-width cache and layout assembly
      slots.rs                   Visible-item slot pool (recycled element IDs)
      snapshot.rs                Visible-item snapshot data
    filter_bar/
      state.rs                   Filter snapshot and filtered model cache
    icons/
      cache.rs                   FileIconCache, MIME candidate, theme resolution
    location_bar/
      draft.rs                   Editable location draft and caret state
      metrics.rs                 Editable metrics, hit-test, scroll math
    pane/
      snapshot.rs                Pane rendering snapshot
      splitter.rs                Splitter drag payload and ratio geometry
    places/
      model.rs                   Place entry, grouping, icon snapshots
      snapshot.rs                Place icon and snapshot types
    properties_dialog/
      metadata.rs                File metadata reader and row generation
    rename/
      draft.rs                   Pane-local rename draft state
      metrics.rs                 Rename caret hit-test and text inset metrics
    rubber_band/
      state.rs                   Rubber-band drag payload and rect projection
    scrollbar/
      drag.rs                    Pane-local scrollbar drag state and app routing
      element.rs                 GPUI scrollbar element, hitbox and drag handlers
      geometry.rs                Scrollbar bounds, hit-test and scroll mapping
    status_bar/
      state.rs                   Snapshot, space info cache, progress handle
      summary.rs                 Pane selection/model summary formatting
  bin/
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser backend
    fika-privileged-helper.rs    System-bus privileged helper
```

Root `Cargo.toml` is a single package. It exposes the `fika_core` library from
`src/lib.rs` (via `src/core.rs`) and builds the `fika`, `fika-xdp-filechooser`,
and `fika-privileged-helper` binaries from `src/main.rs` and `src/bin/`. GPUI
is sourced from the official Zed repository with a git dependency and no numeric
crate release pin.

## Core Model

### Pane

`PaneState` is a core object, not a UI slot. It owns:

- `PaneId`
- `generation`
- `current_dir`
- `DirectoryModel`
- `DirectoryLister`
- watcher state

Opening or closing split panes creates or drops pane state. It must not clone global UI state or share watcher state.

### Directory Lister

The lister mirrors Dolphin's `KDirLister -> KFileItemModel` boundary.

Inputs:

- load directory
- reload current directory
- watcher refresh
- current-directory-removed detection

Outputs:

- `LoadingStarted`
- `ItemsAdded`
- `ItemsDeleted`
- `ItemsRefreshed`
- `ListingCompleted`
- `CurrentDirectoryRemoved`
- `Error`

All outputs carry `PaneId`, `generation`, and path context so stale events can be rejected.

### Directory Model

`DirectoryModel` owns entries and emits model signals:

- keep the previous listing visible on `LoadingStarted`
- reset/replace only when the current request delivers a new `ListingRefreshed`
- insert item ranges
- delete item ranges
- refresh item ranges
- report loading/error state

The GPUI pane consumes snapshots and signals. It does not decide whether a filesystem event is an add, delete, refresh, or full reload. During navigation it cancels transient interactions but retains the old model/layout until the new listing is ready, matching Dolphin's no-blank-frame loading behavior.

### Listing Worker and Cache

`ListingWorkerState` is a per-app singleton that receives listing requests keyed
by `(path, mode)`. Requests from multiple panes showing the same directory are
coalesced into a single `read_dir`. Results are shared as `Arc<Vec<Entry>>` and
retargeted to each requesting pane with pane-local `ModelEntry` identity.

`DirectoryCache` stores fresh listing results keyed by canonical path. On a
`Load` request the cache returns a cached `ListingRefreshed + ListingCompleted`
pair without queuing a background `read_dir`. Entries are evicted via LRU with
per-directory and total entry budgets. Stale cache is not used for `Load`, and
`Reload` marks the entry as stale.

### Location Resolution

`src/core/location.rs` provides path normalization used by the startup argument
parser, location bar input submission, Places Add/Edit, and Tab completion:

- `expand_user_path()` — `~` expansion
- `normalize_start_dir()` — absolute path resolution with home fallback
- `resolve_location_input()` — absolute, relative, and `~` input
- `complete_location_input()` — filesystem Tab completion
- `breadcrumb_segments()` — breadcrumb segment model
- `home_dir()` — `$HOME` lookup

### MIME Detection

`src/core/mime.rs` reads the system shared-mime-info database (`globs2`,
`icons`, `generic-icons`) and provides:

- Literal filename matching (highest priority)
- Multi-suffix matching
- Extension-only matching
- Common magic-byte sniffing (only as fallback for `application/octet-stream`)
- MIME-specific icon and generic icon lookup

### Application Launcher

`src/core/launcher.rs` parses `.desktop` files (`Desktop Entry`, `Desktop
Action`, `MimeType=`, `Exec` field codes) and `mimeapps.list` (Default, Added,
Removed Associations). Open With application lists are sorted by
`mimeapps.list` priority with removed associations filtered out. `.desktop`
`MimeType=` wildcards (e.g., `image/*`) are honored before parent MIME
fallback. Application launch plans are executed as systemd user transient units
via session bus `StartTransientUnit()`.

### Service Menu

`src/core/launcher.rs` also scans dedicated KDE/Fika service-menu directories
and parses `Type=Service` desktop files with `X-KDE-ServiceTypes=`. Conditions
including `X-KDE-Protocols`, `X-KDE-RequiredNumberOfUrls`,
`X-KDE-ShowIfExecutable`, `X-KDE-Priority=TopLevel`, and `X-KDE-Submenu` are
evaluated in core. TopLevel actions are promoted to the root context menu;
`X-KDE-Submenu` actions are rendered as nested submenus under "More Actions".

### Trash

`src/core/file_ops.rs` implements XDG Trash:

- `trash_file()` — unique trash name, `.trashinfo` with `Path=` and
  `DeletionDate=`, move to `files/`
- `restore_file()` — read `.trashinfo`, move back, overwrite-conflict dialog
- `delete_permanently()` — remove `files/` entry and corresponding `info/`
- `empty_trash()` — clear all `files/` and orphaned `info/` entries

Trash-model entries carry `trash_original_path` and `trash_deletion_time`, and
the model supports `TrashOriginalPath` and `TrashDeletionTime` sort roles.

### Thumbnails

`src/core/thumbnails.rs` implements the freedesktop thumbnail specification:

- Thumbnail URI derivation from file path and modification time
- Cache key generation and cache-hit checking
- Failure marker handling
- `EntryData` path role for thumbnail resolution on visible items

### Devices

`src/core/devices.rs` subscribes to UDisks2 signals (`InterfacesAdded`,
`InterfacesRemoved`, `PropertiesChanged`) via the unified bus layer and
maintains mount-info snapshots. `src/core/devices/actions.rs` provides
mount/unmount/eject/safely-remove async operation dispatch with progress,
success, and error messaging. Removable devices are projected into a dynamic
"Removable Devices" section in Places, isolated from user bookmark persistence.

### Network

`src/core/network.rs` classifies filesystem types (GVfs, remote, FUSE) and
resolves Network root paths. The Places sidebar includes a Network section
populated from active remote mounts.

### Bus Control

`src/core/bus.rs` provides a unified D-Bus abstraction:

- `BusKind` (Session / System)
- `BusController` with lazy connection, idle timeout (30s), and
  method-call timeout/retry (3 attempts)
- Structured `BusError` with service name, method name, and error details
- Owned proxy creation for session and system bus
- Routing for UDisks2 signals, systemd transient units, Portal registration,
  privileged-helper operations, and Ark DnD extraction

### Async Runtime Architecture

Fika uses a dual-runtime design:

- `tokio` — multi-threaded runtime for general async: D-Bus, process launch,
  network, watcher callbacks
- `compio` — completion-based file I/O (`io_uring` on Linux, polling fallback)

Runtimes are independent and do not share futures. Cross-runtime data transfer
uses channels. The `io-uring` feature is Linux-only and can be disabled for
other platforms.

## GPUI Layer

The GPUI shell owns:

- window creation through `gpui_platform::application()`
- pane toolbar actions
- split/close/focus routing by `PaneId`
- directory item rendering (compact file grid with slot-pool virtualization)
- scrollbar, rubber-band, and overlay rendering
- location bar (breadcrumb + editable text modes)
- status bar (summary, space info, zoom slider, progress bar)
- filter bar (plain-text/glob toggle, match count)
- Places sidebar (bookmarks, devices, network)
- context menu (target/action model, Open With, service menus, nested submenus)
- drag and drop (item/place source, directory/pane target, external paths)
- clipboard interaction (Copy/Cut/Paste with progress, primary-selection paste)
- inline rename, including pane-local draft state and watcher-rename retargeting
- properties dialog
- application chooser ("Other Application…" with `uniform_list`)
- watcher polling handoff into core events
- pane-local selection, navigation shortcuts, and manager actions
- background file-operation tasks that return affected directories
- chooser path output and portal metadata output

Rendering is intentionally thin. Feature work should move domain logic into
`fika-core` first, then expose it through GPUI actions.

### Key UI Components

#### File Grid (`src/ui/file_grid.rs`)

The file grid renders a compact (column-first) view of directory items using
GPUI declarative elements (`div`, `img`, text). Virtualization is achieved
through three layers:

1. **Layout math** (`src/core/view.rs`): `CompactLayout::visible_items()`
   computes only items intersecting the viewport.
2. **Slot pool** (`src/ui/file_grid/slots.rs`): `VisibleItemSlotPool` recycles
   element IDs from off-screen items, capped at 100.
3. **GPU-composited scroll**: Content translation via
   `left(-scroll_x) / top(-scroll_y)` avoids layout recalculation on scroll.

Active inline rename drafts add a pane-local text-width override to the compact
column metrics. Snapshot generation, item hit-testing, rubber-band visual
intersection and rename caret placement all consume that same expanded layout,
so a long draft name can widen the editor without desynchronizing mouse
geometry.

Inline rename text editing follows normal text-field selection semantics inside
`src/ui/rename/draft.rs`: the initial selection covers the file stem, plain
Left/Right collapse an existing selection to its start/end, Shift+Left/Right and
Shift+Home/End extend the selection from the current anchor, and
Ctrl/Secondary+A selects the full draft name including extension.
The file-grid renderer keeps the inline editor visually tied to the filename
line: only the stable name row receives the text-field border/background,
selection highlight and caret, while the kind/error/extension-warning helper
text stays in the existing helper row below it.

#### Scrollbar (`src/ui/scrollbar.rs` + `src/ui/scrollbar/*`)

The horizontal scrollbar is isolated behind the `src/ui/scrollbar.rs` entry
module:

- `scrollbar/geometry.rs` owns track normalization, window/local hit-testing
  and handle-to-scroll mapping.
- `scrollbar/drag.rs` owns `ActiveScrollBarDrag` and pane-local
  begin/update/finish routing on `FikaApp`.
- `scrollbar/element.rs` owns the GPUI scrollbar element, handle painting,
  prepaint hitbox insertion, cached-track publication and paint-phase
  down/move/up handlers.
- The canvas inserts a prepaint `HitboxBehavior::BlockMouse` hitbox for the
  actual rendered track and converts those bounds to a window-space track rect.
  Prepaint publishes that rect as the pane-local current scrollbar track.
- The canvas registers capture-phase mouse down/move/up handlers during paint.
  Left down starts from that frame's live window-space track rect only when the
  pointer is inside the measured 12px strip and no modal mouse overlay is
  active. Move events are routed by pane-local active drag state, not GPUI DnD
  or GPUI pointer capture, and update scroll from the original window-space
  track rect even after the pointer leaves the strip. This avoids stale hitbox
  capture after scroll changes trigger a redraw and recreate the canvas hitbox.
- `src/ui/file_grid.rs` no longer creates or owns the scrollbar slot. It only
  renders the item viewport and item interactions. `src/ui/pane.rs` owns the
  scrollbar slot as a direct pane-shell sibling below the file-grid viewport,
  keeping wheel and navigation side-button routing outside the item/blank-area
  event tree.

#### Location Bar (`src/ui/location_bar.rs`)

Two modes:

- **Breadcrumb mode**: Rendered with GPUI declarative `div` elements; each
  segment is clickable for navigation.
- **Editable mode**: Uses `canvas()` for text rendering with caret and
  horizontal scroll; Tab completion queries the filesystem via core
  `complete_location_input()`.

#### Status Bar (`src/ui/status_bar.rs`)

Per-pane status bar showing:

- Selection summary (item count and total size)
- Free space on the filesystem of the current directory
- Zoom slider (draggable horizontal track)
- Progress bar with Stop button (for file operations and directory loading)

#### Places Sidebar (`src/ui/places.rs`)

Sections: Home, XDG user dirs, Trash, Removable Devices, Root, Network.

- User bookmarks persist to `user-places.xbel`.
- Device sections are dynamically populated from UDisks2 signals.
- Right-click context menu supports Open, Open in New Pane, Add/Edit/Remove
  bookmark, Copy Location, Properties, and Empty Trash.
- Drag-and-drop: dragging from Places to pane navigates; dragging to Places
  inserts bookmarks or performs file operations.

#### Context Menu (`src/ui/context_menu.rs`)

Generates Dolphin-style context menus with:

- Root actions (Open, Open in New Pane, Cut/Copy/Paste, Rename, Move to Trash,
  Delete Permanently, Properties, Compress/Extract)
- Create New submenu
- Open With dynamic submenu (sorted by `mimeapps.list` priority, with "Other
  Application…" chooser)
- Service-menu actions (from KDE/Fika service directories)
- Sort By submenu (with Trash-specific roles)
- Menu positioning with viewport clamp and flip

#### Drag and Drop (`src/ui/drag_drop.rs`)

Supports:

- Internal item drag (pane to pane, pane to Places, Places to pane)
- Prepared external item/place drag payloads (`text/uri-list` and `text/plain`)
- External file drop (`ExternalPaths`)
- Modifier-based mode switching (no modifier = Copy, Shift = Move,
  Shift+Ctrl = Link)
- Color-coded drop targets (Copy green, Move amber, Link purple)
- Insertion indicators for Places bookmark reorder

## Async and Stale Result Policy

Every pane-scoped async result must include:

- `PaneId`
- `generation`
- source path or operation id

Apply path:

1. Receive event.
2. Resolve pane by `PaneId`.
3. Check generation and path.
4. Apply to core model.
5. Notify GPUI view.

No pane-scoped async result may apply by focused pane.

## Undo and File Operation Policy

File operations belong in core. UI actions should produce operation requests; operation completion should return affected directories and trigger lister refresh for panes that show those directories.

Undo follows the same rule: filesystem change first, affected pane refresh second, no manual item-view rebuild in the UI layer.

## Historical Docs

The archived optimization documents (`docs/OPTIMIZATION.md`,
`docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md`,
`docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md`,
`docs/GPUI_DOLPHIN_MIGRATION_PLAN.md`) describe earlier planning phases and
should be read only for behavior notes and design history. They are not
architecture input for new code.

## Acceptance Definition

The GPUI architecture is acceptable when:

- single pane and split pane refresh correctly from external filesystem changes
- closing a pane drops its lister/watcher and cannot receive stale results
- two panes showing the same directory have independent generation and watcher state
- current-directory-removed uses nearest existing ancestor fallback
- portal and privileged-helper binaries build from the root package
- the main build has no dependency on the removed UI implementation
- all file operations report affected directories and support undo
- context menu, drag-drop, clipboard, and keyboard shortcuts route by `PaneId`
