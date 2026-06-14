mod row;
mod section;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{
    Context, Div, MouseButton, NavigationDirection, ParentElement, Stateful, Styled, div, px, rgb,
};

use super::snapshot::PlaceSnapshot;
use row::place_row;
use section::group_heading;

pub(crate) fn places_sidebar(
    places: Vec<PlaceSnapshot>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let mut rows = Vec::new();
    let mut current_group = None;

    for (index, place) in places.into_iter().enumerate() {
        let starts_group = current_group != Some(place.group);
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                rows.push(group_heading(
                    place.group,
                    place.index,
                    place.insert_before,
                    cx,
                ));
            }
        }
        rows.push(place_row(index, place, !starts_group, cx));
    }

    div()
        .id("places-sidebar")
        .flex()
        .flex_col()
        .w(px(220.0))
        .min_w(px(200.0))
        .min_h_0()
        .mt(px(8.0))
        .mb(px(8.0))
        .ml_2()
        .border_1()
        .rounded_xl()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xf8f9fb))
        .overflow_hidden()
        .px_2()
        .py_2()
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(pane_id) = this.panes.focused() {
                    this.go_back(pane_id);
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(pane_id) = this.panes.focused() {
                    this.go_forward(pane_id);
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_places_blank_context_menu(event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(
            div()
                .px_2()
                .pb_2()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_sm()
                .text_color(rgb(0x24292f))
                .child("Places"),
        )
        .children(rows)
}
