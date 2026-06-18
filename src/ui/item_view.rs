mod scroll_bar;
mod scroll_state;

use fika_core::ViewMode;
use gpui::{ScrollDelta, px};

pub(crate) use scroll_bar::{
    ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT, ItemViewScrollbarAxis, item_view_scrollbar_container,
};
pub(crate) use scroll_state::{ItemViewScrollState, ItemViewScrollViewSnapshot};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewWindowResizePrime {
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) delta_width: f32,
    pub(crate) delta_height: f32,
    pub(crate) width_changed: bool,
    pub(crate) height_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewWindowResizePrimeResult {
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
    pub(crate) resize: Option<ItemViewWindowResizePrime>,
}

impl ItemViewWindowResizePrime {
    pub(crate) fn apply_width_delta(self, width: f32) -> f32 {
        apply_window_resize_delta(width, self.delta_width)
    }

    pub(crate) fn apply_height_delta(self, height: f32) -> f32 {
        apply_window_resize_delta(height, self.delta_height)
    }
}

pub(crate) fn view_mode_uses_horizontal_item_scrollbar(view_mode: ViewMode) -> bool {
    matches!(view_mode, ViewMode::Compact)
}

pub(crate) fn projected_item_viewport_width_for_pane_width(
    pane_width: f32,
    view_mode: ViewMode,
    horizontal_border_extent: f32,
) -> f32 {
    let scrollbar_extent = if view_mode_uses_horizontal_item_scrollbar(view_mode) {
        0.0
    } else {
        ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT
    };
    fika_core::normalize_viewport_extent(
        (pane_width - horizontal_border_extent - scrollbar_extent).max(1.0),
    )
}

pub(crate) fn viewport_extents_after_view_mode_axis_change(
    viewport_width: f32,
    viewport_height: f32,
    previous_mode: ViewMode,
    next_mode: ViewMode,
) -> Option<(f32, f32)> {
    let previous_horizontal = view_mode_uses_horizontal_item_scrollbar(previous_mode);
    let next_horizontal = view_mode_uses_horizontal_item_scrollbar(next_mode);
    if previous_horizontal == next_horizontal {
        return None;
    }

    let extent = ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT;
    let (viewport_width, viewport_height) = if next_horizontal {
        (viewport_width + extent, viewport_height - extent)
    } else {
        (viewport_width - extent, viewport_height + extent)
    };
    Some((
        fika_core::normalize_viewport_extent(viewport_width),
        fika_core::normalize_viewport_extent(viewport_height),
    ))
}

pub(crate) fn viewport_height_after_filter_bar_visibility_change(
    viewport_height: f32,
    visible: bool,
    filter_bar_height: f32,
) -> f32 {
    let delta = if visible {
        -filter_bar_height
    } else {
        filter_bar_height
    };
    fika_core::normalize_viewport_extent(viewport_height + delta)
}

pub(crate) fn window_resize_viewport_prime(
    previous: Option<(f32, f32)>,
    viewport_width: f32,
    viewport_height: f32,
) -> ItemViewWindowResizePrimeResult {
    let viewport_width = fika_core::normalize_viewport_extent(viewport_width);
    let viewport_height = fika_core::normalize_viewport_extent(viewport_height);
    let resize = previous.and_then(|(previous_width, previous_height)| {
        let delta_width = viewport_width - previous_width;
        let delta_height = viewport_height - previous_height;
        let width_changed = viewport_delta_changed(delta_width);
        let height_changed = viewport_delta_changed(delta_height);
        (width_changed || height_changed).then_some(ItemViewWindowResizePrime {
            viewport_width,
            viewport_height,
            delta_width,
            delta_height,
            width_changed,
            height_changed,
        })
    });

    ItemViewWindowResizePrimeResult {
        viewport_width,
        viewport_height,
        resize,
    }
}

pub(crate) fn wheel_scroll_delta_for_view_mode(
    view_mode: ViewMode,
    delta: ScrollDelta,
) -> (f32, f32) {
    let delta = delta.pixel_delta(px(20.0));
    let x = delta.x.as_f32();
    let y = delta.y.as_f32();
    match view_mode {
        ViewMode::Compact => {
            let primary = if x.abs() > y.abs() { x } else { y };
            (-primary, 0.0)
        }
        ViewMode::Icons | ViewMode::Details => (0.0, -y),
    }
}

