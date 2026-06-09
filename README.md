# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://blog.rust-lang.org/2024/02/08/Rust-1.76.0.html)

A lightweight file manager for modern Wayland desktops. The current
implementation is Rust + [Slint](https://slint.dev); the active target is a
full GPUI rewrite with Dolphin as the first behavioral reference.

**Status:** Migration planning. The Slint app is the legacy implementation and
source of reusable Rust modules; new UI architecture work should follow
[docs/TODO.md](docs/TODO.md) and
[docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md).

> [中文版 / Chinese](README.zh-CN.md)

## Features

### File Browsing

- Browse local directories with breadcrumb navigation and direct path entry
- Directory history: back/forward, mouse side-button navigation
- Places sidebar (built-in + user-defined, with drag-to-reorder, rename, open in new window)
- Devices sidebar: storage device discovery via UDisks2 with mount/unmount/eject, pending and error states
- Debounced directory monitoring (inotify) with auto-refresh
- Lightweight virtualized main view: horizontal column-first, Dolphin-style compact layout that stays responsive in large directories
- Split View: preview two directories side by side, swappable focus, independent scrolling, resizable divider

### Search

- Real-time recursive search with progress reporting and cancel
- File type filters: folders, documents, images, audio, video
- Results grouped by relative location, sorted by path
- Search strip with expandable filter chips, using scoped FlexboxLayout for responsive wrapping

### File Operations

- Async file operation queue: copy, move, link, trash, rename
- Conflict handling and one-step undo
- Internal drag-and-drop transfer menu (move / copy / link)
- Rubber-band and multi-selection
- Clipboard integration (Ctrl+C/X/V)
- Create New Folder / New File from context menu
- Duplicate Here, Copy Location, Rename
- Paste image, video, and text content from the Wayland clipboard as files

### Trash Management

- Move to Trash (Delete key) with per-file `.trashinfo` metadata
- Restore From Trash: reads original location from `.trashinfo`
- Delete Permanently (Trash context only, with confirmation)
- Empty Trash with confirmation dialog
- Trash view shows original location and deletion date, sorted by newest deletion first
- Trash directory monitoring: watches both `files/` and `info/` for external changes

### UI / UX

- Light and dark theme switching
- Resizable sidebar and split pane ratio (persisted)
- Guarded minimum window dimensions to prevent content overflow
- COSMIC-style shell surface layering, Dolphin-style compact main file view
- Ctrl+scroll to zoom icon size
- Right-click context menus: file, folder, blank-area, and Places

### Desktop Integration

- Built-in MIME type detection and default application launching (no `xdg-open` dependency)
- Open With menu: default app, added associations, cached associations
- Open With submenu and application chooser dialog ("Other Applications…")
- Set default application (writes user-level `mimeapps.list`)
- Application launching through systemd user scopes for process lifecycle tracking

### Service Menu

- User-installed service menu `.desktop` entries on right-click
- Discovers Fika-owned `fika/servicemenus` and KDE/Dolphin `kio/servicemenus` directories
- MIME-type and multi-selection filtering
- Shell-free Exec field code expansion (`%f`, `%F`, `%u`, `%U`, `%d`, `%n`)
- Submenu grouping and top-level action sorting

### Thumbnails

- Async thumbnail generation: built-in support for PNG / JPEG / WebP
- In-memory LRU cache + disk cache (conforms to the [freedesktop.org Thumbnail Managing Standard](https://specifications.freedesktop.org/thumbnail-spec/))
- External thumbnailer support: auto-discovers XDG `.thumbnailer` entries for PDF / SVG / AVIF and more
- Failure cache: avoids re-queuing broken or unsupported images on repeated scrolls
- Visible-first scheduling: viewport thumbnails generated before offscreen items

### File Chooser / Portal

- Lightweight chooser mode (`--chooser`), usable as an `xdg-desktop-portal` FileChooser backend
- `fika-xdp-filechooser` binary: exposes the `org.freedesktop.impl.portal.FileChooser` D-Bus interface
- Independent of GNOME / KDE / COSMIC / GTK portal backends

### Security

- GUI process is intentionally non-privileged
- Protected operations go through a separate system-bus D-Bus helper (`fika-privileged-helper`)
- Per-method Polkit authorization
- Protected external editor: scratch copy + automatic writeback via systemd unit lifecycle monitoring

### State Persistence

- Window size, sidebar width, split pane ratio
- Dark mode preference
- Icon zoom level
- Last opened directory
- Settings stored in `$XDG_CONFIG_HOME/fika/settings.tsv`

### Migration Direction

Future work targets GPUI + a UI-neutral Rust core. Dolphin's `DolphinView →
KDirLister → KFileItemModel → KItemListView` execution flow is the primary
reference for directory loading, refresh, undo, split pane identity, and model
signals.

See:

- [docs/TODO.md](docs/TODO.md)
- [docs/DESIGN.md](docs/DESIGN.md)
- [docs/REFERENCE.md](docs/REFERENCE.md)
- [docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md)

## Prerequisites

- Rust 1.76+ (2024 edition)
- Linux (Wayland)
- Slint build dependencies: CMake, pkg-config, fontconfig, libxkbcommon

Arch Linux:

```sh
sudo pacman -S cmake pkgconf fontconfig libxkbcommon
```

## Quick Start

```sh
# Build
cargo build --release

# Run as a file manager
cargo run

# Run as a file chooser
cargo run -- --chooser ~/Downloads

# Diagnose device discovery (no GUI)
cargo run -- --diagnose-devices

# Full CLI help
cargo run -- --help
```

## CLI Reference

```
fika [options] [start-directory]
```

### Modes

| Option | Mode | Description |
|--------|------|-------------|
| *(default)* | Manager | Standard file manager window |
| `--chooser` | Chooser | File chooser mode; selected paths are printed to stdout |
| `--diagnose-devices` | Diagnostics | Print device discovery info, no GUI |

### Chooser Mode Options

| Option | Description |
|--------|-------------|
| `--chooser-directory` | Select directories only |
| `--chooser-multiple` | Allow multi-selection |
| `--chooser-save <name>` | Save-file dialog mode |
| `--chooser-save-files <names>` | Save-file with preset filenames (newline-separated) |
| `--chooser-title <text>` | Custom window title |
| `--chooser-accept-label <text>` | Custom accept button label |
| `--chooser-filters <filters>` | File filters (newline-separated, alternating `name\npattern`) |
| `--chooser-filter-index <n>` | Default selected filter index |
| `--chooser-return-filter` | Output the selected filter index |
| `--chooser-choices <choices>` | Additional choice widgets (newline-separated `id\nlabel\nvalue` triples) |
| `--chooser-return-choices` | Output choice widget state |
| `--chooser-parent-window <handle>` | Parent window handle (for portal embedding) |

In chooser mode, select an item and press **Choose** to print the path to stdout and exit. When `--chooser-return-filter` or `--chooser-return-choices` is used, extra metadata is printed with `FIKA_CHOOSER_FILTER\t` and `FIKA_CHOOSER_CHOICE\t` prefixes.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl + C` | Copy selected files to clipboard |
| `Ctrl + X` | Cut selected files to clipboard |
| `Ctrl + V` | Paste files into current directory |
| `Ctrl + A` | Select all visible files |
| `Ctrl + F` | Open search |
| `Ctrl + Z` | Undo last file operation |
| `Delete` | Move selected files to trash (disabled inside Trash) |
| `F5` | Refresh current directory |
| `Escape` | Clear selection / close popups / exit search |
| `Ctrl + Scroll` | Zoom icon size |
| `Mouse back button` | Navigate back to previous directory |

File operation shortcuts (Ctrl+C/X/V/Z/Delete) are blocked while the search box,
save-filename input, or any transient popup is open, to prevent accidental
operations.

## Desktop Integration

Packaged installation deploys D-Bus service files, Polkit policy, and portal
metadata alongside the binaries.

### Install data files

```sh
sudo PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
```

### Staged testing (no root required)

```sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika scripts/install-data.sh
DESTDIR=/tmp/fika-root PREFIX=/usr BINDIR=/usr/lib/fika \
  scripts/check-runtime-integration.sh --metadata-only
```

### Verify runtime integration

After installation, run:

```sh
scripts/check-runtime-integration.sh
```

This checks that the system-bus helper, Polkit action, and portal backend
metadata are correctly installed, and prints a runtime environment summary
(distribution, desktop environment, `portals.conf` location). Add
`--activate-system-helper` to confirm D-Bus activation of the privileged helper
without invoking any privileged file-operation method:

```sh
scripts/check-runtime-integration.sh --activate-system-helper
```

### Portal backend configuration

Installing `fika.portal` only registers the backend; it does **not** make Fika
the active FileChooser. To try the Fika backend, opt in through
`xdg-desktop-portal` configuration — copy the shape shown in
`docs/examples/fika-portals.conf` into the appropriate user or system
`portals.conf`.

## Environment Variables

### Customization

| Variable | Description | Example |
|----------|-------------|---------|
| `FIKA_ICON_THEME` | Override icon theme | `FIKA_ICON_THEME=Papirus` |
| `FIKA_GUI` | Override portal backend frontend binary path | Debug use |
| `FIKA_PRIVILEGED_HELPER` | Override privileged helper binary path | Debug use |

### Debugging

| Variable | Description |
|----------|-------------|
| `FIKA_DEBUG_DEVICES=1` | Print device discovery and monitor diagnostics |
| `FIKA_DEBUG_DND=1` | Print drag-and-drop diagnostics |
| `FIKA_DEBUG_PORTAL=1` | Print portal diagnostics |
| `FIKA_DEBUG_NAV=1` | Print navigation diagnostics |
| `FIKA_DEBUG_PRIVILEGE=1` | Print privileged operation diagnostics |

## Architecture

```
src/
├── main.rs          Entry point, Slint UI callback implementations
├── lib.rs           Crate root
├── config/          CLI argument parsing, paths, settings persistence,
│                    service menu policy
├── app/             UI-thread shared state, async event bridge,
│                    directory loading, DnD, Places, main-view
│                    virtualization, selection, thumbnail pipeline,
│                    split view, search UI, context menu routing,
│                    chooser and device monitor
├── desktop/         Built-in MIME / default-app resolution,
│                    Open With, application chooser, Wayland clipboard,
│                    icon lookup, service menu discovery and launch,
│                    systemd user scope integration
├── fs/              File entries, file operations, device discovery
│                    (UDisks2 + mountinfo), Places backend, recursive
│                    search, thumbnails, privilege escalation
├── support/         Chooser output, generation counters
└── bin/
    ├── fika-privileged-helper.rs   System-bus D-Bus privileged helper
    └── fika-xdp-filechooser.rs     XDG Desktop Portal FileChooser backend
```

The GUI process is intentionally non-privileged. Protected file operations go
through a system-bus D-Bus helper with per-method Polkit authorization.

Detailed design documents:
- [docs/DESIGN.md](docs/DESIGN.md) — Architecture and subsystem design
- [docs/TODO.md](docs/TODO.md) — Implementation roadmap and acceptance criteria
- [docs/REFERENCE.md](docs/REFERENCE.md) — Detailed bilingual (zh/en) reference
- [docs/OPTIMIZATION.md](docs/OPTIMIZATION.md) — Performance optimization notes

## License

[MIT](LICENSE)
