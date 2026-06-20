# Fika winit/wgpu Shell Roadmap

This document is the active UI direction for Fika. The GPUI application remains
the compatibility and behavior baseline while the new Linux-only shell is
proved out. New UI architecture work should target a Fika-specific
`winit + wgpu` runtime instead of extending the GPUI element-tree migration.

The goal is not to adopt another general-purpose widget toolkit. Fika should
borrow the windowing stack that the iced/COSMIC ecosystem is actively validating
on Linux, then build a narrow file-manager renderer, scene model, input router,
and cache policy around Fika's own retained data.

## Decision

Fika is Linux-only. That removes the main reason to keep a cross-platform UI
framework in the hot path. The file view, Places sidebar, selection model,
hover state, drag/drop routing, zoom, thumbnails, and icon/text caches are
specialized enough that emulating Dolphin through GPUI has become more costly
than owning a purpose-built runtime.

The new shell should use:

- `winit` from the iced/COSMIC stack, not an arbitrary upstream windowing
  dependency. The local COSMIC reference resolves `winit` through
  `pop-os/winit` tag `cosmic-0.14`.
- Official crates.io `wgpu` for the render backend. COSMIC's resolved `wgpu`
  version is useful compatibility context, but Fika should depend on upstream
  `wgpu` directly instead of inheriting a framework or editor fork.
- Existing Fika core modules for listing, operations, thumbnails, MIME,
  Places, devices, trash, portal, and privileged-helper behavior.
- Existing retained file-grid and Places models as migration input, not as
  GPUI-specific design constraints.

Do not build the main shell as a libcosmic/iced widget tree. Those projects are
valuable references for Linux windowing, Wayland, DnD, clipboard, text, and
theme integration, but Fika's primary UI should be a dedicated file-manager
surface.

Choosing the iced/COSMIC `winit` path is intentional. For Fika's target
environment, it is more useful than following upstream `winit` in isolation
because it is exercised by real Linux desktop applications and carries the
integration assumptions needed by the iced/libcosmic runtime: Wayland window
and popup behavior, clipboard and drag/drop plumbing, raw-window-handle/wgpu
surface integration, and desktop-session edge cases. Fika should reuse that
tested windowing layer while avoiding the generic widget tree above it.

## Why This Can Outperform GPUI and cosmic-files

Fika has a narrower problem than a general desktop UI framework:

- The file grid can be rendered as a few batched GPU passes over visible slots
  instead of thousands of independent row/item widgets.
- Layout, hit testing, paint command generation, and input routing can share
  one retained geometry projection.
- Scroll and zoom can update viewport state first, then budget expensive
  thumbnail, icon, text-shape, and glyph work behind the visible layer.
- MIME/theme icons, thumbnails, and glyph atlases can be keyed by file-manager
  semantics instead of widget/image handle lifetime.
- Places, Compact, Icons, and Details can share slot, dirty-state, cache, and
  hit-test primitives.
- Linux-only clipboard, URI-list, Wayland DnD, portal, GIO/GVfs, and XDG
  behavior can stay narrow and directly testable.

The tradeoff is explicit ownership. Fika must own frame scheduling, GPU
resources, text cache policy, focus, IME boundaries, popups, clipboard, DnD,
and accessibility planning. This is acceptable only because those pieces can be
implemented for Fika's file-manager workflows instead of for a generic toolkit.

## Architecture Target

```text
core model -> retained UI model -> scene projection -> GPU command batches
          \-> input/hit-test routing -> file-manager actions
```

Core remains UI-neutral. It must not depend on `winit`, `wgpu`, window handles,
or renderer resources.

The shell owns:

- Window lifecycle and event-loop integration.
- Pane, Places, overlay, popup, and chooser scene state.
- Retained geometry for file slots, Details rows, Places rows, scrollbars,
  rubber-band selection, splitters, and context targets.
- Hit testing and pointer/keyboard routing.
- Draw command generation, batching, clipping, transforms, and invalidation.
- Texture atlases for icons, thumbnails, masks, and UI assets.
- Text shaping/raster cache integration through proven text crates. Do not
  hand-roll Unicode shaping, bidi, fallback, or IME text editing.
