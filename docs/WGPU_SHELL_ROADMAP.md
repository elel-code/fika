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
- It projects entries through the existing `IconsLayout` retained geometry and
  a shell-owned Compact projection that derives each column width from the
  longest visible name in that column.
- It renders a pane-local path bar, visible item backgrounds, real theme file/folder
  icons when the active XDG icon theme can resolve them, fallback file/folder
  icon shapes for misses, and real visible file names. Text uses
  `cosmic-text` for shaping/rasterization, then uploads a temporary per-frame
  RGBA atlas for a textured quad batch. The wgpu shell now applies the window
  scale factor to shell metrics before layout/rasterization, so the default
  Icons icon remains 48 logical px (for example 72 physical px at 1.5x scale)
  and the 14px/18px baseline text metric matches the current GPUI Fika scale
  more closely.
- It keeps a bounded persistent label raster cache for visible file/path text,
  keyed by text, size, and color. The per-frame atlas now packs cached label
  rasters instead of reshaping/rasterizing every visible label on every redraw.
- It resolves MIME/theme icons from XDG, GTK, and KDE theme settings; rasterizes
  PNG/WebP/JPEG/BMP/GIF/ICO through `image` and SVG through `usvg/resvg`;
  packs visible icons into a per-frame RGBA icon atlas; and keeps a bounded
  persistent icon raster cache keyed by theme icon file path and size.
- Mouse-wheel scrolling updates retained viewport state. File content now
  reserves and draws shell-owned item-view scrollbars: Icons/Details use a
  vertical right-side track and Compact uses a horizontal bottom track. The
  track and thumb are rounded, and thumb drag plus track click-to-drag update
  the same retained scroll offsets. The frame log includes
  `content_scrollbar=0|1`.
- The experimental binary accepts `--view icons|compact|details`. Icons remains
  the default baseline; Compact uses core `CompactLayout`; Details now has a
  shell-owned row projection with a fixed header and Name/Size/Modified columns.
  Icons and Compact now only paint item highlight/background for hover or
  selection, so plain unhovered items no longer look pre-highlighted. Compact
  labels are left-aligned, and each Compact item's highlight width follows that
  item's own text width rather than filling the whole column.
  The same modes can be switched at runtime with `1/2/3`,
  `Ctrl/Meta+1/2/3`, or fallback `F1/F2/F3` keys; the temporary top-bar mode
  buttons were removed to match the original app toolbar while real toolbar
  controls are still pending.
  `--auto-cycle-views` switches modes once per second for compositor/render
  debugging without any input. Switching clamps the active scroll axis, clears
  transient rubber-band state, refreshes hover from retained geometry, updates
  the window title, emits an immediate `[fika-wgpu] view-mode=...` log line,
  and keeps a short redraw burst active until the switched scene has been
  presented.
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
  or outside click, paints an opaque light menu surface instead of the earlier
  translucent dark overlay, dispatches Open for directory items, dispatches Open for file
  items through GIO default-application URI launch, opens a minimal
  shell-owned Open With chooser for file items using core `MimeApplicationCache`
  and systemd-user launch plans, dispatches item Copy Location to a shell-owned
  Wayland text clipboard provider, dispatches item
  Copy/Cut to the same provider using Fika's URI-list text encoding, dispatches
  blank-menu Paste by reading the Wayland text clipboard, decoding Fika/GNOME
  URI-list text or plain text, executing local core transfer/text-paste helpers,
  reloading the directory, and clearing successful Cut clipboards, plus blank-menu
  Refresh and Select All through existing shell navigation/reload/selection paths, logs
  remaining pending action hits, and emits context target/menu counters.
  Properties now opens a lightweight shell-owned metadata overlay for item and
  blank-directory targets. Blank-menu Create New now opens a shell-owned modal
  with folder/file selection, plain text name capture, validation, real
  `create_dir` / `create_new` filesystem actions, reload, and selection of the
  created entry. Directory item and blank-directory context menus can now Add to
  Places by writing Fika `places.xbel`, rebuilding the sidebar projection, and
  persisting the primary place order. Item Rename now opens a minimal
  shell-owned modal with plain text name capture, validation, real `rename`,
  reload, and selection of the renamed entry. Move to Trash now resolves the
  context target to either the clicked item or the active multi-selection,
  rejects remote paths explicitly, calls core XDG trash handling, reloads the
  pane, and clears stale context state. Trash view context menus now dispatch
  Restore From Trash, Delete Permanently, and Empty Trash through the core
  `TrashViewOperation` path, then reload the Trash view and clear stale context
  and selection state. Restore conflicts now open a shell-owned confirmation
  overlay; Replace reruns the restore through core `TrashViewOperation` with the
  replace policy, then reloads Trash. Cut and Paste reject remote paths
  explicitly. Open With default-application selection, multi-MIME
  `text/uri-list` clipboard export/import, richer multi-conflict handling,
  undo, richer properties, full inline rename, full Create New
  submenus/templates, and new-pane dispatch remain Phase 4 work.
