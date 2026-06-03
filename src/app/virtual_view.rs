use crate::FileEntry;
use crate::app::geometry::{
    MainGridLayout, VirtualGridPlan, split_preview_plan, virtual_grid_plan,
};
use crate::app::pane::{PaneTarget, VirtualViewCache};
use crate::app::selection::{filtered_entries_range, filtered_entry_count};
use crate::app::state::AppState;
use crate::app::thumbnail_pipeline::decorate_entries_with_cached_thumbnails;
use std::ops::Range;
use std::path::PathBuf;

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

#[derive(Clone, Debug)]
pub(crate) struct SplitPreviewUpdate {
    pub(crate) current_dir: PathBuf,
    pub(crate) entry_count: usize,
    pub(crate) viewport_x: f32,
    pub(crate) range: Range<usize>,
    pub(crate) start_column: usize,
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) rebuild_model: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SplitPreviewInput {
    pub(crate) pane_width: f32,
    pub(crate) pane_height: f32,
    pub(crate) zoom_level: i32,
    pub(crate) thumbnail_size_px: u32,
    pub(crate) force_rebuild_model: bool,
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
    state.panes.active.view.viewport_x = plan.viewport_x;

    let rebuild_model = should_rebuild_virtual_model(
        state,
        &plan,
        visible_count,
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
    state.panes.active.view.virtual_view.range = plan.range.clone();
    state.panes.active.view.virtual_view.entry_count = visible_count;
    state.panes.active.view.virtual_view.rows_per_column = input.layout.rows_per_column;
    state.panes.active.view.virtual_view.cell_width = input.layout.cell_width;
    state.panes.active.view.virtual_view.thumbnail_size_px = input.thumbnail_size_px;

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

pub(crate) fn prepare_split_preview_update(
    state: &mut AppState,
    input: SplitPreviewInput,
) -> Option<SplitPreviewUpdate> {
    let (current_dir, entry_count, plan, rebuild_model, mut entries) = {
        let pane = state.panes.pane_mut_for_target(PaneTarget::Inactive)?;
        let entry_count = pane.entries.len();
        let plan = split_preview_plan(
            entry_count,
            input.pane_width,
            input.pane_height,
            pane.view.viewport_x,
            input.zoom_level,
        );
        let rebuild_model = input.force_rebuild_model
            || should_rebuild_virtual_cache(
                &pane.view.virtual_view,
                &plan,
                entry_count,
                input.thumbnail_size_px,
            );
        pane.view.viewport_x = plan.viewport_x;
        let entries = if rebuild_model {
            pane.entries[plan.range.clone()].to_vec()
        } else {
            Vec::new()
        };
        if rebuild_model {
            pane.view.virtual_view.range = plan.range.clone();
            pane.view.virtual_view.entry_count = entry_count;
            pane.view.virtual_view.rows_per_column = plan.rows_per_column;
            pane.view.virtual_view.cell_width = plan.cell_width;
            pane.view.virtual_view.thumbnail_size_px = input.thumbnail_size_px;
        }
        (
            pane.current_dir.clone(),
            entry_count,
            plan,
            rebuild_model,
            entries,
        )
    };

    if rebuild_model {
        decorate_entries_with_cached_thumbnails(state, &mut entries, input.thumbnail_size_px);
    }

    Some(SplitPreviewUpdate {
        current_dir,
        entry_count,
        viewport_x: plan.viewport_x,
        range: plan.range,
        start_column: plan.start_column,
        entries,
        rebuild_model,
    })
}

fn should_rebuild_virtual_model(
    state: &AppState,
    plan: &VirtualGridPlan,
    visible_count: usize,
    thumbnail_size_px: u32,
    schedule_thumbnails: bool,
) -> bool {
    !schedule_thumbnails
        || should_rebuild_virtual_cache(
            &state.panes.active.view.virtual_view,
            plan,
            visible_count,
            thumbnail_size_px,
        )
}

fn should_rebuild_virtual_cache(
    cache: &VirtualViewCache,
    plan: &VirtualGridPlan,
    entry_count: usize,
    thumbnail_size_px: u32,
) -> bool {
    cache.range != plan.range
        || cache.entry_count != entry_count
        || cache.rows_per_column != plan.rows_per_column
        || cache.cell_width != plan.cell_width
        || cache.thumbnail_size_px != thumbnail_size_px
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
        state.panes.active.entries = (0..100).map(test_entry).collect();

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
        assert_eq!(state.panes.active.view.viewport_x, 0.0);

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
        assert_eq!(state.panes.active.view.viewport_x, 40.0);
    }

    #[test]
    fn virtual_view_update_clamps_out_of_bounds_viewport() {
        let mut state = AppState::new(PathBuf::from("/tmp"), Vec::new());
        state.panes.active.entries = (0..10).map(test_entry).collect();

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
        assert_eq!(state.panes.active.view.viewport_x, 70.0);
    }

    #[test]
    fn split_preview_update_slices_inactive_pane_and_clamps_viewport() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());
        assert!(state.panes.open_inactive(PathBuf::from("/tmp/inactive")));
        {
            let inactive = state.panes.inactive_mut().unwrap();
            inactive.entries = (0..100).map(test_entry).collect();
            inactive.view.viewport_x = 1_200.0;
        }

        let update = prepare_split_preview_update(
            &mut state,
            SplitPreviewInput {
                pane_width: 420.0,
                pane_height: 704.0,
                zoom_level: 1,
                thumbnail_size_px: 80,
                force_rebuild_model: false,
            },
        )
        .unwrap();

        assert_eq!(update.current_dir, PathBuf::from("/tmp/inactive"));
        assert_eq!(update.entry_count, 100);
        assert_eq!(update.viewport_x, 1_200.0);
        assert_eq!(update.range, 18..66);
        assert_eq!(update.start_column, 3);
        assert!(update.rebuild_model);
        assert_eq!(update.entries.len(), 48);
        assert_eq!(update.entries[0].name.as_str(), "item-18.txt");
        assert_eq!(
            state.panes.inactive().unwrap().view.viewport_x,
            update.viewport_x
        );

        state.panes.inactive_mut().unwrap().view.viewport_x = 4_000.0;
        let clamped = prepare_split_preview_update(
            &mut state,
            SplitPreviewInput {
                pane_width: 420.0,
                pane_height: 704.0,
                zoom_level: 1,
                thumbnail_size_px: 80,
                force_rebuild_model: false,
            },
        )
        .unwrap();

        assert_eq!(clamped.viewport_x, 3_144.0);
        assert_eq!(clamped.range, 78..100);
        assert!(clamped.rebuild_model);
        assert_eq!(state.panes.inactive().unwrap().view.viewport_x, 3_144.0);
    }

    #[test]
    fn split_preview_update_reuses_model_inside_same_range() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());
        assert!(state.panes.open_inactive(PathBuf::from("/tmp/inactive")));
        state.panes.inactive_mut().unwrap().entries = (0..100).map(test_entry).collect();

        let first = prepare_split_preview_update(
            &mut state,
            SplitPreviewInput {
                pane_width: 420.0,
                pane_height: 704.0,
                zoom_level: 1,
                thumbnail_size_px: 80,
                force_rebuild_model: false,
            },
        )
        .unwrap();
        assert!(first.rebuild_model);
        assert_eq!(first.range, 0..36);
        assert_eq!(first.entries.len(), first.range.len());

        state.panes.inactive_mut().unwrap().view.viewport_x = 40.0;
        let second = prepare_split_preview_update(
            &mut state,
            SplitPreviewInput {
                pane_width: 420.0,
                pane_height: 704.0,
                zoom_level: 1,
                thumbnail_size_px: 80,
                force_rebuild_model: false,
            },
        )
        .unwrap();

        assert!(!second.rebuild_model);
        assert!(second.entries.is_empty());
        assert_eq!(second.range, first.range);

        let forced = prepare_split_preview_update(
            &mut state,
            SplitPreviewInput {
                pane_width: 420.0,
                pane_height: 704.0,
                zoom_level: 1,
                thumbnail_size_px: 80,
                force_rebuild_model: true,
            },
        )
        .unwrap();

        assert!(forced.rebuild_model);
        assert_eq!(forced.range, first.range);
        assert_eq!(forced.entries.len(), first.entries.len());
    }

    #[test]
    fn split_preview_update_returns_none_without_inactive_pane() {
        let mut state = AppState::new(PathBuf::from("/tmp/active"), Vec::new());

        assert!(
            prepare_split_preview_update(
                &mut state,
                SplitPreviewInput {
                    pane_width: 420.0,
                    pane_height: 704.0,
                    zoom_level: 1,
                    thumbnail_size_px: 80,
                    force_rebuild_model: false,
                },
            )
            .is_none()
        );
    }
}
