use crate::FikaApp;
use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, Empty, ParentElement, Stateful, Styled, div, px, rgb};

use super::{fixed_status_text, status_section};

const ZOOM_TRACK_WIDTH: f32 = 142.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ZoomSliderDrag {
    zoom_min: i32,
    zoom_max: i32,
    track_width: f32,
}

pub(super) fn zoom_control(
    pane_id: PaneId,
    zoom_level: i32,
    zoom_icon_size: f32,
    zoom_min: i32,
    zoom_max: i32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    status_section()
        .id(format!("status-zoom-{}", pane_id.0))
        .gap_2()
        .text_xs()
        .child(
            div()
                .min_w_0()
                .flex_shrink_1()
                .truncate()
                .text_color(rgb(0x59636e))
                .child("Zoom:"),
        )
        .child(zoom_track(
            pane_id,
            zoom_level,
            zoom_min,
            zoom_max,
            ZOOM_TRACK_WIDTH,
            cx,
        ))
        .child(fixed_status_text(
            44.0,
            format!("{}px", zoom_icon_size as i32),
        ))
}

fn zoom_track(
    pane_id: PaneId,
    zoom_level: i32,
    zoom_min: i32,
    zoom_max: i32,
    track_width: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("status-zoom-track-{}", pane_id.0))
        .flex()
        .items_center()
        .gap_1()
        .h(px(18.0))
        .w(px(track_width))
        .min_w_0()
        .flex_shrink_1()
        .overflow_hidden()
        .on_drag(
            ZoomSliderDrag {
                zoom_min,
                zoom_max,
                track_width,
            },
            |_, _, _, cx| cx.new(|_| Empty),
        )
        .on_drag_move::<ZoomSliderDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ZoomSliderDrag>, _window, cx| {
                let drag = *event.drag(cx);
                let track_x = (event.event.position.x - event.bounds.origin.x).as_f32();
                this.set_zoom_level_with_context(
                    pane_id,
                    zoom_level_for_track_x(track_x, drag.track_width, drag.zoom_min, drag.zoom_max),
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            },
        ))
        .children((zoom_min..=zoom_max).map(|level| {
            zoom_segment(pane_id, level, level <= zoom_level, level == zoom_level, cx)
        }))
}

pub(crate) fn zoom_level_for_track_x(
    track_x: f32,
    track_width: f32,
    zoom_min: i32,
    zoom_max: i32,
) -> i32 {
    let span = (zoom_max - zoom_min).max(0);
    if span == 0 || track_width <= 0.0 {
        return zoom_min;
    }
    let position = (track_x / track_width).clamp(0.0, 1.0);
    zoom_min + (position * span as f32).round() as i32
}

fn zoom_segment(
    pane_id: PaneId,
    level: i32,
    filled: bool,
    current: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("status-zoom-level-{}-{level}", pane_id.0))
        .w(px(if current { 8.0 } else { 6.0 }))
        .h(px(if current { 16.0 } else { 10.0 }))
        .rounded_md()
        .bg(if filled { rgb(0x2f6fed) } else { rgb(0xd5d9df) })
        .hover(|segment| segment.bg(rgb(0x1f5fd0)))
        .cursor_pointer()
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() {
                    this.set_zoom_level_with_context(pane_id, level, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
}
