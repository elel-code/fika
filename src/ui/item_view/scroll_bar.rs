mod state;

use crate::FikaApp;
use fika_core::{CompactLayout, PaneId, ViewState};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, CursorStyle, Div, Hitbox, HitboxBehavior, MouseButton, NavigationDirection,
    ParentElement, Pixels, Stateful, Styled, Window, canvas, div, fill, point, px, rgb, rgba, size,
};
use state::{
    ItemViewScrollDragSession, ItemViewScrollMetrics, ItemViewScrollPress, item_view_wheel_delta,
};

pub(crate) const ITEM_VIEW_SCROLLBAR_HEIGHT: f32 = 12.0;
const ITEM_VIEW_SCROLLBAR_THUMB_HEIGHT: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollDrag {
    pub(crate) pane_id: PaneId,
    session: ItemViewScrollDragSession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ItemViewScrollPressResult {
    handled: bool,
    changed: bool,
    dragging: bool,
}

impl ItemViewScrollPressResult {
    const IGNORED: Self = Self {
        handled: false,
        changed: false,
        dragging: false,
    };

    fn handled(changed: bool, dragging: bool) -> Self {
        Self {
            handled: true,
            changed,
            dragging,
        }
    }
}

pub(crate) fn item_view_horizontal_scrollbar(
    pane_id: PaneId,
    layout: CompactLayout,
    view: ViewState,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let app = cx.weak_entity();
    let visible = ItemViewScrollMetrics::from_extents(
        layout.content_size().width,
        view.viewport_width,
        view.scroll_x,
        view.viewport_width,
    )
    .is_some();
    let wheel_layout = layout.clone();
    let wheel_view = view.clone();
    div()
        .id(format!("item-view-scroll-x-{}", pane_id.0))
        .h(px(if visible {
            ITEM_VIEW_SCROLLBAR_HEIGHT
        } else {
            0.0
        }))
        .w_full()
        .max_w_full()
        .min_w_0()
        .flex_none()
        .overflow_hidden()
        .occlude()
        .cursor_pointer()
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
                handle_item_view_wheel(
                    this,
                    pane_id,
                    event,
                    window,
                    &wheel_layout,
                    &wheel_view,
                    cx,
                );
            }),
        )
        .when(visible, |scrollbar| {
            scrollbar.child(
                canvas(
                    move |bounds, window, _cx| ItemViewScrollBarPaintState {
                        metrics: ItemViewScrollMetrics::from_extents(
                            layout.content_size().width,
                            view.viewport_width,
                            view.scroll_x,
                            bounds.size.width.as_f32(),
                        ),
                        hitbox: window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
                    },
                    move |bounds, state, window, cx| {
                        if let Some(metrics) = state.metrics {
                            paint_item_view_scrollbar(bounds, metrics, window);
                        }
                        register_item_view_scrollbar_handlers(
                            pane_id,
                            state.metrics,
                            bounds,
                            state.hitbox.clone(),
                            app.clone(),
                            window,
                            cx,
                        );
                    },
                )
                .size_full(),
            )
        })
}

struct ItemViewScrollBarPaintState {
    metrics: Option<ItemViewScrollMetrics>,
    hitbox: Hitbox,
}

fn register_item_view_scrollbar_handlers(
    pane_id: PaneId,
    metrics: Option<ItemViewScrollMetrics>,
    track_bounds: Bounds<Pixels>,
    hitbox: Hitbox,
    app: gpui::WeakEntity<FikaApp>,
    window: &mut Window,
    app_cx: &mut gpui::App,
) {
    let hitbox_for_down = hitbox.clone();
    let app_for_down = app.clone();
    window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
        if !phase.capture() || event.button != MouseButton::Left {
            return;
        }
        let Some(metrics) = metrics else {
            return;
        };
        let local_x = (event.position.x - track_bounds.origin.x).as_f32();
        let local_y = (event.position.y - track_bounds.origin.y).as_f32();
        if !(0.0..=track_bounds.size.width.as_f32()).contains(&local_x)
            || !(0.0..=track_bounds.size.height.as_f32()).contains(&local_y)
        {
            return;
        }
        let handled = app_for_down
            .update(cx, |this, cx| {
                this.panes.focus(pane_id);
                let result = this.press_item_view_scrollbar(pane_id, local_x, metrics);
                if result.changed {
                    cx.notify();
                }
                result
            })
            .unwrap_or(ItemViewScrollPressResult::IGNORED);
        if handled.handled {
            if handled.dragging {
                window.capture_pointer(hitbox_for_down.id);
            }
            cx.stop_propagation();
        }
    });

    let hitbox_for_move = hitbox.clone();
    let app_for_move = app.clone();
    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, cx| {
        if !phase.capture() || !event.dragging() {
            return;
        }
        let Some(metrics) = metrics else {
            return;
        };
        let local_x = (event.position.x - track_bounds.origin.x).as_f32();
        let handled = app_for_move
            .update(cx, |this, cx| {
                let Some(changed) = this.update_item_view_scroll_drag(pane_id, local_x, metrics)
                else {
                    return false;
                };
                window.capture_pointer(hitbox_for_move.id);
                if changed {
                    cx.notify();
                }
                true
            })
            .unwrap_or(false);
        if handled {
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
                let handled = this.finish_item_view_scroll_drag(pane_id);
                if handled {
                    cx.notify();
                }
                handled
            })
            .unwrap_or(false);
        if handled {
            window.release_pointer();
            cx.stop_propagation();
        }
    });

    window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
    if app
        .read_with(app_cx, |this, _cx| {
            this.item_view_scroll_drag
                .is_some_and(|drag| drag.pane_id == pane_id)
        })
        .unwrap_or(false)
    {
        window.set_window_cursor_style(CursorStyle::PointingHand);
    }
}

