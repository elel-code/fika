# Fika TODO

本文档按实现顺序组织。每个任务都包含验收标准，后续实现时逐项更新状态。

状态说明：

- `[x]` 已完成
- `[~]` 部分完成
- `[ ]` 未开始

## Phase 0: Current Baseline

- [x] Slint 1.16.1 pinned in `Cargo.toml`.
- [x] UI entry lives in `ui/app.slint` and is compiled through `build.rs`; shared models/widgets/file tiles are split into focused `.slint` files.
- [x] Dolphin-like shell: toolbar, Places sidebar, main icon area, status bar.
- [x] Dark mode.
- [x] Resizable sidebar.
- [x] Column-first icon layout.
- [x] Main-view tile virtualization.
  - Current: Slint receives only `entry_count` and the visible `virtual_entries` slice, not the full filtered file model.
  - Current: filtering/search rebuilds a lightweight visible-index cache once; normal unfiltered directories use an implicit identity fast path.
  - Current: viewport changes clone only the requested virtual range through that visible-index cache, avoiding repeated full filtered-model allocation during horizontal scrolling.
  - Current: virtual slice preparation lives in `src/app/virtual_view.rs`, so range planning, viewport clamping, rebuild decisions, filtered slicing, and thumbnail-cache decoration are testable away from Slint property updates.
  - Current: virtual tiles are placed inside a local virtual layer anchored at the first virtualized column, so large directories do not force Slint to maintain huge per-tile global coordinates.
  - Current: virtual range metadata is cached; scrolling inside the same range does not reset the Slint model.
  - Current: Rust uses a tested `VirtualGridPlan` to calculate clamped viewport position, scroll extent, visible range, overscan range, and Slint anchor column from one source of truth.
  - Current: offscreen thumbnail completions update the cache without resetting the Slint model; visible completions still refresh the current virtual slice.
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
  - Current: UDisks2 system-bus `ObjectManager` discovery is used as a best-effort enhancement for user-visible external media, including unmounted drives. It accepts media-backed drives that are marked removable, media-removable, ejectable, optical, attached over the USB bus, or advertised through UDisks2 `MediaCompatibility` as optical/flash media, while still respecting UDisks2 hidden/system block hints and filtering empty media slots. Mounted mountinfo entries stay first and win duplicate paths, but UDisks2 still fills in operation metadata such as `/dev/...` `device_path` and eject support for those duplicate rows. UDisks2 failures fall back silently to mountinfo/directory discovery.
  - Current: UDisks2 display names follow the user-visible volume/mount identity first: desktop-provided `Block.HintName` wins, then explicit filesystem labels, mounted media without a label uses the mount-point name, and unmounted unlabeled media falls back to the drive vendor/model before the raw device path.
  - Current: clicking an unmounted UDisks2 filesystem device starts an async `Filesystem.Mount({})`; success refreshes Devices and opens the returned mount point, while failures are shown in the status bar.
  - Current: device rows have a right-click menu with Mount for unmounted media, Open/Unmount for mounted media, and Eject when UDisks2 reports an ejectable drive. These actions run off the UI thread and refresh Devices after completion.
  - Current: device menu actions are driven by explicit `can_mount`, `can_unmount`, and `can_eject` capabilities. The root Filesystem row and mountinfo-only fallback rows remain openable but no longer advertise UDisks2 actions that the backend cannot perform.
  - Current: pending device actions are tracked per `device_path`, so repeated clicks on the same device do not queue overlapping Mount/Unmount/Eject D-Bus calls. Pending rows now render a distinct blue in-progress state, and their right-click menu collapses to a disabled Mounting/Unmounting/Ejecting status row until the action finishes.
  - Current: common UDisks2 D-Bus errors such as busy devices, authorization failures, already-mounted, not-mounted, cancellation, and timeout are mapped to status-bar guidance while retaining the raw error name/detail for diagnostics.
  - Current: failed device actions are retained per device and rendered as a distinct sidebar error state, so the affected row stays visually marked after the status message changes. A later successful action for that device clears the marker.
  - Current: after a successful Unmount/Eject, if the current main view is inside that device's previous mount point, Fika moves the view back to Home and prunes history entries under the removed mount path, matching Dolphin/cosmic-files' avoid-stale-location behavior.
  - Current: Devices discovery runs through the async event bridge; `/proc/self/mountinfo` parsing and UDisks2 system-bus discovery no longer execute on the UI thread, and stale device-list generations are ignored.
  - Current: Devices now has a background monitor. UDisks2 system-bus signals trigger debounced refreshes, and a low-frequency snapshot poll catches missed mount-table or desktop-backend changes.
  - Current: `FIKA_DEBUG_DEVICES=1` prints device discovery and monitor diagnostics, including mountinfo/fallback usage, UDisks2 accepted rows, UDisks2 skip reasons, monitor refresh reasons, and the final merged sidebar device list.
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
- [x] Dolphin-style delayed main-view clearing for uncached directory navigation.
- [x] Per-directory main-view scroll position memory.
  - Current: remembered view states are cached with an LRU cap so long browsing sessions cannot keep every visited path's viewport state forever.
