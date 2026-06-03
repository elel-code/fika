# Fika TODO

本文档按实现顺序组织。每个任务都包含验收标准，后续实现时逐项更新状态。

状态说明：

- `[x]` 已完成
- `[~]` 部分完成
- `[ ]` 未开始

## Phase 0: Current Baseline

- [x] Slint 1.16.1 pinned in `Cargo.toml`.
- [x] Slint master experimental layout gate enabled for scoped FlexboxLayout use.
  - Current: `build.rs` enables Slint's experimental compiler registry so Fika can use `FlexboxLayout` from the master branch without requiring users to export `SLINT_ENABLE_EXPERIMENTAL_FEATURES` manually.
  - Current: FlexboxLayout is limited to local responsive control rows; the main file view keeps its deterministic column-first virtual layout.
- [x] UI entry lives in `ui/app.slint` and is compiled through `build.rs`; shared models/widgets/file tiles are split into focused `.slint` files.
- [x] Dolphin-like shell: toolbar, Places sidebar, main icon area, status bar.
- [x] COSMIC-style shell surface layering outside the main file arrangement.
  - Current: `AppWindow` owns one shared base surface with a separate window-wide shell/header row.
  - Current: `TopBar`, `PathBar`, `SearchPanel`, and `StatusBar` render transparent backgrounds, so they read as one layer with the main pane.
  - Current: `TopBar` lives in the shell/header row, owns global search/split/theme controls, and deliberately does not draw a bottom separator between the shell tool area and main content.
  - Current: below the shell/header row, the rounded sidebar panel and right main pane share one equal-height content row; the navigation/address `PathBar` is the first row inside that right main pane.
  - Current: the main-pane item arrangement intentionally remains Fika's existing Dolphin-like column-first horizontal layout.
- [x] Dark mode.
- [x] Resizable sidebar.
- [x] Column-first icon layout.
- [x] Main-view tile virtualization.
  - Current: Slint receives only `entry_count` and the visible `virtual_entries` slice, not the full filtered file model.
  - Current: filtering/search rebuilds a lightweight visible-index cache once; normal unfiltered directories use an implicit identity fast path.
  - Current: viewport changes clone only the requested virtual range through that visible-index cache, avoiding repeated full filtered-model allocation during horizontal scrolling.
  - Current: virtual slice preparation lives in `src/app/virtual_view.rs`, so range planning, viewport clamping, rebuild decisions, filtered slicing, and thumbnail-cache decoration are testable away from Slint property updates.
  - Current: ScrollView keeps a stable full-width virtual content layer for scrollbar geometry, while rendered tiles live in a local slice layer anchored at the first virtualized column, so large directories avoid both scrollbar-width churn and huge per-tile global coordinates.
  - Current: virtual range metadata is cached; scrolling inside the same range does not reset the Slint model.
  - Current: virtual range reuse now keeps the current Slint model while its cached overscan slice still covers the newly visible columns, and ScrollView viewport writeback ignores sub-pixel drift, reducing large-directory horizontal jitter during small scroll steps.
  - Current: ordinary wheel scrolling requests pane focus only once per event path before panning, while Ctrl+wheel still focuses before zooming.
  - Current: Rust uses a tested `VirtualGridPlan` to calculate clamped viewport position, scroll extent, visible range, overscan range, and Slint anchor column from one source of truth.
  - Current: offscreen thumbnail completions update the cache without resetting the Slint model; visible completions still refresh the current virtual slice.
  - Current: thumbnail scheduling for each virtual-slice sync is capped and owned by `src/app/thumbnail_pipeline.rs`, so large directories cannot enqueue an unbounded number of decode jobs from a single viewport update.
  - Current: rectangle selection narrows candidates to the intersecting column range before resolving paths, so large directories do not scan every visible result for a local drag box.
- [x] Ctrl+wheel zoom for main file tiles.
- [x] Click blank area clears selection and releases LineEdit focus.
- [x] Ctrl multi-select.
- [x] Right-click folder menu with `Add to Places`.
  - Current: `Add to Places` is hidden when the folder path already exists in Places, matching Dolphin's single-directory action behavior.
- [x] Places persistence.
- [x] Places drag reorder with ghost preview and insertion line.
- [x] Internal folder drag from main view into Places.
- [~] Devices sidebar.
  - Current: Devices is Rust-driven rather than hard-coded in Slint, and lists `Filesystem` plus mounted paths under `/run/media/$USER`, `/media/$USER`, `/media`, and `/mnt`.
  - Current: discovery is now mount-table-first via `/proc/self/mountinfo`, closer to Dolphin's KMountPoint/Solid and cosmic-files' mounter-item model than raw directory scanning. Mountinfo parsing keeps source and filesystem type so pseudo filesystems such as `tmpfs` do not appear as removable devices. Directory scanning remains only as a fallback when mountinfo is unavailable.
  - Current: UDisks2 system-bus `ObjectManager` discovery is used as a best-effort enhancement for user-visible external media, including unmounted filesystem-backed volumes. It accepts media-backed drives that are marked removable, media-removable, ejectable, optical, attached over the USB bus, or advertised through UDisks2 `MediaCompatibility` as optical/flash media, while still respecting UDisks2 hidden/system block hints and filtering empty media slots. Raw removable block devices without a `Filesystem` interface are filtered out at discovery time so the sidebar does not advertise devices Fika cannot mount/open. Mounted mountinfo entries stay first and win duplicate paths, but UDisks2 still fills in operation metadata such as `/dev/...` `device_path` and eject support for those duplicate rows. UDisks2 failures fall back silently to mountinfo/directory discovery.
  - Current: UDisks2 display names follow the user-visible volume/mount identity first: desktop-provided `Block.HintName` wins, then explicit filesystem labels, mounted media without a label uses the mount-point name, and unmounted unlabeled media falls back to the drive vendor/model before the raw device path. Sidebar markers now derive simple device semantics first (`USB`, `SD`, `CD`) before falling back to the label initial, matching the Dolphin/Solid and cosmic-files mounter direction of carrying device type/icon metadata instead of only text labels.
  - Current: `DeviceEntry.kind` now classifies rows as `filesystem`, `local-mount`, or `removable-media`. This is intentionally closer to cosmic-files' mounter-item model than a pure path list: mountinfo/root-scan fallback rows stay stable, while duplicate UDisks2 media can upgrade the same row to removable semantics for future icon/menu behavior.
  - Current: clicking an unmounted UDisks2 filesystem device starts an async `Filesystem.Mount({})`; success refreshes Devices and opens the returned mount point, while failures are shown in the status bar.
  - Current: device rows have a right-click menu with Mount for unmounted media, Open/Unmount for mounted media, and Eject when UDisks2 reports an ejectable drive. These actions run off the UI thread and refresh Devices after completion.
  - Current: device menu actions are driven by explicit `can_mount`, `can_unmount`, and `can_eject` capabilities. The root Filesystem row and mountinfo-only fallback rows remain openable but no longer advertise UDisks2 actions that the backend cannot perform.
  - Current: pending device actions are tracked per `device_path`, so repeated clicks on the same device do not queue overlapping Mount/Unmount/Eject D-Bus calls. Pending rows now render a distinct blue in-progress state, and their right-click menu collapses to a disabled Mounting/Unmounting/Ejecting status row until the action finishes.
  - Current: common UDisks2 D-Bus errors such as busy devices, authorization failures, already-mounted, not-mounted, cancellation, and timeout are mapped to status-bar guidance while retaining the raw error name/detail for diagnostics.
  - Current: failed device actions are retained per device and rendered as a distinct sidebar error state, so the affected row stays visually marked after the status message changes. A later successful action for that device clears the marker.
  - Current: after a successful Unmount/Eject, if the current main view is inside that device's previous mount point, Fika moves the view back to Home and prunes history entries under the removed mount path, matching Dolphin/cosmic-files' avoid-stale-location behavior.
  - Current: Devices discovery runs through the async event bridge; `/proc/self/mountinfo` parsing and UDisks2 system-bus discovery no longer execute on the UI thread, and stale device-list generations are ignored.
  - Current: Devices now has a background monitor. UDisks2 system-bus signals trigger debounced refreshes, and a low-frequency snapshot poll catches missed mount-table or desktop-backend changes.
  - Current: `FIKA_DEBUG_DEVICES=1` prints device discovery and monitor diagnostics, including mountinfo/fallback usage, UDisks2 accepted rows, UDisks2 skip reasons, monitor refresh reasons, a single-line discovery summary, mountinfo-only / UDisks2-only / merged row counts, semantic markers, and the final merged sidebar device list.
  - Current: `scripts/check-runtime-integration.sh` now includes a Devices runtime probe that reports UDisks2 service state, system-bus availability, ObjectManager visibility, and Block/Drive/Filesystem interface counts without performing mount, unmount, or eject operations.
  - Current: `fika --diagnose-devices` prints the same Rust-discovered Devices rows and capabilities used by the UI, plus discovery summary, merge stats, and any UDisks2 discovery error, without creating a Slint window or performing device operations.
  - Remaining: distro validation for UDisks2/polkit edge cases.
