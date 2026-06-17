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
use super::layout::CompactColumnWidthCache;
#[cfg(test)]
use super::layout::required_text_width_for_entry;
#[cfg(test)]
use crate::ui::drag_drop::ItemDropTarget;
pub(crate) use builder::raw_file_grid_snapshot;
#[cfg(test)]
use fika_core::ViewMode;
#[cfg(test)]
use fika_core::{DirectoryModel, PaneId, SelectionState, ViewState};
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
}
