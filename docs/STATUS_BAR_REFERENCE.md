# Status Bar Reference

Fika's pane-local status bar maps to Dolphin's view-container status bar flow.

## Dolphin Source

- `../dolphin/src/dolphinviewcontainer.cpp`
  - Creates `DolphinStatusBar` for the view container.
  - Connects `DolphinView::statusBarTextChanged` to `DolphinStatusBar::setDefaultText`.
  - Connects `DolphinView::zoomLevelChanged` to `DolphinStatusBar::setZoomLevel`.
  - Connects `DolphinStatusBar::zoomLevelChanged` back to the active view through `slotStatusBarZoomLevelChanged()`.
  - Connects `DolphinStatusBar::stopPressed` to directory loading cancellation.
- `../dolphin/src/statusbar/dolphinstatusbar.cpp`
  - Owns the text label, zoom label, zoom slider, `StatusBarSpaceInfo`, progress bar, and stop button.
  - Uses a delayed progress-bar timer so short operations do not flash progress UI.
  - Emits `zoomLevelChanged(int)` when the status bar slider changes.
- `../dolphin/src/statusbar/statusbarspaceinfo.cpp`
  - Owns the capacity bar and free-space text button.
  - Uses `SpaceInfoObserver` to update available size, total size, and percent used for the current URL.
- `../dolphin/src/views/dolphinview.cpp`
  - `requestStatusBarText()` summarizes selected items by folder count, file count, and total file size.
  - `emitStatusBarText()` formats selected and unselected count/size text for the status bar.
- `../dolphin/src/views/zoomlevelinfo.cpp`
  - Defines the zoom-level range and icon-size mapping used by the status bar slider tooltip and view zoom state.

## Fika Mapping

- Dolphin view-container status bar -> reusable pane-local GPUI status bar in `src/ui/status_bar.rs`, rendered by `src/ui/pane.rs`.
- Dolphin status text -> each `PaneSnapshot` carries its own `StatusBarSnapshot`, derived from that pane's `DirectoryModel` entries and `SelectionState`.
- Dolphin zoom slider -> status bar draggable segmented zoom control routed through `FikaApp::set_zoom_level(pane_id, ...)`.
- Dolphin space info -> pane path space snapshot cached by `FikaApp` and refreshed on a background task.
- Dolphin progress bar and stop button -> pane-bound `OperationProgressHandle` backed by core `TransferProgress` and an `AtomicBool` cancel flag for internal copy/move.
- Dolphin directory loading stop -> pane loading state tracked by `PaneId + generation + request_serial`, routed to `ListingWorker::cancel_pane()`.
- Dolphin delayed progress timer -> Fika progress snapshots become visible only after the same delayed-progress interval.

## Behavioral Rules

- The status bar is part of each reusable pane, matching Dolphin's `DolphinViewContainer -> DolphinStatusBar` ownership model.
- Status text is stored per `PaneId`; it never falls back to the focused pane.
- Selection summaries use `ItemId` membership and never call `selected_paths()` for status text.
- Select-all remains compact; status summary scans model entries only when `model_generation` or selection revision changes.
- Zoom changes update only the target pane view state and invalidate that pane's compact column metrics.
- Space information is queried off the render path and read from cache during rendering.
- Copy/move progress is reported from core file operation callbacks, and cancellation is handled by core cancellation checks.
- Operation progress is shown only on the operation pane.
- Directory loading Stop cancels only the target pane's current request key; stale listing results still fail the existing target checks.
- Short operations that finish before the delayed-progress threshold do not flash a progress bar.