- [x] Patched non-fatal ICU4X CJK segmentation warning from `icu_segmenter 2.2.0`.
- [x] Mouse Back/Forward scoped to the right-side main pane only.
- [x] Adaptive Open With hover submenu placement.
- [x] Enable Slint experimental built-ins for future `DragArea` / `DropArea` DnD work.
- [x] Dolphin-style DnD self-drop rejection.
- [x] Focused Slint split: `models.slint`, `widgets.slint`, and `file_tile.slint`.
- [x] Focused Rust split for selection and Places UI logic.
- [x] Dolphin-style context menu hover polish.
  - Acceptance: submenu rows use explicit child indicators, child menus keep a timed grace area between parent and child, and drop operation menus include Cancel.
  - Current: parent menu, Open With, and Create New timer handling is centralized in local Slint helper functions, so delay/keep-alive behavior is no longer repeated across individual menu callbacks.
  - Current: Open With and Create New now share one child-submenu hover/timer entrypoint; parent rows, hover bridges, and child menu bodies all use the same keep-alive contract.
  - Current: Open With and Create New also share one active child-menu hover bridge instance, so `ui/app.slint` no longer carries duplicate bridge layout for the two submenu types.
  - Current: file item, Open With, Create New, Transfer, sidebar Places, Devices, Places blank-area, and main viewport context menus own their `PopupSurface` framing in `ui/menus.slint`, reducing repeated popup wrapper layout in `ui/app.slint`.
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
  - Acceptance: user-added places show Rename, Remove, and Open in New Window.
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

- [x] Drag external folder into Places.
  - Acceptance: dropping a folder path on Places adds it.
  - Current: Slint `DropArea` accepts `text/uri-list` / `text/plain`; winit `DroppedFile` remains as platform fallback. Both paths share the same Rust-side drop path normalization for uri-list comments, first valid local file entry, `file:///...` / `file://localhost/...`, remote file URI rejection, and percent decoding.
  - Current: Slint now passes payload plus MIME type to Rust; Rust owns MIME classification, parsing, force-gap policy, and backend source labeling, so Slint DropArea and winit fallback cannot drift in path semantics.
  - Current: Slint DropArea hover feedback and winit fallback slot selection both use the tested Rust `PlaceDropGeometry` over Places list geometry synchronized from Slint, so sidebar scrolling, target item detection, and visual insertion slots share one rule set.
  - Current: successful external Places drops include the handling backend in the status bar, so real desktop tests can distinguish Slint DropArea from the winit fallback before removing either path.
  - Current: `FIKA_DISABLE_WINIT_DROP_FALLBACK=1` disables only the winit `DroppedFile` event bridge, allowing real desktop tests to prove whether the Slint DropArea `text/uri-list` path works independently.
  - Current: `FIKA_DEBUG_DND=1` prints startup DnD configuration plus Slint DropArea and winit fallback drop traces with backend, phase, MIME type, coordinates, slot/target/gap/item state, and a compact payload summary for real desktop validation.
  - Current: Slint external-drop rejection diagnostics now distinguish unsupported MIME, empty payload, and payloads without a local file path; these reasons appear in debug traces and failed drops show specific status-bar guidance.

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
  - Current: thumbnail load results update only the thumbnail success/failure caches and pending map; visible tiles are refreshed by re-decorating the virtual slice, avoiding a full `entries` scan for every completed thumbnail.

