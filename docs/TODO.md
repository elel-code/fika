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
  - Current: FlexboxLayout is limited to local responsive control rows; the main file view keeps its deterministic Dolphin compact horizontal item-view layout.
- [x] UI entry lives in `ui/app.slint` and is compiled through `build.rs`; shared models/widgets/file tiles are split into focused `.slint` files.
- [x] Dolphin-like shell: toolbar, Places sidebar, main file area, status bar.
- [x] COSMIC-style shell surface layering outside the main file arrangement.
  - Current: `AppWindow` owns one shared base surface with a separate window-wide shell/header row.
  - Current: `TopBar`, `PathBar`, `SearchPanel`, and `StatusBar` render transparent backgrounds, so they read as one layer with the main pane.
  - Current: `TopBar` lives in the shell/header row, owns split/theme controls, and deliberately does not draw a bottom separator between the shell tool area and main content.
  - Current: below the shell/header row, the rounded sidebar panel and right main pane share one equal-height content row; the navigation/address `PathBar` is the first row inside that right main pane.
  - Current: the main-pane item arrangement intentionally remains Dolphin compact style: horizontal scrolling, column-first order, icon/media on the left, and filename text on the right.
- [x] Dark mode.
- [x] Resizable sidebar.
- [x] Dolphin compact horizontal item-view layout.
  - Current: the physical scroll axis is X. Items fill `index % rows_per_column` down the current column, then advance with `index / rows_per_column` to the next column.
  - Current: compact item metrics follow Dolphin's compact formula: `itemWidth = padding * 4 + iconSize + fontHeight * 5`, `itemHeight = padding * 2 + max(iconSize, textLines * lineSpacing)`, with an 8px column margin.
  - Current: ordinary directories use one visible title line; Trash and recursive search use pane-local three-line group/title/location rows without introducing a global or focused-pane-only layout mode.
- [x] Main-view tile virtualization.
  - Current: Rust keeps the visible `virtual_entries` slice pane-locally for controller, hit-test, DnD, thumbnail token state, and bounds projection; Slint receives `entry_count` plus pane-local raster/paint/metadata drawing data, not the full filtered file model or full bounds model.
  - Current: filtering/search rebuilds a lightweight visible-index cache once; normal unfiltered directories use an implicit identity fast path.
  - Current: viewport changes clone only the requested virtual range through that visible-index cache, avoiding repeated full filtered-model allocation during horizontal scrolling.
  - Current: virtual slice preparation lives in `src/app/virtual_view.rs`, so range planning, viewport clamping, rebuild decisions, filtered slicing, and thumbnail-cache decoration are testable away from Slint property updates.
  - Current: virtual view tests now cover the same owned snapshot pipeline used at runtime; the old state-backed test-only virtual update helper has been removed.
  - Current: `SplitPaneView` owns a self-managed clipped viewport and scrollbar; rendered tiles live in a local slice layer anchored by Rust-projected item coordinates and shifted by `paint-viewport-x`, while logical `viewport-x` continues to drive scrollbar position, hit-test, selection coordinates, and visible-slice synchronization. Large directories avoid both scrollbar-width churn and huge per-tile global coordinates while unaligned cached slices can still render after zoom.
  - Current: virtual range metadata is cached; scrolling inside the same range does not reset the Slint model.
  - Current: virtual range reuse now keeps the current Slint model while its cached overscan slice still covers the newly visible columns, and self-managed viewport clamping/rounding avoids sub-pixel drift during small scroll steps.
  - Current: Dolphin-style smooth scrolling is layered like `KItemListSmoothScroller`: ordinary wheel input updates logical `viewport-x` immediately and animates only the paint offset when the current virtual slice covers both old and target visible windows; scrollbar drag, relayout, slice geometry changes, scroll-extent changes, pointer press, and external viewport restore stop the animation and sync paint offset immediately.
  - Current: viewport-only state publication patches only the affected `PaneViewData.viewport_x` row field and the mirrored `PaneSurfaceData.view.viewport_x` field, so clamp and restored-scroll updates no longer rebuild the full pane row model, `PaneSurfaceData`, or tile raster path for a single float change. This follows Dolphin's `setScrollOffset()` boundary: offset changes update visible state, not item/widget data.
  - Current: pane-local width and rows-per-column changes clamp the viewport and request a virtual slice refresh directly, so fullscreen/layout changes at the end of a large directory no longer wait for manual scrollbar movement.
  - Current: recursive-search location group annotation is keyed by pane-local visible-result state, so ordinary large-directory scrolling does not rescan every entry just to decide whether the virtual slice needs group labels.
  - Current: background virtual snapshot preparation receives pane-local `Arc<[usize]>` visible indices and `Arc<[String]>` visible-location-group caches, so search/filter scrolling shares those arrays between the pane and snapshot requests instead of cloning the whole filtered result cache.
  - Current: ordinary wheel scrolling requests pane focus only once per event path before panning, while Ctrl+wheel still focuses before zooming.
  - Current: Rust uses one tested compact horizontal virtual plan to calculate clamped viewport position, scroll extent, visible range, overscan range, and Slint anchor column from one source of truth.
  - Current: ordinary rows use the Rust render plan for Dolphin-style compact horizontal tiles: icon on the left, filename on the right, and the same item height as the column-first row layout. Following Dolphin's `updateCompactLayoutTextCache()` separation, Rust projects the ordinary title `Text` frame as the whole tile height so large zoom levels cannot clip names away; Trash and recursive-search rows still use Rust-projected title line y/height plus sparse `show-location` metadata overlays for group/location text. The base `ItemViewEntry` hot row now carries only identity, directory flag, thumbnail state, and media token; successful thumbnail images, group/location strings, fallback file/folder images, and media/title geometry are pane-level or sparse overlay data instead of per-row hot fields. Base icon/name drawing now consumes a pane-local `ItemViewPaintEntry` sidecar projected from row tokens plus bounds, so the Slint title loop no longer pairs `ItemViewEntry` and bounds rows.
  - Current: `PaneViewData` carries Rust-projected item-view layout metrics (`rows_per_column`, cell size, padding, content width, virtual slice width, and scroll max), viewport, empty state, a tile raster base layer (`item_view_raster_layer` plus width/height), and pane-local `ItemViewPaintEntry` / `ItemViewMetadataEntry` overlay models. The business `ItemViewEntry` model stays Rust-side in `PaneView.virtual_entries`; selected/fallback/thumbnail sidecars are Rust-only inputs for raster generation, not Slint-facing pane row data. The old selection revision hot field and old per-item metadata hot fields have been removed; selection/fallback/visible thumbnail completion changes regenerate only the visible tile raster layer, and show-location changes publish only the sparse metadata model.
  - Current: offscreen thumbnail completions update the cache without resetting the Slint model; visible completions still refresh the current virtual slice.
  - Current: thumbnail scheduling for each virtual-slice sync is capped and owned by `src/app/thumbnail_pipeline.rs`, so large directories cannot enqueue an unbounded number of decode jobs from a single viewport update. Visible-first priority is consumed as a three-slice iterator, avoiding a transient `Vec<&T>` allocation on each virtual sync.
  - Current: rectangle selection narrows candidates to the intersecting column range before resolving paths, so large directories do not scan every visible result for a local drag box.
