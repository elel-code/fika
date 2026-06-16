use std::time::Duration;

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
