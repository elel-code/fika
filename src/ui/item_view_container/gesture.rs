use std::time::Instant;

use crate::FikaApp;
use fika_core::{PaneId, ScrollDragTracker, ViewPoint};

use super::scroll_offset::ItemViewScrollOffsetBar;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewWheelGesture {
    maximum: f32,
    tracker: ScrollDragTracker,
}

impl ItemViewWheelGesture {
    fn new(maximum: f32) -> Self {
        Self {
            maximum,
            tracker: ScrollDragTracker::default(),
        }
    }

    fn maximum_matches(self, maximum: f32) -> bool {
        (self.maximum - maximum).abs() <= 0.5
    }

    fn sample(&mut self, target_x: f32, at: Instant) {
        self.tracker.sample(
            ViewPoint {
                x: target_x.clamp(0.0, self.maximum),
                y: 0.0,
            },
            at,
        );
    }

    fn velocity_x(self) -> f32 {
        self.tracker.velocity().x
    }
}

impl FikaApp {
    pub(crate) fn begin_item_view_container_wheel_gesture_at(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        now: Instant,
    ) {
        let current_x = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or(bar.value)
            .clamp(0.0, bar.maximum);
        let mut gesture = ItemViewWheelGesture::new(bar.maximum);
        gesture.sample(current_x, now);
        self.item_view_container_wheel_gestures
            .insert(pane_id, gesture);
    }

    pub(crate) fn sample_item_view_container_wheel_gesture_target_at(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        target_x: f32,
        now: Instant,
    ) {
        let Some(gesture) = self.item_view_container_wheel_gestures.get_mut(&pane_id) else {
            return;
        };
        if !gesture.maximum_matches(bar.maximum) {
            let mut replacement = ItemViewWheelGesture::new(bar.maximum);
            replacement.sample(target_x, now);
            *gesture = replacement;
            return;
        }
        gesture.sample(target_x, now);
    }

    pub(crate) fn finish_item_view_container_wheel_gesture_at(
        &mut self,
        pane_id: PaneId,
    ) -> Option<f32> {
        self.item_view_container_wheel_gestures
            .remove(&pane_id)
            .map(ItemViewWheelGesture::velocity_x)
    }

    pub(crate) fn cancel_item_view_container_wheel_gesture_for_pane(&mut self, pane_id: PaneId) {
        self.item_view_container_wheel_gestures.remove(&pane_id);
    }
}
