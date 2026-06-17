mod dnd_helpers;
mod row;
mod section;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Div, Empty, Entity, ExternalPaths, Hitbox, HitboxBehavior, MouseButton,
    NavigationDirection, ParentElement, Pixels, ScrollHandle, Size, Stateful, Styled, Window,
    canvas, div, fill, point, px, rgb, rgba, size,
};

use crate::ui::background_tasks::{BackgroundTasksSnapshot, background_tasks_panel};
use crate::ui::file_grid::ItemDrag;
use crate::ui::icons::{FileIconCache, FileIconSnapshot, cached_icon_or_fallback};
use std::time::Instant;

use super::drag::PlaceDrag;
use super::interaction::places_interaction_geometry;
use super::perf::{
    PlacesInteractionGeometryPerfLog, PlacesInteractionPolicyLog, PlacesRendererPolicyLog,
    PlacesScrollbarPerfLog, PlacesSidebarPerfLog, custom_places_rows_enabled,
    emit_places_interaction_geometry_perf_log, emit_places_interaction_policy_log,
    emit_places_renderer_policy_log, emit_places_scrollbar_perf_log, emit_places_sidebar_perf_log,
    places_perf_enabled, places_section_count,
};
use super::snapshot::PlaceSnapshot;
use super::visual::places_row_visual_layer;
use row::place_row;
use section::group_heading;

const PLACES_SCROLLBAR_WIDTH: f32 = 10.0;
const PLACES_SCROLLBAR_THUMB_WIDTH: f32 = 4.0;
const PLACES_SCROLLBAR_PADDING: f32 = 3.0;
const PLACES_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 24.0;
pub(crate) const PLACES_SIDEBAR_DEFAULT_WIDTH: f32 = 220.0;
pub(crate) const PLACES_SIDEBAR_MIN_WIDTH: f32 = 160.0;
pub(crate) const PLACES_SIDEBAR_MAX_WIDTH: f32 = 420.0;
const PLACES_SIDEBAR_SPLITTER_WIDTH: f32 = 1.0;
const PLACES_SIDEBAR_SPLITTER_HITBOX_WIDTH: f32 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesSidebarResizeDrag;

pub(crate) fn clamp_places_sidebar_width(width: f32) -> f32 {
    if !width.is_finite() {
        return PLACES_SIDEBAR_DEFAULT_WIDTH;
    }
    width.clamp(PLACES_SIDEBAR_MIN_WIDTH, PLACES_SIDEBAR_MAX_WIDTH)
}

pub(crate) fn places_sidebar_width_from_drag(pointer_x: f32, row_x: f32) -> f32 {
    (pointer_x - row_x).floor()
}

pub(crate) fn places_panel_icon_snapshot(
    cache: &mut FileIconCache,
    visible: bool,
) -> FileIconSnapshot {
    cache.named_icon(
        if visible {
            "places-sidebar-visible"
        } else {
            "places-sidebar-hidden"
        },
        &[
            "sidebar-show",
            "view-left-sidebar",
            "bookmarks",
            "folder-bookmarks",
        ],
        "P",
        if visible { 0x1f4fbf } else { 0x475569 },
        if visible { 0xeaf1ff } else { 0xf1f5f9 },
        18.0,
    )
}

