use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod builder;
mod metadata;
mod thumbnail;
mod visible;

use super::details::{DetailsItemSnapshot, DetailsLayoutMetrics};
use super::layout::CompactColumnWidthCache;
use super::{FileGridSnapshot, VisibleItemSlotPool};
use crate::ui::drag_drop::ItemDropTarget;
use crate::ui::icons::FileIconSnapshot;
use crate::ui::rename::RenameDraft;

use fika_core::{
    CompactLayout, DirectoryModel, FilteredModel, Generation, IconsLayout, ItemId, ItemLayout,
    MetadataRoleScheduler, PaneId, SelectionState, ThumbnailCandidate, ThumbnailScheduler,
    ViewState,
};

#[cfg(test)]
use super::details::{details_layout_metrics, details_name_column_width};
#[cfg(test)]
use super::layout::icon_name_display_lines;
#[cfg(test)]
use super::layout::required_text_width_for_entry;
pub(crate) use builder::raw_file_grid_snapshot;
#[cfg(test)]
use fika_core::ThumbnailRequestPriority;
#[cfg(test)]
use fika_core::ViewMode;
#[cfg(test)]
use gpui::SharedString;
#[cfg(test)]
use thumbnail::visible_thumbnail_candidate;
pub(crate) use thumbnail::{deferred_thumbnail_candidates_for_model, visible_item_thumbnail_path};
pub(crate) use visible::{VisibleItemSnapshot, VisibleItemSnapshotCache};
#[cfg(test)]
use visible::{icon_name_layout_width, icon_name_max_lines};

#[derive(Clone, Debug)]
pub(crate) struct RawVisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) layout: ItemLayout,
    pub(crate) item_id: ItemId,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) thumbnail_path: Option<PathBuf>,
    pub(crate) thumbnail_failed: bool,
    pub(crate) modified_secs: Option<u64>,
    pub(crate) size_bytes: u64,
    pub(crate) metadata_complete: bool,
    pub(crate) metadata_refresh_pending: bool,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) selected: bool,
    pub(crate) drop_target: bool,
    pub(crate) draft_name: Option<String>,
    pub(crate) draft_caret: Option<usize>,
    pub(crate) draft_selection: Option<(usize, usize)>,
    pub(crate) draft_error: Option<String>,
    pub(crate) draft_warning: Option<String>,
}

impl RawVisibleItemSnapshot {
    fn thumbnail_candidate(&self) -> Option<ThumbnailCandidate> {
        thumbnail::visible_thumbnail_candidate(
            self.item_id,
            &self.path,
            self.is_dir,
            self.thumbnail_path.as_ref(),
            self.thumbnail_failed,
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
    pub(crate) modified_secs: Option<u64>,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) selected: bool,
    pub(crate) drop_target: bool,
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
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    },
}

pub(crate) struct FileGridIconRequest<'a> {
    pub(crate) path: &'a Path,
    pub(crate) is_dir: bool,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
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

impl RawFileGridSnapshot {
    pub(crate) fn visible_metadata_role_candidates(&self) -> Vec<fika_core::MetadataRoleCandidate> {
        metadata::visible_metadata_role_candidates(self)
    }

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