fn apply_window_resize_delta(extent: f32, delta: f32) -> f32 {
    fika_core::normalize_viewport_extent(extent + delta)
}

fn viewport_delta_changed(delta: f32) -> bool {
    delta.abs() >= 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_item_viewport_width_accounts_for_scrollbar_axis() {
        assert_eq!(
            projected_item_viewport_width_for_pane_width(300.0, ViewMode::Compact, 2.0),
            298.0
        );
        assert_eq!(
            projected_item_viewport_width_for_pane_width(300.0, ViewMode::Icons, 2.0),
            284.0
        );
        assert_eq!(
            projected_item_viewport_width_for_pane_width(300.0, ViewMode::Details, 2.0),
            284.0
        );
    }

    #[test]
    fn viewport_extents_shift_when_view_mode_scrollbar_axis_changes() {
        assert_eq!(
            viewport_extents_after_view_mode_axis_change(
                400.0,
                300.0,
                ViewMode::Icons,
                ViewMode::Compact
            ),
            Some((414.0, 286.0))
        );
        assert_eq!(
            viewport_extents_after_view_mode_axis_change(
                400.0,
                300.0,
                ViewMode::Compact,
                ViewMode::Details
            ),
            Some((386.0, 314.0))
        );
        assert_eq!(
            viewport_extents_after_view_mode_axis_change(
                400.0,
                300.0,
                ViewMode::Icons,
                ViewMode::Details
            ),
            None
        );
    }

    #[test]
    fn viewport_height_tracks_filter_bar_visibility() {
        assert_eq!(
            viewport_height_after_filter_bar_visibility_change(360.0, true, 35.0),
            325.0
        );
        assert_eq!(
            viewport_height_after_filter_bar_visibility_change(325.0, false, 35.0),
            360.0
        );
        assert_eq!(
            viewport_height_after_filter_bar_visibility_change(20.0, true, 35.0),
            1.0
        );
    }

    #[test]
    fn window_resize_prime_normalizes_and_reports_resize_deltas() {
        assert_eq!(
            window_resize_viewport_prime(None, 1024.9, 768.1),
            ItemViewWindowResizePrimeResult {
                viewport_width: 1024.0,
                viewport_height: 768.0,
                resize: None,
            }
        );

        let result = window_resize_viewport_prime(Some((1024.0, 768.0)), 1224.0, 918.0);
        assert_eq!(
            result,
            ItemViewWindowResizePrimeResult {
                viewport_width: 1224.0,
                viewport_height: 918.0,
                resize: Some(ItemViewWindowResizePrime {
                    viewport_width: 1224.0,
                    viewport_height: 918.0,
                    delta_width: 200.0,
                    delta_height: 150.0,
                    width_changed: true,
                    height_changed: true,
                }),
            }
        );
        let resize = result.resize.unwrap();
        assert_eq!(resize.apply_width_delta(620.0), 820.0);
        assert_eq!(resize.apply_height_delta(360.0), 510.0);

        assert_eq!(
            window_resize_viewport_prime(Some((1024.0, 768.0)), 1024.0, 768.0).resize,
            None
        );
    }

    #[test]
    fn compact_wheel_scroll_maps_vertical_wheel_to_horizontal_axis() {
        assert_eq!(
            wheel_scroll_delta_for_view_mode(
                ViewMode::Compact,
                ScrollDelta::Lines(gpui::point(0.0, -3.0))
            ),
            (60.0, 0.0)
        );
        assert_eq!(
            wheel_scroll_delta_for_view_mode(
                ViewMode::Details,
                ScrollDelta::Lines(gpui::point(0.0, -3.0))
            ),
            (0.0, 60.0)
        );
        assert_eq!(
            wheel_scroll_delta_for_view_mode(
                ViewMode::Icons,
                ScrollDelta::Lines(gpui::point(4.0, 0.0))
            ),
            (0.0, -0.0)
        );
        assert_eq!(
            wheel_scroll_delta_for_view_mode(
                ViewMode::Details,
                ScrollDelta::Lines(gpui::point(4.0, 0.0))
            ),
            (0.0, -0.0)
        );
    }
}
