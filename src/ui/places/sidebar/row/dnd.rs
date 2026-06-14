use std::path::PathBuf;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, ExternalPaths, Stateful};

use crate::ui::drag_drop::{
    FileTransferMode, refresh_active_drag_cursor_for_drop_menu,
    refresh_active_drag_cursor_for_transfer_mode, refresh_active_drag_cursor_not_allowed,
};
use crate::ui::file_grid::ItemDrag;

use super::super::super::drag::{
    PlaceDrag, PlaceDropZone, place_drag_insert_index_for_zone, place_drop_zone,
};

pub(super) struct PlaceRowDndConfig {
    pub(super) mounted: bool,
    pub(super) insert_before_index: usize,
    pub(super) insert_after_index: usize,
    pub(super) path_for_internal_target: PathBuf,
    pub(super) path_for_internal_drop: PathBuf,
    pub(super) path_for_external_target: PathBuf,
    pub(super) path_for_external_drop: PathBuf,
}

pub(super) fn install_place_row_dnd(
    row: Stateful<Div>,
    config: PlaceRowDndConfig,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let PlaceRowDndConfig {
        mounted,
        insert_before_index,
        insert_after_index,
        path_for_internal_target,
        path_for_internal_drop,
        path_for_external_target,
        path_for_external_drop,
    } = config;
    let path_for_item_target = path_for_internal_target.clone();
    let path_for_place_drag_leave = path_for_internal_target.clone();
    let path_for_external_move_target = path_for_external_target.clone();

    row.on_drag_move::<ItemDrag>(cx.listener(
        move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
            if !event.bounds.contains(&event.event.position) {
                if this.clear_place_drop_target_for_row(
                    &path_for_item_target,
                    insert_before_index,
                    insert_after_index,
                ) {
                    cx.notify();
                }
                return;
            }
            let drop_zone = place_drop_zone(event);
            let cursor_mode = match drop_zone {
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                    Some(FileTransferMode::Copy)
                }
                PlaceDropZone::OnPlace if mounted => None,
                PlaceDropZone::OnPlace => None,
            };
            let changed = match drop_zone {
                PlaceDropZone::InsertBefore => {
                    this.set_place_drag_drop_target_for_insert(insert_before_index)
                }
                PlaceDropZone::InsertAfter => {
                    this.set_place_drag_drop_target_for_insert(insert_after_index)
                }
                PlaceDropZone::OnPlace if mounted => {
                    this.set_drop_menu_position(event.event.position);
                    this.set_place_drag_drop_target_for_path(path_for_item_target.clone())
                }
                PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
            };
            if let Some(cursor_mode) = cursor_mode {
                refresh_active_drag_cursor_for_transfer_mode(cursor_mode, window, cx);
            } else if mounted && matches!(drop_zone, PlaceDropZone::OnPlace) {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
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
                if this.clear_place_drop_target_for_row(
                    &path_for_external_move_target,
                    insert_before_index,
                    insert_after_index,
                ) {
                    cx.notify();
                }
                return;
            }
            let drop_zone = place_drop_zone(event);
            let cursor_mode = match drop_zone {
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                    Some(FileTransferMode::Copy)
                }
                PlaceDropZone::OnPlace if mounted => None,
                PlaceDropZone::OnPlace => None,
            };
            let changed = match drop_zone {
                PlaceDropZone::InsertBefore => {
                    this.set_place_drag_drop_target_for_insert(insert_before_index)
                }
                PlaceDropZone::InsertAfter => {
                    this.set_place_drag_drop_target_for_insert(insert_after_index)
                }
                PlaceDropZone::OnPlace if mounted => {
                    this.set_drop_menu_position(event.event.position);
                    this.set_place_drag_drop_target_for_path(path_for_external_move_target.clone())
                }
                PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
            };
            if let Some(cursor_mode) = cursor_mode {
                refresh_active_drag_cursor_for_transfer_mode(cursor_mode, window, cx);
            } else if mounted && matches!(drop_zone, PlaceDropZone::OnPlace) {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
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
                if this.clear_place_drop_target_for_row(
                    &path_for_place_drag_leave,
                    insert_before_index,
                    insert_after_index,
                ) {
                    cx.notify();
                }
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
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Move, window, cx);
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
    .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, _window, cx| {
        if mounted {
            this.drop_item_drag_to_current_place_target(
                drag.payload(),
                path_for_internal_drop.clone(),
                cx,
            );
        }
        cx.stop_propagation();
        cx.notify();
    }))
    .on_drop::<ExternalPaths>(cx.listener(
        move |this, external_paths: &ExternalPaths, _window, cx| {
            if mounted {
                this.drop_external_paths_to_current_place_target(
                    external_paths.paths().to_vec(),
                    path_for_external_drop.clone(),
                    cx,
                );
            }
            cx.stop_propagation();
            cx.notify();
        },
    ))
    .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
        this.drop_place_drag_to_current_place_target(drag.source_index(), insert_after_index);
        cx.stop_propagation();
        cx.notify();
    }))
}
