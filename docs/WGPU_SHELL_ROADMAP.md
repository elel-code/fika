# Fika winit/wgpu Shell Roadmap

Decision, 2026-06-21: Fika's UI mainline is official upstream `winit` `master`
plus official crates.io `wgpu`. The default `fika` binary is the only in-tree
file-manager UI runtime.

## Architecture Target

```text
fika-core
  -> retained file-manager model
  -> reusable pane shell state
  -> wgpu scene projection and batches
  -> input/hit-test routing back to file-manager actions
```

Core must remain UI-neutral. The shell owns window lifecycle, scale handling,
redraw scheduling, retained geometry, hit testing, overlay/menu/dialog state,
texture atlases, thumbnails/text/icon scheduling, and telemetry.

## Dolphin Alignment Breakthrough

2026-06-22: the current shell architecture has moved from per-entry immediate
resolution toward a Dolphin-style item-view hot path. This is the main
breakthrough from the current performance pass.

- MIME/icon roles are reused by role + size instead of paying full theme lookup
  per file path. This matches Dolphin's split between `KFileItemModelRolesUpdater`
  role resolution and view/widget pixmap/text reuse.
- Icon read-ahead now uses a persistent queue with a small per-frame budget,
  matching Dolphin's pattern of spreading pending role work through the event
  loop.
- Text/icon atlases upload dirty subrectangles, and the overlay text renderer is
  not created on ordinary frames without overlays. Compact scrolling therefore
  pays for visible item work, not full-atlas or unused overlay work.
- The icon theme cache keeps renderable hits but no longer retains large
  negative full-path probe sets. In `/bin` compact full-scroll testing,
  `Private_Dirty` dropped from about 97.7 MB to about 43.7-45.9 MB, with
  `[anon]` dropping from about 54.9 MB to about 2.9 MB.

The architecture is therefore materially closer to Dolphin: the reuse unit is a
file-manager role and a view resource, while expensive work is bounded by queues
and cache ownership instead of being constructed per path in the draw path. The
remaining focus is to move first-time visible exact icon role lookup out of
scroll/zoom frames, so compact mode does not spike when a new MIME batch enters
the viewport.

## Current Route

- `src/main.rs` remains the shell entry point.
- `src/shell/` is the extraction target for shell modules.
- `src/core/` owns reusable file-manager behavior.
- `src/bin/fika-xdp-filechooser.rs` and `src/bin/fika-privileged-helper.rs`
  remain integration binaries.

Dependency policy:

```toml
winit = { git = "https://github.com/rust-windowing/winit.git", branch = "master" }
wgpu = "29"
```

## Phases

### Phase 1: Pane Reuse

- Store pane state through reusable pane containers.
- Route selection, hover, context targets, scrollbars, location/filter state,
  keyboard navigation, rubber-band, and DnD by `ShellPaneId`.
- Keep split panes visually and behaviorally identical to the first pane.

### Phase 2: Split The Shell

- Extract app/window/event loop, renderer, scene assembly, pane rendering,
  Places, context menu, dialogs, icons, thumbnails, text, DnD, and telemetry.
- Keep behavior changes small while moving code, so regressions remain easy to
  isolate.

### Phase 3: Dolphin-style Hot Path

- Keep visible-slot virtualization, reusable slot pools, retained geometry, and
  cached projection on the hot path.
- Make Compact/Icons/Details share selection, hit testing, scroll, zoom,
  rename, filter, and DnD boundaries.
- Ensure icon, thumbnail, and text work is visible-first and does not block
  pointer input.

### Phase 4: System Integration

- Wire Open With, service-menu icons/submenus, clipboard, file transfer,
  create, rename, trash, properties, thumbnails, devices, Places dynamic data,
  and portal chooser behavior into the winit/wgpu shell.
- Implement external DnD with narrow Linux-specific support where required.

### Phase 5: Verification

- `cargo check --locked --bin fika`
- `cargo test --locked --bin fika`
- Runtime smoke for Icons/Compact/Details, split panes, hidden files, location
  editing, scroll/zoom, context menus, DnD, thumbnails, devices, and large
  directories.
- Telemetry must cover frame time, layout time, visible slots, cache
  hits/misses, atlas pressure, thumbnails, hit tests, and DnD state.
