# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika is a Rust file-manager shell for Linux desktops. The active implementation
is a GPUI package built around a UI-neutral core and Dolphin-inspired directory
lister/model flow.

GPUI is pulled from the official Zed repository:
`https://github.com/zed-industries/zed`. The manifest does not pin GPUI to a
crate release, branch, revision, or concrete numeric version.

> [中文版 / Chinese](README.zh-CN.md)

## Current Scope

The current cutover build contains:

### Core

- UI-neutral directory lister and model (`fika-core`, `src/core/`).
- Listing worker with per-pane request coalescing, fresh/stale cache, and
  cancellable `read_dir`.
- Directory cache with LRU eviction, entry budget, and shared `Arc<Vec<Entry>>`
  across split panes.
- Pane identity, pane state, split/close routing via stable `PaneId`.
- Pane-local selection, direction-key navigation, Shift-range, Ctrl/secondary
  toggle, and rubber-band selection.
- Current-directory-removed fallback to the nearest existing ancestor.
- Navigation history (Back/Forward) per `PaneId`.
- Compact file-view layout (column-first, per-column width cache, visible-range
  projection, hit-test, viewport math).
- Smooth-scroll easing, kinetic tracker, retarget, and scroll-clamp in core.
- Zoom level mapping (Dolphin-style 0–16 → 16–256 px icon size).
- File operation primitives: copy, move, link, trash, create, rename, undo.
- Privileged operation API surface for protected filesystem actions.
- Location resolution: `~` expansion, absolute/relative paths, breadcrumb
  segments, filesystem tab-completion.
- MIME type detection via shared-mime-info globs, suffixes, and magic bytes.
- Application launcher: `.desktop` parser, `mimeapps.list` Default/Added/Removed
  associations, `XDG_DATA_DIRS` application cache, systemd user transient unit
  launch.
- KDE service-menu parser: `X-KDE-ServiceTypes`, `X-KDE-Priority/TopLevel`,
  `X-KDE-Submenu`, protocol and URL-count conditions.
- Ark archive integration: classifier, DnD extraction via session bus,
  Compress/Extract fallback commands.
- Clipboard model: URI-list encode/decode, GPUI `ClipboardItem` round-trip,
  primary/clipboard selection import, paste-text create.
- Filter model: plain-text and glob name matching, filtered model projection,
  cache invalidation.
- Trash: `$XDG_DATA_HOME/Trash` metadata read, restore, delete-permanently,
  empty, sort by deletion time.
- Thumbnails: freedesktop thumbnail URI, cache key, cache hit, failure marker,
  `EntryData` path role.
- GIO/GVfs device discovery: mount/volume monitor snapshots, removable devices
  section, mount/unmount/eject operations.
- Network/GVfs remote filesystem classification and Places Network root.
- D-Bus bus controller: session/system connection cache, timeout/retry helpers,
  owned proxy creation, structured `BusError`.
- Async runtime scaffolding: `tokio` for general async and D-Bus; `compio` for
  completion-based file I/O (behind feature flag).

### UI (GPUI)

- Manager window with directory pane, pane shell, toolbar, and header.
- Dynamic split panes (`Split` / `Close Pane` via keyboard shortcut).
- Pane-local location bar (breadcrumb mode and editable text mode with caret,
  horizontal scroll, and Tab completion).
- Pane-local status bar: selection summary, free-space info, zoom slider,
  progress bar with Stop button for file operations and directory loading.
- Pane-local filter bar: plain-text/glob toggle, case-sensitive toggle, match
  count, and close button.
- Places sidebar: Home, XDG user dirs, Trash, removable devices, Root, Network;
  user bookmark persistence (`user-places.xbel`); right-click actions (Open,
  Open in New Pane, Add, Edit, Remove, Copy Location, Properties, Empty Trash);
  rounded style and themed icons.
- Compact file grid with visible-item virtualization, slot-pool element reuse
  (capped at 100 recycled slots), and GPU-composited scroll translation.
