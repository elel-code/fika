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

- GPUI manager window with a directory pane.
- Dynamic split panes with stable `PaneId` routing.
- UI-neutral directory lister and model in `fika-core`.
- Pane-scoped reload and filesystem watcher refresh.
- Current-directory-removed fallback to the nearest existing ancestor.
- Pane-local selection, navigation shortcuts, move-to-trash, and undo refresh.
- Minimal GPUI chooser mode that prints selected paths and portal metadata.
- XDG Desktop Portal FileChooser backend binary.
- System-bus privileged helper binary boundary.

The older UI implementation has been removed from the main tree. Work that is
not present in the GPUI package should be treated as future implementation, not
an active feature.

## Layout

```text
src/
  lib.rs                     UI-neutral core module exports
  directory.rs               Directory lister and watcher event classification
  entries.rs                 File entry metadata and sorting input
  model.rs                   Directory model snapshots and model signals
  pane.rs                    Pane identity, pane state, split/close routing
  operations.rs              Operation queue and undo payloads
  file_ops.rs                File transfer/trash/create/rename primitives
  privilege.rs               Privileged operation API surface
  main.rs                    Main `fika` GPUI application and chooser shell
  bin/fika-xdp-filechooser.rs
                             XDG Desktop Portal FileChooser backend
  bin/fika-privileged-helper.rs
                             System-bus helper for protected operations
```

The root manifest is a single Cargo package. It exposes the `fika_core` library
from `src/lib.rs` and builds the `fika`, `fika-xdp-filechooser`, and
`fika-privileged-helper` binaries from `src/main.rs` and `src/bin/`.

## Build

Prerequisites:

- Rust with the 2024 edition toolchain.
- Linux desktop development libraries needed by GPUI and zbus.
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

- [docs/DESIGN.md](docs/DESIGN.md) - Current GPUI/core architecture.
- [docs/TODO.md](docs/TODO.md) - Remaining implementation tasks.
- [docs/REFERENCE.md](docs/REFERENCE.md) - Dolphin and Fika reference index.
- [docs/GPUI_DOLPHIN_MIGRATION_PLAN.md](docs/GPUI_DOLPHIN_MIGRATION_PLAN.md) - Original cutover plan.

## License

[MIT](LICENSE)
