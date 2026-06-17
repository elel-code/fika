# Item View Custom Paint Status

This is the current replacement map for the Dolphin-style item-view migration.
It is a status document, not a promise that every surface must become custom
painted. The architecture target is retained model/layout/controller/painter
state; each renderer still has to beat or match the GPUI baseline before it
becomes the default.

## Current Replacement Matrix

| Surface | Current state | Renderer | Remaining dependency |
| --- | --- | --- | --- |
| Compact/Icons item model and geometry | retained | `DirectoryModel`, visible snapshots, slot pools | none for current path |
| Compact/Icons base background, selection, hover, drop tint, labels | replaced | custom content-level painter | runtime perf and DnD smoke evidence must stay current |
| Compact/Icons thumbnail images | replaced | custom image painter using GPUI `RetainAllImageCache` plus retained same-thumbnail images | pending/failure still reuses retained images or paints thumbnail fallback |
| Compact/Icons MIME/theme-icon images | retained model, GPUI renderer | GPUI `img()` element over retained item shell | theme-icon paths resolve off the render path with the current layout icon size; custom theme-icon painting is only enabled by `FIKA_CUSTOM_THEME_ICONS=1` for A/B evidence |
| Compact/Icons click, menu, hover, cursor, and drop hit testing | replaced | retained viewport/custom hitboxes plus active item-drag window tracker | runtime DnD smoke still required after painter changes |
| Compact/Icons drag start | not replaced | GPUI `Div::on_drag` shell | public GPUI custom-element drag-start API or audited Fika GPUI patch |
| Compact/Icons rename editor | not replaced | GPUI editor overlay | only revisit after caret, selection, IME, and text input behavior are covered |
| Details row model and geometry | retained | Details paint snapshots and row layout projection | none for current path |
| Details row backgrounds, icons, text cells, Trash columns | replaced | custom content-level painter | Details icons use the same cached/preliminary icon policy; runtime Details perf and DnD smoke evidence must stay current |
| Details click, menu, navigation, hover, cursor, drop hit testing | replaced | retained row hit testing/controller state plus active item-drag window tracker | runtime DnD smoke still required after painter changes |
| Details drag start | not replaced | GPUI `Div::on_drag` row shell | same drag-start API or audited GPUI patch gate |
| Places rows and sidebar scrollbar | retained model/slot/target-decision state, renderer not replaced | GPUI elements over retained places projection, `PlacePaintSlotCache` stats, and `places/interaction.rs` target decisions | retained hitboxes and custom row painter still require Places-specific DnD/scroll evidence |

The practical state is: item-view static visuals and most app-side controller
paths have moved to retained/custom-painted architecture. Drag-start and rename
remain GPUI renderer/platform-contract boundaries. Places has retained model,
paint-slot stats, and DnD target-decision helpers, but its row renderer and
row-level event delivery are still GPUI.

## Evidence Anchors

- Renderer policy code: `src/ui/file_grid/renderer_policy.rs`
- Root file-grid render surface composition: `src/ui/file_grid/surface.rs`
- Compact/Icons layout options and Dolphin sizing constants:
  `src/ui/file_grid/layout.rs`
- Compact/Icons static visual painter: `src/ui/file_grid/static_visual.rs`
- Compact/Icons image paint layer: `src/ui/file_grid/image_layer.rs`
- File icon cache and background resolve policy: `src/ui/icons/cache.rs`,
  `src/ui/file_grid/icon_work.rs`, and
  `RawFileGridSnapshot::queue_file_icon_resolve_candidates`
- Compact/Icons transparent item shell boundary: `src/ui/file_grid/item_shell.rs`
- Details visual painter: `src/ui/file_grid/details_visual.rs`
- Details transparent row shell boundary: `src/ui/file_grid/details_shell.rs`
- GPUI rename overlay boundary: `src/ui/file_grid/rename_overlay.rs`
- Shared visual style and item identity helpers: `src/ui/file_grid/style.rs`
- File-grid root API snapshot/props/viewport types: `src/ui/file_grid/types.rs`
- Raw snapshot model/projection types: `src/ui/file_grid/snapshot/types.rs`
- Raw file-grid snapshot construction: `src/ui/file_grid/snapshot/builder.rs`
- Raw-to-render snapshot conversion: `src/ui/file_grid/snapshot/render.rs`
- Visible item slot assignment projection: `src/ui/file_grid/snapshot/slots.rs`
- Metadata/thumbnail scheduler queue projection: `src/ui/file_grid/snapshot/scheduler.rs`
- Visible range/work range projection: `src/ui/file_grid/snapshot/range.rs`
- Visible item snapshot/cache projection: `src/ui/file_grid/snapshot/visible.rs`
- Thumbnail candidate and read-ahead projection: `src/ui/file_grid/snapshot/thumbnail.rs`
- Metadata role candidate projection: `src/ui/file_grid/snapshot/metadata.rs`
- Active item-drag hover routing: `install_active_item_drag_mouse_tracker` plus
  drag preview repaint fallback in `src/ui/file_grid/dnd.rs`
