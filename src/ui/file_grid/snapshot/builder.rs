use std::ops::Range;
use std::path::{Path, PathBuf};

use super::super::details::{
    details_deletion_time_label, details_layout_metrics, details_modified_label,
    details_name_column_width, details_original_path_label, details_size_label,
    details_visible_row_range,
};
use super::super::layout::{
    compact_layout_for_filtered_model_with_text_override,
    compact_layout_for_model_with_text_override, entry_name_text_width, icons_layout_for_model,
    model_index_for_layout_index, rename_text_override_for_model, required_text_width_for_entry,
};
use super::visible_item_thumbnail_path;
use super::{
    RawDetailsItemSnapshot, RawFileGridSnapshot, RawFileGridSnapshotInput, RawVisibleItemSnapshot,
};
use crate::ui::drag_drop::{ItemDropTarget, item_drop_target_matches_directory};
use crate::ui::rename::RenameDraft;
use crate::ui::retained::dolphin_visible_work_indexes;

use fika_core::{DirectoryModel, FilteredModel, ItemLayout, PaneId, SelectionState, ViewMode};

pub(crate) fn raw_file_grid_snapshot(input: RawFileGridSnapshotInput<'_>) -> RawFileGridSnapshot {
    let RawFileGridSnapshotInput {
        pane_id,
        model,
        selection,
        view,
        filtered,
        source_revision,
        rename_draft,
        item_drop_target,
        compact_column_widths,
    } = input;
    let item_count = filtered.map_or_else(|| model.len(), FilteredModel::len);
    let rename_text_override = rename_text_override_for_model(model, rename_draft);

    match view.view_mode {
        ViewMode::Compact => {
            let layout = match filtered {
                Some(filtered) => compact_layout_for_filtered_model_with_text_override(
                    compact_column_widths,
                    model,
                    filtered,
                    source_revision,
                    view,
                    rename_text_override,
                ),
                None => compact_layout_for_model_with_text_override(
                    compact_column_widths,
                    model,
                    view,
                    rename_text_override,
                ),
            };
            let visible_range = visible_layout_index_range(layout.visible_items());
            let viewport = layout.viewport_rect();
            let work_range = visible_range
                .as_ref()
                .map(|range| item_view_work_indexes(range.clone(), item_count))
                .unwrap_or_default();
            let items = work_range
                .into_iter()
                .filter_map(|layout_index| {
                    let model_index = model_index_for_layout_index(filtered, layout_index)?;
                    let entry = model.get(model_index)?;
                    let path = model.path_for_index(model_index)?;
                    let active_rename_draft = active_rename_draft_for_path(rename_draft, &path);
                    let required_text_width =
                        required_text_width_for_entry(entry, active_rename_draft);
                    let item_layout = layout
                        .item_with_required_text_width(layout_index, Some(required_text_width))?;
                    Some(raw_visible_item_snapshot(
                        pane_id,
                        selection,
                        item_drop_target,
                        active_rename_draft,
                        item_layout,
                        entry,
                        path,
                        item_layout.item_rect.intersects(viewport),
                    ))
                })
                .collect::<Vec<_>>();
            RawFileGridSnapshot::Compact { layout, items }
        }
        ViewMode::Icons => {
            let layout =
                icons_layout_for_model(model, filtered, item_count, view, rename_draft, 0.0);
            let visible_range = visible_layout_index_range(layout.visible_items());
            let viewport = layout.viewport_rect();
            let work_range = visible_range
                .as_ref()
                .map(|range| item_view_work_indexes(range.clone(), item_count))
                .unwrap_or_default();
            let items = work_range
                .into_iter()
                .filter_map(|layout_index| {
                    let model_index = model_index_for_layout_index(filtered, layout_index)?;
                    let entry = model.get(model_index)?;
                    let path = model.path_for_index(model_index)?;
                    let active_rename_draft = active_rename_draft_for_path(rename_draft, &path);
                    let item_layout = layout.item(layout_index)?;
                    Some(raw_visible_item_snapshot(
                        pane_id,
                        selection,
                        item_drop_target,
                        active_rename_draft,
                        item_layout,
                        entry,
                        path,
                        item_layout.item_rect.intersects(viewport),
                    ))
                })
                .collect::<Vec<_>>();
            RawFileGridSnapshot::Icons { layout, items }
        }
        ViewMode::Details => {
            let row_count = item_count;
            let metrics = details_layout_metrics(view.icon_size());
            let name_column_width = details_name_column_width(
                max_details_name_text_width(model, filtered, row_count),
                metrics,
            );
            let items =
                details_visible_row_range(row_count, view.viewport_height, view.scroll_y, metrics)
                    .filter_map(|row_index| {
                        let model_index = model_index_for_layout_index(filtered, row_index)?;
                        let entry = model.get(model_index)?;
                        let path = model.path_for_index(model_index)?;
                        let selected = selection.is_selected(entry.id);
                        let drop_target =
                            item_drop_target_matches_directory(item_drop_target, pane_id, &path);
                        Some(RawDetailsItemSnapshot {
                            row_index,
                            item_id: entry.id,
                            path,
                            is_dir: entry.is_dir,
                            name: entry.name.clone(),
                            size_bytes: entry.effective_size_bytes(),
                            modified_secs: entry.effective_modified_secs(),
                            mime_type: entry.effective_mime_type_cloned(),
                            mime_magic_checked: entry.effective_mime_magic_checked(),
                            selected,
                            drop_target,
                            size_label: details_size_label(entry),
                            modified_label: details_modified_label(entry),
                            original_path_label: details_original_path_label(entry),
                            deletion_time_label: details_deletion_time_label(entry),
                        })
                    })
                    .collect::<Vec<_>>();
            RawFileGridSnapshot::Details {
                items,
                row_count,
                metrics,
                name_column_width,
            }
        }
    }
}

