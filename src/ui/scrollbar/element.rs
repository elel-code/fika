use crate::FikaApp;
use fika_core::{HorizontalScrollBarLayout, PaneId};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, Hitbox, HitboxBehavior, MouseButton, ParentElement, Pixels, Stateful,
    Styled, canvas, div, fill, point, px, rgb, size,
};

use super::geometry::{
    SCROLLBAR_THICKNESS, scroll_bar_layout_for_bounds, scrollbar_window_rect_from_bounds,
};
use fika_core::ViewRect;

pub(crate) fn horizontal_scroll_bar(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
    mouse_overlay_active: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("scrollbar-x-{}", pane_id.0))
        .relative()
        .w_full()
        .max_w_full()
        .min_w_0()
        .flex_shrink_1()
        .overflow_hidden()
        .h(px(SCROLLBAR_THICKNESS))
        .bg(rgb(0xe6e9ef))
        .occlude()
        .cursor_pointer()
        .child(scroll_bar_canvas(
            pane_id,
            content_width,
            scroll_x,
            mouse_overlay_active,
            cx,
        ))
}

fn scroll_bar_canvas(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
    mouse_overlay_active: bool,
    cx: &mut Context<FikaApp>,
) -> impl IntoElement {
    let app_for_prepaint = cx.weak_entity();
    let app_for_paint = cx.weak_entity();
    canvas(
        move |bounds, window, cx| {
            let track_window_rect = scrollbar_window_rect_from_bounds(bounds);
            let _ = app_for_prepaint.update(cx, |this, _cx| {
                this.set_horizontal_scrollbar_track(
                    pane_id,
                    track_window_rect,
                    content_width,
                    scroll_x,
                );
            });
            ScrollBarCanvasState {
                bar: scroll_bar_layout_for_bounds(content_width, scroll_x, bounds),
                track_window_rect,
                _hitbox: window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
            }
        },
        move |bounds, state, window, _cx| {
            let Some(bar) = state.bar else {
                return;
            };
            paint_scrollbar_handle(bounds, bar, window);
            register_scrollbar_mouse_handlers(
                pane_id,
                content_width,
                scroll_x,
                mouse_overlay_active,
                state.track_window_rect,
                state._hitbox.clone(),
                app_for_paint.clone(),
                window,
            );
        },
    )
    .size_full()
}

fn register_scrollbar_mouse_handlers(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
    mouse_overlay_active: bool,
    track_window_rect: ViewRect,
    hitbox: Hitbox,
    app: gpui::WeakEntity<FikaApp>,
    window: &mut gpui::Window,
) {
    let hitbox_for_down = hitbox.clone();
    let app_for_down = app.clone();
    window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
        if !phase.capture()
            || mouse_overlay_active
            || event.button != MouseButton::Left
            || !track_window_rect.contains(fika_core::ViewPoint {
                x: event.position.x.as_f32(),
                y: event.position.y.as_f32(),
            })
        {
            return;
        }

        let started = app_for_down
            .update(cx, |this, cx| {
                let started = this.begin_horizontal_scrollbar_drag_from_window_track(
                    pane_id,
                    content_width,
                    scroll_x,
                    track_window_rect,
                    event.position,
                );
                if started {
                    cx.notify();
                }
                started
            })
            .unwrap_or(false);

        if started {
            window.capture_pointer(hitbox_for_down.id);
            window.prevent_default();
            cx.stop_propagation();
        }
    });

    let hitbox_for_move = hitbox.clone();
    let app_for_move = app.clone();
    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
        if !phase.capture() || !event.dragging() {
            return;
        }

        let handled = app_for_move
            .update(cx, |this, cx| {
                if !this.horizontal_scrollbar_drag_is_active_for(pane_id) {
                    return false;
                }
                let changed =
                    this.update_horizontal_scrollbar_drag_from_window(pane_id, event.position);
                if changed {
                    cx.notify();
                }
                true
            })
            .unwrap_or(false);

        if handled {
            window.capture_pointer(hitbox_for_move.id);
            window.prevent_default();
            cx.stop_propagation();
        }
    });

    let app_for_up = app;
    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
        if !phase.capture() || event.button != MouseButton::Left {
            return;
        }

        let finished = app_for_up
            .update(cx, |this, cx| {
                let finished = this.finish_horizontal_scrollbar_drag(pane_id, cx);
                if finished {
                    cx.notify();
                }
                finished
            })
            .unwrap_or(false);

        if finished {
            window.release_pointer();
            window.prevent_default();
            cx.stop_propagation();
        }
    });
}

fn paint_scrollbar_handle(
    bounds: Bounds<Pixels>,
    bar: HorizontalScrollBarLayout,
    window: &mut gpui::Window,
) {
    let handle = bar.handle_rect;
    window.paint_quad(
        fill(
            Bounds::new(
                point(
                    bounds.origin.x + px(handle.x),
                    bounds.origin.y + px(handle.y - bar.track_rect.y),
                ),
                size(px(handle.width), px(handle.height)),
            ),
            rgb(0x7a8494),
        )
        .corner_radii(px(6.0)),
    );
}

struct ScrollBarCanvasState {
    bar: Option<HorizontalScrollBarLayout>,
    track_window_rect: ViewRect,
    _hitbox: Hitbox,
}