- [x] Internal drop transfer menu with Move / Copy / Link actions.
- [x] F5 active refresh; toolbar refresh button removed.
- [x] Mouse Back/Forward navigation with history stacks.
- [x] Built-in MIME/default app launcher without `xdg-open`.
- [x] Tokio runtime initialized.
- [x] Async directory loading.
- [x] Async file open / MIME probing via `spawn_blocking()`.
- [x] Central async event enum for UI-thread result dispatch.
- [x] Event-loop wakeup for async results without polling timer.
- [x] Current directory watcher with debounced refresh.
- [x] ICU4X locale workaround for Slint 1.16.1 segmentation data limitations.
- [x] Split Rust code into focused nested modules for config, desktop integration, filesystem logic, and support helpers.
- [x] Directory cache for instant redraw on previously visited folders.
  - Current: directory entries are cached with an LRU cap so long browsing sessions cannot keep every visited directory's full entry list forever.
  - Current: directory scans run as one background blocking scan, following COSMIC Files' local-directory pattern instead of scheduling per-entry async filesystem operations.
  - Current: watcher/manual refresh results that are visibly unchanged only refresh the cache and status text; they do not reset the Slint model or virtual range.
  - Current: uncached directory navigation keeps the previous visible model in place and defers restoring the target directory viewport until the new entries are ready, matching the COSMIC-style separation between location change and item replacement more closely. The stale model is inert during the transition, so old tiles cannot be opened, selected, dragged, or used as drop targets after the path has changed.
  - Current: directory results update the backing entries, virtual model, `items_path`, and finally `directory_loading`, so the old stale model is not made interactive before the replacement slice is committed.
  - Current: directory read failures no longer clear a committed main-view model. Refresh/cached refresh failures keep the current or cached view and report the error in the status bar; failed uncached Places/Devices navigation rolls back `current_path` to the last committed `items_path` instead of leaving the sidebar and main pane on an unreadable empty target.
  - Current: Places paths and mounted Devices paths are prefetched into the same bounded directory cache in the background, so sidebar jumps are more likely to use the instant cache-hit path instead of showing a long stale-view transition.
  - Current: sidebar prefetches keep a pending-path set, so repeated Places/Devices refreshes do not dispatch duplicate scans for the same uncached directory before the first scan completes.
  - Current: Devices refresh is decoupled from ordinary directory loading. Startup, the device monitor, and explicit Mount/Unmount/Eject outcomes refresh Devices, while Places/Devices navigation no longer rebuilds the sidebar device model on every directory load.
- [x] Dolphin-style delayed main-view clearing for uncached directory navigation.
- [x] Per-directory main-view scroll position memory.
  - Current: remembered view states are cached with an LRU cap so long browsing sessions cannot keep every visited path's viewport state forever.
- [x] Removed local `icu_segmenter` vendor patch after the upstream ICU4X segmentation warning fix landed.
- [x] Mouse Back/Forward scoped to the right-side main pane only.
- [x] Adaptive Open With hover submenu placement.
- [x] Use Slint master `DragArea` / `DropArea` built-ins without the experimental compiler flag.
- [x] Dolphin-style DnD self-drop rejection.
- [x] Focused Slint split: `models.slint`, `widgets.slint`, and `file_tile.slint`.
- [x] Focused Rust split for selection and Places UI logic.
- [x] Dolphin-style context menu hover polish.
  - Acceptance: submenu rows use explicit child indicators, child menus keep a timed grace area between parent and child, and drop operation menus include Cancel.
  - Current: parent menu, Open With, and Create New delayed-close handling is centralized in `MenuLifecycleController`, so delay/keep-alive behavior is no longer repeated across individual menu callbacks or owned directly by `AppWindow`.
  - Current: Open With and Create New now share one child-submenu hover/timer entrypoint; parent rows, hover bridges, and child menu bodies all use the same keep-alive contract.
  - Current: Open With and Create New child menu panels also include a reusable `MenuHoverRegion`, so moving across separators, list edges, or menu padding does not immediately start submenu dismissal.
  - Current: Open With now behaves like a normal child menu: no extra title row, first application row aligns with the parent submenu row, the app list is capped to 7 visible rows, and `Other Applications...` stays as the final fixed action.
  - Current: Open With and Create New also share one active child-menu hover bridge instance, so `ui/app.slint` no longer carries duplicate bridge layout for the two submenu types.
  - Current: `ChildSubmenuLayer` now owns child submenu sizing, child placement, and hover-bridge placement inputs, so `ui/app.slint` no longer carries the active child-menu / bridge intermediate layout properties.
  - Current: Open With and Create New open/close state, row anchors, and pending delayed-close kind now live in the `MenuLifecycle` Slint global, while `MenuLifecycleController` owns the 240ms delayed-close timer and hover/show helpers. `AppWindow` still prepares business data such as Open With candidates, but no longer owns the child-submenu state fields or timer directly.
  - Current: file item, Open With, Create New, Transfer, sidebar Places, Devices, Places blank-area, and main viewport context menus own their `PopupSurface` framing in `ui/menus.slint`, reducing repeated popup wrapper layout in `ui/app.slint`.
  - Current: root file, Places, Devices, Places blank-area, and main viewport context menu hosting is centralized in `RootContextMenuLayer`, so `ui/app.slint` keeps action wiring while `ui/menus.slint` owns the repeated root-menu placement shell.
  - Current: `RootContextMenuLayer` also owns root-menu width/height selection, flip/clamp placement, and Open With / Create New parent-row anchors before forwarding submenu hover/click events, so `ui/app.slint` no longer carries duplicate root-menu geometry properties.
  - Current: ordinary root-menu action rows do not forward hover events, so moving over a non-submenu item does not start child-submenu dismissal; only `SubmenuMenuRow`, child popup hover regions, hover bridges, and child-menu `HoverActionMenuRow` rows participate in keep-alive / delayed-close behavior.
  - Current: chooser filter and choice popups are hosted through `ChooserOptionPopupLayer` / `ChooserChoicePopupLayer`, keeping the popup loops and anchored placement formulae out of `ui/app.slint`.
  - Current: chooser filter and choice popups now share an internal `AnchoredChooserPopup` shell in `ui/menus.slint`, so their above-anchor flip/clamp placement formula is defined once instead of repeated across both chooser popup layers.
  - Current: menu geometry callback access now lives in the `MenuGeometry` Slint global (`ui/menu_geometry.slint`), and child submenu lifecycle state lives in `MenuLifecycle` (`ui/menu_lifecycle.slint`), so `ui/app.slint` no longer forwards placement callbacks or directly owns child-submenu open/anchor state.
- [x] Dolphin/QMenu-style menu placement.
  - Acceptance: root, child, and transfer menus share preferred-point, flip, and clamp placement rules; child hover bridge follows the clamped submenu position.
  - Current: Escape and outside-click dismissal close both the parent context menu and any open child submenu together, avoiding orphaned child menus.

## Phase 1: Stabilize Async Core

- [x] Replace 16ms polling timer with an event wakeup strategy if Slint exposes a stable cross-thread invoke API suitable for this project.
  - Acceptance: async results are applied promptly without continuous idle polling.
  - Notes: keep current timer if the alternative complicates Slint 1.16.1 compatibility.

- [x] Add a central async event enum.
  - Acceptance: directory load, file open, watcher, thumbnail, operation events all share one receiver.
  - Suggested type: `enum AsyncEvent { DirectoryLoaded(...), FileOpened(...), DirectoryChanged(...), ThumbnailReady(...), OperationUpdated(...) }`.