- [x] Visible-first scheduling.
  - Acceptance: thumbnails visible in the viewport are generated before offscreen items.
  - Current: only the current virtual slice plus overscan is scheduled; stale thumbnail results clear only their matching pending key, so viewport changes or zoom changes cannot leave an item permanently stuck as pending.
  - Current: thumbnail jobs for the actually visible columns are queued before left/right overscan thumbnails, keeping large-directory scrolling responsive when many image previews are pending.
  - Current: viewport-only thumbnail scheduling reuses the active directory/zoom generation instead of invalidating in-flight thumbnail work on every scroll.

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
  - Current: Ctrl+C, Ctrl+X, Ctrl+V, and Ctrl+Z are declared with Slint `KeyBinding`; they operate on the selected files/current directory/last undo entry only when menus, dialogs, and text inputs are not active.
  - Current: Cut / Copy also publishes `x-special/gnome-copied-files` to the desktop clipboard through `wl-copy` or `xclip` when available; Copy falls back to `text/uri-list` if the desktop helper cannot publish the GNOME file-list MIME type.
  - Current: opening a context menu refreshes `x-special/gnome-copied-files` / `text/uri-list` from the desktop clipboard through `wl-paste` or `xclip` when Fika's internal clipboard is empty, so Paste can import file selections from other file managers.
  - Current: when importing `text/uri-list`, Fika also checks KDE/Dolphin's `application/x-kde-cutselection` marker so Dolphin Cut pastes as a move rather than a copy.
  - Current: when a desktop file clipboard is available, context-menu refresh replaces Fika's older internal clipboard state, so external Copy/Cut actions take precedence over stale in-app selections.
  - Current: internal and imported desktop clipboard paths are deduplicated while preserving order, so Paste does not enqueue duplicate transfers for the same source.
  - Current: context-menu clipboard refresh and Paste both validate clipboard paths before exposing or queueing transfers, drop entries that no longer exist, and clear the Paste affordance if all clipboard items are stale.
  - Current: Paste counts only transfers accepted by the transfer layer; rejected self/descendant, missing-source, or invalid-target entries do not inflate the queued count, and Cut clipboard state is cleared only after at least one move is accepted.

- [x] First-pass conflict handling.
  - Acceptance: copy/move/link transfers do not silently pick a conflict policy when the destination name exists.
  - Current: transfer conflicts prompt for Overwrite, Keep Both, Rename, or Skip before an operation enters the queue.
  - Current: Apply-to-remaining is limited to Skip, Keep Both, and Overwrite; Rename remains a single-conflict choice because one explicit target name is not safely reusable across unrelated conflicts. The conflict dialog calls this out when Apply-to-remaining is enabled.

- [x] First-pass operation undo.
  - Acceptance: completed copy/link operations can be undone by removing the created target.
  - Acceptance: completed move operations can be undone by moving the item back to its original path when that path is still free.
  - Current: the status bar exposes a one-step Undo action after copy/move/link operations, including overwrite conflicts. Overwrite keeps the replaced target as a temporary backup for the active undo entry; replacing that undo entry cleans the old backup.

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

- [x] Add XDP / `xdg-desktop-portal` integration design.
  - Acceptance: document how Fika maps to `org.freedesktop.impl.portal.FileChooser`.
  - Acceptance: identify process model, request lifecycle, cancellation, and chooser result format.
  - Current: `docs/DESIGN.md` documents the backend bus name, object path, OpenFile flow, cancellation behavior, and packaging metadata.

