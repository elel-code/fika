# Rename Editor Plan

Rename is a text-editing platform boundary. Do not replace the GPUI overlay
with a custom painter until this matrix is behavior-complete.

## Dolphin Reference

Dolphin uses two paths:

- `DolphinView::renameSelectedItems()` starts inline rename only for a single
  selected item when inline rename is enabled. It scrolls the item into view,
  then calls `KItemListView::editRole(index, "text")`.
- `KItemListView::editRole()` marks the item current and delegates the editor
  to the visible `KStandardItemListWidget`.
- `KItemListRoleEditor` is a `KTextEdit`. It owns text editing behavior:
  Escape cancels, Enter commits, Tab/Down can commit and edit next, Backtab/Up
  can edit previous, FocusOut commits except popup focus, Home/End and
  Left/Right selection collapse follow `QTextCursor`, and the editor auto
  adjusts size while the text changes.
- Multiple-item rename or disabled inline rename uses `KIO::RenameFileDialog`.

The important Dolphin lesson is that inline rename is not just painted text. It
is a real text editor with focus, selection, keyboard, validation handoff, and
platform input behavior.

## Current Fika Boundary

Fika currently keeps rename as a GPUI overlay:

- Model state lives in `src/ui/rename/draft.rs` as `RenameDraft`.
- Keyboard classification lives in `src/ui/shortcuts.rs` as
  `RenameInputAction`.
- Geometry and rendering live in `src/ui/file_grid/rename_overlay.rs`.
- App orchestration in `src/main.rs` starts the draft, maps click position to
  caret, validates commit, supports privileged rename, supports Tab
  rename-next, and retargets the draft when file watcher events rename the
  underlying item.

This is acceptable as a platform boundary. It should remain explicit in
renderer-policy logs as a GPUI overlay surface.

## Behavior Matrix

| Behavior | Current Fika | Required Before Custom Editor |
| --- | --- | --- |
| Single-item inline start | Covered by `start_rename_in_pane()` and selection checks. | Keep item scroll/visibility semantics and focused-pane ownership. |
| Multiple-item rename | Not Dolphin-equivalent; Fika asks for one item. | Either keep one-item behavior or design a dialog equivalent before claiming Dolphin parity. |
| Initial stem selection | Covered by `RenameDraft::new()`. | Preserve extension/stem selection and UTF-8 boundaries. |
| Text insert/delete | Covered for key-char, Backspace, Delete. | Add composition-aware text input before replacing GPUI text handling. |
| IME/composition | Not covered by custom model. GPUI receives key chars only. | Required. Need composition start/update/commit/cancel and marked text rendering. |
| Caret mouse hit testing | Covered by layout projection and `rename_caret_for_local_x()`. | Preserve after scroll, zoom, Details layout, and long-name width expansion. |
| Selection | Covered for Shift+Home/End/Left/Right and Select All. | Add mouse drag selection and platform word/line selection decisions if custom editor owns input. |
| FocusOut commit/cancel | Not currently a full text-widget contract. | Define FocusOut behavior and popup/context-menu exceptions before custom editor. |
| Escape cancel | Covered. | Preserve without accidentally committing on focus loss. |
| Enter commit | Covered. | Preserve validation and async operation handoff. |
| Tab rename-next | Covered for forward rename-next. | Add Backtab/previous decision or document intentionally different behavior. |
| Up/Down chain edit | Not implemented. | Decide whether Dolphin Up/Down chain edit is required. |
| Extension warning | Covered by `RenameDraft::extension_warning()`. | Preserve warning placement and no-warning directory behavior. |
| Validation errors | Covered for empty name, remote rename, missing parent, and destination conflict. | Preserve helper text without layout overlap in all item modes. |
| Privileged rename | Covered by privileged draft path. | Preserve action label, status, and pending rename-next clearing. |
| Watcher retarget | Covered by draft retarget tests. | Preserve draft text/caret/selection/error while original path changes. |
| Accessibility | Not covered by custom painter. | Required before replacing a real text-editable overlay. |

## Migration Order

1. Keep the GPUI rename overlay as the default renderer.
2. Add renderer-policy evidence that rename overlay count is visible when a
   draft is active.
3. If a custom editor is still desired, first design an input model that covers
   IME, focus, mouse selection, clipboard, accessibility, and chain-edit
   behavior.
4. Implement the custom editor behind an opt-in flag only after that behavior
   model has tests.
5. Compare against the GPUI overlay for correctness and responsiveness. If the
   custom editor is incomplete or slower, keep the GPUI overlay.

## Acceptance Gates

- Unit tests cover UTF-8 movement, selection, deletion, extension warning,
  validation errors, rename-next, watcher retarget, and privileged rename.
- Runtime smoke covers start rename, click caret, edit, Escape, Enter, Tab
  rename-next, error helper text, scroll/zoom while editing, and rename after
  watcher retarget.
- Custom editor smoke must include IME/composition on a desktop session before
  becoming default.
- No custom rename painter can be accepted if it regresses text input behavior,
  even if the visual paint path is faster.
