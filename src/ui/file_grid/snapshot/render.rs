use super::{
    FileGridIconRequest, RawFileGridSnapshot, VisibleItemSnapshot, VisibleItemSnapshotCache,
};
use crate::ui::icons::FileIconSnapshot;

use super::super::FileGridSnapshot;
use super::super::details::DetailsItemSnapshot;

impl RawFileGridSnapshot {
    pub(crate) fn into_file_grid_snapshot<F>(
        self,
        selection_count: usize,
        visible_item_cache: &mut VisibleItemSnapshotCache,
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
                            &mut icon_for_item,
                        );
                        Some(VisibleItemSnapshot {
                            slot_id: item.slot_id,
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
                            &mut icon_for_item,
                        );
                        Some(VisibleItemSnapshot {
                            slot_id: item.slot_id,
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
                            icon_size: metrics.icon_size,
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
