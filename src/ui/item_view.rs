mod scroll_bar;
mod scroll_state;

use fika_core::ViewMode;
use gpui::{ScrollDelta, px};

pub(crate) use scroll_bar::{
    ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT, ItemViewScrollbarAxis, item_view_scrollbar_container,
};
pub(crate) use scroll_state::{
    ItemViewScrollState, ItemViewScrollSync, ItemViewScrollSyncAction, scroll_sync_changes_view,
};

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