- [x] Ctrl+wheel zoom for main file tiles.
  - Current: zoom changes first try a Dolphin-style cached relayout of the current virtual slice for ordinary large directories, including `/etc`-scale directories; only super-large visible result sets fall back to current-slice style refresh plus background virtual prepare. The fallback still applies the new zoom style to the current visible slice first, matching Dolphin's `KItemListView::setStyleOption()` visible-widget update, then runs the accurate full virtual prepare in the background without synchronously collecting excessive filename width data or rebuilding a huge compact layout inside the zoom input event. The UI thread no longer synchronously computes every icon zoom level during directory/scroll rebuilds; `VirtualViewCache` keeps the pane-local layout as `Arc<ItemViewLayoutEngine>`, so snapshot requests, snapshot results, pane surface metrics, hit-test, rectangle selection, and pane-cache writeback share the column width/offset arrays instead of cloning them. Cache-hit layout matching first checks `Arc::ptr_eq` before falling back to full signature comparison, so scroll/viewport cache hits do not rescan the entire layout width/offset arrays. `CompactItemViewLayout` also stores item widths, column widths, and column offsets as `Arc<[f32]>`, and derives per-item text width instead of allocating a duplicate text-width array. Background virtual prepare reuses the cached layout when the layout signature, entry count, and thumbnail size still match; cache-miss layout now streams `name_width_units` from entries or visible-index caches straight into the layouter instead of allocating a full `visible_name_width_units` intermediate Vec. Content and filter changes clear the cached layout. `PaneEntrySnapshot` caches filename width units when entries enter the pane, so virtual snapshot and cached zoom relayout keep Dolphin-style per-column longest-name widths without repeatedly cloning names or rescanning characters during zoom/directory switches. Thumbnail scheduling after icon-size changes still uses a latest-only timer, reads lightweight pane-local row tokens instead of touching the Slint row model, and streams visible-first priority without allocating a temporary reference Vec. Compact text font metrics stay stable across icon zoom levels so each directory's first zoom does not force a new Slint text font size cold path. Fallback glyphs are regenerated only for the current raster slice, so zoom changes target media geometry and the base layer rather than swapping folder/file fallback `Image` sources. Ordinary window/sidebar/split layout changes still flush immediately. `persist_ui_state()` no longer rebuilds virtual views or synchronously writes `settings.tsv` on the UI thread; interactive settings saves are coalesced and written in the background, while close/navigation keeps a synchronous latest save.
- [x] Click blank area clears selection and releases LineEdit focus.
- [x] Ctrl multi-select.
- [x] Right-click folder menu with `Add to Places`.
  - Current: `Add to Places` is hidden when the folder path already exists in Places, matching Dolphin's single-directory action behavior.
- [x] Places persistence.
  - Current: Places model updates reuse the existing Slint `VecModel` with row-level updates for add/remove/rename/reorder instead of replacing the sidebar model.
- [x] Places drag reorder with ghost preview and insertion line.
- [x] Internal folder drag from main view into Places.
- [~] Devices sidebar.
  - Current: Devices is Rust-driven rather than hard-coded in Slint, and lists `Filesystem` plus mounted paths under `/run/media/$USER`, `/media/$USER`, `/media`, and `/mnt`.
  - Current: discovery is now mount-table-first via `/proc/self/mountinfo`, closer to Dolphin's KMountPoint/Solid and cosmic-files' mounter-item model than raw directory scanning. Mountinfo parsing keeps source and filesystem type so pseudo filesystems such as `tmpfs` do not appear as removable devices. Directory scanning remains only as a fallback when mountinfo is unavailable.
  - Current: UDisks2 system-bus `ObjectManager` discovery is used as a best-effort enhancement for user-visible external media, including unmounted filesystem-backed volumes. It accepts media-backed drives that are marked removable, media-removable, ejectable, optical, attached over the USB bus, or advertised through UDisks2 `MediaCompatibility` as optical/flash media, while still respecting UDisks2 hidden/system block hints and filtering empty media slots. Raw removable block devices without a `Filesystem` interface are filtered out at discovery time so the sidebar does not advertise devices Fika cannot mount/open. Mounted mountinfo entries stay first and win duplicate paths, but UDisks2 still fills in operation metadata such as `/dev/...` `device_path` and eject support for those duplicate rows. UDisks2 failures fall back silently to mountinfo/directory discovery.
  - Current: UDisks2 display names follow the user-visible volume/mount identity first: desktop-provided `Block.HintName` wins, then explicit filesystem labels, mounted media without a label uses the mount-point name, and unmounted unlabeled media falls back to the drive vendor/model before the raw device path. Sidebar markers now derive simple device semantics first (`USB`, `SD`, `CD`) before falling back to the label initial, matching the Dolphin/Solid and cosmic-files mounter direction of carrying device type/icon metadata instead of only text labels.
  - Current: `DeviceEntry.kind` now classifies rows as `filesystem`, `local-mount`, or `removable-media`. This is intentionally closer to cosmic-files' mounter-item model than a pure path list: mountinfo/root-scan fallback rows stay stable, while duplicate UDisks2 media can upgrade the same row to removable semantics for future icon/menu behavior.
  - Current: mountinfo/root-scan rows and UDisks2 rows are now normalized into an internal `MounterDevice` model with explicit backend identity before merge, then projected back to the existing sidebar `DeviceEntry`. This keeps the local removable path on UDisks2 while giving future backend rows the same merge/statistics/sidebar projection path.
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
  - Current: dropping into the source item's current folder hides the no-op Move action and the transfer layer rejects same-folder move requests, while Copy and Link remain available for duplicate/conflict workflows.
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
- [x] Mouse Back/Forward scoped to pane content surfaces only.
- [x] Adaptive Open With hover submenu placement.
- [x] Use Slint master `DragArea` / `DropArea` built-ins without the experimental compiler flag.
- [x] Dolphin-style DnD self-drop rejection.
- [x] Focused Slint split: `models.slint`, `widgets.slint`, menus, overlays, and pane chrome components.
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
  - Acceptance: `ItemViewEntry` can represent thumbnail state without breaking existing icon fallback.

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
  - Current: Cut / Copy publishes `x-special/gnome-copied-files`, `text/uri-list`, and Dolphin/KDE's `application/x-kde-cutselection` through Fika's built-in Wayland data-control owner; clipboard read/write no longer calls external clipboard helper commands.
  - Current: startup and context-menu entry trigger an asynchronous `x-special/gnome-copied-files` / `text/uri-list` clipboard refresh through Fika's built-in Wayland data-control reader; Paste visibility uses the cached result instead of synchronously reading the clipboard from transient menu handling. Ctrl+V / Paste also imports the current desktop clipboard through the async event bridge before queueing transfers, then uses Fika's current internal clipboard state if Wayland data-control is unavailable or the read fails.
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
  - Current: the pane-local search strip follows Dolphin's `Search::Bar` structure more closely: the first row is search input + fixed `Filter` popup button + close/cancel controls, and the second row keeps `Here` / `Everywhere` on the left while active filter chips are pushed to the right like Dolphin's `BarSecondRowFlowLayout`.
  - Current: search text edits are submitted through a 500ms single-shot timer, while Return submits immediately, matching Dolphin's delayed `slotSearchTermEdited()` / `commitCurrentConfiguration()` flow.
  - Current: the `Filter` popup is slot-routed and anchored near the button/chip; the Filter button opens widget-menu-style selector rows for Type / Modified / Size, not a pane-expanding strip.
  - Current: active Type / Modified / Size filters appear as removable selector chips in the second row; clicking the chip body opens only that chip's selector, while the trailing remove control clears only that restriction.
  - Current: filters apply to current-directory filtering and recursive search results.
  - Current: when filters hide some recursive search matches, the completion status explicitly says the visible count is after filters.
  - Current: search is opened from the pane-local `PathBar` search button, so split panes keep independent search UI and state without a main-pane concept.
  - Current: `ui/search_panel.slint` owns the Dolphin-style two-row pane search bar and popup selector surface.
  - Current: search filter chips use scoped `FlexboxLayout` with a grow spacer before the chips, so they wrap inside the second search row and stay visually separated from the location buttons instead of forcing or narrowing the main pane width.
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
  - Current: built-in Open Terminal Here has been removed; terminal actions can be supplied through user service-menu entries under Fika's own service-menu directory.