- A first shell-owned Places sidebar is now drawn as a rounded light panel whose
  top edge aligns with the pane origin below the app-level toolbar, matching the
  right-side pane start rather than the pane body. It builds Home, existing XDG
  directories, Trash, Fika user places, primary
  `places-order.xml`, Network root, network bookmarks, and Root from public
  core APIs, keeps retained row geometry, uses longest-prefix active-place
  projection, updates sidebar hover independently from item hover, owns an
  independent sidebar scroll offset with clipped row rendering and a rounded
  narrow scrollbar track/thumb that supports thumb drag and track click-to-drag,
  paints rounded active/hovered row backgrounds, and dispatches
  left-click place navigation through the same `load_path`/history path as
  file-view navigation. Places
  right-click now creates a shell-owned place context target and a minimal
  context menu that dispatches Open, Copy Location, Properties, and Remove for
  editable user places. Remove writes Fika's `places.xbel`, prunes matching
  place-order entries, reloads the sidebar projection, and clears stale place
  context state. Dynamic devices, richer Places actions such as sidebar
  add/edit/hide and Trash actions, DnD/drop targets, and resizing remain Phase
  4 work.
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
  Alt+Left and Alt+Right map to the same history stack. `F5` and `Ctrl/Meta+R`
  refresh the current directory without writing history, preserving
  selection/focus by entry name when those entries still exist. App-level mouse
  controls for history/reload remain toolbar migration work.
  Loading a new path reuses
  `read_entries_sync`, records normal navigation in a bounded back stack,
  clears forward history only after successful new navigation, resets
  scroll/selection/rubber-band transient state, refreshes hover from retained
  geometry, updates the title, and presents the new scene through the same
  redraw burst path used by view switching.
- Initial view zoom is shell-owned and retained-geometry driven. `Ctrl/Meta + +`,
  `Ctrl/Meta + -`, `Ctrl/Meta + 0`, and `Ctrl/Meta + wheel` adjust or reset a bounded zoom step.
  Icons and Compact update item/icon/text slot metrics, Details updates row and
  icon metrics, scroll is clamped, the focused item is kept visible, and the icon
  resolver now requests rasters at the zoomed slot size. Glyph-level text sizing
  and long-lived glyph atlas policy remain Phase 2 work.
- The shell now prefers a non-sRGB surface format when the compositor offers one,
  because its UI colors and icon/text atlases are already authored in display
  byte space; this avoids the earlier washed-out sRGB double-conversion look.
- A minimal shell-owned status bar is now drawn at the bottom of the content
  pane, not across the Places sidebar. It
  summarizes entry, directory, file, selection, visible-item, view-mode, and zoom
  state, reserves content viewport height, and is excluded from item hit testing.
- A minimal shell-owned filter bar is now available with `Ctrl/Meta+F`.
  Character input updates a retained plain-text name filter, Backspace edits the
  pattern, Enter keeps the pattern applied while leaving text-capture mode, and
  Esc clears/deactivates it. Layout, hit testing, hover, selection, select-all, and
  keyboard navigation all route through the filtered model-index projection.
  Full IME/caret/selection editing remains Phase 4 text-boundary work.
- A minimal shell-owned pane-local location edit mode is now available from `Ctrl/Meta+L`,
  `Ctrl/Meta+D`, `F6`, or clicking the top path bar. It reuses core
  `resolve_location_input` and `complete_location_input`: first typed input
  replaces the current path draft, Backspace edits the draft, Tab completes
  filesystem paths, Enter commits through the retained navigation/history path,
  and Esc cancels. Caret movement, selection editing, and IME remain Phase 4
  text-boundary work.
- Dotfile visibility is now shell-owned. Hidden entries are excluded from the
  retained projection by default; `Ctrl/Meta+H` shows them. Selection is
  retained or pruned through the same projection when the visibility mode
  changes. The app-level Hidden toggle remains toolbar migration work.
