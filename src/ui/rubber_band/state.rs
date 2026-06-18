use fika_core::{PaneId, ViewPoint, ViewRect, ViewState};

const RUBBER_BAND_START_DRAG_DISTANCE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RubberBandState {
    pub(crate) pane_id: PaneId,
    pub(crate) start: ViewPoint,
    pub(crate) current: ViewPoint,
}

impl RubberBandState {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RubberBandDrag {
    pub(crate) pane_id: PaneId,
}

pub(crate) fn rubber_band_drag_distance_reached(start: ViewPoint, current: ViewPoint) -> bool {
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
}