- [x] Observe systemd user scope lifecycle for protected edits.
  - Acceptance: protected external edits can tie scratch cleanup to the editor unit ending.
  - Current: Open/Open With returns the launched `.scope`; GUI associates protected edit token with that unit over D-Bus, and the helper polls systemd user `ActiveState` until the unit exits, then performs final writeback and scratch cleanup.

- [x] Replace lifecycle polling with systemd signals.
  - Acceptance: helper subscribes to `JobRemoved` / unit state change signals instead of polling `ActiveState`.
  - Acceptance: normal non-protected Open/Open With units can be surfaced for diagnostics without GUI log noise.
  - Current: protected external edit lifecycle now subscribes to the associated systemd user unit `ActiveState` property changes and uses polling only as a compatibility fallback when signal subscription is unavailable.
  - Current: normal default Open, Open With, and custom command status messages include the transient unit name when available, or a non-fatal systemd diagnostic when the app still launched without a scope.

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
  - Acceptance: reusable models, buttons, menu rows, Places rows, overlays, and pane chrome are outside the main window file.
  - Current: `ui/models.slint`, `ui/widgets.slint`, `ui/menus.slint`, `ui/dnd_overlay.slint`, `ui/search_panel.slint`, `ui/top_bar.slint`, and `ui/status_bar.slint` are imported by `ui/app.slint`; common menu rows and popup surface styling live in `ui/widgets.slint`, while file, Open With, Create New, Transfer, Places, and viewport menu content is isolated in `ui/menus.slint`.
  - Current: the standalone file tile component has been removed for the Dolphin-style viewport path; visible main-view tile primitives are now inlined in `SplitPaneView` so the next renderer/reuse pass has one focused replacement point.
  - Current: `TopBar` owns the shell toolbar drawing and `PathBar` owns pane-local path/search entry controls, so `ui/app.slint` keeps action wiring without carrying that chrome drawing.
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
  - Current: `src/app/virtual_view.rs` prepares clamped viewport state and the current virtual business slice, then `main.rs` updates Rust-side `ItemViewEntry` rows, maintains the Rust bounds sidecar, projects Slint-facing paint/overlay sidecars, and schedules visible thumbnails.

- [x] Split directory-load preparation out of `main.rs`.
  - Acceptance: navigation vs same-directory refresh state transitions are testable outside UI callback wiring.
  - Current: `src/app/directory_loading.rs` owns load generation updates, cache lookup, search cancellation, view-context reset rules, and thumbnail-pipeline preservation rules; `main.rs` applies the result to UI state and starts the async read task.

- [~] Continue focused Rust and Slint decomposition while feature work proceeds.
  - Acceptance: new behavior should opportunistically move related Rust state/controller logic and reusable Slint rows/layers/components into focused files instead of growing `main.rs` or `ui/app.slint`.
  - Current direction: split both `.rs` and `.slint` incrementally alongside real feature/performance work, keeping each extraction tied to tested behavior rather than preserving historical compatibility paths.
  - Current: service-menu context action snapshotting, Slint model synchronization, shell-free launch dispatch, and source-pane status updates now live in `src/app/context_service_menu.rs`; coalesced interactive settings persistence lives in `src/app/settings_save.rs`; `main.rs` only routes pane/menu callbacks, captures UI snapshots, and dispatches async events.

