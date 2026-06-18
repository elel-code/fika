use std::collections::HashMap;

use fika_core::{PaneId, ViewMode, ViewPoint, ViewRect, ViewState, normalize_viewport_extent};
use gpui::prelude::*;
use gpui::{Context, Div, Empty, MouseButton, NavigationDirection, Stateful, div, rgba};

use crate::FikaApp;
use crate::ui::item_view::{ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT, ItemViewScrollbarAxis};
use crate::ui::rubber_band::RubberBandDrag;

use super::controller::{
    handle_file_grid_wheel, handle_item_mouse_down, handle_pane_navigation_mouse_down,
};
use super::dnd::install_file_grid_path_drop_shell;
use super::{FileGridMode, FileGridRenderSnapshot, PaneViewportGeometry};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MeasuredViewport {
    pub(super) rect: ViewRect,
    pub(super) max_scroll_x: f32,
    pub(super) max_scroll_y: f32,
}

pub(super) fn scrollbar_axis_for_snapshot(
    snapshot: &FileGridRenderSnapshot,
) -> ItemViewScrollbarAxis {
    match snapshot {
        FileGridRenderSnapshot::Compact { .. } => ItemViewScrollbarAxis::Horizontal,
        FileGridRenderSnapshot::Icons { .. } | FileGridRenderSnapshot::Details { .. } => {
            ItemViewScrollbarAxis::Vertical
        }
    }
}

pub(super) fn view_mode_for_snapshot(snapshot: &FileGridRenderSnapshot) -> ViewMode {
    match snapshot {
        FileGridRenderSnapshot::Compact { .. } => ViewMode::Compact,
        FileGridRenderSnapshot::Icons { .. } => ViewMode::Icons,
        FileGridRenderSnapshot::Details { .. } => ViewMode::Details,
    }
}

pub(super) fn viewport_bounds_update_requires_notify(
    previous: Option<&ViewState>,
    next: Option<&ViewState>,
    projected_width: Option<f32>,
    measured_rect: ViewRect,
) -> bool {
    let (Some(previous), Some(next)) = (previous, next) else {
        return true;
    };
    if !viewport_value_eq(previous.scroll_x, next.scroll_x)
        || !viewport_value_eq(previous.scroll_y, next.scroll_y)
    {
        return true;
    }
    if !viewport_value_eq(previous.viewport_height, measured_rect.height) {
        return true;
    }
    if projected_width.is_some_and(|width| viewport_value_eq(width, measured_rect.width)) {
        return false;
    }
    !viewport_value_eq(previous.viewport_width, measured_rect.width)
}

fn viewport_value_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

pub(super) fn measured_viewport_for_scrollbar_axis(
    bounds: gpui::Bounds<gpui::Pixels>,
    content_width: f32,
    content_height: f32,
    axis: ItemViewScrollbarAxis,
) -> MeasuredViewport {
    let wrapper_width = normalize_viewport_extent(bounds.size.width.as_f32());
    let wrapper_height = normalize_viewport_extent(bounds.size.height.as_f32());
    let (width, height) = match axis {
        ItemViewScrollbarAxis::Horizontal => (
            wrapper_width,
            normalize_viewport_extent(
                (wrapper_height - ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT).max(1.0),
            ),
        ),
        ItemViewScrollbarAxis::Vertical => (
            normalize_viewport_extent(
                (wrapper_width - ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT).max(1.0),
            ),
            wrapper_height,
        ),
    };
    let (max_scroll_x, max_scroll_y) = match axis {
        ItemViewScrollbarAxis::Horizontal => ((content_width - width).max(0.0), 0.0),
        ItemViewScrollbarAxis::Vertical => (0.0, (content_height - height).max(0.0)),
    };
    MeasuredViewport {
        rect: ViewRect {
            x: bounds.origin.x.as_f32(),
            y: bounds.origin.y.as_f32(),
            width,
            height,
        },
        max_scroll_x,
        max_scroll_y,
    }
}

pub(crate) fn content_point_from_window_position(
    geometry: PaneViewportGeometry,
    view: &ViewState,
    position: gpui::Point<gpui::Pixels>,
) -> Option<ViewPoint> {
    let window_point = ViewPoint {
        x: position.x.as_f32(),
        y: position.y.as_f32(),
    };
    if !geometry.window_rect.contains(window_point) {
        return None;
    }
    let local_x = window_point.x - geometry.window_rect.x;
    let local_y = window_point.y - geometry.window_rect.y;
    Some(ViewPoint {
        x: local_x + view.scroll_x,
        y: local_y + view.scroll_y,
    })
}

pub(crate) fn clamped_content_point_from_window_position(
    geometry: PaneViewportGeometry,
    view: &ViewState,
    position: gpui::Point<gpui::Pixels>,
) -> ViewPoint {
    let local_x =
        (position.x.as_f32() - geometry.window_rect.x).clamp(0.0, geometry.window_rect.width);
    let local_y =
        (position.y.as_f32() - geometry.window_rect.y).clamp(0.0, geometry.window_rect.height);
    ViewPoint {
        x: local_x + view.scroll_x,
        y: local_y + view.scroll_y,
    }
}

pub(crate) fn pane_at_window_position(
    pane_ids: &[PaneId],
    geometries: &HashMap<PaneId, PaneViewportGeometry>,
    position: gpui::Point<gpui::Pixels>,
) -> Option<PaneId> {
    let window_point = ViewPoint {
        x: position.x.as_f32(),
        y: position.y.as_f32(),
    };
    pane_ids.iter().copied().find(|pane_id| {
        geometries
            .get(pane_id)
            .is_some_and(|geometry| geometry.window_rect.contains(window_point))
    })
}

