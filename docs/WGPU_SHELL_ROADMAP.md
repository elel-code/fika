# Fika winit/wgpu Shell Roadmap

This document is the active UI direction for Fika.

Decision, 2026-06-21: the new shell mainline is **official upstream
`winit` master + official crates.io `wgpu`**. The GPUI application remains the
compatibility and behavior baseline while the winit shell is made complete.
The SCTK backend is no longer the main route; keep it buildable as an
experiment/reference backend, but do not add new shell behavior there unless a
later decision explicitly reopens it.

## Decision

Fika is Linux-only, but that does not require owning every Wayland protocol
edge directly. The recent SCTK spike exposed shell-level cost in startup
presentation, input coordinates, scale handling, popup behavior, and DnD
plumbing. Those problems sit below the file-manager scene and would continue to
slow migration.

The mainline should use:

- `winit` from `https://github.com/rust-windowing/winit.git`, branch `master`.
  This is the official upstream branch. Do not use the Pop!_OS/COSMIC or
  iced-maintained fork for the mainline.
- Official crates.io `wgpu` for rendering.
- Existing Fika core modules for directory listing, operations, thumbnails,
  MIME/application launching, Places, devices, trash, portal, and privileged
  helper behavior.
- Existing GPUI retained-model work and the `fika-wgpu` spike as migration
  input, not as a reason to keep a monolithic renderer file.

`winit` is a shell shim here, not a portability goal. Fika can remain
Linux-focused while letting upstream winit own the difficult window/event/scale
surface boundary.

## Architecture Target

```text
fika-core
  -> retained file-manager model
  -> winit shell state
  -> wgpu scene projection and GPU batches
  -> input/hit-test routing back to file-manager actions
```

Core remains UI-neutral. It must not depend on GPUI, winit, SCTK, Wayland
protocol objects, `wgpu`, raw window handles, or renderer resources.

The winit/wgpu shell owns:

- Window lifecycle and event loop integration.
- Surface resize, scale-factor changes, redraw scheduling, and frame pacing.
- Pane, Places, overlay, context menu, dialog, and chooser scene state.
- Retained geometry for file slots, Details rows, Places rows, splitters,
  scrollbars, rubber-band selection, drag targets, and context targets.
- Hit testing and pointer/keyboard routing.
- Draw command generation, batching, clipping, transforms, and invalidation.
- Texture atlases/caches for MIME/theme icons, thumbnails, text, and UI assets.
- Performance telemetry for frame time, layout, hit tests, visible slots,
  cache hits/misses, atlas pressure, thumbnails, and DnD state.

## Source Direction

Current state:

- `src/bin/fika-wgpu.rs` is the winit/wgpu prototype and becomes the new shell
  migration source.
- `src/bin/fika_wgpu/` already contains a small start of module extraction.
- `src/bin/fika-sctk.rs` and `src/bin/fika_sctk/` are experimental/reference
  code only. Keep them compiling when practical, but stop moving new behavior
  into SCTK.
- `src/main.rs` remains the GPUI baseline until the winit shell passes the
  promotion gates.

Expected cleanup:

- Split `src/bin/fika-wgpu.rs` into focused modules under `src/bin/fika_wgpu/`.
- Move reusable pane, Places, dialog, icon, thumbnail, text, DnD, and telemetry
  pieces out of the monolith before adding major new behavior.
- Keep shell-only state in the winit shell; move reusable file-manager behavior
  into `fika-core` or shared UI-neutral modules.

## Migration Phases

### Phase 0: Route Switch

- Use upstream `winit` master in `Cargo.toml`.
- Remove old documentation that names SCTK as the sole shell target.
- Preserve SCTK as an experiment/reference backend.
- Verify the existing winit shell still builds with the upstream branch.

### Phase 1: Stabilize the winit Shell

- Treat `fika-wgpu` as the active new-shell binary.
- Reconfirm startup presentation, DPI, pointer coordinates, keyboard routing,
  scrollbars, rubber-band selection, context menus, and location caret behavior.
- Keep `/etc`, the repo root, a large local directory, and split-pane runs as
  smoke targets.
- Track regressions with shell-native telemetry rather than GPUI renderer
  counters.

### Phase 2: Break Up the Monolith

- Current priority: pane reuse. Every pane must share `ShellPaneState`, pane
  view/projection, scroll metrics, slot pool, layout adapters, and later
  input/action routing boundaries.
- First extraction is in place: `src/bin/fika_wgpu/clipboard.rs` owns the
  shell clipboard wrapper, and `src/bin/fika_wgpu/location.rs` owns
  `PathHistory`, `LocationDraft`, and UTF-8 cursor normalization for location
  editing. `src/bin/fika_wgpu/selection.rs` now owns selection state,
  keyboard navigation actions, click context, and rubber-band state.
  `src/bin/fika_wgpu/pane.rs` owns pane kind/state/view/projection data,
  scroll metrics, split metrics, and the visible-slot pool.
  `src/bin/fika_wgpu/pane_layout.rs` owns the shell layout enum, Compact and
  Details layout adapters, and keyboard navigation target calculation.
- `ShellScene` no longer keeps global selection. Pane state owns selection, and
  hover, context targets, double-click open, and scrollbar drag paths carry pane
  identity so no pane remains an implicit main route.
- Extract app/window/event loop, renderer, scene, pane, Places, context menu,
  dialogs, icons, thumbnails, text, DnD, and telemetry modules.
- Keep behavior changes small while moving code, so regressions remain easy to
  identify.
- Start removing duplicated code between the winit prototype and SCTK only when
  the shared code is shell-neutral.

### Phase 3: Dolphin-style Pane Architecture

- Align pane projection with Dolphin's model/controller/view split.
- Keep visible-slot virtualization, reusable slot pools, and retained geometry
  as the hot path.
- Make Compact/Icons/Details share selection, hit testing, scroll, zoom,
  rename, filter, and DnD boundaries.
- Ensure icon/thumbnail/text work is visible-first and never blocks input.

### Phase 4: System Integration

- Wire Open With, service menu submenus, clipboard, file transfer, create,
  rename, trash, properties, thumbnails, devices, and Places dynamic data into
  the winit shell.
- Implement external DnD using winit's platform surface where possible and
  narrow Linux-specific support where needed.
- Keep remote/GVfs behavior explicit: unsupported local file operations must
  fail safely.

### Phase 5: Mainline Promotion

Promote the winit shell only after evidence proves it is a better default than
the GPUI baseline:

- `cargo check --locked --bin fika-wgpu`
- `cargo test --locked --bin fika-wgpu`
- representative runtime smokes for Icons/Compact/Details, split panes,
  hidden files, location editing, scroll/zoom, context menus, DnD, thumbnails,
  devices, and large directories
- telemetry showing frame/layout/cache behavior is not worse than the current
  baseline
- docs updated so `fika-wgpu` is no longer described as an experiment

## Dependency Policy

- `winit` mainline dependency:

```toml
winit = { git = "https://github.com/rust-windowing/winit.git", branch = "master" }
```

- `wgpu` remains the official crates.io dependency.
- Do not pin to the COSMIC/iced fork unless a future route decision explicitly
  changes this document.
- Do not add a second window/event backend abstraction before the winit shell
  has been made complete enough to evaluate.
