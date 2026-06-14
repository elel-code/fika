use fika_core::{PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, CursorStyle, DispatchPhase, Div, Entity, EntityId, Hitbox,
    HitboxBehavior, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement,
    Pixels, Point, ScrollHandle, Size, Stateful, Window, canvas, div, fill, point, px, rgb, rgba,
    size,
};

use crate::FikaApp;

const SCROLLBAR_PADDING: Pixels = px(4.0);
const MINIMUM_THUMB_SIZE: Pixels = px(25.0);
pub(crate) const ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT: f32 = 14.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemViewScrollbarAxis {
    Horizontal,
    Vertical,
}

impl ItemViewScrollbarAxis {
    fn point_axis(self, point: Point<Pixels>) -> Pixels {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }

    fn bounds_axis_size(self, bounds: Bounds<Pixels>) -> Pixels {
        match self {
            Self::Horizontal => bounds.size.width,
            Self::Vertical => bounds.size.height,
        }
    }

    fn bounds_cross_size(self, bounds: Bounds<Pixels>) -> Pixels {
        match self {
            Self::Horizontal => bounds.size.height,
            Self::Vertical => bounds.size.width,
        }
    }

    fn thumb_bounds(
        self,
        track_bounds: Bounds<Pixels>,
        thumb_start: Pixels,
        thumb_size: Pixels,
    ) -> Bounds<Pixels> {
        match self {
            Self::Horizontal => Bounds::new(
                point(track_bounds.origin.x + thumb_start, track_bounds.origin.y),
                size(
                    thumb_size.min(track_bounds.size.width),
                    track_bounds.size.height,
                ),
            ),
            Self::Vertical => Bounds::new(
                point(track_bounds.origin.x, track_bounds.origin.y + thumb_start),
                size(
                    track_bounds.size.width,
                    thumb_size.min(track_bounds.size.height),
                ),
            ),
        }
    }
}

pub(crate) fn item_view_scrollbar_container(
    pane_id: PaneId,
    scroll_handle: &ScrollHandle,
    axis: ItemViewScrollbarAxis,
    rubber_band: Option<ViewRect>,
    viewport: Stateful<Div>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let app = cx.weak_entity();
    let state = window.use_keyed_state(
        format!("item-view-zed-scrollbar-{}", pane_id.0),
        cx,
        |_, cx| {
            ItemViewScrollbarState::new(scroll_handle.clone(), cx.entity_id(), pane_id, app.clone())
        },
    );
    state.update(cx, |state, _cx| {
        state.scroll_handle = scroll_handle.clone();
        state.pane_id = pane_id;
        state.app = app.clone();
        state.axis = axis;
    });

    let viewport = viewport
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .track_scroll(scroll_handle)
        .overflow_x_scroll()
        .overflow_y_scroll()
        .when_some(
            rubber_band.filter(|rect| rubber_band_rect_is_visible(*rect)),
            |viewport, rect| viewport.child(rubber_band_overlay(rect)),
        );

    let wrapper = div()
        .id(format!("item-view-scroll-wrapper-{}", pane_id.0))
        .relative()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_hidden();

    match axis {
        ItemViewScrollbarAxis::Horizontal => wrapper
            .flex_col()
            .child(viewport)
            .child(item_view_scrollbar(state, axis)),
        ItemViewScrollbarAxis::Vertical => wrapper
            .flex_row()
            .child(viewport)
            .child(item_view_scrollbar(state, axis)),
    }
}

fn rubber_band_rect_is_visible(rect: ViewRect) -> bool {
    rect.width >= 2.0 && rect.height >= 2.0
}

struct ItemViewScrollbarState {
    scroll_handle: ScrollHandle,
    notify_id: EntityId,
    pane_id: PaneId,
    app: gpui::WeakEntity<FikaApp>,
    axis: ItemViewScrollbarAxis,
    thumb_state: ThumbState,
    last_prepaint_state: Option<ScrollbarPrepaintState>,
}