pub(crate) fn handle_item_view_wheel(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::ScrollWheelEvent,
    window: &mut Window,
    layout: &CompactLayout,
    view: &ViewState,
    cx: &mut Context<FikaApp>,
) {
    if wheel_modifiers_request_zoom(event.modifiers) {
        app.finish_rubber_band(pane_id);
        app.zoom_pane_from_wheel(pane_id, event.delta);
        cx.stop_propagation();
        cx.notify();
        return;
    }

    app.finish_rubber_band(pane_id);
    let delta = item_view_wheel_delta(event.delta, item_view_scroll_single_step(window));
    let changed = app.scroll_item_view_by_delta(pane_id, layout, view, delta);
    if changed {
        cx.notify();
    }
    cx.stop_propagation();
}

fn item_view_scroll_single_step(window: &Window) -> f32 {
    window.line_height().as_f32().floor().max(1.0) * 2.0
}

fn wheel_modifiers_request_zoom(modifiers: gpui::Modifiers) -> bool {
    modifiers.control || modifiers.secondary()
}

fn paint_item_view_scrollbar(
    bounds: Bounds<Pixels>,
    metrics: ItemViewScrollMetrics,
    window: &mut Window,
) {
    window.paint_quad(fill(bounds, rgba(0x00000000)));
    let thumb_height = ITEM_VIEW_SCROLLBAR_THUMB_HEIGHT.min(bounds.size.height.as_f32());
    let thumb_y = ((bounds.size.height.as_f32() - thumb_height) / 2.0).max(0.0);
    window.paint_quad(
        fill(
            Bounds::new(
                point(
                    bounds.origin.x + px(metrics.handle_left),
                    bounds.origin.y + px(thumb_y),
                ),
                size(px(metrics.handle_width), px(thumb_height)),
            ),
            rgb(0x7a8494),
        )
        .corner_radii(px((thumb_height / 2.0).max(1.0))),
    );
}

impl FikaApp {
    fn press_item_view_scrollbar(
        &mut self,
        pane_id: PaneId,
        local_x: f32,
        metrics: ItemViewScrollMetrics,
    ) -> ItemViewScrollPressResult {
        match metrics.press_kind(local_x) {
            ItemViewScrollPress::None => ItemViewScrollPressResult::IGNORED,
            ItemViewScrollPress::Thumb => {
                self.finish_rubber_band(pane_id);
                self.item_view_scroll_drag = Some(ItemViewScrollDrag {
                    pane_id,
                    session: metrics.thumb_drag_session(local_x),
                });
                ItemViewScrollPressResult::handled(false, true)
            }
            press @ (ItemViewScrollPress::PageBackward | ItemViewScrollPress::PageForward) => {
                self.finish_rubber_band(pane_id);
                self.item_view_scroll_drag = None;
                let Some(scroll_x) = metrics.scroll_x_after_page_press(press) else {
                    return ItemViewScrollPressResult::IGNORED;
                };
                let changed =
                    self.write_item_view_scroll_x(pane_id, scroll_x, metrics.max_scroll_x);
                ItemViewScrollPressResult::handled(changed, false)
            }
        }
    }

    fn update_item_view_scroll_drag(
        &mut self,
        pane_id: PaneId,
        local_x: f32,
        metrics: ItemViewScrollMetrics,
    ) -> Option<bool> {
        let drag = self.item_view_scroll_drag?;
        if drag.pane_id != pane_id {
            return None;
        }
        Some(self.write_item_view_scroll_x(
            pane_id,
            drag.session.scroll_x_for_local_x(local_x, metrics),
            metrics.max_scroll_x,
        ))
    }

    fn finish_item_view_scroll_drag(&mut self, pane_id: PaneId) -> bool {
        if self
            .item_view_scroll_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.item_view_scroll_drag = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_item_view_scroll_for_pane(&mut self, pane_id: PaneId) {
        if self
            .item_view_scroll_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.item_view_scroll_drag = None;
        }
    }

    pub(crate) fn scroll_item_view_by_delta(
        &mut self,
        pane_id: PaneId,
        layout: &CompactLayout,
        view: &ViewState,
        delta_x: f32,
    ) -> bool {
        let viewport_width = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.viewport_width)
            .unwrap_or(view.viewport_width);
        let max_scroll_x = (layout.content_size().width - viewport_width)
            .floor()
            .max(0.0);
        if max_scroll_x <= 0.0 {
            self.cancel_item_view_scroll_for_pane(pane_id);
            return false;
        }
        let previous = self.panes.pane(pane_id).map(|pane| pane.view.scroll_x);
        let Some(next) = self
            .panes
            .scroll_view(pane_id, delta_x, 0.0, max_scroll_x, 0.0)
        else {
            return false;
        };
        previous.is_some_and(|previous| (next.scroll_x - previous).abs() > f32::EPSILON)
    }

    fn write_item_view_scroll_x(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        max_scroll_x: f32,
    ) -> bool {
        let previous = self.panes.pane(pane_id).map(|pane| pane.view.scroll_x);
        let Some(next) = self
            .panes
            .set_view_scroll(pane_id, scroll_x, 0.0, max_scroll_x, 0.0)
        else {
            return false;
        };
        previous.is_some_and(|previous| (next.scroll_x - previous).abs() > f32::EPSILON)
    }
}
