use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::details::{
    DetailsItemSnapshot, details_deletion_time_label, details_modified_label,
    details_original_path_label, details_size_label, details_visible_row_range,
};
use super::layout::{
    CompactColumnWidthCache, CompactTextWidthOverride,
    compact_layout_for_filtered_model_with_text_override,
    compact_layout_for_model_with_text_override, compact_text_width, compact_text_width_for_name,
    model_index_for_layout_index,
};
use super::{FileGridSnapshot, VisibleItemSlotPool, icons_layout_options};
use crate::ui::drag_drop::{FileTransferMode, ItemDropTarget, item_drop_target_mode_for_directory};
use crate::ui::icons::FileIconSnapshot;
use crate::ui::rename::RenameDraft;

use fika_core::{
    CompactLayout, DirectoryModel, FilteredModel, Generation, IconsLayout, ItemId, ItemLayout,
    MetadataRoleCandidate, MetadataRoleScheduler, PaneId, SelectionState, ThumbnailCandidate,
    ThumbnailRequestPriority, ThumbnailScheduler, ViewMode, ViewState,
    mime_magic_resolution_required, thumbnail_read_ahead_indexes,
    thumbnail_request_may_have_preview,
};

pub(crate) fn format_entry_kind_label(entry: &fika_core::ModelEntry) -> String {
    if let Some(deletion_time) = &entry.trash_deletion_time {
        return fika_core::format_trash_deletion_time(deletion_time);
    }
    if entry.is_dir {
        "Folder".to_string()
    } else if !entry.effective_metadata_complete()
        && entry.effective_size_bytes() == 0
        && entry.effective_modified_secs().is_none()
    {
        "-".to_string()
    } else {
        fika_core::format_size(entry.effective_size_bytes())
    }
}

pub(crate) fn format_entry_detail_label(entry: &fika_core::ModelEntry) -> String {
    if let Some(original_path) = &entry.trash_original_path {
        return fika_core::format_trash_original_location(
            original_path,
            entry.trash_deletion_time.as_deref(),
        );
    }
    format_entry_kind_label(entry)
}

pub(crate) fn visible_item_thumbnail_path(entry: &fika_core::ModelEntry) -> Option<PathBuf> {
    if entry.is_dir {
        None
    } else {
        entry.thumbnail_path.clone()
    }
}

pub(crate) fn rename_text_override_for_model(
    model: &DirectoryModel,
    draft: Option<&RenameDraft>,
) -> Option<CompactTextWidthOverride> {
    let draft = draft?;
    let model_index = model.index_of_path(&draft.original_path)?;
    Some(CompactTextWidthOverride {
        model_index,
        text_width: compact_text_width_for_name(&draft.draft_name),
    })
}

pub(crate) fn required_text_width_for_entry(
    entry: &fika_core::EntryData,
    draft: Option<&RenameDraft>,
) -> f32 {
    let base_width = compact_text_width(entry.name_width_units);
    draft
        .map(|draft| base_width.max(compact_text_width_for_name(&draft.draft_name)))
        .unwrap_or(base_width)
}

#[derive(Clone, Debug)]
pub(crate) struct VisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) layout: fika_core::ItemLayout,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) detail_label: String,
    pub(crate) thumbnail_path: Option<PathBuf>,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) selected: bool,
    pub(crate) selection_count: usize,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) draft_name: Option<String>,
    pub(crate) draft_caret: Option<usize>,
    pub(crate) draft_selection: Option<(usize, usize)>,
    pub(crate) draft_error: Option<String>,
    pub(crate) draft_warning: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct RawVisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) layout: ItemLayout,
    pub(crate) item_id: ItemId,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) detail_label: String,
    pub(crate) thumbnail_path: Option<PathBuf>,
    pub(crate) modified_secs: Option<u64>,
    pub(crate) size_bytes: u64,
    pub(crate) metadata_complete: bool,
    pub(crate) metadata_refresh_pending: bool,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) icon_name: Option<Arc<str>>,
    pub(crate) selected: bool,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) draft_name: Option<String>,
    pub(crate) draft_caret: Option<usize>,
    pub(crate) draft_selection: Option<(usize, usize)>,
    pub(crate) draft_error: Option<String>,
    pub(crate) draft_warning: Option<String>,
}