- Runtime DnD debug channel: `FIKA_DEBUG_DND=1`, especially
  `[fika dnd] active-item-move`
- Compact/Icons image paint channel: `[fika item-image]`
  (`thumb_loaded`, `thumb_decoded`, `thumb_retained`, `thumb_fallback`;
  `theme_*` counters appear only in custom-theme A/B runs)
- Details visual paint channel: `[fika details-visual]`
- Renderer surface count channel: `[fika renderer-policy]`
- Runtime checklist: `docs/ITEM_VIEW_RUNTIME_SMOKE.md`
- Per-surface decisions: `docs/ITEM_VIEW_RENDERER_DECISIONS.md`

## Full Transition Roadmap

### R1: Freeze Current Evidence

Collect a desktop-session runtime log across Compact, Icons, and Details after
each painter or shell-boundary change:

```sh
FIKA_PERF_ITEM_VIEW=1 cargo run -- ~/Downloads 2>&1 | tee /tmp/fika-item-view.log
scripts/check-item-view-runtime-log.sh /tmp/fika-item-view.log
scripts/summarize-item-view-renderer-evidence.sh /tmp/fika-item-view.log
```

The log must include renderer-policy coverage for Compact, Icons, and Details.
Human review must still exercise item drag, directory drop, pane drop, Places
drop/reorder, external path drop, and rename caret click.

### R2: Separate Architecture From Renderer Code

Continue moving the item-view code toward Dolphin-style ownership boundaries:

- model/projection data stays in snapshot/layout code
- hit testing and DnD decisions stay in controller helpers
- painter data stays in paint snapshots and painter modules
- renderer choice stays in renderer-policy modules

Read-ahead must stay on the role/update side of that split. Dolphin computes the
visible first/last indexes in `KItemListViewLayouter::updateVisibleIndexes()`,
then `KFileItemModelRolesUpdater::indexesToResolve()` appends visible files,
visible directories, and bounded before/after read-ahead indexes for role work.
Fika mirrors that by keeping the raw Compact/Icons work range for scheduler
projection, while render conversion only materializes render snapshots for
currently visible items or already-cached read-ahead content. Invisible
read-ahead items may retain snapshot/cache state, but they must not enter
static visual or image prepaint, and they must not introduce new synchronous
icon-theme, image-cache-load, or text-shaping misses into the current frame.
Visible icon cache misses are the Dolphin-style exception: before render
conversion, Fika may spend Dolphin's 200ms `MaxBlockTimeout` budget resolving
visible `iconName` theme paths so the first visible paint does not show marker
icons and then switch to MIME icons. Read-ahead icon-theme work stays queued.

Current GPUI icon work follows that boundary: render conversion asks
`FileIconCache` for a cached or preliminary snapshot only. If the theme path is
missing, `RawFileGridSnapshot::queue_file_icon_resolve_candidates()` projects
the Dolphin visible/read-ahead order, and `FileIconResolveQueue` owns queued,
seen, and in-flight request state for background batches. Resolve completion
invalidates visible item snapshot caches so the next frame can swap preliminary
fallback icons for theme images without doing theme-directory scanning inside
the scroll or zoom frame.
When zoom changes the requested icon size, exact-size misses reuse an existing
same-kind cached icon snapshot from another size as a transition while the
exact-size request stays queued. This avoids a fallback-marker flash between
two real theme icons. Fika intentionally does not freeze the theme-icon resolve
size during zoom: Dolphin delays preview/role updater work with
`triggerIconSizeUpdate()`, but ordinary `iconName` pixmaps are generated from
the widget's current style-option icon size. Freezing Fika's theme-icon size
would create a visible second size adjustment when the delayed size commits.