- [~] Spike Dolphin-style self-managed main viewport.
  - Acceptance: main file view scroll offset is no longer sourced from `ScrollView` / `Flickable`; Rust owns the first item-view layout/hit-test layer through `src/app/item_view.rs`.
  - Acceptance: the Slint layer is reduced to a clipped viewport shell plus input/DnD primitives (`Rectangle`, `TouchArea`, `DragArea`, `DropArea`) and does not use a standalone per-item tile component as the core performance path.
  - Acceptance: DnD remains on Slint's native `data-transfer` path, with Rust hit-test deciding item/blank/drop target semantics for both internal file drags and external file-list clipboard/paste-adjacent workflows.
  - Acceptance: `/etc`, `/usr/lib`, split view dual-pane scrolling, end-of-directory fullscreen/resize, rectangle selection, context menus, and drag/drop are tested against the current self-managed viewport + visible tile primitive path.
  - Reference: Dolphin's `kfileitemmodel`, `kitemlistviewlayouter`, `kitemlistview`, `kitemlistcontroller`, and `kstandarditemlistwidget` split. For compact layout specifically, `kstandarditemlistview.cpp` switches CompactLayout to `Qt::Horizontal`, `kitemlistviewlayouter.cpp` transposes layout data for horizontal scrolling, and `kstandarditemlistwidget.cpp` uses the whole widget height as the compact text frame. The goal is Dolphin-like model/layouter/controller/rendering ownership, not a lower-level replacement for `Flickable` alone.
  - Current: the main file area now uses `Rectangle + TouchArea + DragArea + self-managed scrollbar` directly, and transfer/DnD target hit-test plus rectangle-selection item geometry have moved into `src/app/item_view.rs`.
  - Current: each pane owns a pane-local `ItemViewInputState`; Slint now reports blank-area press/move/release/cancel events while Rust decides whether the gesture clears selection or commits a rectangle selection.
  - Current: item press, double-click activation, item context menus, and the internal main-view drag source are pane-level coordinate events; visible tile primitives no longer own `TouchArea`, `DragArea`, wheel handling, activation callbacks, context-menu callbacks, or path-based DnD data sources. The viewport-level `DragArea.data` publishes only blank-suppress / pending sentinels, and the real payload is resolved from Rust pane-local press-time `drag_source`, so hover/move no longer refreshes drag data or re-enters Rust hit-test.
  - Current: visible tile `x/y/width/text_width` is maintained in a pane-local Rust `ItemViewItemBounds` sidecar and projected into Slint-facing `ItemViewPaintEntry` / sparse overlay rows, while tile height, media/text rectangles, title line position, tile sizing/font display tokens, selection backgrounds, fallback folder/file glyphs, and loaded thumbnails are pane-level/raster data rather than per-row `ItemViewEntry` fields. Ordinary rows use Dolphin-style compact horizontal tiles with left media and right text; ordinary title `Text` receives Dolphin's whole-item compact text-frame shape from Rust to avoid large-zoom clipping, while recursive-search rows with group/location metadata keep the Rust-projected multi-line text positions through a separate sparse metadata model. Base `ItemViewEntry` rows no longer carry group/location strings, fallback images, metadata/primitive geometry, or Slint-facing base paint data. Pane row data carries Rust-projected rows/cell/content/scroll metrics and the current tile raster layer, so `SplitPaneView` no longer computes scrollbar extent, zoom-derived tile metrics, title metadata branching, per-item text/media layout, per-row fallback/thumbnail branching, or per-loop row modulo inside the Slint main-view layer, and overlapping entries do not get hot-row churn just because slice-local coordinates shifted.
  - Current: `ItemViewPaintEntry` has been narrowed to title text plus Rust-projected paint geometry; directory/file identity stays in Rust row tokens, so the Slint title paint model no longer carries `is_dir` for fallback branching. Paint and metadata rows no longer carry Slint-facing `slice_index`; selection, folder/file fallback geometry, loaded thumbnail geometry, and current drop-target geometry feed the Rust-generated tile raster base layer. Media/metadata identity stays in Rust-only source/token sidecars.
  - Current: Dolphin compact visual metrics now live in `src/app/item_view_metrics.rs`. `geometry.rs` owns viewport/visible-range layout, `item_view.rs` owns input/controller/hit-test, and `item_view_renderer.rs` owns render plan, metadata projection, fallback glyph drawing, and the tile raster base layer. They consume the shared media/font/line/cell/row source instead of owning duplicate zoom formulae; zoom updates tile geometry and regenerates only the current visible raster slice instead of swapping Slint fallback image sources.
  - Current: the old standalone tile component has been deleted; `SplitPaneView` now owns the visible tile primitive loops directly: base media/name rendering is split into flat `Image` and `Text` loops using Dolphin compact column-first coordinates, and those loops no longer use per-item `HorizontalLayout` / `VerticalLayout` containers.
  - Current: selected backgrounds and drop targets are now painted into the tile raster base layer. Selection state is kept out of `ItemViewEntry` row data; `src/app/model_update.rs` updates pane-local `ItemViewRowToken` sidecars, bumps `PaneView`'s pane-local raster revision, and `ItemViewTileFrameBatch` derives selection/drop/fallback geometry directly from row tokens plus item bounds. The former `selection_revision` field/property path is deleted rather than kept as a compatibility trigger.
  - Current: file/folder fallback media rendering and successful thumbnail rendering have moved out of Slint and into `src/app/item_view_renderer.rs` as part of the tile raster base layer. Folder/file fallback geometry is derived directly from `ItemViewTileFrameBatch` plans and no longer exists as a separate model; successful thumbnail images have moved out of `ItemViewEntry` into renderer-owned Rust-only `ItemViewMediaSource` rows that are projected into pane-local `ItemViewRasterMediaEntry` raster inputs. `SplitPaneView` draws one raster base layer that already contains selection, fallback glyphs, loaded thumbnails, and drop targets. The raster media input reuses its pane-local `Vec` through a Rust-only `slice_index/media_token` sidecar and carries only image/x/y inside Rust, so thumbnail refreshes and cached relayouts do not replace a Slint media model, compare `Image` rows, look up `root.bounds[media.slice_index]`, or keep `slice_index` in the paint loop. There is no Slint-facing folder/file fallback image model, thumbnail raster model, fallback sidecar model, or per-row `is_dir` source branch left.
  - Current: metadata overlay rendering now uses a pane-local sparse `ItemViewMetadataEntry` model. Rust projects only non-empty group/location rows through Rust-only `ItemViewMetadataOverlaySource`, then `item_view_renderer.rs` attaches item x/y from the current bounds sidecar and drops `slice_index` before publishing the Slint-facing row. `SplitPaneView` still draws one metadata `Text` loop from `root.metadata`, so the current text backend stays Slint-native while the renderer owns the swappable text projection boundary. Ordinary empty metadata rows no longer instantiate hidden `Text` primitives when `show-location` is active. Non-empty metadata refreshes reuse the same pane-local `VecModel` with sparse row updates instead of replacing the whole overlay model.
  - Current: `item_view_renderer.rs` now exposes `ItemViewFrameEntry`, `ItemViewTileFrameSource`, `ItemViewTileFramePlan`, and `ItemViewTileFrameBatch` as the Rust-side per-tile frame boundary for the future self-rendered path. Both Slint-facing `ItemViewEntry` snapshots and pane-local cached `ItemViewRowToken` rows can feed the same frame source; `ItemViewTileFramePlan` then keeps only visible, bounds-backed text/fallback/highlight primitives, while token-only thumbnail updates still read from the source through the batch media-token lookup API. Batch construction, paint-row projection, selection/fallback/drop-target raster geometry, and source-index media-token lookup now live in `item_view_renderer.rs`; `model_update.rs` consumes batch APIs to reuse/publish pane-local models instead of recombining geometry rules separately for each Slint primitive model.
  - Current: virtual slice row reuse now uses pane-local `ItemViewRowToken` sidecars owned by `src/app/model_update.rs`, including a lightweight `media_token` for thumbnails/fallback media. Loaded thumbnail images live in pane-local Rust-only `ItemViewRasterMediaEntry` raster input, so reused rows no longer need `VecModel::row_data()` clones just to compare visible `ItemViewEntry` values. The Rust-projected `ItemViewItemBounds` and Slint-facing `ItemViewPaintEntry` are reused during range slides and cached relayouts; selection/fallback/drop geometry is derived on demand by `ItemViewTileFrameBatch`, the raster media input reuses its pane-local `Vec` through Rust-only `ItemViewMediaToken`, and metadata updates use Rust-only `ItemViewMetadataOverlaySource` before publishing compact Slint-facing rows. `PaneViewData` no longer exposes `ItemViewEntry` or `bounds` to Slint, and visible primitive loops no longer cross-index bounds for selection/fallback/media/metadata/drop overlays; selection/fallback/thumbnail/drop target are already in the raster base layer, and the remaining Slint overlays read only their own paint/overlay rows. `PaneView` keeps a pane-local tile raster cache keyed by raster input plus pane-local raster revision; row token, bounds, selection, media token, and drop target changes bump the revision, so repeated `sync_pane_view_ui()` calls with an unchanged visible slice reuse the last `Image` without cloning or comparing visible rows.
  - Current: split-pane virtual snapshots copy visible rows into an independent `VecModel` and clone the matching row-token sidecar, keeping pane models independent while preserving the current visible slice.
  - Current: the Dolphin smooth scroller split is now represented by logical `viewport-x` plus animated `paint-viewport-x`. Logical viewport updates immediately for Rust sync, scrollbar, hit-test, DnD, and rectangle selection; only the visible primitive offset animates on ordinary wheel scroll, and only while the current virtual slice covers both the old paint window and target viewport window. Scrollbar click/drag, relayout, slice start/width changes, `scroll-max-x` changes, pointer press, and external viewport writes call `stop-smooth-scroll()` so manual control and directory restore do not lag behind animation.
  - Current: Slint-facing layout metrics, virtual range planning, virtual slice geometry, range-hint alignment, cache signature comparison, hit-test, item bounds, rectangle-selection candidate logic, layout mode, scroll axis, and logical→physical item projection are now exposed through `ItemViewLayouter`, with `CompactItemViewLayout` as the first implementation. Runtime view/cache state now stores `ItemViewLayoutEngine` rather than `CompactItemViewLayout` directly, and the engine delegates to the compact layouter through the same trait boundary. `split_view.rs`, `virtual_view.rs`, `pane.rs`, `item_view.rs`, and `main.rs` import the layouter trait instead of calling concrete public compact methods directly. Compact layout construction can consume precomputed filename width units from `PaneEntrySnapshot`, so Dolphin-style compact size hints remain per-column longest-name driven while zoom and virtual refresh avoid hot-path filename cloning/measurement; background cache-miss layout now streams those width units directly from entries/visible-index caches instead of materializing a separate width Vec, and cache-hit signature checks use shared `Arc` identity before falling back to full layout-array comparison. This is still compact-only at runtime, but the state boundary now matches Dolphin's layouter responsibility split more closely.
  - Current: item-view press, activation, item context-menu hit-test/select intent, and blank-area gesture lifecycle now route through `src/app/item_view.rs`; item press hit-test plus drag-source setup are handled by `press_entry_at_pane_point()`, activation hit-test returns an `ActivatePath` action from `activate_entry_at_pane_point()`, context-menu hit-test returns a `RequestContextMenu` action from `context_menu_entry_at_pane_point()`, blank press/move/release/cancel use slot-level controller helpers, and `main.rs` executes returned `ItemViewControllerAction` values instead of directly mutating `ItemViewInputState` internals or matching gesture details. Context-menu controller actions now carry `ItemViewHitEntry` instead of the full `FileEntry`, so the controller-facing payload is limited to path/name/size/modified/is-dir identity needed by menu routing. This is still callback-driven, but it moves another step toward Dolphin's controller signal/action split.
  - Remaining gaps vs Dolphin's `kfileitemmodel → kitemlistviewlayouter → kitemlistview → kitemlistcontroller → kstandarditemlistwidget` five-layer split:
  - Remaining: **Polymorphic Layouter** — Dolphin 的 `kitemlistviewlayouter` 通过 `setItemLayout()` 支持 Compact / Details / Icons 三种模式，内部按逻辑坐标计算后用 `itemRect()` 转置到物理坐标。Fika 已有 `ItemViewLayouter` trait、layout mode/scroll axis、logical→physical projection 边界，以及 `ItemViewLayoutEngine::Compact` 运行时 enum dispatch；仍缺真正的 Details/Icons layouter 分支、layout-mode 切换状态和对应 Slint/Rust render plan。
  - Remaining: **Self-rendered Tile Frame** — Dolphin 的 `kstandarditemlistwidget` 用 `QPainter` 在 widget 上一次性绘制整个可见区域（图标+文字+高亮+拖放反馈）并复用 widget 实例。Fika 已有 Rust-side `ItemViewTileFrameSource`、bounds-backed `ItemViewTileFramePlan` 和 `ItemViewTileFrameBatch` 作为统一 tile frame 输入/绘制批次，并已增加 `ItemViewTileFrameRasterInput` / `ItemViewTileFrameRaster`，可从 batch 产出一张 `SharedPixelBuffer`/`Image` raster layer。当前 raster layer 已接入 `SplitPaneView`，并覆盖选中背景、folder/file fallback glyph、已加载 thumbnail 和当前 drop target；`PaneView` 现在按 raster input + pane-local raster revision 保存 cache，view invalidate/clear 时释放。主视图仍使用 Slint 原生 `Text` primitive 绘制标题/metadata。剩余迁移重点是稳定 title/metadata text backend 输入边界、tile reuse 和必要的字体/text stack 评估；文字 raster 只作为可选 backend，必须在实测快于 Slint 原生 `Text` 后再接入。tile 复用仍停留在 `VecModel` 滑动更新（`update_sliding_vec_model`），缺少 Dolphin 式的 tile 实例复用池。
  - Remaining: **Model Trait Abstraction** — Dolphin 的 `kfileitemmodel` 是抽象基类，layouter/controller/view 通过接口消费。Fika 已在 renderer 输入侧增加 `ItemViewFrameEntry`，让 tile frame source 能消费 `ItemViewEntry` 和 cached `ItemViewRowToken`；controller context-menu payload 也已收窄到 `ItemViewHitEntry`，不再把完整 `FileEntry` 作为 action 边界。但 `PaneEntrySnapshot` / `FileEntry` 到 layouter、selection、navigation 的完整模型接口仍未抽象，更换数据源仍需要适配。
  - Remaining: **Controller Signal Bus** — Dolphin 的 `kitemlistcontroller` 是独立输入层，通过信号与 view 通信。Fika 已把 item press hit-test/drag-source setup、activation hit-test、item context-menu hit-test/select intent 和 blank gesture lifecycle 收敛进 `item_view.rs` controller helpers，并通过 `ItemViewControllerAction` 返回执行意图，context-menu action 只携带 controller-facing `ItemViewHitEntry`。但 controller 仍不是完整事件总线：selection 执行、context-menu UI route/service-menu async refresh 和其他 UI/async routing 仍由 `main.rs` 回调编排。
  - Remaining: **Two-phase View Refresh** — Dolphin 显式保留旧 visible state → 新数据就绪 → 原子替换。Fika 的 async virtual refresh in-flight/pending bookkeeping 已收敛进 `VirtualViewRefreshState`，不再是 `PaneView` 上的两个裸字段；但过滤/搜索/监控刷新仍直接原地更新 virtual slice，尚无覆盖所有 view refresh 类型的统一 pending→commit 状态机。
  - Next: 继续把剩余 Slint overlay 的输入边界收敛到 renderer：先保持 Slint 原生 `Text` backend，稳定 title/metadata text projection，再用 profiling 判断是否需要可选文字 raster backend。

