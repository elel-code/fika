use std::collections::HashMap;

use fika_core::PaneId;
use gpui::{ScrollHandle, point, px};

use super::scroll_restore::PendingItemViewScroll;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollSync {
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) max_scroll_x: f32,
    pub(crate) max_scroll_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollRestore {
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) effective_max_scroll_x: f32,
    pub(crate) effective_max_scroll_y: f32,
    pub(crate) handle_changed: bool,
    pub(crate) needs_another_pass: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ItemViewScrollHandleObservation {
    scroll_x: f32,
    scroll_y: f32,
    max_scroll_x: f32,
    max_scroll_y: f32,
    bounds_valid: bool,
}

#[derive(Default)]
pub(crate) struct ItemViewScrollState {
    handles: HashMap<PaneId, ScrollHandle>,
    pending: HashMap<PaneId, PendingItemViewScroll>,
}

impl ItemViewScrollState {
    pub(crate) fn handle_for_pane(&mut self, pane_id: PaneId) -> ScrollHandle {
        self.handles.entry(pane_id).or_default().clone()
    }

    pub(crate) fn sync_from_handle(
        &self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
    ) -> Option<ItemViewScrollSync> {
        if self.pending.contains_key(&pane_id) {
            return None;
        }
        let observation = self.handle_observation(pane_id)?;
        sync_from_handle_observation(observation, view_scroll_x, view_scroll_y)
    }

    pub(crate) fn preserve_for_layout_change(
        &mut self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
    ) -> (f32, f32) {
        let (observed_scroll_x, observed_scroll_y) =
            self.scroll_for_pane(pane_id, view_scroll_x, view_scroll_y);
        let (scroll_x, scroll_y) = if let Some(pending) = self.pending.get_mut(&pane_id) {
            let scroll_x = observed_scroll_x.max(pending.scroll_x());
            let scroll_y = observed_scroll_y.max(pending.scroll_y());
            pending.retarget(scroll_x, scroll_y);
            (scroll_x, scroll_y)
        } else {
            self.pending.insert(
                pane_id,
                PendingItemViewScroll::new(observed_scroll_x, observed_scroll_y),
            );
            (observed_scroll_x, observed_scroll_y)
        };
        self.set_handle_offset(pane_id, scroll_x, scroll_y);
        (scroll_x, scroll_y)
    }

    pub(crate) fn prime_pending_for_render(&self, pane_id: PaneId, scroll_handle: &ScrollHandle) {
        let Some(pending) = self.pending.get(&pane_id) else {
            return;
        };
        set_scroll_handle_offset(scroll_handle, pending.scroll_x(), pending.scroll_y());
    }

    pub(crate) fn effective_max_scroll_for_bounds(
        &self,
        pane_id: PaneId,
        max_scroll_x: f32,
        max_scroll_y: f32,
    ) -> (f32, f32) {
        self.pending
            .get(&pane_id)
            .map_or((max_scroll_x, max_scroll_y), |pending| {
                (
                    max_scroll_x.max(pending.scroll_x()),
                    max_scroll_y.max(pending.scroll_y()),
                )
            })
    }

    pub(crate) fn restore_pending(
        &mut self,
        pane_id: PaneId,
        max_scroll_x: f32,
        max_scroll_y: f32,
        scroll_value_eq: impl Fn(f32, f32) -> bool,
    ) -> Option<ItemViewScrollRestore> {
        let handle_observation = self.handle_observation(pane_id);
        let restore = self.restore_pending_with_observation(
            pane_id,
            max_scroll_x,
            max_scroll_y,
            handle_observation,
            scroll_value_eq,
        )?;
        Some(restore)
    }

