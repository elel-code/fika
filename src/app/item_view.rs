use crate::app::geometry::{
    PATH_BAR_HEIGHT, STATUS_BAR_HEIGHT, active_main_pane_width, icon_cell_width, icon_row_height,
    inactive_main_pane_width, main_pane_bounds, search_panel_height,
};
use crate::app::selection::filtered_entry_at_for_slot;
use crate::app::state::AppState;
use crate::{AppWindow, FileEntry};
use slint::ComponentHandle;

const ITEM_VIEW_PADDING: f32 = 14.0;
const TILE_TRAILING_GAP: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewLayout {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) viewport_x: f32,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

impl ItemViewLayout {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        viewport_x: f32,
        rows_per_column: usize,
        cell_width: f32,
        row_height: f32,
        padding: f32,
    ) -> Self {
        Self {
            x,
            y,
            width: width.max(1.0),
            height: height.max(1.0),
            viewport_x: viewport_x.max(0.0),
            rows_per_column: rows_per_column.max(1),
            cell_width: cell_width.max(1.0),
            row_height: row_height.max(1.0),
            padding: padding.max(0.0),
        }
    }

    pub(crate) fn from_ui(ui: &AppWindow, state: &AppState, slot: i32) -> Option<Self> {
        let pane_state = state.panes.pane_for_slot(slot)?;
        if slot != 0 && !ui.get_split_view_open() {
            return None;
        }

        let window_size = ui.window().size().to_logical(ui.window().scale_factor());
        let pane = main_pane_bounds(
            ui.get_sidebar_width_px(),
            window_size.width,
            window_size.height,
        );
        let main_width = (pane.right - pane.left).max(1.0);
        let (x, width) = pane_slot_geometry(
            pane.left,
            main_width,
            ui.get_split_view_open(),
            ui.get_split_pane_ratio(),
            slot,
        )?;
        let search_height = if state.panes.focused_slot() == slot {
            search_panel_height(
                ui.get_search_bar_open(),
                ui.get_search_query().as_str(),
                ui.get_search_kind_filter(),
                ui.get_search_modified_filter(),
                ui.get_search_size_filter(),
                width,
            )
        } else {
            0.0
        };
        let cell_width = icon_cell_width(ui.get_icon_zoom_level());
        let row_height = icon_row_height(ui.get_icon_zoom_level());
        let height =
            (pane.bottom - pane.top - PATH_BAR_HEIGHT - STATUS_BAR_HEIGHT - search_height).max(1.0);
        let available_grid_height = (height - 2.0 * ITEM_VIEW_PADDING).max(row_height);
        let rows_per_column = (available_grid_height / row_height).floor().max(1.0) as usize;

        Some(Self::new(
            x,
            pane.top + PATH_BAR_HEIGHT + search_height,
            width,
            height,
            pane_state.view.viewport_x,
            rows_per_column,
            cell_width,
            row_height,
            ITEM_VIEW_PADDING,
        ))
    }

    pub(crate) fn index_at_point(self, x: f32, y: f32) -> Option<usize> {
        if x < self.x || x > self.x + self.width || y < self.y || y > self.y + self.height {
            return None;
        }

        let local_x = x - self.x - self.padding + self.viewport_x;
        let local_y = y - self.y - self.padding;
        if local_x < 0.0 || local_y < 0.0 {
            return None;
        }

        let column = (local_x / self.cell_width).floor() as usize;
        let row = (local_y / self.row_height).floor() as usize;
        if row >= self.rows_per_column {
            return None;
        }

        let inside_tile_x = local_x - column as f32 * self.cell_width;
        if inside_tile_x > (self.cell_width - TILE_TRAILING_GAP).max(1.0) {
            return None;
        }

        Some(column * self.rows_per_column + row)
    }
}

fn pane_slot_geometry(
    main_left: f32,
    main_width: f32,
    split_open: bool,
    split_pane_ratio: f32,
    slot: i32,
) -> Option<(f32, f32)> {
    match slot {
        0 => Some((
            main_left,
            active_main_pane_width(main_width, split_open, split_pane_ratio).max(1.0),
        )),
        1 if split_open => {
            let width = inactive_main_pane_width(main_width, split_open, split_pane_ratio).max(1.0);
            Some((main_left + main_width - width, width))
        }
        _ => None,
    }
}

pub(crate) fn entry_at_pane_point(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
) -> Option<FileEntry> {
    let layout = ItemViewLayout::from_ui(ui, state, slot)?;
    let index = layout.index_at_point(x, y)?;
    filtered_entry_at_for_slot(state, slot, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_view_layout_hit_test_uses_column_first_order_and_viewport() {
        let layout = ItemViewLayout::new(100.0, 50.0, 250.0, 220.0, 300.0, 2, 100.0, 100.0, 10.0);

        assert_eq!(layout.index_at_point(115.0, 65.0), Some(6));
        assert_eq!(layout.index_at_point(115.0, 165.0), Some(7));
    }

    #[test]
    fn item_view_layout_hit_test_rejects_padding_and_cell_gap() {
        let layout = ItemViewLayout::new(100.0, 50.0, 250.0, 220.0, 0.0, 2, 100.0, 100.0, 10.0);

        assert_eq!(layout.index_at_point(105.0, 65.0), None);
        assert_eq!(layout.index_at_point(199.0, 65.0), None);
        assert_eq!(layout.index_at_point(115.0, 265.0), None);
    }

    #[test]
    fn pane_slot_geometry_matches_split_ratio_model() {
        assert_eq!(
            pane_slot_geometry(280.0, 900.0, false, 0.5, 0),
            Some((280.0, 900.0))
        );
        assert_eq!(pane_slot_geometry(280.0, 900.0, false, 0.5, 1), None);
        assert_eq!(
            pane_slot_geometry(280.0, 900.0, true, 0.5, 0),
            Some((280.0, 449.0))
        );
        assert_eq!(
            pane_slot_geometry(280.0, 900.0, true, 0.5, 1),
            Some((730.0, 450.0))
        );
    }
}
