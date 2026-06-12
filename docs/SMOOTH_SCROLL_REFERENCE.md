# Smooth Scroll Reference

This document records Dolphin's item-list smooth scrolling model and how Fika
maps it. Fika keeps the Dolphin-style core scroll math in `src/core/scroll.rs`,
but the GPUI UI path currently disables smooth/kinetic animation while the
basic mouse-wheel and scrollbar hitbox behavior is being stabilized.

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
  clears pane-local scroll animation state when viewport/content bounds change.
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
- Dolphin interrupted animation handling remains encoded in
  `SmoothScroll::scroll_contents_by()`, which carries the old target forward and
  advances the new start by Dolphin's exact
  `distance/currentOffset/oldEndOffset/endOffset/startOffset` sequence from
  `KItemListSmoothScroller::scrollContentsBy()`.
- Dolphin fresh/retarget easing remains available in core as `InOutQuad` for
  new wheel animations and `OutQuad` for retargeted wheel animations.
- Dolphin `QScroller` kinetic gesture path is represented in core by
  `ScrollDragTracker` plus kinetic `SmoothScroll`, but the UI drag-release path
  currently clears the tracker instead of starting inertia.
- Dolphin `setScrollOffset()` synchronous layout path maps to Fika's current UI
  behavior: wheel events and scrollbar drags write `ViewState.scroll_x/scroll_y`
  immediately, so `compact_layout_for_model()` and visible-item virtualization
  are recalculated from the current offset without waiting for animation ticks.
- Zed `SplitEditorView` / `PaneGroup` resize behavior -> splitter drag is
  resolved against the parent row bounds and pane flex allocation. Fika projects
  that allocation into `viewport_width` before building the compact layout, so
  virtualized visible columns and the horizontal scrollbar do not wait for a
  later child prepaint pass during split resize.

## Implementation Notes

- Smooth scroll state remains pane-local when present; split panes never share
  animation state.
- Current UI isolation mode disables smooth and kinetic animation: ordinary
  wheel events call `scroll_pane_smooth()`, but that function now clamps and
  writes the new offset immediately; scrollbar drag release calls
  `finish_scrollbar_drag()` only to clear drag tracking and stale smooth state.
- Directory navigation/back/forward resets `ViewState` scroll to `0,0` in core.
- Directory switching, pane close, zoom changes and viewport bound changes clear
  smooth scroll state and scrollbar drag trackers.
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
- The scrollbar slot and scrollbar widget are GPUI mouse occlusion hitboxes.
  Starting a scrollbar drag cancels any rubber-band selection, and the low-level
  scrollbar mouse down/move/up listeners stop propagation so the item viewport
  below cannot start selection or hover work "through" the scrollbar.
  Scrollbar left-button down is handled during GPUI capture phase, matching the
  intent of Zed's own scrollbar component, so `occlude()` and bubble-phase
  blockers cannot prevent a drag from starting.
  The scrollbar reserve child prepaint and scrollbar canvas both cache the
  measured track origin and width into pane-local UI state. The scrollbar
  container can therefore start a drag from a normal left-button down using
  window coordinates even if the canvas-level listener is bypassed.
  The reserve slot and the scrollbar widget consume left-button down even when
  the cached track bounds are temporarily unavailable, so the file viewport
  cannot receive a selection, rubber-band, or item press through the scrollbar.
  Active drag movement is handled from capture-phase window mouse move/up
  events and no longer depends on `MouseMoveEvent::dragging()`, because some
  platform paths do not keep `pressed_button` populated for every move event.
  The track/reserve must not use a scroll-only mouse blocker: wheel events still
  route through the scrollbar, but left-button drag needs normal mouse hit
  testing so the canvas-level down/move/up listeners can start and update the
  active scrollbar drag.
  While a scrollbar drag is active, pane snapshots render a full-pane mouse
  capture layer that forwards window-coordinate move/up events back through the
  cached track bounds. Dragging no longer depends on the pointer staying inside
  the 12px scrollbar strip after the initial press.
- Ordinary wheel events enter the pane-local scroll path and write the offset
  immediately. Ctrl/secondary+wheel is routed to pane-local zoom instead,
  cancels active rubber-band selection, and does not update horizontal scroll
  state.
- The model remains unchanged: scrolling only changes view offset and does not
  allocate extra visible items beyond the existing virtualized range.
- Scroll state stays as `f32`; GPUI rendering rounds the translated content
  offset to whole pixels.