Directory-load MIME metadata and visible icon paths now follow Dolphin's
visible-widget exception to that async rule. Dolphin keeps full role resolution
asynchronous, but `updateVisibleIcons()`/`initializeItemListWidget()` gives
created visible items an `iconName`, and `pixmapForIcon()` synchronously obtains
the themed pixmap through `QPixmapCache`. Fika mirrors this by resolving only
visible generic MIME metadata and visible theme-icon paths before queueing the
remaining metadata/icon work. Visible icon path resolution uses Dolphin's
`MaxBlockTimeout` budget of 200ms and still does not decode image resources in
the render/prepaint path. Read-ahead and offscreen items remain scheduler-owned.

Image-backed work follows the same visual stability rule. Thumbnail probe
success and failure remain model roles, and the thumbnail paint layer keeps a
real decoded thumbnail through transient GPUI image-cache misses whenever the
semantic source still matches. MIME/theme icons default to GPUI `img()`
elements fed by the same retained item snapshots because `/etc` evidence showed
the custom image layer exposed a first-load placeholder frame that GPUI's image
element path avoided. The custom theme-icon paint path remains available only
through `FIKA_CUSTOM_THEME_ICONS=1` for paired evidence. In either renderer,
theme icon decoding stays on GPUI's image-cache path; render/prepaint code must
not synchronously read or decode theme icon files. Thumbnails are retained only
by exact thumbnail path and continue to use contained image bounds. Thumbnail
fallback icons are still painted when no real image exists yet or the semantic
source changed.

The immediate non-GUI-safe work is to freeze fresh runtime evidence after the
Dolphin-aligned zoom/icon visual update, then execute the P15 transition order.
The large file-grid renderer/controller module has already been split into
focused model/projection, controller/hit-test, painter, and renderer-policy
modules.

### R3: Resolve Drag-Start Boundary

Do not remove the remaining GPUI drag-start shells until one of these is true:

- GPUI exposes a public custom-element drag-start API.
- Fika carries a small audited GPUI patch that exposes drag start from retained
  hitboxes, with runtime DnD evidence.

Removing the shell before this gate would make the architecture less reliable,
even if it looks closer to full custom paint.

Current source audit, using GPUI `0.2.2` from Zed commit `f16a469`, keeps this
gate closed. Drag initiation is exposed through
`Interactivity::on_drag` / `InteractiveElement::on_drag` in
`crates/gpui/src/elements/div.rs`, which constructs the typed drag preview from
an interactive element hitbox. GPUI custom elements can insert hitboxes with
`Window::insert_hitbox()` and can observe mouse/drag movement, but there is no
public API that starts a typed drag from an arbitrary retained painter hitbox.
`App::has_active_drag()` is only an observer for an already-started drag. The
practical boundary is therefore unchanged: item and Details drag-start shells
stay until GPUI exposes that hook or Fika intentionally carries a small,
audited patch.

The shell is now only the drag initiation boundary. Pane-internal item drag
hover must not depend on GPUI per-element `on_drag_move`; runtime evidence showed
that self-drags can emit `item-start` without later element drag-move callbacks.
Fika tracks active item drags from a window mouse listener installed by the
retained interaction layer, then routes the window position through the same
retained pane hit-test used by Places and external drops.

The accepted fallback is the drag preview repaint path. GPUI may keep repainting
the drag preview while the pointer moves even when it does not deliver the
underlying pane's drag-move callback for a same-window item drag. Fika therefore
uses the preview render pass only as a clock to query the current window mouse
position and run the same retained hit test. A valid smoke log can show only
`active-item-move via=preview`; the required signal is that the move reaches
`kind=Some(Directory)` before drop and the directory item highlights while the
cursor is over it.

The 2026-06-17 runtime trace confirmed this exact path: a pane self-drag first
reported `kind=Some(Pane)`, then crossed a directory and reported
`kind=Some(Directory) changed=true` through `via=preview` without requiring a
per-item `on_drag_move`. That means the accepted architecture is retained
hit-testing plus preview-driven ticking until GPUI exposes a public retained
drag-start/move API that can replace the remaining shell boundary.

### R4: Evaluate Rename Boundary

Keep the GPUI rename overlay while text editing remains a GPUI-owned platform
contract. A custom rename renderer needs behavior coverage for focus, caret
movement, selection, validation state, commit/cancel, and IME before it can be
accepted.

The concrete behavior matrix and Dolphin source comparison live in
`docs/RENAME_EDITOR_PLAN.md`.

### R5: Evaluate Places Renderer Separately

Places is not part of the current item-view custom-paint win. Before replacing
it:

- capture a GPUI baseline for scroll, reorder, external drop, item drop, device
  entries, hidden sections, and context menus
- define a retained Places row/section painter boundary
- prove that custom paint does not regress DnD or scroll behavior

Until then, keep Places on GPUI elements fed by retained places projection and
drag/drop state.

The concrete retained-row design and Dolphin source comparison live in
`docs/PLACES_RENDERER_PLAN.md`.

Places remains useful as the behavior reference for pane drop hover: dragging a
Place over pane directories and dragging a pane item over pane directories
should both produce a retained `Directory` item drop target while moving.

### R6: Pool Reuse Target

The long-term reuse-pool target is valid only when reusable visual identity is
owned outside GPUI child identity:

- Compact/Icons use visible slot ids and retained paint snapshots
- Details use row paint snapshots and shape caches
- image and text shaping caches are pane-local and slot/content keyed
- renderer-policy logs prove which surfaces remain GPUI shells

This target can advance while drag-start and rename stay on GPUI. The pool
boundary is the retained item/row state, not a claim that every renderer is
custom-painted today.

### R7: Full Transition Execution Order

The next transition work must follow this order:

1. Freeze current desktop-session evidence after the Dolphin-aligned zoom
   icon visual update. Use `~/Downloads` for ordinary MIME/thumbnail behavior,
   `/etc` for large mixed-directory scrolling, and `FIKA_DEBUG_DND=1` for pane
   self-drag hover.
2. Update `docs/ITEM_VIEW_RENDERER_DECISIONS.md` with evidence before changing
   a renderer surface. Do not treat a passing unit test as enough evidence for
   DnD, resize, fullscreen, or zoom visual stability.
3. Resolve the drag-start platform boundary from GPUI source or an audited
   local patch. Remove item/details drag shells only after retained hitboxes can
   start drags without losing payload, preview, cursor offset, or external drop
   behavior.
4. Treat Places as its own migration. It needs a GPUI baseline and a
   Places-specific retained row painter plan before any custom-paint switch.
5. Keep rename as a GPUI text-editing boundary until a custom editor covers
   focus, caret hit testing, UTF-8 selection, validation, commit/cancel, Tab
   rename-next, and IME.
6. Keep tightening reuse-pool evidence: ordinary item-view frames should show
   retained visual/image/text/interaction ownership with only the explicitly
   accepted GPUI platform boundaries remaining.

The detailed task board for this order is P15 in
`docs/ITEM_VIEW_CUSTOM_PAINT_TODO.md`.

### R8: Concrete Full-Transition Tracks

The accepted direction is a retained/custom-painted item view, but the
execution must stay split into evidence-backed tracks:

1. **Evidence track**: keep refreshing desktop-session logs for `~/Downloads`
   and `/etc`, including resize, fullscreen, scroll, zoom, mode switches, and
   DnD. These logs decide whether a renderer stays custom-painted, not the
   architectural preference alone.
   For image flicker and zoom-size investigations, include the historical
   GPUI-image baseline at `a3f5b0f` and transition checkpoints
   `d497593`/`8d1198f`/`36da130`/`b0cac9a` before changing the current image
   renderer.
2. **Painter track**: continue moving visual work into content-level painters
   only where the painter consumes retained snapshots and can match Dolphin's
   widget behavior. The next painter work is stabilization and measurement of
   image cold-load/zoom paths, not adding new visual surfaces blindly.
3. **Controller track**: keep click, menu, hover, cursor, selection, pane drop,
   item drop, and external drop routed through retained viewport hit testing.
   GPUI per-item callbacks are only temporary platform bridges.
4. **Shell-boundary track**: remove drag-start shells only after a public GPUI
   custom-element drag-start API or an audited local GPUI patch exists. Keep
   rename on GPUI until a behavior matrix covers text input and IME.
5. **Places track**: treat Places as a separate renderer migration. Its model
   and DnD state may be retained first, but the GPUI renderer stays until a
   Places-specific baseline and painter design are recorded.
6. **Ownership track**: keep extracting orchestration from `src/main.rs` into
   Dolphin-aligned file-grid modules when the move is behavior-preserving. This
   includes role scheduling handoff, runtime evidence helpers, and eventually
   shell-boundary ownership.

This is the practical meaning of "fully transition": every item-view behavior
should be owned by retained model/layout/controller/painter state, while any
remaining GPUI renderer is an explicit platform boundary with evidence and a
removal gate.