- `[fika-wgpu]` logs include view mode, window/UI scale, path, entry count,
  visible item count,
  Places count/hover/change/scroll counters, quad count, selected count, hovered item index, active rubber-band state,
  context target kind, context menu state, properties overlay state, hit-test/selection/keyboard navigation/rubber-band/view-switch/path-change/open/copy-location/file-clipboard/paste
  counters, reload/location/filter/hidden counters, zoom percent and zoom-change counters, icon count, icon cache
  hit/miss count, icon cache bytes, icon atlas bytes, icon resolve/raster time,
  text label count, text cache hit/miss count, text cache bytes, text atlas
  bytes, draw batch count, render reason, layout time, text raster time, render
  time, and `scroll_x` / `scroll_y` offsets.
- Local target-session smokes with `timeout 4s target/debug/fika-wgpu --view
  icons|compact|details /etc` reached `shell-ready` and emitted `frame=1` on
  Vulkan with `surface-format=Rgba8Unorm srgb=0` and real icon/text atlas
  counters. The timeout exits are expected for the automated smokes.

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
  shell-owned context menu overlay are in the file view. Places row hover,
  left-click navigation, right-click context targets, and the minimal
  Open/Copy Location/Properties/Remove place menu are shell-owned. Device/place
  edit/hide/add action dispatch and DnD target lookup remain pending.
- Pane item to pane directory, pane item to Places, Places to pane, external
  path drop, and URI-list clipboard paths are covered by automated or isolated
  smoke runs.
- DnD hover does not depend on per-row or per-item widget callbacks.
- Drag cursor/action state follows Copy/Move/Link semantics.

### Phase 4: Chrome, Overlays, and Chooser

Implement the surrounding UI needed to make the shell usable: Places, toolbar,
location bar, filter bar, status bar, context menus, dialogs, and chooser mode.

Current checkpoint: the first chrome slices split the app-level toolbar from
pane-local chrome and remove the temporary Back/Forward/Reload/Hidden/view-mode
mouse buttons from the toolbar. The toolbar now only carries the original-style
Places toggle affordance until the real app-level controls are migrated; keyboard
reload, hidden-file, history, and view-mode commands remain available. The pane
starts below the toolbar with margin, while the rounded Places panel aligns to
the pane origin, includes the original-style title/row/icon metrics, and keeps
left-click navigation plus a minimal Open/Copy Location/Properties/Remove row
context menu. Content and Places scrollbars now use rounded tracks/thumbs and
support thumb drag plus track click-to-drag without leaving retained geometry.
The pane still owns its bottom status bar with
directory/selection/view/zoom summary, a minimal `Ctrl/Meta+F` filter bar, a
pane-local 28px `Ctrl/Meta+L`/`Ctrl/Meta+D`/`F6` location edit mode matching the
original header scale, and a lightweight opaque light file-view context menu
overlay for item/blank right-clicks. The context menu now follows the original
196px width, 28px rows, 4px vertical padding, 8px viewport margin, edge
flip/clamp positioning, 18px icon slot, `shadow_md`-style depth, grouped row
separators, padding-aware hover/hit-test rows, geometric fallback icons instead
of letter markers, and text scale independent from file view zoom. Overlay
quads/text now render in a separate top pass so menu/dialog surfaces cover
underlying item text. Properties opens a
minimal metadata overlay for item and blank-directory targets. Create New opens
a minimal shell-owned modal for blank-directory targets and performs real
folder/file creation followed by reload and selection. Rename opens a minimal
shell-owned modal for item targets and performs real filesystem rename followed
by reload and selection. Move to Trash handles item or selected item targets
through core trash operations, with remote paths rejected before filesystem
mutation. Filter, location, create-name, and rename-name text editing remain
intentionally narrow until the full IME/caret/selection text boundary is
migrated; context menu dispatch currently covers Open directory, Refresh,
Select All, Properties, minimal Create New, minimal Rename, minimal Move to
Trash, Trash view Restore/Delete Permanently/Empty Trash, Copy/Cut/Copy
Location, Paste, and the minimal Places row Open/Copy Location/Properties/Remove
menu, while richer Places actions/devices/DnD, richer Trash conflict handling,
undo, richer properties, full inline rename, full Create New submenus/templates,
Open With default-app selection, and new-pane actions remain pending.

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
