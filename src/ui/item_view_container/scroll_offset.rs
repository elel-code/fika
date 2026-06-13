use std::time::Instant;

use crate::FikaApp;
use fika_core::{CompactLayout, PaneId, ViewPoint, ViewRect, ViewState, normalize_viewport_extent};
use gpui::{Context, Pixels, ScrollDelta, Window};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollOffsetBar {
    pub(crate) value: f32,
    pub(crate) maximum: f32,
    pub(crate) page_step: f32,
    pub(crate) single_step: f32,
    pub(crate) content_extent: f32,
    pub(crate) view_extent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollTrack {
    pub(crate) track_rect: ViewRect,
    pub(crate) thumb_rect: ViewRect,
    bar: ItemViewScrollOffsetBar,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewScrollBarDrag {
    pub(crate) pane_id: PaneId,
    pub(crate) bar: ItemViewScrollOffsetBar,
    pub(crate) track_window_rect: ViewRect,
    pub(crate) thumb_grab_x: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemViewScrollPress {
    None,
    Page,
    Thumb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemViewScrollBarEvent {
    pub(crate) handled: bool,
    pub(crate) changed: bool,
    pub(crate) dragging: bool,
}

impl ItemViewScrollBarEvent {
    pub(crate) const IGNORED: Self = Self {
        handled: false,
        changed: false,
        dragging: false,
    };

    fn handled(changed: bool, dragging: bool) -> Self {
        Self {
            handled: true,
            changed,
            dragging,
        }
    }
}

impl ItemViewScrollOffsetBar {
    pub(crate) fn from_layout(layout: &CompactLayout, view: &ViewState) -> Option<Self> {
        Self::from_extents(
            layout.content_size().width,
            view.viewport_width,
            view.scroll_x,
            1.0,
        )
    }

    pub(crate) fn from_extents(
        content_extent: f32,
        view_extent: f32,
        value: f32,
        single_step: f32,
    ) -> Option<Self> {
        let content_extent = content_extent.max(0.0);
        let view_extent = normalize_viewport_extent(view_extent).max(0.0);
        let page_step = view_extent.floor().max(0.0);
        let maximum = (content_extent - view_extent).floor().max(0.0);
        if content_extent <= 0.0 || page_step <= 0.0 || maximum <= 0.0 {
            return None;
        }
        Some(Self {
            value: value.floor().clamp(0.0, maximum),
            maximum,
            page_step,
            single_step: single_step.floor().max(1.0),
            content_extent,
            view_extent,
        })
    }

    pub(crate) fn remeasured(self, view_extent: f32) -> Option<Self> {
        Self::from_extents(
            self.content_extent,
            view_extent,
            self.value,
            self.single_step,
        )
    }

    pub(crate) fn with_single_step(self, single_step: f32) -> Self {
        Self {
            single_step: single_step.floor().max(1.0),
            ..self
        }
    }

    pub(crate) fn with_value(self, value: f32) -> Self {
        Self {
            value: value.floor().clamp(0.0, self.maximum),
            ..self
        }
    }

    pub(crate) fn track(self, track_extent: f32, thickness: f32) -> Option<ItemViewScrollTrack> {
        let track_extent = normalize_viewport_extent(track_extent).floor().max(0.0);
        let thickness = normalize_viewport_extent(thickness).floor().max(0.0);
        if track_extent <= 0.0 || thickness <= 0.0 || self.maximum <= 0.0 {
            return None;
        }

        let min_thumb_extent = (thickness * 2.0).min(track_extent);
        let thumb_extent = (self.page_step / (self.maximum + self.page_step) * track_extent)
            .clamp(min_thumb_extent, track_extent)
            .floor()
            .max(1.0);
        let travel = (track_extent - thumb_extent).max(0.0);
        let thumb_x = if travel <= 0.0 {
            0.0
        } else {
            (self.value / self.maximum * travel).round()
        };

        Some(ItemViewScrollTrack {
            track_rect: ViewRect {
                x: 0.0,
                y: 0.0,
                width: track_extent,
                height: thickness,
            },
            thumb_rect: ViewRect {
                x: thumb_x.clamp(0.0, travel),
                y: 0.0,
                width: thumb_extent,
                height: thickness,
            },
            bar: self,
        })
    }

    pub(crate) fn wheel_delta(self, delta: ScrollDelta) -> f32 {
        match delta {
            ScrollDelta::Pixels(delta) => -(delta.x.as_f32() + delta.y.as_f32()),
            ScrollDelta::Lines(delta) => -(delta.x + delta.y) * self.single_step,
        }
    }

    pub(crate) fn value_after_delta(self, delta: f32) -> f32 {
        (self.value + delta).round().clamp(0.0, self.maximum)
    }

    pub(crate) fn value_after_page_press(self, track: ItemViewScrollTrack, x: f32) -> f32 {
        if x < track.thumb_rect.x {
            (self.value - self.page_step).clamp(0.0, self.maximum)
        } else if x >= track.thumb_rect.right() {
            (self.value + self.page_step).clamp(0.0, self.maximum)
        } else {
            self.value
        }
    }
}

impl ItemViewScrollTrack {
    fn press_kind(self, x: f32, y: f32) -> ItemViewScrollPress {
        let point = ViewPoint { x, y };
        if !self.track_rect.contains(point) {
            return ItemViewScrollPress::None;
        }
        if self.thumb_rect.contains(point) {
            ItemViewScrollPress::Thumb
        } else {
            ItemViewScrollPress::Page
        }
    }

    fn thumb_grab_x(self, x: f32) -> f32 {
        (x - self.thumb_rect.x).clamp(0.0, self.thumb_rect.width)
    }

    fn value_for_thumb_x(self, thumb_x: f32) -> f32 {
        let travel = (self.track_rect.width - self.thumb_rect.width).max(0.0);
        if travel <= 0.0 || self.bar.maximum <= 0.0 {
            return 0.0;
        }
        (thumb_x.clamp(0.0, travel) / travel * self.bar.maximum)
            .round()
            .clamp(0.0, self.bar.maximum)
    }
}

pub(crate) fn item_view_scroll_single_step(line_height: Pixels) -> f32 {
    line_height.as_f32().floor().max(1.0) * 2.0
}

pub(crate) fn handle_item_view_container_wheel(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::ScrollWheelEvent,
    window: &mut Window,
    layout: &CompactLayout,
    view: &ViewState,
    cx: &mut Context<FikaApp>,
) {
    if wheel_modifiers_request_zoom(event.modifiers) {
        app.finish_rubber_band(pane_id);
        app.zoom_pane_from_wheel(pane_id, event.delta);
        cx.stop_propagation();
        cx.notify();
        return;
    }

    app.finish_rubber_band(pane_id);
    let now = Instant::now();
    if let Some(bar) = ItemViewScrollOffsetBar::from_layout(layout, view)
        .map(|bar| bar.with_single_step(item_view_scroll_single_step(window.line_height())))
    {
        if matches!(event.touch_phase, gpui::TouchPhase::Started) {
            app.begin_item_view_container_wheel_gesture_at(pane_id, bar, now);
        }
        let changed = app.smooth_scroll_item_view_container_by_delta_at(
            pane_id,
            bar,
            bar.wheel_delta(event.delta),
            now,
            cx,
        );
        if changed {
            if let Some(target_x) = app.item_view_container_smooth_target_x(pane_id) {
                app.sample_item_view_container_wheel_gesture_target_at(pane_id, bar, target_x, now);
            }
            cx.notify();
        }
        if matches!(event.touch_phase, gpui::TouchPhase::Ended)
            && let Some(velocity_x) = app.finish_item_view_container_wheel_gesture_at(pane_id)
            && app.start_item_view_container_kinetic_scroll_at(pane_id, bar, velocity_x, now, cx)
        {
            cx.notify();
        }
    } else {
        app.cancel_item_view_container_wheel_gesture_for_pane(pane_id);
    }
    cx.stop_propagation();
}

fn wheel_modifiers_request_zoom(modifiers: gpui::Modifiers) -> bool {
    modifiers.control || modifiers.secondary()
}

impl FikaApp {
    pub(crate) fn set_item_view_container_scroll_offset(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        value: f32,
    ) -> bool {
        self.cancel_item_view_container_wheel_gesture_for_pane(pane_id);
        self.cancel_item_view_container_smooth_scroll_for_pane(pane_id);
        self.write_item_view_container_scroll_offset(pane_id, bar, value)
    }

    pub(crate) fn write_item_view_container_scroll_offset(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        value: f32,
    ) -> bool {
        let previous = self
            .panes
            .pane(pane_id)
            .map(|pane| (pane.view.scroll_x, pane.view.scroll_y));
        let Some(view) = self
            .panes
            .set_view_scroll(pane_id, value, 0.0, bar.maximum, 0.0)
        else {
            return false;
        };
        previous.is_some_and(|(x, y)| {
            (view.scroll_x - x).abs() > f32::EPSILON || (view.scroll_y - y).abs() > f32::EPSILON
        })
    }

    #[cfg(test)]
    pub(crate) fn scroll_item_view_container_by_delta(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        delta: f32,
    ) -> bool {
        let current = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or(bar.value);
        let live_bar = bar.with_value(current);
        self.set_item_view_container_scroll_offset(
            pane_id,
            live_bar,
            live_bar.value_after_delta(delta),
        )
    }

    pub(crate) fn begin_item_view_scrollbar_press(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        track_window_rect: ViewRect,
        position: gpui::Point<gpui::Pixels>,
    ) -> ItemViewScrollBarEvent {
        let Some(track) = bar.track(track_window_rect.width, track_window_rect.height) else {
            return ItemViewScrollBarEvent::IGNORED;
        };
        let local_x = position.x.as_f32() - track_window_rect.x;
        let local_y = position.y.as_f32() - track_window_rect.y;
        match track.press_kind(local_x, local_y) {
            ItemViewScrollPress::None => ItemViewScrollBarEvent::IGNORED,
            ItemViewScrollPress::Page => {
                self.item_view_container_scroll_drag = None;
                let changed = self.set_item_view_container_scroll_offset(
                    pane_id,
                    bar,
                    bar.value_after_page_press(track, local_x),
                );
                ItemViewScrollBarEvent::handled(changed, false)
            }
            ItemViewScrollPress::Thumb => {
                self.finish_rubber_band(pane_id);
                self.cancel_item_view_container_wheel_gesture_for_pane(pane_id);
                self.cancel_item_view_container_smooth_scroll_for_pane(pane_id);
                self.item_view_container_scroll_drag = Some(ItemViewScrollBarDrag {
                    pane_id,
                    bar,
                    track_window_rect,
                    thumb_grab_x: track.thumb_grab_x(local_x),
                });
                ItemViewScrollBarEvent::handled(false, true)
            }
        }
    }

    pub(crate) fn update_item_view_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        position: gpui::Point<gpui::Pixels>,
    ) -> ItemViewScrollBarEvent {
        let Some(drag) = self.item_view_container_scroll_drag else {
            return ItemViewScrollBarEvent::IGNORED;
        };
        if drag.pane_id != pane_id {
            return ItemViewScrollBarEvent::IGNORED;
        }
        let Some(track) = drag
            .bar
            .track(drag.track_window_rect.width, drag.track_window_rect.height)
        else {
            self.item_view_container_scroll_drag = None;
            return ItemViewScrollBarEvent::IGNORED;
        };
        let local_x = position.x.as_f32() - drag.track_window_rect.x;
        let changed = self.write_item_view_container_scroll_offset(
            pane_id,
            drag.bar,
            track.value_for_thumb_x(local_x - drag.thumb_grab_x),
        );
        ItemViewScrollBarEvent::handled(changed, true)
    }

    pub(crate) fn finish_item_view_scrollbar_drag(&mut self, pane_id: PaneId) -> bool {
        if self
            .item_view_container_scroll_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.item_view_container_scroll_drag = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel_item_view_container_scroll_for_pane(&mut self, pane_id: PaneId) {
        self.cancel_item_view_container_wheel_gesture_for_pane(pane_id);
        self.cancel_item_view_container_smooth_scroll_for_pane(pane_id);
        if self
            .item_view_container_scroll_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.item_view_container_scroll_drag = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_bar_matches_dolphin_horizontal_values() {
        let bar = ItemViewScrollOffsetBar::from_extents(1000.8, 240.4, 180.9, 28.7).unwrap();

        assert_eq!(bar.maximum, 760.0);
        assert_eq!(bar.page_step, 240.0);
        assert_eq!(bar.value, 180.0);
        assert_eq!(bar.single_step, 28.0);
    }

    #[test]
    fn thumb_extent_uses_page_step_over_scrollbar_range() {
        let bar = ItemViewScrollOffsetBar::from_extents(1000.0, 250.0, 0.0, 20.0).unwrap();
        let track = bar.track(200.0, 12.0).unwrap();

        assert_eq!(track.thumb_rect.width, 50.0);
        assert_eq!(track.track_rect.width, 200.0);
    }

    #[test]
    fn track_press_pages_by_page_step() {
        let bar = ItemViewScrollOffsetBar::from_extents(1000.0, 200.0, 400.0, 20.0).unwrap();
        let track = bar.track(200.0, 12.0).unwrap();

        assert_eq!(bar.value_after_page_press(track, 0.0), 200.0);
        assert_eq!(bar.value_after_page_press(track, 199.0), 600.0);
    }

    #[test]
    fn drag_maps_thumb_position_to_scroll_value() {
        let bar = ItemViewScrollOffsetBar::from_extents(1000.0, 200.0, 0.0, 20.0).unwrap();
        let track = bar.track(200.0, 12.0).unwrap();
        let travel = track.track_rect.width - track.thumb_rect.width;

        assert_eq!(track.value_for_thumb_x(0.0), 0.0);
        assert_eq!(track.value_for_thumb_x(travel), bar.maximum);
        assert!((track.value_for_thumb_x(travel / 2.0) - bar.maximum / 2.0).abs() <= 1.0);
    }

    #[test]
    fn wheel_lines_use_dolphin_single_step() {
        let bar = ItemViewScrollOffsetBar::from_extents(1000.0, 200.0, 0.0, 30.0).unwrap();

        assert_eq!(
            bar.wheel_delta(ScrollDelta::Lines(gpui::point(0.0, -3.0))),
            90.0
        );
    }
}
