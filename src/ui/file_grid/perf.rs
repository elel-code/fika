use std::time::Duration;

use fika_core::{PaneId, ViewMode};

use crate::FikaApp;

use super::ItemPaintSlotStats;
use super::{DetailsTextShapeCache, StaticItemTextShapeCache, TextShapeCacheStats};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemViewPerfFrameState {
    mode: ViewMode,
    item_count: usize,
    visible_count: usize,
}

impl ItemViewPerfFrameState {
    pub(crate) fn new(mode: ViewMode, item_count: usize, visible_count: usize) -> Self {
        Self {
            mode,
            item_count,
            visible_count,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemViewPerfPhase {
    Initial,
    ModeSwitch,
    ContentChange,
    GeometryChange,
    VisualChange,
    Steady,
}

impl ItemViewPerfPhase {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::ModeSwitch => "mode-switch",
            Self::ContentChange => "content-change",
            Self::GeometryChange => "geometry-change",
            Self::VisualChange => "visual-change",
            Self::Steady => "steady",
        }
    }
}

pub(crate) fn classify_item_view_perf_phase(
    previous: Option<ItemViewPerfFrameState>,
    current: ItemViewPerfFrameState,
    slot_stats: ItemPaintSlotStats,
) -> ItemViewPerfPhase {
    let Some(previous) = previous else {
        return ItemViewPerfPhase::Initial;
    };
    if previous.mode != current.mode {
        return ItemViewPerfPhase::ModeSwitch;
    }
    if previous.item_count != current.item_count || slot_stats.content_changed > 0 {
        return ItemViewPerfPhase::ContentChange;
    }
    if previous.visible_count != current.visible_count
        || slot_stats.geometry_changed > 0
        || slot_stats.inserted > 0
        || slot_stats.removed > 0
    {
        return ItemViewPerfPhase::GeometryChange;
    }
    if slot_stats.visual_changed > 0 {
        return ItemViewPerfPhase::VisualChange;
    }
    ItemViewPerfPhase::Steady
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_view_perf_phase_separates_mode_switch_from_resize() {
        let previous = ItemViewPerfFrameState::new(ViewMode::Compact, 48, 32);
        assert_eq!(
            classify_item_view_perf_phase(
                None,
                ItemViewPerfFrameState::new(ViewMode::Compact, 48, 32),
                ItemPaintSlotStats::default(),
            ),
            ItemViewPerfPhase::Initial
        );
        assert_eq!(
            classify_item_view_perf_phase(
                Some(previous),
                ItemViewPerfFrameState::new(ViewMode::Icons, 48, 40),
                ItemPaintSlotStats {
                    inserted: 40,
                    ..Default::default()
                },
            ),
            ItemViewPerfPhase::ModeSwitch
        );
        assert_eq!(
            classify_item_view_perf_phase(
                Some(ItemViewPerfFrameState::new(ViewMode::Icons, 48, 40)),
                ItemViewPerfFrameState::new(ViewMode::Icons, 48, 48),
                ItemPaintSlotStats {
                    inserted: 8,
                    unchanged: 40,
                    ..Default::default()
                },
            ),
            ItemViewPerfPhase::GeometryChange
        );
        assert_eq!(
            classify_item_view_perf_phase(
                Some(ItemViewPerfFrameState::new(ViewMode::Icons, 48, 48)),
                ItemViewPerfFrameState::new(ViewMode::Icons, 49, 48),
                ItemPaintSlotStats {
                    content_changed: 1,
                    unchanged: 48,
                    ..Default::default()
                },
            ),
            ItemViewPerfPhase::ContentChange
        );
        assert_eq!(
            classify_item_view_perf_phase(
                Some(ItemViewPerfFrameState::new(ViewMode::Icons, 48, 48)),
                ItemViewPerfFrameState::new(ViewMode::Icons, 48, 48),
                ItemPaintSlotStats {
                    visual_changed: 1,
                    unchanged: 47,
                    ..Default::default()
                },
            ),
            ItemViewPerfPhase::VisualChange
        );
        assert_eq!(
            classify_item_view_perf_phase(
                Some(ItemViewPerfFrameState::new(ViewMode::Icons, 48, 48)),
                ItemViewPerfFrameState::new(ViewMode::Icons, 48, 48),
                ItemPaintSlotStats {
                    unchanged: 48,
                    ..Default::default()
                },
            ),
            ItemViewPerfPhase::Steady
        );
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
