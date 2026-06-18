use std::collections::{HashMap, HashSet};

use fika_core::PaneId;
use gpui::{ScrollHandle, point, px};

const LAYOUT_CHANGE_AUTHORITATIVE_FRAMES: u8 = 2;
const VIEW_SYNC_AUTHORITATIVE_FRAMES: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollSync {
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) max_scroll_x: f32,
    pub(crate) max_scroll_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollViewSnapshot {
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) max_scroll_x: f32,
    pub(crate) max_scroll_y: f32,
}

impl ItemViewScrollViewSnapshot {
    pub(crate) fn new(scroll_x: f32, scroll_y: f32, max_scroll_x: f32, max_scroll_y: f32) -> Self {
        Self {
            scroll_x,
            scroll_y,
            max_scroll_x,
            max_scroll_y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ItemViewScrollSyncAction {
    None,
    SyncHandleToView,
    SyncView(ItemViewScrollSync),
}

impl ItemViewScrollSyncAction {
    pub(crate) fn into_outcome(
        self,
        view: ItemViewScrollViewSnapshot,
    ) -> ItemViewScrollSyncOutcome {
        match self {
            ItemViewScrollSyncAction::None | ItemViewScrollSyncAction::SyncHandleToView => {
                ItemViewScrollSyncOutcome {
                    sync: None,
                    changed: false,
                }
            }
            ItemViewScrollSyncAction::SyncView(sync) => ItemViewScrollSyncOutcome {
                sync: Some(sync),
                changed: scroll_sync_changes_view(view, sync),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollSyncOutcome {
    pub(crate) sync: Option<ItemViewScrollSync>,
    pub(crate) changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollBoundsSync {
    pub(crate) action: ItemViewScrollSyncAction,
    pub(crate) handle_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollDragFinish {
    pub(crate) action: ItemViewScrollSyncAction,
    pub(crate) was_dragging: bool,
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
    authoritative_scroll: HashMap<PaneId, u8>,
    scrollbar_dragging: HashSet<PaneId>,
}

impl ItemViewScrollState {
    pub(crate) fn handle_for_pane(&mut self, pane_id: PaneId) -> ScrollHandle {
        self.handles.entry(pane_id).or_default().clone()
    }

    pub(crate) fn mark_authoritative_for_frames(&mut self, pane_id: PaneId, frames: u8) {
        self.authoritative_scroll.insert(pane_id, frames);
    }

    pub(crate) fn clear_authoritative_scroll(&mut self, pane_id: PaneId) {
        self.authoritative_scroll.remove(&pane_id);
    }

    pub(crate) fn has_authoritative_scroll(&self, pane_id: PaneId) -> bool {
        self.authoritative_scroll.contains_key(&pane_id)
    }

    pub(crate) fn tick_authoritative_scroll(&mut self, pane_id: PaneId) {
        if let Some(remaining) = self.authoritative_scroll.get_mut(&pane_id) {
            if *remaining <= 1 {
                self.authoritative_scroll.remove(&pane_id);
            } else {
                *remaining -= 1;
            }
        }
    }

    pub(crate) fn is_scrollbar_dragging(&self, pane_id: PaneId) -> bool {
        self.scrollbar_dragging.contains(&pane_id)
    }

    pub(crate) fn begin_scrollbar_drag(&mut self, pane_id: PaneId) -> bool {
        self.authoritative_scroll.remove(&pane_id);
        self.scrollbar_dragging.insert(pane_id)
    }

    fn finish_scrollbar_drag(&mut self, pane_id: PaneId) -> bool {
        self.scrollbar_dragging.remove(&pane_id)
    }

    pub(crate) fn clear_transient_state(&mut self, pane_id: PaneId) {
        self.authoritative_scroll.remove(&pane_id);
        self.scrollbar_dragging.remove(&pane_id);
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

    pub(crate) fn sync_action_from_handle(
        &mut self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> ItemViewScrollSyncAction {
        if self.is_scrollbar_dragging(pane_id) {
            return self
                .sync_from_authoritative_handle(pane_id, view_max_scroll_x, view_max_scroll_y)
                .map(ItemViewScrollSyncAction::SyncView)
                .unwrap_or(ItemViewScrollSyncAction::None);
        }
        if self.has_authoritative_scroll(pane_id) {
            self.sync_handle_to_view(pane_id, view_scroll_x, view_scroll_y);
            return ItemViewScrollSyncAction::SyncHandleToView;
        }
        let Some(sync) = self.sync_from_handle(
            pane_id,
            view_scroll_x,
            view_scroll_y,
            view_max_scroll_x,
            view_max_scroll_y,
        ) else {
            return ItemViewScrollSyncAction::None;
        };
        if !scroll_offset_matches(view_scroll_x, sync.scroll_x)
            || !scroll_offset_matches(view_scroll_y, sync.scroll_y)
        {
            self.clear_authoritative_scroll(pane_id);
        }
        ItemViewScrollSyncAction::SyncView(sync)
    }

    pub(crate) fn sync_action_from_authoritative_handle(
        &self,
        pane_id: PaneId,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> ItemViewScrollSyncAction {
        self.sync_from_authoritative_handle(pane_id, view_max_scroll_x, view_max_scroll_y)
            .map(ItemViewScrollSyncAction::SyncView)
            .unwrap_or(ItemViewScrollSyncAction::None)
    }

    pub(crate) fn sync_after_bounds_update(
        &mut self,
        pane_id: PaneId,
        view_scroll_x: f32,
        view_scroll_y: f32,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> ItemViewScrollBoundsSync {
        if self.is_scrollbar_dragging(pane_id) {
            let action = self
                .sync_from_authoritative_handle(pane_id, view_max_scroll_x, view_max_scroll_y)
                .map(ItemViewScrollSyncAction::SyncView)
                .unwrap_or(ItemViewScrollSyncAction::None);
            return ItemViewScrollBoundsSync {
                action,
                handle_changed: false,
            };
        }
        let handle_changed = self.sync_handle_to_view(pane_id, view_scroll_x, view_scroll_y);
        self.tick_authoritative_scroll(pane_id);
        ItemViewScrollBoundsSync {
            action: ItemViewScrollSyncAction::SyncHandleToView,
            handle_changed,
        }
    }

    pub(crate) fn finish_scrollbar_drag_with_sync(
        &mut self,
        pane_id: PaneId,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> ItemViewScrollDragFinish {
        let was_dragging = self.finish_scrollbar_drag(pane_id);
        let action = self.sync_action_from_authoritative_handle(
            pane_id,
            view_max_scroll_x,
            view_max_scroll_y,
        );
        ItemViewScrollDragFinish {
            action,
            was_dragging,
        }
    }

    fn sync_from_authoritative_handle(
        &self,
        pane_id: PaneId,
        view_max_scroll_x: f32,
        view_max_scroll_y: f32,
    ) -> Option<ItemViewScrollSync> {
        let observation = self.handle_observation(pane_id)?;
        Some(sync_from_authoritative_handle_observation(
            observation,
            view_max_scroll_x,
            view_max_scroll_y,
        ))
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
        self.mark_authoritative_for_frames(pane_id, LAYOUT_CHANGE_AUTHORITATIVE_FRAMES);
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

    pub(crate) fn sync_handle_to_view_authoritatively(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
    ) -> bool {
        self.mark_authoritative_for_frames(pane_id, VIEW_SYNC_AUTHORITATIVE_FRAMES);
        self.sync_handle_to_view(pane_id, scroll_x, scroll_y)
    }

    pub(crate) fn sync_handle_after_user_scroll(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
    ) -> bool {
        self.clear_authoritative_scroll(pane_id);
        self.sync_handle_to_view(pane_id, scroll_x, scroll_y)
    }

    pub(crate) fn sync_handle_to_view_clearing_transients(
        &mut self,
        pane_id: PaneId,
        scroll_x: f32,
        scroll_y: f32,
    ) -> bool {
        self.clear_transient_state(pane_id);
        self.sync_handle_to_view(pane_id, scroll_x, scroll_y)
    }

    pub(crate) fn reset_pane(&mut self, pane_id: PaneId) {
        if let Some(scroll_handle) = self.handles.get(&pane_id) {
            scroll_handle.set_offset(point(px(0.0), px(0.0)));
        }
        self.clear_transient_state(pane_id);
    }

    pub(crate) fn remove_pane(&mut self, pane_id: PaneId) {
        self.handles.remove(&pane_id);
        self.clear_transient_state(pane_id);
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

fn sync_from_authoritative_handle_observation(
    observation: ItemViewScrollHandleObservation,
    view_max_scroll_x: f32,
    view_max_scroll_y: f32,
) -> ItemViewScrollSync {
    let (scroll_x, max_scroll_x) =
        sync_axis_from_authoritative_handle_observation(observation.scroll_x, view_max_scroll_x);
    let (scroll_y, max_scroll_y) =
        sync_axis_from_authoritative_handle_observation(observation.scroll_y, view_max_scroll_y);
    ItemViewScrollSync {
        scroll_x,
        scroll_y,
        max_scroll_x,
        max_scroll_y,
    }
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

fn sync_axis_from_authoritative_handle_observation(
    observed_scroll: f32,
    view_max_scroll: f32,
) -> (f32, f32) {
    let view_max_scroll = view_max_scroll.max(0.0);
    if view_max_scroll <= 0.5 {
        return (0.0, 0.0);
    }
    let observed_scroll = observed_scroll.max(0.0);
    if observed_scroll <= 0.5 {
        return (0.0, view_max_scroll);
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

pub(crate) fn scroll_sync_changes_view(
    view: ItemViewScrollViewSnapshot,
    sync: ItemViewScrollSync,
) -> bool {
    !scroll_offset_matches(view.scroll_x, sync.scroll_x)
        || !scroll_offset_matches(view.scroll_y, sync.scroll_y)
        || !scroll_offset_matches(view.max_scroll_x, sync.max_scroll_x)
        || !scroll_offset_matches(view.max_scroll_y, sync.max_scroll_y)
}

fn scroll_offset_matches(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
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
        assert!(state.has_authoritative_scroll(pane_id));
        state.tick_authoritative_scroll(pane_id);
        assert!(state.has_authoritative_scroll(pane_id));
        state.tick_authoritative_scroll(pane_id);
        assert!(!state.has_authoritative_scroll(pane_id));
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
    fn scroll_state_syncs_handle_to_view_authoritatively() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        assert!(state.sync_handle_to_view_authoritatively(pane_id, 180.0, 40.0));

        assert_eq!(handle.offset(), point(px(-180.0), px(-40.0)));
        assert!(state.has_authoritative_scroll(pane_id));
        state.tick_authoritative_scroll(pane_id);
        assert!(state.has_authoritative_scroll(pane_id));
        state.tick_authoritative_scroll(pane_id);
        assert!(!state.has_authoritative_scroll(pane_id));
    }

    #[test]
    fn scroll_state_user_scroll_clears_authoritative_state_and_syncs_handle() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        state.mark_authoritative_for_frames(pane_id, 2);
        assert!(state.sync_handle_after_user_scroll(pane_id, 180.0, 40.0));

        assert_eq!(handle.offset(), point(px(-180.0), px(-40.0)));
        assert!(!state.has_authoritative_scroll(pane_id));
    }

    #[test]
    fn scroll_state_syncs_handle_to_view_while_clearing_transients() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        state.mark_authoritative_for_frames(pane_id, 2);
        state.begin_scrollbar_drag(pane_id);
        assert!(state.sync_handle_to_view_clearing_transients(pane_id, 180.0, 40.0));

        assert_eq!(handle.offset(), point(px(-180.0), px(-40.0)));
        assert!(!state.has_authoritative_scroll(pane_id));
        assert!(!state.is_scrollbar_dragging(pane_id));
    }

    #[test]
    fn scroll_state_owns_authoritative_and_drag_transient_state() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();

        state.mark_authoritative_for_frames(pane_id, 2);
        assert!(state.has_authoritative_scroll(pane_id));
        state.tick_authoritative_scroll(pane_id);
        assert!(state.has_authoritative_scroll(pane_id));
        state.tick_authoritative_scroll(pane_id);
        assert!(!state.has_authoritative_scroll(pane_id));

        state.mark_authoritative_for_frames(pane_id, 1);
        assert!(state.begin_scrollbar_drag(pane_id));
        assert!(!state.has_authoritative_scroll(pane_id));
        assert!(state.is_scrollbar_dragging(pane_id));
        assert!(state.finish_scrollbar_drag(pane_id));
        assert!(!state.is_scrollbar_dragging(pane_id));

        state.mark_authoritative_for_frames(pane_id, 1);
        state.begin_scrollbar_drag(pane_id);
        state.clear_transient_state(pane_id);
        assert!(!state.has_authoritative_scroll(pane_id));
        assert!(!state.is_scrollbar_dragging(pane_id));
    }

    #[test]
    fn scroll_state_sync_action_respects_authoritative_and_drag_modes() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        state.mark_authoritative_for_frames(pane_id, 1);
        handle.set_offset(point(px(0.0), px(0.0)));
        assert_eq!(
            state.sync_action_from_handle(pane_id, 180.0, 0.0, 1_000.0, 0.0),
            ItemViewScrollSyncAction::SyncHandleToView
        );
        assert_eq!(handle.offset().x, px(-180.0));

        state.begin_scrollbar_drag(pane_id);
        handle.set_offset(point(px(-320.0), px(0.0)));
        assert_eq!(
            state.sync_action_from_handle(pane_id, 180.0, 0.0, 1_000.0, 0.0),
            ItemViewScrollSyncAction::SyncView(ItemViewScrollSync {
                scroll_x: 320.0,
                scroll_y: 0.0,
                max_scroll_x: 1_000.0,
                max_scroll_y: 0.0,
            })
        );
    }

    #[test]
    fn scroll_state_bounds_update_syncs_handle_or_drag_view() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        state.mark_authoritative_for_frames(pane_id, 1);
        assert_eq!(
            state.sync_after_bounds_update(pane_id, 180.0, 0.0, 1_000.0, 0.0),
            ItemViewScrollBoundsSync {
                action: ItemViewScrollSyncAction::SyncHandleToView,
                handle_changed: true,
            }
        );
        assert_eq!(handle.offset().x, px(-180.0));
        assert!(!state.has_authoritative_scroll(pane_id));

        state.begin_scrollbar_drag(pane_id);
        handle.set_offset(point(px(-320.0), px(0.0)));
        assert_eq!(
            state.sync_after_bounds_update(pane_id, 180.0, 0.0, 1_000.0, 0.0),
            ItemViewScrollBoundsSync {
                action: ItemViewScrollSyncAction::SyncView(ItemViewScrollSync {
                    scroll_x: 320.0,
                    scroll_y: 0.0,
                    max_scroll_x: 1_000.0,
                    max_scroll_y: 0.0,
                }),
                handle_changed: false,
            }
        );
    }

    #[test]
    fn scroll_state_finish_drag_reports_action_and_drag_state() {
        let pane_id = PaneId(1);
        let mut state = ItemViewScrollState::default();
        let handle = state.handle_for_pane(pane_id);

        state.begin_scrollbar_drag(pane_id);
        handle.set_offset(point(px(-480.0), px(0.0)));

        assert_eq!(
            state.finish_scrollbar_drag_with_sync(pane_id, 1_000.0, 0.0),
            ItemViewScrollDragFinish {
                action: ItemViewScrollSyncAction::SyncView(ItemViewScrollSync {
                    scroll_x: 480.0,
                    scroll_y: 0.0,
                    max_scroll_x: 1_000.0,
                    max_scroll_y: 0.0,
                }),
                was_dragging: true,
            }
        );
        assert!(!state.is_scrollbar_dragging(pane_id));
    }

    #[test]
    fn scroll_sync_action_outcome_reports_view_change_only_for_sync_view() {
        let view = ItemViewScrollViewSnapshot::new(100.0, 0.0, 1_000.0, 0.0);

        assert_eq!(
            ItemViewScrollSyncAction::SyncHandleToView.into_outcome(view),
            ItemViewScrollSyncOutcome {
                sync: None,
                changed: false,
            }
        );
        assert_eq!(
            ItemViewScrollSyncAction::SyncView(ItemViewScrollSync {
                scroll_x: 100.2,
                scroll_y: 0.0,
                max_scroll_x: 1_000.0,
                max_scroll_y: 0.0,
            })
            .into_outcome(view),
            ItemViewScrollSyncOutcome {
                sync: Some(ItemViewScrollSync {
                    scroll_x: 100.2,
                    scroll_y: 0.0,
                    max_scroll_x: 1_000.0,
                    max_scroll_y: 0.0,
                }),
                changed: false,
            }
        );
        assert_eq!(
            ItemViewScrollSyncAction::SyncView(ItemViewScrollSync {
                scroll_x: 180.0,
                scroll_y: 0.0,
                max_scroll_x: 1_000.0,
                max_scroll_y: 0.0,
            })
            .into_outcome(view),
            ItemViewScrollSyncOutcome {
                sync: Some(ItemViewScrollSync {
                    scroll_x: 180.0,
                    scroll_y: 0.0,
                    max_scroll_x: 1_000.0,
                    max_scroll_y: 0.0,
                }),
                changed: true,
            }
        );
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
    fn authoritative_handle_sync_accepts_drag_offset_even_when_maximum_lags() {
        let observation = ItemViewScrollHandleObservation {
            scroll_x: 220.0,
            scroll_y: 0.0,
            max_scroll_x: 0.0,
            max_scroll_y: 0.0,
            bounds_valid: false,
        };

        assert_eq!(
            sync_from_authoritative_handle_observation(observation, 1_000.0, 0.0),
            ItemViewScrollSync {
                scroll_x: 220.0,
                scroll_y: 0.0,
                max_scroll_x: 1_000.0,
                max_scroll_y: 0.0,
            }
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
