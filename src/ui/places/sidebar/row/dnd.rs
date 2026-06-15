use std::path::{Path, PathBuf};

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, ExternalPaths, Stateful};

use crate::ui::drag_drop::{
    FileTransferMode, item_drop_reject_reason, refresh_active_drag_cursor_for_drop_menu,
    refresh_active_drag_cursor_for_transfer_mode, refresh_active_drag_cursor_not_allowed,
};
use crate::ui::file_grid::ItemDrag;

use super::super::super::drag::{
    PlaceDrag, PlaceDropZone, place_drag_insert_index, place_drag_insert_index_for_zone,
    place_drop_zone,
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

fn place_drag_row_insert_index(
    movable: bool,
    source_index: usize,
    drop_zone: PlaceDropZone,
    insert_before_index: usize,
    insert_after_index: usize,
) -> Option<usize> {
    if !movable {
        return None;
    }
    match drop_zone {
        PlaceDropZone::InsertBefore => place_drag_insert_index(source_index, insert_before_index),
        PlaceDropZone::InsertAfter => place_drag_insert_index(source_index, insert_after_index),
        PlaceDropZone::OnPlace => {
            place_drag_insert_index_for_zone(source_index, insert_before_index, drop_zone)
        }
    }
}

fn handle_place_row_path_list_drag_move(
    app: &mut FikaApp,
    source_paths: &[PathBuf],
    drop_zone: PlaceDropZone,
    mounted: bool,
    insert_before_index: usize,
    insert_after_index: usize,
    target_path: &Path,
    window: &mut gpui::Window,
    cx: &mut Context<FikaApp>,
) {
    let accepts_insert = app.dragged_paths_can_add_place(source_paths);
    let accepts_place = mounted && item_drop_reject_reason(source_paths, target_path).is_none();
    let changed = match drop_zone {
        PlaceDropZone::InsertBefore if accepts_insert => {
            app.set_place_drag_drop_target_for_insert(insert_before_index)
        }
        PlaceDropZone::InsertAfter if accepts_insert => {
            app.set_place_drag_drop_target_for_insert(insert_after_index)
        }
        PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter => app.clear_drag_drop_targets(),
        PlaceDropZone::OnPlace if accepts_place => {
            app.set_place_drag_drop_target_for_path(target_path.to_path_buf())
        }
        PlaceDropZone::OnPlace => app.clear_drag_drop_targets(),
    };
    if accepts_insert
        && matches!(
            drop_zone,
            PlaceDropZone::InsertBefore | PlaceDropZone::InsertAfter
        )
    {
        refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
        app.refresh_drop_target_lease(cx);
    } else if accepts_place && matches!(drop_zone, PlaceDropZone::OnPlace) {
        refresh_active_drag_cursor_for_drop_menu(window, cx);
        app.refresh_drop_target_lease(cx);
    } else {
        refresh_active_drag_cursor_not_allowed(window, cx);
    }
    if changed {
        cx.notify();
    }
    cx.stop_propagation();
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
    let path_for_external_move_target = path_for_external_target.clone();

    row.on_drag_move::<ItemDrag>(cx.listener(
        move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
            if !event.bounds.contains(&event.event.position) {
                return;
            }
            let source_paths = this.item_drag_source_paths(&event.drag(cx).payload());
            let drop_zone = place_drop_zone(event);
            handle_place_row_path_list_drag_move(
                this,
                &source_paths,
                drop_zone,
                mounted,
                insert_before_index,
                insert_after_index,
                &path_for_item_target,
                window,
                cx,
            );
        },
    ))
    .on_drag_move::<ExternalPaths>(cx.listener(
        move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
            if !event.bounds.contains(&event.event.position) {
                return;
            }
            let source_paths = this.external_drag_source_paths(event.drag(cx).paths());
            let drop_zone = place_drop_zone(event);
            handle_place_row_path_list_drag_move(
                this,
                &source_paths,
                drop_zone,
                mounted,
                insert_before_index,
                insert_after_index,
                &path_for_external_move_target,
                window,
                cx,
            );
        },
    ))
    .on_drag_move::<PlaceDrag>(cx.listener(
        move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
            if !event.bounds.contains(&event.event.position) {
                return;
            }
            let drag = event.drag(cx);
            let drop_zone = place_drop_zone(event);
            let insert_index = place_drag_row_insert_index(
                drag.movable(),
                drag.source_index(),
                drop_zone,
                insert_before_index,
                insert_after_index,
            );
            let changed = match insert_index {
                Some(index) => this.set_place_drag_drop_target_for_insert(index),
                None => this.clear_drag_drop_targets(),
            };
            match insert_index {
                Some(_) => {
                    refresh_active_drag_cursor_for_transfer_mode(
                        FileTransferMode::Move,
                        window,
                        cx,
                    );
                    this.refresh_drop_target_lease(cx);
                }
                None => {
                    refresh_active_drag_cursor_not_allowed(window, cx);
                }
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
        if this.current_place_drop_target_is_insert() {
            let position = window.mouse_position();
            this.drop_place_drag_to_current_place_target(drag.source_index(), position, cx);
        }
        cx.stop_propagation();
        cx.notify();
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movable_place_drag_uses_row_body_as_reorder_target() {
        assert_eq!(
            place_drag_row_insert_index(true, 0, PlaceDropZone::OnPlace, 1, 2),
            Some(2)
        );
        assert_eq!(
            place_drag_row_insert_index(true, 2, PlaceDropZone::OnPlace, 1, 2),
            Some(1)
        );
        assert_eq!(
            place_drag_row_insert_index(true, 1, PlaceDropZone::OnPlace, 1, 2),
            None
        );
    }

    #[test]
    fn movable_place_drag_uses_row_edges_for_reorder() {
        assert_eq!(
            place_drag_row_insert_index(true, 0, PlaceDropZone::InsertAfter, 1, 2),
            Some(2)
        );
        assert_eq!(
            place_drag_row_insert_index(true, 2, PlaceDropZone::InsertBefore, 1, 2),
            Some(1)
        );
    }

    #[test]
    fn movable_place_drag_rejects_noop_row_edges() {
        assert_eq!(
            place_drag_row_insert_index(true, 0, PlaceDropZone::InsertBefore, 0, 1),
            None
        );
        assert_eq!(
            place_drag_row_insert_index(true, 0, PlaceDropZone::InsertAfter, 0, 1),
            None
        );
        assert_eq!(
            place_drag_row_insert_index(true, 1, PlaceDropZone::InsertAfter, 0, 1),
            None
        );
    }

    #[test]
    fn non_movable_place_drag_has_no_places_row_target() {
        assert_eq!(
            place_drag_row_insert_index(false, 0, PlaceDropZone::OnPlace, 1, 2),
            None
        );
        assert_eq!(
            place_drag_row_insert_index(false, 0, PlaceDropZone::InsertAfter, 1, 2),
            None
        );
    }
}