- [x] Add cancellation/generation helper utilities.
  - Acceptance: every async pipeline has a clear stale-result policy.
  - Current generation fields should be consolidated when more pipelines are added.

## Phase 2: Directory Monitoring

- [x] Add current-directory watcher.
  - Acceptance: creating, deleting, renaming, or modifying files in the current directory causes the view to refresh.
  - Acceptance: switching directory stops or invalidates the old watcher.

- [x] Debounce watcher events.
  - Acceptance: a burst of filesystem events triggers one refresh.
  - Suggested delay: 100-250ms.

- [x] Preserve selection where possible after refresh.
  - Acceptance: if a selected path still exists after reload, it remains selected.
  - Acceptance: removed paths are dropped from selection.

## Phase 3: Main View Selection

- [x] Shift range selection.
  - Acceptance: click item A, Shift+click item B selects the visual range between them.
  - Acceptance: range follows current column-first ordering.

- [x] Ctrl+A select all visible items.
  - Acceptance: only filtered/visible items are selected.

- [x] Esc clears selection and context menu.
  - Acceptance: Esc closes context menu first; if no menu is open, clears selection.

- [x] Drag rectangle selection.
  - Acceptance: dragging over main blank area displays a selection rectangle.
  - Acceptance: intersected visible tiles become selected.

- [x] Multi-selection context menu cleanup.
  - Acceptance: right-clicking an already selected item while multiple items are selected opens a batch-safe menu.
  - Acceptance: single-item actions such as Rename, Open With, and Add to Places are hidden until batch equivalents exist.

## Phase 4: Places Management

- [x] Right-click Places menu.
  - Acceptance: user-added places show Rename, Remove, and Open in New Window backed by a real launch path.
  - Acceptance: built-in places do not expose destructive actions unless explicitly supported.
  - Current: Open in New Window launches a new Fika process in a systemd user scope when systemd user D-Bus is available, reports non-fatal scope diagnostics in the status bar, and records the unit name in app state.

- [x] Rename place.
  - Acceptance: label updates immediately and persists to `places.tsv`.

- [x] Remove place.
  - Acceptance: item disappears and persists.
  - Acceptance: removing the current place does not change current directory.

- [x] Restore default places.
  - Acceptance: user can restore Home/Desktop/Documents/etc. without manually editing config.
  - Current: Restore Defaults is available from the Places blank-area context menu.
  - Current: the Places blank-area context menu also offers Add Current Folder when the current directory is not already in Places.

- [x] Cross-application DnD fallback removal.
  - Acceptance: the current project scope only supports app-internal Slint `DragArea` / `DropArea` payloads.
  - Acceptance: external desktop payloads such as `text/uri-list` and `text/plain` are rejected as unsupported instead of being parsed through a second native-window path.
  - Acceptance: no winit, X11, or native-window fallback is kept for drag and drop.
  - Current: internal file/folder/place drags already use the same target highlighting and transfer-menu rules for Places, main-pane blank space, and main-pane folders.
  - Future: once Slint exposes stable cross-application DnD, reopen this as a new design pass for external folder/file drops.

- [x] Drag folder from main view into Places.
  - Acceptance: dropping into the gap between Places items inserts a new Place at that slot.
  - Acceptance: dropping onto an existing Place opens the transfer menu.

- [x] Drag Places item into main view.
  - Acceptance: dropping into the main view opens the transfer menu targeting the current folder.
  - Acceptance: dropping onto a folder tile targets that folder using release-coordinate geometry rather than hover delivery.

## Phase 5: Thumbnail Pipeline

- [x] Add thumbnail model fields.
  - Acceptance: `FileEntry` can represent thumbnail state without breaking existing icon fallback.

- [x] Async image thumbnail generation.
  - Acceptance: PNG/JPEG/WebP files eventually show thumbnails.
  - Acceptance: failed thumbnails fall back to generic icon.

- [x] Thumbnail cache.
  - Acceptance: repeated visits reuse cached thumbnails when size/mtime match.
  - Acceptance: thumbnail memory use does not grow without bound while browsing many image directories.
  - Current: cache entries are capped and the oldest entry is evicted when the limit is exceeded.
  - Current: inspired by `cosmic-files/src/thumbnail_cacher.rs`, failed thumbnail attempts are also cached by path, mtime, and target size with capped LRU eviction, so broken/unsupported images do not repeatedly enqueue decode jobs while scrolling large directories.
  - Current: thumbnail loading also reads and writes freedesktop.org disk cache PNGs under `normal` / `large` / `x-large` / `xx-large`, and writes `fail/fika-$version` markers after decode errors so repeated visits do not decode the same broken image again until the source mtime changes.
  - Current: thumbnail load results update only the shared thumbnail success/failure caches and the active pane's pending map; visible tiles are refreshed by re-decorating the virtual slice, avoiding a full `entries` scan for every completed thumbnail.
  - Current: same-directory refresh and watcher reload preserve the active thumbnail generation and pending jobs, following COSMIC Files' item/thumbnail separation; full navigation still cancels stale thumbnail work.

- [x] Visible-first scheduling.
  - Acceptance: thumbnails visible in the viewport are generated before offscreen items.
  - Current: only the current virtual slice plus overscan is scheduled; stale thumbnail results clear only their matching pending key, so viewport changes or zoom changes cannot leave an item permanently stuck as pending.
  - Current: thumbnail jobs for the actually visible columns are queued before left/right overscan thumbnails, keeping large-directory scrolling responsive when many image previews are pending.
  - Current: the visible-first scheduler now owns duplicate suppression, pending-state marking, and the per-view-sync job cap in `src/app/thumbnail_pipeline.rs`; `main.rs` only submits the prioritized virtual slice and starts the returned async jobs.
  - Current: viewport-only thumbnail scheduling reuses the active directory/zoom generation instead of invalidating in-flight thumbnail work on every scroll.
  - Current: refresh/reload also reuses the active thumbnail generation, so in-flight visible thumbnails are not thrown away while directory entries are refreshed.

## Phase 6: File Operations

- [x] New folder.
  - Acceptance: creates a folder in current directory and refreshes view.
  - Acceptance: name collision is handled.

- [x] Rename file/folder.
  - Acceptance: inline or dialog rename works.
  - Acceptance: errors are shown in status or operation panel.

- [x] Trash.
  - Acceptance: selected files move to trash when possible.
  - Acceptance: hard delete is not the default.

- [x] Copy/move/link drop operations.
  - Acceptance: Move, Copy, and Link from internal drop menu run off the UI thread.
  - Current: operations are queued and run off the UI thread; existing target names open a conflict dialog with Overwrite, Keep Both, Rename, and Skip.

- [x] Internal file clipboard.
  - Acceptance: item and multi-selection context menus expose Cut and Copy.
  - Acceptance: current-folder and folder-item menus expose Paste when the internal clipboard has paths.
  - Current: paste reuses the existing async copy/move operation queue and privileged fallback.
  - Current: Ctrl+C, Ctrl+X, Ctrl+V, Ctrl+Z, and Delete are declared with Slint `KeyBinding`; they operate on the selected files/current directory/last undo entry only when menus, dialogs, and text inputs are not active.
  - Current: Cut / Copy also publishes `x-special/gnome-copied-files` to the Wayland desktop clipboard through `wl-copy` when available; Copy falls back to `text/uri-list` if the desktop helper cannot publish the GNOME file-list MIME type.
  - Current: startup and context-menu entry trigger an asynchronous `x-special/gnome-copied-files` / `text/uri-list` clipboard refresh through `wl-paste`; Paste visibility uses the cached result instead of synchronously reading the clipboard from transient menu handling. Ctrl+V / Paste still performs a synchronous fallback read only if the cache is empty.
  - Current: when importing `text/uri-list`, Fika also checks KDE/Dolphin's `application/x-kde-cutselection` marker so Dolphin Cut pastes as a move rather than a copy.
  - Current: when a desktop file clipboard is available, the cached desktop clipboard replaces Fika's older internal clipboard state, so external Copy/Cut actions take precedence over stale in-app selections. Clipboard refreshes use a generation counter, so stale background reads cannot overwrite a newer Fika Cut/Copy.
  - Current: internal and imported desktop clipboard paths are deduplicated while preserving order, so Paste does not enqueue duplicate transfers for the same source.
  - Current: context-menu clipboard refresh and Paste both validate clipboard paths before exposing or queueing transfers, drop entries that no longer exist, and clear the Paste affordance if all clipboard items are stale.
  - Current: Paste counts only transfers accepted by the transfer layer; rejected self/descendant, missing-source, or invalid-target entries do not inflate the queued count, and Cut clipboard state is cleared only after at least one move is accepted.
  - Current: Paste and drag/drop transfer acceptance share the same self/descendant rejection helper, including canonical path checks so symlinked targets that resolve inside the source folder are rejected before queueing.
  - Current: the same self/descendant rejection also lives in the core file operation path, so privileged helper calls or future non-UI transfer entrypoints cannot bypass the safety check.

