use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ContentMask, Context, CursorStyle, DispatchPhase, Div, Element, ElementId, Entity,
    EntityId, GlobalElementId, Hitbox, HitboxBehavior, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Position,
    ScrollHandle, Size, Stateful, Style, Window, div, fill, point, px, relative, rgb, rgba, size,
};

use crate::FikaApp;

const SCROLLBAR_WIDTH: Pixels = px(6.0);
const SCROLLBAR_PADDING: Pixels = px(4.0);
const MINIMUM_THUMB_SIZE: Pixels = px(25.0);

pub(crate) fn item_view_scrollbar_container(
    pane_id: PaneId,
    scroll_handle: &ScrollHandle,
    viewport: Stateful<Div>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let state = window.use_keyed_state(
        format!("item-view-zed-scrollbar-{}", pane_id.0),
        cx,
        |_, cx| ItemViewScrollbarState::new(scroll_handle.clone(), cx.entity_id()),
    );
    state.update(cx, |state, _cx| {
        state.scroll_handle = scroll_handle.clone();
    });

    div()
        .id(format!("item-view-scroll-wrapper-{}", pane_id.0))
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(
            viewport
                .relative()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .size_full()
                .track_scroll(scroll_handle)
                .overflow_x_scroll(),
        )
        .child(ItemViewScrollbarElement { state })
}

struct ItemViewScrollbarState {
    scroll_handle: ScrollHandle,
    notify_id: EntityId,
    thumb_state: ThumbState,
    last_prepaint_state: Option<ScrollbarPrepaintState>,
}

impl ItemViewScrollbarState {
    fn new(scroll_handle: ScrollHandle, notify_id: EntityId) -> Self {
        Self {
            scroll_handle,
            notify_id,
            thumb_state: ThumbState::Inactive,
            last_prepaint_state: None,
        }
    }

    fn set_offset(&self, offset_x: Pixels, cx: &mut App) {
        let current = self.scroll_handle.offset();
        self.scroll_handle.set_offset(point(offset_x, current.y));
        cx.notify(self.notify_id);
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
        let viewport_size = self.track_bounds.size.width;
        let thumb_size = self.thumb_bounds.size.width;
        let thumb_offset = match event_type {
            ScrollbarMouseEvent::TrackClick => thumb_size / 2.0,
            ScrollbarMouseEvent::ThumbDrag(thumb_offset) => thumb_offset,
        };
        let thumb_start = (event_position.x - self.track_bounds.origin.x - thumb_offset)
            .clamp(px(0.0), viewport_size - thumb_size);
        let percentage = if viewport_size > thumb_size {
            thumb_start / (viewport_size - thumb_size)
        } else {
            0.0
        };

        -max_offset.x * percentage
    }
}

enum ScrollbarMouseEvent {
    TrackClick,
    ThumbDrag(Pixels),
}

struct ItemViewScrollbarElement {
    state: Entity<ItemViewScrollbarState>,
}

