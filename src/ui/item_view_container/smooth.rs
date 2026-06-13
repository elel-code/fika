use std::time::Instant;

use crate::FikaApp;
use fika_core::{
    PaneId, SMOOTH_SCROLL_FRAME, ScrollBounds, SmoothScroll, ViewPoint, normalize_viewport_extent,
};
use gpui::Context;

use super::scroll_offset::ItemViewScrollOffsetBar;

pub(crate) type ItemViewSmoothScroll = SmoothScroll;

impl FikaApp {
    pub(crate) fn smooth_scroll_item_view_container_by_delta_at(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        delta: f32,
        now: Instant,
        cx: &mut Context<FikaApp>,
    ) -> bool {
        let changed =
            self.start_item_view_container_smooth_scroll_by_delta_at(pane_id, bar, delta, now);
        if changed {
            self.ensure_item_view_container_smooth_tick(cx);
        }
        changed
    }

    pub(crate) fn start_item_view_container_smooth_scroll_by_delta_at(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        delta: f32,
        now: Instant,
    ) -> bool {
        let Some(current_x) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.view.scroll_x.clamp(0.0, bar.maximum))
        else {
            return false;
        };
        let live_bar = bar.with_value(current_x);
        let target_x = live_bar.value_after_delta(delta);
        if (target_x - current_x).abs() <= f32::EPSILON {
            return false;
        }

        let current = ViewPoint {
            x: current_x,
            y: 0.0,
        };
        let bounds = ScrollBounds::new(live_bar.maximum, 0.0);
        let distance = ViewPoint {
            x: current_x - target_x,
            y: 0.0,
        };
        let scroll = self
            .item_view_container_smooth_scrolls
            .get(&pane_id)
            .copied()
            .filter(|scroll| scroll.maximum_matches(bounds))
            .map_or_else(
                || SmoothScroll::from_scroll_contents_by(current, distance, bounds, now),
                |scroll| scroll.scroll_contents_by(current, distance, bounds, now),
            );
        self.item_view_container_smooth_scrolls
            .insert(pane_id, scroll);
        true
    }

    pub(crate) fn item_view_container_smooth_target_x(&self, pane_id: PaneId) -> Option<f32> {
        self.item_view_container_smooth_scrolls
            .get(&pane_id)
            .and_then(|scroll| scroll.target_offset())
            .map(|target| target.x)
    }

    pub(crate) fn start_item_view_container_kinetic_scroll_at(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        velocity_x: f32,
        now: Instant,
        cx: &mut Context<FikaApp>,
    ) -> bool {
        let started = self
            .start_item_view_container_kinetic_scroll_by_velocity_at(pane_id, bar, velocity_x, now);
        if started {
            self.ensure_item_view_container_smooth_tick(cx);
        }
        started
    }

    pub(crate) fn start_item_view_container_kinetic_scroll_by_velocity_at(
        &mut self,
        pane_id: PaneId,
        bar: ItemViewScrollOffsetBar,
        velocity_x: f32,
        now: Instant,
    ) -> bool {
        let bounds = ScrollBounds::new(bar.maximum, 0.0);
        let Some(scroll) = SmoothScroll::kinetic(
            ViewPoint {
                x: velocity_x,
                y: 0.0,
            },
            bounds,
            now,
        ) else {
            return false;
        };
        self.item_view_container_smooth_scrolls
            .insert(pane_id, scroll);
        true
    }

    pub(crate) fn advance_item_view_container_smooth_scrolls_at(&mut self, now: Instant) -> bool {
        let pane_ids = self
            .item_view_container_smooth_scrolls
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for pane_id in pane_ids {
            let Some(mut scroll) = self.item_view_container_smooth_scrolls.remove(&pane_id) else {
                continue;
            };
            let Some(current) = self.panes.pane(pane_id).map(|pane| ViewPoint {
                x: pane.view.scroll_x,
                y: pane.view.scroll_y,
            }) else {
                continue;
            };
            let bounds = scroll.bounds();
            let advanced = scroll.advance(current, now);
            let offset = ViewPoint {
                x: normalize_viewport_extent(advanced.offset.x),
                y: normalize_viewport_extent(advanced.offset.y),
            };
            let _ =
                self.panes
                    .set_view_scroll(pane_id, offset.x, offset.y, bounds.max_x, bounds.max_y);
            if advanced.active {
                self.item_view_container_smooth_scrolls
                    .insert(pane_id, scroll);
            }
        }
        !self.item_view_container_smooth_scrolls.is_empty()
    }

    pub(crate) fn cancel_item_view_container_smooth_scroll_for_pane(&mut self, pane_id: PaneId) {
        self.item_view_container_smooth_scrolls.remove(&pane_id);
    }

    fn ensure_item_view_container_smooth_tick(&mut self, cx: &mut Context<FikaApp>) {
        if self.item_view_container_smooth_tick_running {
            return;
        }
        self.item_view_container_smooth_tick_running = true;
        cx.spawn(
            move |this: gpui::WeakEntity<FikaApp>, cx: &mut gpui::AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        cx.background_executor().timer(SMOOTH_SCROLL_FRAME).await;
                        let Ok(active) = this.update(&mut cx, |app, cx| {
                            let active =
                                app.advance_item_view_container_smooth_scrolls_at(Instant::now());
                            cx.notify();
                            if !active {
                                app.item_view_container_smooth_tick_running = false;
                            }
                            active
                        }) else {
                            break;
                        };
                        if !active {
                            break;
                        }
                    }
                }
            },
        )
        .detach();
    }
}