- Performance logs that replace GPUI renderer-policy logs with shell-native
  frame, cache, atlas, batch, and hit-test counters.

## Migration Phases

### Phase 0: Shell Spike

Add a separate experimental binary, tentatively `fika-wgpu`, without deleting
the GPUI binary. It should open a window, initialize `wgpu`, drive the existing
directory listing model, and render `/etc` with a minimal Compact view.

Current checkpoint:

- `src/bin/fika-wgpu.rs` exists as an independent binary.
- It accepts an optional path argument and defaults to the current directory.
- It reads directory entries through `fika_core::read_entries_sync`.
- It projects entries through the existing `IconsLayout` retained geometry.
- It renders a top path bar, visible item backgrounds, real theme file/folder
  icons when the active XDG icon theme can resolve them, fallback file/folder
  icon shapes for misses, and real visible file names. Text uses
  `cosmic-text` for shaping/rasterization, then uploads a temporary per-frame
  RGBA atlas for a textured quad batch.
- It keeps a bounded persistent label raster cache for visible file/path text,
  keyed by text, size, and color. The per-frame atlas now packs cached label
  rasters instead of reshaping/rasterizing every visible label on every redraw.
- It resolves MIME/theme icons from XDG, GTK, and KDE theme settings; rasterizes
  PNG/WebP/JPEG/BMP/GIF/ICO through `image` and SVG through `usvg/resvg`;
  packs visible icons into a per-frame RGBA icon atlas; and keeps a bounded
  persistent icon raster cache keyed by theme icon file path and size.
- Mouse-wheel scrolling updates retained viewport state.
- The experimental binary accepts `--view icons|compact|details`. Icons remains
  the default baseline; Compact uses core `CompactLayout`; Details now has a
  shell-owned row projection with a fixed header and Name/Size/Modified columns.
  The same modes can be switched at runtime with top-bar `Icons / Compact /
  Details` buttons, `1/2/3`, `Ctrl/Meta+1/2/3`, or fallback `F1/F2/F3` keys.
  `--auto-cycle-views` switches modes once per second for compositor/render
  debugging without any input. Switching clamps the active scroll axis, clears
  transient rubber-band state, refreshes hover from retained geometry, updates
  the window title, emits an immediate `[fika-wgpu] view-mode=...` log line,
  and keeps a short redraw burst active until the switched scene has been
  presented. The active top-bar segment and a full-width mode color stripe make
  the current projection visible even when a directory's file content looks
  similar across modes.
- Pointer move/leave and left-click events now route through shell-owned
  retained hit testing. The spike tracks hovered item, single selection,
  Ctrl/Meta toggle selection, and Shift range selection by model index, then
  paints hover/selection state from the same slot projection.
- Right-click context targeting now also routes through shell-owned retained
  hit testing. Right-clicking an unselected item syncs selection to that item,
  right-clicking an already-selected item preserves the multi-selection while
  focusing the clicked model index, and right-clicking blank content records a
  blank directory target without starting rubber-band selection. The shell now
  stores a lightweight context target snapshot, opens a clamped shell-owned
  context menu overlay for item/blank targets, updates row hover, closes on Esc
  or outside click, dispatches Open for directory items plus blank-menu Refresh
  and Select All through existing shell navigation/reload/selection paths, logs
  remaining pending action hits, and emits context target/menu counters.
  Clipboard, rename, trash, create-new, properties, and new-pane dispatch remain
  Phase 4 work.
- Blank-space left-drag now runs rubber-band selection through the same
  retained Icons geometry. Plain drag replaces the selection, Shift extends it,
  Ctrl/Meta toggles it against the press-time base selection, and the band is
  painted as a clipped GPU overlay.
- Keyboard navigation now handles Arrow, Home/End, and Page Up/Down keys
  through the same retained selection state. Shift extends the current range,
  and the focused item is scrolled into view. `Ctrl/Meta+A` selects all current
  directory entries, and `Esc` clears selection while canceling any transient
  rubber-band operation.