    fn restore_pending_with_observation(
        &mut self,
        pane_id: PaneId,
        max_scroll_x: f32,
        max_scroll_y: f32,
        handle_observation: Option<ItemViewScrollHandleObservation>,
        scroll_value_eq: impl Fn(f32, f32) -> bool,
    ) -> Option<ItemViewScrollRestore> {
        let pending = self.pending.get_mut(&pane_id)?;
        let (scroll_x, scroll_y) = pending.target_for_max_scroll(max_scroll_x, max_scroll_y);
        let handle_stable = handle_observation.is_some_and(|observation| {
            scroll_value_eq(observation.scroll_x, scroll_x)
                && scroll_value_eq(observation.scroll_y, scroll_y)
                && observation.bounds_valid
                && scroll_value_eq(observation.max_scroll_x, max_scroll_x.max(0.0))
                && scroll_value_eq(observation.max_scroll_y, max_scroll_y.max(0.0))
                && (scroll_x <= 0.5 || observation.max_scroll_x + 0.5 >= scroll_x)
                && (scroll_y <= 0.5 || observation.max_scroll_y + 0.5 >= scroll_y)
        });
        let stable = handle_stable
            && max_scroll_x.max(0.0) + 0.5 >= scroll_x
            && max_scroll_y.max(0.0) + 0.5 >= scroll_y;
        let needs_another_pass = pending.observe_stable_pass(stable);
        let handle_changed = self.set_handle_offset(pane_id, scroll_x, scroll_y);
        if !needs_another_pass {
            self.pending.remove(&pane_id);
        }

        Some(ItemViewScrollRestore {
            scroll_x,
            scroll_y,
            effective_max_scroll_x: max_scroll_x.max(scroll_x),
            effective_max_scroll_y: max_scroll_y.max(scroll_y),
            handle_changed,
            needs_another_pass,
        })
    }

    pub(crate) fn reset_pane(&mut self, pane_id: PaneId) {
        self.pending.remove(&pane_id);
        if let Some(scroll_handle) = self.handles.get(&pane_id) {
            scroll_handle.set_offset(point(px(0.0), px(0.0)));
        }
    }

    pub(crate) fn remove_pane(&mut self, pane_id: PaneId) {
        self.pending.remove(&pane_id);
        self.handles.remove(&pane_id);
    }

    pub(crate) fn remove_pending(&mut self, pane_id: PaneId) {
        self.pending.remove(&pane_id);
    }

    #[cfg(test)]
    pub(crate) fn pending_scroll_x(&self, pane_id: PaneId) -> Option<f32> {
        self.pending
            .get(&pane_id)
            .map(PendingItemViewScroll::scroll_x)
    }

    #[cfg(test)]
    pub(crate) fn pending_scroll_y(&self, pane_id: PaneId) -> Option<f32> {
        self.pending
            .get(&pane_id)
            .map(PendingItemViewScroll::scroll_y)
    }

    #[cfg(test)]
    pub(crate) fn has_pending(&self, pane_id: PaneId) -> bool {
        self.pending.contains_key(&pane_id)
    }

    fn scroll_for_pane(
        &self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
    ) -> (f32, f32) {
        let view_scroll_x = view_scroll_x.max(0.0);
        let view_scroll_y = view_scroll_y.max(0.0);
        let Some(observation) = self.handle_observation(pane_id) else {
            return (view_scroll_x, view_scroll_y);
        };
        (
            scroll_axis_for_pane(
                observation.scroll_x,
                observation.max_scroll_x,
                view_scroll_x,
                observation.bounds_valid,
            ),
            scroll_axis_for_pane(
                observation.scroll_y,
                observation.max_scroll_y,
                view_scroll_y,
                observation.bounds_valid,
            ),
        )
    }

    fn set_handle_offset(&self, pane_id: PaneId, scroll_x: f32, scroll_y: f32) -> bool {
        let Some(scroll_handle) = self.handles.get(&pane_id) else {
            return false;
        };
        set_scroll_handle_offset(scroll_handle, scroll_x, scroll_y)
    }

    fn handle_observation(&self, pane_id: PaneId) -> Option<ItemViewScrollHandleObservation> {
        let scroll_handle = self.handles.get(&pane_id)?;
        let bounds = scroll_handle.bounds();
        Some(ItemViewScrollHandleObservation {
            scroll_x: (-scroll_handle.offset().x.as_f32()).max(0.0),
            scroll_y: (-scroll_handle.offset().y.as_f32()).max(0.0),
            max_scroll_x: scroll_handle.max_offset().x.as_f32().max(0.0),
            max_scroll_y: scroll_handle.max_offset().y.as_f32().max(0.0),
            bounds_valid: bounds.size.width.as_f32() > 0.0 && bounds.size.height.as_f32() > 0.0,
        })
    }
}

