# GPUI + Dolphin Migration Plan

This is the concrete plan for replacing the current Slint UI with GPUI while making Dolphin the first reference target.

> **Status: Completed.** All 8 implementation slices have been delivered in the
> GPUI mainline. The Slint implementation has been removed. The current codebase
> follows this plan's architecture: UI-neutral `fika-core` library, GPUI shell
> in `src/ui/`, and Dolphin-style directory/model/selection contracts. See
> `README.md`, `docs/DESIGN.md`, and `docs/TODO.md` for the current state.
> This document is retained as a historical record of the architecture transition.

## 1. Objective

Fika will be rebuilt as:

- a UI-neutral Rust core for directory/model/operation behavior
- a GPUI shell for windows, panes, item views, menus, dialogs, and input
- a Dolphin-like directory lister and model signal flow

The migration is a rewrite of the UI architecture. It is not a compatibility port of `.slint` files.

## 2. Why Replace Slint

The current problems are structural:

- static `.slint` component shape forces slot-based panes instead of natural pane entities
- dynamic pane state has to be projected through `ModelRc<VecModel<...>>`
- file identity, row identity, slot identity, overlay identity, and pane identity have to be manually synchronized
- callbacks easily lose pane identity and fall back to focused pane
- directory refresh and undo require glue to compensate for missing lister/model signal boundaries
- every UI state change risks a full pane or sidecar model rebuild

GPUI does not solve file-manager semantics by itself, but it lets Fika express panes, view state, and model updates directly as Rust entities. That removes the static UI glue layer that currently hides execution-flow mistakes.

## 3. Non-Negotiable Architecture Rules

- Dolphin source flow is checked before implementation.
- Every pane has a stable `PaneId`.
- Every pane-scoped async result carries `PaneId + generation`.
- Every same-generation overlapping request carries a request serial.
- Directory changes enter `DirectoryLister` first, then `DirectoryModel`, then view.
- Undo and file operations refresh by lister/model events, not by manual UI rebuild.
- Closing a pane drops its lister, watcher, pending view work, and stale-result target.
- No Slint compatibility code is kept in the GPUI path.

## 4. Dolphin Source Execution Flow

### Directory Entry Point

`../dolphin/src/views/dolphinview.cpp:2337`

```cpp
void DolphinView::loadDirectory(const QUrl &url, bool reload)
{
    if (reload) {
        m_model->refreshDirectory(url);
    } else {
        m_model->loadDirectory(url);
    }
}
```

Fika target:

```rust
pane.lister.load_directory(path, LoadMode::Load);
pane.lister.load_directory(path, LoadMode::Reload);
```

Manual refresh, watcher rescan, and operation/undo affected-directory refresh must call the reload path.

### Lister to Model

`../dolphin/src/kitemviews/kfileitemmodel.cpp:300`

```cpp
connect(m_dirLister, &KCoreDirLister::itemsAdded, this, &KFileItemModel::slotItemsAdded);
connect(m_dirLister, &KCoreDirLister::itemsDeleted, this, &KFileItemModel::slotItemsDeleted);
connect(m_dirLister, &KCoreDirLister::refreshItems, this, &KFileItemModel::slotRefreshItems);
connect(m_dirLister, &KCoreDirLister::listingDirCompleted, this, &KFileItemModel::slotCompleted);
```

Fika target:

```rust
enum DirectoryListerEvent {
    ItemsAdded { pane_id: PaneId, generation: Generation, path: PathBuf, entries: Vec<Entry> },
    ItemsDeleted { pane_id: PaneId, generation: Generation, path: PathBuf, paths: Vec<PathBuf> },
    ItemsRefreshed { pane_id: PaneId, generation: Generation, path: PathBuf, pairs: Vec<RefreshPair> },
    ListingRefreshed { pane_id: PaneId, generation: Generation, path: PathBuf, entries: Vec<Entry> },
    ListingCompleted { pane_id: PaneId, generation: Generation, path: PathBuf },
    CurrentDirectoryRemoved { pane_id: PaneId, generation: Generation, path: PathBuf },
    Error { pane_id: PaneId, generation: Generation, path: PathBuf, message: String },
}
```