pub(crate) fn places_panel_button(
    visible: bool,
    icon: FileIconSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id("places-sidebar-toggle")
        .h(px(28.0))
        .min_w(px(28.0))
        .px_1()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .border_color(if visible {
            rgb(0x2f6fed)
        } else {
            rgb(0xb6bcc6)
        })
        .bg(if visible {
            rgb(0xeaf1ff)
        } else {
            rgb(0xffffff)
        })
        .hover(|button| button.bg(rgb(0xdbe7fb)))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() {
                    this.toggle_places_sidebar_from_button(cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .child(
            div()
                .w(px(18.0))
                .h(px(18.0))
                .flex_none()
                .overflow_hidden()
                .child(cached_icon_or_fallback(&icon, || {
                    div().text_xs().child("P").into_any_element()
                })),
        )
}

pub(crate) fn places_sidebar_splitter(cx: &mut Context<FikaApp>) -> Stateful<Div> {
    div()
        .id("places-sidebar-splitter")
        .relative()
        .flex_none()
        .w(px(PLACES_SIDEBAR_SPLITTER_WIDTH))
        .h_full()
        .bg(rgb(0xc8ced6))
        .child(
            div()
                .id("places-sidebar-splitter-hitbox")
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .left(px((PLACES_SIDEBAR_SPLITTER_WIDTH
                    - PLACES_SIDEBAR_SPLITTER_HITBOX_WIDTH)
                    / 2.0))
                .w(px(PLACES_SIDEBAR_SPLITTER_HITBOX_WIDTH))
                .cursor_col_resize()
                .block_mouse_except_scroll()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.click_count() >= 2 && this.reset_places_sidebar_width(cx) {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_drag(PlacesSidebarResizeDrag, |_, _, _, cx| cx.new(|_| Empty)),
        )
        .hover(|splitter| splitter.bg(rgb(0x2f6fed)))
}

pub(crate) fn places_sidebar(
    places: Vec<PlaceSnapshot>,
    background_tasks: Option<BackgroundTasksSnapshot>,
    width: f32,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let perf_enabled = places_perf_enabled();
    let build_started = perf_enabled.then(Instant::now);
    let row_count = places.len();
    let section_count = places_section_count(&places);
    let interaction_geometry = perf_enabled.then(|| {
        let started = Instant::now();
        let geometry = places_interaction_geometry(&places);
        (geometry, started.elapsed())
    });
    let custom_row_visuals = custom_places_rows_enabled();
    let state = window.use_keyed_state("places-sidebar-scrollbar", cx, |_, _| {
        PlacesSidebarScrollState::new()
    });
    let scroll_handle = state.read(cx).scroll_handle.clone();
    let app = cx.weak_entity();
    let row_visual_layer =
        custom_row_visuals.then(|| places_row_visual_layer(places.clone(), app.clone()));
    let mut rows = Vec::new();
    let mut current_group = None;

    for (index, place) in places.into_iter().enumerate() {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                rows.push(group_heading(
                    place.group,
                    place.index,
                    custom_row_visuals,
                    cx,
                ));
            }
        }
        rows.push(place_row(index, place, custom_row_visuals, cx));
    }
    if let Some(started) = build_started {
        emit_places_sidebar_perf_log(PlacesSidebarPerfLog {
            row_count,
            section_count,
            element_count: rows.len(),
            build_elapsed: started.elapsed(),
        });
        emit_places_renderer_policy_log(PlacesRendererPolicyLog {
            row_count,
            section_count,
            custom_row_visuals,
            scrollbar_canvas_count: 1,
        });
        emit_places_interaction_policy_log(PlacesInteractionPolicyLog {
            row_count,
            section_count,
        });
        if let Some((geometry, elapsed)) = &interaction_geometry {
            emit_places_interaction_geometry_perf_log(PlacesInteractionGeometryPerfLog {
                rows: geometry.rows().len(),
                sections: geometry.sections().len(),
                entries: geometry.entries(),
                content_height: geometry.content_height(),
                elapsed: *elapsed,
            });
        }
    }

    div()
        .id("places-sidebar")
        .flex()
        .flex_col()
        .w(px(clamp_places_sidebar_width(width)))
        .min_w(px(PLACES_SIDEBAR_MIN_WIDTH))
        .min_h_0()
        .mt(px(8.0))
        .mb(px(8.0))
        .ml_2()
        .border_1()
        .rounded_xl()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xf8f9fb))
        .overflow_hidden()
        .px_2()
        .py_2()
        .on_drag_move::<ItemDrag>(cx.listener(
            |this, event: &gpui::DragMoveEvent<ItemDrag>, _window, cx| {
                if clear_places_drop_target_after_sidebar_leave(
                    this,
                    event.bounds,
                    event.event.position,
                ) {
                    cx.notify();
                }
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            |this, event: &gpui::DragMoveEvent<ExternalPaths>, _window, cx| {
                if clear_places_drop_target_after_sidebar_leave(
                    this,
                    event.bounds,
                    event.event.position,
                ) {
                    cx.notify();
                }
            },
        ))
        .on_drag_move::<PlaceDrag>(cx.listener(
            |this, event: &gpui::DragMoveEvent<PlaceDrag>, _window, cx| {
                if clear_places_drop_target_after_sidebar_leave(
                    this,
                    event.bounds,
                    event.event.position,
                ) {
                    cx.notify();
                }
            },
        ))
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(pane_id) = this.panes.focused() {
                    this.go_back(pane_id);
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(pane_id) = this.panes.focused() {
                    this.go_forward(pane_id);
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_places_blank_context_menu(event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(
            div()
                .px_2()
                .pb_2()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_sm()
                .text_color(rgb(0x24292f))
                .child("Places"),
        )
        .child(
            div()
                .relative()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("places-sidebar-list")
                        .relative()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&scroll_handle)
                        .when_some(row_visual_layer, |list, layer| list.child(layer))
                        .children(rows),
                )
                .child(places_sidebar_scrollbar(state, perf_enabled)),
        )
        .when_some(background_tasks, |sidebar, tasks| {
            sidebar.child(background_tasks_panel(tasks, cx))
        })
}

fn clear_places_drop_target_after_sidebar_leave(
    app: &mut FikaApp,
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> bool {
    if places_sidebar_contains_drag_position(bounds, position) {
        return false;
    }
    app.clear_place_drop_target()
}

fn places_sidebar_contains_drag_position(
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> bool {
    bounds.contains(&position)
}

struct PlacesSidebarScrollState {
    scroll_handle: ScrollHandle,
    drag_grab_y: Option<f32>,
}

impl PlacesSidebarScrollState {
    fn new() -> Self {
        Self {
            scroll_handle: ScrollHandle::new(),
            drag_grab_y: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlacesSidebarScrollbarMetrics {
    track_top: f32,
    track_height: f32,
    thumb_top: f32,
    thumb_height: f32,
    max_scroll_y: f32,
}

impl PlacesSidebarScrollbarMetrics {
    fn thumb_bounds(self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let thumb_x = (bounds.size.width.as_f32() - PLACES_SCROLLBAR_THUMB_WIDTH).max(0.0) / 2.0;
        Bounds::new(
            point(
                bounds.origin.x + px(thumb_x),
                bounds.origin.y + px(self.thumb_top),
            ),
            size(px(PLACES_SCROLLBAR_THUMB_WIDTH), px(self.thumb_height)),
        )
    }
}

struct PlacesSidebarScrollbarPaintState {
    metrics: Option<PlacesSidebarScrollbarMetrics>,
    hitbox: Option<Hitbox>,
}

fn places_sidebar_scrollbar(state: Entity<PlacesSidebarScrollState>, perf_enabled: bool) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(PLACES_SCROLLBAR_WIDTH))
        .h_full()
        .child(
            canvas(
                {
                    let state = state.clone();
                    move |bounds, window, cx| {
                        let scroll_handle = state.read(cx).scroll_handle.clone();
                        let metrics = places_sidebar_scrollbar_metrics(&scroll_handle, bounds);
                        PlacesSidebarScrollbarPaintState {
                            metrics,
                            hitbox: metrics
                                .map(|_| window.insert_hitbox(bounds, HitboxBehavior::BlockMouse)),
                        }
                    }
                },
                move |bounds, paint_state, window, cx| {
                    paint_places_sidebar_scrollbar(bounds, &paint_state, window);
                    if perf_enabled {
                        emit_places_sidebar_scrollbar_perf(&paint_state);
                    }
                    install_places_sidebar_scrollbar_mouse_handlers(
                        state.clone(),
                        bounds,
                        paint_state,
                        window,
                        cx,
                    );
                },
            )
            .size_full(),
        )
}

fn places_sidebar_scrollbar_metrics(
    scroll_handle: &ScrollHandle,
    bounds: Bounds<Pixels>,
) -> Option<PlacesSidebarScrollbarMetrics> {
    let viewport_height = scroll_handle.bounds().size.height.as_f32();
    places_sidebar_scrollbar_metrics_for_values(
        if viewport_height > 0.0 {
            viewport_height
        } else {
            bounds.size.height.as_f32()
        },
        scroll_handle.max_offset().y.as_f32(),
        -scroll_handle.offset().y.as_f32(),
        bounds.size.height.as_f32(),
    )
}

fn places_sidebar_scrollbar_metrics_for_values(
    viewport_height: f32,
    max_scroll_y: f32,
    scroll_y: f32,
    bounds_height: f32,
) -> Option<PlacesSidebarScrollbarMetrics> {
    let max_scroll_y = max_scroll_y.max(0.0);
    let track_top = PLACES_SCROLLBAR_PADDING;
    let track_height = (bounds_height - PLACES_SCROLLBAR_PADDING * 2.0).max(0.0);
    if max_scroll_y <= 0.0 || viewport_height <= 0.0 || track_height <= 0.0 {
        return None;
    }

    let content_height = viewport_height + max_scroll_y;
    let thumb_height = (track_height * (viewport_height / content_height))
        .clamp(PLACES_SCROLLBAR_MIN_THUMB_HEIGHT, track_height)
        .floor();
    if thumb_height >= track_height {
        return None;
    }

    let available = (track_height - thumb_height).max(0.0);
    let thumb_top =
        track_top + (scroll_y.clamp(0.0, max_scroll_y) / max_scroll_y).clamp(0.0, 1.0) * available;

    Some(PlacesSidebarScrollbarMetrics {
        track_top,
        track_height,
        thumb_top,
        thumb_height,
        max_scroll_y,
    })
}

fn paint_places_sidebar_scrollbar(
    bounds: Bounds<Pixels>,
    paint_state: &PlacesSidebarScrollbarPaintState,
    window: &mut Window,
) {
    let Some(metrics) = paint_state.metrics else {
        return;
    };

    let track_x =
        bounds.origin.x + px((bounds.size.width.as_f32() - PLACES_SCROLLBAR_THUMB_WIDTH) / 2.0);
    let track_bounds = Bounds::new(
        point(track_x, bounds.origin.y + px(metrics.track_top)),
        Size {
            width: px(PLACES_SCROLLBAR_THUMB_WIDTH),
            height: px(metrics.track_height),
        },
    );
    window.paint_quad(fill(track_bounds, rgba(0xd5dbe466)).corner_radii(px(2.0)));
    window.paint_quad(fill(metrics.thumb_bounds(bounds), rgba(0x6f7b8acc)).corner_radii(px(2.0)));
}

fn emit_places_sidebar_scrollbar_perf(paint_state: &PlacesSidebarScrollbarPaintState) {
    let Some(metrics) = paint_state.metrics else {
        emit_places_scrollbar_perf_log(PlacesScrollbarPerfLog {
            visible: false,
            max_scroll_y: 0.0,
            thumb_height: 0.0,
            track_height: 0.0,
        });
        return;
    };
    emit_places_scrollbar_perf_log(PlacesScrollbarPerfLog {
        visible: true,
        max_scroll_y: metrics.max_scroll_y,
        thumb_height: metrics.thumb_height,
        track_height: metrics.track_height,
    });
}

fn install_places_sidebar_scrollbar_mouse_handlers(
    state: Entity<PlacesSidebarScrollState>,
    bounds: Bounds<Pixels>,
    paint_state: PlacesSidebarScrollbarPaintState,
    window: &mut Window,
    _cx: &mut App,
) {
    let (Some(metrics), Some(hitbox)) = (paint_state.metrics, paint_state.hitbox.clone()) else {
        return;
    };

    window.on_mouse_event({
        let state = state.clone();
        move |event: &gpui::MouseDownEvent, phase, window, cx| {
            if !phase.capture() || event.button != MouseButton::Left {
                return;
            }
            if !bounds.contains(&event.position) {
                return;
            }

            let local_y = (event.position.y - bounds.origin.y).as_f32();
            let grab_y = if metrics.thumb_bounds(bounds).contains(&event.position) {
                local_y - metrics.thumb_top
            } else {
                metrics.thumb_height / 2.0
            };
            state.update(cx, |state, cx| {
                state.drag_grab_y = Some(grab_y);
                set_places_sidebar_scroll_y(
                    &state.scroll_handle,
                    places_sidebar_scroll_y_for_local_y(local_y, grab_y, metrics),
                );
                cx.notify();
            });
            window.capture_pointer(hitbox.id);
            cx.stop_propagation();
        }
    });

    window.on_mouse_event({
        let state = state.clone();
        move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
            if !phase.capture() || !event.dragging() {
                return;
            }
            let local_y = (event.position.y - bounds.origin.y).as_f32();
            state.update(cx, |state, cx| {
                let Some(grab_y) = state.drag_grab_y else {
                    return;
                };
                set_places_sidebar_scroll_y(
                    &state.scroll_handle,
                    places_sidebar_scroll_y_for_local_y(local_y, grab_y, metrics),
                );
                cx.notify();
                cx.stop_propagation();
            });
        }
    });

    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
        if !phase.capture() || event.button != MouseButton::Left {
            return;
        }
        state.update(cx, |state, cx| {
            if state.drag_grab_y.take().is_some() {
                window.release_pointer();
                cx.notify();
                cx.stop_propagation();
            }
        });
    });
}

fn places_sidebar_scroll_y_for_local_y(
    local_y: f32,
    grab_y: f32,
    metrics: PlacesSidebarScrollbarMetrics,
) -> f32 {
    let available = (metrics.track_height - metrics.thumb_height).max(0.0);
    if available <= 0.0 || metrics.max_scroll_y <= 0.0 {
        return 0.0;
    }
    let thumb_top = (local_y - grab_y).clamp(metrics.track_top, metrics.track_top + available);
    ((thumb_top - metrics.track_top) / available * metrics.max_scroll_y)
        .clamp(0.0, metrics.max_scroll_y)
}

fn set_places_sidebar_scroll_y(scroll_handle: &ScrollHandle, scroll_y: f32) {
    let current = scroll_handle.offset();
    scroll_handle.set_offset(point(current.x, px(-scroll_y.max(0.0))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_sidebar_scrollbar_metrics_hide_without_overflow() {
        assert_eq!(
            places_sidebar_scrollbar_metrics_for_values(200.0, 0.0, 0.0, 200.0),
            None
        );
    }

    #[test]
    fn places_sidebar_scrollbar_metrics_track_offset_and_size() {
        let metrics =
            places_sidebar_scrollbar_metrics_for_values(200.0, 300.0, 150.0, 206.0).unwrap();

        assert_eq!(metrics.track_top, PLACES_SCROLLBAR_PADDING);
        assert_eq!(metrics.track_height, 200.0);
        assert_eq!(metrics.thumb_height, 80.0);
        assert_eq!(metrics.thumb_top, 63.0);
    }

    #[test]
    fn places_sidebar_scrollbar_drag_maps_track_position_to_scroll() {
        let metrics =
            places_sidebar_scrollbar_metrics_for_values(200.0, 300.0, 0.0, 206.0).unwrap();

        assert_eq!(
            places_sidebar_scroll_y_for_local_y(metrics.track_top, 0.0, metrics),
            0.0
        );
        assert_eq!(
            places_sidebar_scroll_y_for_local_y(
                metrics.track_top + metrics.track_height,
                metrics.thumb_height,
                metrics
            ),
            300.0
        );
    }

    #[test]
    fn places_sidebar_drag_leave_geometry_clears_only_outside_bounds() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(220.0), px(300.0)));

        assert!(places_sidebar_contains_drag_position(
            bounds,
            point(px(24.0), px(80.0))
        ));
        assert!(!places_sidebar_contains_drag_position(
            bounds,
            point(px(240.0), px(80.0))
        ));
    }
}
