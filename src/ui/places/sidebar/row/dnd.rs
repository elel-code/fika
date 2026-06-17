use std::path::{Path, PathBuf};

use crate::FikaApp;
use gpui::prelude::*;
use gpui::{Context, Div, ExternalPaths, Stateful};

use crate::ui::drag_drop::item_drop_reject_reason;
use crate::ui::file_grid::ItemDrag;

use super::super::super::drag::{PlaceDrag, PlaceDropZone, place_drop_zone};
use super::super::super::interaction::{
    PlaceRowTargetInput, place_row_path_list_target, place_row_place_drag_target,
};
use super::super::dnd_helpers::{
    apply_place_interaction_decision, refresh_place_interaction_cursor,
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
    let decision = place_row_path_list_target(PlaceRowTargetInput {
        drop_zone,
        mounted,
        can_add_place: app.dragged_paths_can_add_place(source_paths),
        accepts_place: item_drop_reject_reason(source_paths, target_path).is_none(),
        insert_before_index,
        insert_after_index,
        target_path,
    });
    let changed = apply_place_interaction_decision(app, &decision);
    refresh_place_interaction_cursor(app, decision.cursor, window, cx);
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
            let decision = place_row_place_drag_target(
                drag.movable(),
                drag.source_index(),
                drop_zone,
                insert_before_index,
                insert_after_index,
            );
            let changed = apply_place_interaction_decision(this, &decision);
            refresh_place_interaction_cursor(this, decision.cursor, window, cx);
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
