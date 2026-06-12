mod devices;
mod model;
mod projection;
mod snapshot;

pub(crate) use devices::replace_removable_device_places;
pub(crate) use model::{
    DEVICES_GROUP, PlaceEntry, REMOVABLE_DEVICES_GROUP, build_places, default_place_label,
    read_live_device_snapshot, removable_device_place_entries,
};
#[cfg(test)]
pub(crate) use model::{
    NETWORK_GROUP, active_place_index, build_places_with_devices, place_is_mounted,
};
pub(crate) use projection::place_snapshots_for;
pub(crate) use snapshot::{PlaceIcon, PlaceSnapshot};

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{
    Context, Div, ExternalPaths, MouseButton, NavigationDirection, ParentElement, Stateful, Styled,
    StyledImage, div, img, px, rgb, rgba,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::drag_drop::{
    FileTransferMode, file_transfer_mode_for_modifiers,
    refresh_active_drag_cursor_for_transfer_mode, refresh_active_drag_cursor_not_allowed,
};
use super::file_grid::ItemDrag;
use super::icons::FileIconSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlaceDrag {
    path: PathBuf,
    label: Arc<str>,
    source_index: usize,
    movable: bool,
}

impl PlaceDrag {
    pub(crate) fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub(crate) fn source_index(&self) -> usize {
        self.source_index
    }

    fn movable(&self) -> bool {
        self.movable
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
                    move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position);
                        let changed =
                            contains && this.set_place_drag_drop_target_for_insert(insert_index);
                        if contains {
                            refresh_active_drag_cursor_for_transfer_mode(
                                FileTransferMode::Copy,
                                window,
                                cx,
                            );
                            this.schedule_drop_target_stale_clear(cx);
                        }
                        if changed {
                            cx.notify();
                        }
                        if contains {
                            cx.stop_propagation();
                        }
                    },
                ))
                .on_drag_move::<ExternalPaths>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position);
                        let changed =
                            contains && this.set_place_drag_drop_target_for_insert(insert_index);
                        if contains {
                            refresh_active_drag_cursor_for_transfer_mode(
                                FileTransferMode::Copy,
                                window,
                                cx,
                            );
                            this.schedule_drop_target_stale_clear(cx);
                        }
                        if changed {
                            cx.notify();
                        }
                        if contains {
                            cx.stop_propagation();
                        }
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
                .on_drag_move::<PlaceDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position);
                        let drag = event.drag(cx);
                        let changed = contains
                            && drag.movable()
                            && this.set_place_drag_drop_target_for_insert(insert_index);
                        if contains && drag.movable() {
                            refresh_active_drag_cursor_for_transfer_mode(
                                FileTransferMode::Move,
                                window,
                                cx,
                            );
                            this.schedule_drop_target_stale_clear(cx);
                        } else if contains {
                            let cleared = this.clear_drag_drop_targets();
                            refresh_active_drag_cursor_not_allowed(window, cx);
                            if cleared {
                                cx.notify();
                            }
                        }
                        if changed {
                            cx.notify();
                        }
                        if contains {
                            cx.stop_propagation();
                        }
                    },
                ))
                .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
                    this.drop_place_drag_to_place_insert(drag.source_index(), insert_index);
                    cx.stop_propagation();
                    cx.notify();
                }))
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
        source_index: place.index,
        movable: place.editable && place.removable,
    };
    let context_place = place.clone();
    let insert_before_index = place.index;
    let insert_after_index = place.index + 1;
    let row_drop_target = place.drop_target;
    let active = place.active;
    let mounted = place.mounted;
    let device = place.device;
    let network = place.network;
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
                .border_1()
                .border_color(place_row_border_color(active, row_drop_target))
                .bg(place_row_background(active, row_drop_target))
                .hover(move |row| row.bg(place_row_hover_background(active, row_drop_target)))
                .when(mounted || device || network, |row| row.cursor_pointer())
                .on_drag(place_drag, |drag, _, _, cx| {
                    cx.new(|_| PlaceDragPreview {
                        label: drag.label.clone(),
                        path: drag.path.clone(),
                    })
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
                )
                .on_drag_move::<ItemDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                        if !event.bounds.contains(&event.event.position) {
                            return;
                        }
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        let drop_zone = place_drop_zone(event);
                        let cursor_mode = match drop_zone {
                            PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                                Some(FileTransferMode::Copy)
                            }
                            PlaceDropZone::OnPlace if mounted => Some(mode),
                            PlaceDropZone::OnPlace => None,
                        };
                        let changed = match drop_zone {
                            PlaceDropZone::InsertBefore => {
                                this.set_place_drag_drop_target_for_insert(insert_before_index)
                            }
                            PlaceDropZone::InsertAfter => {
                                this.set_place_drag_drop_target_for_insert(insert_after_index)
                            }
                            PlaceDropZone::OnPlace if mounted => this
                                .set_place_drag_drop_target_for_path(
                                    path_for_internal_target.clone(),
                                    mode,
                                ),
                            PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
                        };
                        if let Some(cursor_mode) = cursor_mode {
                            refresh_active_drag_cursor_for_transfer_mode(cursor_mode, window, cx);
                        } else {
                            refresh_active_drag_cursor_not_allowed(window, cx);
                        }
                        this.schedule_drop_target_stale_clear(cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drag_move::<ExternalPaths>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                        if !event.bounds.contains(&event.event.position) {
                            return;
                        }
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        let drop_zone = place_drop_zone(event);
                        let cursor_mode = match drop_zone {
                            PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                                Some(FileTransferMode::Copy)
                            }
                            PlaceDropZone::OnPlace if mounted => Some(mode),
                            PlaceDropZone::OnPlace => None,
                        };
                        let changed = match drop_zone {
                            PlaceDropZone::InsertBefore => {
                                this.set_place_drag_drop_target_for_insert(insert_before_index)
                            }
                            PlaceDropZone::InsertAfter => {
                                this.set_place_drag_drop_target_for_insert(insert_after_index)
                            }
                            PlaceDropZone::OnPlace if mounted => this
                                .set_place_drag_drop_target_for_path(
                                    path_for_external_target.clone(),
                                    mode,
                                ),
                            PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
                        };
                        if let Some(cursor_mode) = cursor_mode {
                            refresh_active_drag_cursor_for_transfer_mode(cursor_mode, window, cx);
                        } else {
                            refresh_active_drag_cursor_not_allowed(window, cx);
                        }
                        this.schedule_drop_target_stale_clear(cx);
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drag_move::<PlaceDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                        if !event.bounds.contains(&event.event.position) {
                            return;
                        }
                        let drag = event.drag(cx);
                        let Some(insert_index) = place_drag_insert_index_for_zone(
                            drag.source_index(),
                            insert_before_index,
                            place_drop_zone(event),
                        ) else {
                            let changed = this.clear_drag_drop_targets();
                            refresh_active_drag_cursor_not_allowed(window, cx);
                            if changed {
                                cx.notify();
                            }
                            cx.stop_propagation();
                            return;
                        };
                        let changed = if drag.movable() {
                            this.set_place_drag_drop_target_for_insert(insert_index)
                        } else {
                            this.clear_drag_drop_targets()
                        };
                        if drag.movable() {
                            refresh_active_drag_cursor_for_transfer_mode(
                                FileTransferMode::Move,
                                window,
                                cx,
                            );
                            this.schedule_drop_target_stale_clear(cx);
                        } else {
                            refresh_active_drag_cursor_not_allowed(window, cx);
                        }
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    if mounted {
                        this.drop_item_drag_to_current_place_target(
                            drag.payload(),
                            path_for_internal_drop.clone(),
                            mode,
                            cx,
                        );
                    }
                    cx.stop_propagation();
                    cx.notify();
                }))
                .on_drop::<ExternalPaths>(cx.listener(
                    move |this, external_paths: &ExternalPaths, window, cx| {
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        if mounted {
                            this.drop_external_paths_to_current_place_target(
                                external_paths.paths().to_vec(),
                                path_for_external_drop.clone(),
                                mode,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ))
                .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
                    this.drop_place_drag_to_current_place_target(
                        drag.source_index(),
                        insert_after_index,
                    );
                    cx.stop_propagation();
                    cx.notify();
                }))
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