- [x] First-pass conflict handling.
  - Acceptance: copy/move/link transfers do not silently pick a conflict policy when the destination name exists.
  - Current: transfer conflicts prompt for Overwrite, Keep Both, Rename, or Skip before an operation enters the queue.
  - Current: Apply-to-remaining supports Skip, Keep Both, Overwrite, and Rename. Rename keeps the user's explicit target name for the current conflict and assigns each remaining conflict its own unique `copy`-style suggested name, so one hand-written name is not reused across unrelated files.

- [x] First-pass operation undo.
  - Acceptance: completed copy/link operations can be undone by removing the created target.
  - Acceptance: completed move operations can be undone by moving the item back to its original path when that path is still free.
  - Current: the status bar exposes a one-step Undo action after copy/move/link operations, including overwrite conflicts. Overwrite keeps the replaced target as a temporary backup for the active undo entry; replacing that undo entry cleans the old backup.
  - Current: if Undo fails, the same Undo entry is restored so the user can fix the blocking condition and retry. If a newer Undo entry appeared before the failure result returned, Fika keeps the newer entry instead of overwriting it.

- [x] Copy/move operation queue.
  - Acceptance: long operations run in background and report progress.
  - Acceptance: cancellation is possible.
  - Current: operations run one at a time from a queue, report byte progress for copied data, and both queued and active copy/move operations can be cancelled.

- [x] Error summary.
  - Acceptance: batch failures are collected and shown together.

- [x] First-pass privileged file operations.
  - Acceptance: protected writes are retried only after explicit user confirmation.
  - Current: permission failures prompt for administrator authorization and call the constrained D-Bus helper.

## Phase 7: Search

- [x] Async recursive search.
  - Acceptance: search can include subdirectories without blocking UI.
  - Acceptance: status shows progress or searching state.
  - Current: recursive search sets an explicit loading state so the main pane shows a searching message while subfolders are being scanned.

- [x] Search cancellation.
  - Acceptance: editing the query or changing directory cancels/invalidates old results.
  - Current: clearing search, submitting a new search, pressing Cancel, or navigating away sets the active recursive search cancel flag, invalidates its generation, and clears the loading state; stale results and stale progress events are still ignored on return to the UI thread.
  - Current: recursive search reports periodic folder/result progress in the status bar, and explicit cancellation includes the latest scanned-folder/result counts when available.

- [x] Search result location display.
  - Acceptance: results clearly show parent directory when recursive search is active.
  - Current: recursive results are sorted by parent location; the first item in each location group shows a group label, while every result still keeps its parent location line for disambiguation.

- [x] Search filters.
  - Acceptance: search can filter by broad type, modified age, and size without parsing display strings.
  - Current: the search strip exposes Type / Modified / Size cycle buttons; filters apply to current-directory filtering and recursive search results.
  - Current: when filters hide some recursive search matches, the completion status explicitly says the visible count is after filters.
  - Current: COSMIC-style search query input lives in `TopBar`: the search button turns into a fixed-width header search field while active.
  - Current: `ui/search_panel.slint` is now a lightweight main-pane filter strip for recursive search, Type, Modified, Size, Cancel, Clear, and Close.
  - Current: search filter controls use scoped `FlexboxLayout`, so they wrap inside the filter strip instead of forcing or narrowing the main pane width.
  - Current: search UI state helpers, recursive-search cancellation token handling, and search status text live in `src/app/search_ui.rs`, keeping `main.rs` focused on callback wiring and async search startup.

## Phase 8: Open With

- [x] Right-click file menu.
  - Acceptance: files expose default Open With when a default application exists, plus Open With submenu.
  - Acceptance: folders expose Open Folder With and Add to Places.
  - Acceptance: multi-selection context menus expose only implemented batch-safe actions.

- [x] List candidate desktop apps.
  - Acceptance: menu includes default app, added associations from `mimeapps.list`, and cache associations from `mimeinfo.cache`.

- [x] Open With custom app.
  - Acceptance: hovering Open With expands the associated-app submenu.
  - Acceptance: the last submenu option is `Other Applications...`, which opens an application chooser dialog.

- [x] Set default app.
  - Acceptance: project can write user-level `mimeapps.list` safely.
  - Notes: Other Applications uses one dialog-level checkbox for setting the selected app as default; candidates no longer carry per-row default actions.

- [x] Launch opened applications through systemd user scopes.
  - Acceptance: default Open, Open With, and custom command can be started as transient user units/scopes.
  - Current: Fika spawns the application with current desktop Exec semantics, then attaches the child PID to a transient user `.scope` through `org.freedesktop.systemd1.Manager.StartTransientUnit`.
  - Current: if systemd user D-Bus is unavailable, the application still opens and Fika logs a diagnostic.
  - Current: Open Terminal Here keeps explicit `FIKA_TERMINAL` / `TERMINAL` overrides first, then follows the cosmic-files-style terminal lookup by querying `x-scheme-handler/terminal`, resolving visible `TerminalEmulator` desktop entries, preferring CosmicTerm when present, and only then falling back to known terminal executable names.

- [x] Observe systemd user scope lifecycle for protected edits.
  - Acceptance: protected external edits can tie scratch cleanup to the editor unit ending.
  - Current: Open/Open With returns the launched `.scope`; GUI associates protected edit token with that unit over D-Bus, and the helper polls systemd user `ActiveState` until the unit exits, then performs final writeback and scratch cleanup.

- [x] Replace lifecycle polling with systemd signals.
  - Acceptance: helper subscribes to `JobRemoved` / unit state change signals instead of polling `ActiveState`.
  - Acceptance: normal non-protected Open/Open With units can be surfaced for diagnostics without GUI log noise.
  - Current: protected external edit lifecycle now subscribes to the associated systemd user unit `ActiveState` property changes and uses polling only as a compatibility fallback when signal subscription is unavailable.
  - Current: normal default Open, Open With, custom command, and Open Terminal Here status messages include the transient unit name when available, or a non-fatal systemd diagnostic when the app still launched without a scope.

## Phase 9: State Persistence

- [x] Persist window size.
- [x] Persist sidebar width.
- [x] Persist dark mode.
- [x] Persist icon zoom level.
- [x] Persist last opened directory.

Acceptance for all:

- Values are loaded at startup.
- Values are saved on change or exit.
- Corrupt config falls back to defaults.

## Phase 10: Portal / Privileged Helper

- [x] Define chooser output contract.
  - Acceptance: chooser stdout format is documented and stable.
  - Current: chooser filter/choice parsing, selected-choice mutation, stdout metadata generation, save-name validation, and selected-directory resolution live in `src/app/chooser.rs`; `main.rs` keeps UI syncing and process-output/exit boundaries.

- [x] Add XDP / `xdg-desktop-portal` integration design.
  - Acceptance: document how Fika maps to `org.freedesktop.impl.portal.FileChooser`.
  - Acceptance: identify process model, request lifecycle, cancellation, and chooser result format.
  - Current: `docs/DESIGN.md` documents the backend bus name, object path, OpenFile flow, cancellation behavior, packaging metadata, and the distinction between installed backend descriptors and active `portals.conf` FileChooser selection.

