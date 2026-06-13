use crate::FikaApp;
use fika_core::{PaneId, ViewPoint, ViewRect};
use gpui::{Context, Pixels, Point};

#[cfg(test)]
use super::geometry::scrollbar_drag_track_rect;
use super::geometry::{
    HorizontalScrollBarTrack, horizontal_scrollbar_track, scroll_x_for_scrollbar_drag,
    scrollbar_drag_start_from_local, scrollbar_local_track_rect, scrollbar_track_local_point,
    scrollbar_track_x_from_window,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ActiveScrollBarDrag {
    pub(crate) pane_id: PaneId,
    pub(crate) content_width: f32,
    pub(crate) track_window_rect: ViewRect,
    pub(crate) handle_grab_x: f32,
}

impl FikaApp {
    #[cfg(test)]
    pub(crate) fn set_horizontal_scrollbar_track(
        &mut self,
        pane_id: PaneId,
        track_window_rect: ViewRect,
        content_width: f32,
        scroll_x: f32,
    ) -> bool {
        let Some(track) = horizontal_scrollbar_track(track_window_rect, content_width, scroll_x)
        else {
            return self.horizontal_scrollbar_tracks.remove(&pane_id).is_some();
        };
        if self.horizontal_scrollbar_tracks.get(&pane_id) == Some(&track) {
            return false;
        }
        self.horizontal_scrollbar_tracks.insert(pane_id, track);
        true
    }

    pub(crate) fn refresh_horizontal_scrollbar_track_from_layout(
        &mut self,
        pane_id: PaneId,
        track_window_rect: ViewRect,
    ) -> Option<HorizontalScrollBarTrack> {
        let projection = self.layout_projection_for_pane(pane_id)?;
        let content_width = projection.layout.content_size().width;
        let scroll_x = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or_default();
        let Some(track) = horizontal_scrollbar_track(track_window_rect, content_width, scroll_x)
        else {
            self.horizontal_scrollbar_tracks.remove(&pane_id);
            return None;
        };
        if self.horizontal_scrollbar_tracks.get(&pane_id) != Some(&track) {
            self.horizontal_scrollbar_tracks.insert(pane_id, track);
        }
        Some(track)
    }

    pub(crate) fn begin_horizontal_scrollbar_drag_from_cached_track(
        &mut self,
        pane_id: PaneId,
        position: Point<Pixels>,
    ) -> bool {
        let Some(track) = self.horizontal_scrollbar_tracks.get(&pane_id).copied() else {
            return false;
        };
        let Some(local) = scrollbar_track_local_point(track.window_rect, position) else {
            return false;
        };
        self.begin_horizontal_scrollbar_drag_from_track_point(
            pane_id,
            track.content_width,
            track.scroll_x,
            local,
            track.window_rect,
        )
    }

    #[cfg(test)]
    pub(crate) fn begin_horizontal_scrollbar_drag_from_window_track(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        track_window_rect: ViewRect,
        position: Point<Pixels>,
    ) -> bool {
        let Some(track) = horizontal_scrollbar_track(track_window_rect, content_width, scroll_x)
        else {
            return false;
        };
        let Some(local) = scrollbar_track_local_point(track.window_rect, position) else {
            return false;
        };
        self.horizontal_scrollbar_tracks.insert(pane_id, track);
        self.begin_horizontal_scrollbar_drag_from_track_point(
            pane_id,
            track.content_width,
            track.scroll_x,
            local,
            track.window_rect,
        )
    }

    pub(crate) fn update_horizontal_scrollbar_drag_from_window(
        &mut self,
        pane_id: PaneId,
        position: Point<Pixels>,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        self.update_horizontal_scrollbar_drag(
            pane_id,
            scrollbar_track_x_from_window(drag.track_window_rect, position),
            drag.track_window_rect.width,
        )
    }

    pub(crate) fn horizontal_scrollbar_drag_is_active_for(&self, pane_id: PaneId) -> bool {
        self.active_scrollbar_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
    }

    #[cfg(test)]
    pub(crate) fn begin_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        start_track_x: f32,
        track_width: f32,
    ) -> bool {
        let track_rect = scrollbar_drag_track_rect(track_width);
        self.begin_horizontal_scrollbar_drag_from_track_point(
            pane_id,
            content_width,
            scroll_x,
            ViewPoint {
                x: start_track_x,
                y: super::geometry::SCROLLBAR_THICKNESS / 2.0,
            },
            track_rect,
        )
    }

    fn begin_horizontal_scrollbar_drag_from_track_point(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        local: ViewPoint,
        track_window_rect: ViewRect,
    ) -> bool {
        if self.active_scrollbar_drag.is_some() {
            return false;
        }
        let local_track_rect = scrollbar_local_track_rect(track_window_rect);
        let Some((initial_scroll_x, handle_grab_x, max_scroll_x)) =
            scrollbar_drag_start_from_local(content_width, scroll_x, local, local_track_rect)
        else {
            return false;
        };
        self.finish_rubber_band(pane_id);
        self.active_scrollbar_drag = Some(ActiveScrollBarDrag {
            pane_id,
            content_width,
            track_window_rect,
            handle_grab_x,
        });
        self.set_pane_scroll_immediate(pane_id, initial_scroll_x, 0.0, max_scroll_x, 0.0);
        true
    }

    pub(crate) fn update_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        track_x: f32,
        track_width: f32,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        let Some((scroll_x, max_scroll_x)) = scroll_x_for_scrollbar_drag(
            drag.content_width,
            drag.handle_grab_x,
            track_x,
            track_width,
        ) else {
            return false;
        };
        let previous_scroll_x = self
            .panes
            .pane(drag.pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or_default();
        self.set_pane_scroll_immediate(drag.pane_id, scroll_x, 0.0, max_scroll_x, 0.0);
        self.panes
            .pane(drag.pane_id)
            .is_some_and(|pane| (pane.view.scroll_x - previous_scroll_x).abs() > f32::EPSILON)
    }

    pub(crate) fn finish_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        self.active_scrollbar_drag = None;
        self.finish_scrollbar_drag_for_content_width(drag.pane_id, drag.content_width, cx);
        true
    }

    pub(crate) fn clear_horizontal_scrollbar_drag_for_pane(&mut self, pane_id: PaneId) {
        if self
            .active_scrollbar_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.active_scrollbar_drag = None;
        }
        self.horizontal_scrollbar_tracks.remove(&pane_id);
    }
}
