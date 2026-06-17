mod builder;
mod metadata;
mod range;
mod render;
mod scheduler;
mod slots;
mod thumbnail;
mod types;
mod visible;

#[cfg(test)]
use super::FileGridSnapshot;
#[cfg(test)]
use super::VisibleItemSlotPool;
#[cfg(test)]
use super::layout::CompactColumnWidthCache;
#[cfg(test)]
use super::layout::icon_name_display_lines;
#[cfg(test)]
use super::layout::required_text_width_for_entry;
#[cfg(test)]
use crate::ui::drag_drop::ItemDropTarget;
#[cfg(test)]
use crate::ui::icons::FileIconSnapshot;
pub(crate) use builder::raw_file_grid_snapshot;
#[cfg(test)]
use fika_core::ViewMode;
#[cfg(test)]
use fika_core::{
    DirectoryModel, IconsLayout, ItemId, ItemLayout, PaneId, SelectionState, ViewState,
};
#[cfg(test)]
use gpui::SharedString;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
pub(crate) use thumbnail::{deferred_thumbnail_candidates_for_model, visible_item_thumbnail_path};
pub(crate) use types::{
    FileGridIconRequest, RawDetailsItemSnapshot, RawFileGridSnapshot, RawFileGridSnapshotInput,
    RawVisibleItemSnapshot,
};
pub(crate) use visible::{VisibleItemSnapshot, VisibleItemSnapshotCache};
#[cfg(test)]
use visible::{icon_name_layout_width, icon_name_max_lines};

#[cfg(test)]
mod tests {
    use super::*;

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

        let snapshot = raw_file_grid.into_file_grid_snapshot(1, &mut cache, |request| {
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
        let _first = first_raw.into_file_grid_snapshot(1, &mut cache, |_| icon.clone());

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

        let snapshot = second_raw.into_file_grid_snapshot(1, &mut cache, |request| {
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