fn handle_is_transient_zero(
    observation: ItemViewScrollHandleObservation,
    view_scroll_x: f32,
    view_scroll_y: f32,
) -> bool {
    observation.bounds_valid
        && observation.max_scroll_x <= 0.5
        && observation.scroll_x <= 0.5
        && observation.max_scroll_y <= 0.5
        && observation.scroll_y <= 0.5
        && (view_scroll_x > 0.5 || view_scroll_y > 0.5)
}

fn sync_from_handle_observation(
    observation: ItemViewScrollHandleObservation,
    view_scroll_x: f32,
    view_scroll_y: f32,
) -> Option<ItemViewScrollSync> {
    if !observation.bounds_valid {
        return None;
    }
    if handle_is_transient_zero(observation, view_scroll_x, view_scroll_y) {
        return None;
    }
    let (scroll_x, max_scroll_x) = sync_axis_from_handle_observation(
        observation.scroll_x,
        observation.max_scroll_x,
        view_scroll_x,
    );
    let (scroll_y, max_scroll_y) = sync_axis_from_handle_observation(
        observation.scroll_y,
        observation.max_scroll_y,
        view_scroll_y,
    );
    Some(ItemViewScrollSync {
        scroll_x,
        scroll_y,
        max_scroll_x,
        max_scroll_y,
    })
}

fn scroll_axis_for_pane(
    observed_scroll: f32,
    observed_max_scroll: f32,
    view_scroll: f32,
    bounds_valid: bool,
) -> f32 {
    let view_scroll = view_scroll.max(0.0);
    let observed_scroll = observed_scroll.max(0.0);
    let observed_max_scroll = observed_max_scroll.max(0.0);
    if bounds_valid {
        return if observed_max_scroll + 0.5 < view_scroll && observed_scroll + 0.5 < view_scroll {
            view_scroll
        } else if observed_max_scroll > 0.0 {
            observed_scroll.clamp(0.0, observed_max_scroll)
        } else {
            observed_scroll.max(view_scroll)
        };
    }
    let scroll = observed_scroll.max(view_scroll);
    if observed_max_scroll > 0.0 {
        scroll.clamp(0.0, observed_max_scroll.max(view_scroll))
    } else {
        scroll
    }
}

fn sync_axis_from_handle_observation(
    observed_scroll: f32,
    observed_max_scroll: f32,
    view_scroll: f32,
) -> (f32, f32) {
    let view_scroll = view_scroll.max(0.0);
    let observed_scroll = observed_scroll.max(0.0);
    let observed_max_scroll = observed_max_scroll.max(0.0);
    if observed_max_scroll + 0.5 < view_scroll && observed_scroll + 0.5 < view_scroll {
        return (view_scroll, view_scroll);
    }
    if observed_max_scroll <= 0.5 || observed_scroll > observed_max_scroll + 0.5 {
        let scroll = observed_scroll.max(view_scroll);
        return (scroll, scroll);
    }
    (
        observed_scroll.clamp(0.0, observed_max_scroll),
        observed_max_scroll,
    )
}