- [x] Add `zbus` XDP portal backend prototype.
  - Acceptance: backend can launch `fika --chooser` and return selected files.
  - Acceptance: backend implements the needed `org.freedesktop.impl.portal.FileChooser` methods for initial local-file use.
  - Current: `fika-xdp-filechooser` owns `org.freedesktop.impl.portal.desktop.fika`, exposes `/org/freedesktop/portal/desktop`, implements OpenFile / SaveFile / SaveFiles through `fika --chooser`, and returns local `file://` URIs.
  - Current: OpenFile supports directory and multiple selection flags; SaveFile and SaveFiles support local-path save target selection.
  - Current: the portal request title is passed to the chooser window title and accept_label is passed to the chooser confirmation button; portal glob filters are exposed as a chooser filter button, current_filter chooses the initial filter when it matches the supported glob filter list, and the selected filter is returned with the result.
  - Current: MIME-only portal filters are not exposed as empty chooser filters because the current Fika chooser UI can only express glob-pattern filtering.
  - Current: portal choices are exposed as chooser footer controls; clicking a choice opens a small option menu instead of blindly cycling, and the selected choices are returned with the result.
  - Current: recognized `wayland:` / `x11:` `parent_window` handles are preserved and forwarded to `fika --chooser --chooser-parent-window`; empty, malformed, or unknown handles are dropped. `FIKA_DEBUG_PORTAL=1` logs the backend parse decision and the chooser-side received handle, and both diagnostics explicitly report that native transient binding is still disabled. Native transient parent binding remains platform/window-backend work.
  - Current: the backend subscribes to the portal request handle's `org.freedesktop.impl.portal.Request.Close` signal while `fika --chooser` is running; request Close maps to portal response `1` and drops the chooser wait future. The chooser process is also launched with `kill_on_drop`, so Close, backend-side cancellation, or connection teardown do not leave an orphan chooser window.
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
  - Current: `data/dbus-1/interfaces/org.fika.FileManager1.Privileged.xml` defines CreateFolder / Rename / Trash / Transfer and external-edit writeback methods.

- [x] D-Bus privileged helper prototype.
  - Acceptance: GUI calls helper over D-Bus instead of spawning one-shot operation argv.
  - Current: GUI first invokes `org.fika.FileManager1.Privileged` on the system bus, letting D-Bus activation start `fika-privileged-helper --system-bus`.
  - Current: if the installable system service is unavailable in a dev checkout, GUI falls back to the older `pkexec --disable-internal-agent fika-privileged-helper --session-bus ...` path.

## Phase 11: Code Organization And Dolphin Parity Cleanup

- [x] Split common Slint data models and widgets.
  - Acceptance: reusable models, buttons, menu rows, Places rows, and file tiles are outside the main window file.
  - Current: `ui/models.slint`, `ui/widgets.slint`, `ui/menus.slint`, and `ui/file_tile.slint` are imported by `ui/app.slint`; common menu rows and popup surface styling live in `ui/widgets.slint`, while file item, Open With, Create New, Transfer, Places, and viewport menu content is isolated in `ui/menus.slint`.
  - Current: dialog bodies and centered popup wrappers live in `ui/dialogs.slint`, so `ui/app.slint` keeps dialog action wiring without repeating the transparent centering shell for every modal.

- [x] Split pure selection logic out of `main.rs`.
  - Acceptance: filtering, visible-path retention, range selection, rectangle selection, and append-unique behavior are testable outside UI callbacks.
  - Current: `src/app/selection.rs` owns these helpers; existing selection tests still pass.

- [x] Split Places UI logic out of `main.rs`.
  - Acceptance: add/rename/remove/restore/reorder/drop handling for Places is grouped away from main callback wiring.
  - Current: `src/app/places.rs` owns Places persistence and drop normalization helpers.

