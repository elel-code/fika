use super::snapshot::{
    RawFileGridSnapshot, RawFileGridSnapshotInput, RetainedFileGridProjection,
    project_retained_file_grid_snapshot, queue_raw_file_grid_model_work, raw_file_grid_snapshot,
    visible_metadata_role_results_for_raw_grid,
};
use crate::FikaApp;
use crate::ui::drag_drop::ItemDropTarget;
use crate::ui::icons::FileIconSnapshot;
use crate::ui::rename::RenameDraft;
use fika_core::{
    FilteredModel, Generation, MetadataRoleResult, PaneId, ThumbnailProbeResult, ViewMode,
    ViewState, apply_metadata_role_result_to_model, apply_thumbnail_probe_result_to_model,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

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

    pub(crate) fn resolve_visible_metadata_roles_for_raw_grid(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        raw_file_grid: &RawFileGridSnapshot,
        budget: Duration,
    ) -> bool {
        let results =
            visible_metadata_role_results_for_raw_grid(pane_id, generation, raw_file_grid, budget);
        if results.is_empty() {
            return false;
        }

        let changed = self.finish_metadata_role_results(results);
        if changed {
            self.invalidate_file_grid_visible_snapshot_cache(pane_id);
        }
        changed
    }

    pub(crate) fn finish_metadata_role_results(
        &mut self,
        results: Vec<MetadataRoleResult>,
    ) -> bool {
        let mut changed = false;
        for result in results {
            let Some(pane) = self.panes.pane_mut(result.pane_id) else {
                continue;
            };
            if pane.generation != result.generation {
                continue;
            }
            changed |= apply_metadata_role_result_to_model(&mut pane.model, result);
        }
        changed
    }

    pub(crate) fn finish_thumbnail_probe_results(
        &mut self,
        results: Vec<ThumbnailProbeResult>,
    ) -> bool {
        let mut changed = false;
        for result in results {
            let Some(pane) = self.panes.pane_mut(result.pane_id) else {
                continue;
            };
            if pane.generation != result.generation {
                continue;
            }
            if apply_thumbnail_probe_result_to_model(&mut pane.model, result) {
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn icon_snapshot_for_model_item(
        &mut self,
        path: &Path,
        is_dir: bool,
        mime_type: Option<Arc<str>>,
        mime_magic_checked: bool,
        icon_size: f32,
    ) -> FileIconSnapshot {
        self.file_icons.cached_or_preliminary_icon_for(
            path,
            is_dir,
            mime_type,
            mime_magic_checked,
            icon_size,
        )
    }

    pub(crate) fn cancel_metadata_role_work_for_pane(&mut self, pane_id: PaneId) {
        self.metadata_role_scheduler.cancel_pane(pane_id);
    }

    pub(crate) fn cancel_stale_metadata_role_work_for_pane(&mut self, pane_id: PaneId) {
        let Some(generation) = self.panes.pane(pane_id).map(|pane| pane.generation) else {
            self.cancel_metadata_role_work_for_pane(pane_id);
            return;
        };
        self.metadata_role_scheduler
            .cancel_stale_pane_generations(pane_id, generation);
    }

    pub(crate) fn cancel_thumbnail_work_for_pane(&mut self, pane_id: PaneId) {
        self.thumbnail_scheduler.cancel_pane(pane_id);
    }

    pub(crate) fn cancel_stale_thumbnail_work_for_pane(&mut self, pane_id: PaneId) {
        let Some(generation) = self.panes.pane(pane_id).map(|pane| pane.generation) else {
            self.cancel_thumbnail_work_for_pane(pane_id);
            return;
        };
        self.thumbnail_scheduler
            .cancel_stale_pane_generations(pane_id, generation);
    }
}
