use std::path::PathBuf;

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, ExternalPaths, Stateful};

use crate::ui::drag_drop::{
    FileTransferMode, item_drop_reject_reason, refresh_active_drag_cursor_for_drop_menu,
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
                return;
            }
            let source_paths = this.item_drag_source_paths(&event.drag(cx).payload());
            let drop_zone = place_drop_zone(event);
            let accepts_insert = this.dragged_paths_can_add_place(&source_paths);
            let accepts_place =
                mounted && item_drop_reject_reason(&source_paths, &path_for_item_target).is_none();
            let changed = match drop_zone {
                PlaceDropZone::InsertBefore if accepts_insert => {
                    this.set_place_drag_drop_target_for_insert(insert_before_index)
                }
                PlaceDropZone::InsertAfter if accepts_insert => {
                    this.set_place_drag_drop_target_for_insert(insert_after_index)
                }
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                    this.clear_drag_drop_targets()
                }
                PlaceDropZone::OnPlace if accepts_place => {
                    this.set_place_drag_drop_target_for_path(path_for_item_target.clone())
                }
                PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
            };
            if accepts_insert
                && matches!(
                    drop_zone,
                    PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter
                )
            {
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
                this.schedule_drop_target_stale_clear(cx);
            } else if accepts_place && matches!(drop_zone, PlaceDropZone::OnPlace) {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
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
    .on_drag_move::<ExternalPaths>(cx.listener(
        move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
            if !event.bounds.contains(&event.event.position) {
                return;
            }
            let source_paths = this.external_drag_source_paths(event.drag(cx).paths());
            let drop_zone = place_drop_zone(event);
            let accepts_insert = this.dragged_paths_can_add_place(&source_paths);
            let accepts_place = mounted
                && item_drop_reject_reason(&source_paths, &path_for_external_move_target).is_none();
            let changed = match drop_zone {
                PlaceDropZone::InsertBefore if accepts_insert => {
                    this.set_place_drag_drop_target_for_insert(insert_before_index)
                }
                PlaceDropZone::InsertAfter if accepts_insert => {
                    this.set_place_drag_drop_target_for_insert(insert_after_index)
                }
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                    this.clear_drag_drop_targets()
                }
                PlaceDropZone::OnPlace if accepts_place => {
                    this.set_place_drag_drop_target_for_path(path_for_external_move_target.clone())
                }
                PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
            };
            if accepts_insert
                && matches!(
                    drop_zone,
                    PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter
                )
            {
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
                this.schedule_drop_target_stale_clear(cx);
            } else if accepts_place && matches!(drop_zone, PlaceDropZone::OnPlace) {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
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
    .on_drag_move::<PlaceDrag>(cx.listener(
        move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
            if !event.bounds.contains(&event.event.position) {
                return;
            }
            let drag = event.drag(cx);
            let source_path = drag.path();
            let drop_zone = place_drop_zone(event);
            let accepts_place = mounted
                && item_drop_reject_reason(
                    std::slice::from_ref(&source_path),
                    &path_for_place_drag_leave,
                )
                .is_none();
            let changed = match drop_zone {
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter if drag.movable() => {
                    let insert_index = place_drag_insert_index_for_zone(
                        drag.source_index(),
                        insert_before_index,
                        drop_zone,
                    )
                    .unwrap_or(insert_after_index);
                    this.set_place_drag_drop_target_for_insert(insert_index)
                }
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => {
                    if accepts_place {
                        this.set_place_drag_drop_target_for_path(path_for_place_drag_leave.clone())
                    } else {
                        this.clear_drag_drop_targets()
                    }
                }
                PlaceDropZone::OnPlace if accepts_place => {
                    this.set_place_drag_drop_target_for_path(path_for_place_drag_leave.clone())
                }
                PlaceDropZone::OnPlace => this.clear_drag_drop_targets(),
            };
            if matches!(
                drop_zone,
                PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter
            ) && drag.movable()
            {
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Move, window, cx);
                this.schedule_drop_target_stale_clear(cx);
            } else if accepts_place {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
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
        if this.current_place_drop_target_is_insert()
            || this.current_place_drop_target_matches_path(&path_for_internal_drop)
        {
            let position = window.mouse_position();
            this.drop_item_drag_to_current_place_target(
                drag.payload(),
                path_for_internal_drop.clone(),
                position,
                cx,
            );
        }
        cx.stop_propagation();
        cx.notify();
    }))
    .on_drop::<ExternalPaths>(cx.listener(
        move |this, external_paths: &ExternalPaths, window, cx| {
            if this.current_place_drop_target_is_insert()
                || this.current_place_drop_target_matches_path(&path_for_external_drop)
            {
                let position = window.mouse_position();
                this.drop_external_paths_to_current_place_target(
                    external_paths.paths().to_vec(),
                    path_for_external_drop.clone(),
                    position,
                    cx,
                );
            }
            cx.stop_propagation();
            cx.notify();
        },
    ))
    .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, window, cx| {
        let position = window.mouse_position();
        this.drop_place_drag_to_current_place_target(
            drag.source_index(),
            insert_after_index,
            position,
            cx,
        );
        cx.stop_propagation();
        cx.notify();
    }))
}