impl ItemViewScrollbarState {
    fn new(
        scroll_handle: ScrollHandle,
        notify_id: EntityId,
        pane_id: PaneId,
        app: gpui::WeakEntity<FikaApp>,
    ) -> Self {
        Self {
            scroll_handle,
            notify_id,
            pane_id,
            app,
            axis: ItemViewScrollbarAxis::Horizontal,
            thumb_state: ThumbState::Inactive,
            last_prepaint_state: None,
        }
    }

    fn set_axis_offset(&self, offset: Pixels, cx: &mut App) {
        let current = self.scroll_handle.offset();
        let offset = match self.axis {
            ItemViewScrollbarAxis::Horizontal => point(offset, current.y),
            ItemViewScrollbarAxis::Vertical => point(current.x, offset),
        };
        self.scroll_handle.set_offset(offset);
        let _ = self.app.update(cx, |this, cx| {
            if this.update_item_view_scrollbar_drag(self.pane_id) {
                cx.notify();
            }
        });
        cx.notify(self.notify_id);
    }

    fn begin_drag(&self, cx: &mut App) {
        let _ = self.app.update(cx, |this, cx| {
            if this.begin_item_view_scrollbar_drag(self.pane_id) {
                cx.notify();
            }
        });
    }

    fn finish_drag(&self, cx: &mut App) {
        let _ = self.app.update(cx, |this, cx| {
            if this.finish_item_view_scrollbar_drag(self.pane_id) {
                cx.notify();
            }
        });
    }

    fn set_thumb_state(&mut self, state: ThumbState, cx: &mut App) {
        if self.thumb_state != state {
            self.thumb_state = state;
            cx.notify(self.notify_id);
        }
    }

    fn hit_for_position(&self, position: &Point<Pixels>) -> Option<&ScrollbarLayout> {
        self.last_prepaint_state
            .as_ref()
            .and_then(|state| state.track_hit_for_position(position))
    }

    fn thumb_for_position(&self, position: &Point<Pixels>) -> Option<&ScrollbarLayout> {
        self.last_prepaint_state
            .as_ref()
            .and_then(|state| state.thumb_for_position(position))
    }

