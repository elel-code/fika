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
| Compact/Icons thumbnail and theme-icon images | replaced | custom image painter using GPUI `RetainAllImageCache` | keep GPUI image decode/cache unless a narrower image baseline wins |
| Compact/Icons click, menu, hover, cursor, and drop hit testing | replaced | retained viewport/custom hitboxes plus active item-drag window tracker | runtime DnD smoke still required after painter changes |
| Compact/Icons drag start | not replaced | GPUI `Div::on_drag` shell | public GPUI custom-element drag-start API or audited Fika GPUI patch |
| Compact/Icons rename editor | not replaced | GPUI editor overlay | only revisit after caret, selection, IME, and text input behavior are covered |
| Details row model and geometry | retained | Details paint snapshots and row layout projection | none for current path |
| Details row backgrounds, icons, text cells, Trash columns | replaced | custom content-level painter | runtime Details perf and DnD smoke evidence must stay current |
| Details click, menu, navigation, hover, cursor, drop hit testing | replaced | retained row hit testing/controller state plus active item-drag window tracker | runtime DnD smoke still required after painter changes |
| Details drag start | not replaced | GPUI `Div::on_drag` row shell | same drag-start API or audited GPUI patch gate |
| Places rows and sidebar scrollbar | not replaced | GPUI elements over retained places projection | requires separate GPUI baseline, runtime DnD smoke, and Places-specific custom painter plan |

The practical state is: item-view static visuals and most app-side controller
paths have moved to retained/custom-painted architecture. Drag-start and rename
remain GPUI renderer/platform-contract boundaries. Places has retained model and
DnD state helpers, but its renderer is still GPUI.

## Evidence Anchors

- Renderer policy code: `src/ui/file_grid/renderer_policy.rs`
- Root file-grid render surface composition: `src/ui/file_grid/surface.rs`
- Compact/Icons layout options and Dolphin sizing constants:
  `src/ui/file_grid/layout.rs`
- Compact/Icons static visual painter: `src/ui/file_grid/static_visual.rs`
- Compact/Icons image paint layer: `src/ui/file_grid/image_layer.rs`
- Compact/Icons transparent item shell boundary: `src/ui/file_grid/item_shell.rs`
- Details visual painter: `src/ui/file_grid/details_visual.rs`
- Details transparent row shell boundary: `src/ui/file_grid/details_shell.rs`
- GPUI rename overlay boundary: `src/ui/file_grid/rename_overlay.rs`
- Shared visual style and item identity helpers: `src/ui/file_grid/style.rs`
- File-grid root API snapshot/props/viewport types: `src/ui/file_grid/types.rs`
- Visible item snapshot/cache projection: `src/ui/file_grid/snapshot/visible.rs`
- Thumbnail candidate and read-ahead projection: `src/ui/file_grid/snapshot/thumbnail.rs`
- Metadata role candidate projection: `src/ui/file_grid/snapshot/metadata.rs`
- Active item-drag hover routing: `install_active_item_drag_mouse_tracker` plus
  drag preview repaint fallback in `src/ui/file_grid/dnd.rs`
- Runtime DnD debug channel: `FIKA_DEBUG_DND=1`, especially
  `[fika dnd] active-item-move`
- Compact/Icons image paint channel: `[fika item-image]`
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

The immediate non-GUI-safe work is to split the large `src/ui/file_grid.rs`
painter/controller code into smaller modules without changing behavior.

### R3: Resolve Drag-Start Boundary

Do not remove the remaining GPUI drag-start shells until one of these is true:

- GPUI exposes a public custom-element drag-start API.
- Fika carries a small audited GPUI patch that exposes drag start from retained
  hitboxes, with runtime DnD evidence.

Removing the shell before this gate would make the architecture less reliable,
even if it looks closer to full custom paint.

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

### R5: Evaluate Places Renderer Separately

Places is not part of the current item-view custom-paint win. Before replacing
it:

- capture a GPUI baseline for scroll, reorder, external drop, item drop, device
  entries, hidden sections, and context menus
- define a retained Places row/section painter boundary
- prove that custom paint does not regress DnD or scroll behavior

Until then, keep Places on GPUI elements fed by retained places projection and
drag/drop state.

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