impl RawVisibleItemSnapshot {
    fn thumbnail_candidate(&self) -> Option<ThumbnailCandidate> {
        visible_thumbnail_candidate(
            self.item_id,
            &self.path,
            self.is_dir,
            self.thumbnail_path.as_ref(),
            self.modified_secs,
            self.size_bytes,
            self.metadata_complete,
            self.metadata_refresh_pending,
            self.mime_type.as_ref(),
            self.mime_magic_checked,
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RawDetailsItemSnapshot {
    pub(crate) row_index: usize,
    pub(crate) item_id: ItemId,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) size_bytes: u64,
    pub(crate) metadata_complete: bool,
    pub(crate) metadata_refresh_pending: bool,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) icon_name: Option<Arc<str>>,
    pub(crate) selected: bool,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) size_label: String,
    pub(crate) modified_label: String,
    pub(crate) original_path_label: String,
    pub(crate) deletion_time_label: String,
}

#[derive(Clone, Debug)]
pub(crate) enum RawFileGridSnapshot {
    Compact {
        layout: CompactLayout,
        items: Vec<RawVisibleItemSnapshot>,
    },
    Icons {
        layout: IconsLayout,
        items: Vec<RawVisibleItemSnapshot>,
    },
    Details {
        items: Vec<RawDetailsItemSnapshot>,
        row_count: usize,
    },
}

pub(crate) struct FileGridIconRequest<'a> {
    pub(crate) item_id: ItemId,
    pub(crate) path: &'a Path,
    pub(crate) is_dir: bool,
    pub(crate) metadata_complete: bool,
    pub(crate) size_bytes: u64,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) icon_name: Option<Arc<str>>,
    pub(crate) icon_size: f32,
}

pub(crate) struct RawFileGridSnapshotInput<'a> {
    pub(crate) pane_id: PaneId,
    pub(crate) model: &'a DirectoryModel,
    pub(crate) selection: &'a SelectionState,
    pub(crate) view: &'a ViewState,
    pub(crate) filtered: Option<&'a FilteredModel>,
    pub(crate) source_revision: u64,
    pub(crate) rename_draft: Option<&'a RenameDraft>,
    pub(crate) item_drop_target: Option<&'a ItemDropTarget>,
    pub(crate) compact_column_widths: &'a mut CompactColumnWidthCache,
}

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
            let items = layout
                .visible_items()
                .filter_map(|visible_item| {
                    let layout_index = visible_item.model_index;
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
                    ))
                })
                .collect::<Vec<_>>();
            RawFileGridSnapshot::Compact { layout, items }
        }
        ViewMode::Icons => {
            let layout = IconsLayout::new(item_count, icons_layout_options(view, 0.0));
            let items = layout
                .visible_items()
                .filter_map(|visible_item| {
                    let layout_index = visible_item.model_index;
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
                    ))
                })
                .collect::<Vec<_>>();
            RawFileGridSnapshot::Icons { layout, items }
        }
        ViewMode::Details => {
            let row_count = item_count;
            let items = details_visible_row_range(row_count, view.viewport_height, view.scroll_y)
                .filter_map(|row_index| {
                    let model_index = model_index_for_layout_index(filtered, row_index)?;
                    let entry = model.get(model_index)?;
                    let path = model.path_for_index(model_index)?;
                    let selected = selection.is_selected(entry.id);
                    let drop_target =
                        item_drop_target_mode_for_directory(item_drop_target, pane_id, &path);
                    Some(RawDetailsItemSnapshot {
                        row_index,
                        item_id: entry.id,
                        path,
                        is_dir: entry.is_dir,
                        name: entry.name.clone(),
                        size_bytes: entry.effective_size_bytes(),
                        metadata_complete: entry.effective_metadata_complete(),
                        metadata_refresh_pending: entry.metadata_refresh_pending,
                        mime_type: entry.effective_mime_type_cloned(),
                        mime_magic_checked: entry.effective_mime_magic_checked(),
                        icon_name: entry.icon_name.clone(),
                        selected,
                        drop_target,
                        size_label: details_size_label(entry),
                        modified_label: details_modified_label(entry),
                        original_path_label: details_original_path_label(entry),
                        deletion_time_label: details_deletion_time_label(entry),
                    })
                })
                .collect::<Vec<_>>();
            RawFileGridSnapshot::Details { items, row_count }
        }
    }
}