    fn thumb_layout(&self) -> Option<&ScrollbarLayout> {
        self.last_prepaint_state
            .as_ref()
            .and_then(|state| state.thumb.as_ref())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ThumbState {
    Inactive,
    Hover,
    Dragging(Pixels),
}

impl ThumbState {
    fn is_dragging(self) -> bool {
        matches!(self, ThumbState::Dragging(_))
    }
}

struct ScrollbarPrepaintState {
    thumb: Option<ScrollbarLayout>,
}

impl ScrollbarPrepaintState {
    fn track_hit_for_position(&self, position: &Point<Pixels>) -> Option<&ScrollbarLayout> {
        self.thumb
            .as_ref()
            .filter(|layout| layout.track_bounds.contains(position))
    }

    fn thumb_for_position(&self, position: &Point<Pixels>) -> Option<&ScrollbarLayout> {
        self.thumb
            .as_ref()
            .filter(|layout| layout.thumb_bounds.contains(position))
    }
}

struct ScrollbarLayout {
    axis: ItemViewScrollbarAxis,
    thumb_bounds: Bounds<Pixels>,
    track_bounds: Bounds<Pixels>,
    cursor_hitbox: Hitbox,
}

impl ScrollbarLayout {
    fn compute_click_offset(
        &self,
        event_position: Point<Pixels>,
        max_offset: Point<Pixels>,
        event_type: ScrollbarMouseEvent,
    ) -> Pixels {
        let viewport_size = self.axis.bounds_axis_size(self.track_bounds);
        let thumb_size = self.axis.bounds_axis_size(self.thumb_bounds);
        let thumb_offset = match event_type {
            ScrollbarMouseEvent::TrackClick => thumb_size / 2.0,
            ScrollbarMouseEvent::ThumbDrag(thumb_offset) => thumb_offset,
        };
        let thumb_start = (self.axis.point_axis(event_position)
            - self.axis.point_axis(self.track_bounds.origin)
            - thumb_offset)
            .clamp(px(0.0), viewport_size - thumb_size);
        let percentage = if viewport_size > thumb_size {
            thumb_start / (viewport_size - thumb_size)
        } else {
            0.0
        };

        -self.axis.point_axis(max_offset) * percentage
    }
}

enum ScrollbarMouseEvent {
    TrackClick,
    ThumbDrag(Pixels),
}

struct ItemViewScrollbarPaintState {
    layout: Option<ScrollbarLayout>,
}

fn item_view_scrollbar(state: Entity<ItemViewScrollbarState>, axis: ItemViewScrollbarAxis) -> Div {
    let scrollbar = div().relative().flex_none().overflow_hidden().child(
        canvas(
            {
                let state = state.clone();
                move |bounds, window, cx| {
                    let layout = item_view_scrollbar_layout(&state, bounds, window, cx);
                    ItemViewScrollbarPaintState { layout }
                }
            },
            move |bounds, paint_state, window, cx| {
                paint_item_view_scrollbar(bounds, paint_state, state, window, cx);
            },
        )
        .size_full(),
    );

    match axis {
        ItemViewScrollbarAxis::Horizontal => scrollbar
            .w_full()
            .h(px(ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT)),
        ItemViewScrollbarAxis::Vertical => scrollbar
            .w(px(ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT))
            .h_full(),
    }
}

fn item_view_scrollbar_layout(
    state: &Entity<ItemViewScrollbarState>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> Option<ScrollbarLayout> {
    let state = state.read(cx);
    let axis = state.axis;
    let max_offset = state.scroll_handle.max_offset();
    let viewport_bounds = state.scroll_handle.bounds();
    let visible_bounds = if viewport_bounds.size.width > Pixels::ZERO
        && viewport_bounds.size.height > Pixels::ZERO
    {
        viewport_bounds
    } else {
        bounds
    };
    let viewport_axis_size = axis.bounds_axis_size(visible_bounds);
    let max_axis_offset = axis.point_axis(max_offset);
    let thumb_track = inset_bounds(bounds, SCROLLBAR_PADDING);
    let track_axis_size = axis.bounds_axis_size(thumb_track);

    if max_axis_offset <= Pixels::ZERO
        || viewport_axis_size <= Pixels::ZERO
        || track_axis_size <= Pixels::ZERO
        || axis.bounds_cross_size(thumb_track) <= Pixels::ZERO
    {
        return None;
    }

    let content_axis_size = viewport_axis_size + max_axis_offset;
    let visible_percentage = viewport_axis_size / content_axis_size;
    let thumb_axis_size = MINIMUM_THUMB_SIZE.max(track_axis_size * visible_percentage);
    if thumb_axis_size > track_axis_size {
        return None;
    }

    let current_offset = axis
        .point_axis(state.scroll_handle.offset())
        .clamp(-max_axis_offset, Pixels::ZERO)
        .abs();
    let thumb_start = (current_offset / max_axis_offset) * (track_axis_size - thumb_axis_size);
    let thumb_bounds = axis.thumb_bounds(thumb_track, thumb_start, thumb_axis_size);
    let cursor_hitbox = window.insert_hitbox(thumb_track, HitboxBehavior::BlockMouseExceptScroll);

    Some(ScrollbarLayout {
        axis,
        thumb_bounds,
        track_bounds: thumb_track,
        cursor_hitbox,
    })
}

fn paint_item_view_scrollbar(
    _bounds: Bounds<Pixels>,
    paint_state: ItemViewScrollbarPaintState,
    state: Entity<ItemViewScrollbarState>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(layout) = paint_state.layout else {
        state.update(cx, |state, _cx| state.last_prepaint_state = None);
        return;
    };

    window.paint_quad(fill(layout.track_bounds, rgba(0xe1e6ee66)).corner_radii(px(3.0)));
    let thumb_color = match state.read(cx).thumb_state {
        ThumbState::Dragging(_) => rgb(0x4f5f73),
        ThumbState::Hover => rgb(0x68778b),
        ThumbState::Inactive => rgb(0x8792a2),
    };
    window.paint_quad(fill(layout.thumb_bounds, thumb_color).corner_radii(px(3.0)));

    if state.read(cx).thumb_state.is_dragging() {
        window.set_window_cursor_style(CursorStyle::Arrow);
    } else {
        window.set_cursor_style(CursorStyle::Arrow, &layout.cursor_hitbox);
    }

    state.update(cx, |state, _cx| {
        state.last_prepaint_state = Some(ScrollbarPrepaintState {
            thumb: Some(layout),
        })
    });

    let capture_phase = if state.read(cx).thumb_state.is_dragging() {
        DispatchPhase::Capture
    } else {
        DispatchPhase::Bubble
    };

    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseDownEvent, phase, window, cx| {
            state.update(cx, |state, cx| {
                let Some(scrollbar_layout) = (phase == capture_phase
                    && event.button == MouseButton::Left)
                    .then(|| state.hit_for_position(&event.position))
                    .flatten()
                else {
                    return;
                };

                if scrollbar_layout.thumb_bounds.contains(&event.position) {
                    let offset = scrollbar_layout.axis.point_axis(event.position)
                        - scrollbar_layout
                            .axis
                            .point_axis(scrollbar_layout.thumb_bounds.origin);
                    window.capture_pointer(scrollbar_layout.cursor_hitbox.id);
                    state.begin_drag(cx);
                    state.set_thumb_state(ThumbState::Dragging(offset), cx);
                } else {
                    let click_offset = scrollbar_layout.compute_click_offset(
                        event.position,
                        state.scroll_handle.max_offset(),
                        ScrollbarMouseEvent::TrackClick,
                    );
                    state.set_axis_offset(click_offset, cx);
                }
                cx.stop_propagation();
            });
        }
    });

    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase != capture_phase {
                return;
            }

            let thumb_state = state.read(cx).thumb_state;
            match thumb_state {
                ThumbState::Dragging(drag_offset) if event.dragging() => {
                    if let Some(scrollbar_layout) = state.read(cx).thumb_layout() {
                        let drag_offset = scrollbar_layout.compute_click_offset(
                            event.position,
                            state.read(cx).scroll_handle.max_offset(),
                            ScrollbarMouseEvent::ThumbDrag(drag_offset),
                        );
                        state.update(cx, |state, cx| state.set_axis_offset(drag_offset, cx));
                        cx.stop_propagation();
                    }
                }
                _ => {
                    let next_state = if state.read(cx).thumb_for_position(&event.position).is_some()
                    {
                        ThumbState::Hover
                    } else {
                        ThumbState::Inactive
                    };
                    state.update(cx, |state, cx| state.set_thumb_state(next_state, cx));
                }
            }
        }
    });

    window.on_mouse_event({
        let state = state.clone();
        move |event: &MouseUpEvent, phase, window, cx| {
            if phase != capture_phase {
                return;
            }

            let next_state = if state.read(cx).thumb_for_position(&event.position).is_some() {
                ThumbState::Hover
            } else {
                ThumbState::Inactive
            };
            state.update(cx, |state, cx| {
                let was_dragging = state.thumb_state.is_dragging();
                state.set_thumb_state(next_state, cx);
                if was_dragging {
                    state.finish_drag(cx);
                }
            });
            window.release_pointer();
        }
    });
}

fn inset_bounds(bounds: Bounds<Pixels>, inset: Pixels) -> Bounds<Pixels> {
    Bounds::new(
        point(bounds.origin.x + inset, bounds.origin.y + inset),
        Size {
            width: (bounds.size.width - 2.0 * inset).max(Pixels::ZERO),
            height: (bounds.size.height - 2.0 * inset).max(Pixels::ZERO),
        },
    )
}

fn rubber_band_overlay(rect: ViewRect) -> Stateful<Div> {
    div()
        .id("rubber-band")
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .h(px(rect.height))
        .border_1()
        .rounded_sm()
        .border_color(rgb(0x2563eb))
        .bg(rgba(0x2563eb26))
}
