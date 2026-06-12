use crate::FikaApp;
use fika_core::{
    HorizontalScrollBarLayout, PaneId, ViewPoint, ViewRect, horizontal_scroll_bar_layout,
    normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, MouseButton, ParentElement, Pixels, Stateful, Styled, Window, canvas,
    div, fill, point, px, rgb, size,
};

pub(crate) const SCROLLBAR_THICKNESS: f32 = 12.0;
pub(crate) const SCROLLBAR_MIN_HANDLE_WIDTH: f32 = 36.0;

pub(crate) fn install_scrollbar_drag_window_capture(
    app: gpui::WeakEntity<FikaApp>,
    window: &mut Window,
) {
    let app_for_move = app.clone();
    window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
        if !phase.capture() {
            return;
        }
        let handled = app_for_move
            .update(cx, |this, cx| {
                let Some(drag) = this.active_scrollbar_drag else {
                    return false;
                };
                if this.update_horizontal_scrollbar_drag_from_window(drag.pane_id, event.position) {
                    cx.notify();
                }
                true
            })
            .unwrap_or(false);
        if handled {
            cx.stop_propagation();
        }
    });

    let app_for_up = app;
    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _window, cx| {
        if !phase.capture() || event.button != MouseButton::Left {
            return;
        }
        let handled = app_for_up
            .update(cx, |this, cx| {
                let Some(drag) = this.active_scrollbar_drag else {
                    return false;
                };
                if this.finish_horizontal_scrollbar_drag(drag.pane_id, cx) {
                    cx.notify();
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if handled {
            cx.stop_propagation();
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ActiveScrollBarDrag {
    pub(crate) pane_id: PaneId,
    pub(crate) content_width: f32,
    pub(crate) track_window_rect: ViewRect,
    pub(crate) handle_grab_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HorizontalScrollBarTrack {
    pub(crate) window_rect: ViewRect,
    pub(crate) content_width: f32,
    pub(crate) scroll_x: f32,
    pub(crate) handle_rect: ViewRect,
}

pub(crate) fn horizontal_scroll_bar(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
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
        .child(scroll_bar_handle_canvas(
            pane_id,
            content_width,
            scroll_x,
            cx,
        ))
}

pub(crate) fn scrollbar_drag_capture_overlay(
    pane_id: PaneId,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let app = cx.weak_entity();
    div()
        .id(format!("scrollbar-drag-capture-{}", pane_id.0))
        .absolute()
        .inset_0()
        .occlude()
        .on_mouse_move(
            cx.listener(move |this, event: &gpui::MouseMoveEvent, _window, cx| {
                let handled = this
                    .active_scrollbar_drag
                    .is_some_and(|drag| drag.pane_id == pane_id);
                if handled
                    && this.update_horizontal_scrollbar_drag_from_window(pane_id, event.position)
                {
                    cx.notify();
                }
                if handled {
                    cx.stop_propagation();
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, _event: &gpui::MouseUpEvent, _window, cx| {
                if this.finish_horizontal_scrollbar_drag(pane_id, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |this, _event: &gpui::MouseUpEvent, _window, cx| {
                if this.finish_horizontal_scrollbar_drag(pane_id, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            canvas(
                |_bounds, _window, _cx| (),
                move |_bounds, _state, window, _cx| {
                    let app_for_move = app.clone();
                    window.on_mouse_event(
                        move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                            if !phase.capture() {
                                return;
                            }
                            let handled = app_for_move
                                .update(cx, |this, cx| {
                                    let active = this
                                        .active_scrollbar_drag
                                        .is_some_and(|drag| drag.pane_id == pane_id);
                                    if active
                                        && this.update_horizontal_scrollbar_drag_from_window(
                                            pane_id,
                                            event.position,
                                        )
                                    {
                                        cx.notify();
                                    }
                                    active
                                })
                                .unwrap_or(false);
                            if handled {
                                cx.stop_propagation();
                            }
                        },
                    );

                    let app_for_up = app.clone();
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _window, cx| {
                        if !phase.capture() || event.button != MouseButton::Left {
                            return;
                        }
                        let handled = app_for_up
                            .update(cx, |this, cx| {
                                if this.finish_horizontal_scrollbar_drag(pane_id, cx) {
                                    cx.notify();
                                    true
                                } else {
                                    false
                                }
                            })
                            .unwrap_or(false);
                        if handled {
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        )
}

fn scroll_x_for_scrollbar_drag(
    content_width: f32,
    handle_grab_x: f32,
    current_track_x: f32,
    track_width: f32,
) -> Option<(f32, f32)> {
    let mapping_bar = horizontal_scroll_bar_layout(
        content_width,
        0.0,
        track_width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    let handle_x = current_track_x - handle_grab_x;
    Some((
        mapping_bar.scroll_x_for_handle_x(handle_x),
        mapping_bar.max_scroll_x,
    ))
}

fn scroll_x_for_scrollbar_drag_start(
    content_width: f32,
    scroll_x: f32,
    start_track_x: f32,
    track_width: f32,
) -> Option<(f32, f32, f32)> {
    let bar = horizontal_scroll_bar_layout(
        content_width,
        scroll_x,
        track_width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    let target_scroll_x =
        if start_track_x >= bar.handle_rect.x && start_track_x < bar.handle_rect.right() {
            scroll_x.clamp(0.0, bar.max_scroll_x)
        } else {
            bar.scroll_x_for_track_x(start_track_x)
        };
    let target_bar = horizontal_scroll_bar_layout(
        content_width,
        target_scroll_x,
        track_width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    let handle_grab_x =
        (start_track_x - target_bar.handle_rect.x).clamp(0.0, target_bar.handle_rect.width);
    Some((target_scroll_x, handle_grab_x, bar.max_scroll_x))
}

fn scrollbar_track_local_point(
    track: HorizontalScrollBarTrack,
    position: gpui::Point<gpui::Pixels>,
) -> Option<ViewPoint> {
    let point = ViewPoint {
        x: position.x.as_f32(),
        y: position.y.as_f32(),
    };
    if !track.window_rect.contains(point) {
        return None;
    }
    Some(ViewPoint {
        x: point.x - track.window_rect.x,
        y: point.y - track.window_rect.y,
    })
}

fn normalized_scrollbar_window_rect(rect: ViewRect) -> ViewRect {
    ViewRect {
        x: rect.x,
        y: rect.y,
        width: normalize_viewport_extent(rect.width).max(0.0),
        height: normalize_viewport_extent(rect.height).max(0.0),
    }
}

fn horizontal_scrollbar_track(
    window_rect: ViewRect,
    content_width: f32,
    scroll_x: f32,
) -> Option<HorizontalScrollBarTrack> {
    let window_rect = normalized_scrollbar_window_rect(window_rect);
    if window_rect.width <= 0.0 || window_rect.height <= 0.0 {
        return None;
    }
    let bar = horizontal_scroll_bar_layout(
        content_width,
        scroll_x,
        window_rect.width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    Some(HorizontalScrollBarTrack {
        window_rect,
        content_width,
        scroll_x: scroll_x.clamp(0.0, bar.max_scroll_x),
        handle_rect: bar.handle_rect,
    })
}

#[cfg(test)]
fn scrollbar_drag_track_rect(track_width: f32) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: track_width,
        height: SCROLLBAR_THICKNESS,
    }
}

fn scrollbar_local_track_rect(window_rect: ViewRect) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: window_rect.width,
        height: window_rect.height,
    }
}

fn scrollbar_point_is_in_track(local: ViewPoint, track_rect: ViewRect) -> bool {
    track_rect.contains(local)
}

fn scrollbar_track_x_from_window(rect: ViewRect, position: gpui::Point<gpui::Pixels>) -> f32 {
    (position.x.as_f32() - rect.x).clamp(0.0, rect.width)
}

fn scrollbar_window_rect_from_bounds(bounds: Bounds<Pixels>) -> ViewRect {
    ViewRect {
        x: bounds.origin.x.as_f32(),
        y: bounds.origin.y.as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
    }
}

fn scrollbar_track_x_from_local(local: ViewPoint, track_rect: ViewRect) -> f32 {
    local.x.clamp(0.0, track_rect.width)
}

fn scrollbar_track_width(track_rect: ViewRect) -> f32 {
    normalize_viewport_extent(track_rect.width).max(0.0)
}

fn scrollbar_drag_start_from_local(
    content_width: f32,
    scroll_x: f32,
    local: ViewPoint,
    local_track_rect: ViewRect,
) -> Option<(f32, f32, f32)> {
    if !scrollbar_point_is_in_track(local, local_track_rect) {
        return None;
    }
    scroll_x_for_scrollbar_drag_start(
        content_width,
        scroll_x,
        scrollbar_track_x_from_local(local, local_track_rect),
        scrollbar_track_width(local_track_rect),
    )
}

impl FikaApp {
    pub(crate) fn set_horizontal_scrollbar_track(
        &mut self,
        pane_id: PaneId,
        window_rect: ViewRect,
        content_width: f32,
        scroll_x: f32,
    ) -> bool {
        let Some(track) = horizontal_scrollbar_track(window_rect, content_width, scroll_x) else {
            return self.horizontal_scrollbar_tracks.remove(&pane_id).is_some();
        };
        if self.horizontal_scrollbar_tracks.get(&pane_id) == Some(&track) {
            return false;
        }
        self.horizontal_scrollbar_tracks.insert(pane_id, track);
        true
    }

    pub(crate) fn begin_horizontal_scrollbar_drag_from_window(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(track) = self.horizontal_scrollbar_tracks.get(&pane_id).copied() else {
            return false;
        };
        let Some(local) = scrollbar_track_local_point(track, position) else {
            return false;
        };
        self.begin_horizontal_scrollbar_drag_from_track_point(
            pane_id,
            track.content_width,
            track.scroll_x,
            local,
            track.window_rect,
        )
    }

    pub(crate) fn begin_horizontal_scrollbar_drag_from_window_track(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        track_window_rect: ViewRect,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(track) = horizontal_scrollbar_track(track_window_rect, content_width, scroll_x)
        else {
            return false;
        };
        let Some(local) = scrollbar_track_local_point(track, position) else {
            return false;
        };
        self.horizontal_scrollbar_tracks.insert(pane_id, track);
        self.begin_horizontal_scrollbar_drag_from_track_point(
            pane_id,
            track.content_width,
            track.scroll_x,
            local,
            track.window_rect,
        )
    }

    pub(crate) fn update_horizontal_scrollbar_drag_from_window(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        self.update_horizontal_scrollbar_drag(
            pane_id,
            scrollbar_track_x_from_window(drag.track_window_rect, position),
            drag.track_window_rect.width,
        )
    }

    #[cfg(test)]
    pub(crate) fn begin_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        start_track_x: f32,
        track_width: f32,
    ) -> bool {
        let track_rect = scrollbar_drag_track_rect(track_width);
        self.begin_horizontal_scrollbar_drag_from_track_point(
            pane_id,
            content_width,
            scroll_x,
            ViewPoint {
                x: start_track_x,
                y: SCROLLBAR_THICKNESS / 2.0,
            },
            track_rect,
        )
    }

    fn begin_horizontal_scrollbar_drag_from_track_point(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        local: ViewPoint,
        track_window_rect: ViewRect,
    ) -> bool {
        let local_track_rect = scrollbar_local_track_rect(track_window_rect);
        let Some((initial_scroll_x, handle_grab_x, max_scroll_x)) =
            scrollbar_drag_start_from_local(content_width, scroll_x, local, local_track_rect)
        else {
            return false;
        };
        self.finish_rubber_band(pane_id);
        self.active_scrollbar_drag = Some(ActiveScrollBarDrag {
            pane_id,
            content_width,
            track_window_rect,
            handle_grab_x,
        });
        self.set_pane_scroll_immediate(pane_id, initial_scroll_x, 0.0, max_scroll_x, 0.0);
        true
    }

    pub(crate) fn update_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        track_x: f32,
        track_width: f32,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        let Some((scroll_x, max_scroll_x)) = scroll_x_for_scrollbar_drag(
            drag.content_width,
            drag.handle_grab_x,
            track_x,
            track_width,
        ) else {
            return false;
        };
        let previous_scroll_x = self
            .panes
            .pane(drag.pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or_default();
        self.set_pane_scroll_immediate(drag.pane_id, scroll_x, 0.0, max_scroll_x, 0.0);
        self.panes
            .pane(drag.pane_id)
            .is_some_and(|pane| (pane.view.scroll_x - previous_scroll_x).abs() > f32::EPSILON)
    }

    pub(crate) fn finish_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        self.active_scrollbar_drag = None;
        self.finish_scrollbar_drag_for_content_width(drag.pane_id, drag.content_width, cx);
        true
    }

    pub(crate) fn clear_horizontal_scrollbar_drag_for_pane(&mut self, pane_id: PaneId) {
        if self
            .active_scrollbar_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.active_scrollbar_drag = None;
        }
        self.horizontal_scrollbar_tracks.remove(&pane_id);
    }
}

fn scroll_bar_handle_canvas(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
    cx: &mut Context<FikaApp>,
) -> impl IntoElement {
    let app = cx.weak_entity();
    let app_for_prepaint = app.clone();
    canvas(
        move |bounds, _window, cx| {
            let track_window_rect = scrollbar_window_rect_from_bounds(bounds);
            let _ = app_for_prepaint.update(cx, |this, _cx| {
                this.set_horizontal_scrollbar_track(
                    pane_id,
                    track_window_rect,
                    content_width,
                    scroll_x,
                );
            });
            scroll_bar_layout_for_bounds(content_width, scroll_x, bounds)
        },
        move |bounds, bar, window, _cx| {
            let Some(bar) = bar else {
                return;
            };
            let track_window_rect = scrollbar_window_rect_from_bounds(bounds);

            let app_for_down = app.clone();
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _window, cx| {
                if !phase.capture()
                    || event.button != MouseButton::Left
                    || !bounds.contains(&event.position)
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
                    cx.stop_propagation();
                }
            });

            let app_for_move = app.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                if !phase.capture() {
                    return;
                }
                let handled = app_for_move
                    .update(cx, |this, cx| {
                        let active = this
                            .active_scrollbar_drag
                            .is_some_and(|drag| drag.pane_id == pane_id);
                        if active
                            && this.update_horizontal_scrollbar_drag_from_window(
                                pane_id,
                                event.position,
                            )
                        {
                            cx.notify();
                        }
                        active
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            });

            let app_for_up = app.clone();
            window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _window, cx| {
                if !phase.capture() || event.button != MouseButton::Left {
                    return;
                }
                let handled = app_for_up
                    .update(cx, |this, cx| {
                        if this.finish_horizontal_scrollbar_drag(pane_id, cx) {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            });

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
        },
    )
    .absolute()
    .size_full()
}

fn scroll_bar_layout_for_bounds(
    content_width: f32,
    scroll_x: f32,
    bounds: Bounds<Pixels>,
) -> Option<HorizontalScrollBarLayout> {
    horizontal_scroll_bar_layout(
        content_width,
        scroll_x,
        normalize_viewport_extent(bounds.size.width.as_f32()),
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )
}

#[cfg(test)]
mod tests {
    use super::{scroll_x_for_scrollbar_drag, scroll_x_for_scrollbar_drag_start};

    #[test]
    fn scrollbar_drag_preserves_initial_handle_grab_offset() {
        let content_width = 1200.0;
        let track_width = 180.0;
        let initial_scroll_x = 240.0;
        let start_track_x = 60.0;

        let (_, handle_grab_x, _) = scroll_x_for_scrollbar_drag_start(
            content_width,
            initial_scroll_x,
            start_track_x,
            track_width,
        )
        .unwrap();
        let (same_scroll_x, max_scroll_x) =
            scroll_x_for_scrollbar_drag(content_width, handle_grab_x, start_track_x, track_width)
                .unwrap();
        let (moved_scroll_x, moved_max_scroll_x) = scroll_x_for_scrollbar_drag(
            content_width,
            handle_grab_x,
            start_track_x + 24.0,
            track_width,
        )
        .unwrap();

        assert_eq!(max_scroll_x, moved_max_scroll_x);
        assert!(
            (same_scroll_x - initial_scroll_x).abs() <= 0.5,
            "drag start should not re-center the handle under the cursor"
        );
        assert!(moved_scroll_x > initial_scroll_x + 100.0);
    }
}
