use crate::app::geometry::{
    MainItemViewLayout, PATH_BAR_HEIGHT, STATUS_BAR_HEIGHT, active_main_pane_width,
    inactive_main_pane_width, main_pane_bounds, search_panel_height,
};
use crate::app::selection::{filtered_entry_at_for_slot, filtered_entry_count_for_slot};
use crate::app::state::AppState;
use crate::{AppWindow, FileEntry};
use slint::ComponentHandle;
use std::ops::Range;

const SELECTION_DRAG_THRESHOLD: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewLayout {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) viewport_x: f32,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionRect {
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
    pub(crate) rows_per_column: i32,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

impl SelectionRect {
    pub(crate) fn candidate_range(self, visible_count: usize) -> Range<usize> {
        if visible_count == 0 {
            return 0..0;
        }

        let rows_per_column = self.rows_per_column.max(1) as usize;
        let cell_width = self.cell_width.max(1.0);
        let column_width = self.column_width.max(1.0);
        let column_offset = self.column_offset.max(0.0);

        let first_column = ((self.x1 - self.padding - column_offset - cell_width) / column_width)
            .floor()
            .max(0.0) as usize;
        let last_column = ((self.x2 - self.padding - column_offset) / column_width)
            .floor()
            .max(0.0) as usize;

        let start = first_column
            .saturating_mul(rows_per_column)
            .min(visible_count);
        let end = ((last_column + 1).saturating_mul(rows_per_column)).min(visible_count);
        start..end.max(start)
    }

    pub(crate) fn intersects_index(self, index: usize) -> bool {
        let rows_per_column = self.rows_per_column.max(1) as usize;
        let column = index / rows_per_column;
        let row = index % rows_per_column;
        let tile_x1 =
            self.padding + self.column_offset.max(0.0) + column as f32 * self.column_width.max(1.0);
        let tile_y1 = self.padding + row as f32 * self.row_height;
        let tile_x2 = tile_x1 + self.cell_width.max(1.0);
        let tile_y2 = tile_y1 + self.row_height.max(1.0);

        RectBounds::new(self.x1, self.y1, self.x2, self.y2)
            .intersects(RectBounds::new(tile_x1, tile_y1, tile_x2, tile_y2))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewInputMetrics {
    pub(crate) rows_per_column: i32,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

impl ItemViewInputMetrics {
    pub(crate) fn new(
        rows_per_column: i32,
        cell_width: f32,
        column_width: f32,
        column_offset: f32,
        row_height: f32,
        padding: f32,
    ) -> Self {
        Self {
            rows_per_column: rows_per_column.max(1),
            cell_width: cell_width.max(1.0),
            column_width: column_width.max(1.0),
            column_offset: column_offset.max(0.0),
            row_height: row_height.max(1.0),
            padding: padding.max(0.0),
        }
    }

    fn selection_rect(self, gesture: SelectionRectGesture) -> SelectionRect {
        let (x1, x2) = ordered_pair(gesture.start_x, gesture.current_x);
        let (y1, y2) = ordered_pair(gesture.start_y, gesture.current_y);
        SelectionRect {
            x1,
            y1,
            x2,
            y2,
            rows_per_column: self.rows_per_column,
            cell_width: self.cell_width,
            column_width: self.column_width,
            column_offset: self.column_offset,
            row_height: self.row_height,
            padding: self.padding,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ItemViewInputState {
    selection_rect: Option<SelectionRectGesture>,
}

impl ItemViewInputState {
    pub(crate) fn press_blank(
        &mut self,
        x: f32,
        y: f32,
        metrics: ItemViewInputMetrics,
        toggle: bool,
    ) {
        self.selection_rect = Some(SelectionRectGesture {
            start_x: x,
            start_y: y,
            current_x: x,
            current_y: y,
            metrics,
            toggle,
            active: false,
        });
    }

    pub(crate) fn move_blank(&mut self, x: f32, y: f32) -> bool {
        let Some(mut gesture) = self.selection_rect else {
            return false;
        };
        gesture.current_x = x;
        gesture.current_y = y;
        gesture.active |= selection_drag_threshold_crossed(gesture.start_x, gesture.start_y, x, y);
        self.selection_rect = Some(gesture);
        gesture.active
    }

    pub(crate) fn release_blank(&mut self, x: f32, y: f32) -> ItemViewReleaseAction {
        let Some(mut gesture) = self.selection_rect.take() else {
            return ItemViewReleaseAction::None;
        };
        gesture.current_x = x;
        gesture.current_y = y;
        gesture.active |= selection_drag_threshold_crossed(gesture.start_x, gesture.start_y, x, y);
        if gesture.active {
            ItemViewReleaseAction::SelectRect {
                rect: gesture.metrics.selection_rect(gesture),
                toggle: gesture.toggle,
            }
        } else {
            ItemViewReleaseAction::ClearSelection
        }
    }

    pub(crate) fn cancel_blank(&mut self) {
        self.selection_rect = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SelectionRectGesture {
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    metrics: ItemViewInputMetrics,
    toggle: bool,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ItemViewReleaseAction {
    None,
    ClearSelection,
    SelectRect { rect: SelectionRect, toggle: bool },
}

fn selection_drag_threshold_crossed(
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
) -> bool {
    (current_x - start_x).abs() > SELECTION_DRAG_THRESHOLD
        || (current_y - start_y).abs() > SELECTION_DRAG_THRESHOLD
}

fn ordered_pair(a: f32, b: f32) -> (f32, f32) {
    if a <= b { (a, b) } else { (b, a) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RectBounds {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl RectBounds {
    fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn intersects(self, other: Self) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }
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
        column_width: f32,
        column_offset: f32,
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
            column_width: column_width.max(1.0),
            column_offset: column_offset.max(0.0),
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
        let height =
            (pane.bottom - pane.top - PATH_BAR_HEIGHT - STATUS_BAR_HEIGHT - search_height).max(1.0);
        let layout = MainItemViewLayout::from_ui_for_pane_width_with_text_lines(
            ui,
            width,
            state.panes.focused_slot() == slot,
            pane_state.item_view_text_line_count(),
        );
        let compact_item_view =
            layout.compact_item_view(filtered_entry_count_for_slot(state, slot));

        Some(Self::new(
            x,
            pane.top + PATH_BAR_HEIGHT + search_height,
            width,
            height,
            pane_state.view.viewport_x,
            compact_item_view.rows_per_column,
            compact_item_view.cell_width,
            compact_item_view.column_width,
            compact_item_view.column_offset,
            compact_item_view.row_height,
            compact_item_view.padding,
        ))
    }

    pub(crate) fn index_at_point(self, x: f32, y: f32) -> Option<usize> {
        if x < self.x || x > self.x + self.width || y < self.y || y > self.y + self.height {
            return None;
        }

        let local_x = x - self.x - self.padding - self.column_offset + self.viewport_x;
        let local_y = y - self.y - self.padding;
        if local_x < 0.0 || local_y < 0.0 {
            return None;
        }

        let column = (local_x / self.column_width).floor() as usize;
        let row = (local_y / self.row_height).floor() as usize;
        if row >= self.rows_per_column {
            return None;
        }

        let inside_tile_x = local_x - column as f32 * self.column_width;
        if inside_tile_x > self.cell_width.max(1.0) {
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
    let index = item_index_at_pane_point(ui, state, slot, x, y)?;
    filtered_entry_at_for_slot(state, slot, index)
}

pub(crate) fn item_index_at_pane_point(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
) -> Option<usize> {
    let layout = ItemViewLayout::from_ui(ui, state, slot)?;
    layout.index_at_point(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_view_layout_hit_test_uses_column_first_order_and_viewport() {
        let layout = ItemViewLayout::new(
            100.0, 50.0, 250.0, 220.0, 300.0, 2, 100.0, 112.0, 10.0, 100.0, 10.0,
        );

        assert_eq!(layout.index_at_point(115.0, 65.0), Some(4));
        assert_eq!(layout.index_at_point(115.0, 165.0), Some(5));
    }

    #[test]
    fn item_view_layout_hit_test_rejects_padding_and_cell_gap() {
        let layout = ItemViewLayout::new(
            100.0, 50.0, 250.0, 220.0, 0.0, 2, 100.0, 112.0, 10.0, 100.0, 10.0,
        );

        assert_eq!(layout.index_at_point(105.0, 65.0), None);
        assert_eq!(layout.index_at_point(221.0, 65.0), None);
        assert_eq!(layout.index_at_point(115.0, 271.0), None);
    }

    #[test]
    fn item_view_controller_does_not_own_renderer_pipeline() {
        let item_view = include_str!("item_view.rs");
        let model_update = include_str!("model_update.rs");
        let renderer = include_str!("item_view_renderer.rs");
        let render_metrics = concat!("struct ", "ItemViewRenderMetrics");
        let render_plan = concat!("decorate_", "render_plan_with_metadata");
        let fallback_media = concat!("decorate_", "fallback_media");
        let row_token = concat!("struct ", "ItemViewRowToken");

        assert!(
            !item_view.contains(render_metrics)
                && !item_view.contains(render_plan)
                && !item_view.contains(fallback_media)
                && !item_view.contains(row_token)
                && model_update.contains(row_token)
                && renderer.contains(render_metrics)
                && renderer.contains(render_plan)
                && renderer.contains(fallback_media),
            "item_view.rs should stay focused on controller/input/hit-test; renderer projection belongs in item_view_renderer.rs and row reuse sidecars belong in model_update.rs"
        );
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

    #[test]
    fn selection_rect_uses_column_first_item_geometry() {
        let rect = SelectionRect {
            x1: 0.0,
            y1: 0.0,
            x2: 109.0,
            y2: 205.0,
            rows_per_column: 2,
            cell_width: 100.0,
            column_width: 112.0,
            column_offset: 10.0,
            row_height: 100.0,
            padding: 10.0,
        };

        assert!(rect.intersects_index(0));
        assert!(rect.intersects_index(1));
        assert!(!rect.intersects_index(2));
        assert_eq!(rect.candidate_range(4), 0..2);
    }

    #[test]
    fn selection_rect_candidate_range_limits_intersecting_columns() {
        let rect = SelectionRect {
            x1: 244.0,
            y1: 0.0,
            x2: 343.0,
            y2: 205.0,
            rows_per_column: 2,
            cell_width: 100.0,
            column_width: 112.0,
            column_offset: 10.0,
            row_height: 100.0,
            padding: 10.0,
        };

        assert_eq!(rect.candidate_range(20), 2..6);
        assert!(rect.intersects_index(4));
        assert!(rect.intersects_index(5));
        assert!(!rect.intersects_index(2));
        assert!(!rect.intersects_index(6));
    }

    #[test]
    fn item_view_input_turns_blank_click_into_clear_selection() {
        let mut input = ItemViewInputState::default();
        input.press_blank(
            10.0,
            20.0,
            ItemViewInputMetrics::new(3, 100.0, 112.0, 14.0, 50.0, 14.0),
            false,
        );

        assert!(!input.move_blank(14.0, 24.0));
        assert_eq!(
            input.release_blank(14.0, 24.0),
            ItemViewReleaseAction::ClearSelection
        );
    }

    #[test]
    fn item_view_input_turns_blank_drag_into_selection_rect() {
        let mut input = ItemViewInputState::default();
        input.press_blank(
            120.0,
            80.0,
            ItemViewInputMetrics::new(3, 100.0, 112.0, 14.0, 50.0, 14.0),
            true,
        );

        assert!(input.move_blank(40.0, 140.0));
        assert_eq!(
            input.release_blank(40.0, 140.0),
            ItemViewReleaseAction::SelectRect {
                rect: SelectionRect {
                    x1: 40.0,
                    y1: 80.0,
                    x2: 120.0,
                    y2: 140.0,
                    rows_per_column: 3,
                    cell_width: 100.0,
                    column_width: 112.0,
                    column_offset: 14.0,
                    row_height: 50.0,
                    padding: 14.0,
                },
                toggle: true,
            }
        );
    }

    #[test]
    fn item_view_input_cancel_drops_pending_blank_selection() {
        let mut input = ItemViewInputState::default();
        input.press_blank(
            10.0,
            20.0,
            ItemViewInputMetrics::new(3, 100.0, 112.0, 14.0, 50.0, 14.0),
            false,
        );

        input.cancel_blank();

        assert_eq!(
            input.release_blank(100.0, 120.0),
            ItemViewReleaseAction::None
        );
    }
}
