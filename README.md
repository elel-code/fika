# Fika

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/rust-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)

Fika is a Linux-focused Rust file manager. The UI mainline is now the
Fika-specific `winit + wgpu` shell in `fika-wgpu`; the previous UI runtimes have
been removed from the source tree.

> [中文版 / Chinese](README.zh-CN.md)

## Current Runtime

- `fika-wgpu` is the default run target and the only in-tree file-manager UI.
- `winit` comes from official upstream `rust-windowing/winit` `master`.
- `wgpu` comes from the official crates.io release.
- `fika-core` stays UI-neutral and owns filesystem/domain behavior.
- Portal and privileged-helper binaries remain separate integration pieces.

## Source Layout

```text
src/
  lib.rs                         UI-neutral core exports
  core.rs                        Core module re-exports
  cli.rs                         Shared CLI parsing entry point
  cli/
    args.rs                      Manager/chooser argument parsing
  core/                          Directory, pane, operations, launcher,
                                 Places, devices, thumbnails, trash, D-Bus
  bin/
    fika-wgpu.rs                 winit/wgpu shell entry point
    fika_wgpu/                   Extracted shell modules
    fika-xdp-filechooser.rs      XDG Desktop Portal FileChooser backend
    fika-privileged-helper.rs    D-Bus helper for privileged operations
```

## Build And Run

```bash
cargo run --bin fika-wgpu -- --view compact /etc
cargo test --bin fika-wgpu
```

Because `default-run` is `fika-wgpu`, this also starts the current shell:

```bash
cargo run -- /etc
```

## Architecture Notes

- Pane state is routed by stable pane identity and stored through reusable pane
  containers, so split panes use the same state/projection/slot-pool path.
- Hot item views are retained and virtualized: visible-slot reuse, cached
  projection, cached text/icon atlas work, and explicit scroll metrics.
- Core behavior follows Dolphin as the first reference for file-manager
  semantics, while the shell owns rendering, hit testing, DPI, input routing,
  overlays, and telemetry.

## Active Docs

- [docs/TODO.md](docs/TODO.md) — current task board.
- [docs/WGPU_SHELL_ROADMAP.md](docs/WGPU_SHELL_ROADMAP.md) — UI runtime route
  and migration gates.