- [x] Apply Dolphin DnD target validation.
  - Acceptance: dropping an item onto itself, or a folder into its own descendant, does not open the transfer menu and shows a status message.
  - Current: transfer preparation and execution both reject self/descendant targets.

- [~] Add Dolphin-style user custom context menu actions.
  - Acceptance: users can expose opt-in custom file/folder context actions backed by desktop/service-menu style metadata, with visibility constrained by target type and selection.
  - Current: `src/desktop/service_menu.rs` discovers Fika-owned `fika/servicemenus` entries first and KDE/Dolphin-compatible `kio/servicemenus` entries second from each XDG data dir, filters `KonqPopupMenu/Plugin` actions by MIME and multi-selection-safe Exec fields, expands desktop Exec field codes into shell-free argv, and sorts top-level actions first.
  - Current: item and blank-area right-click routing refresh a generation-guarded `AppState` snapshot of matching service-menu actions off the UI thread, clear stale popup rows immediately, then update the Slint action model when discovery returns.
  - Current: matching service-menu actions now render in file/viewport context menus, clicking launches the stored shell-free argv through the existing systemd user-scope launch path, and status reports back to the source pane.
  - Current: context action UI/controller behavior is isolated in `src/app/context_service_menu.rs`; top-level actions stay as direct rows, non-top-level `X-KDE-Submenu` groups render as true hover submenus through the shared `MenuLifecycleController` / `ChildSubmenuLayer` path, and popup geometry counts only root action/submenu rows.
  - Current: matching service-menu actions now keep both an all-actions snapshot and a policy-filtered visible snapshot. The file/viewport menu exposes `Configure Service Actions...` when actions exist, and the dialog lets users choose between "all except disabled" and opt-in "only checked" policy modes while enabling/disabling the current matching service actions without losing hidden rows. Policy is persisted in `service-menu-policy.tsv`.
  - Current: service-menu `Icon=` metadata is resolved through a small XDG icon-theme lookup path during background discovery, then shown in the root action rows, grouped child action rows, and the service-action configuration dialog when an image file is found.
  - Next: add Dolphin-style user-defined custom context actions on top of the existing desktop/service-menu parser, with the action editor/selection UI split into focused Rust and Slint modules as it lands.

- [x] Apply Dolphin-like context menu grouping and submenu grace.
  - Acceptance: context menus use grouped separators, submenu indicators are separate from labels, child menus anchor to their parent row and have a hover bridge to avoid accidental disappearance while moving between parent and child.
  - Current: Open With, Create New, and service-menu group submenus share delayed-close handling through `MenuLifecycleController`, one `ChildSubmenuLayer` placement/hover-bridge host, reusable invisible bridge hit areas, and panel-level hover regions; submenu open/anchor/close-pending state is owned by `MenuLifecycle`, while `AppWindow` only routes business actions such as preparing Open With candidates or service-menu child rows. Viewport menu order follows Dolphin more closely with Create New first.

