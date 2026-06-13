use fika_core::{
    HorizontalScrollBarLayout, ViewPoint, ViewRect, horizontal_scroll_bar_layout,
    normalize_viewport_extent,
};
use gpui::{Bounds, Pixels};

pub(crate) const SCROLLBAR_THICKNESS: f32 = 12.0;
pub(crate) const SCROLLBAR_MIN_HANDLE_WIDTH: f32 = 36.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HorizontalScrollBarTrack {
    pub(crate) window_rect: ViewRect,
    pub(crate) content_width: f32,
    pub(crate) scroll_x: f32,
}

pub(super) fn horizontal_scrollbar_track(
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
    })
}

pub(super) fn scroll_x_for_scrollbar_drag(
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

pub(super) fn scroll_x_for_scrollbar_drag_start(
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

pub(super) fn scrollbar_track_local_point(
    window_rect: ViewRect,
    position: gpui::Point<gpui::Pixels>,
) -> Option<ViewPoint> {
    let window_rect = normalized_scrollbar_window_rect(window_rect);
    let point = ViewPoint {
        x: position.x.as_f32(),
        y: position.y.as_f32(),
    };
    if !window_rect.contains(point) {
        return None;
    }
    Some(ViewPoint {
        x: point.x - window_rect.x,
        y: point.y - window_rect.y,
    })
}

pub(super) fn normalized_scrollbar_window_rect(rect: ViewRect) -> ViewRect {
    ViewRect {
        x: rect.x,
        y: rect.y,
        width: normalize_viewport_extent(rect.width).max(0.0),
        height: normalize_viewport_extent(rect.height).max(0.0),
    }
}

#[cfg(test)]
pub(super) fn scrollbar_drag_track_rect(track_width: f32) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: track_width,
        height: SCROLLBAR_THICKNESS,
    }
}

pub(super) fn scrollbar_local_track_rect(window_rect: ViewRect) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: window_rect.width,
        height: window_rect.height,
    }
}

pub(super) fn scrollbar_point_is_in_track(local: ViewPoint, track_rect: ViewRect) -> bool {
    track_rect.contains(local)
}

pub(super) fn scrollbar_track_x_from_window(
    rect: ViewRect,
    position: gpui::Point<gpui::Pixels>,
) -> f32 {
    (position.x.as_f32() - rect.x).clamp(0.0, rect.width)
}

pub(super) fn scrollbar_window_rect_from_bounds(bounds: Bounds<Pixels>) -> ViewRect {
    ViewRect {
        x: bounds.origin.x.as_f32(),
        y: bounds.origin.y.as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
    }
}

pub(super) fn scrollbar_track_x_from_local(local: ViewPoint, track_rect: ViewRect) -> f32 {
    local.x.clamp(0.0, track_rect.width)
}

pub(super) fn scrollbar_track_width(track_rect: ViewRect) -> f32 {
    normalize_viewport_extent(track_rect.width).max(0.0)
}

pub(super) fn scrollbar_drag_start_from_local(
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

pub(super) fn scroll_bar_layout_for_bounds(
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
    use super::{
        SCROLLBAR_THICKNESS, scroll_x_for_scrollbar_drag, scroll_x_for_scrollbar_drag_start,
        scrollbar_track_local_point,
    };
    use fika_core::ViewRect;
    use gpui::{point, px};

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

    #[test]
    fn scrollbar_window_hit_test_uses_live_track_rect() {
        let rect = ViewRect {
            x: 100.0,
            y: 400.0,
            width: 240.0,
            height: SCROLLBAR_THICKNESS,
        };

        assert_eq!(
            scrollbar_track_local_point(rect, point(px(180.0), px(406.0))),
            Some(fika_core::ViewPoint { x: 80.0, y: 6.0 })
        );
        assert_eq!(
            scrollbar_track_local_point(rect, point(px(180.0), px(399.0))),
            None
        );
        assert_eq!(
            scrollbar_track_local_point(rect, point(px(341.0), px(406.0))),
            None
        );
    }
}
