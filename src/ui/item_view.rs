mod scroll_bar;
mod scroll_state;

use fika_core::ViewMode;

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
}
