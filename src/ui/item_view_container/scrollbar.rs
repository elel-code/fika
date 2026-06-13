use crate::FikaApp;
use fika_core::{CompactLayout, PaneId, ViewRect, ViewState};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, CursorStyle, Div, Hitbox, HitboxBehavior, MouseButton, NavigationDirection,
    ParentElement, Pixels, Stateful, Styled, canvas, div, fill, point, px, rgb, rgba, size,
};

use super::scroll_offset::{
    ItemViewScrollBarEvent, ItemViewScrollOffsetBar, handle_item_view_container_wheel,
};

pub(crate) const ITEM_VIEW_SCROLLBAR_EXTENT: f32 = 12.0;

pub(crate) fn horizontal_scrollbar(
    pane_id: PaneId,
    layout: CompactLayout,
    view: ViewState,
    bar: ItemViewScrollOffsetBar,
    mouse_overlay_active: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("item-view-scrollbar-x-{}", pane_id.0))
        .h(px(ITEM_VIEW_SCROLLBAR_EXTENT))
        .w_full()
        .max_w_full()
        .min_w_0()
        .flex_none()
        .overflow_hidden()
        .occlude()
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.panes.focus(pane_id);
                this.go_back(pane_id);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.panes.focus(pane_id);
                this.go_forward(pane_id);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_scroll_wheel(
            cx.listener(move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                handle_item_view_container_wheel(this, pane_id, event, window, &layout, &view, cx);
            }),
        )
        .child(scrollbar_canvas(pane_id, bar, mouse_overlay_active, cx))
}

fn scrollbar_canvas(
    pane_id: PaneId,
    bar: ItemViewScrollOffsetBar,
    mouse_overlay_active: bool,
    cx: &mut Context<FikaApp>,
) -> impl IntoElement {
    let app = cx.weak_entity();
    canvas(
        move |bounds, window, _cx| {
            let track_window_rect = track_window_rect_from_bounds(bounds);
            let measured_bar = bar.remeasured(bounds.size.width.as_f32());
            ScrollbarPaintState {
                bar: measured_bar,
                track_window_rect,
                hitbox: window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
            }
        },
        move |bounds, state, window, cx| {
            if let Some(bar) = state.bar {
                if let Some(track) =
                    bar.track(bounds.size.width.as_f32(), bounds.size.height.as_f32())
                {
                    paint_scrollbar(bounds, track.thumb_rect, window);
                }
                register_scrollbar_handlers(
                    pane_id,
                    bar,
                    state.track_window_rect,
                    state.hitbox.clone(),
                    mouse_overlay_active,
                    app.clone(),
                    window,
                    cx,
                );
            }
        },
    )
    .size_full()
}

fn register_scrollbar_handlers(
    pane_id: PaneId,
    bar: ItemViewScrollOffsetBar,
    track_window_rect: ViewRect,
    hitbox: Hitbox,
    mouse_overlay_active: bool,
    app: gpui::WeakEntity<FikaApp>,
    window: &mut gpui::Window,
    app_cx: &mut gpui::App,
) {
    let app_for_down = app.clone();
    let hitbox_for_down = hitbox.clone();
    window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
        if !phase.capture() || mouse_overlay_active || event.button != MouseButton::Left {
            return;
        }

        let result = app_for_down
            .update(cx, |this, cx| {
                let result = this.begin_item_view_scrollbar_press(
                    pane_id,
                    bar,
                    track_window_rect,
                    event.position,
                );
                if result.handled {
                    this.panes.focus(pane_id);
                }
                if result.changed {
                    cx.notify();
                }
                result
            })
            .unwrap_or(ItemViewScrollBarEvent::IGNORED);
        if result.handled {
            if result.dragging {
                window.capture_pointer(hitbox_for_down.id);
            }
            window.prevent_default();
            cx.stop_propagation();
        }
    });

    let app_for_move = app.clone();
    let hitbox_for_move = hitbox.clone();
    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
        if !phase.capture() || !event.dragging() {
            return;
        }

        let result = app_for_move
            .update(cx, |this, cx| {
                let result = this.update_item_view_scrollbar_drag(pane_id, event.position);
                if result.changed {
                    cx.notify();
                }
                result
            })
            .unwrap_or(ItemViewScrollBarEvent::IGNORED);
        if result.handled {
            window.capture_pointer(hitbox_for_move.id);
            window.prevent_default();
            cx.stop_propagation();
        }
    });

    let app_for_up = app.clone();
    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
        if !phase.capture() || event.button != MouseButton::Left {
            return;
        }

        let handled = app_for_up
            .update(cx, |this, cx| {
                let handled = this.finish_item_view_scrollbar_drag(pane_id);
                if handled {
                    cx.notify();
                }
                handled
            })
            .unwrap_or(false);

        if handled {
            window.release_pointer();
            window.prevent_default();
            cx.stop_propagation();
        }
    });

    window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
    if app
        .read_with(app_cx, |this, _cx| {
            this.item_view_container_scroll_drag
                .is_some_and(|drag| drag.pane_id == pane_id)
        })
        .unwrap_or(false)
    {
        window.set_window_cursor_style(CursorStyle::PointingHand);
    }
}

fn paint_scrollbar(bounds: Bounds<Pixels>, thumb_rect: ViewRect, window: &mut gpui::Window) {
    window.paint_quad(fill(bounds, rgba(0x00000000)).corner_radii(px(0.0)));
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(thumb_rect.x), bounds.origin.y),
                size(px(thumb_rect.width), px(thumb_rect.height)),
            ),
            rgb(0x7a8494),
        )
        .corner_radii(px((thumb_rect.height / 2.0).max(1.0))),
    );
}

fn track_window_rect_from_bounds(bounds: Bounds<Pixels>) -> ViewRect {
    ViewRect {
        x: bounds.origin.x.as_f32(),
        y: bounds.origin.y.as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
    }
}

struct ScrollbarPaintState {
    bar: Option<ItemViewScrollOffsetBar>,
    track_window_rect: ViewRect,
    hitbox: Hitbox,
}