pub(super) fn file_grid_viewport_shell(
    pane_id: PaneId,
    _drop_target: bool,
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let shell = div()
        .id(format!("items-viewport-{}", pane_id.0))
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .bg(rgba(0x00000000))
        .occlude()
        .overflow_hidden()
        .on_scroll_wheel(
            cx.listener(move |this, event: &gpui::ScrollWheelEvent, _window, cx| {
                handle_file_grid_wheel(this, pane_id, event, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                handle_pane_navigation_mouse_down(this, pane_id, NavigationDirection::Back);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                handle_pane_navigation_mouse_down(this, pane_id, NavigationDirection::Forward);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(hit) = this.item_at_window_position(pane_id, event.position) {
                    if handle_item_mouse_down(this, pane_id, hit.path, hit.is_dir, mode, event, cx)
                    {
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }

                let pressed = this.press_rubber_band_from_window_if_blank(pane_id, event.position);
                cx.stop_propagation();
                if pressed {
                    cx.notify();
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, _event: &gpui::MouseUpEvent, _window, cx| {
                this.finish_rubber_band(pane_id);
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |this, _event: &gpui::MouseUpEvent, _window, cx| {
                this.finish_rubber_band(pane_id);
                cx.notify();
            }),
        )
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() && this.handle_blank_click(pane_id, event.position()) {
                    cx.notify();
                }
                if event.standard_click() {
                    cx.stop_propagation();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                let shown = if let Some(hit) = this.item_at_window_position(pane_id, event.position)
                {
                    this.show_item_context_menu(pane_id, hit.path, hit.is_dir, event.position)
                } else {
                    this.show_blank_context_menu_if_blank(pane_id, event.position)
                };
                cx.stop_propagation();
                if shown {
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Middle,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if !matches!(mode, FileGridMode::Manager) {
                    return;
                }
                if let Some(hit) = this.item_at_window_position(pane_id, event.position) {
                    if hit.is_dir {
                        this.paste_primary_into_directory(pane_id, hit.path, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                } else if this.paste_primary_into_pane_if_blank(pane_id, event.position, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .on_drag(RubberBandDrag { pane_id }, |_, _, _, cx| cx.new(|_| Empty))
        .on_drag_move::<RubberBandDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<RubberBandDrag>, _window, cx| {
                if !this.rubber_band_active_for_pane(pane_id) {
                    if this.activate_pending_rubber_band_from_window(pane_id, event.event.position)
                    {
                        cx.stop_propagation();
                        cx.notify();
                    }
                    return;
                }
                if this.update_rubber_band_from_window(pane_id, event.event.position) {
                    cx.stop_propagation();
                    cx.notify();
                }
            },
        ))
        .on_drop::<RubberBandDrag>(cx.listener(
            move |this, _drag: &RubberBandDrag, _window, cx| {
                this.finish_rubber_band(pane_id);
                cx.notify();
            },
        ));
    install_file_grid_path_drop_shell(shell, pane_id, cx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    fn test_geometry() -> PaneViewportGeometry {
        PaneViewportGeometry {
            window_rect: ViewRect {
                x: 100.0,
                y: 50.0,
                width: 300.0,
                height: 200.0,
            },
        }
    }

    #[test]
    fn content_point_from_window_position_accounts_for_origin_and_scroll() {
        let view = ViewState {
            scroll_x: 12.0,
            scroll_y: 8.0,
            ..ViewState::default()
        };

        assert_eq!(
            content_point_from_window_position(test_geometry(), &view, point(px(120.0), px(70.0))),
            Some(ViewPoint { x: 32.0, y: 28.0 })
        );
        assert_eq!(
            content_point_from_window_position(test_geometry(), &view, point(px(420.0), px(70.0))),
            None
        );
    }

    #[test]
    fn clamped_content_point_from_window_position_clamps_to_viewport() {
        let view = ViewState {
            scroll_x: 12.0,
            scroll_y: 8.0,
            ..ViewState::default()
        };

        assert_eq!(
            clamped_content_point_from_window_position(
                test_geometry(),
                &view,
                point(px(1000.0), px(900.0))
            ),
            ViewPoint { x: 312.0, y: 208.0 }
        );
        assert_eq!(
            clamped_content_point_from_window_position(
                test_geometry(),
                &view,
                point(px(90.0), px(40.0))
            ),
            ViewPoint { x: 12.0, y: 8.0 }
        );
    }

    #[test]
    fn pane_at_window_position_uses_pane_order_for_hits() {
        let panes = [PaneId(2), PaneId(1)];
        let geometries = HashMap::from([
            (
                PaneId(1),
                PaneViewportGeometry {
                    window_rect: ViewRect {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 100.0,
                    },
                },
            ),
            (
                PaneId(2),
                PaneViewportGeometry {
                    window_rect: ViewRect {
                        x: 50.0,
                        y: 0.0,
                        width: 100.0,
                        height: 100.0,
                    },
                },
            ),
        ]);

        assert_eq!(
            pane_at_window_position(&panes, &geometries, point(px(75.0), px(50.0))),
            Some(PaneId(2))
        );
        assert_eq!(
            pane_at_window_position(&panes, &geometries, point(px(25.0), px(50.0))),
            Some(PaneId(1))
        );
        assert_eq!(
            pane_at_window_position(&panes, &geometries, point(px(250.0), px(50.0))),
            None
        );
    }
}