fn max_details_name_text_width(
    model: &DirectoryModel,
    filtered: Option<&FilteredModel>,
    row_count: usize,
) -> f32 {
    (0..row_count)
        .filter_map(|layout_index| {
            let model_index = model_index_for_layout_index(filtered, layout_index)?;
            model
                .get(model_index)
                .map(|entry| entry_name_text_width(entry))
        })
        .fold(0.0, f32::max)
}

fn active_rename_draft_for_path<'a>(
    rename_draft: Option<&'a RenameDraft>,
    path: &Path,
) -> Option<&'a RenameDraft> {
    rename_draft.filter(|draft| draft.original_path == path)
}

fn raw_visible_item_snapshot(
    pane_id: PaneId,
    selection: &SelectionState,
    item_drop_target: Option<&ItemDropTarget>,
    active_rename_draft: Option<&RenameDraft>,
    layout: ItemLayout,
    entry: &fika_core::ModelEntry,
    path: PathBuf,
    visible: bool,
) -> RawVisibleItemSnapshot {
    let selected = selection.is_selected(entry.id);
    let drop_target = item_drop_target_matches_directory(item_drop_target, pane_id, &path);
    let thumbnail_path = visible_item_thumbnail_path(entry);
    RawVisibleItemSnapshot {
        slot_id: 0,
        visible,
        layout,
        item_id: entry.id,
        path,
        is_dir: entry.is_dir,
        name: entry.name.clone(),
        thumbnail_path,
        thumbnail_failed: entry.thumbnail_failed,
        modified_secs: entry.effective_modified_secs(),
        size_bytes: entry.effective_size_bytes(),
        metadata_complete: entry.effective_metadata_complete(),
        metadata_refresh_pending: entry.metadata_refresh_pending,
        mime_type: entry.effective_mime_type_cloned(),
        mime_magic_checked: entry.effective_mime_magic_checked(),
        selected,
        drop_target,
        draft_name: active_rename_draft.map(|draft| draft.draft_name.clone()),
        draft_caret: active_rename_draft.map(|draft| draft.caret),
        draft_selection: active_rename_draft.and_then(|draft| draft.selection),
        draft_error: active_rename_draft.and_then(|draft| draft.error.clone()),
        draft_warning: active_rename_draft.and_then(|draft| draft.extension_warning(entry.is_dir)),
    }
}

fn visible_layout_index_range(items: impl IntoIterator<Item = ItemLayout>) -> Option<Range<usize>> {
    let mut indexes = items.into_iter().map(|item| item.model_index);
    let first = indexes.next()?;
    let mut start = first;
    let mut end = first;
    for index in indexes {
        start = start.min(index);
        end = end.max(index);
    }
    Some(start..end + 1)
}

fn item_view_work_indexes(visible_range: Range<usize>, item_count: usize) -> Vec<usize> {
    dolphin_visible_work_indexes(visible_range.clone(), item_count, visible_range.len())
}