- [x] Add `zbus` XDP portal backend prototype.
  - Acceptance: backend can launch `fika --chooser` and return selected files.
  - Acceptance: backend implements the needed `org.freedesktop.impl.portal.FileChooser` methods for initial local-file use.
  - Current: `fika-xdp-filechooser` owns `org.freedesktop.impl.portal.desktop.fika`, exposes `/org/freedesktop/portal/desktop`, implements OpenFile / SaveFile / SaveFiles through `fika --chooser`, and returns local `file://` URIs for the chooser-selected paths without resolving symlinks to their targets.
  - Current: the backend is independent of GNOME/KDE/COSMIC/GTK portal backends. Installing `fika.portal` registers Fika as a backend but does not make it active; validation requires `portals.conf` to select `fika` for `org.freedesktop.impl.portal.FileChooser`.
  - Current: OpenFile supports directory and multiple selection flags; SaveFile and SaveFiles support local-path save target selection.
  - Current: the portal request title is passed to the chooser window title and accept_label is passed to the chooser confirmation button; portal glob filters are exposed as a chooser filter popup in the footer, common MIME filters such as images, text, audio, video, archives, PDF, JSON, XML, Microsoft Office, and OpenDocument formats are conservatively converted into extension glob patterns, current_filter chooses the initial filter when it matches an exposed chooser filter, and the selected filter is returned as the original portal filter with the result. Empty portal filter labels are mapped to stable chooser labels such as `Filter 1`, while result mapping still preserves the original portal filter.
  - Current: unknown MIME-only portal filters remain hidden instead of appearing as empty chooser filters, because the current Fika chooser UI can only express glob-pattern filtering.
  - Current: portal choices are exposed as chooser footer controls; clicking a choice opens a small option menu instead of blindly cycling, and the selected choices are returned with the result.
  - Current: portal choice specs now use strict ID/default/option-ID validation and display-label sanitization before launching the chooser, so separator characters from portal clients cannot corrupt the chooser argument protocol or produce mismatched result IDs.
  - Current: recognized `wayland:` `parent_window` handles are preserved and forwarded to `fika --chooser --chooser-parent-window`; empty, malformed, or unknown handles are dropped. `FIKA_DEBUG_PORTAL=1` logs the backend parse decision and the chooser-side received handle, and both diagnostics explicitly report `parent_binding=metadata-only`, `parent_binding_reason=slint-parent-token-binding-unavailable`, and `native_transient=false`. Native transient parent binding remains Wayland platform/window-backend work.
  - Current: `FIKA_DEBUG_PORTAL=1` also prints one request summary per OpenFile / SaveFile / SaveFiles call, including request handle, start directory, selection/save flags, portal/chooser filter counts, MIME-mapped filter count, hidden unsupported filter count, initial filter index, parent-window forwarding state, parent binding status, and `native_transient=false`. When the chooser future finishes, the same debug stream now records whether the request selected paths, was cancelled by the user, produced empty output, was closed by the portal request, or failed with an error.
  - Current: the backend subscribes to the portal request handle's `org.freedesktop.impl.portal.Request.Close` signal while `fika --chooser` is running; request Close maps to portal response `1`, actively asks the chooser lifecycle task to terminate the child process, and waits for that termination. The chooser process is also launched with `kill_on_drop` as a fallback, so backend-side cancellation or connection teardown still does not leave an orphan chooser window.
  - Current: closing the chooser window exits with a dedicated cancel code that maps to portal response `1`; unexpected chooser failures now return a D-Bus error with exit status and stderr instead of being silently treated as user cancellation.

- [x] Design Polkit helper protocol.
  - Acceptance: write-back flow, temp files, permissions, and error handling are documented.

- [x] Implement protected write-back helper.
  - Acceptance: privileged writes do not happen in GUI process.

- [x] Installable Polkit action for packaged builds.
  - Acceptance: replace generic `pkexec <current-exe>` prompt text with a stable Fika action id.
  - Current: the policy template defines `org.fika.FileManager.privileged-helper` for per-method authority checks.

- [x] D-Bus privileged helper protocol draft.
  - Acceptance: document a constrained D-Bus interface for protected operations.
  - Current: `data/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml` defines CreateFolder / CreateFile / Rename / Trash / Transfer and external-edit writeback methods.

- [x] D-Bus privileged helper prototype.
  - Acceptance: GUI calls helper over D-Bus instead of spawning one-shot operation argv.
  - Current: GUI first invokes `org.fika.FileManager1.Privileged` on the system bus, letting D-Bus activation start `fika-privileged-helper --system-bus`.
  - Current: if the installable system service is unavailable in a dev checkout, GUI falls back to the older `pkexec --disable-internal-agent fika-privileged-helper --session-bus ...` path.

## Phase 11: Code Organization And Dolphin Parity Cleanup

- [x] Split common Slint data models, widgets, overlays, and menus.
  - Acceptance: reusable models, buttons, menu rows, Places rows, and file tiles are outside the main window file.
  - Current: `ui/models.slint`, `ui/widgets.slint`, `ui/menus.slint`, `ui/file_tile.slint`, `ui/dnd_overlay.slint`, `ui/search_panel.slint`, `ui/top_bar.slint`, and `ui/status_bar.slint` are imported by `ui/app.slint`; common menu rows and popup surface styling live in `ui/widgets.slint`, while file item, Open With, Create New, Transfer, Places, and viewport menu content is isolated in `ui/menus.slint`.
  - Current: `TopBar` owns the toolbar/path-entry layout, so `ui/app.slint` keeps path/search/theme action wiring without carrying the top bar drawing.
  - Current: `StatusBar` owns the bottom status/chooser/footer layout, so `ui/app.slint` keeps status and chooser action wiring without carrying the bottom row drawing.
  - Current: `DragOverlayLayer` owns Places insertion lines, drag ghost previews, and rejected-drop banners, so `ui/app.slint` keeps DnD state and action wiring without carrying repeated overlay drawing.
  - Current: dialog bodies and centered popup wrappers live in `ui/dialogs.slint`, so `ui/app.slint` keeps dialog action wiring without repeating the transparent centering shell for every modal.

- [x] Split pure selection logic out of `main.rs`.
  - Acceptance: filtering, visible-path retention, range selection, rectangle selection, and append-unique behavior are testable outside UI callbacks.
  - Current: `src/app/selection.rs` owns these helpers; existing selection tests still pass.

- [x] Split Places UI logic out of `main.rs`.
  - Acceptance: add/rename/remove/restore/reorder/drop handling for Places is grouped away from main callback wiring.
  - Current: `src/app/places.rs` owns Places persistence, add/rename/remove/restore/reorder handling, and path normalization for Places additions.

- [x] Split virtual main-view preparation out of `main.rs`.
  - Acceptance: large-directory viewport slicing and rebuild decisions are testable outside UI callback wiring.
  - Current: `src/app/virtual_view.rs` prepares clamped viewport state and the current virtual `FileEntry` slice; `main.rs` only applies Slint properties and schedules visible thumbnails.

- [x] Split directory-load preparation out of `main.rs`.
  - Acceptance: navigation vs same-directory refresh state transitions are testable outside UI callback wiring.
  - Current: `src/app/directory_loading.rs` owns load generation updates, cache lookup, search cancellation, view-context reset rules, and thumbnail-pipeline preservation rules; `main.rs` applies the result to UI state and starts the async read task.

- [x] Apply Dolphin DnD target validation.
  - Acceptance: dropping an item onto itself, or a folder into its own descendant, does not open the transfer menu and shows a status message.
  - Current: transfer preparation and execution both reject self/descendant targets.

- [x] Apply Dolphin-like context menu grouping and submenu grace.
  - Acceptance: context menus use grouped separators, submenu indicators are separate from labels, child menus anchor to their parent row and have a hover bridge to avoid accidental disappearance while moving between parent and child.
  - Current: Open With and Create New submenus share delayed-close handling through `MenuLifecycleController`, one `ChildSubmenuLayer` placement/hover-bridge host, reusable invisible bridge hit areas, and panel-level hover regions; submenu open/anchor/close-pending state is owned by `MenuLifecycle`, while `AppWindow` only routes business actions such as preparing Open With candidates. Viewport menu order follows Dolphin more closely with Create New first.

