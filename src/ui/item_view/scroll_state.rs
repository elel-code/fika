use std::collections::HashMap;

use fika_core::PaneId;
use gpui::{ScrollHandle, point, px};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollSync {
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) max_scroll_x: f32,
    pub(crate) max_scroll_y: f32,
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
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> Option<ItemViewScrollSync> {
        let observation = self.handle_observation(pane_id)?;
        sync_from_handle_observation(
            observation,
            view_scroll_x,
            view_scroll_y,
            view_max_scroll_x,
            view_max_scroll_y,
        )
    }

    pub(crate) fn preserve_for_layout_change(
        &mut self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> (f32, f32) {
        let (scroll_x, scroll_y) = self.scroll_for_pane(
            pane_id,
            view_scroll_x,
            view_scroll_y,
            view_max_scroll_x,
            view_max_scroll_y,
        );
        self.set_handle_offset(pane_id, scroll_x, scroll_y);
        (scroll_x, scroll_y)
    }

    pub(crate) fn sync_handle_to_view(
        &self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
    ) -> bool {
        self.set_handle_offset(pane_id, scroll_x, scroll_y)
    }

    pub(crate) fn reset_pane(&mut self, pane_id: PaneId) {
        if let Some(scroll_handle) = self.handles.get(&pane_id) {
            scroll_handle.set_offset(point(px(0.0), px(0.0)));
        }
    }

    pub(crate) fn remove_pane(&mut self, pane_id: PaneId) {
        self.handles.remove(&pane_id);
    }

    fn scroll_for_pane(
        &self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> (f32, f32) {
        let view_max_scroll_x = view_max_scroll_x.max(0.0);
        let view_max_scroll_y = view_max_scroll_y.max(0.0);
        let view_scroll_x = view_scroll_x.clamp(0.0, view_max_scroll_x);
        let view_scroll_y = view_scroll_y.clamp(0.0, view_max_scroll_y);
        let Some(observation) = self.handle_observation(pane_id) else {
            return (view_scroll_x, view_scroll_y);
        };
        (
            scroll_axis_for_pane(
                observation.scroll_x,
                observation.max_scroll_x,
                view_scroll_x,
                view_max_scroll_x,
                observation.bounds_valid,
            ),
            scroll_axis_for_pane(
                observation.scroll_y,
                observation.max_scroll_y,
                view_scroll_y,
                view_max_scroll_y,
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

fn sync_from_handle_observation(
    observation: ItemViewScrollHandleObservation,
    view_scroll_x: f32,
    view_scroll_y: f32,
    view_max_scroll_x: f32,
    view_max_scroll_y: f32,
) -> Option<ItemViewScrollSync> {
    if !observation.bounds_valid {
        return None;
    }
    let (scroll_x, max_scroll_x) = sync_axis_from_handle_observation(
        observation.scroll_x,
        observation.max_scroll_x,
        view_scroll_x,
        view_max_scroll_x,
    );
    let (scroll_y, max_scroll_y) = sync_axis_from_handle_observation(
        observation.scroll_y,
        observation.max_scroll_y,
        view_scroll_y,
        view_max_scroll_y,
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
    view_max_scroll: f32,
    bounds_valid: bool,
) -> f32 {
    let view_max_scroll = view_max_scroll.max(0.0);
    if view_max_scroll <= 0.5 {
        return 0.0;
    }
    let view_scroll = view_scroll.clamp(0.0, view_max_scroll);
    let observed_scroll = observed_scroll.max(0.0);
    let observed_max_scroll = observed_max_scroll.max(0.0);
    if !bounds_valid || observed_max_scroll + 0.5 < view_max_scroll {
        return view_scroll;
    }
    observed_scroll.clamp(0.0, view_max_scroll)
}

fn sync_axis_from_handle_observation(
    observed_scroll: f32,
    observed_max_scroll: f32,
    view_scroll: f32,
    view_max_scroll: f32,
) -> (f32, f32) {
    let view_max_scroll = view_max_scroll.max(0.0);
    if view_max_scroll <= 0.5 {
        return (0.0, 0.0);
    }
    let view_scroll = view_scroll.clamp(0.0, view_max_scroll);
    let observed_scroll = observed_scroll.max(0.0);
    let observed_max_scroll = observed_max_scroll.max(0.0);
    if observed_max_scroll + 0.5 < view_max_scroll {
        return (view_scroll, view_max_scroll);
    }
    (observed_scroll.clamp(0.0, view_max_scroll), view_max_scroll)
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
    fn scroll_axis_accepts_handle_offset_when_maximum_matches_view() {
        assert_eq!(
            scroll_axis_for_pane(180.0, 1_000.0, 120.0, 1_000.0, true),
            180.0
        );
    }

    #[test]
    fn scroll_axis_keeps_view_offset_when_handle_maximum_lags() {
        assert_eq!(scroll_axis_for_pane(0.0, 0.0, 180.0, 1_000.0, true), 180.0);
        assert_eq!(
            scroll_axis_for_pane(90.0, 200.0, 180.0, 1_000.0, true),
            180.0
        );
    }

    #[test]
    fn scroll_state_preserves_view_scroll_for_layout_change_until_handle_bounds_match() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);
        handle.set_offset(point(px(-180.0), px(0.0)));

        state.preserve_for_layout_change(pane_id, 120.0, 0.0, 1_000.0, 0.0);

        assert_eq!(handle.offset().x, px(-120.0));
    }

    #[test]
    fn scroll_state_syncs_handle_to_view_immediately() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        assert!(state.sync_handle_to_view(pane_id, 180.0, 40.0));

        assert_eq!(handle.offset(), point(px(-180.0), px(-40.0)));
    }

    #[test]
    fn scroll_state_does_not_sync_lagging_handle_maximum_over_view_scroll() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 0.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        };

        assert_eq!(
            sync_from_handle_observation(observation, 180.0, 0.0, 1_000.0, 0.0),
            Some(ItemViewScrollSync {
                scroll_x: 180.0,
                scroll_y: 0.0,
                max_scroll_x: 1_000.0,
                max_scroll_y: 0.0,
            })
        );
        assert_eq!(
            sync_from_handle_observation(observation, 0.0, 180.0, 0.0, 1_000.0),
            Some(ItemViewScrollSync {
                scroll_x: 0.0,
                scroll_y: 180.0,
                max_scroll_x: 0.0,
                max_scroll_y: 1_000.0,
            })
        );
    }

    #[test]
    fn scroll_state_clamps_to_view_maximum_when_layout_has_no_scroll_range() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 180.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            bounds_valid: true,
        };

        assert_eq!(
            sync_from_handle_observation(observation, 180.0, 0.0, 0.0, 0.0),
            Some(ItemViewScrollSync {
                scroll_x: 0.0,
                scroll_y: 0.0,
                max_scroll_x: 0.0,
                max_scroll_y: 0.0,
            })
        );
    }
}
