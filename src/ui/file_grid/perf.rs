use std::time::Duration;

use fika_core::PaneId;

use crate::FikaApp;

use super::{DetailsTextShapeCache, StaticItemTextShapeCache, TextShapeCacheStats};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemLayerPerfStats {
    pub(crate) prepaint_count: usize,
    pub(crate) prepaint_us: u128,
    pub(crate) paint_count: usize,
    pub(crate) paint_us: u128,
}

pub(crate) type StaticItemVisualPerfStats = ItemLayerPerfStats;
pub(crate) type ItemImagePerfStats = ItemLayerPerfStats;
pub(crate) type DetailsVisualPerfStats = ItemLayerPerfStats;
pub(crate) type ItemInteractionPerfStats = ItemLayerPerfStats;

impl ItemLayerPerfStats {
    pub(super) fn has_activity(self) -> bool {
        self.prepaint_count > 0 || self.paint_count > 0
    }

    pub(super) fn record_prepaint(&mut self, elapsed: Duration, count: usize) {
        self.prepaint_count += count;
        self.prepaint_us += elapsed.as_micros();
    }

    pub(super) fn record_paint(&mut self, elapsed: Duration, count: usize) {
        self.paint_count += count;
        self.paint_us += elapsed.as_micros();
    }
}

impl FikaApp {
    pub(super) fn take_static_item_text_shape_cache_stats(
        &mut self,
        pane_id: PaneId,
    ) -> TextShapeCacheStats {
        self.static_item_text_shape_caches
            .get_mut(&pane_id)
            .map(StaticItemTextShapeCache::take_stats)
            .unwrap_or_default()
    }

    pub(super) fn take_details_text_shape_cache_stats(
        &mut self,
        pane_id: PaneId,
    ) -> TextShapeCacheStats {
        self.details_text_shape_caches
            .get_mut(&pane_id)
            .map(DetailsTextShapeCache::take_stats)
            .unwrap_or_default()
    }

    pub(super) fn take_static_item_visual_perf_stats(
        &mut self,
        pane_id: PaneId,
    ) -> StaticItemVisualPerfStats {
        self.static_item_visual_perf_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    pub(super) fn take_item_image_perf_stats(&mut self, pane_id: PaneId) -> ItemImagePerfStats {
        self.item_image_perf_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    pub(super) fn take_details_visual_perf_stats(
        &mut self,
        pane_id: PaneId,
    ) -> DetailsVisualPerfStats {
        self.details_visual_perf_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    pub(super) fn take_item_interaction_perf_stats(
        &mut self,
        pane_id: PaneId,
    ) -> ItemInteractionPerfStats {
        self.item_interaction_perf_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    pub(super) fn record_static_item_visual_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.static_item_visual_perf_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    pub(super) fn record_static_item_visual_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.static_item_visual_perf_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }

    pub(super) fn record_item_image_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_image_perf_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    pub(super) fn record_item_image_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_image_perf_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }

    pub(super) fn record_details_visual_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.details_visual_perf_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    pub(super) fn record_details_visual_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.details_visual_perf_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }

    pub(super) fn record_item_interaction_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_interaction_perf_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    pub(super) fn record_item_interaction_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_interaction_perf_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }
}
