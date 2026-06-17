mod dnd;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

use dnd::{PlaceRowDndConfig, install_place_row_dnd};

use super::super::drag::{PlaceDrag, PlaceDragPreview};
use super::super::icon_view::place_icon_view;
use super::super::snapshot::PlaceSnapshot;
use super::super::style::{
    PlaceInsertIndicatorEdge, place_insert_indicator, place_row_background, place_row_border_color,
    place_row_hover_background,
};
use super::super::visual::PLACE_ROW_HEIGHT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaceRowHighlight {
    active: bool,
    drop_target: bool,
    hover_enabled: bool,
}

fn place_row_highlight(active: bool, drop_target: bool, insert_target: bool) -> PlaceRowHighlight {
    PlaceRowHighlight {
        active: active && !insert_target,
        drop_target: drop_target && !insert_target,
        hover_enabled: !insert_target,
    }
}

fn place_row_drag_is_movable(place: &PlaceSnapshot) -> bool {
    place.group.is_empty()
}

pub(super) fn place_row(
    visible_index: usize,
    place: PlaceSnapshot,
    custom_visual: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let row_id = format!("place-{visible_index}");
    let path = place.path.clone();
    let place_drag = PlaceDrag::new(
        place.path.clone(),
        place.label.as_str(),
        place.icon.clone(),
        place.index,
        place_row_drag_is_movable(&place),
    );
    let context_place = place.clone();
    let insert_before_index = place.index;
    let insert_after_index = place.index + 1;
    let insert_target = place.insert_before || place.insert_after;
    let highlight = place_row_highlight(place.active, place.drop_target, insert_target);
    let row_drop_target = highlight.drop_target;
    let active = highlight.active;
    let mounted = place.mounted;
    let device = place.device;
    let network = place.network;
    let device_id = place.device_id.clone();
    let label = place.label.clone();

    let row = div()
        .id(row_id)
        .relative()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(if custom_visual {
            rgba(0x00000000)
        } else {
            place_row_border_color(active, row_drop_target)
        })
        .bg(if custom_visual {
            rgba(0x00000000)
        } else {
            place_row_background(active, row_drop_target)
        })
        .when(highlight.hover_enabled, |row| {
            row.hover(move |row| {
                row.bg(if custom_visual {
                    rgba(0x00000000)
                } else {
                    place_row_hover_background(active, row_drop_target)
                })
            })
        })
        .when(mounted || device || network, |row| row.cursor_pointer())
        .on_drag(place_drag, |drag, cursor_offset, _, cx| {
            cx.new(|_| PlaceDragPreview::from_drag(drag, cursor_offset))
        })
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.activate_place(
                path.clone(),
                device_id.clone(),
                label.clone(),
                mounted,
                device,
                network,
                cx,
            );
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
    let mut row = install_place_row_dnd(
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

    if custom_visual {
        row = row
            .h(px(PLACE_ROW_HEIGHT))
            .child(place_icon_view(&place.icon, active))
            .child(div().flex_1());
    } else {
        row = row
            .child(place_icon_view(&place.icon, active))
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
                    .child(place.label.clone()),
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
            });
    }

    div()
        .id(format!("place-wrap-{visible_index}"))
        .relative()
        .flex()
        .flex_col()
        .child(row)
        .when(place.insert_before && !custom_visual, |row| {
            row.child(place_insert_indicator(
                format!("place-insert-before-{visible_index}"),
                PlaceInsertIndicatorEdge::Before,
            ))
        })
        .when(place.insert_after && !custom_visual, |row| {
            row.child(place_insert_indicator(
                format!("place-insert-after-{visible_index}"),
                PlaceInsertIndicatorEdge::After,
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icons::FileIconSnapshot;

    #[test]
    fn place_row_insert_target_suppresses_ordinary_row_highlight() {
        assert_eq!(
            place_row_highlight(true, true, true),
            PlaceRowHighlight {
                active: false,
                drop_target: false,
                hover_enabled: false,
            }
        );
        assert_eq!(
            place_row_highlight(true, true, false),
            PlaceRowHighlight {
                active: true,
                drop_target: true,
                hover_enabled: true,
            }
        );
    }

    #[test]
    fn place_row_drag_reorder_allows_primary_place_sources() {
        let mut place = PlaceSnapshot {
            index: 0,
            group: "",
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: "Work".to_string(),
            path: "/tmp/work".into(),
            device_id: None,
            mounted: true,
            device: false,
            network: false,
            device_ejectable: false,
            device_can_power_off: false,
            active: false,
            drop_target: false,
            insert_before: false,
            insert_after: false,
            trash_place: false,
            trash_has_items: false,
            editable: true,
            removable: true,
        };

        assert!(place_row_drag_is_movable(&place));

        place.active = true;
        assert!(place_row_drag_is_movable(&place));

        place.active = false;
        place.editable = false;
        place.removable = false;
        assert!(place_row_drag_is_movable(&place));

        place.group = "Network";
        assert!(!place_row_drag_is_movable(&place));
    }
}