- Directory activation now stays inside the shell-owned input path: Enter opens
  the focused selected directory, double-click opens a directory resolved from
  retained hit testing, and Backspace or Alt+Up loads the parent directory. The
  top bar also has shell-owned Back/Forward/Up controls, with Alt+Left and
  Alt+Right mapped to the same history stack. A top-bar Reload control plus
  `F5` and `Ctrl/Meta+R` refresh the current directory without writing history,
  preserving selection/focus by entry name when those entries still exist.
  Loading a new path reuses
  `read_entries_sync`, records normal navigation in a bounded back stack,
  clears forward history only after successful new navigation, resets
  scroll/selection/rubber-band transient state, refreshes hover from retained
  geometry, updates the title, and presents the new scene through the same
  redraw burst path used by view switching.
- Initial view zoom is shell-owned and retained-geometry driven. `Ctrl/Meta + +`,
  `Ctrl/Meta + -`, and `Ctrl/Meta + 0` adjust or reset a bounded zoom step.
  Icons and Compact update item/icon/text slot metrics, Details updates row and
  icon metrics, scroll is clamped, the focused item is kept visible, and the icon
  resolver now requests rasters at the zoomed slot size. Glyph-level text sizing
  and long-lived glyph atlas policy remain Phase 2 work.
- A minimal shell-owned status bar is now drawn at the bottom of the window. It
  summarizes entry, directory, file, selection, visible-item, view-mode, and zoom
  state, reserves content viewport height, and is excluded from item hit testing.
- A minimal shell-owned filter bar is now available with `Ctrl/Meta+F`.
  Character input updates a retained plain-text name filter, Backspace edits the
  pattern, Enter keeps the pattern applied while leaving text-capture mode, and
  Esc clears/deactivates it. Layout, hit testing, hover, selection, select-all, and
  keyboard navigation all route through the filtered model-index projection.
  Full IME/caret/selection editing remains Phase 4 text-boundary work.
- A minimal shell-owned location edit mode is now available from `Ctrl/Meta+L`,
  `Ctrl/Meta+D`, `F6`, or clicking the top path bar. It reuses core
  `resolve_location_input` and `complete_location_input`: first typed input
  replaces the current path draft, Backspace edits the draft, Tab completes
  filesystem paths, Enter commits through the retained navigation/history path,
  and Esc cancels. Caret movement, selection editing, and IME remain Phase 4
  text-boundary work.
- Dotfile visibility is now shell-owned. Hidden entries are excluded from the
  retained projection by default; `Ctrl/Meta+H` or the top-bar `Hidden` toggle
  shows them. Selection is retained or pruned through the same projection when
  the visibility mode changes.
- `[fika-wgpu]` logs include view mode, path, entry count, visible item count,
  quad count, selected count, hovered item index, active rubber-band state,
  context target kind, context menu state, hit-test/selection/keyboard navigation/rubber-band/view-switch/path-change
  counters, reload/location/filter/hidden counters, zoom percent and zoom-change counters, icon count, icon cache
  hit/miss count, icon cache bytes, icon atlas bytes, icon resolve/raster time,
  text label count, text cache hit/miss count, text cache bytes, text atlas
  bytes, draw batch count, render reason, layout time, text raster time, render
  time, and `scroll_x` / `scroll_y` offsets.
- Local target-session smokes with `timeout 4s target/debug/fika-wgpu --view
  icons|compact|details /etc` reached `shell-ready` and emitted `frame=1` on
  Vulkan with real icon/text atlas counters. The timeout exits are expected for
  the automated smokes.

Still pending in Phase 0: glyph-level cache/atlas retention, manual
open/close/interaction smoke evidence, DnD targeting, and the final choice of
initial Compact vs Icons default.

Acceptance:

- [x] Builds without changing the existing GPUI app.
- [~] Opens on the target Linux desktop session and reaches the first rendered
  frame in an automated timeout smoke. Manual close and interaction smoke remain
  pending.
- [~] Renders visible directory slots with real theme icons when available,
  fallback icons for misses, and real file-name text via texture atlases.
- [~] Routes basic pointer hover, mouse selection, keyboard navigation,
  select-all/clear shortcuts, right-click context target selection, and
  rubber-band selection through retained geometry. DnD targeting remains
  pending.