`LoadingStarted` is a request/lifecycle signal, not a visual model reset. Fika
keeps the previous `DirectoryModel` and pane layout visible while the current
request is pending, cancels only transient UI interactions, and swaps the model
when the matching `ListingRefreshed` arrives. This follows Dolphin's practical
no-blank-frame behavior during directory changes and avoids flicker when async
listing is slower than the UI frame.

### Model to View

`../dolphin/src/kitemviews/kitemlistview.cpp:1812` connects the model to item view slots for item changes, insertions, removals, moves, groups, and sorting.

Fika target:

```rust
enum DirectoryModelSignal {
    ItemsInserted(ItemRangeList),
    ItemsRemoved(ItemRangeList),
    ItemsChanged(ItemRangeList, ChangedRoles),
    ItemsMoved(MoveList),
    GroupsChanged,
    SortChanged,
    ModelReset,
}
```

GPUI views subscribe to pane-local model signals. They do not parse filesystem watcher events.

### Current Directory Removed

`../dolphin/src/dolphinviewcontainer.cpp:1088` moves a local deleted current directory to the nearest existing ancestor.

Fika target:

- lister detects current directory removal
- pane validates `PaneId + generation + path`
- pane navigates to nearest existing ancestor
- message is shown in that pane

## 5. Target Module Design

### `fika-core::pane`

Owns:

- `PaneId`
- `PaneGeneration`
- `PaneState`
- pane history
- pane selection
- pane view options

Public API:

```rust
pub struct PaneId(u64);

pub struct PaneState {
    pub id: PaneId,
    pub generation: Generation,
    pub current_dir: PathBuf,
    pub model: DirectoryModel,
    pub selection: SelectionState,
    pub view: ViewState,
}
```

### `fika-core::directory`

Owns:

- `DirectoryLister`
- watcher abstraction
- lister event classification
- full reload fallback
- current directory removed detection

The watcher cannot apply UI changes. It only feeds lister events.

### `fika-core::model`

Owns:

- `DirectoryModel`
- entry storage
- path-index lookup
- sorting/filtering
- trash metadata
- model signals

### `fika-core::operations`

Owns:

- file operation queue
- operation progress
- undo registration
- undo serial
- affected-directory calculation

### `fika-gpui`

Owns:

- `FikaApp`
- `MainWindow`
- `PaneEntity`
- `ItemViewEntity`
- menus/dialogs
- input routing
- GPUI image/text rendering

GPUI entities consume core events and submit controller actions back to core.

## 6. Pane Identity Contract

Required invariants:

- `PaneId` is allocated once and never reused during process lifetime.
- split open creates a new `PaneId`.
- split close drops the closed pane's watcher/lister.
- focus changes do not change pane identity.
- async result apply never uses focused pane as fallback.
- two panes showing the same path still have separate generation, selection, scroll, lister, and watcher state.

Tests:

- close pane while directory read is running -> result is ignored
- split two panes on same path -> external create updates both by their own events
- focus other pane while undo completes -> original affected pane refreshes
- manual refresh on inactive pane -> inactive pane updates, focused pane does not

## 7. Directory Refresh Design

### Load

1. User navigates pane to path.
2. Pane increments generation.
3. Pane starts lister load.
4. Lister emits loading started.
5. Lister scans entries.
6. Model receives items/listing.
7. View receives model signal.
8. Lister emits completed.

### Refresh

1. Manual F5, watcher rescan, operation result, or undo result calls pane lister reload.
2. Lister produces item deltas where possible.
3. If event is unclassifiable, lister emits `ListingRefreshed`.
4. Model diffs current entries and emits insert/delete/change signals.
5. View updates visible item layout.

### Watcher Delta

Mapping:

- create -> `ItemsAdded`
- remove -> `ItemsDeleted`
- rename both -> `ItemsRefreshed` with old/new pair
- modify metadata/data -> `ItemsRefreshed`
- rescan/no path/unclassified -> `ListingRefreshed`
- watched root removed -> `CurrentDirectoryRemoved`

## 8. GPUI Rendering Plan

### First View Mode

Start with Dolphin compact horizontal layout:

- rows fill vertically
- columns advance horizontally
- ordinary wheel scrolls horizontally
- item layout owns icon rect and text rect
- model index is not GPUI row index

