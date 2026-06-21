# Fika winit/wgpu Shell Roadmap

Decision, 2026-06-21: Fika's UI mainline is official upstream `winit` `master`
plus official crates.io `wgpu`. `fika-wgpu` is the default run target and the
only in-tree file-manager UI runtime.

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

## Current Route

- `src/bin/fika-wgpu.rs` remains the shell entry point.
- `src/bin/fika_wgpu/` is the extraction target for shell modules.
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

- `cargo check --locked --bin fika-wgpu`
- `cargo test --locked --bin fika-wgpu`
- Runtime smoke for Icons/Compact/Details, split panes, hidden files, location
  editing, scroll/zoom, context menus, DnD, thumbnails, devices, and large
  directories.
- Telemetry must cover frame time, layout time, visible slots, cache
  hits/misses, atlas pressure, thumbnails, hit tests, and DnD state.
