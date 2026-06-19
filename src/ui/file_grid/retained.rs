use super::icon_work::{
    DOLPHIN_VISIBLE_ICON_SYNC_BUDGET, FileIconSyncStats,
    resolve_visible_file_icons_for_raw_grid_with_stats,
};
use super::perf::{ItemViewPerfLogFrame, ItemViewPerfPhase, emit_item_view_perf_log};
use super::snapshot::{
    QueuedVisibleModelWork, RawFileGridSnapshot, RawFileGridSnapshotInput,
    RetainedFileGridProjection, project_retained_file_grid_snapshot,
    queue_raw_file_grid_model_work, raw_file_grid_snapshot,
    visible_metadata_role_results_for_raw_grid,
};
use super::{
    FileGridRenderSnapshot, ItemPaintSlotCache, ItemPaintSlotStats, VisibleItemSlotPool,
    VisibleItemSnapshotCache,
};
use crate::FikaApp;
use crate::ui::drag_drop::ItemDropTarget;
use crate::ui::icons::{FileIconSnapshot, file_icon_resolve_results_for_requests};
use crate::ui::rename::RenameDraft;
use fika_core::{
    FilteredModel, Generation, MAX_ZOOM_LEVEL, MIN_ZOOM_LEVEL, MetadataRoleResult, PaneId,
    ThumbnailProbeResult, ViewMode, ViewState, apply_metadata_role_result_to_model,
    apply_thumbnail_probe_result_to_model, icon_size_for_zoom_level,
    metadata_role_results_for_requests, thumbnail_probe_results_for_requests,
};
use gpui::{AppContext, Context};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const METADATA_ROLE_BATCH_SIZE: usize = 16;
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const THUMBNAIL_PROBE_BATCH_SIZE: usize = 32;

pub(crate) struct PaneRawFileGridSnapshot {
    pub(crate) raw_file_grid: RawFileGridSnapshot,
    pub(crate) model_data_generation: u64,
}

pub(crate) struct FileGridVisibleWorkFrame {
    pub(crate) icon_sync_elapsed: Option<Duration>,
    pub(crate) queue_elapsed: Option<Duration>,
}

pub(crate) struct RetainedFileGridFrame {
    pub(crate) file_grid: FileGridRenderSnapshot,
    pub(crate) item_paint_slot_stats: ItemPaintSlotStats,
    pub(crate) visible_count: usize,
    pub(crate) item_view_perf_phase: Option<ItemViewPerfPhase>,
}

pub(crate) struct PaneFileGridRenderFrame {
    pub(crate) file_grid: FileGridRenderSnapshot,
    pub(crate) warm_static_visual_file_grid: Option<FileGridRenderSnapshot>,
    pub(crate) item_count: usize,
    visible_count: usize,
    raw_elapsed: Option<Duration>,
    icon_sync_elapsed: Option<Duration>,
    queue_elapsed: Option<Duration>,
    convert_elapsed: Option<Duration>,
    item_paint_slot_stats: ItemPaintSlotStats,
    item_view_perf_phase: Option<ItemViewPerfPhase>,
}

