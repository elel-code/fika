const ITEM_VIEW_SCROLL_RESTORE_SETTLE_PASSES: u8 = 2;
const ITEM_VIEW_SCROLL_RESTORE_ZERO_MAX_PASSES: u8 = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PendingItemViewScroll {
    scroll_x: f32,
    scroll_y: f32,
    settle_passes_remaining: u8,
    zero_max_x_passes: u8,
    zero_max_y_passes: u8,
}

impl PendingItemViewScroll {
    pub(crate) fn new(scroll_x: f32, scroll_y: f32) -> Self {
        Self {
            scroll_x: scroll_x.max(0.0),
            scroll_y: scroll_y.max(0.0),
            settle_passes_remaining: ITEM_VIEW_SCROLL_RESTORE_SETTLE_PASSES,
            zero_max_x_passes: 0,
            zero_max_y_passes: 0,
        }
    }

    pub(crate) fn scroll_x(&self) -> f32 {
        self.scroll_x
    }

    pub(crate) fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    pub(crate) fn reset_settle_passes(&mut self) {
        self.settle_passes_remaining = ITEM_VIEW_SCROLL_RESTORE_SETTLE_PASSES;
        self.zero_max_x_passes = 0;
        self.zero_max_y_passes = 0;
    }

    pub(crate) fn retarget(&mut self, scroll_x: f32, scroll_y: f32) {
        self.scroll_x = scroll_x.max(0.0);
        self.scroll_y = scroll_y.max(0.0);
        self.reset_settle_passes();
    }

    pub(crate) fn target_for_max_scroll(
        &mut self,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> (f32, f32) {
        let scroll_x = target_axis_for_max_scroll(
            &mut self.scroll_x,
            &mut self.zero_max_x_passes,
            max_scroll_x,
        );
        let scroll_y = target_axis_for_max_scroll(
            &mut self.scroll_y,
            &mut self.zero_max_y_passes,
            max_scroll_y,
        );
        (scroll_x, scroll_y)
    }

    pub(crate) fn observe_stable_pass(&mut self, stable: bool) -> bool {
        if !stable {
            self.settle_passes_remaining = ITEM_VIEW_SCROLL_RESTORE_SETTLE_PASSES;
            return true;
        }
        self.settle_passes_remaining = self.settle_passes_remaining.saturating_sub(1);
        self.settle_passes_remaining > 0
    }
}

fn target_axis_for_max_scroll(scroll: &mut f32, zero_max_passes: &mut u8, max_scroll: f32) -> f32 {
    let max_scroll = max_scroll.max(0.0);
    if *scroll <= max_scroll {
        *zero_max_passes = 0;
        return *scroll;
    }

    if max_scroll > 0.0 {
        *scroll = max_scroll;
        *zero_max_passes = 0;
        return *scroll;
    }

    *zero_max_passes = zero_max_passes.saturating_add(1);
    if *zero_max_passes >= ITEM_VIEW_SCROLL_RESTORE_ZERO_MAX_PASSES {
        *scroll = 0.0;
        *zero_max_passes = 0;
    }
    *scroll
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_restore_keeps_target_until_bounds_are_stably_large_enough() {
        let mut pending = PendingItemViewScroll::new(180.0, 90.0);

        assert_eq!(pending.target_for_max_scroll(0.0, 0.0), (180.0, 90.0));
        assert_eq!(pending.target_for_max_scroll(20.0, 30.0), (20.0, 30.0));
        assert_eq!(pending.scroll_x(), 20.0);
        assert_eq!(pending.scroll_y(), 30.0);
    }

    #[test]
    fn pending_restore_does_not_accept_transient_zero_max_scroll() {
        let mut pending = PendingItemViewScroll::new(180.0, 90.0);

        for _ in 0..ITEM_VIEW_SCROLL_RESTORE_ZERO_MAX_PASSES - 1 {
            assert_eq!(pending.target_for_max_scroll(0.0, 0.0), (180.0, 90.0));
        }
        assert_eq!(pending.scroll_x(), 180.0);
        assert_eq!(pending.scroll_y(), 90.0);
        assert_eq!(pending.target_for_max_scroll(1_000.0, 500.0), (180.0, 90.0));
    }

    #[test]
    fn pending_restore_eventually_accepts_stable_zero_max_scroll() {
        let mut pending = PendingItemViewScroll::new(180.0, 90.0);

        for _ in 0..ITEM_VIEW_SCROLL_RESTORE_ZERO_MAX_PASSES - 1 {
            assert_eq!(pending.target_for_max_scroll(0.0, 0.0), (180.0, 90.0));
        }
        assert_eq!(pending.target_for_max_scroll(0.0, 0.0), (0.0, 0.0));
        assert_eq!(pending.scroll_x(), 0.0);
        assert_eq!(pending.scroll_y(), 0.0);
    }

    #[test]
    fn pending_restore_requires_consecutive_stable_passes() {
        let mut pending = PendingItemViewScroll::new(180.0, 90.0);

        assert!(pending.observe_stable_pass(true));
        assert!(pending.observe_stable_pass(false));
        assert!(pending.observe_stable_pass(true));
        assert!(!pending.observe_stable_pass(true));
    }
}