- [x] Apply Dolphin/QMenu-style popup placement.
  - Acceptance: menus prefer the requested popup point, flip if they would overflow the safe rect, and clamp when the window is too small.
  - Current: context, Open With, Create New, transfer, and chooser-choice popup surfaces use shared Rust `PopupPlacement` geometry, reusable popup surface styling, and a common outside-click dismiss layer.
  - Current: root context placement, Transfer placement, Open With / Create New / service-menu child placement plus hover bridge geometry, and chooser-choice above-button placement now use Rust helpers registered on the `MenuGeometry` Slint global; root menu coordinate calculation is encapsulated by `RootContextMenuLayer`, Transfer fixed sizing plus root-menu flip/clamp placement is encapsulated by `TransferMenuLayer`, and child placement plus bridge coordinate calculation is encapsulated by `ChildSubmenuLayer`, reducing duplicated popup positioning logic in `ui/app.slint` and keeping transfer code focused on the drop anchor and target semantics.

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
  - Current direction: shell visuals should move closer to COSMIC Files: the window-wide shell/header owns global tools, below it each pane's `PathBar` / search panel / file content share one calm surface, and the sidebar content reads as a raised rounded panel in the same content row.
  - Current direction: outside the main-pane item arrangement, UI chrome should increasingly follow COSMIC Files for color, spacing, toolbar layout, address-entry position, Back/Forward controls, and transient surface styling; search remains pane-local and may selectively borrow Dolphin's search bar structure where it better serves split-pane independence.
  - Current direction: once the current structural/menu/performance work is stable, all non-main-pane chrome may move further toward COSMIC Files directly: colors, layout rhythm, address-bar position, Back/Forward affordances, and sidebar treatment should follow COSMIC where practical, while preserving Fika's rounded raised sidebar content panel, pane-local search ownership, and the existing main-pane arrangement.
  - Current direction: future UI work should freely copy COSMIC Files for all chrome outside the main file arrangement, including color tokens, top-bar/main-pane layer treatment, address-bar alignment, menus, dialogs, and sidebar rhythm. Pane-local search remains Dolphin-inspired unless Fika gains a better split-pane-compatible COSMIC pattern.
  - Current direction: each pane-local `PathBar` and file content should continue to read as one flat content layer inside the below-header pane area, while the sidebar remains a rounded raised content panel in the same row; the sidebar may be more Fika-specific, but its spacing and rhythm should still start from COSMIC.
  - Current: first shell pass aligns the path bar, search filter panel, status bar, and main pane to one shared surface while the sidebar uses a rounded panel color and a softer divider, keeping the main pane's column-first layout untouched.
  - Current: the chrome geometry pass now keeps Slint and Rust geometry in sync for the 56px shell header, 56px main-pane path bar, and Dolphin-style 84px/116px two-row search strip, so main-pane hit testing and virtual layout follow the visible shell.
  - Current: header/path controls now use a lighter 32px shared `ToolButton`, 32px path/search input surfaces, and softer light-theme sidebar colors, moving non-main-pane chrome closer to COSMIC while leaving the main file arrangement unchanged.
  - Current: `PathBar` follows COSMIC's previous/next navigation grouping. Home remains a Places/sidebar action rather than a top/path bar button, and the visible Up button is removed from chrome.
  - Current: Search is pane-local again: `PathBar` opens a Dolphin-style `SearchPanel`, and detailed filters live in an anchored popup with removable active-filter chips.
  - Current: `TopBar` Split now uses the shared `ToolButton` selected state instead of a hand-drawn one-off rectangle, keeping header controls in one COSMIC-like component family.
  - Current: `TopBar` lives in the shell/header row for global tools; `PathBar` lives as the first row inside the right main pane in the below-header content row for address/navigation.
  - Current: the default sidebar width is now 280px to better match COSMIC's narrower navigation rhythm, while persisted user widths still override it.
  - Current: the search input uses the main first-row stretch inside the pane-local `SearchPanel`, and the path field stays in the separate main-pane `PathBar` so search mode cannot squeeze split-pane geometry or create Slint layout recursion.
  - Current: `AppWindow` now owns a single `main-content-left` edge shared by the sidebar panel and main pane; the sidebar panel starts in the below-header content row, and its right border is the visible divider.
  - Current: the light shell base is subtly distinct from the raised white sidebar, the sidebar border is stronger than the flat top/main separators, and Places/Devices rows are inset inside the rounded sidebar panel.
  - Current: sidebar content geometry now uses a below-header same-row panel with a 16px radius, while pane toolbars and content remain shared flat bases inside the pane area.
  - Current: shared header controls now use quieter COSMIC-like 32px icon-button styling with 8px radius and lighter text weight, and pane-local path/search fields use calmer light/dark tokens without changing the main file arrangement.

- [~] Align menu/action enablement with COSMIC where it fits Fika.
  - Reference: `cosmic-files/src/menu.rs` and `cosmic-files/src/app.rs`.
  - Acceptance: context menu action grouping, disabled/hidden states, and current-folder actions are reviewed against COSMIC without changing the already-fixed submenu lifetime rules.
  - Current: the main-view blank context menu exposes Select All and keeps a disabled Paste row when no file clipboard is available, matching COSMIC's stable action layout instead of making lower rows jump as clipboard state changes.
  - Current: single-folder context menus keep Paste Into Folder as a disabled row when no file clipboard is available, matching the main-view blank Paste behavior.
  - Current: the Places blank-area menu also keeps Add Current Folder as a disabled row when the current folder is already present in Places, so Restore Defaults does not jump vertically between locations.
  - Current: shared Slint `MenuItem` rows now support COSMIC-style right-aligned shortcut hints, and context menus only display hints for actions already handled by `KeyBinding` (`Ctrl+A`, `Ctrl+V`, `Ctrl+C`, `Ctrl+X`, `Delete`).
  - Current: built-in `Open Terminal Here` has been removed from file and viewport menus; equivalent terminal launchers belong in user-configurable service-menu actions.
  - Current: root menu metrics callbacks now share one Rust registration path, and geometry tests cover Open With / Create New parent-row offsets plus hover bridges when child menus are clamped by the window edge.
  - Current: hover bridge geometry is now tested across right-side child menus, left-flipped child menus, and vertically clamped child menus, so the bridge must cover both the parent submenu row and the first child-menu row along the real pointer path.
  - Current: repeated context-menu rows for submenu parents, Paste, and Cut/Copy are now small internal components in `ui/menus.slint`; `ui/app.slint` only wires menu actions and no longer owns low-level row layout.
  - Current: ordinary action rows now also route through an internal `ActionMenuRow`, so raw `MenuItem` usage is limited to row wrapper internals while file, viewport, Places, Devices, transfer, chooser, Open With, and Create New menus share the same action-row enabled/shortcut/hover/click wiring.

- [x] Revisit clipboard behavior against COSMIC's cached Wayland model.
  - Reference: `cosmic-files/src/clipboard.rs` and clipboard handling in `cosmic-files/src/app.rs`.
  - Acceptance: paste availability does not depend on reading clipboard from transient menu contexts, and future paste-image/text/video-to-file workflows have a documented path.
  - Current: file-list paste availability now uses Fika's cached clipboard state; startup and menu entry only schedule background refreshes. Clipboard availability refreshes are single-flight, so repeated startup/menu triggers reuse the pending Wayland data-control read instead of spawning duplicate clipboard queries.
  - Current: when the clipboard has no file-list payload, Fika probes the advertised MIME types through its built-in Wayland data-control reader and caches whether image, video, or text content is pasteable without reading the full payload from transient menu handling. Paste then follows COSMIC's order: files first, then image, video, and text, writing non-file contents as unique `Pasted Image.*`, `Pasted Video.*`, or `Pasted Text.txt` files with Undo support.