impl PaneFileGridRenderFrame {
    pub(crate) fn emit_perf_log(&self, pane_id: PaneId, mode: ViewMode, total_elapsed: Duration) {
        emit_item_view_perf_log(ItemViewPerfLogFrame {
            pane_id,
            mode,
            phase: self.item_view_perf_phase,
            item_count: self.item_count,
            visible_count: self.visible_count,
            raw_elapsed: self.raw_elapsed,
            icon_sync_elapsed: self.icon_sync_elapsed,
            queue_elapsed: self.queue_elapsed,
            convert_elapsed: self.convert_elapsed,
            total_elapsed,
            slot_stats: self.item_paint_slot_stats,
        });
    }
}

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

    pub(crate) fn raw_file_grid_snapshot_after_visible_metadata_sync(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        view: &ViewState,
        filtered: Option<&FilteredModel>,
        source_revision: u64,
        rename_draft: Option<&RenameDraft>,
        item_drop_target: Option<&ItemDropTarget>,
        metadata_budget: Duration,
    ) -> Option<PaneRawFileGridSnapshot> {
        let mut raw_file_grid = self.raw_file_grid_snapshot_for_pane(
            pane_id,
            view,
            filtered,
            source_revision,
            rename_draft,
            item_drop_target,
        )?;
        if self.resolve_visible_metadata_roles_for_raw_grid(
            pane_id,
            generation,
            &raw_file_grid,
            metadata_budget,
        ) {
            raw_file_grid = self.raw_file_grid_snapshot_for_pane(
                pane_id,
                view,
                filtered,
                source_revision,
                rename_draft,
                item_drop_target,
            )?;
        }
        let model_data_generation = self.panes.pane(pane_id)?.model.data_generation();
        Some(PaneRawFileGridSnapshot {
            raw_file_grid,
            model_data_generation,
        })
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

    pub(crate) fn warm_static_visual_file_grid_for_pane(
        &mut self,
        pane_id: PaneId,
        view: &ViewState,
        selection_count: usize,
        filtered: Option<&FilteredModel>,
        source_revision: u64,
        rename_draft: Option<&RenameDraft>,
        item_drop_target: Option<&ItemDropTarget>,
    ) -> Option<FileGridRenderSnapshot> {
        let mut warm_view = view.clone();
        match view.view_mode {
            ViewMode::Compact => {
                warm_view.set_view_mode(ViewMode::Icons);
            }
            ViewMode::Icons => {
                warm_view.set_view_mode(ViewMode::Compact);
            }
            ViewMode::Details => return None,
        }
        let raw_file_grid = self.raw_file_grid_snapshot_for_pane(
            pane_id,
            &warm_view,
            filtered,
            source_revision,
            rename_draft,
            item_drop_target,
        )?;
        let mut visible_item_slots = VisibleItemSlotPool::default();
        let mut visible_item_cache = VisibleItemSnapshotCache::default();
        let mut item_paint_slots = ItemPaintSlotCache::default();
        let file_icon_size = warm_view.icon_size();
        let projection = project_retained_file_grid_snapshot(
            raw_file_grid,
            selection_count,
            &mut visible_item_slots,
            &mut visible_item_cache,
            &mut item_paint_slots,
            None,
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
        match projection.snapshot {
            FileGridRenderSnapshot::Compact { .. } | FileGridRenderSnapshot::Icons { .. } => {
                Some(projection.snapshot)
            }
            FileGridRenderSnapshot::Details { .. } => None,
        }
    }

    pub(crate) fn project_retained_file_grid_frame_for_pane(
        &mut self,
        pane_id: PaneId,
        raw_file_grid: RawFileGridSnapshot,
        selection_count: usize,
        file_icon_size: f32,
        view_mode: ViewMode,
        item_count: usize,
        perf_enabled: bool,
    ) -> RetainedFileGridFrame {
        let visible_count = raw_file_grid
            .visible_layout_range_and_count()
            .or_else(|| raw_file_grid.visible_work_range_and_count())
            .map(|(_, count)| count)
            .unwrap_or_default();
        let projection = self.project_retained_file_grid_for_pane(
            pane_id,
            raw_file_grid,
            selection_count,
            file_icon_size,
        );
        let item_paint_slot_stats = projection.slot_stats;
        let item_view_perf_phase = if perf_enabled {
            Some(self.record_item_view_perf_frame(
                pane_id,
                view_mode,
                item_count,
                visible_count,
                item_paint_slot_stats,
            ))
        } else {
            None
        };

        RetainedFileGridFrame {
            file_grid: projection.snapshot,
            item_paint_slot_stats,
            visible_count,
            item_view_perf_phase,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_file_grid_render_frame_for_pane(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        view: &ViewState,
        selection_count: usize,
        filtered: Option<&FilteredModel>,
        source_revision: u64,
        rename_draft: Option<&RenameDraft>,
        item_drop_target: Option<&ItemDropTarget>,
        metadata_budget: Duration,
        perf_enabled: bool,
        cx: &mut Context<Self>,
    ) -> Option<PaneFileGridRenderFrame> {
        let raw_started = perf_enabled.then(Instant::now);
        let prepared_raw_file_grid = self.raw_file_grid_snapshot_after_visible_metadata_sync(
            pane_id,
            generation,
            view,
            filtered,
            source_revision,
            rename_draft,
            item_drop_target,
            metadata_budget,
        )?;
        let raw_file_grid = prepared_raw_file_grid.raw_file_grid;
        let model_data_generation = prepared_raw_file_grid.model_data_generation;
        let raw_elapsed = raw_started.map(|started| started.elapsed());

        let file_icon_size = view.icon_size();
        let file_icon_resolve_sizes = file_icon_resolve_sizes_for_zoom_level(view.zoom_level);
        let item_count = {
            let pane = self.panes.pane(pane_id)?;
            filtered.map_or_else(|| pane.model.len(), |filtered| filtered.len())
        };
        let visible_work_frame = self.sync_and_start_file_grid_visible_work_for_raw_grid(
            pane_id,
            generation,
            view.view_mode,
            model_data_generation,
            source_revision,
            item_count,
            &raw_file_grid,
            file_icon_size,
            &file_icon_resolve_sizes,
            filtered,
            perf_enabled,
            cx,
        )?;

        let convert_started = perf_enabled.then(Instant::now);
        let retained_file_grid_frame = self.project_retained_file_grid_frame_for_pane(
            pane_id,
            raw_file_grid,
            selection_count,
            file_icon_size,
            view.view_mode,
            item_count,
            perf_enabled,
        );
        let convert_elapsed = convert_started.map(|started| started.elapsed());
        let warm_static_visual_file_grid = self.warm_static_visual_file_grid_for_pane(
            pane_id,
            view,
            selection_count,
            filtered,
            source_revision,
            rename_draft,
            item_drop_target,
        );

        Some(PaneFileGridRenderFrame {
            file_grid: retained_file_grid_frame.file_grid,
            warm_static_visual_file_grid,
            item_count,
            visible_count: retained_file_grid_frame.visible_count,
            raw_elapsed,
            icon_sync_elapsed: visible_work_frame.icon_sync_elapsed,
            queue_elapsed: visible_work_frame.queue_elapsed,
            convert_elapsed,
            item_paint_slot_stats: retained_file_grid_frame.item_paint_slot_stats,
            item_view_perf_phase: retained_file_grid_frame.item_view_perf_phase,
        })
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
        file_icon_resolve_sizes: &[f32],
        filtered: Option<&fika_core::FilteredModel>,
    ) -> Option<QueuedVisibleModelWork> {
        let pane = self.panes.pane(pane_id)?;
        Some(queue_raw_file_grid_model_work(
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
            file_icon_resolve_sizes,
            &pane.model,
            filtered,
        ))
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
    ) -> FileIconSyncStats {
        let stats = resolve_visible_file_icons_for_raw_grid_with_stats(
            &mut self.file_icons,
            &self.file_icon_resolve_queue,
            raw_file_grid,
            file_icon_size,
            DOLPHIN_VISIBLE_ICON_SYNC_BUDGET,
        );
        if stats.changed > 0 {
            self.invalidate_file_grid_visible_snapshot_cache(pane_id);
        }
        stats
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

    pub(crate) fn start_queued_file_grid_model_work(
        &mut self,
        queued_work: QueuedVisibleModelWork,
        cx: &mut Context<Self>,
    ) {
        if queued_work.is_empty() {
            return;
        }
        if queued_work.metadata_role {
            self.maybe_start_metadata_role(cx);
        }
        if queued_work.thumbnail_probe {
            self.maybe_start_thumbnail_probe(cx);
        }
        if queued_work.file_icon_resolve {
            self.maybe_start_file_icon_resolve(cx);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sync_and_start_file_grid_visible_work_for_raw_grid(
        &mut self,
        pane_id: PaneId,
        generation: Generation,
        view_mode: ViewMode,
        model_data_generation: u64,
        source_revision: u64,
        item_count: usize,
        raw_file_grid: &RawFileGridSnapshot,
        file_icon_size: f32,
        file_icon_resolve_sizes: &[f32],
        filtered: Option<&fika_core::FilteredModel>,
        perf_enabled: bool,
        cx: &mut Context<Self>,
    ) -> Option<FileGridVisibleWorkFrame> {
        let icon_sync_started = perf_enabled.then(Instant::now);
        let icon_sync_stats =
            self.resolve_visible_file_icons_for_raw_grid(pane_id, raw_file_grid, file_icon_size);
        let icon_sync_elapsed = icon_sync_started.map(|started| started.elapsed());
        if let Some(elapsed) = icon_sync_elapsed
            && icon_sync_stats.has_activity()
        {
            eprintln!(
                "[fika icon-sync] pane={} mode={:?} candidates={} cached={} queued={} resolved={} changed={} budget_exhausted={} total={}us",
                pane_id.0,
                view_mode,
                icon_sync_stats.candidates,
                icon_sync_stats.cached,
                icon_sync_stats.queued,
                icon_sync_stats.resolved,
                icon_sync_stats.changed,
                icon_sync_stats.budget_exhausted,
                elapsed.as_micros(),
            );
        }

        let queue_started = perf_enabled.then(Instant::now);
        let queued_file_grid_model_work = self.queue_file_grid_model_work_for_raw_grid(
            pane_id,
            generation,
            view_mode,
            model_data_generation,
            source_revision,
            item_count,
            raw_file_grid,
            file_icon_size,
            file_icon_resolve_sizes,
            filtered,
        )?;
        let queue_elapsed = queue_started.map(|started| started.elapsed());
        self.start_queued_file_grid_model_work(queued_file_grid_model_work, cx);

        Some(FileGridVisibleWorkFrame {
            icon_sync_elapsed,
            queue_elapsed,
        })
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

fn file_icon_resolve_sizes_for_zoom_level(zoom_level: i32) -> Vec<f32> {
    let mut sizes = Vec::new();
    for level in [
        zoom_level,
        zoom_level + 1,
        zoom_level + 2,
        zoom_level - 1,
        zoom_level - 2,
    ] {
        if !(MIN_ZOOM_LEVEL..=MAX_ZOOM_LEVEL).contains(&level) {
            continue;
        }
        let size = icon_size_for_zoom_level(level);
        if !sizes.iter().any(|existing| *existing == size) {
            sizes.push(size);
        }
    }
    sizes
}
