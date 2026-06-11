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

- Dolphin `KItemListSmoothScroller` -> `src/core/scroll.rs::SmoothScroll`.
- Dolphin scrollbar maximum invalidation -> `FikaApp::set_pane_viewport_bounds()`
  clears pane-local smooth scroll state when viewport/content bounds change.
- Dolphin `KItemListContainer::updateGeometries()` keeps the item view geometry
  separate from the scrollbar extent. Fika mirrors this by rendering the file
  content viewport and the horizontal scrollbar as sibling slots: the item
  viewport clips only items, while the scrollbar slot is outside that clipping
  subtree.
- Dolphin scrollbar geometry stays constrained to the current pane's visible
  item-view area. Fika measures the pane content viewport, normalizes the
  extent so it never exceeds the measured width, and feeds that same width to
  `ViewState`, `CompactLayout::horizontal_scroll_bar()`, max-scroll clamping and
  drag mapping.
- Dolphin interrupted animation handling -> `SmoothScroll::scroll_contents_by()`
  carries the old target forward and advances the new start by Dolphin's exact
  `distance/currentOffset/oldEndOffset/endOffset/startOffset` sequence from
  `KItemListSmoothScroller::scrollContentsBy()`.
- Dolphin fresh/retarget easing -> `InOutQuad` for new wheel animations and
  `OutQuad` for retargeted wheel animations.
- Dolphin `QScroller` kinetic gesture path -> Fika samples scrollbar drag
  velocity with `ScrollDragTracker` and starts a pane-local kinetic `SmoothScroll`
  on drag release.
- Dolphin `setScrollOffset()` synchronous layout path -> Fika's animation tick
  writes `ViewState.scroll_x/scroll_y`, so `compact_layout_for_model()` and
  visible-item virtualization are recalculated from the current animated
  offset, not from a full-model render path.
- Zed `SplitEditorView` / `PaneGroup` resize behavior -> splitter drag is
  resolved against the parent row bounds and pane flex allocation. Fika projects
  that allocation into `viewport_width` before building the compact layout, so
  virtualized visible columns and the horizontal scrollbar do not wait for a
  later child prepaint pass during split resize.

## Implementation Notes

- Smooth scroll state is stored per `PaneId`; split panes never share animation
  state.
- Directory navigation/back/forward resets `ViewState` scroll to `0,0` in core.
- Directory switching, pane close, zoom changes and viewport bound changes clear
  smooth scroll and scrollbar drag trackers.
- Viewport width/height are normalized from GPUI's measured pane bounds before
  layout. Fractional widths are rounded down, not up, so the horizontal scrollbar
  cannot become wider than the current pane visible width and then be clipped by
  the slot.
- During split dragging, the pane allocation from the splitter state is used as
  the immediate layout viewport. The measured viewport still reconciles the
  exact GPUI bounds after paint, but it is no longer the first source of truth
  for resize-time virtualization.
- The horizontal scrollbar widget fills the pane shell (`w_full + min_w_0`) and
  reads its actual GPUI bounds for drag mapping. It does not rely on clipping an
  oversized flex child; the rendered control's layout width is the same visible
  pane area used for scrollbar math.
- The model remains unchanged: smooth scrolling only changes view offset and does
  not allocate extra visible items beyond the existing virtualized range.
- Scroll state stays as `f32`; GPUI rendering rounds the translated content
  offset to whole pixels.
