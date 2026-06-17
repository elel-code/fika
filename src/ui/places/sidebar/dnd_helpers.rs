use crate::FikaApp;
use gpui::Context;

use crate::ui::drag_drop::{
    FileTransferMode, refresh_active_drag_cursor_for_drop_menu,
    refresh_active_drag_cursor_for_transfer_mode, refresh_active_drag_cursor_not_allowed,
};

use super::super::interaction::{
    PlaceInteractionCursor, PlaceInteractionDecision, PlaceInteractionTarget,
};

pub(super) fn apply_place_interaction_decision(
    app: &mut FikaApp,
    decision: &PlaceInteractionDecision,
) -> bool {
    match &decision.target {
        PlaceInteractionTarget::Clear => app.clear_drag_drop_targets(),
        PlaceInteractionTarget::Insert { index } => {
            app.set_place_drag_drop_target_for_insert(*index)
        }
        PlaceInteractionTarget::Place { path } => {
            app.set_place_drag_drop_target_for_path(path.clone())
        }
    }
}

pub(super) fn refresh_place_interaction_cursor(
    app: &mut FikaApp,
    cursor: PlaceInteractionCursor,
    window: &mut gpui::Window,
    cx: &mut Context<FikaApp>,
) {
    match cursor {
        PlaceInteractionCursor::Copy => {
            refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Copy, window, cx);
            app.refresh_drop_target_lease(cx);
        }
        PlaceInteractionCursor::Move => {
            refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Move, window, cx);
            app.refresh_drop_target_lease(cx);
        }
        PlaceInteractionCursor::DropMenu => {
            refresh_active_drag_cursor_for_drop_menu(window, cx);
            app.refresh_drop_target_lease(cx);
        }
        PlaceInteractionCursor::NotAllowed => refresh_active_drag_cursor_not_allowed(window, cx),
    }
}
