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
- Text cache now stores reusable alpha masks, with color carried by text
  vertices and the shader; the text atlas is a persistent R8 atlas. The same
  label in different colors therefore shares one mask, and after scrolling
  `/bin` compact to the end the 3096 label cache entries occupy about 9.1 MB
  with stable `text_atlas_reused` hits. This is closer to Dolphin's
  `QStaticText::AggressiveCaching` boundary: cache text shape/raster resources,
  then reuse them while painting.
- Text/icon atlases upload dirty subrectangles, and the overlay text renderer is
  not created on ordinary frames without overlays. Compact scrolling therefore
  pays for visible item work, not full-atlas or unused overlay work.
- The icon theme cache keeps renderable hits but no longer retains large
  negative full-path probe sets. In `/bin` compact full-scroll testing,
  `Private_Dirty` dropped from about 97.7 MB to about 43.7-45.9 MB, with
  `[anon]` dropping from about 54.9 MB to about 2.9 MB.
- Follow-up work moved visible exact icon role lookup out of all UI
  prewarm/draw frames. Normal frames now read the exact cache or show a role
  fallback while the pending resolver owns theme lookup.
- Zoom/scroll SVG icon raster misses now go to a background worker. UI frames
  prefer the exact cache, nearby-size cache, role-raster cache, or generic role
  fallback instead of synchronously rasterizing SVGs during ordinary redraws.
  This matches Dolphin's role/pixmap reuse direction and avoids transient
  missing icons during compact zoom.
- Icon resolver pending requests now carry visible/deferred priority, and the
  worker promotes visible role requests over deferred read-ahead. This keeps
  current viewport role work ahead of background warmup, closer to Dolphin's
  event-loop-drained role updater.
- Core MIME metadata role scheduling has the same visible/deferred boundary:
  visible metadata work is batched before deferred background work, same-key
  deferred requests can be promoted, and visible snapshots no longer discard
  deferred background requests.
- The winit/wgpu shell now uses that metadata boundary during prewarm/render:
  visible MIME metadata candidates are drained before deferred read-ahead and
  stale results are guarded by pane, path, entry index, size, and modified time.
- Metadata deferred read-ahead is now budgeted per frame, and thumbnail plus
  folder-preview background workers share one visible/deferred priority queue
  helper instead of carrying duplicated queue logic in `src/main.rs`.

The architecture is therefore materially closer to Dolphin: the reuse unit is a
file-manager role and a view resource, while expensive work is bounded by queues
and cache ownership instead of being constructed per path in the draw path. In
current debug measurements, `/bin` compact full-scroll and end-position dwell
has `Private_Dirty` at 45.5 MB, `autosmoke-scroll render_us_p50/p95/max` around
2.17/3.78/5.94 ms, and `icon_raster_us_max=0`; `/etc` compact rapid scroll has
`render_us_p95` around 3.9 ms; compact rapid zoom has `render_us_p95` around
4.5 ms with `icon_raster_us_max=0`. Quick small-directory tail scrolling now
has a desktop-session gate path through
`scripts/run-retained-renderer-evidence.sh --metadata-tail-scroll`: the current
Icons fixture evidence shows startup metadata visible/deferred queueing
(`visible_total=44`, `deferred_total=128`) and autosmoke-scroll metadata drain
(`results_total=32`, `applied_total=32`) with `icon_raster_us_max=0` and
`max_new_scroll_y=1693.0`.

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
- External file DnD import is wired through winit file drag events; `text/uri-list`
  export and any missing Wayland-specific support remain follow-up work.

### Phase 5: Verification

- `cargo check --locked --bin fika`
- `cargo test --locked --bin fika`
- Runtime smoke for Icons/Compact/Details, split panes, hidden files, location
  editing, scroll/zoom, context menus, DnD, thumbnails, devices, and large
  directories. Small-directory MIME role tail scrolling has a dedicated
  `scripts/run-retained-renderer-evidence.sh --metadata-tail-scroll` gate and
  should be included in the broader matrix.
- Telemetry must cover frame time, layout time, visible slots, cache
  hits/misses, atlas pressure, thumbnails, metadata role prewarm/drain, hit
  tests, and DnD state.
