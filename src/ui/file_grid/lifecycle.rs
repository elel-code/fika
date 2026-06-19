use std::path::Path;
use std::sync::Arc;

use fika_core::PaneId;

use crate::FikaApp;
use crate::ui::icons::ThemeIconImageKey;

impl FikaApp {
    pub(super) fn mark_theme_icon_image_path_ready(
        &mut self,
        key: ThemeIconImageKey,
        path: Arc<Path>,
    ) -> bool {
        self.theme_icon_readiness.mark_ready_path(key, path)
    }

    pub(crate) fn clear_file_grid_projection_state(&mut self, pane_id: PaneId) {
        self.visible_item_slots.remove(&pane_id);
        self.item_paint_slots.remove(&pane_id);
        self.visible_item_snapshot_caches.remove(&pane_id);
        self.static_item_text_shape_caches.remove(&pane_id);
        self.details_text_shape_caches.remove(&pane_id);
        self.clear_item_view_perf_state(pane_id);
        self.clear_hovered_item_for_pane(pane_id);
        self.compact_column_widths.remove(&pane_id);
        self.visible_work_keys.remove(&pane_id);
    }

    pub(crate) fn clear_file_grid_mode_switch_state(&mut self, pane_id: PaneId) {
        self.item_paint_slots.remove(&pane_id);
        self.visible_item_snapshot_caches.remove(&pane_id);
        self.static_item_text_shape_caches.remove(&pane_id);
        self.details_text_shape_caches.remove(&pane_id);
        self.clear_item_view_perf_layer_stats(pane_id);
        self.clear_hovered_item_for_pane(pane_id);
        self.compact_column_widths.remove(&pane_id);
    }

    pub(crate) fn invalidate_file_grid_visible_snapshot_cache(&mut self, pane_id: PaneId) {
        self.visible_item_snapshot_caches.remove(&pane_id);
    }

    pub(crate) fn invalidate_all_file_grid_visible_snapshot_caches(&mut self) {
        self.visible_item_snapshot_caches.clear();
    }
}