- [x] Split virtual main-view preparation out of `main.rs`.
  - Acceptance: large-directory viewport slicing and rebuild decisions are testable outside UI callback wiring.
  - Current: `src/app/virtual_view.rs` prepares clamped viewport state and the current virtual `FileEntry` slice; `main.rs` only applies Slint properties and schedules visible thumbnails.

- [x] Apply Dolphin DnD target validation.
  - Acceptance: dropping an item onto itself, or a folder into its own descendant, does not open the transfer menu and shows a status message.
  - Current: transfer preparation and execution both reject self/descendant targets.

- [x] Apply Dolphin-like context menu grouping and submenu grace.
  - Acceptance: context menus use grouped separators, submenu indicators are separate from labels, child menus anchor to their parent row and have a hover bridge to avoid accidental disappearance while moving between parent and child.
  - Current: Open With and Create New submenus share delayed-close timers, one active child-menu placement/hover-bridge property set, and reusable invisible bridge hit areas; timer start/stop, parent+child close behavior, and context-menu opening are routed through local Slint helper functions; viewport menu order follows Dolphin more closely with Create New first.

- [x] Apply Dolphin/QMenu-style popup placement.
  - Acceptance: menus prefer the requested popup point, flip if they would overflow the safe rect, and clamp when the window is too small.
  - Current: context, Open With, Create New, transfer, and chooser-choice popup surfaces use shared Rust `PopupPlacement` geometry, reusable popup surface styling, and a common outside-click dismiss layer.
  - Current: root context placement, Transfer placement, Open With / Create New child placement plus hover bridge geometry, and chooser-choice above-button placement now use Rust helpers, reducing duplicated popup positioning logic in `ui/app.slint`.
  - Current: helper exits after idle time when no external edit tokens are active.

- [x] Per-method polkit authority check.
  - Acceptance: helper asks polkit authority for `org.fika.FileManager.privileged-helper` per protected operation when the packaged action is installed.
  - Acceptance: missing packaged policy gives a clear diagnostic and does not fall back to unsafe writes.
  - Current: system-bus helper uses `org.freedesktop.PolicyKit1.Authority.CheckAuthorization` for every D-Bus method. Polkit failures include the action id and `org.fika.FileManager.policy` installation hint. The session-bus pkexec fallback keeps uid matching only for development.
  - Current: privileged helper fallback errors distinguish system-bus activation, development session-bus helper, and pkexec startup failures, and include the policy/polkit-agent installation hint.

- [x] Install data helper.
  - Acceptance: packagers can install D-Bus, polkit, and portal metadata without hand-editing template paths.
  - Current: `scripts/install-data.sh` expands `@bindir@` and installs system bus service, bus policy, polkit action, D-Bus interface XML, portal service, and portal descriptor under `DESTDIR` / `PREFIX` aware paths.
  - Current: `scripts/check-install-data.sh` performs a non-root install into a temporary `DESTDIR` and verifies expected file locations, `@bindir@` expansion, root system-bus activation, D-Bus send policy, exported privileged methods, polkit defaults, polkit prompt text, portal backend metadata, and absence of placeholder metadata such as `example.invalid`.

- [x] External editor writeback flow.
  - Acceptance: protected files open as ordinary scratch paths under `/run/user/$UID/fika-edit`.
  - Acceptance: writeback uses `CommitExternalEdit`, not a root editor or user-visible admin URI.
  - Current: default Open / Open With / custom Open With fall back to scratch on permission errors; status bar exposes Save Back and Discard for pending protected edits.
  - Current: helper watches scratch files and writes back on save, so GUI can close after launching the editor; Save Back remains a manual flush/cleanup action.

- [x] Helper-owned external editor lifecycle.
  - Acceptance: helper can track editor systemd unit lifetime or token expiry and clean scratch files without relying on the GUI.
  - Acceptance: closing Fika windows does not leave unbounded helper lifetime.
  - Current: helper tracks associated systemd unit lifetime and cleans scratch after unit exit; tokens without a unit are expired after a bounded TTL with a final writeback attempt.

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
