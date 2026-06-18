use super::snapshot::{
    RawFileGridSnapshot, RetainedFileGridProjection, project_retained_file_grid_snapshot,
};
use crate::FikaApp;
use fika_core::PaneId;

impl FikaApp {
    pub(crate) fn project_retained_file_grid_for_pane(
        &mut self,
        pane_id: PaneId,
        raw_file_grid: RawFileGridSnapshot,
        selection_count: usize,
        file_icon_size: f32,
    ) -> RetainedFileGridProjection {
        let mut visible_item_slots = self.visible_item_slots.remove(&pane_id).unwrap_or_default();
        let mut visible_item_cache = self
            .visible_item_snapshot_caches
            .remove(&pane_id)
            .unwrap_or_default();
        let mut item_paint_slots = self.item_paint_slots.remove(&pane_id).unwrap_or_default();
        let hovered_item = self.hovered_item.item_for_pane(pane_id);
        let projection = project_retained_file_grid_snapshot(
            raw_file_grid,
            selection_count,
            &mut visible_item_slots,
            &mut visible_item_cache,
            &mut item_paint_slots,
            hovered_item,
            file_icon_size,
            |request| {
                self.icon_snapshot_for_model_item(
                    request.path,
                    request.is_dir,
                    request.mime_type.clone(),
                    request.mime_magic_checked,
                    request.icon_size,
                )
            },
        );
        self.visible_item_slots.insert(pane_id, visible_item_slots);
        self.visible_item_snapshot_caches
            .insert(pane_id, visible_item_cache);
        self.item_paint_slots.insert(pane_id, item_paint_slots);
        projection
    }
}
