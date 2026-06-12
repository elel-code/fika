use crate::{FikaApp, PlaceSnapshot, file_transfer_mode_for_modifiers};
use gpui::prelude::*;
use gpui::{
    Context, Div, ExternalPaths, MouseButton, ParentElement, Stateful, Styled, div, px, rgb, rgba,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::file_grid::ItemDrag;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceDrag {
    path: PathBuf,
    label: Arc<str>,
}

impl PlaceDrag {
    pub(crate) fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

struct PlaceDragPreview {
    label: Arc<str>,
    path: PathBuf,
}

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
        .h_full()
        .my_2()
        .ml_2()
        .border_1()
        .rounded_lg()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xf8f9fb))
        .px_2()
        .py_2()
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

fn group_heading(
    label: &'static str,
    insert_index: usize,
    insert_before: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("place-group-wrap-{label}"))
        .flex()
        .flex_col()
        .when(insert_before, |row| {
            row.child(place_insert_indicator(format!(
                "place-insert-before-group-{label}"
            )))
        })
        .child(
            div()
                .id(format!("place-group-{label}"))
                .px_2()
                .pt_2()
                .pb_1()
                .text_xs()
                .text_color(rgb(0x6b7280))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        this.show_place_section_context_menu(label, event.position);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_drag_move::<ItemDrag>(cx.listener(
                    move |this, _event: &gpui::DragMoveEvent<ItemDrag>, _window, cx| {
                        let changed = this.set_place_drag_drop_target_for_insert(insert_index);
                        this.schedule_drop_target_stale_clear(cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drag_move::<ExternalPaths>(cx.listener(
                    move |this, _event: &gpui::DragMoveEvent<ExternalPaths>, _window, cx| {
                        let changed = this.set_place_drag_drop_target_for_insert(insert_index);
                        this.schedule_drop_target_stale_clear(cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, _window, cx| {
                    this.drop_item_drag_to_place_insert(drag.payload(), insert_index);
                    cx.stop_propagation();
                    cx.notify();
                }))
                .on_drop::<ExternalPaths>(cx.listener(
                    move |this, external_paths: &ExternalPaths, _window, cx| {
                        this.drop_external_paths_to_place_insert(
                            external_paths.paths().to_vec(),
                            insert_index,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    },
                ))
                .child(label),
        )
}

fn place_row(
    visible_index: usize,
    place: PlaceSnapshot,
    show_insert_before: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let row_id = format!("place-{visible_index}");
    let path = place.path.clone();
    let path_for_internal_target = place.path.clone();
    let path_for_internal_drop = place.path.clone();
    let path_for_external_target = place.path.clone();
    let path_for_external_drop = place.path.clone();
    let place_drag = PlaceDrag {
        path: place.path.clone(),
        label: Arc::from(place.label.as_str()),
    };
    let context_place = place.clone();
    let insert_before_index = place.index;
    let insert_after_index = place.index + 1;
    let row_drop_target = place.drop_target;
    let active = place.active;
    let marker_color = if place.trash_place && place.trash_has_items {
        rgb(0x2f6fed)
    } else {
        rgb(0x59636e)
    };
    div()
        .id(format!("place-wrap-{visible_index}"))
        .flex()
        .flex_col()
        .when(show_insert_before && place.insert_before, |row| {
            row.child(place_insert_indicator(format!(
                "place-insert-before-{visible_index}"
            )))
        })
        .child(
            div()
                .id(row_id)
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(place_row_background(active, row_drop_target))
                .hover(move |row| row.bg(place_row_hover_background(active, row_drop_target)))
                .cursor_pointer()
                .on_drag(place_drag, |drag, _, _, cx| {
                    cx.new(|_| PlaceDragPreview {
                        label: drag.label.clone(),
                        path: drag.path.clone(),
                    })
                })
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.open_place(path.clone());
                    cx.notify();
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        this.show_place_context_menu(context_place.clone(), event.position);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_drag_move::<ItemDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ItemDrag>, _window, cx| {
                        let changed = match place_drop_zone(event) {
                            PlaceDropZone::InsertBefore => {
                                this.set_place_drag_drop_target_for_insert(insert_before_index)
                            }
                            PlaceDropZone::InsertAfter => {
                                this.set_place_drag_drop_target_for_insert(insert_after_index)
                            }
                            PlaceDropZone::OnPlace => this.set_place_drag_drop_target_for_path(
                                path_for_internal_target.clone(),
                            ),
                        };
                        this.schedule_drop_target_stale_clear(cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drag_move::<ExternalPaths>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ExternalPaths>, _window, cx| {
                        let changed = match place_drop_zone(event) {
                            PlaceDropZone::InsertBefore => {
                                this.set_place_drag_drop_target_for_insert(insert_before_index)
                            }
                            PlaceDropZone::InsertAfter => {
                                this.set_place_drag_drop_target_for_insert(insert_after_index)
                            }
                            PlaceDropZone::OnPlace => this.set_place_drag_drop_target_for_path(
                                path_for_external_target.clone(),
                            ),
                        };
                        this.schedule_drop_target_stale_clear(cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    this.drop_item_drag_to_current_place_target(
                        drag.payload(),
                        path_for_internal_drop.clone(),
                        mode,
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                }))
                .on_drop::<ExternalPaths>(cx.listener(
                    move |this, external_paths: &ExternalPaths, _window, cx| {
                        this.drop_external_paths_to_current_place_target(
                            external_paths.paths().to_vec(),
                            path_for_external_drop.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    },
                ))
                .child(
                    div()
                        .w(px(28.0))
                        .text_xs()
                        .text_color(marker_color)
                        .child(place.marker),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .text_color(if place.active {
                            rgb(0x1f4fbf)
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
            row.child(place_insert_indicator(format!(
                "place-insert-after-{visible_index}"
            )))
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceDropZone {
    InsertBefore,
    OnPlace,
    InsertAfter,
}

fn place_drop_zone<T>(event: &gpui::DragMoveEvent<T>) -> PlaceDropZone {
    let local_y = (event.event.position.y - event.bounds.origin.y).as_f32();
    place_drop_zone_for_y(local_y, event.bounds.size.height.as_f32())
}

fn place_drop_zone_for_y(local_y: f32, height: f32) -> PlaceDropZone {
    let edge = (height * 0.28).clamp(4.0, 10.0);
    if local_y <= edge {
        PlaceDropZone::InsertBefore
    } else if local_y >= height - edge {
        PlaceDropZone::InsertAfter
    } else {
        PlaceDropZone::OnPlace
    }
}

fn place_row_background(active: bool, drop_target: bool) -> gpui::Rgba {
    if drop_target {
        rgba(0x16a34a2e)
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xf8f9fb)
    }
}

fn place_row_hover_background(active: bool, drop_target: bool) -> gpui::Rgba {
    if drop_target {
        rgba(0x16a34a3d)
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xeef3f8)
    }
}

fn place_insert_indicator(id: String) -> impl IntoElement {
    div()
        .id(id)
        .mx_2()
        .h(px(2.0))
        .rounded_full()
        .bg(rgb(0x2f6fed))
}

impl gpui::Render for PlaceDragPreview {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x94a3b8))
            .bg(rgb(0xffffff))
            .text_sm()
            .text_color(rgb(0x1f2937))
            .child(format!(
                "{} -> {}",
                self.label,
                display_path_for_drag(&self.path)
            ))
    }
}

fn display_path_for_drag(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}
