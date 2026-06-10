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
- Dolphin breadcrumb buttons -> `BreadcrumbSegment { label, path }` rendered by the reusable pane component.

## Behavioral Rules

- Location state is pane-local and routed by `PaneId`.
- Breadcrumb clicks navigate through the same path as other pane loads so history stays pane-local.
- Ctrl+L and Alt+D switch the focused pane into editable location mode.
- Enter commits the typed path, Escape exits editable mode, and Tab attempts filesystem completion.
