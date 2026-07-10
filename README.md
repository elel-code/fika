# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika is a Linux-focused Rust file manager. The UI mainline is now the default
`fika` binary, a Fika-specific `winit + wgpu` shell; the previous UI runtimes
have been removed from the source tree.

> [中文版 / Chinese](README.zh-CN.md)

## Current Runtime

- `fika` is the default run target and the only in-tree file-manager UI.
- `winit` currently uses `slint-ui/winit` `drag-n-drop` for the in-flight
  cross-platform DnD API.
- `wgpu` comes from the official crates.io release.
- `fika-core` stays UI-neutral and owns filesystem/domain behavior.
- Clipboard integration uses Wayland `wl_data_device` directly; paste does not
  shell out to `wl-paste`, `wl-copy`, or `xclip`.
- Portal and privileged-helper binaries remain separate integration pieces.

## Source Layout

```text
src/
  lib.rs                         UI-neutral core exports
  main.rs                        winit/wgpu shell entry point
  core.rs                        Core module re-exports
  cli.rs                         Shared CLI parsing entry point
  cli/
    args.rs                      Manager/chooser argument parsing
  core/                          Directory, pane, operations, launcher,
                                 Places, devices, thumbnails, trash, D-Bus
  shell/                         Extracted shell modules
  bin/
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser backend
    fika-privileged-helper.rs    D-Bus helper for privileged operations
```

## Build And Run

```bash
cargo run --bin fika -- --view compact /etc
cargo test --bin fika
```

Because `default-run` is `fika`, this also starts the current shell:

```bash
cargo run -- /etc
```

## Architecture Notes

- Pane state is routed by stable pane identity and stored through reusable pane
  containers, so split panes use the same state/projection/slot-pool path.
- Hot item views are retained and virtualized: visible-slot reuse, cached
  projection, cached text/icon atlas work, and explicit scroll metrics.
- The shell hot path uses MIME/icon role reuse by role + size, queued
  read-ahead, dirty-subrect atlas uploads, and tighter icon theme cache
  ownership.
- Core behavior follows Dolphin as the first reference for file-manager
  semantics, while the shell owns rendering, hit testing, DPI, input routing,
  overlays, and telemetry.

## Reference Docs

- [docs/DEVICES_REFERENCE.md](docs/DEVICES_REFERENCE.md) — devices and Places
  behavior.
- [docs/NETWORK_REFERENCE.md](docs/NETWORK_REFERENCE.md) — network locations
  behavior.
- [docs/PERFORMANCE_ALIGNMENT.md](docs/PERFORMANCE_ALIGNMENT.md) — Dolphin-first
  performance reference policy.
- [docs/TRASH_REFERENCE.md](docs/TRASH_REFERENCE.md) — trash behavior.