- [x] Apply Dolphin/QMenu-style popup placement.
  - Acceptance: menus prefer the requested popup point, flip if they would overflow the safe rect, and clamp when the window is too small.
  - Current: context, Open With, Create New, transfer, and chooser-choice popup surfaces use shared Rust `PopupPlacement` geometry, reusable popup surface styling, and a common outside-click dismiss layer.
  - Current: root context placement, Transfer placement, Open With / Create New child placement plus hover bridge geometry, and chooser-choice above-button placement now use Rust helpers registered on the `MenuGeometry` Slint global; root menu coordinate calculation is encapsulated by `RootContextMenuLayer`, Transfer fixed sizing plus root-menu flip/clamp placement is encapsulated by `TransferMenuLayer`, and child placement plus bridge coordinate calculation is encapsulated by `ChildSubmenuLayer`, reducing duplicated popup positioning logic in `ui/app.slint` and keeping transfer code focused on the drop anchor and target semantics.

- [x] Per-method polkit authority check.
  - Acceptance: helper asks polkit authority for `org.fika.FileManager.privileged-helper` per protected operation when the packaged action is installed.
  - Acceptance: missing packaged policy gives a clear diagnostic and does not fall back to unsafe writes.
  - Current: system-bus helper uses `org.freedesktop.PolicyKit1.Authority.CheckAuthorization` for every D-Bus method. Polkit authority, check, and denial failures include the action id and `org.fika.FileManager.policy` installation hint. The session-bus pkexec fallback keeps uid matching only for development.
  - Current: privileged helper fallback errors distinguish system-bus activation, development session-bus helper, and pkexec startup failures, and include the policy/polkit-agent installation hint.
  - Current: `FIKA_DEBUG_PRIVILEGE=1` makes the helper print startup/exit lifecycle summaries with service mode, bus connection source, authorized subject, idle duration, and active external-edit token count.

- [x] Install data helper.
  - Acceptance: packagers can install D-Bus, polkit, and portal metadata without hand-editing template paths.
  - Current: `scripts/install-data.sh` expands `@bindir@` and installs system bus service, bus policy, polkit action, D-Bus interface XML, portal service, and portal descriptor under `DESTDIR` / `PREFIX` aware paths.
  - Current: `scripts/check-install-data.sh` performs a non-root install into a temporary `DESTDIR` and verifies expected file locations, `@bindir@` expansion, root system-bus activation, D-Bus send policy, exported privileged methods, polkit defaults, polkit prompt text, portal backend metadata, and absence of placeholder metadata such as `example.invalid`.

- [x] Runtime integration diagnostic helper.
  - Acceptance: installed packages have a repeatable check for D-Bus activation metadata, Polkit action visibility, portal backend metadata, and helper binary placement.
  - Current: `scripts/check-runtime-integration.sh` validates staged metadata with `--metadata-only`, prints OS/session/systemd/portal/polkit-agent/UDisks2/tooling context in normal mode, probes UDisks2 ObjectManager visibility and `fika --diagnose-devices` output for Devices validation, reports the active `portals.conf` FileChooser backend selection, validates installed helper/portal executables and D-Bus activatable names, queries the installed polkit action when `pkaction` is available, can optionally activate-check the system helper via `--activate-system-helper` without calling any privileged file-operation method, and supports `--record FILE` for saving comparable distro/desktop validation reports.
  - Current: metadata-only validation now rejects unexpanded `@bindir@`, placeholder `example.invalid`, and dev-only `pkexec` / `--session-bus` service entries in installed metadata.
  - Remaining: run this diagnostic with `--record` on target distributions/desktops and record any polkit/systemd/dbus activation differences.

- [x] External editor writeback flow.
  - Acceptance: protected files open as ordinary scratch paths under `/run/user/$UID/fika-edit`.
  - Acceptance: writeback uses `CommitExternalEdit`, not a root editor or user-visible admin URI.
  - Current: default Open / Open With / custom Open With fall back to scratch on permission errors; status bar exposes Admin Save and Discard for pending admin write-backs.
  - Current: helper watches scratch files and writes back on save, so GUI can close after launching the editor; Admin Save remains a manual flush/cleanup action.

- [x] Helper-owned external editor lifecycle.
  - Acceptance: helper can track editor systemd unit lifetime or token expiry and clean scratch files without relying on the GUI.
  - Acceptance: closing Fika windows does not leave unbounded helper lifetime.
  - Current: helper tracks associated systemd unit lifetime and cleans scratch after unit exit; tokens without a unit are expired after a bounded TTL with a final writeback attempt.
  - Current: helper exits after idle time when no external edit tokens are active.

## Phase 12: COSMIC Files Reference Pass

- [~] Treat COSMIC Files as the primary Rust/design reference outside the main-pane layout.
  - Acceptance: new UI polish and desktop-integration work first checks `./cosmic-files` before reaching for Dolphin-specific behavior.
  - Acceptance: the current Dolphin-like column-first main-pane arrangement, horizontal scrolling, and virtualized Slint tile model stay intact.
  - Current: `docs/COSMIC_REFERENCE.md` records the policy and concrete source files to inspect.
  - Current direction: shell visuals should move closer to COSMIC Files: the window-wide shell/header owns global search/tools, below it the main-pane `PathBar` and main pane share one calm surface, and the sidebar content reads as a raised rounded panel in the same content row.
  - Current direction: outside the main-pane item arrangement, UI chrome should increasingly follow COSMIC Files for color, spacing, toolbar layout, address-entry position, Back/Forward controls, top-bar search placement, and transient surface styling; the sidebar keeps Fika's rounded panel treatment on top of COSMIC proportions.
  - Current direction: once the current structural/menu/performance work is stable, all non-main-pane chrome may move further toward COSMIC Files directly: colors, layout rhythm, address-bar position, Back/Forward affordances, search field position/display, and sidebar treatment should follow COSMIC where practical, while preserving Fika's rounded raised sidebar content panel and the existing main-pane arrangement.
  - Current direction: future UI work should freely copy COSMIC Files for all chrome outside the main file arrangement, including color tokens, top-bar/main-pane layer treatment, address-bar alignment, navigation/search placement, menus, dialogs, and sidebar rhythm. The main pane's item arrangement remains the explicit exception.
  - Current direction: `PathBar` and the main pane should continue to read as one flat content layer inside the below-header right pane, while the sidebar remains a rounded raised content panel in the same row; the sidebar may be more Fika-specific, but its spacing and rhythm should still start from COSMIC.
  - Current: first shell pass aligns the path bar, search filter panel, status bar, and main pane to one shared surface while the sidebar uses a rounded panel color and a softer divider, keeping the main pane's column-first layout untouched.
  - Current: the COSMIC-style chrome pass now keeps Slint and Rust geometry in sync for the 56px shell header, 56px main-pane path bar, and 44px/78px search filter strip, so main-pane hit testing and virtual layout follow the visible shell.
  - Current: header/path controls now use a lighter 32px shared `ToolButton`, 32px path/search input surfaces, and softer light-theme sidebar colors, moving non-main-pane chrome closer to COSMIC while leaving the main file arrangement unchanged.
  - Current: `PathBar` follows COSMIC's previous/next navigation grouping. Home remains a Places/sidebar action rather than a top/path bar button, and the visible Up button is removed from chrome.
  - Current: Search follows COSMIC's header behavior more closely: the shell `TopBar` search button becomes an inline search field, while detailed filters stay in a slim main-pane strip.
  - Current: `TopBar` Split now uses the shared `ToolButton` selected state instead of a hand-drawn one-off rectangle, keeping header controls in one COSMIC-like component family.
  - Current: `TopBar` lives in the shell/header row for global tools; `PathBar` lives as the first row inside the right main pane in the below-header content row for address/navigation.
  - Current: the default sidebar width is now 280px to better match COSMIC's narrower navigation rhythm, while persisted user widths still override it.
  - Current: the top-bar search field now uses bounded min/preferred/max layout constraints, and the path field stays in the separate main-pane `PathBar` so search mode cannot squeeze the main-pane geometry or create Slint layout recursion.
  - Current: `AppWindow` now owns a single `main-content-left` edge shared by the sidebar panel and main pane; the sidebar panel starts in the below-header content row, and its right border is the visible divider.
  - Current: the light shell base is subtly distinct from the raised white sidebar, the sidebar border is stronger than the flat top/main separators, and Places/Devices rows are inset inside the rounded sidebar panel.
  - Current: sidebar content geometry now uses a below-header same-row panel with a 16px radius, while the main-pane toolbar and main content remain a shared flat base inside the right pane.
  - Current: shared header controls now use quieter COSMIC-like 32px icon-button styling with 8px radius and lighter text weight, and path/search fields use calmer light/dark tokens without changing the main file arrangement.