fn set_scroll_handle_offset(scroll_handle: &ScrollHandle, scroll_x: f32, scroll_y: f32) -> bool {
    let current = scroll_handle.offset();
    let next_x = px(-scroll_x.max(0.0));
    let next_y = px(-scroll_y.max(0.0));
    if current.x == next_x && current.y == next_y {
        return false;
    }
    scroll_handle.set_offset(point(next_x, next_y));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_state_preserves_larger_handle_or_view_scroll_for_layout_change() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(-180.0), px(0.0)));

        state.preserve_for_layout_change(pane_id, 120.0, 0.0);

        assert_eq!(state.pending_scroll_x(pane_id), Some(180.0));
    }

    #[test]
    fn scroll_state_preserves_vertical_handle_or_view_scroll_for_layout_change() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(0.0), px(-240.0)));

        state.preserve_for_layout_change(pane_id, 0.0, 120.0);

        assert_eq!(state.pending_scroll_x(pane_id), Some(0.0));
        assert_eq!(state.pending_scroll_y(pane_id), Some(240.0));
        assert_eq!(handle.offset().y, px(-240.0));
    }

    #[test]
    fn scroll_state_does_not_retarget_pending_restore_to_transient_zero() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(-180.0), px(0.0)));
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);

        handle.set_offset(point(px(0.0), px(0.0)));
        state.preserve_for_layout_change(pane_id, 0.0, 0.0);

        assert_eq!(state.pending_scroll_x(pane_id), Some(180.0));
    }

    #[test]
    fn scroll_state_allows_pending_restore_to_retarget_forward() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(-180.0), px(0.0)));
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);

        handle.set_offset(point(px(-260.0), px(0.0)));
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);

        assert_eq!(state.pending_scroll_x(pane_id), Some(260.0));
    }

    #[test]
    fn scroll_state_does_not_sync_transient_zero_handle_over_view_scroll() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 0.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        };

        assert_eq!(sync_from_handle_observation(observation, 180.0, 0.0), None);
        assert_eq!(sync_from_handle_observation(observation, 0.0, 180.0), None);
        assert_eq!(
            sync_from_handle_observation(
                ItemViewScrollHandleObservation {
                    scroll_x: 180.0,
                    scroll_y: 40.0,
                    max_scroll_x: 1_000.0,
                    max_scroll_y: 200.0,
                    bounds_valid: true,
                },
                180.0,
                0.0,
            ),
            Some(ItemViewScrollSync {
                scroll_x: 180.0,
                scroll_y: 40.0,
                max_scroll_x: 1_000.0,
                max_scroll_y: 200.0,
            })
        );
    }

    #[test]
    fn scroll_state_syncs_vertical_offset_for_details_view() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 0.0,
            scroll_y: 96.0,
            max_scroll_x: 0.0,
            max_scroll_y: 400.0,
            bounds_valid: true,
        };

        assert_eq!(
            sync_from_handle_observation(observation, 0.0, 40.0),
            Some(ItemViewScrollSync {
                scroll_x: 0.0,
                scroll_y: 96.0,
                max_scroll_x: 0.0,
                max_scroll_y: 400.0,
            })
        );
    }

    #[test]
    fn scroll_state_does_not_clamp_observed_offset_to_lagging_zero_max_scroll() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        };

        assert_eq!(
            sync_from_handle_observation(observation, 0.0, 0.0),
            Some(ItemViewScrollSync {
                scroll_x: 180.0,
                scroll_y: 0.0,
                max_scroll_x: 180.0,
                max_scroll_y: 0.0,
            })
        );
        assert_eq!(
            sync_from_handle_observation(observation, 220.0, 0.0),
            Some(ItemViewScrollSync {
                scroll_x: 220.0,
                scroll_y: 0.0,
                max_scroll_x: 220.0,
                max_scroll_y: 0.0,
            })
        );
    }

    #[test]
    fn scroll_state_does_not_clamp_observed_offset_to_lagging_small_max_scroll() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 40.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        };

        assert_eq!(
            sync_from_handle_observation(observation, 0.0, 0.0),
            Some(ItemViewScrollSync {
                scroll_x: 180.0,
                scroll_y: 0.0,
                max_scroll_x: 180.0,
                max_scroll_y: 0.0,
            })
        );
    }

    #[test]
    fn scroll_state_does_not_accept_lagging_small_max_scroll_at_start() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 0.0,
            scroll_y: 0.0,
            max_scroll_x: 40.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        };

        assert_eq!(
            sync_from_handle_observation(observation, 180.0, 0.0),
            Some(ItemViewScrollSync {
                scroll_x: 180.0,
                scroll_y: 0.0,
                max_scroll_x: 180.0,
                max_scroll_y: 0.0,
            })
        );
        assert_eq!(scroll_axis_for_pane(0.0, 40.0, 180.0, true), 180.0);
    }

    #[test]
    fn scroll_state_rejects_transient_zero_restore_until_stable() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(-180.0), px(0.0)));
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);
        handle.set_offset(point(px(0.0), px(0.0)));

        let first = state
            .restore_pending(pane_id, 1_000.0, 0.0, approx_eq)
            .unwrap();
        assert_eq!(first.scroll_x, 180.0);
        assert!(first.handle_changed);
        assert!(first.needs_another_pass);
        assert_eq!(handle.offset().x, px(-180.0));

        let stable_handle = Some(ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 1_000.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        });
        assert!(
            state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stable_handle, approx_eq)
                .unwrap()
                .needs_another_pass
        );
        assert!(
            !state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stable_handle, approx_eq)
                .unwrap()
                .needs_another_pass
        );
        assert!(!state.has_pending(pane_id));
    }

    #[test]
    fn scroll_state_restores_vertical_scroll_after_layout_change() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(0.0), px(-240.0)));
        state.preserve_for_layout_change(pane_id, 0.0, 240.0);
        handle.set_offset(point(px(0.0), px(0.0)));

        let first = state
            .restore_pending(pane_id, 0.0, 1_000.0, approx_eq)
            .unwrap();
        assert_eq!(first.scroll_x, 0.0);
        assert_eq!(first.scroll_y, 240.0);
        assert_eq!(first.effective_max_scroll_x, 0.0);
        assert_eq!(first.effective_max_scroll_y, 1_000.0);
        assert!(first.handle_changed);
        assert!(first.needs_another_pass);
        assert_eq!(handle.offset(), point(px(0.0), px(-240.0)));
    }

    #[test]
    fn scroll_state_waits_for_gpui_max_offset_to_match_layout_max() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);

        let stale_handle = Some(ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 600.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        });
        for _ in 0..3 {
            let restore = state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stale_handle, approx_eq)
                .unwrap();
            assert_eq!(restore.scroll_x, 180.0);
            assert!(restore.needs_another_pass);
            assert!(state.has_pending(pane_id));
        }

        let stable_handle = Some(ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 1_000.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        });
        assert!(
            state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stable_handle, approx_eq)
                .unwrap()
                .needs_another_pass
        );
        assert!(
            !state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stable_handle, approx_eq)
                .unwrap()
                .needs_another_pass
        );
        assert!(!state.has_pending(pane_id));
    }

    #[test]
    fn scroll_state_uses_pending_scroll_as_effective_bounds_max() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(-180.0), px(0.0)));
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);

        assert_eq!(
            state.effective_max_scroll_for_bounds(pane_id, 0.0, 0.0),
            (180.0, 0.0)
        );
        assert_eq!(
            state.effective_max_scroll_for_bounds(pane_id, 1_000.0, 0.0),
            (1_000.0, 0.0)
        );
    }

    #[test]
    fn scroll_state_uses_pending_vertical_scroll_as_effective_bounds_max() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(0.0), px(-240.0)));
        state.preserve_for_layout_change(pane_id, 0.0, 240.0);

        assert_eq!(
            state.effective_max_scroll_for_bounds(pane_id, 0.0, 0.0),
            (0.0, 240.0)
        );
        assert_eq!(
            state.effective_max_scroll_for_bounds(pane_id, 0.0, 1_000.0),
            (0.0, 1_000.0)
        );
    }

    #[test]
    fn scroll_state_waits_for_gpui_handle_bounds_before_finishing_restore() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        state.preserve_for_layout_change(pane_id, 180.0, 0.0);

        let invalid_bounds_handle = Some(ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 1_000.0,
            max_scroll_y: 0.0,
            bounds_valid: false,
        });
        for _ in 0..3 {
            let restore = state
                .restore_pending_with_observation(
                    pane_id,
                    1_000.0,
                    0.0,
                    invalid_bounds_handle,
                    approx_eq,
                )
                .unwrap();
            assert_eq!(restore.scroll_x, 180.0);
            assert!(restore.needs_another_pass);
            assert!(state.has_pending(pane_id));
        }

        let transient_zero_handle = Some(ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        });
        for _ in 0..3 {
            let restore = state
                .restore_pending_with_observation(
                    pane_id,
                    1_000.0,
                    0.0,
                    transient_zero_handle,
                    approx_eq,
                )
                .unwrap();
            assert_eq!(restore.scroll_x, 180.0);
            assert!(restore.needs_another_pass);
            assert!(state.has_pending(pane_id));
        }

        let stable_handle = Some(ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 1_000.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        });
        assert!(
            state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stable_handle, approx_eq)
                .unwrap()
                .needs_another_pass
        );
        assert!(
            !state
                .restore_pending_with_observation(pane_id, 1_000.0, 0.0, stable_handle, approx_eq)
                .unwrap()
                .needs_another_pass
        );
        assert!(!state.has_pending(pane_id));
    }

    fn approx_eq(left: f32, right: f32) -> bool {
        (left - right).abs() < 0.5
    }
}
