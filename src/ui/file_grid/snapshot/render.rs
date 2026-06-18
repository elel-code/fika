use super::{
    FileGridIconRequest, RawFileGridSnapshot, VisibleItemSnapshot, VisibleItemSnapshotCache,
};
use crate::ui::icons::FileIconSnapshot;

use super::super::details::DetailsItemSnapshot;
use super::super::{
    FileGridRenderSnapshot, FileGridSnapshot, ItemPaintSlotCache, ItemPaintSlotStats,
    VisibleItemSlotPool,
};

pub(crate) struct RetainedFileGridProjection {
    pub(crate) snapshot: FileGridRenderSnapshot,
    pub(crate) slot_stats: ItemPaintSlotStats,
}

pub(crate) fn project_retained_file_grid_snapshot<F>(
    mut raw_file_grid: RawFileGridSnapshot,
    selection_count: usize,
    visible_item_slots: &mut VisibleItemSlotPool,
    visible_item_cache: &mut VisibleItemSnapshotCache,
    item_paint_slots: &mut ItemPaintSlotCache,
    hovered_item: Option<fika_core::ItemId>,
    file_icon_size: f32,
    icon_for_item: F,
) -> RetainedFileGridProjection
where
    F: for<'a> FnMut(FileGridIconRequest<'a>) -> FileIconSnapshot,
{
    raw_file_grid.assign_visible_item_slots(visible_item_slots);
    let file_grid = raw_file_grid.into_file_grid_snapshot(
        selection_count,
        visible_item_cache,
        file_icon_size,
        icon_for_item,
    );
    let projection = item_paint_slots.project_file_grid_snapshot(file_grid, hovered_item);
    RetainedFileGridProjection {
        snapshot: projection.snapshot,
        slot_stats: projection.stats,
    }
}