- [~] Align menu/action enablement with COSMIC where it fits Fika.
  - Reference: `cosmic-files/src/menu.rs` and `cosmic-files/src/app.rs`.
  - Acceptance: context menu action grouping, disabled/hidden states, and current-folder actions are reviewed against COSMIC without changing the already-fixed submenu lifetime rules.
  - Current: the main-view blank context menu exposes Select All and keeps a disabled Paste row when no file clipboard is available, matching COSMIC's stable action layout instead of making lower rows jump as clipboard state changes.
  - Current: single-folder context menus keep Paste Into Folder as a disabled row when no file clipboard is available, matching the main-view blank Paste behavior.
  - Current: the Places blank-area menu also keeps Add Current Folder as a disabled row when the current folder is already present in Places, so Restore Defaults does not jump vertically between locations.
  - Current: shared Slint `MenuItem` rows now support COSMIC-style right-aligned shortcut hints, and context menus only display hints for actions already handled by `KeyBinding` (`Ctrl+A`, `Ctrl+V`, `Ctrl+C`, `Ctrl+X`, `Delete`).
  - Current: single-folder context menus now expose `Open Terminal Here` for that folder, matching COSMIC's selected-directory terminal action while reusing Fika's existing terminal discovery and systemd-scope launch path.
  - Current: root menu metrics callbacks now share one Rust registration path, and geometry tests cover Open With / Create New parent-row offsets plus hover bridges when child menus are clamped by the window edge.
  - Current: hover bridge geometry is now tested across right-side child menus, left-flipped child menus, and vertically clamped child menus, so the bridge must cover both the parent submenu row and the first child-menu row along the real pointer path.
  - Current: repeated context-menu rows for submenu parents, Paste, and Cut/Copy are now small internal components in `ui/menus.slint`; `ui/app.slint` only wires menu actions and no longer owns low-level row layout.
  - Current: ordinary action rows now also route through an internal `ActionMenuRow`, so raw `MenuItem` usage is limited to row wrapper internals while file, viewport, Places, Devices, transfer, chooser, Open With, and Create New menus share the same action-row enabled/shortcut/hover/click wiring.

- [~] Revisit clipboard behavior against COSMIC's cached Wayland model.
  - Reference: `cosmic-files/src/clipboard.rs` and clipboard handling in `cosmic-files/src/app.rs`.
  - Acceptance: paste availability does not depend on reading clipboard from transient menu contexts, and future paste-image/text/video-to-file workflows have a documented path.
  - Current: file-list paste availability now uses Fika's cached clipboard state; startup and menu entry only schedule background refreshes.
  - Current: when the clipboard has no file-list payload, Fika probes `wl-paste --list-types` and caches whether image, video, or text content is pasteable without reading the full payload from transient menu handling. Paste then follows COSMIC's order: files first, then image, video, and text, writing non-file contents as unique `Pasted Image.*`, `Pasted Video.*`, or `Pasted Text.txt` files with Undo support.

- [~] Evolve file operation progress toward COSMIC's controller split.
  - Reference: `cosmic-files/src/operation/controller.rs`, `recursive.rs`, and `notifiers.rs`.
  - Acceptance: queued copy/move/link/trash work has clearer progress/cancel state and less direct coupling to status-bar text.
  - Current: `src/app/operation_controller.rs` now owns operation queue snapshots, start gating, active operation id/cancel flag lifecycle, and cancellation summaries; `transfer.rs` calls these helpers instead of mutating every queue/control field inline.
  - Current: queued/start/progress/complete/failed/cancel status text is also formatted by `operation_controller.rs`, leaving `main.rs` and `transfer.rs` to apply status updates rather than build operation copy directly.
  - Current: transfer result handling is split into a pure `OperationResultDisposition` summary for completed / privilege-required / failed outcomes, while `main.rs` keeps only UI-side effects such as Undo registration, privileged prompts, refresh, and queue advancement.
  - Current: remaining-queue suffixes and privilege-waiting status updates are also computed by `operation_controller.rs`, so `main.rs` no longer owns operation status composition.
  - Current: operation completion now goes through an `OperationCompletionSummary` in `operation_controller.rs`; stale result ids are ignored before they can register Undo or open a privilege prompt, and cache invalidation / current-directory refresh decisions no longer live in `main.rs`.
  - Current: operation completion summaries now report all affected pane ids, so file operations can refresh active and inactive split panes without `main.rs` re-deriving source/target directory ownership.
  - Current: the affected-directory to pane-id mapping is now shared by ordinary file operations, Undo refresh, privileged operation results, and protected external-edit save-back, reducing active-pane-only refresh paths in split view.
  - Current: operation progress events now go through an `OperationProgressUpdate` in `operation_controller.rs`; stale progress ids are ignored by the controller instead of being special-cased in `main.rs`.
  - Current: the controller also tracks the last active-operation progress bucket, so repeated progress callbacks inside the same percentage/unknown-size state do not churn status-bar updates.
  - Current: transfer-conflict status text for Skip and Apply to remaining is now computed by `operation_controller.rs`, keeping `transfer.rs` focused on queue mutation and popup routing while preserving tested user-facing copy.

