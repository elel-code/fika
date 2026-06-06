use crate::app::geometry::{
    CompactItemViewLayout, PATH_BAR_HEIGHT, STATUS_BAR_HEIGHT, active_main_pane_width,
    inactive_main_pane_width, main_pane_bounds, search_panel_height,
};
use crate::app::selection::filtered_entry_at_for_slot;
use crate::app::state::AppState;
use crate::{AppWindow, FileEntry};
use slint::ComponentHandle;
use std::ops::Range;

const SELECTION_DRAG_THRESHOLD: f32 = 5.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewLayout {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) viewport_x: f32,
    pub(crate) layout: CompactItemViewLayout,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionRect {
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
    pub(crate) layout: CompactItemViewLayout,
}

impl SelectionRect {
    pub(crate) fn candidate_range(&self, visible_count: usize) -> Range<usize> {
        let range = self.layout.selection_candidate_range(self.x1, self.x2);
        range.start.min(visible_count)
            ..range
                .end
                .min(visible_count)
                .max(range.start.min(visible_count))
    }

    pub(crate) fn intersects_index(&self, index: usize) -> bool {
        self.layout
            .intersects_index(index, self.x1, self.y1, self.x2, self.y2)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ItemViewInputState {
    selection_rect: Option<SelectionRectGesture>,
    drag_source: Option<ItemViewDragSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewDragSource {
    path: String,
    is_dir: bool,
}

impl ItemViewDragSource {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn is_dir(&self) -> bool {
        self.is_dir
    }
}

impl ItemViewInputState {
    pub(crate) fn set_drag_source(&mut self, path: String, is_dir: bool) {
        self.drag_source = Some(ItemViewDragSource { path, is_dir });
    }

    pub(crate) fn clear_drag_source(&mut self) {
        self.drag_source = None;
    }

    pub(crate) fn drag_source(&self) -> Option<&ItemViewDragSource> {
        self.drag_source.as_ref()
    }

    pub(crate) fn press_blank(
        &mut self,
        x: f32,
        y: f32,
        layout: CompactItemViewLayout,
        toggle: bool,
    ) {
        self.clear_drag_source();
        self.selection_rect = Some(SelectionRectGesture {
            start_x: x,
            start_y: y,
            current_x: x,
            current_y: y,
            layout,
            toggle,
            active: false,
        });
    }

    pub(crate) fn move_blank(&mut self, x: f32, y: f32) -> bool {
        let Some(mut gesture) = self.selection_rect.take() else {
            return false;
        };
        gesture.current_x = x;
        gesture.current_y = y;
        gesture.active |= selection_drag_threshold_crossed(gesture.start_x, gesture.start_y, x, y);
        let active = gesture.active;
        self.selection_rect = Some(gesture);
        active
    }

    pub(crate) fn release_blank(&mut self, x: f32, y: f32) -> ItemViewReleaseAction {
        let Some(mut gesture) = self.selection_rect.take() else {
            return ItemViewReleaseAction::None;
        };
        gesture.current_x = x;
        gesture.current_y = y;
        gesture.active |= selection_drag_threshold_crossed(gesture.start_x, gesture.start_y, x, y);
        if gesture.active {
            let (x1, x2) = ordered_pair(gesture.start_x, gesture.current_x);
            let (y1, y2) = ordered_pair(gesture.start_y, gesture.current_y);
            ItemViewReleaseAction::SelectRect {
                rect: SelectionRect {
                    x1,
                    y1,
                    x2,
                    y2,
                    layout: gesture.layout,
                },
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

#[derive(Clone, Debug, PartialEq)]
struct SelectionRectGesture {
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    layout: CompactItemViewLayout,
    toggle: bool,
    active: bool,
}

#[derive(Clone, Debug, PartialEq)]
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

impl ItemViewLayout {
    pub(crate) fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        viewport_x: f32,
        layout: CompactItemViewLayout,
    ) -> Self {
        Self {
            x,
            y,
            width: width.max(1.0),
            height: height.max(1.0),
            viewport_x: viewport_x.max(0.0),
            layout,
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
        let search_panel_visible = pane_state.search.panel_visible();
        let search_height = search_panel_height(search_panel_visible, width);
        let height =
            (pane.bottom - pane.top - PATH_BAR_HEIGHT - STATUS_BAR_HEIGHT - search_height).max(1.0);
        let layout = pane_state.view.virtual_view.layout.clone()?;

        Some(Self::new(
            x,
            pane.top + PATH_BAR_HEIGHT + search_height,
            width,
            height,
            pane_state.view.viewport_x,
            layout,
        ))
    }

    pub(crate) fn index_at_point(&self, x: f32, y: f32) -> Option<usize> {
        if x < self.x || x > self.x + self.width || y < self.y || y > self.y + self.height {
            return None;
        }

        let local_x = x - self.x + self.viewport_x;
        let local_y = y - self.y;
        self.layout.index_at_content_point(local_x, local_y)
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
    use crate::app::geometry::compact_item_view_layout;

    fn test_layout() -> CompactItemViewLayout {
        compact_item_view_layout(
            300.0,
            ["short", "tiny", "very-long-name"],
            2,
            100.0,
            100.0,
            10.0,
            2.0,
            46.0,
            4.0,
            15.0,
        )
    }

    #[test]
    fn item_view_layout_hit_test_uses_layout_offsets_and_viewport() {
        let compact = test_layout();
        let second_column_x = compact.column_offsets[1] + 10.0 - 40.0;
        let layout = ItemViewLayout::new(100.0, 50.0, 250.0, 220.0, 40.0, compact);

        assert_eq!(
            layout.index_at_point(100.0 + second_column_x, 65.0),
            Some(2)
        );
    }

    #[test]
    fn selection_rect_uses_variable_item_geometry() {
        let compact = test_layout();
        let rect = SelectionRect {
            x1: 0.0,
            y1: 0.0,
            x2: compact.column_offsets[0] + compact.column_widths[0] + 10.0,
            y2: 205.0,
            layout: compact,
        };

        assert!(rect.intersects_index(0));
        assert!(rect.intersects_index(1));
        assert!(!rect.intersects_index(2));
        assert_eq!(rect.candidate_range(3), 0..2);
    }

    #[test]
    fn item_view_input_turns_blank_click_into_clear_selection() {
        let mut input = ItemViewInputState::default();
        input.press_blank(10.0, 20.0, test_layout(), false);

        assert!(!input.move_blank(14.0, 24.0));
        assert_eq!(
            input.release_blank(14.0, 24.0),
            ItemViewReleaseAction::ClearSelection
        );
    }

    #[test]
    fn item_view_input_turns_blank_drag_into_selection_rect() {
        let layout = test_layout();
        let mut input = ItemViewInputState::default();
        input.press_blank(120.0, 80.0, layout.clone(), true);

        assert!(input.move_blank(40.0, 140.0));
        assert_eq!(
            input.release_blank(40.0, 140.0),
            ItemViewReleaseAction::SelectRect {
                rect: SelectionRect {
                    x1: 40.0,
                    y1: 80.0,
                    x2: 120.0,
                    y2: 140.0,
                    layout,
                },
                toggle: true,
            }
        );
    }

    #[test]
    fn item_view_input_cancel_drops_pending_blank_selection() {
        let mut input = ItemViewInputState::default();
        input.press_blank(10.0, 20.0, test_layout(), false);

        input.cancel_blank();

        assert_eq!(
            input.release_blank(100.0, 120.0),
            ItemViewReleaseAction::None
        );
    }

    #[test]
    fn item_view_input_tracks_press_drag_source_until_blank_press() {
        let mut input = ItemViewInputState::default();
        input.set_drag_source("/tmp/file.txt".to_string(), false);

        let source = input.drag_source().expect("drag source");
        assert_eq!(source.path(), "/tmp/file.txt");
        assert!(!source.is_dir());

        input.press_blank(10.0, 20.0, test_layout(), false);

        assert!(input.drag_source().is_none());
    }
}
