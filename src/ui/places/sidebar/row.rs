mod dnd;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba};

use dnd::{PlaceRowDndConfig, install_place_row_dnd};

use super::super::drag::{PlaceDragStartSource, install_place_drag_start_shell};
use super::super::icon_view::place_icon_view;
use super::super::perf::PlacesRowVisualPolicy;
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

pub(super) fn place_row(
    visible_index: usize,
    place: &PlaceSnapshot,
    row_visual_policy: PlacesRowVisualPolicy,
    force_gpui_text: bool,
    force_gpui_icon: bool,
    row_shell_cursor_enabled: bool,
    row_shell_targeting_enabled: bool,
    row_shell_dnd_enabled: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let custom_chrome = row_visual_policy.custom_layer_enabled();
    let gpui_text = force_gpui_text || !row_visual_policy.paints_text();
    let gpui_icon = force_gpui_icon || !row_visual_policy.paints_icon();
    let row_id = format!("place-{visible_index}");
    let place_drag_source = PlaceDragStartSource::from_snapshot(place);
    let insert_before_index = place.index;
    let insert_after_index = place.index + 1;
    let insert_target = place.insert_before || place.insert_after;
    let highlight = place_row_highlight(place.active, place.drop_target, insert_target);
    let row_drop_target = highlight.drop_target;
    let active = highlight.active;
    let mounted = place.mounted;
    let device = place.device;
    let network = place.network;

    let row = div()
        .id(row_id)
        .relative()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(if custom_chrome {
            rgba(0x00000000)
        } else {
            place_row_border_color(active, row_drop_target)
        })
        .bg(if custom_chrome {
            rgba(0x00000000)
        } else {
            place_row_background(active, row_drop_target)
        })
        .when(highlight.hover_enabled, |row| {
            row.hover(move |row| {
                row.bg(if custom_chrome {
                    if row_drop_target {
                        rgba(0xf59e0b26)
                    } else if active {
                        rgba(0x2f6fed18)
                    } else {
                        rgba(0x64748b18)
                    }
                } else {
                    place_row_hover_background(active, row_drop_target)
                })
            })
        })
        .when(
            row_shell_cursor_enabled && (mounted || device || network),
            |row| row.cursor_pointer(),
        );
    let row = if custom_chrome {
        row.w_full().h(px(PLACE_ROW_HEIGHT))
    } else {
        row.px_2().py_1()
    };
    let mut row = install_place_drag_start_shell(row, place_drag_source);

    if row_shell_targeting_enabled {
        let path = place.path.clone();
        let device_id = place.device_id.clone();
        let label = place.label.clone();
        let context_place = place.clone();
        row = row
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
    }
    let mut row = if row_shell_dnd_enabled {
        let path = place.path.clone();
        install_place_row_dnd(
            row,
            PlaceRowDndConfig {
                mounted,
                insert_before_index,
                insert_after_index,
                path_for_internal_target: path.clone(),
                path_for_internal_drop: path.clone(),
                path_for_external_target: path.clone(),
                path_for_external_drop: path,
            },
            cx,
        )
    } else {
        row
    };

    if gpui_icon {
        row = row.child(place_icon_view(&place.icon, active));
    }

    if gpui_text {
        row = row.child(
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
        );
    }
    if !custom_chrome && place.trash_place {
        row = row.child(
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
        );
    }

    div()
        .id(format!("place-wrap-{visible_index}"))
        .relative()
        .flex()
        .flex_col()
        .child(row)
        .when(place.insert_before && !custom_chrome, |row| {
            row.child(place_insert_indicator(
                format!("place-insert-before-{visible_index}"),
                PlaceInsertIndicatorEdge::Before,
            ))
        })
        .when(place.insert_after && !custom_chrome, |row| {
            row.child(place_insert_indicator(
                format!("place-insert-after-{visible_index}"),
                PlaceInsertIndicatorEdge::After,
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
