use super::snapshot::{
    RawFileGridSnapshot, RawFileGridSnapshotInput, RetainedFileGridProjection,
    project_retained_file_grid_snapshot, queue_raw_file_grid_model_work, raw_file_grid_snapshot,
};
use crate::FikaApp;
use crate::ui::drag_drop::ItemDropTarget;
use crate::ui::rename::RenameDraft;
use fika_core::{FilteredModel, Generation, PaneId, ViewMode, ViewState};

impl FikaApp {
    pub(crate) fn raw_file_grid_snapshot_for_pane(
        &mut self,
        pane_id: PaneId,
        view: &ViewState,
        filtered: Option<&FilteredModel>,
        source_revision: u64,
        rename_draft: Option<&RenameDraft>,
        item_drop_target: Option<&ItemDropTarget>,
    ) -> Option<RawFileGridSnapshot> {
        let pane = self.panes.pane(pane_id)?;
        Some(raw_file_grid_snapshot(RawFileGridSnapshotInput {
            pane_id,
            model: &pane.model,
            selection: &pane.selection,
            view,
            filtered,
            source_revision,
            rename_draft,
            item_drop_target,
            compact_column_widths: self.compact_column_widths.entry(pane_id).or_default(),
        }))
    }

    pub(crate) fn project_retained_file_grid_for_pane(
        &mut self,
        pane_id: PaneId,
        raw_file_grid: RawFileGridSnapshot,
        selection_count: usize,
        file_icon_size: f32,
    ) -> RetainedFileGridProjection {
        let mut visible_item_slots = self.visible_item_slots.remove(&pane_id).unwrap_or_default();
        let mut visible_item_cache = self
            .visible_item_snapshot_caches
            .remove(&pane_id)
            .unwrap_or_default();
        let mut item_paint_slots = self.item_paint_slots.remove(&pane_id).unwrap_or_default();
        let hovered_item = self.hovered_item.item_for_pane(pane_id);
        let projection = project_retained_file_grid_snapshot(
            raw_file_grid,
            selection_count,
            &mut visible_item_slots,
            &mut visible_item_cache,
            &mut item_paint_slots,
            hovered_item,
            file_icon_size,
            |request| {
                self.icon_snapshot_for_model_item(
                    request.path,
                    request.is_dir,
                    request.mime_type.clone(),
                    request.mime_magic_checked,
                    request.icon_size,
                )
            },
        );
        self.visible_item_slots.insert(pane_id, visible_item_slots);
        self.visible_item_snapshot_caches
            .insert(pane_id, visible_item_cache);
        self.item_paint_slots.insert(pane_id, item_paint_slots);
        projection
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn queue_file_grid_model_work_for_raw_grid(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        view_mode: ViewMode,
        model_data_generation: u64,
        source_revision: u64,
        item_count: usize,
        raw_file_grid: &RawFileGridSnapshot,
        file_icon_size: f32,
        filtered: Option<&fika_core::FilteredModel>,
    ) -> Option<(bool, bool, bool)> {
        let pane = self.panes.pane(pane_id)?;
        Some(
            queue_raw_file_grid_model_work(
                &mut self.visible_work_keys,
                &mut self.metadata_role_scheduler,
                &mut self.thumbnail_scheduler,
                &self.file_icons,
                &mut self.file_icon_resolve_queue,
                pane_id,
                generation,
                view_mode,
                model_data_generation,
                source_revision,
                item_count,
                raw_file_grid,
                file_icon_size,
                &pane.model,
                filtered,
            )
            .into_tuple(),
        )
    }
}
