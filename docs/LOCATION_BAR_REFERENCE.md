# Location Bar Reference

Fika's pane-local location bar maps to Dolphin's `KUrlNavigator` path.

## Dolphin Source

- `../dolphin/src/dolphinmainwindow.cpp`
  - `DolphinMainWindow::replaceLocation()` handles the shortcut that switches the active view's URL navigator into editable mode.
  - `DolphinMainWindow::toggleEditLocation()` handles explicit editable-location toggling.
  - `DolphinMainWindow::changeUrl()` receives URL changes from the active view container and routes them through the active view.
  - The action setup registers `replace_location` with Ctrl+L / Alt+D and `editable_location` with F6.
- `../dolphin/src/dolphinnavigatorswidgetaction.cpp`
  - Creates one `DolphinUrlNavigator` per active split view container.
  - Connects `KUrlNavigator::urlChanged` to view navigation.
  - Keeps special buttons such as Trash/Network tied to the navigator's current URL.
- `../dolphin/src/dolphinnavigatorswidgetaction.h`
  - Stores primary and secondary URL navigator accessors for split views.

## Fika Mapping

- Dolphin `KUrlNavigator` -> Fika pane header location bar.
- Dolphin active view container URL routing -> `FikaApp::load_pane(PaneId, PathBuf)`.
- Dolphin editable URL mode -> pane-scoped `LocationDraft`.
- Editable draft/caret/snapshot state -> `src/ui/location_bar.rs` as the module
  entry and `src/ui/location_bar/draft.rs` as the directory-style child module.
- Editable metrics/caret hit-test state -> `src/ui/location_bar/metrics.rs` as a
  directory-style child module.
- Dolphin breadcrumb buttons -> core `BreadcrumbSegment { label, path }`
  built by `src/core/location.rs` and rendered by the reusable pane component.
- Dolphin URL parsing/completion behavior -> `src/core/location.rs`, which owns
  `~` expansion, startup directory normalization, absolute/relative path
  resolution, breadcrumb segment construction, and filesystem completion strings
  shared by startup path parsing, Places Add/Edit path input, and the pane
  location bar.
- Editable location mode uses pane-local caret and horizontal scroll state, so
  long paths truncate inside the pane header instead of forcing pane width.
- Editable text metrics include the current visible width, and cursor drawing
  clamps safely when split panes are narrower than the cursor itself.

## Behavioral Rules

- Location state is pane-local and routed by `PaneId`.
- Breadcrumb clicks navigate through the same path as other pane loads so history stays pane-local.
- Ctrl+L and Alt+D switch the focused pane into editable location mode.
- Enter commits the typed path, Escape exits editable mode, and Tab attempts filesystem completion.
- Path text parsing and completion are UI-neutral core behavior; the GPUI app
  only owns the active draft, caret metrics, and pane navigation dispatch.
- Caret movement keeps the cursor visible without resetting the horizontal
  scroll unless the caret crosses the visible edge.
- Breadcrumb segment text is shrinkable and clipped inside the pane header; a
  long path segment cannot impose a new minimum pane width.
