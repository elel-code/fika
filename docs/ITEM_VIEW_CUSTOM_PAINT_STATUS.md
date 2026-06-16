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
| Compact/Icons click, menu, hover, cursor, and drop hit testing | replaced | retained viewport/custom hitboxes | runtime DnD smoke still required after painter changes |
| Compact/Icons drag start | not replaced | GPUI `Div::on_drag` shell | public GPUI custom-element drag-start API or audited Fika GPUI patch |
| Compact/Icons rename editor | not replaced | GPUI editor overlay | only revisit after caret, selection, IME, and text input behavior are covered |
| Details row model and geometry | retained | Details paint snapshots and row layout projection | none for current path |
| Details row backgrounds, icons, text cells, Trash columns | replaced | custom content-level painter | runtime Details perf and DnD smoke evidence must stay current |
| Details click, menu, navigation, hover, cursor, drop hit testing | replaced | retained row hit testing/controller state | runtime DnD smoke still required after painter changes |
| Details drag start | not replaced | GPUI `Div::on_drag` row shell | same drag-start API or audited GPUI patch gate |
| Places rows and sidebar scrollbar | not replaced | GPUI elements over retained places projection | requires separate GPUI baseline, runtime DnD smoke, and Places-specific custom painter plan |

The practical state is: item-view static visuals and most app-side controller
paths have moved to retained/custom-painted architecture. Drag-start and rename
remain GPUI renderer/platform-contract boundaries. Places has retained model and
DnD state helpers, but its renderer is still GPUI.

## Evidence Anchors

- Renderer policy code: `src/ui/file_grid/renderer_policy.rs`
- Compact/Icons static visual painter: `StaticItemVisualLayerElement` in
  `src/ui/file_grid.rs`
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
