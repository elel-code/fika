# Search and Filter Reference

Fika's pane-local filter bar follows Dolphin's inline filter path, not a
render-layer search.

## Dolphin Sources

- `../dolphin/src/filterbar/filterbar.{h,cpp}`
  - Owns the inline filter widget.
  - `QLineEdit::textChanged` emits `FilterBar::filterChanged`.
  - Escape clears a non-empty filter and closes the bar when the field is
    already empty.
  - Enter and navigation keys hand focus back to the file view.
  - The default mode is Dolphin's glob mode, with case-insensitive matching.

- `../dolphin/src/dolphinviewcontainer.{h,cpp}`
  - Creates `FilterBar` in each view container.
  - Connects `filterChanged`, `filterModeChanged`, and
    `caseSensitiveChanged` to the active `DolphinView`.
  - `setFilterBarVisible(true)` focuses and selects the filter input.
  - `DolphinView::urlChanged` clears the filter text when the lock button is
    not enabled.

- `../dolphin/src/views/dolphinview.{h,cpp}`
  - `DolphinView::setNameFilter()` forwards directly to the item model.
  - Filter state does not become navigation history.

- `../dolphin/src/kitemviews/private/kfileitemmodelfilter.{h,cpp}`
  - Implements `PlainText`, `Glob`, and `Regex` matching.
  - Glob mode uses wildcard conversion with unanchored matching.
  - Case-insensitive plain text stores a lower-case pattern for cheap matching.

- `../dolphin/src/kitemviews/kfileitemmodel.{h,cpp}`
  - `KFileItemModel::setNameFilter()` dispatches pending inserts, updates the
    model filter, then calls `applyFilters()`.
  - `applyFilters()` moves hidden items out of the visible item list and
    restores matching items from `m_filteredItems`.

## Fika Mapping

- Core model identity remains unchanged: `DirectoryModel` still stores all
  entries and stable `ItemId`s.
- A pane-local filter state mirrors Dolphin's view-container filter state.
- Filter bar UI state is split as a directory-style Rust module:
  `src/ui/filter_bar.rs` is the module entry, and
  `src/ui/filter_bar/state.rs` owns `FilterBarSnapshot`, `PaneFilterState`,
  and filtered model cache key/entry structs.
- The active filter produces a cached visible-index mapping from layout index
  to `DirectoryModel` index. The cache key is the pane id, model generation,
  filter mode, case sensitivity, and query.
- GPUI rendering consumes `CompactLayout` with the filtered item count, then
  maps each visible layout index back to the stable model entry.
- Rubber-band, hit-test, range selection, keyboard movement, and select-all
  operate on the filtered mapping only while a non-empty filter is active.
- Closing the filter bar clears the query and releases its cached index vector.