### View Layers

Recommended GPUI view composition:

- pane chrome view
- search/filter view
- item viewport view
- selection overlay
- context menu layer
- dialog layer

The item viewport should be a Rust-owned visible layout, not a static list widget. The model and layouter decide visible indexes; GPUI renders those items.

## 9. Migration Phases

### Phase A: Spike

Deliverable:

- separate GPUI binary or crate
- single pane
- load local directory
- display names
- external create/delete refreshes view

Acceptance:

- no Slint dependency in the GPUI crate
- directory events carry `PaneId + generation`
- tests cover stale read result and watcher update

### Phase B: Core Extraction

Deliverable:

- `fika-core` crate
- moved file ops, entries, generation, operation controller
- UI-neutral image/cache types where needed

Acceptance:

- core builds without Slint or GPUI
- old Slint binary can still compile only if kept behind old path
- new tests run against core directly

### Phase C: Directory Model Parity

Deliverable:

- `DirectoryLister`
- `DirectoryModel`
- Dolphin-style model signals

Acceptance:

- add/delete/rename/modify tests
- full reload fallback test
- current directory removed test
- split pane same-path test

### Phase D: GPUI Pane Shell

Deliverable:

- dynamic pane tree
- split open/close
- focus routing
- path bar
- status bar

Acceptance:

- every pane action resolves by `PaneId`
- inactive pane refresh works
- closing pane drops watcher/lister

### Phase E: Item View and Selection

Deliverable:

- compact layout
- scroll
- hit-test
- single/ctrl/shift/rubber-band selection

Acceptance:

- selection tests are core/controller tests
- UI rendering is replaceable
- large directory remains responsive

### Phase F: Operations and Undo

Deliverable:

- copy/move/link/trash/rename/create
- undo with serial
- affected pane refresh

Acceptance:

- undo never applies stale serial
- undo refresh does not call manual UI rebuild
- file operation completion refreshes all affected panes

### Phase G: Feature Recovery

Deliverable:

- context menus
- service menus
- open with
- thumbnails
- devices
- recursive search
- portal chooser

Acceptance:

- each feature routes through pane/core contracts
- no focused-pane fallback
- no UI-thread blocking I/O

### Phase H: Cutover

Deliverable:

- GPUI app becomes primary `fika`
- Slint UI removed from main build
- docs updated

Acceptance:

- `Cargo.toml` no longer depends on `slint` / `slint-build`
- `ui/*.slint` is deleted or archived outside the build
- README describes GPUI architecture

## 10. Testing Strategy

Required test categories:

- core unit tests for directory model deltas
- watcher classification tests
- stale generation tests
- undo serial tests
- split pane identity tests
- lister refresh tests after operations
- GPUI smoke test for app startup and basic pane rendering

Manual smoke cases:

- create file externally in current pane
- delete file externally in current pane
- rename file externally
- delete current directory
- split two panes on same path and modify path externally
- undo copy/move/trash/rename
- close pane while async read is in flight

## 11. Cutover Criteria

The GPUI rewrite replaces the Slint app only after:

- directory refresh is correct without split-toggle workaround
- undo refresh is correct without manual pane rebuild
- split pane identity is stable
- trash metadata changes update the visible trash model
- thumbnail and service-menu work off the UI thread
- portal chooser path is decided
- the old Slint dependency can be removed

## 12. Risks

- GPUI text/layout primitives may require custom item-view rendering work.
- File-manager semantics still need custom lister/model logic; GPUI only removes UI glue pressure.
- Portal chooser embedding may require a separate strategy from the main GPUI shell.
- Some Slint-era tests assert implementation shape rather than behavior and must be replaced.

## 13. First Implementation Slice

Implement in this order:

1. `PaneId`, `Generation`, `DirectoryListerEvent`, `DirectoryModelSignal`.
2. UI-neutral directory entry type.
3. `DirectoryModel` with add/delete/refresh/full listing APIs.
4. `DirectoryLister` for local directory load/reload.
5. watcher classification feeding lister events.
6. GPUI single-pane shell.
7. GPUI split-pane shell.
8. undo refresh through lister reload.

Do not migrate menus, thumbnails, devices, or portal before the directory lister and split-pane identity contracts are proven.