    pub(crate) fn visible_layout_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                raw_visible_layout_range_and_count(items)
            }
            Self::Details { .. } => None,
        }
    }

    pub(crate) fn visible_work_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                raw_visible_layout_range_and_count(items)
            }
            Self::Details { items, .. } => {
                layout_index_range_and_count(items.iter().map(|item| item.row_index))
            }
        }
    }

    pub(crate) fn queue_metadata_role_candidates(
        &self,
        scheduler: &mut MetadataRoleScheduler,
        pane_id: PaneId,
        generation: Generation,
    ) -> bool {
        scheduler.queue_candidates(pane_id, generation, self.visible_metadata_role_candidates())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_item_thumbnail_path_uses_file_cache_hit_only() {
        let thumbnail = PathBuf::from("/tmp/fika-thumbnail-cache/normal/hash.png");
        let file = fika_core::ModelEntry {
            id: fika_core::ItemId(1),
            metadata_role: None,
            metadata_refresh_pending: false,
            thumbnail_path: Some(thumbnail.clone()),
            thumbnail_failed: false,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("photo.jpg"),
                name_width_units: 9,
                target_path: None,
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
            thumbnail_failed: false,
            entry: fika_core::Entry::new(fika_core::EntryData {
                name: Arc::from("Pictures"),
                name_width_units: 8,
                target_path: None,
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
                thumbnail_path: None,
                thumbnail_failed: false,
                modified_secs: visible_entry.effective_modified_secs(),
                size_bytes: visible_entry.effective_size_bytes(),
                metadata_complete: visible_entry.effective_metadata_complete(),
                metadata_refresh_pending: visible_entry.metadata_refresh_pending,
                mime_type: visible_entry.effective_mime_type_cloned(),
                mime_magic_checked: visible_entry.effective_mime_magic_checked(),
                selected: false,
                drop_target: false,
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
    fn raw_file_grid_snapshot_marks_directory_drop_target_visible_item_in_all_modes() {
        let directory = PathBuf::from("/tmp/fika-directory-drop-target-projection");
        let entries = Arc::new(vec![
            test_directory_entry("target"),
            test_entry("source.txt", Some("text/plain"), true, Some(10)),
        ]);
        let mut model = DirectoryModel::for_directory(directory.clone());
        model.replace_listing(directory.clone(), entries);
        let pane_id = PaneId(3);
        let target_path = directory.join("target");
        let target = ItemDropTarget::Directory {
            pane_id,
            path: target_path.clone(),
        };

        for view_mode in [ViewMode::Icons, ViewMode::Compact, ViewMode::Details] {
            let mut compact_column_widths = CompactColumnWidthCache::default();
            let snapshot = raw_file_grid_snapshot(RawFileGridSnapshotInput {
                pane_id,
                model: &model,
                selection: &SelectionState::default(),
                view: &ViewState {
                    view_mode,
                    ..ViewState::default()
                },
                filtered: None,
                source_revision: 0,
                rename_draft: None,
                item_drop_target: Some(&target),
                compact_column_widths: &mut compact_column_widths,
            });

            match snapshot {
                RawFileGridSnapshot::Icons { items, .. }
                | RawFileGridSnapshot::Compact { items, .. } => {
                    let target_item = items
                        .iter()
                        .find(|item| item.path == target_path)
                        .expect("target directory item should be visible");
                    let source_item = items
                        .iter()
                        .find(|item| item.path == directory.join("source.txt"))
                        .expect("source file item should be visible");
                    assert!(target_item.drop_target, "{view_mode:?}");
                    assert!(!source_item.drop_target, "{view_mode:?}");
                }
                RawFileGridSnapshot::Details { items, .. } => {
                    let target_item = items
                        .iter()
                        .find(|item| item.path == target_path)
                        .expect("target directory row should be visible");
                    let source_item = items
                        .iter()
                        .find(|item| item.path == directory.join("source.txt"))
                        .expect("source file row should be visible");
                    assert!(target_item.drop_target, "{view_mode:?}");
                    assert!(!source_item.drop_target, "{view_mode:?}");
                }
            }
        }
    }

    #[test]
    fn raw_icon_snapshot_uses_full_text_width_for_names() {
        let directory = PathBuf::from("/tmp/fika-icon-full-text-width");
        let entries = Arc::new(vec![test_entry("i", Some("text/plain"), true, Some(10))]);
        let mut model = DirectoryModel::for_directory(directory.clone());
        model.replace_listing(directory, entries);
        let view = ViewState {
            view_mode: ViewMode::Icons,
            ..ViewState::default()
        };
        let mut compact_column_widths = CompactColumnWidthCache::default();

        let snapshot = raw_file_grid_snapshot(RawFileGridSnapshotInput {
            pane_id: PaneId(3),
            model: &model,
            selection: &SelectionState::default(),
            view: &view,
            filtered: None,
            source_revision: 0,
            rename_draft: None,
            item_drop_target: None,
            compact_column_widths: &mut compact_column_widths,
        });

        let RawFileGridSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icon snapshot");
        };
        let item = items.first().expect("icon item should be visible");
        let options = crate::ui::file_grid::icons_layout_options(&view, 0.0);
        let estimated_text_width =
            required_text_width_for_entry(model.get(0).unwrap(), None) + options.padding * 2.0;

        assert_eq!(
            item.layout.text_rect.width,
            options.item_width - options.padding * 2.0
        );
        assert!(item.layout.text_rect.width > estimated_text_width);
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
        let mut cache = VisibleItemSnapshotCache::default();
        let snapshot = raw_file_grid.into_file_grid_snapshot(2, &mut cache, |request| {
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

        let snapshot = raw_file_grid.into_file_grid_snapshot(1, &mut cache, |_| icon.clone());

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
    fn icon_item_snapshot_cache_reuses_content_across_layout_only_resize() {
        let mut slots = VisibleItemSlotPool::default();
        let mut cache = VisibleItemSnapshotCache::default();
        let icon = test_icon_snapshot();
        let mut icon_requests = 0;

        let mut first_raw = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![test_raw_visible_item(1, "alpha.txt", 0)],
        };
        first_raw.assign_visible_item_slots(&mut slots);
        let first = first_raw.into_file_grid_snapshot(1, &mut cache, |_| {
            icon_requests += 1;
            icon.clone()
        });
        let FileGridSnapshot::Icons { items: first, .. } = first else {
            panic!("expected icons snapshot");
        };

        let mut second_item = test_raw_visible_item(1, "alpha.txt", 0);
        second_item.layout.item_rect.x = 24.0;
        second_item.layout.visual_rect.x = 24.0;
        second_item.layout.icon_rect.x = 24.0;
        second_item.layout.text_rect.x = 24.0;
        let mut second_raw = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![second_item],
        };
        second_raw.assign_visible_item_slots(&mut slots);
        let second = second_raw.into_file_grid_snapshot(1, &mut cache, |_| {
            icon_requests += 1;
            icon.clone()
        });
        let FileGridSnapshot::Icons { items: second, .. } = second else {
            panic!("expected icons snapshot");
        };

        assert_eq!(icon_requests, 1);
        assert_eq!(first[0].icon_name_lines, second[0].icon_name_lines);
        assert_eq!(second[0].layout.item_rect.x, 24.0);
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

    #[test]
    fn raw_file_grid_snapshot_queues_only_generic_magic_metadata() {
        let mut complete = test_raw_visible_item(1, "complete.txt", 0);
        complete.metadata_complete = true;
        let mut missing_icon = test_raw_visible_item(2, "missing-icon.txt", 1);
        missing_icon.metadata_complete = true;
        let mut incomplete = test_raw_visible_item(3, "incomplete.txt", 2);
        incomplete.metadata_complete = false;
        let mut refresh_pending = test_raw_visible_item(4, "refresh-pending.txt", 3);
        refresh_pending.metadata_complete = true;
        refresh_pending.metadata_refresh_pending = true;
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

        assert_eq!(batch.requests.len(), 1);
        assert_eq!(batch.requests[0].item_id(), ItemId(5));
        assert_eq!(batch.requests[0].path(), Path::new("/tmp/payload"));
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
    fn raw_file_grid_snapshot_does_not_queue_network_metadata_role() {
        let mut remote = test_raw_visible_item(1, "payload", 0);
        remote.path = PathBuf::from("smb://server/share/payload");
        remote.metadata_complete = true;
        remote.size_bytes = 12;
        remote.mime_type = Some(Arc::from("application/octet-stream"));
        remote.mime_magic_checked = false;
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(1, fika_core::IconsLayoutOptions::default()),
            items: vec![remote],
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
                false,
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
    fn thumbnail_candidates_skip_network_paths() {
        let mime_type = Arc::from("image/png");

        assert_eq!(
            visible_thumbnail_candidate(
                ItemId(1),
                Path::new("smb://server/share/photo.png"),
                false,
                None,
                false,
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
            false,
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

    #[test]
    fn thumbnail_candidates_skip_failed_preview_role() {
        let mime_type = Arc::from("image/png");

        assert_eq!(
            visible_thumbnail_candidate(
                ItemId(1),
                Path::new("/tmp/photo.png"),
                false,
                None,
                true,
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

    fn test_entry(
        name: &str,
        mime_type: Option<&str>,
        mime_magic_checked: bool,
        modified_secs: Option<u64>,
    ) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.chars().count() as u16,
            target_path: None,
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

    fn test_directory_entry(name: &str) -> fika_core::Entry {
        fika_core::Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.chars().count() as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs: Some(42),
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: true,
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
