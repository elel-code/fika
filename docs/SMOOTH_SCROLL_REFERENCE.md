# Smooth Scroll Reference

This document records Dolphin's item-list smooth scrolling model. Fika's
previous `src/core/scroll.rs` and `src/ui/item_view_container/*` smooth paths
were deleted with the broken pane-coupled scrollbar. The current code keeps the
independent item-view scrollbar in `src/ui/item_view/scroll_bar.rs`; wheel input
updates the pane `ViewState` directly through the Dolphin `setScrollOffset()`
ownership model. Smooth and kinetic scrolling are reference behavior for a
future rebuild, not active compatibility code.

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
  - When an animation is already running, the Dolphin formula is preserved:
    `distance += currentOffset - oldEndOffset`,
    `endOffset = currentOffset - distance`, then
    `startOffset += (endOffset - currentOffset) * 1000 / (duration * 60)`,
    clamped toward `endOffset`.
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

- Dolphin `KItemListContainer` owned scrollbars -> currently
  `src/ui/item_view/scroll_bar.rs`, mounted by `src/ui/file_grid.rs` as a
  sibling overlay of the tracked item viewport rather than by `src/ui/pane.rs`;
  geometry and drag math read/write the pane-local `gpui::ScrollHandle`.
- Dolphin `KItemListSmoothScroller` is documented here only as the future
  smooth/kinetic target. Fika currently has no active smooth-scroller module,
  no animation tick task, and no viewport kinetic state.
- Dolphin scrollbar maximum invalidation and `updateGeometries()` -> viewport
  bounds are owned by GPUI `track_scroll()`. `ViewState` owns the current
  maximum scroll offsets and clamps the current scroll position when layout
  bounds report a different maximum.
- Dolphin `setScrollOffset()` synchronous layout path maps to GPUI
  `ScrollHandle` offset changes and the same offset is written into
  `ViewState.scroll_x` / `ViewState.scroll_y` for visible-item virtualization.
- Dolphin `QScroller` kinetic gesture behavior is not wired in the current
  code. It must stay separate from scrollbar thumb release when rebuilt.
- Zed `SplitEditorView` / `PaneGroup` resize behavior -> splitter drag is
  resolved against the parent row bounds and pane flex allocation. Fika projects
  that allocation into `viewport_width` before building the compact layout, so
  virtualized visible columns do not wait for a later child prepaint pass during
  split resize.

## Implementation Notes

- The previous GPUI app-side smooth-scroll bridge, pane scrollbar drag/cache
  implementation and `item_view_container` rewrite have been removed. There is
  no active `scroll_pane_smooth()`, cached scrollbar track or
  `src/core/scroll.rs` module in the current code.
- Ordinary wheel events compute the Dolphin orientation mapping first
  (compact = horizontal, icons/details = vertical), then call the same
  pane-local scroll offset path used by scrollbar drag. The wheel handler is
  installed on the viewport and item visual rows, so hovering an item does not
  bypass pane scrolling. Scrollbar page press and thumb drag write the view
  offset immediately through the same handle and do not enter smooth scrolling
  or kinetic release. Ctrl/secondary+wheel remains routed to pane-local zoom.
- Directory navigation/back/forward resets `ViewState` scroll to `0,0` in core.
- Zoom/layout changes preserve the current scroll offset by writing the
  view-owned offset back into the `ScrollHandle` until layout bounds settle.
- Viewport width/height are normalized from GPUI's measured pane bounds before
  layout. Fractional widths are rounded down, not up, so the horizontal scrollbar
  cannot become wider than the current pane visible width and then be clipped by
  the slot.
- During split dragging, the pane allocation from the splitter state is used as
  the immediate layout viewport. The measured viewport still reconciles the
  exact GPUI bounds after paint, but it is no longer the first source of truth
  for resize-time virtualization.
- The removed horizontal scrollbar widget used a GPUI canvas, pane-local drag
  state and cached track snapshots. Those files are gone; the current scrollbar
  is a new container component and derives live geometry from the tracked
  viewport `ScrollHandle`.
- Ctrl/secondary+wheel is routed to pane-local zoom, cancels active rubber-band
  selection, and does not update horizontal scroll state.
- Blank press records a pending rubber-band origin, but drawing and selection
  only start after the Dolphin drag-distance threshold is crossed; plain blank
  clicks clear selection without painting a tiny rectangle.
- The model remains unchanged: scrolling only changes view offset and does not
  allocate extra visible items beyond the existing virtualized range.
- Scroll state stays as `f32`; GPUI rendering rounds the translated content
  offset to whole pixels.