- Horizontal scrollbar: live canvas bounds, paint-phase capture-move tracking,
  reserve-area measured-track fallback, and handle-grab offset preservation.
- Rubber-band (box) selection: viewport-local projection, drag clamp, exclusion
  from scrollbar/pane-chrome areas.
- Context menu: target/action/item/icon model; root, submenu, and nested
  submenu rendering; service-menu grouping; Open With dynamic submenu; Ark
  fallback grouping; viewport clamp/flip positioning.
- Open With "Other Application…" chooser: `uniform_list` virtual list, visible
  icon range, Set Default write-back to `mimeapps.list`.
- Drag and drop: item/place drag source, directory/item/blank/pane drop target,
  `.desktop` application DnD, external file drops via GPUI `ExternalPaths`,
  Copy/Move/Link drop menu with hover feedback, Places bookmark insertion and
  reorder.
- Inline rename: pane-local draft state, text input, Enter/Escape
  commit/cancel.
- Properties dialog: single-path and multi-select metadata rows.
- Clipboard interaction: internal Copy/Cut/Paste with progress bar and undo;
  primary-selection paste via middle-click.
- Chooser shell: file/directory selection, multi-select, filter/choice/portal
  metadata output.
- Keyboard shortcuts: pane-scoped navigation, selection, zoom, filter,
  clipboard, undo, and text-input classification.

### Binaries and Integration

- `fika` — main GPUI application and chooser shell.
- `fika-xdp-filechooser` — XDG Desktop Portal FileChooser backend.
- `fika-privileged-helper` — system-bus helper for protected operations.
- D-Bus service files, Polkit policy, and portal metadata under `data/`.

The older UI implementation has been removed from the main tree. Work that is
not present in the GPUI package should be treated as future implementation, not
an active feature.

## Layout

```text
src/
  lib.rs                         UI-neutral core module exports
  main.rs                        GPUI application and chooser shell
  core.rs                        Core module re-exports
  core/archive.rs                Ark DnD extraction and classification
  core/bus.rs                    D-Bus session/system bus controller
  core/cache.rs                  Directory entry cache (LRU, per-pane)
  core/clipboard.rs              URI-list encode/decode and GPUI round-trip
  core/devices.rs                GIO/GVfs device discovery entry point
  core/devices/actions.rs        Mount/unmount/eject/safely-remove ops
  core/directory.rs              Directory lister and watcher events
  core/entries.rs                File entry metadata and sorting
  core/file_ops.rs               File transfer/trash/create/rename primitives
  core/filter.rs                 Name filter model (plain-text, glob)
  core/launcher.rs               .desktop / mimeapps.list app discovery
  core/launcher/ark.rs           Ark archive launch plan construction
  core/launcher/results.rs       Launch result types
  core/listing_worker.rs         Background directory-read worker
  core/location.rs               Path resolution, breadcrumbs, tab-completion
  core/mime.rs                   MIME detection via shared-mime-info
  core/model.rs                  Directory model snapshots and signals
  core/network.rs                GVfs/remote filesystem classification
  core/operations.rs             Operation queue and undo boundary
  core/operations/tasks.rs       File operation task result types
  core/pane.rs                   Pane identity, state, split/close routing
  core/places.rs                 Places model (bookmarks, devices, network)
  core/privilege.rs              Privileged operation API surface
  core/scroll.rs                 Smooth-scroll easing and kinetic tracker
  core/thumbnails.rs             Freedesktop thumbnail URI and cache keys
  core/view.rs                   Compact layout, viewport math, visible range
  ui.rs                          UI module re-exports
  ui/application_chooser.rs      "Other Application…" chooser entry point
  ui/application_chooser/
    identity.rs                  Application chooser item identity
  ui/chooser.rs                  File chooser mode entry point
  ui/chooser/state.rs            Chooser state and portal metadata output
  ui/clipboard.rs                Clipboard UI entry point
  ui/clipboard/state.rs          Copy/cut mode and GPUI ClipboardItem state
  ui/context_menu.rs             Context menu target, action, icon model
  ui/controls.rs                 Shared UI control helpers
  ui/drag_drop.rs                Drag-drop UI entry point
  ui/drag_drop/state.rs          DnD state, path normalization, target matching
  ui/file_grid.rs                File grid UI entry point
  ui/file_grid/layout.rs         Compact column-width cache and layout assembly
  ui/file_grid/slots.rs          Visible-item slot pool (recycled IDs)
  ui/file_grid/snapshot.rs       Visible-item snapshot data
  ui/filter_bar.rs               Filter bar UI entry point
  ui/filter_bar/state.rs         Filter snapshot and filtered model cache
  ui/icons.rs                    File/named icon entry point
  ui/icons/cache.rs              FileIconCache, MIME candidate, theme resolve
  ui/location_bar.rs             Location bar UI entry point
  ui/location_bar/draft.rs       Editable location draft and caret state
  ui/location_bar/metrics.rs     Editable metrics, hit-test, and scroll math
  ui/pane.rs                     Pane shell UI entry point
  ui/pane/snapshot.rs            Pane rendering snapshot
  ui/pane/splitter.rs            Splitter drag payload and ratio geometry
  ui/place_draft.rs              Places Add/Edit draft state
  ui/places.rs                   Places sidebar UI entry point
  ui/places/model.rs             Place entry, grouping, and icon snapshots
  ui/places/snapshot.rs          Place icon and snapshot types
  ui/properties_dialog.rs        Properties dialog entry point
  ui/properties_dialog/
    metadata.rs                  File metadata reader and row generation
  ui/rename.rs                   Inline rename entry point
  ui/rename/draft.rs             Pane-local rename draft state
  ui/rubber_band.rs              Rubber-band selection entry point
  ui/rubber_band/state.rs        Rubber-band drag payload and rect projection
  ui/scrollbar.rs                Horizontal scrollbar, paint-phase capture
  ui/shortcuts.rs                Keyboard shortcut classification
  ui/status_bar.rs               Status bar UI entry point
  ui/status_bar/state.rs         Snapshot, space info cache, progress handle
  ui/status_bar/summary.rs       Pane selection/model summary formatting
  src/bin/
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser backend
    fika-privileged-helper.rs    System-bus privileged helper
```

