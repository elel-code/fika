# Smooth Scroll Reference

Fika's pane-local smooth scrolling maps to Dolphin's item-list container
smooth scroller rather than to a generic GPUI scroll view.

## Dolphin Source

- `../dolphin/src/kitemviews/private/kitemlistsmoothscroller.h`
  - Defines `KItemListSmoothScroller` as the helper around a `QScrollBar`,
    target object and animated scroll property.
  - Exposes `scrollContentsBy()`, `scrollTo()`, `requestScrollBarUpdate()`,
    `handleWheelEvent()` and `scrollingStopped()`.
- `../dolphin/src/kitemviews/private/kitemlistsmoothscroller.cpp`
  - Creates a `QPropertyAnimation` for the target scroll property.
  - `scrollContentsBy()` computes the target offset after the scrollbar has
    changed, keeps interrupted animations continuous by advancing the start
    offset by one frame, and uses `InOutQuad` for a fresh animation and
    `OutQuad` for retargeted animations.
  - `requestScrollBarUpdate()` stops running animation when the scrollbar
    maximum changes, so relayout/content changes do not keep stale animation
    targets.
  - `handleWheelEvent()` forwards wheel events to the scrollbar while enabling
    smooth scrolling for that event.
- `../dolphin/src/kitemviews/kitemlistcontainer.cpp`
  - Owns separate horizontal and vertical `KItemListSmoothScroller` instances.
  - Forwards `scrollContentsBy(dx, dy)` to the corresponding smooth scrollers.
  - Uses `QScroller::scroller(viewport())` / `grabGesture()` for kinetic
    gesture scrolling and stops it through the controller.
  - Connects smooth-scroller `scrollingStopped` back to `KItemListView`.
- `../dolphin/src/kitemviews/kitemlistview.cpp`
  - `KItemListView::setScrollOffset()` clamps the offset and immediately calls
    `doLayout(NoAnimation)`, so smooth scrolling still lays out visible items
    on each animated offset.

## Fika Mapping

- Dolphin `KItemListSmoothScroller` -> `src/core/scroll.rs::SmoothScroll`.
- Dolphin scrollbar maximum invalidation -> `FikaApp::set_pane_viewport_bounds()`
  clears pane-local smooth scroll state when viewport/content bounds change.
- Dolphin interrupted animation handling -> `SmoothScroll::retarget()` carries
  the old target forward and advances the new start by one frame.
- Dolphin fresh/retarget easing -> `InOutQuad` for new wheel animations and
  `OutQuad` for retargeted wheel animations.
- Dolphin `QScroller` kinetic gesture path -> Fika samples scrollbar drag
  velocity with `ScrollDragTracker` and starts a pane-local kinetic `SmoothScroll`
  on drag release.
- Dolphin `setScrollOffset()` synchronous layout path -> Fika's animation tick
  writes `ViewState.scroll_x/scroll_y`, so `compact_layout_for_model()` and
  visible-item virtualization are recalculated from the current animated
  offset, not from a full-model render path.

## Implementation Notes

- Smooth scroll state is stored per `PaneId`; split panes never share animation
  state.
- Directory navigation/back/forward resets `ViewState` scroll to `0,0` in core.
- Directory switching, pane close, zoom changes and viewport bound changes clear
  smooth scroll and scrollbar drag trackers.
- The model remains unchanged: smooth scrolling only changes view offset and does
  not allocate extra visible items beyond the existing virtualized range.
- Scroll state stays as `f32`; GPUI rendering rounds the translated content
  offset to whole pixels.