- [ ] Add split view / dual-pane browsing.
  - Reference: `cosmic-files/src/app.rs` tab model wiring and `cosmic-files/src/tab.rs` location/view state separation; keep Dolphin as the behavioral reference for exact side-by-side split-pane UX.
  - Acceptance: each pane has independent current directory, selection, search/filter state, viewport position, history stacks, and focused-pane ownership for shortcuts, menus, DnD targets, and status updates.
  - Acceptance: the existing column-first, horizontal-scroll, virtualized main-pane item layout remains unchanged inside each pane.
  - Acceptance: Places, Devices, Open Terminal Here, Open With, Trash, and privileged operations act on the focused pane unless an operation explicitly targets the other pane.
  - Current: Back/Forward history is now isolated behind `PaneHistory` instead of naked AppState stacks. Fika still exposes one pane, but history push/pop/prune semantics are testable as a pane-owned state block, matching the direction of COSMIC's per-tab location/history model.
  - Current: Selection paths and range anchor are now isolated behind `PaneSelection`, so the next split-view pass can give each pane its own selected set without reworking clipboard, chooser, trash, and menu call sites at the same time.
  - Current: Search query, type/mtime/size filters, the filtered visible-index cache, and recursive-search cancel/progress/generation are now isolated behind the active `PaneState`.
  - Current: Virtual range/cache metadata, active viewport position, thumbnail pending map, and per-directory viewport restore cache are now isolated behind `PaneView`, so visible tile slicing, scroll restoration, and thumbnail-range checks can become per-pane before the split UI is exposed.
  - Current: Directory-load, open-file, and thumbnail generation counters are now owned by `PaneState`, so stale async results are scoped to the pane that started the work instead of a global app counter.
  - Current: The active main-pane directory, entries, history, selection, search, and virtual-view state are now grouped under `PaneState`. Fika still renders one pane, but most pane-owned data now has a single ownership boundary before the visible split UI is added.
  - Current: `AppState` now owns `PanesState` with an explicit active pane slot instead of a naked `PaneState`, so the next split-view pass can add a second pane and focused-pane routing without rewriting every pane-owned state field again.
  - Current: `PanesState` now stores an optional inactive pane and exposes open/close/focus split state transitions. Opening Split clones a snapshot of the active pane into an inactive pane, including directory entries, search/filter state, virtual-view metadata, and viewport position, while intentionally not copying selection or history.
  - Current: the UI now renders a visible inactive-pane preview beside the active virtualized main pane when Split is open. The active pane keeps the existing column-first virtual grid and uses half the main-pane width for search height, scroll extent, blank-area input, and virtual slicing.
  - Current: the inactive-pane preview now uses the same `VirtualGridPlan` family as the main view and slices entries around its own horizontal viewport instead of sending a fixed first chunk, reducing Slint model/tile pressure for large split directories.
  - Current: inactive-pane preview slicing, viewport clamping, thumbnail decoration, and virtual-start metadata now live in `src/app/virtual_view.rs`, so `main.rs` only applies the prepared preview model to Slint properties.
  - Current: inactive-pane preview virtual range metadata is cached; scrolling inside the same preview range reuses the existing Slint model instead of resetting it on every viewport update.
  - Current: inactive-pane preview scrolling also reuses its current model while the cached overscan slice covers the newly visible columns, so split panes avoid anchor/model churn on small scroll movements.
  - Current: the inactive-pane preview now handles ordinary wheel / horizontal wheel as horizontal scrolling on its own viewport, while Ctrl+wheel still uses the shared icon zoom action.
  - Current: double-clicking an inactive preview tile now routes focus to that pane and then reuses the active-pane `open_path()` flow, so split preview can open folders/files without duplicating navigation and file-open logic.
  - Current: clicking the inactive preview swaps active/inactive pane focus without a directory rescan, synchronizes address/search/selection/status state from the newly active pane, and rebuilds the active virtual slice for the focused pane.
  - Current: the split divider has a wider draggable hit target, live drag feedback, continuous ratio updates for both pane virtual views, and persists the adjusted ratio on release.
  - Current: directory load requests and directory watcher refreshes now carry a stable pane id. If focus moves before the async result returns, the result updates the pane that requested it, and inactive-pane results refresh the preview/cache instead of being discarded as stale active-pane data.
  - Current: file operation completion refreshes now use the same pane-id route. Operations that affect the inactive split pane schedule a preserved inactive reload instead of only refreshing the active pane.
  - Current: ordinary copy/move/link start, progress, and completion status also use the affected pane-id route, so async operation messages do not jump to the pane focused while queued operations run.
  - Current: FileAction completions such as create, rename, trash, duplicate, and paste-non-file also return affected directories and refresh through the shared pane-id route instead of blindly refreshing the active pane. Permission-denied FileAction results that open the privileged prompt defer refresh until the privileged result returns.
  - Current: asynchronous FileAction completion status now writes back to the status bar for the pane whose directory was affected, so completion messages do not jump to the other pane after focus changes.
  - Current: Undo, privileged operation completion, file open/open-with launch status, and protected external-edit save-back also refresh or report through pane-id routing, so inactive split previews and status bars are not left stale after those non-directory async completions; Undo start/completion/failure, privileged completion, file-open success/failure, and admin write-back save/discard/failure status now write to the same affected/requesting panes.
  - Current: protected external-edit pending state is pane-local: each split pane owns its own admin write-back marker and Save/Discard controls route through the clicked pane id instead of a global focused-pane flag. The status bar also shows a fixed `ADMIN` badge beside pending write-back status so privileged scratch/write-back state is visually distinct from ordinary status text.
  - Current: split pane scrolling now uses the dominant wheel axis instead of adding vertical and horizontal deltas together, pane viewport refreshes go through one shared clamp-aware writeback helper, and wheel input that clamps to the current viewport no longer triggers a virtual-grid refresh.
  - Current: `PanesState` now exposes `PaneTarget::{Focused, Inactive, Id}` lookup helpers, giving shortcuts, menus, DnD, and async operation code a tested route away from hard-coded `active` access toward focused-pane or explicit-pane routing.
  - Current: split panes now render through the same `PaneSlot` wrapper around `FilePane`; address bars, content, status bars, mouse side buttons, context menus, selection, DnD/drop targets, chooser controls, and external-edit controls route through one pane-id aware surface instead of per-side UI bindings.
  - Current: successful device unmount cleanup prunes mount paths from both active and inactive pane histories, so the split scaffold does not strand a removed device path in the hidden pane.

- [x] Expand Trash beyond first-pass move/undo.
  - Reference: `cosmic-files/src/trash.rs`, `cosmic-files/src/operation/mod.rs`, `cosmic-files/src/menu.rs`, and `cosmic-files/src/app.rs`.
  - Acceptance: Trash has a navigable location/sidebar entry with Empty Trash, Restore From Trash, Delete Permanently, trash-specific sorting/metadata where practical, and a rescan/watch path after trash operations.
  - Acceptance: normal Delete continues to prefer Move to Trash for local files; permanent delete requires explicit Trash-context action or confirmation.
  - Current: Fika can move single/multiple paths to XDG Trash, write `.trashinfo`, summarize failures, and undo by restoring trashed paths. Places now includes a built-in Trash entry pointing at XDG Trash `files/`, and clicking it ensures the XDG Trash directories exist before navigation. Trash-context file menus now expose explicit Restore From Trash and Delete Permanently actions; Restore reads the original location from `.trashinfo`, while permanent delete is restricted to Trash `files/` entries. Trash blank-area menus expose Empty Trash, which deletes Trash `files/` entries and removes matching/orphan `.trashinfo` metadata. Empty Trash and Delete Permanently now require a confirmation dialog before execution. Trash view entries show original-location/deletion-date metadata from `.trashinfo`, sort deleted items by newest deletion date first, and the properties dialog labels the timestamp as Deleted inside Trash. Trash view monitoring watches both XDG Trash `files/` and `info/`, so external metadata changes refresh the current Trash listing.

- [ ] Continue device and mount polish with COSMIC's mounter abstraction in mind.
  - Reference: `cosmic-files/src/mounter/mod.rs` and `mounter/gvfs.rs`.
  - Acceptance: local removable devices stay on Fika's UDisks2 system-bus path, while future network/removable abstractions can share one sidebar model.
  - Current: mounted Devices entries participate in the same sidebar directory prefetch path as Places after asynchronous device discovery, reducing uncached transitions when jumping through the Devices section.

- [~] Move thumbnail caching closer to the freedesktop model used by COSMIC.
  - Reference: `cosmic-files/src/thumbnail_cacher.rs` and `thumbnailer.rs`.
  - Acceptance: cache files, failure markers, and external thumbnailer desktop entries are considered before adding more ad-hoc thumbnail code.
  - Current: thumbnail keys now carry freedesktop size buckets and cache filename identity, and thumbnail load reads/writes freedesktop cache/fail-marker paths based on the Thumbnail Managing Standard (`file://` URI MD5, `normal` / `large` / `x-large` / `xx-large`, and `fail/fika-$version`).
  - Current: Fika discovers freedesktop `.thumbnailer` entries from XDG thumbnailer directories, honors `TryExec`, matches exact and top-level wildcard MIME entries, expands `%i` / `%u` / `%o` / `%s` Exec field codes without a shell, and lets external thumbnailers generate the standard cache file for non-built-in formats such as PDF/SVG.
  - Current: thumbnail dispatch is now bounded per virtual-view sync and kept in the thumbnail pipeline rather than inline in `main.rs`, matching the COSMIC-inspired separation between directory items, view state, and thumbnail work.

- [x] Keep pointer-scope behavior aligned with COSMIC's mouse-area approach.
  - Reference: `cosmic-files/src/mouse_area.rs`.
  - Acceptance: side-button navigation and future pointer gestures remain scoped to the intended pane and do not leak into sidebar/topbar interactions.
  - Current: mouse Back/Forward follows COSMIC's area-owned pointer handling model: Slint `PointerEventButton.back` / `forward` handling lives on the main-pane blank/grid layer and `FileTile`, while sidebar, topbar, splitter, menus, and status bar do not emit history navigation.
  - Current: a source-level regression test guards that side-button navigation stays out of non-main-view Slint sources.

- [x] Re-evaluate transparent external-editor saves after D-Bus writeback is stable.
  - Acceptance: no FUSE layer is introduced unless scratch/writeback proves insufficient.
  - Current decision: keep the scratch/writeback model. It now has helper-owned file watching, explicit Commit/Discard operations, systemd unit lifecycle cleanup, TTL fallback cleanup, and focused tests for multiple saves, commit, discard, stale expiry, and changed-original rejection. Do not introduce a FUSE layer unless a real external-editor workflow proves the scratch path model insufficient.

## Always-On Quality

- [x] Keep `cargo fmt --check` passing.
- [x] Keep `cargo check` passing.
- [x] Keep `cargo test` passing.
- [x] Keep `cargo clippy --all-targets --all-features -- -D warnings` passing.
- [x] Keep `cargo run -- --help` working.
- [x] Add tests for new non-UI logic.
- [x] Keep Slint layout responsive at minimum window size.
