use std::collections::HashMap;
use std::env;
use std::time::Duration;

use fika_core::{PaneId, ViewMode};

use crate::FikaApp;
use crate::ui::retained::env_flag_is_truthy;

use super::ItemPaintSlotStats;
use super::{DetailsTextShapeCache, StaticItemTextShapeCache, TextShapeCacheStats};

const PERF_ITEM_VIEW_ENV: &str = "FIKA_PERF_ITEM_VIEW";
const VISIBLE_GLYPH_RASTER_BUDGET_US: u128 = 2_000;
const VISIBLE_GLYPH_RASTER_BUDGET_COUNT: usize = 96;
const READ_AHEAD_GLYPH_RASTER_BUDGET_US: u128 = 500;
const READ_AHEAD_GLYPH_RASTER_BUDGET_COUNT: usize = 24;

pub(crate) fn item_view_perf_enabled() -> bool {
    env::var(PERF_ITEM_VIEW_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
}

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

#[derive(Default)]
pub(crate) struct ItemViewPerfState {
    frames: HashMap<PaneId, ItemViewPerfFrameState>,
    static_item_visual_stats: HashMap<PaneId, StaticItemVisualPerfStats>,
    static_item_glyph_budget_stats: HashMap<PaneId, GlyphRasterBudgetStats>,
    item_image_stats: HashMap<PaneId, ItemImagePerfStats>,
    details_visual_stats: HashMap<PaneId, DetailsVisualPerfStats>,
    details_glyph_budget_stats: HashMap<PaneId, GlyphRasterBudgetStats>,
    item_interaction_stats: HashMap<PaneId, ItemInteractionPerfStats>,
}

impl ItemViewPerfState {
    fn record_frame(
        &mut self,
        pane_id: PaneId,
        mode: ViewMode,
        item_count: usize,
        visible_count: usize,
        slot_stats: ItemPaintSlotStats,
    ) -> ItemViewPerfPhase {
        let current_frame = ItemViewPerfFrameState::new(mode, item_count, visible_count);
        let previous_frame = self.frames.insert(pane_id, current_frame);
        classify_item_view_perf_phase(previous_frame, current_frame, slot_stats)
    }

    fn clear_pane(&mut self, pane_id: PaneId) {
        self.frames.remove(&pane_id);
        self.clear_layer_stats(pane_id);
    }

    fn clear_layer_stats(&mut self, pane_id: PaneId) {
        self.static_item_visual_stats.remove(&pane_id);
        self.static_item_glyph_budget_stats.remove(&pane_id);
        self.item_image_stats.remove(&pane_id);
        self.details_visual_stats.remove(&pane_id);
        self.details_glyph_budget_stats.remove(&pane_id);
        self.item_interaction_stats.remove(&pane_id);
    }

    fn take_static_item_visual_stats(&mut self, pane_id: PaneId) -> StaticItemVisualPerfStats {
        self.static_item_visual_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    fn take_static_item_glyph_budget_stats(&mut self, pane_id: PaneId) -> GlyphRasterBudgetStats {
        self.static_item_glyph_budget_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    fn take_item_image_stats(&mut self, pane_id: PaneId) -> ItemImagePerfStats {
        self.item_image_stats.remove(&pane_id).unwrap_or_default()
    }

    fn take_details_visual_stats(&mut self, pane_id: PaneId) -> DetailsVisualPerfStats {
        self.details_visual_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    fn take_details_glyph_budget_stats(&mut self, pane_id: PaneId) -> GlyphRasterBudgetStats {
        self.details_glyph_budget_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    fn take_item_interaction_stats(&mut self, pane_id: PaneId) -> ItemInteractionPerfStats {
        self.item_interaction_stats
            .remove(&pane_id)
            .unwrap_or_default()
    }

    fn record_static_item_visual_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.static_item_visual_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    fn record_static_item_visual_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.static_item_visual_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }

    fn record_static_item_glyph_budget_stats(
        &mut self,
        pane_id: PaneId,
        stats: GlyphRasterBudgetStats,
    ) {
        self.static_item_glyph_budget_stats
            .entry(pane_id)
            .or_default()
            .add(stats);
    }

    fn record_item_image_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
        source_stats: ItemImageSourcePerfStats,
    ) {
        self.item_image_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count, source_stats);
    }

    fn record_item_image_paint(&mut self, pane_id: PaneId, elapsed: Duration, count: usize) {
        self.item_image_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }

    fn record_details_visual_prepaint(&mut self, pane_id: PaneId, elapsed: Duration, count: usize) {
        self.details_visual_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    fn record_details_visual_paint(&mut self, pane_id: PaneId, elapsed: Duration, count: usize) {
        self.details_visual_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }

    fn record_details_glyph_budget_stats(
        &mut self,
        pane_id: PaneId,
        stats: GlyphRasterBudgetStats,
    ) {
        self.details_glyph_budget_stats
            .entry(pane_id)
            .or_default()
            .add(stats);
    }

    fn record_item_interaction_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_interaction_stats
            .entry(pane_id)
            .or_default()
            .record_prepaint(elapsed, count);
    }

    fn record_item_interaction_paint(&mut self, pane_id: PaneId, elapsed: Duration, count: usize) {
        self.item_interaction_stats
            .entry(pane_id)
            .or_default()
            .record_paint(elapsed, count);
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ItemViewPerfLogFrame {
    pub(crate) pane_id: PaneId,
    pub(crate) mode: ViewMode,
    pub(crate) phase: Option<ItemViewPerfPhase>,
    pub(crate) item_count: usize,
    pub(crate) visible_count: usize,
    pub(crate) raw_elapsed: Option<Duration>,
    pub(crate) icon_sync_elapsed: Option<Duration>,
    pub(crate) queue_elapsed: Option<Duration>,
    pub(crate) convert_elapsed: Option<Duration>,
    pub(crate) total_elapsed: Duration,
    pub(crate) slot_stats: ItemPaintSlotStats,
}

pub(crate) fn emit_item_view_perf_log(frame: ItemViewPerfLogFrame) {
    eprintln!(
        "[fika item-view] pane={} mode={:?} phase={} items={} visible={} raw={}us icon_sync={}us queue={}us convert={}us total={}us",
        frame.pane_id.0,
        frame.mode,
        frame
            .phase
            .map(ItemViewPerfPhase::label)
            .unwrap_or("unknown"),
        frame.item_count,
        frame.visible_count,
        duration_micros(frame.raw_elapsed),
        duration_micros(frame.icon_sync_elapsed),
        duration_micros(frame.queue_elapsed),
        duration_micros(frame.convert_elapsed),
        frame.total_elapsed.as_micros(),
    );
    if frame.slot_stats.has_activity() {
        eprintln!(
            "[fika item-paint-slots] pane={} mode={:?} inserted={} content={} geometry={} visual={} unchanged={} removed={} entries={}",
            frame.pane_id.0,
            frame.mode,
            frame.slot_stats.inserted,
            frame.slot_stats.content_changed,
            frame.slot_stats.geometry_changed,
            frame.slot_stats.visual_changed,
            frame.slot_stats.unchanged,
            frame.slot_stats.removed,
            frame.slot_stats.entries,
        );
    }
}

fn duration_micros(duration: Option<Duration>) -> u128 {
    duration.map_or(0, |elapsed| elapsed.as_micros())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemLayerPerfStats {
    pub(crate) prepaint_count: usize,
    pub(crate) prepaint_us: u128,
    pub(crate) paint_count: usize,
    pub(crate) paint_us: u128,
}

pub(crate) type StaticItemVisualPerfStats = ItemLayerPerfStats;
pub(crate) type DetailsVisualPerfStats = ItemLayerPerfStats;
pub(crate) type ItemInteractionPerfStats = ItemLayerPerfStats;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct GlyphRasterBudgetStats {
    pub(crate) requested: usize,
    pub(crate) cache_hits: usize,
    pub(crate) cache_misses: usize,
    pub(crate) computed: usize,
    pub(crate) deferred: usize,
    pub(crate) failed: usize,
    pub(crate) compute_us: u128,
    pub(crate) budget_exhausted: bool,
}

impl GlyphRasterBudgetStats {
    pub(crate) fn has_activity(self) -> bool {
        self.requested > 0
            || self.computed > 0
            || self.deferred > 0
            || self.failed > 0
            || self.budget_exhausted
    }

    fn add(&mut self, stats: Self) {
        self.requested += stats.requested;
        self.cache_hits += stats.cache_hits;
        self.cache_misses += stats.cache_misses;
        self.computed += stats.computed;
        self.deferred += stats.deferred;
        self.failed += stats.failed;
        self.compute_us += stats.compute_us;
        self.budget_exhausted |= stats.budget_exhausted;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GlyphRasterMissBudget {
    max_compute_us: u128,
    max_computes: usize,
    stats: GlyphRasterBudgetStats,
}

impl GlyphRasterMissBudget {
    pub(crate) fn visible() -> Self {
        Self::new(
            VISIBLE_GLYPH_RASTER_BUDGET_US,
            VISIBLE_GLYPH_RASTER_BUDGET_COUNT,
        )
    }

    pub(crate) fn read_ahead() -> Self {
        Self::new(
            READ_AHEAD_GLYPH_RASTER_BUDGET_US,
            READ_AHEAD_GLYPH_RASTER_BUDGET_COUNT,
        )
    }

    fn new(max_compute_us: u128, max_computes: usize) -> Self {
        Self {
            max_compute_us,
            max_computes,
            stats: GlyphRasterBudgetStats::default(),
        }
    }

    pub(crate) fn record_cache_hit(&mut self) {
        self.stats.requested += 1;
        self.stats.cache_hits += 1;
    }

    pub(crate) fn record_cache_miss(&mut self) {
        self.stats.requested += 1;
        self.stats.cache_misses += 1;
    }

    pub(crate) fn allow_compute(&mut self) -> bool {
        if self.stats.computed >= self.max_computes || self.stats.compute_us >= self.max_compute_us
        {
            self.stats.budget_exhausted = true;
            return false;
        }
        true
    }

    pub(crate) fn record_compute(&mut self, elapsed: Duration) {
        self.stats.computed += 1;
        self.stats.compute_us += elapsed.as_micros();
        if self.stats.computed >= self.max_computes || self.stats.compute_us >= self.max_compute_us
        {
            self.stats.budget_exhausted = true;
        }
    }

    pub(crate) fn record_deferred(&mut self) {
        self.stats.deferred += 1;
        self.stats.budget_exhausted = true;
    }

    pub(crate) fn record_failed(&mut self, elapsed: Duration) {
        self.stats.failed += 1;
        self.stats.compute_us += elapsed.as_micros();
    }

    pub(crate) fn stats(self) -> GlyphRasterBudgetStats {
        self.stats
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemImageSourcePerfStats {
    pub(crate) theme_loaded: usize,
    pub(crate) theme_decoded: usize,
    pub(crate) theme_retained: usize,
    pub(crate) theme_placeholder: usize,
    pub(crate) theme_prewarm_loaded: usize,
    pub(crate) theme_prewarm_decoded: usize,
    pub(crate) theme_prewarm_retained: usize,
    pub(crate) theme_prewarm_pending: usize,
    pub(crate) thumbnail_loaded: usize,
    pub(crate) thumbnail_decoded: usize,
    pub(crate) thumbnail_retained: usize,
    pub(crate) thumbnail_fallback: usize,
}

impl ItemImageSourcePerfStats {
    fn add(&mut self, stats: Self) {
        self.theme_loaded += stats.theme_loaded;
        self.theme_decoded += stats.theme_decoded;
        self.theme_retained += stats.theme_retained;
        self.theme_placeholder += stats.theme_placeholder;
        self.theme_prewarm_loaded += stats.theme_prewarm_loaded;
        self.theme_prewarm_decoded += stats.theme_prewarm_decoded;
        self.theme_prewarm_retained += stats.theme_prewarm_retained;
        self.theme_prewarm_pending += stats.theme_prewarm_pending;
        self.thumbnail_loaded += stats.thumbnail_loaded;
        self.thumbnail_decoded += stats.thumbnail_decoded;
        self.thumbnail_retained += stats.thumbnail_retained;
        self.thumbnail_fallback += stats.thumbnail_fallback;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemImagePerfStats {
    pub(crate) prepaint_count: usize,
    pub(crate) prepaint_us: u128,
    pub(crate) paint_count: usize,
    pub(crate) paint_us: u128,
    pub(crate) sources: ItemImageSourcePerfStats,
}

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

impl ItemImagePerfStats {
    pub(super) fn has_activity(self) -> bool {
        self.prepaint_count > 0 || self.paint_count > 0
    }

    pub(super) fn record_prepaint(
        &mut self,
        elapsed: Duration,
        count: usize,
        source_stats: ItemImageSourcePerfStats,
    ) {
        self.prepaint_count += count;
        self.prepaint_us += elapsed.as_micros();
        self.sources.add(source_stats);
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
    fn item_view_perf_env_flag_truthy_values_are_explicit() {
        assert!(env_flag_is_truthy("1"));
        assert!(env_flag_is_truthy(" true "));
        assert!(env_flag_is_truthy("YES"));
        assert!(env_flag_is_truthy("on"));
        assert!(!env_flag_is_truthy(""));
        assert!(!env_flag_is_truthy("0"));
        assert!(!env_flag_is_truthy("false"));
        assert!(!env_flag_is_truthy("disabled"));
    }

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

    #[test]
    fn item_view_perf_state_can_clear_layer_stats_without_resetting_phase_history() {
        let mut state = ItemViewPerfState::default();
        let pane_id = PaneId(1);

        assert_eq!(
            state.record_frame(
                pane_id,
                ViewMode::Compact,
                48,
                32,
                ItemPaintSlotStats::default(),
            ),
            ItemViewPerfPhase::Initial
        );

        state.clear_layer_stats(pane_id);

        assert_eq!(
            state.record_frame(
                pane_id,
                ViewMode::Icons,
                48,
                40,
                ItemPaintSlotStats::default(),
            ),
            ItemViewPerfPhase::ModeSwitch
        );

        state.clear_pane(pane_id);

        assert_eq!(
            state.record_frame(
                pane_id,
                ViewMode::Icons,
                48,
                40,
                ItemPaintSlotStats::default(),
            ),
            ItemViewPerfPhase::Initial
        );
    }

    #[test]
    fn glyph_raster_miss_budget_tracks_hits_and_defers_over_budget() {
        let mut budget = GlyphRasterMissBudget::new(10, 1);

        budget.record_cache_hit();
        budget.record_cache_miss();
        assert!(budget.allow_compute());
        budget.record_compute(Duration::from_micros(4));
        assert!(!budget.allow_compute());
        budget.record_deferred();

        assert_eq!(
            budget.stats(),
            GlyphRasterBudgetStats {
                requested: 2,
                cache_hits: 1,
                cache_misses: 1,
                computed: 1,
                deferred: 1,
                failed: 0,
                compute_us: 4,
                budget_exhausted: true,
            }
        );
    }
}

impl FikaApp {
    pub(crate) fn record_item_view_perf_frame(
        &mut self,
        pane_id: PaneId,
        mode: ViewMode,
        item_count: usize,
        visible_count: usize,
        slot_stats: ItemPaintSlotStats,
    ) -> ItemViewPerfPhase {
        self.item_view_perf
            .record_frame(pane_id, mode, item_count, visible_count, slot_stats)
    }

    pub(crate) fn clear_item_view_perf_state(&mut self, pane_id: PaneId) {
        self.item_view_perf.clear_pane(pane_id);
    }

    pub(crate) fn clear_item_view_perf_layer_stats(&mut self, pane_id: PaneId) {
        self.item_view_perf.clear_layer_stats(pane_id);
    }

    pub(super) fn take_static_item_text_shape_cache_stats(
        &mut self,
        pane_id: PaneId,
    ) -> TextShapeCacheStats {
        self.static_item_text_shape_caches
            .get_mut(&pane_id)
            .map(StaticItemTextShapeCache::take_stats)
            .unwrap_or_default()
    }

    pub(super) fn take_static_item_glyph_raster_cache_stats(
        &mut self,
        pane_id: PaneId,
    ) -> TextShapeCacheStats {
        self.static_item_text_shape_caches
            .get_mut(&pane_id)
            .map(StaticItemTextShapeCache::take_glyph_stats)
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

    pub(super) fn take_details_glyph_raster_cache_stats(
        &mut self,
        pane_id: PaneId,
    ) -> TextShapeCacheStats {
        self.details_text_shape_caches
            .get_mut(&pane_id)
            .map(DetailsTextShapeCache::take_glyph_stats)
            .unwrap_or_default()
    }

    pub(super) fn take_static_item_visual_perf_stats(
        &mut self,
        pane_id: PaneId,
    ) -> StaticItemVisualPerfStats {
        self.item_view_perf.take_static_item_visual_stats(pane_id)
    }

    pub(super) fn take_static_item_glyph_budget_stats(
        &mut self,
        pane_id: PaneId,
    ) -> GlyphRasterBudgetStats {
        self.item_view_perf
            .take_static_item_glyph_budget_stats(pane_id)
    }

    pub(super) fn take_item_image_perf_stats(&mut self, pane_id: PaneId) -> ItemImagePerfStats {
        self.item_view_perf.take_item_image_stats(pane_id)
    }

    pub(super) fn take_details_visual_perf_stats(
        &mut self,
        pane_id: PaneId,
    ) -> DetailsVisualPerfStats {
        self.item_view_perf.take_details_visual_stats(pane_id)
    }

    pub(super) fn take_details_glyph_budget_stats(
        &mut self,
        pane_id: PaneId,
    ) -> GlyphRasterBudgetStats {
        self.item_view_perf.take_details_glyph_budget_stats(pane_id)
    }

    pub(super) fn take_item_interaction_perf_stats(
        &mut self,
        pane_id: PaneId,
    ) -> ItemInteractionPerfStats {
        self.item_view_perf.take_item_interaction_stats(pane_id)
    }

    pub(super) fn record_static_item_visual_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_static_item_visual_prepaint(pane_id, elapsed, count);
    }

    pub(super) fn record_static_item_visual_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_static_item_visual_paint(pane_id, elapsed, count);
    }

    pub(super) fn record_static_item_glyph_budget_stats(
        &mut self,
        pane_id: PaneId,
        stats: GlyphRasterBudgetStats,
    ) {
        self.item_view_perf
            .record_static_item_glyph_budget_stats(pane_id, stats);
    }

    pub(super) fn record_item_image_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
        source_stats: ItemImageSourcePerfStats,
    ) {
        self.item_view_perf
            .record_item_image_prepaint(pane_id, elapsed, count, source_stats);
    }

    pub(super) fn record_item_image_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_item_image_paint(pane_id, elapsed, count);
    }

    pub(super) fn record_details_visual_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_details_visual_prepaint(pane_id, elapsed, count);
    }

    pub(super) fn record_details_visual_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_details_visual_paint(pane_id, elapsed, count);
    }

    pub(super) fn record_details_glyph_budget_stats(
        &mut self,
        pane_id: PaneId,
        stats: GlyphRasterBudgetStats,
    ) {
        self.item_view_perf
            .record_details_glyph_budget_stats(pane_id, stats);
    }

    pub(super) fn record_item_interaction_prepaint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_item_interaction_prepaint(pane_id, elapsed, count);
    }

    pub(super) fn record_item_interaction_paint(
        &mut self,
        pane_id: PaneId,
        elapsed: Duration,
        count: usize,
    ) {
        self.item_view_perf
            .record_item_interaction_paint(pane_id, elapsed, count);
    }
}
