# Dolphin Retained Renderer Alignment

This document defines what “Dolphin-aligned custom rendering” means for Fika.
It answers the recurring question: full custom rendering can reach the expected
performance, but only when the surrounding model, cache, painter and event
delivery architecture are also Dolphin-aligned. Replacing a GPUI element with a
custom painter is not sufficient by itself.

## Target

The long-term target is a Dolphin-style retained view:

- A stable model owns item/place identity, ordering, selection and roles.
- A viewport-level controller owns hit testing, hover, drop targeting and
  activation dispatch.
- A retained layout projection owns visible geometry and slot reuse.
- Painters consume projected state and cached assets; they do not resolve file
  roles, scan icon themes, decode images or decide DnD semantics.
- Runtime analyzers decide whether a custom renderer can become the default.

This is compatible with GPUI. GPUI can remain the windowing, text, image decode
or typed drag bridge while Fika owns the Dolphin-style model/controller/painter
state. The bridge should only disappear when equivalent retained behavior and
performance evidence exist.

## Root Cause Of The Remaining Gap

The remaining gap is not proof that GPUI built-in rendering is inherently faster
than a retained custom renderer. It shows that some surfaces are not yet a
complete Dolphin-style loop.

| Surface | Current gap | Dolphin-aligned requirement |
| --- | --- | --- |
| MIME/theme icons | Custom image painting can expose first-frame placeholder or ready-state churn. | Use semantic icon identity plus size/scale/theme keyed retained image readiness. Once a real image is loaded for a key, zoom/scroll frames must not fall back to blank or marker placeholders. |
| Zoom | Any delayed icon identity or image-size commit creates a visible second jump. | Geometry changes immediately; expensive role/image work is deferred or warmed, and paint uses stable icon identity for the current size. |
| Large `/etc`-style directories | Visible role/icon work can spike if conversion synchronously resolves too much. | Keep visible-first bounded role work, cache resolved visible roles, and never let read-ahead work enter the paint/convert path. |
| Places | Row chrome is custom, but text/icons and some typed DnD bridge state remain GPUI. | Event delivery and hit testing must be viewport-level retained state before claiming full Places retained behavior. Drag start remains a separate platform boundary. |
| Drag start | GPUI public API still exposes typed drag start through interactive elements. | Either keep a minimal shell, or add/audit a retained-hitbox typed drag-start API before removal. |
| Rename | GPUI editor still provides focus, caret, selection and IME behavior. | Retained rename must pass the full editor behavior matrix before default use. |

## Dolphin Contracts Fika Must Match

### Model And Identity

- Item identity is `ItemId` plus pane-local retained slots, not GPUI element
  keys.
- Place identity is the projected semantic identity used by the Places slot
  cache, not row element identity.
- Renderer policy must be derived from model/layout/readiness state and logged
  every time it is used as evidence.
- Sorting, Places order, device state, selection and drop semantics are never
  owned by painters.

### Layout And Slot Reuse

- Scroll and zoom update layout geometry without rebuilding logical item state.
- Visible slot pools keep stable identity across overlapping scroll ranges.
- New visible rows/items may allocate slots; unchanged visible rows/items should
  be visual or geometry updates, not content rebuilds.
- Layout projection is the only source of paint and hit-test rectangles.

### Role And Asset Readiness

- Visible work is allowed to be prioritized; read-ahead work must stay off the
  render path.
- MIME/theme icon renderer promotion requires readiness for the exact
  `(icon_name, icon_size_px, scale, theme/color-mode)` key that the painter will
  draw.
- Thumbnail retention remains keyed by thumbnail/source path, not theme icon
  identity.
- Image decode can continue to use GPUI `RetainAllImageCache`; custom rendering
  does not imply a custom decoder.

### Painter

- Painters must only consume retained snapshots, text shapes and retained image
  handles.
- A painter cannot synchronously scan an icon theme, read a thumbnail, inspect
  MIME magic or enqueue model role changes during prepaint/paint.
- Fallback visuals are acceptable only before a real same-key image exists.
  After a same-key image exists, pending/failed refreshes retain the last real
  image.

### Controller And Event Delivery

- Viewport-level hit testing owns hover, cursor, activation target, context-menu
  target and drop target selection.
- Places row/section shells are not allowed to be counted as retained event
  delivery once the policy claims full retained events.
- Typed drag start is tracked separately from event delivery because current
  GPUI exposes it as a platform bridge.

## Default Promotion Rules

A renderer can become default only when all of these are true:

- The Dolphin owner split is documented: model, layout, controller/hit-test,
  painter, cache and remaining bridge.
- The GPUI baseline and candidate custom path are compared on the same directory,
  viewport action and mode.
- Analyzer gates pass without weakening existing checks.
- Logs show no user-visible placeholder churn, blank first frame, icon-size
  second jump, synchronous paint-time decode or event-shell regression.
- The relevant decision document records the root cause, implementation boundary,
  Dolphin comparison and `/tmp` evidence paths.

If custom paint loses to GPUI on a surface, keep the retained model/controller
state and continue using the GPUI renderer for that surface. That is still
Dolphin-aligned when the bridge is explicit and evidence-backed.

## Execution Order

1. Freeze evidence with `scripts/run-retained-renderer-evidence.sh --core`.
2. Finish MIME/theme icon hybrid evidence before changing the default icon
   renderer.
3. Finish Places retained event delivery before claiming full Places retained
   behavior.
4. Re-audit GPUI typed drag-start support after dependency updates.
5. Convert rename only after its editor behavior matrix has tests or runtime
   smoke coverage.
6. After each accepted slice, update the relevant plan/TODO and commit it
   separately.