impl Element for ItemViewScrollbarElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<ScrollbarPrepaintState>;

    fn id(&self) -> Option<ElementId> {
        Some(("item-view-zed-scrollbar-element", self.state.entity_id()).into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            position: Position::Absolute,
            inset: Default::default(),
            size: size(relative(1.0), relative(1.0)).map(Into::into),
            ..Default::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.state.read(cx);
        let max_offset = state.scroll_handle.max_offset();
        let viewport_bounds = state.scroll_handle.bounds();
        let visible_bounds = if viewport_bounds.size.width > Pixels::ZERO
            && viewport_bounds.size.height > Pixels::ZERO
        {
            viewport_bounds
        } else {
            bounds
        };
        let viewport_width = visible_bounds.size.width;
        let viewport_height = visible_bounds.size.height;

        if max_offset.x <= Pixels::ZERO
            || viewport_width <= Pixels::ZERO
            || viewport_height <= Pixels::ZERO
        {
            return None;
        }

        let content_width = viewport_width + max_offset.x;
        let visible_percentage = viewport_width / content_width;
        let thumb_width = MINIMUM_THUMB_SIZE.max(viewport_width * visible_percentage);
        if thumb_width > viewport_width {
            return None;
        }

        let current_offset = state
            .scroll_handle
            .offset()
            .x
            .clamp(-max_offset.x, Pixels::ZERO)
            .abs();
        let thumb_start = (current_offset / max_offset.x) * (viewport_width - thumb_width);
        let track_height = SCROLLBAR_WIDTH + 2.0 * SCROLLBAR_PADDING;
        let track_bounds = Bounds::new(
            point(
                visible_bounds.origin.x,
                visible_bounds.origin.y + (viewport_height - track_height).max(Pixels::ZERO),
            ),
            size(viewport_width, track_height.min(viewport_height)),
        );
        let thumb_track = inset_bounds(track_bounds, SCROLLBAR_PADDING);
        let thumb_bounds = Bounds::new(
            point(thumb_track.origin.x + thumb_start, thumb_track.origin.y),
            size(
                thumb_width.min(thumb_track.size.width),
                thumb_track.size.height,
            ),
        );
        let cursor_hitbox =
            window.insert_hitbox(thumb_track, HitboxBehavior::BlockMouseExceptScroll);

        Some(ScrollbarPrepaintState {
            thumb: Some(ScrollbarLayout {
                thumb_bounds,
                track_bounds: thumb_track,
                cursor_hitbox,
            }),
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint_state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(prepaint_state) = prepaint_state.take() else {
            self.state
                .update(cx, |state, _cx| state.last_prepaint_state = None);
            return;
        };
        let Some(layout) = prepaint_state.thumb.as_ref() else {
            self.state.update(cx, |state, _cx| {
                state.last_prepaint_state = Some(prepaint_state)
            });
            return;
        };

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(fill(layout.track_bounds, rgba(0x00000000)));
            let thumb_color = match self.state.read(cx).thumb_state {
                ThumbState::Dragging(_) => rgb(0x4f5f73),
                ThumbState::Hover => rgb(0x68778b),
                ThumbState::Inactive => rgb(0x8792a2),
            };
            window.paint_quad(fill(layout.thumb_bounds, thumb_color).corner_radii(Pixels::MAX));

            if self.state.read(cx).thumb_state.is_dragging() {
                window.set_window_cursor_style(CursorStyle::Arrow);
            } else {
                window.set_cursor_style(CursorStyle::Arrow, &layout.cursor_hitbox);
            }
        });

        self.state.update(cx, |state, _cx| {
            state.last_prepaint_state = Some(prepaint_state)
        });

        let capture_phase = if self.state.read(cx).thumb_state.is_dragging() {
            DispatchPhase::Capture
        } else {
            DispatchPhase::Bubble
        };

        window.on_mouse_event({
            let state = self.state.clone();
            move |event: &MouseDownEvent, phase, _window, cx| {
                state.update(cx, |state, cx| {
                    let Some(scrollbar_layout) = (phase == capture_phase
                        && event.button == MouseButton::Left)
                        .then(|| state.hit_for_position(&event.position))
                        .flatten()
                    else {
                        return;
                    };

                    if scrollbar_layout.thumb_bounds.contains(&event.position) {
                        let offset = event.position.x - scrollbar_layout.thumb_bounds.origin.x;
                        state.set_thumb_state(ThumbState::Dragging(offset), cx);
                    } else {
                        let click_offset = scrollbar_layout.compute_click_offset(
                            event.position,
                            state.scroll_handle.max_offset(),
                            ScrollbarMouseEvent::TrackClick,
                        );
                        state.set_offset(click_offset, cx);
                    }
                    cx.stop_propagation();
                });
            }
        });

        window.on_mouse_event({
            let state = self.state.clone();
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
                            state.update(cx, |state, cx| state.set_offset(drag_offset, cx));
                            cx.stop_propagation();
                        }
                    }
                    _ => {
                        let next_state =
                            if state.read(cx).thumb_for_position(&event.position).is_some() {
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
            let state = self.state.clone();
            move |event: &MouseUpEvent, phase, _window, cx| {
                if phase != capture_phase {
                    return;
                }

                let next_state = if state.read(cx).thumb_for_position(&event.position).is_some() {
                    ThumbState::Hover
                } else {
                    ThumbState::Inactive
                };
                state.update(cx, |state, cx| state.set_thumb_state(next_state, cx));
            }
        });
    }
}

impl IntoElement for ItemViewScrollbarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
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
