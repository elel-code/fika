use super::icon_work::{
    DOLPHIN_VISIBLE_ICON_SYNC_BUDGET,
    resolve_visible_file_icons_for_raw_grid as resolve_visible_file_icons_for_raw_grid_with_cache,
};
use super::snapshot::{
    RawFileGridSnapshot, RawFileGridSnapshotInput, RetainedFileGridProjection,
    project_retained_file_grid_snapshot, queue_raw_file_grid_model_work, raw_file_grid_snapshot,
    visible_metadata_role_results_for_raw_grid,
};
use crate::FikaApp;
use crate::ui::drag_drop::ItemDropTarget;
use crate::ui::icons::{FileIconSnapshot, file_icon_resolve_results_for_requests};
use crate::ui::rename::RenameDraft;
use fika_core::{
    FilteredModel, Generation, MetadataRoleResult, PaneId, ThumbnailProbeResult, ViewMode,
    ViewState, apply_metadata_role_result_to_model, apply_thumbnail_probe_result_to_model,
    metadata_role_results_for_requests, thumbnail_probe_results_for_requests,
};
use gpui::{AppContext, Context};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const METADATA_ROLE_BATCH_SIZE: usize = 16;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const THUMBNAIL_PROBE_BATCH_SIZE: usize = 32;

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

    pub(crate) fn resolve_visible_file_icons_for_raw_grid(
        &mut self,
        pane_id: PaneId,
        raw_file_grid: &RawFileGridSnapshot,
        file_icon_size: f32,
    ) -> bool {
        let changed = resolve_visible_file_icons_for_raw_grid_with_cache(
            &mut self.file_icons,
            &self.file_icon_resolve_queue,
            raw_file_grid,
            file_icon_size,
            DOLPHIN_VISIBLE_ICON_SYNC_BUDGET,
        );
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

    pub(crate) fn maybe_start_metadata_role(&mut self, cx: &mut Context<Self>) {
        let Some(batch) = self
            .metadata_role_scheduler
            .start_role_batch(METADATA_ROLE_BATCH_SIZE)
        else {
            return;
        };
        let requests = batch.requests;

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let results = cx
                        .background_spawn(
                            async move { metadata_role_results_for_requests(requests) },
                        )
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.metadata_role_scheduler
                            .finish_role_batch_with_results(&results);
                        let changed = app.finish_metadata_role_results(results);
                        app.maybe_start_metadata_role(cx);
                        if changed {
                            cx.notify();
                        }
                    });
                }
            },
        )
        .detach();
    }

    pub(crate) fn maybe_start_file_icon_resolve(&mut self, cx: &mut Context<Self>) {
        let Some(requests) = self.file_icon_resolve_queue.start_next_batch() else {
            return;
        };
        let finished_requests = requests.clone();

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let results = cx
                        .background_spawn(async move {
                            file_icon_resolve_results_for_requests(requests)
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.file_icon_resolve_queue.finish_batch(finished_requests);
                        let changed = app.file_icons.finish_resolve_results(results);
                        if changed {
                            app.invalidate_all_file_grid_visible_snapshot_caches();
                        }
                        app.maybe_start_file_icon_resolve(cx);
                        if changed {
                            cx.notify();
                        }
                    });
                }
            },
        )
        .detach();
    }

    pub(crate) fn maybe_start_thumbnail_probe(&mut self, cx: &mut Context<Self>) {
        let Some(batch) = self
            .thumbnail_scheduler
            .start_probe_batch(THUMBNAIL_PROBE_BATCH_SIZE)
        else {
            return;
        };
        let cache_root = batch.cache_root;
        let requests = batch.requests;
        let cancel_handle = batch.cancel_handle;

        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let results = cx
                        .background_spawn(async move {
                            thumbnail_probe_results_for_requests(
                                cache_root,
                                requests,
                                cancel_handle,
                            )
                        })
                        .await;
                    let _ = this.update(&mut cx, |app, cx| {
                        app.thumbnail_scheduler.finish_probe_batch();
                        let changed = app.finish_thumbnail_probe_results(results);
                        app.maybe_start_thumbnail_probe(cx);
                        if changed {
                            cx.notify();
                        }
                    });
                }
            },
        )
        .detach();
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
