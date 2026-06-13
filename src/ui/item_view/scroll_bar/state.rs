use fika_core::normalize_viewport_extent;
use gpui::ScrollDelta;

const ITEM_VIEW_SCROLLBAR_MIN_THUMB_WIDTH: f32 = 24.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemViewScrollPress {
    None,
    Thumb,
    PageBackward,
    PageForward,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ItemViewScrollMetrics {
    pub(super) scroll_x: f32,
    pub(super) handle_left: f32,
    pub(super) handle_width: f32,
    pub(super) max_scroll_x: f32,
    pub(super) page_step: f32,
    pub(super) track_width: f32,
}

impl ItemViewScrollMetrics {
    pub(super) fn from_extents(
        content_width: f32,
        viewport_width: f32,
        scroll_x: f32,
        track_width: f32,
    ) -> Option<Self> {
        let content_width = content_width.max(0.0);
        let viewport_width = normalize_viewport_extent(viewport_width).max(0.0);
        let track_width = normalize_viewport_extent(track_width).floor().max(0.0);
        let max_scroll_x = (content_width - viewport_width).floor().max(0.0);
        if content_width <= 0.0
            || viewport_width <= 0.0
            || track_width <= 0.0
            || max_scroll_x <= 0.0
        {
            return None;
        }

        let page_step = viewport_width.floor().max(1.0);
        let handle_width = (viewport_width / content_width * track_width)
            .clamp(
                ITEM_VIEW_SCROLLBAR_MIN_THUMB_WIDTH.min(track_width),
                track_width,
            )
            .floor()
            .max(1.0);
        let available = (track_width - handle_width).max(0.0);
        let scroll_x = scroll_x.clamp(0.0, max_scroll_x);
        let handle_left = if available > 0.0 {
            (scroll_x / max_scroll_x * available).round()
        } else {
            0.0
        };

        Some(Self {
            scroll_x,
            handle_left: handle_left.clamp(0.0, available),
            handle_width,
            max_scroll_x,
            page_step,
            track_width,
        })
    }

    pub(super) fn press_kind(self, local_x: f32) -> ItemViewScrollPress {
        if !(0.0..=self.track_width).contains(&local_x) {
            return ItemViewScrollPress::None;
        }
        if local_x < self.handle_left {
            ItemViewScrollPress::PageBackward
        } else if local_x >= self.handle_left + self.handle_width {
            ItemViewScrollPress::PageForward
        } else {
            ItemViewScrollPress::Thumb
        }
    }

    pub(super) fn scroll_x_after_page_press(self, press: ItemViewScrollPress) -> Option<f32> {
        match press {
            ItemViewScrollPress::PageBackward => {
                Some((self.scroll_x - self.page_step).clamp(0.0, self.max_scroll_x))
            }
            ItemViewScrollPress::PageForward => {
                Some((self.scroll_x + self.page_step).clamp(0.0, self.max_scroll_x))
            }
            ItemViewScrollPress::None | ItemViewScrollPress::Thumb => None,
        }
    }

    pub(super) fn thumb_drag_session(self, local_x: f32) -> ItemViewScrollDragSession {
        ItemViewScrollDragSession {
            grab_x: (local_x - self.handle_left).clamp(0.0, self.handle_width),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ItemViewScrollDragSession {
    grab_x: f32,
}

impl ItemViewScrollDragSession {
    pub(super) fn scroll_x_for_local_x(self, local_x: f32, metrics: ItemViewScrollMetrics) -> f32 {
        let available = (metrics.track_width - metrics.handle_width).max(0.0);
        if available <= 0.0 || metrics.max_scroll_x <= 0.0 {
            return 0.0;
        }
        let handle_left = (local_x - self.grab_x).clamp(0.0, available);
        (handle_left / available * metrics.max_scroll_x)
            .round()
            .clamp(0.0, metrics.max_scroll_x)
    }
}

pub(super) fn item_view_wheel_delta(delta: ScrollDelta, single_step: f32) -> f32 {
    match delta {
        ScrollDelta::Pixels(delta) => -(delta.x.as_f32() + delta.y.as_f32()),
        ScrollDelta::Lines(delta) => -(delta.x + delta.y) * single_step,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_use_visible_track_width() {
        let metrics = ItemViewScrollMetrics::from_extents(1000.0, 250.0, 125.0, 200.0).unwrap();

        assert_eq!(metrics.track_width, 200.0);
        assert_eq!(metrics.handle_width, 50.0);
        assert_eq!(metrics.max_scroll_x, 750.0);
        assert_eq!(metrics.page_step, 250.0);
        assert_eq!(metrics.handle_left, 25.0);
    }

    #[test]
    fn thumb_drag_maps_local_position_to_scroll_offset() {
        let metrics = ItemViewScrollMetrics::from_extents(1000.0, 200.0, 0.0, 200.0).unwrap();
        let drag = metrics.thumb_drag_session(metrics.handle_left + metrics.handle_width / 2.0);

        assert_eq!(
            drag.scroll_x_for_local_x(metrics.handle_width / 2.0, metrics),
            0.0
        );
        assert_eq!(
            drag.scroll_x_for_local_x(metrics.track_width, metrics),
            metrics.max_scroll_x
        );
    }

    #[test]
    fn page_press_scrolls_by_page_step_without_starting_thumb_drag() {
        let metrics = ItemViewScrollMetrics::from_extents(1000.0, 200.0, 400.0, 200.0).unwrap();

        assert_eq!(metrics.press_kind(0.0), ItemViewScrollPress::PageBackward);
        assert_eq!(
            metrics.scroll_x_after_page_press(ItemViewScrollPress::PageBackward),
            Some(200.0)
        );
        assert_eq!(
            metrics.scroll_x_after_page_press(ItemViewScrollPress::PageForward),
            Some(600.0)
        );
    }

    #[test]
    fn wheel_lines_use_dolphin_single_step() {
        assert_eq!(
            item_view_wheel_delta(ScrollDelta::Lines(gpui::point(0.0, -3.0)), 30.0),
            90.0
        );
    }
}
