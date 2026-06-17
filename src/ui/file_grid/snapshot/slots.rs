use super::super::VisibleItemSlotPool;
use super::RawFileGridSnapshot;

impl RawFileGridSnapshot {
    pub(crate) fn assign_visible_item_slots(&mut self, slots: &mut VisibleItemSlotPool) {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                slots.update_visible_items(items.iter().map(|item| item.item_id));
                for item in items {
                    item.slot_id = slots.slot_for_item(item.item_id).unwrap_or_default();
                }
            }
            Self::Details { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::sync::Arc;

    use fika_core::{IconsLayout, ItemId, ItemLayout};

    use crate::ui::icons::FileIconSnapshot;

    use super::super::super::FileGridSnapshot;
    use super::super::super::details::{details_layout_metrics, details_name_column_width};
    use super::super::{RawVisibleItemSnapshot, VisibleItemSnapshotCache};

    #[test]
    fn raw_file_grid_snapshot_assigns_slots_before_final_conversion() {
        let mut raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(2, fika_core::IconsLayoutOptions::default()),
            items: vec![
                test_raw_visible_item(1, "alpha.txt", 0),
                test_raw_visible_item(2, "beta.txt", 1),
            ],
        };
        let mut slots = VisibleItemSlotPool::default();

        raw_file_grid.assign_visible_item_slots(&mut slots);

        let mut requests = Vec::new();
        let icon = test_icon_snapshot();
        let mut cache = VisibleItemSnapshotCache::default();
        let snapshot = raw_file_grid.into_file_grid_snapshot(2, &mut cache, 48.0, |request| {
            requests.push((request.path.to_path_buf(), request.icon_size));
            icon.clone()
        });

        let FileGridSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons snapshot");
        };
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.slot_id != 0));
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, PathBuf::from("/tmp/alpha.txt"));
    }

    #[test]
    fn details_snapshot_preserves_item_view_slot_pool() {
        let mut slots = VisibleItemSlotPool::default();
        let item_id = ItemId(7);
        slots.update_visible_items([item_id]);
        let slot_id = slots.slot_for_item(item_id);
        let mut raw_file_grid = RawFileGridSnapshot::Details {
            items: Vec::new(),
            row_count: 0,
            metrics: details_layout_metrics(48.0),
            name_column_width: details_name_column_width(0.0, details_layout_metrics(48.0)),
        };

        raw_file_grid.assign_visible_item_slots(&mut slots);

        assert_eq!(slots.slot_for_item(item_id), slot_id);
    }

    fn test_raw_visible_item(id: u64, name: &str, model_index: usize) -> RawVisibleItemSnapshot {
        RawVisibleItemSnapshot {
            slot_id: 0,
            visible: true,
            layout: test_layout(model_index),
            item_id: ItemId(id),
            path: PathBuf::from("/tmp").join(name),
            is_dir: false,
            name: Arc::from(name),
            thumbnail_path: None,
            thumbnail_failed: false,
            modified_secs: Some(42),
            size_bytes: 12,
            metadata_complete: true,
            metadata_refresh_pending: false,
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
            selected: false,
            drop_target: false,
            draft_name: None,
            draft_caret: None,
            draft_selection: None,
            draft_error: None,
            draft_warning: None,
        }
    }

    fn test_icon_snapshot() -> FileIconSnapshot {
        FileIconSnapshot {
            icon_name: Arc::from("text-plain"),
            path: None,
            fallback_marker: Arc::from("TXT"),
            fallback_fg: 0xffffff,
            fallback_bg: 0x222222,
        }
    }

    fn test_layout(model_index: usize) -> ItemLayout {
        let rect = fika_core::ViewRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        ItemLayout {
            model_index,
            column: 0,
            row: model_index,
            item_rect: rect,
            visual_rect: rect,
            icon_rect: rect,
            text_rect: rect,
        }
    }
}
