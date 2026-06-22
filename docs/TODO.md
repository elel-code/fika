# Fika TODO: winit/wgpu Mainline

2026-06-21: Fika's only in-tree file-manager UI is now `fika`, built on
official upstream `winit` `master` and crates.io `wgpu`.

Status:

- `[x]` complete
- `[~]` in progress
- `[ ]` not started
- `[!]` blocked or needs a hard decision

## Hard Rules

- [x] `fika` is the default and active UI runtime.
- [x] Core remains UI-neutral: no windowing, renderer, Wayland object, raw
  window handle, or GPU resource dependencies in `src/core/`.
- [x] Every pane uses stable `ShellPaneId` routing and the same pane state,
  projection, scroll metrics, layout adapter, and visible-slot pool path.
- [x] Dolphin remains the first behavior reference for directory loading,
  refresh, selection, rename, undo, DnD, service menus, and view architecture.
- [x] Direct crates.io dependencies do not use wildcard versions.

## Current Source Boundaries

- `src/main.rs`: current shell entry point and remaining monolith.
- `src/shell/`: extracted shell modules.
- `src/core/`: UI-neutral file-manager/domain logic.
- `src/bin/fika-xdp-filechooser.rs`: portal backend.
- `src/bin/fika-privileged-helper.rs`: privileged operation helper.

## Active Work

- [~] Finish pane reuse:
  every pane must be addressed through pane containers, not dedicated
  first/second-pane fields.
- [~] Continue splitting `src/main.rs` into focused modules:
  app/window/event loop, renderer, scene, pane assembly, Places, context menu,
  dialogs, icons, thumbnails, text, DnD, and telemetry.
- [~] Keep item view hot paths Dolphin-aligned:
  visible-slot virtualization, reusable slot pools, retained geometry,
  filtered projection, visible-first icon/thumbnail/text work, and scroll
  metrics.
  2026-06-22 breakthrough: MIME/icon role reuse, queued read-ahead,
  alpha-mask text caching, persistent R8 text atlas reuse, subrect atlas
  uploads, and bounded icon-theme caching have moved the runtime closer to
  Dolphin's role updater + widget cache architecture; `/bin` compact
  full-scroll end-position `Private_Dirty` is now about 45.5 MB instead of
  about 97.7 MB.
  Follow-up: visible exact icon role lookup is nonblocking in ordinary UI
  frames, compact zoom/scroll avoids synchronous SVG raster, and the remaining
  work is visible-priority MIME role draining so small-directory tail ranges do
  not show generic -> exact icon replacement.
- [ ] Complete system integration in the winit/wgpu shell:
  Open With, service-menu icons/submenus, clipboard, create/rename/file
  transfer/trash/properties, thumbnail worker, devices, and dynamic Places.
- [ ] Make DnD basically usable:
  internal pane/place targets, external file import, Copy/Move/Link drop menu,
  drag preview, hover feedback, and safe failure paths. `text/uri-list` export
  remains pending.
- [ ] Mainline verification:
  `cargo check`, `cargo test`, runtime smoke for `/etc`, repo root, large
  directories, Icons/Compact/Details, split pane, scrollbars, rubber-band,
  context menus, location editing, hidden files, thumbnails, and devices.

## Pending Areas

- [ ] KDE service-menu advanced conditions:
  `X-KDE-Require=`, `X-KDE-ShowIfRunning=`, and related predicates.
- [ ] Trash multi-storage aggregation:
  `trash:/`, removable storage `.Trash-$uid`, and cross-volume restore details.
- [ ] Accessibility boundary for the custom rendered shell.