- [~] Evolve file operation progress toward COSMIC's controller split.
  - Reference: `cosmic-files/src/operation/controller.rs`, `recursive.rs`, and `notifiers.rs`.
  - Acceptance: queued copy/move/link/trash work has clearer progress/cancel state and less direct coupling to status-bar text.
  - Current: `src/app/operation_controller.rs` now owns operation queue snapshots, start gating, active operation id/cancel flag lifecycle, and cancellation summaries; `transfer.rs` calls these helpers instead of mutating every queue/control field inline.
  - Current: queued/start/progress/complete/failed/cancel status text is also formatted by `operation_controller.rs`, leaving `main.rs` and `transfer.rs` to apply status updates rather than build operation copy directly.
  - Current: operation completion now goes through an `OperationCompletionSummary` in `operation_controller.rs`; stale result ids are ignored before they can register Undo or open a privilege prompt, and cache invalidation / affected-pane refresh decisions no longer live in `main.rs`.
  - Current: completed / privilege-required / failed transfer outcomes now return controller-owned status text, transfer Undo registration summaries, and privilege prompt requests. `main.rs` only applies those UI effects, refreshes the returned pane ids, and advances the queue.
  - Current: remaining-queue suffixes and privilege-waiting status updates are also computed by `operation_controller.rs`, so `main.rs` no longer owns operation status composition.
  - Current: operation completion summaries now report all affected pane ids, so file operations can refresh every matching pane slot without `main.rs` re-deriving source/target directory ownership.
  - Current: the affected-directory to pane-id mapping is now shared by ordinary file operations, Undo refresh, privileged operation results, and protected external-edit save-back, reducing active-pane-only refresh paths in split view.
  - Current: operation progress events now go through an `OperationProgressUpdate` in `operation_controller.rs`; stale progress ids are ignored by the controller instead of being special-cased in `main.rs`.
  - Current: the controller also tracks the last active-operation progress bucket, so repeated progress callbacks inside the same percentage/unknown-size state do not churn status-bar updates.
  - Current: transfer-conflict status text for Skip and Apply to remaining is now computed by `operation_controller.rs`, preserving tested user-facing copy while leaving `transfer.rs` to apply popup/status UI effects.
  - Current: cancelling queued/active operations now returns the active operation's pane ids in `OperationCancelSummary`; cancellation status is applied through the same affected-pane status route as operation start/progress/completion instead of jumping to the pane focused when Cancel is clicked.
  - Current: start-next-operation queue popping, invalid-request skipping, conflict registration, affected-pane routing, and start status now flow through `OperationStartDecision` from `operation_controller.rs`; `transfer.rs` applies the returned UI effects only after releasing the state borrow and then dispatches the async task.
  - Current: transfer conflict apply-to-remaining queue mutation, default Rename policy, and accepted Cut clipboard source cleanup are now owned by `operation_controller.rs`; `transfer.rs` only delegates those state changes, then syncs clipboard/status UI after the mutable `AppState` borrow has ended.
  - Current: Undo completion now flows through `FileUndoCompletionSummary`; affected-directory derivation, failed-Undo restoration, status text, Undo UI state, and old overwrite-backup cleanup decisions live in `operation_controller.rs`, while `main.rs` refreshes pane ids, applies returned UI state, and performs backup cleanup after releasing the mutable `AppState` borrow.
  - Current: Undo start/take decisions now flow through `FileUndoStartDecision`, including empty-state copy, affected-pane routing, start status text, and Undo UI state; `main.rs` applies the returned UI state after releasing the mutable `AppState` borrow and then dispatches the async task.
  - Current: transfer Undo registration and generic FileAction Undo replacement now return `FileUndoRegistrationSummary`; the controller owns transfer-operation eligibility, previous overwrite-backup cleanup decisions, and Undo button label/availability state instead of letting `main.rs` rebuild that state from `last_undo`.
  - Current: FileAction completion now flows through `FileActionCompletionSummary`; success/failure status text, permission-denied privilege requests, affected-directory refresh lists, and generic FileAction Undo registration are produced by `operation_controller.rs`, while `main.rs` only applies the returned UI effects.
  - Current: privileged operation completion now flows through `PrivilegedOperationCompletionSummary`; success/failure status text and affected-directory refresh lists are produced by `operation_controller.rs`, while `main.rs` only refreshes affected panes and applies the returned status after releasing the mutable `AppState` borrow.
  - Current: file-open completion now flows through `FileOpenCompletionSummary`; stale generation checks, launched systemd-unit tracking, protected external-edit registration, and success/failure status text are produced by `operation_controller.rs`, while `main.rs` only syncs external-edit pane UI when that pending state changed and applies the returned pane status after releasing the mutable `AppState` borrow.
  - Current: protected external edit Save/Discard start decisions now flow through `ExternalEditStartDecision`; pane-slot lookup, pane-local pending-session selection, missing-state status text, and save/discard start status text are produced by `operation_controller.rs`, while `main.rs` only applies the returned status and dispatches the returned D-Bus session.
  - Current: protected external edit completion now flows through `ExternalEditCompletionSummary`; pending-token cleanup, save/discard/failure status text, affected-directory refresh, and status target fallback are produced by `operation_controller.rs`, while `main.rs` only syncs pane-local external edit UI when pending state changed, refreshes affected panes, and applies the returned status after releasing the mutable `AppState` borrow.