fn place_icon_view(icon: &FileIconSnapshot, active: bool) -> Div {
    let fallback_kind = place_fallback_kind_for_snapshot(icon);
    let fallback_fg = if active { 0x1f4fbf } else { icon.fallback_fg };
    let fallback_bg = if active { 0xeaf1ff } else { icon.fallback_bg };
    let container = div()
        .w(px(22.0))
        .h(px(22.0))
        .flex_none()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden();

    match &icon.path {
        Some(path) => {
            container.child(img(path.clone()).size_full().with_fallback(move || {
                place_fallback_icon(fallback_kind, fallback_fg, fallback_bg)
            }))
        }
        None => container.child(place_fallback_icon(fallback_kind, fallback_fg, fallback_bg)),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceFallbackKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Trash,
    Root,
    Bookmark,
    Folder,
}

fn place_fallback_kind_for_snapshot(icon: &FileIconSnapshot) -> PlaceFallbackKind {
    let icon_name = icon.icon_name.as_str();
    if icon_name.contains("home") {
        PlaceFallbackKind::Home
    } else if icon_name.contains("desktop") || icon_name.contains("display") {
        PlaceFallbackKind::Desktop
    } else if icon_name.contains("document") {
        PlaceFallbackKind::Documents
    } else if icon_name.contains("download") {
        PlaceFallbackKind::Downloads
    } else if icon_name.contains("music") || icon_name.contains("audio") {
        PlaceFallbackKind::Music
    } else if icon_name.contains("picture")
        || icon_name.contains("image")
        || icon_name.contains("photo")
    {
        PlaceFallbackKind::Pictures
    } else if icon_name.contains("video") {
        PlaceFallbackKind::Videos
    } else if icon_name.contains("trash") {
        PlaceFallbackKind::Trash
    } else if icon_name.contains("harddisk") || icon_name.contains("root") {
        PlaceFallbackKind::Root
    } else if icon_name.contains("favorite") || icon_name.contains("bookmark") {
        PlaceFallbackKind::Bookmark
    } else {
        PlaceFallbackKind::Folder
    }
}

fn place_fallback_icon(kind: PlaceFallbackKind, fg: u32, bg: u32) -> gpui::AnyElement {
    let icon = div()
        .size_full()
        .rounded_md()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg))
        .overflow_hidden();

    match kind {
        PlaceFallbackKind::Home => icon
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(6.0))
                    .w(px(10.0))
                    .h(px(4.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .top(px(10.0))
                    .w(px(12.0))
                    .h(px(8.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(13.0))
                    .w(px(3.0))
                    .h(px(5.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Desktop => icon
            .child(
                div()
                    .absolute()
                    .left(px(4.0))
                    .top(px(5.0))
                    .w(px(14.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(15.0))
                    .w(px(2.0))
                    .h(px(3.0))
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(18.0))
                    .w(px(8.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Documents => icon
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(4.0))
                    .w(px(10.0))
                    .h(px(14.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(9.0))
                    .w(px(6.0))
                    .h(px(1.0))
                    .bg(rgb(bg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(12.0))
                    .w(px(6.0))
                    .h(px(1.0))
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Downloads => folder_icon_shape(icon, fg, bg)
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(8.0))
                    .w(px(2.0))
                    .h(px(7.0))
                    .bg(rgb(bg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(13.0))
                    .w(px(6.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Music => icon
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(5.0))
                    .w(px(2.0))
                    .h(px(10.0))
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(13.0))
                    .w(px(7.0))
                    .h(px(5.0))
                    .rounded_full()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(5.0))
                    .w(px(6.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Pictures => icon
            .child(
                div()
                    .absolute()
                    .left(px(4.0))
                    .top(px(5.0))
                    .w(px(14.0))
                    .h(px(12.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(8.0))
                    .w(px(3.0))
                    .h(px(3.0))
                    .rounded_full()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(14.0))
                    .w(px(10.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Videos => icon
            .child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .top(px(6.0))
                    .w(px(12.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .top(px(9.0))
                    .w(px(6.0))
                    .h(px(4.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Trash => icon
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(5.0))
                    .w(px(8.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(8.0))
                    .w(px(10.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(10.0))
                    .w(px(2.0))
                    .h(px(6.0))
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Root => icon
            .child(
                div()
                    .absolute()
                    .left(px(4.0))
                    .top(px(6.0))
                    .w(px(14.0))
                    .h(px(10.0))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(12.0))
                    .w(px(8.0))
                    .h(px(2.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(15.0))
                    .top(px(8.0))
                    .w(px(2.0))
                    .h(px(2.0))
                    .rounded_full()
                    .bg(rgb(fg)),
            ),
        PlaceFallbackKind::Bookmark => icon
            .child(
                div()
                    .absolute()
                    .left(px(7.0))
                    .top(px(4.0))
                    .w(px(8.0))
                    .h(px(14.0))
                    .rounded_sm()
                    .bg(rgb(fg)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(9.0))
                    .top(px(14.0))
                    .w(px(4.0))
                    .h(px(4.0))
                    .rounded_sm()
                    .bg(rgb(bg)),
            ),
        PlaceFallbackKind::Folder => folder_icon_shape(icon, fg, bg),
    }
    .into_any_element()
}

fn folder_icon_shape(icon: Div, fg: u32, bg: u32) -> Div {
    icon.child(
        div()
            .absolute()
            .left(px(5.0))
            .top(px(6.0))
            .w(px(7.0))
            .h(px(4.0))
            .rounded_sm()
            .bg(rgb(fg)),
    )
    .child(
        div()
            .absolute()
            .left(px(4.0))
            .top(px(9.0))
            .w(px(14.0))
            .h(px(8.0))
            .rounded_sm()
            .bg(rgb(fg)),
    )
    .child(
        div()
            .absolute()
            .left(px(6.0))
            .top(px(11.0))
            .w(px(10.0))
            .h(px(2.0))
            .rounded_sm()
            .bg(rgb(bg)),
    )
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

fn place_drag_insert_index_for_zone(
    source_index: usize,
    target_index: usize,
    zone: PlaceDropZone,
) -> Option<usize> {
    match zone {
        PlaceDropZone::InsertBefore => Some(target_index),
        PlaceDropZone::InsertAfter => Some(target_index + 1),
        PlaceDropZone::OnPlace if source_index < target_index => Some(target_index + 1),
        PlaceDropZone::OnPlace if source_index > target_index => Some(target_index),
        PlaceDropZone::OnPlace => None,
    }
}

fn place_row_background(active: bool, drop_target: Option<FileTransferMode>) -> gpui::Rgba {
    if let Some(mode) = drop_target {
        place_drop_target_background(mode)
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xf8f9fb)
    }
}

fn place_row_border_color(active: bool, drop_target: Option<FileTransferMode>) -> gpui::Rgba {
    if let Some(mode) = drop_target {
        place_drop_target_border_color(mode)
    } else if active {
        rgb(0xbfdbfe)
    } else {
        rgba(0x00000000)
    }
}

fn place_row_hover_background(active: bool, drop_target: Option<FileTransferMode>) -> gpui::Rgba {
    if let Some(mode) = drop_target {
        place_drop_target_hover_background(mode)
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xeef3f8)
    }
}

fn place_drop_target_background(mode: FileTransferMode) -> gpui::Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a34),
        FileTransferMode::Move => rgba(0xd9770634),
        FileTransferMode::Link => rgba(0x7c3aed34),
    }
}

fn place_drop_target_hover_background(mode: FileTransferMode) -> gpui::Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a4a),
        FileTransferMode::Move => rgba(0xd977064a),
        FileTransferMode::Link => rgba(0x7c3aed4a),
    }
}

fn place_drop_target_border_color(mode: FileTransferMode) -> gpui::Rgba {
    match mode {
        FileTransferMode::Copy => rgb(0x16a34a),
        FileTransferMode::Move => rgb(0xd97706),
        FileTransferMode::Link => rgb(0x7c3aed),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn icon_snapshot(icon_name: &str, fallback_marker: &str) -> FileIconSnapshot {
        FileIconSnapshot {
            icon_name: icon_name.to_string(),
            path: None,
            fallback_marker: fallback_marker.to_string(),
            fallback_fg: 0x1f4fbf,
            fallback_bg: 0xeaf1ff,
        }
    }

    #[test]
    fn place_fallback_kind_uses_icon_identity_not_text_marker() {
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("user-desktop", "D")),
            PlaceFallbackKind::Desktop
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("folder-documents", "D")),
            PlaceFallbackKind::Documents
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("folder-download", "DL")),
            PlaceFallbackKind::Downloads
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("user-trash", "T")),
            PlaceFallbackKind::Trash
        );
        assert_eq!(
            place_fallback_kind_for_snapshot(&icon_snapshot("folder", "Documents")),
            PlaceFallbackKind::Folder
        );
    }
}