- [~] Emits frame timing, visible range, draw-command counters, temporary
  icon/text atlas counters, retained hit-test counters, and bounded
  icon/label-cache counters. Glyph-level and thumbnail atlas counters will
  start once those resource retention layers exist.

### Phase 1: File View Parity Core

Implement Compact, Icons, and Details scene projection from existing Fika
models.

Acceptance:

- [~] `/etc` renders in Compact, Icons, and Details via `--view`; `~/Downloads`
  and manual interaction smokes remain pending.
- [~] Scroll, hover, keyboard navigation, runtime mode switching, projection
  zoom, reload, location editing, filtering, hidden-file visibility, selection, and
  select-all/clear shortcuts work from retained geometry for the initial
  projections. Glyph-level text zoom policy remains pending.
- [~] Layout/hit-test/paint share the same shell layout abstraction for Icons,
  Compact, and Details.
- No synchronous theme scan, MIME magic read, thumbnail decode, or text shaping
  occurs in the steady render pass.

### Phase 2: Cache and Text Pipeline

Promote the initial Phase 0 icon atlas into budgeted semantic icon work, then
add thumbnail texture retention, text shaping cache, glyph atlas policy, and
eviction telemetry.

Acceptance:

- Zoom does not invalidate loaded same-semantic icons except when size/DPI
  requires a new raster.
- Cold glyph/icon work is budgeted and visible-first.
- Cached thumbnails appear on the first eligible frame.
- Cache logs show hit/miss/evict/bytes and per-frame compute time.

### Phase 3: Interaction and DnD

Move remaining pointer routing, context target selection, directory hover,
Places hover, and drag/drop target lookup into shell-owned hit testing.

Acceptance:

- [~] Pane item/blank right-click context target selection and the first
  shell-owned context menu overlay are in the file view. Places context targets
  and broader file-operation action dispatch remain pending.
- Pane item to pane directory, pane item to Places, Places to pane, external
  path drop, and URI-list clipboard paths are covered by automated or isolated
  smoke runs.
- DnD hover does not depend on per-row or per-item widget callbacks.
- Drag cursor/action state follows Copy/Move/Link semantics.

### Phase 4: Chrome, Overlays, and Chooser

Implement the surrounding UI needed to make the shell usable: Places, toolbar,
location bar, filter bar, status bar, context menus, dialogs, and chooser mode.

Current checkpoint: the first chrome slices are a minimal bottom status bar with
directory/selection/view/zoom summary, a minimal `Ctrl/Meta+F` filter bar, a
minimal `Ctrl/Meta+L`/`Ctrl/Meta+D`/`F6` location edit mode, and a lightweight
file-view context menu overlay for item/blank right-clicks. Filter and location
text editing remain intentionally narrow until the full IME/caret/selection
text boundary is migrated; context menu dispatch currently covers Open
directory, Refresh, and Select All, while clipboard, rename, trash, create-new,
properties, and new-pane actions remain pending.

Acceptance:

- Common file-manager workflows are possible without launching the GPUI shell.
- Text editing boundaries for rename, location, filter, and application search
  have explicit IME/caret/selection coverage.
- Portal file chooser output remains compatible with the existing backend.

### Phase 5: Default Promotion

Promote the new shell only after same-scenario evidence shows behavior parity
and better or more predictable frame costs than both GPUI Fika and the relevant
COSMIC Files baseline.

Acceptance:

- GPUI stays available as a fallback during the promotion window.
- `/etc`, `~/Downloads`, large local directories, mixed thumbnail directories,
  removable devices, trash, and network roots have smoke coverage.
- Performance gates cover frame build time, GPU submission count, draw batches,
  texture bytes, glyph/icon/thumbnail cache behavior, and input latency.

## Documentation Policy

The GPUI retained-renderer documents are historical evidence and migration
input. They should no longer be treated as the active architecture target.

Keep:

- Dolphin behavior references.
- Core/system integration references.
- GPUI performance evidence that gives baseline numbers or behavior coverage.

Delete or rewrite:

- Completed plans whose only purpose was moving from the old UI to GPUI.
- Documents that describe "continue GPUI retained migration" as the active
  future direction.
- Duplicated TODO slices once their evidence has been summarized in this
  roadmap or in a shell-specific implementation note.
