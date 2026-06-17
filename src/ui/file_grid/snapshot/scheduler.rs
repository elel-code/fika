use fika_core::{
    Generation, MetadataRoleScheduler, PaneId, ThumbnailCandidate, ThumbnailScheduler,
};

use super::{FileGridIconRequest, RawFileGridSnapshot, RawVisibleItemSnapshot, thumbnail};

impl RawFileGridSnapshot {
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
                    .filter_map(raw_visible_thumbnail_candidate)
                    .chain(deferred_candidates),
            ),
            Self::Details { .. } => {
                scheduler.queue_candidates(pane_id, generation, deferred_candidates)
            }
        }
    }

    pub(crate) fn queue_file_icon_resolve_candidates<F>(&self, mut queue: F) -> bool
    where
        F: for<'a> FnMut(FileGridIconRequest<'a>) -> bool,
    {
        let mut queued = false;
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                let visible_range = self
                    .visible_layout_range_and_count()
                    .map(|(range, _)| range);

                for item in items.iter().filter(|item| item.visible && !item.is_dir) {
                    queued |= queue(file_icon_request_for_item(item));
                }
                for item in items.iter().filter(|item| item.visible && item.is_dir) {
                    queued |= queue(file_icon_request_for_item(item));
                }

                if let Some(visible_range) = visible_range {
                    for item in items.iter().filter(|item| {
                        !item.visible && item.layout.model_index >= visible_range.end
                    }) {
                        queued |= queue(file_icon_request_for_item(item));
                    }
                    for item in items.iter().rev().filter(|item| {
                        !item.visible && item.layout.model_index < visible_range.start
                    }) {
                        queued |= queue(file_icon_request_for_item(item));
                    }
                }
            }
            Self::Details { items, metrics, .. } => {
                for item in items.iter().filter(|item| !item.is_dir) {
                    queued |= queue(FileGridIconRequest {
                        path: &item.path,
                        is_dir: item.is_dir,
                        mime_type: item.mime_type.clone(),
                        mime_magic_checked: item.mime_magic_checked,
                        icon_size: metrics.icon_size,
                    });
                }
                for item in items.iter().filter(|item| item.is_dir) {
                    queued |= queue(FileGridIconRequest {
                        path: &item.path,
                        is_dir: item.is_dir,
                        mime_type: item.mime_type.clone(),
                        mime_magic_checked: item.mime_magic_checked,
                        icon_size: metrics.icon_size,
                    });
                }
            }
        }
        queued
    }
}

fn file_icon_request_for_item(item: &RawVisibleItemSnapshot) -> FileGridIconRequest<'_> {
    FileGridIconRequest {
        path: &item.path,
        is_dir: item.is_dir,
        mime_type: item.mime_type.clone(),
        mime_magic_checked: item.mime_magic_checked,
        icon_size: item.layout.icon_rect.width,
    }
}

fn raw_visible_thumbnail_candidate(item: &RawVisibleItemSnapshot) -> Option<ThumbnailCandidate> {
    if !item.visible {
        return None;
    }
    thumbnail::visible_thumbnail_candidate(
        item.item_id,
        &item.path,
        item.is_dir,
        item.thumbnail_path.as_ref(),
        item.thumbnail_failed,
        item.modified_secs,
        item.size_bytes,
        item.metadata_complete,
        item.metadata_refresh_pending,
        item.mime_type.as_ref(),
        item.mime_magic_checked,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use fika_core::{IconsLayout, ItemId, ItemLayout};

    use super::super::RawDetailsItemSnapshot;

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
    fn visible_metadata_role_candidates_skip_read_ahead_items() {
        let mut visible = test_raw_visible_item(5, "visible.bin", 4);
        visible.metadata_complete = true;
        visible.size_bytes = 12;
        visible.mime_type = Some(Arc::from("application/octet-stream"));
        visible.mime_magic_checked = false;
        let mut read_ahead = test_raw_visible_item(6, "ahead.bin", 5);
        read_ahead.visible = false;
        read_ahead.metadata_complete = true;
        read_ahead.size_bytes = 12;
        read_ahead.mime_type = Some(Arc::from("application/octet-stream"));
        read_ahead.mime_magic_checked = false;
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(6, fika_core::IconsLayoutOptions::default()),
            items: vec![visible, read_ahead],
        };

        let candidates = raw_file_grid.visible_metadata_role_candidates();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].item_id, ItemId(5));
        assert_eq!(candidates[0].path, PathBuf::from("/tmp/visible.bin"));
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
    fn file_icon_resolve_candidates_follow_visible_and_read_ahead_order() {
        let mut before = test_raw_visible_item(1, "before.txt", 0);
        before.visible = false;
        let visible_file = test_raw_visible_item(2, "visible-file.txt", 1);
        let mut visible_dir = test_raw_visible_item(3, "visible-dir", 2);
        visible_dir.is_dir = true;
        let visible_second_file = test_raw_visible_item(4, "visible-second-file.txt", 3);
        let mut after = test_raw_visible_item(5, "after.txt", 4);
        after.visible = false;
        let raw_file_grid = RawFileGridSnapshot::Icons {
            layout: IconsLayout::new(5, fika_core::IconsLayoutOptions::default()),
            items: vec![
                before,
                visible_file,
                visible_dir,
                visible_second_file,
                after,
            ],
        };
        let mut paths = Vec::new();

        assert!(raw_file_grid.queue_file_icon_resolve_candidates(|request| {
            paths.push(
                request
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            );
            true
        }));

        assert_eq!(
            paths,
            vec![
                "visible-file.txt",
                "visible-second-file.txt",
                "visible-dir",
                "after.txt",
                "before.txt",
            ]
        );
    }

    #[test]
    fn details_file_icon_resolve_candidates_queue_files_before_directories() {
        let raw_file_grid = RawFileGridSnapshot::Details {
            items: vec![
                test_raw_details_item(0, 1, "alpha.txt", false),
                test_raw_details_item(1, 2, "Documents", true),
                test_raw_details_item(2, 3, "beta.txt", false),
            ],
            row_count: 3,
            metrics: super::super::super::details::details_layout_metrics(48.0),
            name_column_width: 260.0,
        };
        let mut paths = Vec::new();

        assert!(raw_file_grid.queue_file_icon_resolve_candidates(|request| {
            paths.push(
                request
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
            );
            true
        }));

        assert_eq!(paths, vec!["alpha.txt", "beta.txt", "Documents"]);
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

    fn test_raw_details_item(
        row_index: usize,
        id: u64,
        name: &str,
        is_dir: bool,
    ) -> RawDetailsItemSnapshot {
        RawDetailsItemSnapshot {
            row_index,
            item_id: ItemId(id),
            path: PathBuf::from("/tmp").join(name),
            is_dir,
            name: Arc::from(name),
            size_bytes: 12,
            modified_secs: Some(42),
            mime_type: Some(Arc::from(if is_dir {
                "inode/directory"
            } else {
                "text/plain"
            })),
            mime_magic_checked: true,
            selected: false,
            drop_target: false,
            size_label: "12 B".to_string(),
            modified_label: "Today".to_string(),
            original_path_label: String::new(),
            deletion_time_label: String::new(),
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