- [x] Add split view / dual-pane browsing.
  - Reference: `cosmic-files/src/app.rs` tab model wiring and `cosmic-files/src/tab.rs` location/view state separation; keep Dolphin as the behavioral reference for exact side-by-side split-pane UX.
  - Acceptance: each pane has independent current directory, selection, search/filter state, viewport position, history stacks, and focused-pane ownership for shortcuts, menus, DnD targets, and status updates.
  - Acceptance: the existing Dolphin compact horizontal virtual item-view remains unchanged inside each pane.
  - Acceptance: Places, Devices, Open With, Trash, service-menu actions, and privileged operations act on the focused pane unless an operation explicitly targets the other pane.
  - Current: `PanesState` owns a slot-indexed pane list with a focused slot and stable pane ids. Opening Split clones the focused pane into a new pane slot, including directory entries, search/filter state, virtual-view metadata, and viewport position, while intentionally not copying selection or history.
  - Current: split panes render through the same `PaneSlotSurface -> PaneSlot -> FilePane -> SplitPaneView` component path. Each physical pane owns its own address bar, search/filter controls, virtualized file surface, status bar, external-edit controls, and chooser footer state through one reusable binding surface.
  - Current: pane geometry is slot-driven rather than hand-coded per side. `pane-slot-x()` / `pane-slot-width()` derive physical pane bounds from slot index, visible pane count, main-pane width, and divider width.
  - Current: the existing Dolphin compact horizontal virtual item-view runs inside every pane. Virtual slicing, viewport clamping, thumbnail decoration, virtual-start metadata, and model-reuse decisions live in `src/app/virtual_view.rs` and are applied per pane slot.
  - Current: every pane keeps its own horizontal viewport, virtual range cache, thumbnail pending map, per-directory viewport restore cache, selection, history, search query, filters, recursive-search generation/progress/cancel state, and async load/open/thumbnail generations.
  - Current: mouse side buttons, blank-area clicks, double-click activation, selection, rectangle selection, DnD/drop targets, context menus, path submission, Back/Forward, status updates, chooser controls, external-edit Save/Discard, and Ctrl+wheel zoom route through one slot-aware `PaneRouting` surface instead of side-specific bindings.
  - Current: Places and Devices highlight follow the Rust-synced focused pane path immediately, and sidebar navigation targets the focused pane.
  - Current: closing Split closes the focused pane slot and preserves the remaining pane without swapping pane state behind the user's back.
  - Current: the split divider has a wider draggable hit target, live drag feedback, continuous ratio updates for every visible pane virtual view, and persists the adjusted ratio on release.
  - Current: directory load requests and directory watcher refreshes carry a stable pane id. If focus moves before the async result returns, the result updates the pane that requested it.
  - Current: file operation completion refreshes now use the same pane-id route. Operations that affect any pane slot schedule a preserved reload for that pane instead of only refreshing the focused pane.
  - Current: ordinary copy/move/link start, progress, and completion status also use the affected pane-id route, so async operation messages do not jump to the pane focused while queued operations run.
  - Current: FileAction completions such as create, rename, trash, duplicate, and paste-non-file also return affected directories and refresh through the shared pane-id route instead of blindly refreshing the active pane. Permission-denied FileAction results that open the privileged prompt defer refresh until the privileged result returns.
  - Current: asynchronous FileAction completion status now writes back to the status bar for the pane whose directory was affected, so completion messages do not jump to the other pane after focus changes.
  - Current: Undo, privileged operation completion, file open/open-with launch status, and protected external-edit save-back also refresh or report through pane-id routing; Undo start/completion/failure, privileged completion, file-open success/failure, and admin write-back save/discard/failure status now write to the same affected/requesting panes.
  - Current: protected external-edit pending state is pane-local: each split pane owns its own admin write-back marker and Save/Discard controls route through the clicked pane id instead of a global focused-pane flag. The status bar also shows a fixed `ADMIN` badge beside pending write-back status so privileged scratch/write-back state is visually distinct from ordinary status text.
  - Current: split pane scrolling now uses the dominant wheel axis instead of adding vertical and horizontal deltas together, pane viewport refreshes go through one shared clamp-aware writeback helper, and wheel input that clamps to the current viewport no longer triggers a virtual item-view refresh.
  - Current: `PanesState` now exposes `PaneTarget::{Active, Focused, Slot, Id}` lookup helpers, giving shortcuts, menus, DnD, and async operation code a tested route away from hard-coded `active` access toward focused-pane or explicit-pane routing.
  - Current: stable `pane_slots` model rows are updated in place when slot shape is unchanged, so focusing an already visible pane or refreshing a pane does not rebuild its Slint surface and break gestures.
  - Current: pure pane focus changes update global focused-pane fields and then refresh only the old and new pane slot rows, avoiding a full pane-slot model pass while still downgrading/activating focus-derived pane UI.
  - Current: affected-pane refresh releases slot lookup borrows before entering refresh paths, avoiding nested `RefCell` borrow panics during async operation completion.
  - Current: successful device unmount cleanup prunes mount paths from all pane histories, so split panes do not strand removed device paths.

- [x] Expand Trash beyond first-pass move/undo.
  - Reference: `cosmic-files/src/trash.rs`, `cosmic-files/src/operation/mod.rs`, `cosmic-files/src/menu.rs`, and `cosmic-files/src/app.rs`.
  - Acceptance: Trash has a navigable location/sidebar entry with Empty Trash, Restore From Trash, Delete Permanently, trash-specific sorting/metadata where practical, and a rescan/watch path after trash operations.
  - Acceptance: normal Delete continues to prefer Move to Trash for local files; permanent delete requires explicit Trash-context action or confirmation.
  - Current: Fika can move single/multiple paths to XDG Trash, write `.trashinfo`, summarize failures, and undo by restoring trashed paths. Places now includes a built-in Trash entry pointing at XDG Trash `files/`, and clicking it ensures the XDG Trash directories exist before navigation. Trash-context file menus now expose explicit Restore From Trash and Delete Permanently actions; Restore reads the original location from `.trashinfo`, while permanent delete is restricted to Trash `files/` entries. Trash blank-area menus expose Empty Trash, which deletes Trash `files/` entries and removes matching/orphan `.trashinfo` metadata. Empty Trash and Delete Permanently now require a confirmation dialog before execution. Trash view entries show original-location/deletion-date metadata from `.trashinfo`, sort deleted items by newest deletion date first, and the properties dialog labels the timestamp as Deleted inside Trash. Trash view monitoring watches both XDG Trash `files/` and `info/`, so external metadata changes refresh the current Trash listing.

- [~] Continue device and mount polish with COSMIC's mounter abstraction in mind.
  - Reference: `cosmic-files/src/mounter/mod.rs` and `mounter/gvfs.rs`.
  - Acceptance: local removable devices stay on Fika's UDisks2 system-bus path, while future network/removable abstractions can share one sidebar model.
  - Current: mounted Devices entries participate in the same sidebar directory prefetch path as Places after asynchronous device discovery, reducing uncached transitions when jumping through the Devices section.
  - Current: Devices discovery now has a COSMIC-like internal mounter item path: mountinfo/root-scan and UDisks2 results become backend-tagged `MounterDevice` rows before duplicate merging, diagnostics statistics, and projection to Slint `DeviceEntry`.
  - Remaining: add any future GVfs/network backend behind that shared model without replacing the existing UDisks2 local removable device operations.

- [x] Move thumbnail caching closer to the freedesktop model used by COSMIC.
  - Reference: `cosmic-files/src/thumbnail_cacher.rs` and `thumbnailer.rs`.
  - Acceptance: cache files, failure markers, and external thumbnailer desktop entries are considered before adding more ad-hoc thumbnail code.
  - Current: thumbnail keys now carry freedesktop size buckets and cache filename identity, and thumbnail load reads/writes freedesktop cache/fail-marker paths based on the Thumbnail Managing Standard (`file://` URI MD5, `normal` / `large` / `x-large` / `xx-large`, and `fail/fika-$version`).
  - Current: freedesktop cache reads now validate PNG text metadata before reuse: `Thumb::URI` must match the source URI and `Thumb::MTime` must match the source mtime. Missing, unreadable, or mismatched metadata is treated as a cache miss and the stale cache file is removed before regenerating.
  - Current: Fika discovers freedesktop `.thumbnailer` entries from XDG thumbnailer directories, honors `TryExec`, matches exact and top-level wildcard MIME entries, expands `%i` / `%u` / `%o` / `%s` Exec field codes without a shell, and lets external thumbnailers generate the standard cache file for non-built-in formats such as PDF/SVG.
  - Current: thumbnail dispatch is now bounded per virtual-view sync and kept in the thumbnail pipeline rather than inline in `main.rs`, matching the COSMIC-inspired separation between directory items, view state, and thumbnail work.

- [x] Keep pointer-scope behavior aligned with COSMIC's mouse-area approach.
  - Reference: `cosmic-files/src/mouse_area.rs`.
  - Acceptance: side-button navigation and future pointer gestures remain scoped to the intended pane and do not leak into sidebar/topbar interactions.
  - Current: mouse Back/Forward follows COSMIC's area-owned pointer handling model: Slint `PointerEventButton.back` / `forward` handling lives on the main-pane viewport input layer, while sidebar, topbar, splitter, menus, and status bar do not emit history navigation.
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