impl RawFileGridSnapshot {
    pub(crate) fn assign_visible_item_slots(&mut self, slots: &mut VisibleItemSlotPool) {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                slots.update_visible_items(items.iter().map(|item| item.item_id));
                for item in items {
                    item.slot_id = slots.slot_for_item(item.item_id).unwrap_or_default();
                }
            }
            Self::Details { .. } => slots.update_visible_items(std::iter::empty::<ItemId>()),
        }
    }

    pub(crate) fn into_file_grid_snapshot<F>(
        self,
        selection_count: usize,
        mut icon_for_item: F,
    ) -> FileGridSnapshot
    where
        F: for<'a> FnMut(FileGridIconRequest<'a>) -> FileIconSnapshot,
    {
        match self {
            Self::Compact { layout, items } => {
                let items = items
                    .into_iter()
                    .filter_map(|item| {
                        if item.slot_id == 0 {
                            return None;
                        }
                        let icon = icon_for_item(FileGridIconRequest {
                            item_id: item.item_id,
                            path: &item.path,
                            is_dir: item.is_dir,
                            metadata_complete: item.metadata_complete,
                            size_bytes: item.size_bytes,
                            mime_type: item.mime_type.clone(),
                            mime_magic_checked: item.mime_magic_checked,
                            icon_name: item.icon_name.clone(),
                            icon_size: item.layout.icon_rect.width,
                        });
                        Some(VisibleItemSnapshot {
                            slot_id: item.slot_id,
                            layout: item.layout,
                            path: item.path,
                            is_dir: item.is_dir,
                            name: item.name,
                            detail_label: item.detail_label,
                            thumbnail_path: item.thumbnail_path,
                            icon,
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
                FileGridSnapshot::Compact { layout, items }
            }
            Self::Icons { layout, items } => {
                let items = items
                    .into_iter()
                    .filter_map(|item| {
                        if item.slot_id == 0 {
                            return None;
                        }
                        let icon = icon_for_item(FileGridIconRequest {
                            item_id: item.item_id,
                            path: &item.path,
                            is_dir: item.is_dir,
                            metadata_complete: item.metadata_complete,
                            size_bytes: item.size_bytes,
                            mime_type: item.mime_type.clone(),
                            mime_magic_checked: item.mime_magic_checked,
                            icon_name: item.icon_name.clone(),
                            icon_size: item.layout.icon_rect.width,
                        });
                        Some(VisibleItemSnapshot {
                            slot_id: item.slot_id,
                            layout: item.layout,
                            path: item.path,
                            is_dir: item.is_dir,
                            name: item.name,
                            detail_label: item.detail_label,
                            thumbnail_path: item.thumbnail_path,
                            icon,
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
                FileGridSnapshot::Icons { layout, items }
            }
            Self::Details { items, row_count } => {
                let items = items
                    .into_iter()
                    .map(|item| {
                        let icon = icon_for_item(FileGridIconRequest {
                            item_id: item.item_id,
                            path: &item.path,
                            is_dir: item.is_dir,
                            metadata_complete: item.metadata_complete,
                            size_bytes: item.size_bytes,
                            mime_type: item.mime_type.clone(),
                            mime_magic_checked: item.mime_magic_checked,
                            icon_name: item.icon_name.clone(),
                            icon_size: super::details::DETAILS_ICON_SIZE,
                        });
                        DetailsItemSnapshot {
                            row_index: item.row_index,
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
                FileGridSnapshot::Details { items, row_count }
            }
        }
    }

    pub(crate) fn visible_layout_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                raw_visible_layout_range_and_count(items)
            }
            Self::Details { .. } => None,
        }
    }

    pub(crate) fn queue_metadata_role_candidates(
        &self,
        scheduler: &mut MetadataRoleScheduler,
        pane_id: PaneId,
        generation: Generation,
    ) -> bool {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => scheduler.queue_candidates(
                pane_id,
                generation,
                items
                    .iter()
                    .filter(|item| {
                        metadata_role_update_needed(
                            item.is_dir,
                            item.size_bytes,
                            item.metadata_complete,
                            item.metadata_refresh_pending,
                            item.mime_type.as_deref(),
                            item.mime_magic_checked,
                        )
                    })
                    .map(|item| MetadataRoleCandidate {
                        item_id: item.item_id,
                        path: item.path.clone(),
                    }),
            ),
            Self::Details { items, .. } => scheduler.queue_candidates(
                pane_id,
                generation,
                items
                    .iter()
                    .filter(|item| {
                        metadata_role_update_needed(
                            item.is_dir,
                            item.size_bytes,
                            item.metadata_complete,
                            item.metadata_refresh_pending,
                            item.mime_type.as_deref(),
                            item.mime_magic_checked,
                        )
                    })
                    .map(|item| MetadataRoleCandidate {
                        item_id: item.item_id,
                        path: item.path.clone(),
                    }),
            ),
        }
    }

    pub(crate) fn queue_thumbnail_candidates(
        &self,
        scheduler: &mut ThumbnailScheduler,
        pane_id: PaneId,
        generation: Generation,
        deferred_candidates: impl IntoIterator<Item = ThumbnailCandidate>,
    ) -> bool {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => scheduler.queue_candidates(
                pane_id,
                generation,
                items
                    .iter()
                    .filter_map(|item| item.thumbnail_candidate())
                    .chain(deferred_candidates),
            ),
            Self::Details { .. } => {
                scheduler.queue_candidates(pane_id, generation, deferred_candidates)
            }
        }
    }
}

fn metadata_role_update_needed(
    is_dir: bool,
    size_bytes: u64,
    metadata_complete: bool,
    metadata_refresh_pending: bool,
    mime_type: Option<&str>,
    mime_magic_checked: bool,
) -> bool {
    if is_dir {
        return false;
    }

    !metadata_complete
        || metadata_refresh_pending
        || mime_magic_resolution_required(is_dir, size_bytes, mime_type, mime_magic_checked)
}

fn active_rename_draft_for_path<'a>(
    rename_draft: Option<&'a RenameDraft>,
    path: &std::path::Path,
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
) -> RawVisibleItemSnapshot {
    let selected = selection.is_selected(entry.id);
    let drop_target = item_drop_target_mode_for_directory(item_drop_target, pane_id, &path);
    RawVisibleItemSnapshot {
        slot_id: 0,
        layout,
        item_id: entry.id,
        path,
        is_dir: entry.is_dir,
        name: entry.name.clone(),
        detail_label: format_entry_detail_label(entry),
        thumbnail_path: visible_item_thumbnail_path(entry),
        modified_secs: entry.effective_modified_secs(),
        size_bytes: entry.effective_size_bytes(),
        metadata_complete: entry.effective_metadata_complete(),
        metadata_refresh_pending: entry.metadata_refresh_pending,
        mime_type: entry.effective_mime_type_cloned(),
        mime_magic_checked: entry.effective_mime_magic_checked(),
        icon_name: entry.icon_name.clone(),
        selected,
        drop_target,
        draft_name: active_rename_draft.map(|draft| draft.draft_name.clone()),
        draft_caret: active_rename_draft.map(|draft| draft.caret),
        draft_selection: active_rename_draft.and_then(|draft| draft.selection),
        draft_error: active_rename_draft.and_then(|draft| draft.error.clone()),
        draft_warning: active_rename_draft.and_then(|draft| draft.extension_warning(entry.is_dir)),
    }
}

pub(crate) fn deferred_thumbnail_candidates_for_model<'a>(
    raw_file_grid: &RawFileGridSnapshot,
    model: &'a DirectoryModel,
    filtered: Option<&'a FilteredModel>,
    item_count: usize,
) -> impl Iterator<Item = ThumbnailCandidate> + 'a {
    raw_file_grid
        .visible_layout_range_and_count()
        .into_iter()
        .flat_map(move |(visible_range, visible_count)| {
            thumbnail_read_ahead_indexes(visible_range, item_count, visible_count)
        })
        .filter_map(move |layout_index| {
            let model_index = model_index_for_layout_index(filtered, layout_index)?;
            let entry = model.get(model_index)?;
            let path = model.path_for_index(model_index)?;
            if entry.is_dir
                || !entry.effective_metadata_complete()
                || entry.metadata_refresh_pending
                || visible_item_thumbnail_path(entry).is_some()
                || !thumbnail_request_may_have_preview(
                    &path,
                    entry.effective_mime_type().map(Arc::as_ref),
                )
            {
                return None;
            }
            if mime_magic_resolution_required(
                entry.is_dir,
                entry.effective_size_bytes(),
                entry.effective_mime_type().map(Arc::as_ref),
                entry.effective_mime_magic_checked(),
            ) {
                return None;
            }
            Some(ThumbnailCandidate {
                item_id: entry.id,
                path,
                modified_secs: entry.effective_modified_secs()?,
                metadata_complete: entry.effective_metadata_complete(),
                mime_type: entry
                    .effective_mime_type()
                    .map(|mime| mime.as_ref().to_string()),
                priority: ThumbnailRequestPriority::Deferred,
            })
        })
}

pub(crate) fn layout_index_range_and_count(
    indexes: impl IntoIterator<Item = usize>,
) -> Option<(Range<usize>, usize)> {
    let mut indexes = indexes.into_iter();
    let first = indexes.next()?;
    let mut start = first;
    let mut end = first;
    let mut count = 1;
    for index in indexes {
        start = start.min(index);
        end = end.max(index);
        count += 1;
    }
    Some((start..end + 1, count))
}

fn raw_visible_layout_range_and_count(
    items: &[RawVisibleItemSnapshot],
) -> Option<(Range<usize>, usize)> {
    layout_index_range_and_count(items.iter().map(|item| item.layout.model_index))
}

fn visible_thumbnail_candidate(
    item_id: ItemId,
    path: &std::path::Path,
    is_dir: bool,
    thumbnail_path: Option<&PathBuf>,
    modified_secs: Option<u64>,
    size_bytes: u64,
    metadata_complete: bool,
    metadata_refresh_pending: bool,
    mime_type: Option<&Arc<str>>,
    mime_magic_checked: bool,
) -> Option<ThumbnailCandidate> {
    if is_dir
        || !metadata_complete
        || metadata_refresh_pending
        || thumbnail_path.is_some()
        || !thumbnail_request_may_have_preview(path, mime_type.map(Arc::as_ref))
        || mime_magic_resolution_required(
            is_dir,
            size_bytes,
            mime_type.map(Arc::as_ref),
            mime_magic_checked,
        )
    {
        return None;
    }
    Some(ThumbnailCandidate {
        item_id,
        path: path.to_path_buf(),
        modified_secs: modified_secs?,
        metadata_complete,
        mime_type: mime_type.map(|mime| mime.as_ref().to_string()),
        priority: ThumbnailRequestPriority::Visible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_entry(data: fika_core::EntryData) -> fika_core::ModelEntry {
        fika_core::ModelEntry {
            id: fika_core::ItemId(1),
            entry: fika_core::Entry::new(data),
            metadata_role: None,
            metadata_refresh_pending: false,
            icon_name: None,
            thumbnail_path: None,
        }
    }

    #[test]
    fn visible_item_thumbnail_path_uses_file_cache_hit_only() {
        let thumbnail = PathBuf::from("/tmp/fika-thumbnail-cache/normal/hash.png");
        let file = fika_core::ModelEntry {
            id: fika_core::ItemId(1),
            metadata_role: None,
            metadata_refresh_pending: false,
            thumbnail_path: Some(thumbnail.clone()),
            icon_name: None,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("photo.jpg"),
                name_width_units: 9,
                size_bytes: 12,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: Some(Arc::from("image/jpeg")),
                mime_magic_checked: true,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: false,
            }),
        };
        let dir = fika_core::ModelEntry {
            id: fika_core::ItemId(2),
            metadata_role: None,
            metadata_refresh_pending: false,
            thumbnail_path: Some(thumbnail.clone()),
            icon_name: None,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("Pictures"),
                name_width_units: 8,
                size_bytes: 0,
                modified_secs: Some(42),
                metadata_complete: true,
                mime_type: None,
                mime_magic_checked: true,
                trash_original_path: None,
                trash_deletion_time: None,
                is_dir: true,
            }),
        };

        assert_eq!(visible_item_thumbnail_path(&file), Some(thumbnail));
        assert_eq!(visible_item_thumbnail_path(&dir), None);
    }

    #[test]
    fn entry_detail_label_exposes_trash_original_path_and_deletion_time() {
        let entry = model_entry(fika_core::EntryData {
            name: Arc::from("deleted.txt"),
            name_width_units: 11,
            size_bytes: 12,
            modified_secs: Some(42),
            metadata_complete: true,
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
            trash_original_path: Some(PathBuf::from("/home/user/Documents/deleted.txt")),
            trash_deletion_time: Some(Arc::from("2026-06-13T12:30:00")),
            is_dir: false,
        });

        assert_eq!(
            format_entry_detail_label(&entry),
            "Original: /home/user/Documents - Deleted: 2026-06-13 12:30"
        );
        assert_eq!(format_entry_kind_label(&entry), "2026-06-13 12:30");
    }

    #[test]
    fn incomplete_file_metadata_does_not_render_fake_zero_size() {
        let entry = model_entry(fika_core::EntryData {
            name: Arc::from("payload"),
            name_width_units: 7,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: false,
            mime_type: Some(Arc::from("application/octet-stream")),
            mime_magic_checked: false,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        });

        assert_eq!(format_entry_kind_label(&entry), "-");
        assert_eq!(format_entry_detail_label(&entry), "-");
    }

    #[test]
    fn pending_metadata_with_preserved_size_keeps_rendering_last_known_size() {
        let mut entry = model_entry(fika_core::EntryData {
            name: Arc::from("payload"),
            name_width_units: 7,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: false,
            mime_type: Some(Arc::from("application/octet-stream")),
            mime_magic_checked: false,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        });
        entry.metadata_role = Some(fika_core::EntryMetadataRole {
            size_bytes: 1536,
            modified_secs: Some(42),
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
        });
        entry.metadata_refresh_pending = true;

        assert_eq!(format_entry_kind_label(&entry), "1.5 KB");
        assert_eq!(format_entry_detail_label(&entry), "1.5 KB");
    }

    #[test]
    fn layout_index_range_and_count_uses_visible_indexes_without_collecting_layouts() {
        assert_eq!(
            layout_index_range_and_count([12, 10, 11]),
            Some((10..13, 3))
        );
        assert_eq!(
            layout_index_range_and_count(std::iter::empty::<usize>()),
            None
        );
    }

    #[test]
    fn deferred_thumbnail_candidates_stream_from_model_read_ahead() {
        let directory = PathBuf::from("/tmp/fika-deferred-thumbnail-candidates");
        let entries = Arc::new(vec![
            test_entry("a-visible.jpg", Some("image/jpeg"), true, Some(10)),
            test_entry("b-candidate.png", Some("image/png"), true, Some(20)),
            test_entry(
                "c-needs-magic.bin",
                Some("application/octet-stream"),
                false,
                Some(30),
            ),
            test_entry("d-no-mtime.jpg", Some("image/jpeg"), true, None),
        ]);
        let mut model = DirectoryModel::for_directory(directory.clone());
        model.replace_listing(directory.clone(), entries);
        let visible_entry = model.get(0).unwrap();
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(4, fika_core::IconsLayoutOptions::default()),
            items: vec![RawVisibleItemSnapshot {
                slot_id: 0,
                layout: test_layout(0),
                item_id: visible_entry.id,
                path: model.path_for_index(0).unwrap(),
                is_dir: visible_entry.is_dir,
                name: visible_entry.name.clone(),
                detail_label: String::new(),
                thumbnail_path: None,
                modified_secs: visible_entry.effective_modified_secs(),
                size_bytes: visible_entry.effective_size_bytes(),
                metadata_complete: visible_entry.effective_metadata_complete(),
                metadata_refresh_pending: visible_entry.metadata_refresh_pending,
                mime_type: visible_entry.effective_mime_type_cloned(),
                mime_magic_checked: visible_entry.effective_mime_magic_checked(),
                icon_name: None,
                selected: false,
                drop_target: None,
                draft_name: None,
                draft_caret: None,
                draft_selection: None,
                draft_error: None,
                draft_warning: None,
            }],
        };

        let candidates =
            deferred_thumbnail_candidates_for_model(&raw_file_grid, &model, None, model.len())
                .collect::<Vec<_>>();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, directory.join("b-candidate.png"));
        assert_eq!(candidates[0].modified_secs, 20);
        assert_eq!(candidates[0].priority, ThumbnailRequestPriority::Deferred);
    }

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
        let snapshot = raw_file_grid.into_file_grid_snapshot(2, |request| {
            requests.push((
                request.item_id,
                request.path.to_path_buf(),
                request.icon_size,
            ));
            icon.clone()
        });

        let FileGridSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons snapshot");
        };
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.slot_id != 0));
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].0, ItemId(1));
        assert_eq!(requests[0].1, PathBuf::from("/tmp/alpha.txt"));
    }

    #[test]
    fn raw_file_grid_snapshot_queues_incomplete_refresh_and_magic_metadata() {
        let mut complete = test_raw_visible_item(1, "complete.txt", 0);
        complete.metadata_complete = true;
        complete.icon_name = Some(Arc::from("text-plain"));
        let mut missing_icon = test_raw_visible_item(2, "missing-icon.txt", 1);
        missing_icon.metadata_complete = true;
        missing_icon.icon_name = None;
        let mut incomplete = test_raw_visible_item(3, "incomplete.txt", 2);
        incomplete.metadata_complete = false;
        let mut refresh_pending = test_raw_visible_item(4, "refresh-pending.txt", 3);
        refresh_pending.metadata_complete = true;
        refresh_pending.metadata_refresh_pending = true;
        refresh_pending.icon_name = Some(Arc::from("text-plain"));
        let mut generic_unchecked = test_raw_visible_item(5, "payload", 4);
        generic_unchecked.metadata_complete = true;
        generic_unchecked.size_bytes = 12;
        generic_unchecked.mime_type = Some(Arc::from("application/octet-stream"));
        generic_unchecked.mime_magic_checked = false;
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(5, fika_core::IconsLayoutOptions::default()),
            items: vec![
                complete,
                missing_icon,
                incomplete,
                refresh_pending,
                generic_unchecked,
            ],
        };
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(raw_file_grid.queue_metadata_role_candidates(
            &mut scheduler,
            PaneId(1),
            Generation(1)
        ));
        let batch = scheduler.start_role_batch(8).unwrap();

        assert_eq!(batch.requests.len(), 3);
        assert_eq!(batch.requests[0].item_id(), ItemId(3));
        assert_eq!(batch.requests[0].path(), Path::new("/tmp/incomplete.txt"));
        assert_eq!(batch.requests[1].item_id(), ItemId(4));
        assert_eq!(
            batch.requests[1].path(),
            Path::new("/tmp/refresh-pending.txt")
        );
        assert_eq!(batch.requests[2].item_id(), ItemId(5));
        assert_eq!(batch.requests[2].path(), Path::new("/tmp/payload"));
    }

    #[test]
    fn raw_file_grid_snapshot_does_not_queue_directory_metadata_role() {
        let mut directory = test_raw_visible_item(1, "Documents", 0);
        directory.is_dir = true;
        directory.metadata_complete = false;
        directory.metadata_refresh_pending = true;
        directory.mime_type = Some(Arc::from("inode/directory"));
        directory.mime_magic_checked = true;
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![directory],
        };
        let mut scheduler = MetadataRoleScheduler::default();

        assert!(!raw_file_grid.queue_metadata_role_candidates(
            &mut scheduler,
            PaneId(1),
            Generation(1)
        ));
        assert!(scheduler.start_role_batch(8).is_none());
    }

    #[test]
    fn thumbnail_candidates_skip_plain_text_without_preview_support() {
        let mime_type = Arc::from("text/plain");

        assert_eq!(
            visible_thumbnail_candidate(
                ItemId(1),
                Path::new("/tmp/notes.txt"),
                false,
                None,
                Some(42),
                12,
                true,
                false,
                Some(&mime_type),
                true,
            ),
            None
        );
    }

    #[test]
    fn thumbnail_candidates_include_images() {
        let mime_type = Arc::from("image/png");

        let candidate = visible_thumbnail_candidate(
            ItemId(1),
            Path::new("/tmp/photo.png"),
            false,
            None,
            Some(42),
            12,
            true,
            false,
            Some(&mime_type),
            true,
        )
        .unwrap();

        assert_eq!(candidate.path, PathBuf::from("/tmp/photo.png"));
        assert_eq!(candidate.mime_type.as_deref(), Some("image/png"));
        assert_eq!(candidate.priority, ThumbnailRequestPriority::Visible);
    }

    fn test_entry(
        name: &str,
        mime_type: Option<&str>,
        mime_magic_checked: bool,
        modified_secs: Option<u64>,
    ) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.chars().count() as u16,
            size_bytes: 12,
            modified_secs,
            metadata_complete: true,
            mime_type: mime_type.map(Arc::from),
            mime_magic_checked,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn test_raw_visible_item(id: u64, name: &str, model_index: usize) -> RawVisibleItemSnapshot {
        RawVisibleItemSnapshot {
            slot_id: 0,
            layout: test_layout(model_index),
            item_id: ItemId(id),
            path: PathBuf::from("/tmp").join(name),
            is_dir: false,
            name: Arc::from(name),
            detail_label: String::new(),
            thumbnail_path: None,
            modified_secs: Some(42),
            size_bytes: 12,
            metadata_complete: true,
            metadata_refresh_pending: false,
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
            icon_name: Some(Arc::from("text-plain")),
            selected: false,
            drop_target: None,
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
            render_image: None,
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
