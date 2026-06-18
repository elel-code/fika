use std::collections::HashSet;

use fika_core::{PaneId, ViewPoint, ViewRect, ViewState};

const RUBBER_BAND_START_DRAG_DISTANCE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PendingRubberBand {
    pub(crate) pane_id: PaneId,
    pub(crate) start: ViewPoint,
}

impl PendingRubberBand {
    pub(crate) fn new(pane_id: PaneId, start: ViewPoint) -> Self {
        Self { pane_id, start }
    }

    pub(crate) fn is_for_pane(self, pane_id: PaneId) -> bool {
        self.pane_id == pane_id
    }

    pub(crate) fn can_activate(self, pane_id: PaneId, current: ViewPoint) -> bool {
        self.is_for_pane(pane_id) && rubber_band_drag_distance_reached(self.start, current)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RubberBandState {
    pub(crate) pane_id: PaneId,
    pub(crate) start: ViewPoint,
    pub(crate) current: ViewPoint,
}

impl RubberBandState {
    pub(crate) fn new(pane_id: PaneId, start: ViewPoint) -> Self {
        Self {
            pane_id,
            start,
            current: start,
        }
    }

    pub(crate) fn is_for_pane(self, pane_id: PaneId) -> bool {
        self.pane_id == pane_id
    }

    pub(crate) fn with_current_for_pane(
        mut self,
        pane_id: PaneId,
        current: ViewPoint,
    ) -> Option<Self> {
        if !self.is_for_pane(pane_id) {
            return None;
        }
        self.current = current;
        Some(self)
    }

    pub(crate) fn rect(self) -> ViewRect {
        let x = self.start.x.min(self.current.x);
        let y = self.start.y.min(self.current.y);
        ViewRect {
            x,
            y,
            width: self.start.x.max(self.current.x) - x,
            height: self.start.y.max(self.current.y) - y,
        }
    }

    pub(crate) fn viewport_rect(self, view: &ViewState) -> ViewRect {
        let rect = self.rect();
        let viewport = ViewRect {
            x: view.scroll_x,
            y: view.scroll_y,
            width: view.viewport_width.max(0.0),
            height: view.viewport_height.max(0.0),
        };
        let x = rect.x.max(viewport.x);
        let y = rect.y.max(viewport.y);
        let right = rect.right().min(viewport.right());
        let bottom = rect.bottom().min(viewport.bottom());
        ViewRect {
            x: (x - view.scroll_x).round(),
            y: (y - view.scroll_y).round(),
            width: (right - x).max(0.0),
            height: (bottom - y).max(0.0),
        }
    }
}

pub(crate) fn finish_rubber_band_for_pane(
    pending: &mut Option<PendingRubberBand>,
    active: &mut Option<RubberBandState>,
    pane_id: PaneId,
) -> bool {
    let mut changed = false;
    if pending
        .as_ref()
        .is_some_and(|pending| pending.is_for_pane(pane_id))
    {
        *pending = None;
        changed = true;
    }
    if active
        .as_ref()
        .is_some_and(|active| active.is_for_pane(pane_id))
    {
        *active = None;
        changed = true;
    }
    changed
}

pub(crate) fn set_rubber_band_selection_activity_for_count(
    selection_panes: &mut HashSet<PaneId>,
    pane_id: PaneId,
    selected_count: usize,
) -> bool {
    if selected_count > 0 {
        selection_panes.insert(pane_id);
        true
    } else {
        selection_panes.remove(&pane_id);
        false
    }
}

pub(crate) fn clear_rubber_band_selection_activity_for_pane(
    selection_panes: &mut HashSet<PaneId>,
    pane_id: PaneId,
) -> bool {
    selection_panes.remove(&pane_id)
}

pub(crate) fn rubber_band_selection_activity_is_active(
    selection_panes: &HashSet<PaneId>,
    pane_id: PaneId,
    selected_count: Option<usize>,
) -> bool {
    selection_panes.contains(&pane_id) && selected_count.is_some_and(|selected| selected > 0)
}

pub(crate) fn active_rubber_band_viewport_rect_for_pane(
    active: Option<RubberBandState>,
    pane_id: PaneId,
    view: &ViewState,
) -> Option<ViewRect> {
    active
        .filter(|band| band.is_for_pane(pane_id))
        .map(|band| band.viewport_rect(view))
}

pub(crate) fn active_rubber_band_is_for_pane(
    active: Option<RubberBandState>,
    pane_id: PaneId,
) -> bool {
    active.is_some_and(|band| band.is_for_pane(pane_id))
}

pub(crate) fn clear_active_rubber_band_for_pane(
    active: &mut Option<RubberBandState>,
    pane_id: PaneId,
) -> bool {
    if !active_rubber_band_is_for_pane(*active, pane_id) {
        return false;
    }
    *active = None;
    true
}

pub(crate) fn press_pending_rubber_band_for_pane(
    pending: &mut Option<PendingRubberBand>,
    active: &mut Option<RubberBandState>,
    pane_id: PaneId,
    start: ViewPoint,
) {
    *active = None;
    *pending = Some(PendingRubberBand::new(pane_id, start));
}

pub(crate) fn start_active_rubber_band_for_pane(
    pending: &mut Option<PendingRubberBand>,
    active: &mut Option<RubberBandState>,
    pane_id: PaneId,
    start: ViewPoint,
) {
    *pending = None;
    *active = Some(RubberBandState::new(pane_id, start));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RubberBandDrag {
    pub(crate) pane_id: PaneId,
}

fn rubber_band_drag_distance_reached(start: ViewPoint, current: ViewPoint) -> bool {
    (start.x - current.x).abs() + (start.y - current.y).abs() >= RUBBER_BAND_START_DRAG_DISTANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_distance_uses_dolphin_like_manhattan_threshold() {
        let start = ViewPoint { x: 10.0, y: 10.0 };

        assert!(!rubber_band_drag_distance_reached(
            start,
            ViewPoint { x: 13.0, y: 12.0 }
        ));
        assert!(rubber_band_drag_distance_reached(
            start,
            ViewPoint { x: 13.0, y: 13.0 }
        ));
        assert!(rubber_band_drag_distance_reached(
            start,
            ViewPoint { x: 4.0, y: 10.0 }
        ));
    }

    #[test]
    fn pending_rubber_band_activation_requires_same_pane_and_drag_distance() {
        let pending = PendingRubberBand::new(PaneId(1), ViewPoint { x: 10.0, y: 10.0 });

        assert!(!pending.can_activate(PaneId(2), ViewPoint { x: 20.0, y: 20.0 }));
        assert!(!pending.can_activate(PaneId(1), ViewPoint { x: 13.0, y: 12.0 }));
        assert!(pending.can_activate(PaneId(1), ViewPoint { x: 13.0, y: 13.0 }));
    }

    #[test]
    fn active_rubber_band_update_requires_same_pane() {
        let start = ViewPoint { x: 10.0, y: 10.0 };
        let current = ViewPoint { x: 20.0, y: 30.0 };
        let band = RubberBandState::new(PaneId(1), start);

        assert_eq!(band.current, start);
        assert!(band.with_current_for_pane(PaneId(2), current).is_none());
        assert_eq!(
            band.with_current_for_pane(PaneId(1), current),
            Some(RubberBandState {
                pane_id: PaneId(1),
                start,
                current
            })
        );
    }

    #[test]
    fn finish_rubber_band_for_pane_clears_only_matching_state() {
        let start = ViewPoint { x: 10.0, y: 10.0 };
        let mut pending = Some(PendingRubberBand::new(PaneId(1), start));
        let mut active = Some(RubberBandState::new(PaneId(2), start));

        assert!(finish_rubber_band_for_pane(
            &mut pending,
            &mut active,
            PaneId(1)
        ));
        assert_eq!(pending, None);
        assert_eq!(active, Some(RubberBandState::new(PaneId(2), start)));

        assert!(finish_rubber_band_for_pane(
            &mut pending,
            &mut active,
            PaneId(2)
        ));
        assert_eq!(pending, None);
        assert_eq!(active, None);
        assert!(!finish_rubber_band_for_pane(
            &mut pending,
            &mut active,
            PaneId(2)
        ));
    }

    #[test]
    fn selection_activity_tracks_selected_count() {
        let mut selection_panes = HashSet::new();

        assert!(set_rubber_band_selection_activity_for_count(
            &mut selection_panes,
            PaneId(1),
            2
        ));
        assert!(selection_panes.contains(&PaneId(1)));
        assert!(!set_rubber_band_selection_activity_for_count(
            &mut selection_panes,
            PaneId(1),
            0
        ));
        assert!(!selection_panes.contains(&PaneId(1)));
    }

    #[test]
    fn selection_activity_clear_and_active_check_use_selected_count() {
        let mut selection_panes = HashSet::from([PaneId(1)]);

        assert!(rubber_band_selection_activity_is_active(
            &selection_panes,
            PaneId(1),
            Some(1)
        ));
        assert!(!rubber_band_selection_activity_is_active(
            &selection_panes,
            PaneId(1),
            Some(0)
        ));
        assert!(!rubber_band_selection_activity_is_active(
            &selection_panes,
            PaneId(2),
            Some(1)
        ));

        assert!(clear_rubber_band_selection_activity_for_pane(
            &mut selection_panes,
            PaneId(1)
        ));
        assert!(!rubber_band_selection_activity_is_active(
            &selection_panes,
            PaneId(1),
            Some(1)
        ));
        assert!(!clear_rubber_band_selection_activity_for_pane(
            &mut selection_panes,
            PaneId(1)
        ));
    }

    #[test]
    fn active_rubber_band_viewport_rect_requires_matching_pane() {
        let band = RubberBandState::new(PaneId(1), ViewPoint { x: 10.0, y: 10.0 })
            .with_current_for_pane(PaneId(1), ViewPoint { x: 30.0, y: 40.0 })
            .unwrap();
        let view = ViewState {
            scroll_x: 5.0,
            scroll_y: 6.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            ..ViewState::default()
        };

        assert_eq!(
            active_rubber_band_viewport_rect_for_pane(Some(band), PaneId(1), &view),
            Some(band.viewport_rect(&view))
        );
        assert_eq!(
            active_rubber_band_viewport_rect_for_pane(Some(band), PaneId(2), &view),
            None
        );
        assert_eq!(
            active_rubber_band_viewport_rect_for_pane(None, PaneId(1), &view),
            None
        );
    }

    #[test]
    fn active_rubber_band_query_and_clear_require_matching_pane() {
        let start = ViewPoint { x: 10.0, y: 10.0 };
        let band = RubberBandState::new(PaneId(1), start);
        let mut active = Some(band);

        assert!(active_rubber_band_is_for_pane(active, PaneId(1)));
        assert!(!active_rubber_band_is_for_pane(active, PaneId(2)));
        assert!(!clear_active_rubber_band_for_pane(&mut active, PaneId(2)));
        assert_eq!(active, Some(band));
        assert!(clear_active_rubber_band_for_pane(&mut active, PaneId(1)));
        assert_eq!(active, None);
        assert!(!active_rubber_band_is_for_pane(active, PaneId(1)));
    }

    #[test]
    fn press_pending_rubber_band_replaces_active_band() {
        let active_start = ViewPoint { x: 1.0, y: 2.0 };
        let pending_start = ViewPoint { x: 10.0, y: 20.0 };
        let mut pending = None;
        let mut active = Some(RubberBandState::new(PaneId(2), active_start));

        press_pending_rubber_band_for_pane(&mut pending, &mut active, PaneId(1), pending_start);

        assert_eq!(active, None);
        assert_eq!(
            pending,
            Some(PendingRubberBand::new(PaneId(1), pending_start))
        );
    }

    #[test]
    fn start_active_rubber_band_replaces_pending_band() {
        let pending_start = ViewPoint { x: 1.0, y: 2.0 };
        let active_start = ViewPoint { x: 10.0, y: 20.0 };
        let mut pending = Some(PendingRubberBand::new(PaneId(2), pending_start));
        let mut active = None;

        start_active_rubber_band_for_pane(&mut pending, &mut active, PaneId(1), active_start);

        assert_eq!(pending, None);
        assert_eq!(active, Some(RubberBandState::new(PaneId(1), active_start)));
    }
}