impl RawFileGridSnapshot {
    pub(crate) fn into_file_grid_snapshot<F>(
        self,
        selection_count: usize,
        visible_item_cache: &mut VisibleItemSnapshotCache,
        file_icon_size: f32,
        mut icon_for_item: F,
    ) -> FileGridSnapshot
    where
        F: for<'a> FnMut(FileGridIconRequest<'a>) -> FileIconSnapshot,
    {
        match self {
            Self::Compact { layout, items } => {
                visible_item_cache.begin_visible_update();
                let items = items
                    .into_iter()
                    .filter_map(|item| {
                        if item.slot_id == 0 {
                            return None;
                        }
                        let content = visible_item_cache.content_for_raw_item(
                            &item,
                            false,
                            item.visible,
                            file_icon_size,
                            &mut icon_for_item,
                        )?;
                        Some(VisibleItemSnapshot {
                            slot_id: item.slot_id,
                            visible: item.visible,
                            item_id: item.item_id,
                            layout: item.layout,
                            is_dir: content.is_dir,
                            name: content.name,
                            display_name: content.display_name,
                            thumbnail_path: content.thumbnail_path,
                            icon: content.icon,
                            fallback_marker: content.fallback_marker,
                            icon_name_lines: content.icon_name_lines,
                            drag_path: content.drag_path,
                            selected: item.selected,
                            selection_count,
                            drop_target: item.drop_target,
                            draft_name: item.draft_name,
                            draft_caret: item.draft_caret,
                            draft_selection: item.draft_selection,
                            draft_error: item.draft_error,
                            draft_warning: item.draft_warning,
                        })
                    })
                    .collect::<Vec<_>>();
                visible_item_cache.retain_current_visible();
                FileGridSnapshot::Compact { layout, items }
            }
            Self::Icons { layout, items } => {
                visible_item_cache.begin_visible_update();
                let items = items
                    .into_iter()
                    .filter_map(|item| {
                        if item.slot_id == 0 {
                            return None;
                        }
                        let content = visible_item_cache.content_for_raw_item(
                            &item,
                            true,
                            item.visible,
                            file_icon_size,
                            &mut icon_for_item,
                        )?;
                        Some(VisibleItemSnapshot {
                            slot_id: item.slot_id,
                            visible: item.visible,
                            item_id: item.item_id,
                            layout: item.layout,
                            is_dir: content.is_dir,
                            name: content.name,
                            display_name: content.display_name,
                            thumbnail_path: content.thumbnail_path,
                            icon: content.icon,
                            fallback_marker: content.fallback_marker,
                            icon_name_lines: content.icon_name_lines,
                            drag_path: content.drag_path,
                            selected: item.selected,
                            selection_count,
                            drop_target: item.drop_target,
                            draft_name: item.draft_name,
                            draft_caret: item.draft_caret,
                            draft_selection: item.draft_selection,
                            draft_error: item.draft_error,
                            draft_warning: item.draft_warning,
                        })
                    })
                    .collect::<Vec<_>>();
                visible_item_cache.retain_current_visible();
                FileGridSnapshot::Icons { layout, items }
            }
            Self::Details {
                items,
                row_count,
                metrics,
                name_column_width,
            } => {
                let items = items
                    .into_iter()
                    .map(|item| {
                        let icon = icon_for_item(FileGridIconRequest {
                            path: &item.path,
                            is_dir: item.is_dir,
                            mime_type: item.mime_type.clone(),
                            mime_magic_checked: item.mime_magic_checked,
                            icon_size: file_icon_size,
                        });
                        DetailsItemSnapshot {
                            row_index: item.row_index,
                            item_id: item.item_id,
                            path: item.path,
                            is_dir: item.is_dir,
                            name: item.name,
                            icon,
                            selected: item.selected,
                            selection_count,
                            drop_target: item.drop_target,
                            size_label: item.size_label,
                            modified_label: item.modified_label,
                            original_path_label: item.original_path_label,
                            deletion_time_label: item.deletion_time_label,
                        }
                    })
                    .collect::<Vec<_>>();
                FileGridSnapshot::Details {
                    items,
                    row_count,
                    metrics,
                    name_column_width,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::sync::Arc;

    use fika_core::{IconsLayout, ItemId, ItemLayout};
    use gpui::SharedString;

    use crate::ui::icons::FileIconSnapshot;

    use super::super::super::VisibleItemSlotPool;
    use super::super::super::layout::icon_name_display_lines;
    use super::super::super::{FileGridRenderSnapshot, FileGridSnapshot, ItemPaintSlotCache};
    use super::super::visible::{icon_name_layout_width, icon_name_max_lines};
    use super::super::{RawVisibleItemSnapshot, VisibleItemSnapshotCache};

    #[test]
    fn retained_file_grid_projection_assigns_slots_and_paint_state() {
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(1, "visible.txt", 0)],
        };
        let mut slots = VisibleItemSlotPool::default();
        let mut cache = VisibleItemSnapshotCache::default();
        let mut paint_slots = ItemPaintSlotCache::default();
        let icon = test_icon_snapshot();
        let mut icon_requests = Vec::new();

        let projection = project_retained_file_grid_snapshot(
            raw_file_grid,
            1,
            &mut slots,
            &mut cache,
            &mut paint_slots,
            Some(ItemId(1)),
            48.0,
            |request| {
                icon_requests.push(request.path.to_path_buf());
                icon.clone()
            },
        );

        assert_eq!(icon_requests, vec![PathBuf::from("/tmp/visible.txt")]);
        assert_eq!(projection.slot_stats.inserted, 1);
        assert_eq!(projection.slot_stats.entries, 1);
        let slot_id = slots
            .slot_for_item(ItemId(1))
            .expect("visible item should receive a retained slot");
        let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
            panic!("expected retained icons snapshot");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].slot_id, slot_id);
        assert_eq!(items[0].item_id, ItemId(1));
        assert!(items[0].visual.hovered);
    }

    #[test]
    fn raw_icon_snapshot_does_not_resolve_uncached_read_ahead_item_content() {
        let mut read_ahead = test_raw_visible_item(2, "read-ahead.txt", 1);
        read_ahead.visible = false;
        let mut raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(2, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(1, "visible.txt", 0), read_ahead],
        };
        let mut slots = VisibleItemSlotPool::default();
        raw_file_grid.assign_visible_item_slots(&mut slots);
        let icon = test_icon_snapshot();
        let mut icon_requests = Vec::new();
        let mut cache = VisibleItemSnapshotCache::default();

        let snapshot = raw_file_grid.into_file_grid_snapshot(1, &mut cache, 48.0, |request| {
            icon_requests.push(request.path.to_path_buf());
            icon.clone()
        });

        let FileGridSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons snapshot");
        };
        assert_eq!(icon_requests, vec![PathBuf::from("/tmp/visible.txt")]);
        assert_eq!(items.len(), 1);
        assert!(items[0].visible);
        assert_eq!(items[0].item_id, ItemId(1));
    }

    #[test]
    fn raw_icon_snapshot_reuses_cached_read_ahead_item_content_without_resolving_it() {
        let icon = test_icon_snapshot();
        let mut slots = VisibleItemSlotPool::default();
        let mut cache = VisibleItemSnapshotCache::default();
        let mut first_raw = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(1, "cached.txt", 0)],
        };
        first_raw.assign_visible_item_slots(&mut slots);
        let _first = first_raw.into_file_grid_snapshot(1, &mut cache, 48.0, |_| icon.clone());

        let mut cached_read_ahead = test_raw_visible_item(1, "cached.txt", 0);
        cached_read_ahead.visible = false;
        let mut second_raw = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(2, fika_core::IconsLayoutOptions::default()),
            items: vec![
                cached_read_ahead,
                test_raw_visible_item(2, "visible-now.txt", 1),
            ],
        };
        second_raw.assign_visible_item_slots(&mut slots);
        let mut icon_requests = Vec::new();

        let snapshot = second_raw.into_file_grid_snapshot(1, &mut cache, 48.0, |request| {
            icon_requests.push(request.path.to_path_buf());
            icon.clone()
        });

        let FileGridSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons snapshot");
        };
        assert_eq!(icon_requests, vec![PathBuf::from("/tmp/visible-now.txt")]);
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .any(|item| item.item_id == ItemId(1) && !item.visible)
        );
        assert!(
            items
                .iter()
                .any(|item| item.item_id == ItemId(2) && item.visible)
        );
    }

    #[test]
    fn icon_snapshot_precomputes_name_lines_with_safe_width() {
        let long_name = "elzykosuda227446+breuyev@hotmail.cpa.2026-06-22.json";
        let mut raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(1, long_name, 0)],
        };
        let mut slots = VisibleItemSlotPool::default();
        raw_file_grid.assign_visible_item_slots(&mut slots);
        let icon = test_icon_snapshot();
        let mut cache = VisibleItemSnapshotCache::default();

        let snapshot = raw_file_grid.into_file_grid_snapshot(1, &mut cache, 48.0, |_| icon.clone());

        let FileGridSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons snapshot");
        };
        let item = items.first().expect("icon item should be visible");
        let expected = icon_name_display_lines(
            long_name,
            icon_name_layout_width(item.layout.text_rect.width),
            icon_name_max_lines(item.layout.text_rect.height),
        );
        assert_eq!(
            item.icon_name_lines
                .iter()
                .map(SharedString::as_ref)
                .collect::<Vec<_>>(),
            expected.iter().map(String::as_str).collect::<Vec<_>>()
        );
        assert!(
            item.icon_name_lines
                .last()
                .is_some_and(|line| line.contains('\u{2026}'))
        );
    }

    #[test]
    fn icon_snapshot_uses_view_icon_size_instead_of_layout_icon_rect() {
        let mut item = test_raw_visible_item(1, "zoomed.txt", 0);
        item.layout.icon_rect.width = 128.0;
        item.layout.icon_rect.height = 128.0;
        let mut raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![item],
        };
        let mut slots = VisibleItemSlotPool::default();
        raw_file_grid.assign_visible_item_slots(&mut slots);
        let icon = test_icon_snapshot();
        let mut cache = VisibleItemSnapshotCache::default();
        let mut requested_icon_sizes = Vec::new();

        let _snapshot = raw_file_grid.into_file_grid_snapshot(1, &mut cache, 48.0, |request| {
            requested_icon_sizes.push(request.icon_size);
            icon.clone()
        });

        assert_eq!(requested_icon_sizes, vec![48.0]);
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
