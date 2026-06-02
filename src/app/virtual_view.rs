use crate::FileEntry;
use crate::app::geometry::{MainGridLayout, VirtualGridPlan, virtual_grid_plan};
use crate::app::selection::{filtered_entries_range, filtered_entry_count};
use crate::app::state::AppState;
use crate::app::thumbnail_pipeline::decorate_entries_with_cached_thumbnails;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VirtualViewUpdate {
    pub(crate) entry_count: usize,
    pub(crate) viewport_x: f32,
    pub(crate) viewport_clamped: bool,
    pub(crate) range: Range<usize>,
    pub(crate) visible_range: Range<usize>,
    pub(crate) start_column: usize,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) rebuild_model: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VirtualViewInput {
    pub(crate) layout: MainGridLayout,
    pub(crate) requested_viewport_x: f32,
    pub(crate) viewport_width: f32,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) schedule_thumbnails: bool,
    pub(crate) visible_count_override: Option<usize>,
}

pub(crate) fn prepare_virtual_view_update(
    state: &mut AppState,
    input: VirtualViewInput,
) -> VirtualViewUpdate {
    let visible_count = input
        .visible_count_override
        .unwrap_or_else(|| filtered_entry_count(state));
    let plan = virtual_grid_plan(
        visible_count,
        input.layout.rows_per_column,
        input.requested_viewport_x,
        input.viewport_width,
        input.layout.cell_width,
        input.layout.padding,
        2,
    );
    let viewport_clamped = (plan.viewport_x - input.requested_viewport_x).abs() > f32::EPSILON;

    let rebuild_model = should_rebuild_virtual_model(
        state,
        &plan,
        visible_count,
        input.layout,
        input.thumbnail_size_px,
        input.schedule_thumbnails,
    );
    if !rebuild_model {
        return VirtualViewUpdate {
            entry_count: visible_count,
            viewport_x: plan.viewport_x,
            viewport_clamped,
            range: plan.range,
            visible_range: plan.visible_range,
            start_column: plan.start_column,
            entries: Vec::new(),
            rebuild_model: false,
        };
    }

    let mut entries = filtered_entries_range(state, plan.range.clone());
    decorate_entries_with_cached_thumbnails(state, &mut entries, input.thumbnail_size_px);
    state.pane.view.virtual_view.range = plan.range.clone();
    state.pane.view.virtual_view.entry_count = visible_count;
    state.pane.view.virtual_view.rows_per_column = input.layout.rows_per_column;
    state.pane.view.virtual_view.cell_width = input.layout.cell_width;
    state.pane.view.virtual_view.thumbnail_size_px = input.thumbnail_size_px;

    VirtualViewUpdate {
        entry_count: visible_count,
        viewport_x: plan.viewport_x,
        viewport_clamped,
        range: plan.range,
        visible_range: plan.visible_range,
        start_column: plan.start_column,
        entries,
        rebuild_model: true,
    }
}

fn should_rebuild_virtual_model(
    state: &AppState,
    plan: &VirtualGridPlan,
    visible_count: usize,
    layout: MainGridLayout,
    thumbnail_size_px: u32,
    schedule_thumbnails: bool,
) -> bool {
    !schedule_thumbnails
        || state.pane.view.virtual_view.range != plan.range
        || state.pane.view.virtual_view.entry_count != visible_count
        || state.pane.view.virtual_view.rows_per_column != layout.rows_per_column
        || state.pane.view.virtual_view.cell_width != layout.cell_width
        || state.pane.view.virtual_view.thumbnail_size_px != thumbnail_size_px
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileEntry;
    use crate::app::state::AppState;
    use slint::Image;
    use std::path::PathBuf;

    fn layout() -> MainGridLayout {
        MainGridLayout {
            main_x: 0.0,
            main_y: 0.0,
            viewport_x: 0.0,
            rows_per_column: 4,
            cell_width: 100.0,
            row_height: 80.0,
            padding: 10.0,
        }
    }

    fn test_entry(index: usize) -> FileEntry {
        FileEntry {
            name: format!("item-{index}.txt").into(),
            path: format!("/tmp/item-{index}.txt").into(),
            group: String::new().into(),
            location: String::new().into(),
            kind: "File".into(),
            size: "1 KB".into(),
            size_bytes: 1024.0,
            modified: "Today".into(),
            modified_age_days: 0,
            is_dir: false,
            thumbnail_state: 0,
            thumbnail: Image::default(),
        }
    }

    #[test]
    fn virtual_view_update_reuses_model_inside_same_range() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.pane.entries = (0..100).map(test_entry).collect();

        let first = prepare_virtual_view_update(
            &mut state,
            VirtualViewInput {
                layout: layout(),
                requested_viewport_x: 0.0,
                viewport_width: 250.0,
                thumbnail_size_px: 64,
                schedule_thumbnails: true,
                visible_count_override: None,
            },
        );
        assert!(first.rebuild_model);
        assert_eq!(first.range, 0..24);
        assert_eq!(first.entries.len(), 24);

        let second = prepare_virtual_view_update(
            &mut state,
            VirtualViewInput {
                layout: layout(),
                requested_viewport_x: 40.0,
                viewport_width: 250.0,
                thumbnail_size_px: 64,
                schedule_thumbnails: true,
                visible_count_override: None,
            },
        );
        assert!(!second.rebuild_model);
        assert!(second.entries.is_empty());
    }

    #[test]
    fn virtual_view_update_clamps_out_of_bounds_viewport() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.pane.entries = (0..10).map(test_entry).collect();

        let update = prepare_virtual_view_update(
            &mut state,
            VirtualViewInput {
                layout: layout(),
                requested_viewport_x: 800.0,
                viewport_width: 250.0,
                thumbnail_size_px: 64,
                schedule_thumbnails: true,
                visible_count_override: None,
            },
        );

        assert!(update.viewport_clamped);
        assert_eq!(update.viewport_x, 70.0);
        assert_eq!(update.range, 0..10);
    }
}