The root manifest is a single Cargo package. It exposes the `fika_core` library
from `src/lib.rs` (via `src/core.rs`) and builds the `fika`,
`fika-xdp-filechooser`, and `fika-privileged-helper` binaries from
`src/main.rs` and `src/bin/`.

## Build

Prerequisites:

- Rust with the 2024 edition toolchain.
- Linux desktop development libraries needed by GPUI, GIO/GVfs, and zbus.
- Network access the first time Cargo fetches the Zed repository dependencies.

Build and run:

```sh
cargo build
cargo run -- /path/to/start
```

Run the chooser shell:

```sh
cargo run -- --chooser ~/Downloads
cargo run -- --chooser-directory --chooser-multiple ~/Downloads
```

Run checks:

```sh
cargo fmt --all
cargo test
cargo check
```

## CLI

```text
fika [options] [start-directory]
```

| Option | Description |
| --- | --- |
| `--chooser` | Start in file chooser mode. |
| `--chooser-directory` | Select directories instead of files. |
| `--chooser-multiple` | Select more than one path before confirmation. |
| `--chooser-title <text>` | Set the chooser window title. |
| `--chooser-accept-label <text>` | Set the chooser action label. |
| `--chooser-filter-index <n>` | Return `n` as selected filter metadata. |
| `--chooser-return-filter` | Print selected filter metadata before paths. |
| `--chooser-choices <list>` | Preserve portal choice metadata. |
| `--chooser-return-choices` | Print selected choice metadata before paths. |
| `--chooser-parent-window <handle>` | Accept the portal parent-window argument. |
| `-h`, `--help` | Print help. |

The chooser prints paths to stdout. When requested, metadata rows are printed
before paths with `FIKA_CHOOSER_FILTER` and `FIKA_CHOOSER_CHOICE` prefixes.

