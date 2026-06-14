mod dnd;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb};

use dnd::{PlaceRowDndConfig, install_place_row_dnd};

use super::super::drag::{PlaceDrag, PlaceDragPreview};
use super::super::icon_view::place_icon_view;
use super::super::snapshot::PlaceSnapshot;
use super::super::style::{
    PlaceInsertIndicatorEdge, place_insert_indicator, place_row_background, place_row_border_color,
    place_row_hover_background,
};

pub(super) fn place_row(
    visible_index: usize,
    place: PlaceSnapshot,
    show_insert_before: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let row_id = format!("place-{visible_index}");
    let path = place.path.clone();
    let place_drag = PlaceDrag::new(
        place.path.clone(),
        place.label.as_str(),
        place.icon.clone(),
        place.index,
        place.editable && place.removable,
    );
    let context_place = place.clone();
    let insert_before_index = place.index;
    let insert_after_index = place.index + 1;
    let row_drop_target = place.drop_target;
    let active = place.active;
    let mounted = place.mounted;
    let device = place.device;
    let network = place.network;

    let row = div()
        .id(row_id)
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(place_row_border_color(active, row_drop_target))
        .bg(place_row_background(active, row_drop_target))
        .hover(move |row| row.bg(place_row_hover_background(active, row_drop_target)))
        .when(mounted || device || network, |row| row.cursor_pointer())
        .on_drag(place_drag, |drag, _, _, cx| {
            cx.new(|_| PlaceDragPreview::from_drag(drag))
        })
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.activate_place(path.clone(), mounted, device, network, cx);
            cx.stop_propagation();
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_place_context_menu(context_place.clone(), event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        );
    let row = install_place_row_dnd(
        row,
        PlaceRowDndConfig {
            mounted,
            insert_before_index,
            insert_after_index,
            path_for_internal_target: place.path.clone(),
            path_for_internal_drop: place.path.clone(),
            path_for_external_target: place.path.clone(),
            path_for_external_drop: place.path.clone(),
        },
        cx,
    );

    div()
        .id(format!("place-wrap-{visible_index}"))
        .relative()
        .flex()
        .flex_col()
        .when(show_insert_before && place.insert_before, |row| {
            row.child(place_insert_indicator(
                format!("place-insert-before-{visible_index}"),
                PlaceInsertIndicatorEdge::Before,
            ))
        })
        .child(
            row.child(place_icon_view(&place.icon, active))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .text_color(if place.active {
                            rgb(0x1f4fbf)
                        } else if !place.mounted {
                            rgb(0x6b7280)
                        } else {
                            rgb(0x24292f)
                        })
                        .child(place.label),
                )
                .when(place.trash_place, |row| {
                    row.child(
                        div()
                            .id(format!("place-trash-state-{visible_index}"))
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded_full()
                            .bg(if place.trash_has_items {
                                rgb(0x2f6fed)
                            } else {
                                rgb(0xc8ced6)
                            }),
                    )
                }),
        )
        .when(place.insert_after, |row| {
            row.child(place_insert_indicator(
                format!("place-insert-after-{visible_index}"),
                PlaceInsertIndicatorEdge::After,
            ))
        })
}