## Desktop Integration

Packaged installation deploys D-Bus service files, Polkit policy, and portal
metadata alongside the binaries.

```sh
sudo PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
scripts/check-runtime-integration.sh
```

Installing `fika.portal` only registers the backend. To make it the active
FileChooser backend, opt in through `xdg-desktop-portal` configuration. See
[docs/examples/fika-portals.conf](docs/examples/fika-portals.conf).

## Documentation

### Architecture and Planning

- [docs/DESIGN.md](docs/DESIGN.md) — Current GPUI/core architecture and subsystem boundaries.
- [docs/TODO.md](docs/TODO.md) — Remaining implementation tasks and active blockers.
- [docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md) — Original cutover plan from removed UI to GPUI.
- [docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md](docs/DOLPHIN_ITEM_SLOT_REUSE_PLAN.md) — Archived slot-reuse design notes.
- [docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md](docs/SCROLL_ZOOM_PERFORMANCE_PLAN.md) — Archived scroll/zoom performance plan.
- [docs/OPTIMIZATION.md](docs/OPTIMIZATION.md) — Archived optimization notes.
- [docs/BUG_ANALYSIS_BLANK_DIRECTORY.md](docs/BUG_ANALYSIS_BLANK_DIRECTORY.md) — Blank-directory bug analysis.

### Dolphin / Fika Reference

- [docs/REFERENCE.md](docs/REFERENCE.md) — Dolphin-to-Fika concept mapping and engineering checks.
- [docs/LOCATION_BAR_REFERENCE.md](docs/LOCATION_BAR_REFERENCE.md) — Dolphin `KUrlNavigator` breadcrumb and editable modes.
- [docs/ZOOM_REFERENCE.md](docs/ZOOM_REFERENCE.md) — Dolphin zoom level, icon-size mapping, and grid update.
- [docs/STATUS_BAR_REFERENCE.md](docs/STATUS_BAR_REFERENCE.md) — Dolphin `DolphinStatusBar` info display and zoom slider.
- [docs/SMOOTH_SCROLL_REFERENCE.md](docs/SMOOTH_SCROLL_REFERENCE.md) — Dolphin `QScroller` smooth/kinetic scrolling.
- [docs/SEARCH_REFERENCE.md](docs/SEARCH_REFERENCE.md) — Dolphin search box and KIO search integration.

### Interaction Reference

- [docs/CONTEXT_MENU_REFERENCE.md](docs/CONTEXT_MENU_REFERENCE.md) — Dolphin context menu complete execution flow.
- [docs/DRAG_DROP_REFERENCE.md](docs/DRAG_DROP_REFERENCE.md) — Dolphin drag-and-drop execution flow.
- [docs/CLIPBOARD_REFERENCE.md](docs/CLIPBOARD_REFERENCE.md) — Dolphin / KIO file clipboard and GPUI round-trip.

### System Integration Reference

- [docs/MIME_LAUNCHER_REFERENCE.md](docs/MIME_LAUNCHER_REFERENCE.md) — MIME detection, application launching, systemd.
- [docs/DEVICES_REFERENCE.md](docs/DEVICES_REFERENCE.md) — GIO/GVfs device discovery, mount/unmount/eject.
- [docs/TRASH_REFERENCE.md](docs/TRASH_REFERENCE.md) — XDG Trash spec and Dolphin trash implementation.
- [docs/THUMBNAIL_REFERENCE.md](docs/THUMBNAIL_REFERENCE.md) — Freedesktop thumbnail spec and pipeline.
- [docs/NETWORK_REFERENCE.md](docs/NETWORK_REFERENCE.md) — GVfs remote filesystem classification and mounts.
- [docs/BUS_CONTROL_REFERENCE.md](docs/BUS_CONTROL_REFERENCE.md) — D-Bus bus control, zbus connections, systemd/Portal routing.
- [docs/ARK_REFERENCE.md](docs/ARK_REFERENCE.md) — Ark/kerfuffle archive integration and D-Bus interface.

## License

[MIT](LICENSE)
