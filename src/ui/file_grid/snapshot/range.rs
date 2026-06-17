use std::ops::Range;

use fika_core::{Generation, ViewMode};

use super::{RawFileGridSnapshot, RawVisibleItemSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PaneVisibleWorkKey {
    generation: Generation,
    view_mode: ViewMode,
    model_data_generation: u64,
    source_revision: u64,
    item_count: usize,
    visible_start: usize,
    visible_end: usize,
    visible_count: usize,
}

impl PaneVisibleWorkKey {
    pub(crate) fn new(
        generation: Generation,
        view_mode: ViewMode,
        model_data_generation: u64,
        source_revision: u64,
        item_count: usize,
        raw_file_grid: &RawFileGridSnapshot,
    ) -> Self {
        let (visible_range, visible_count) = raw_file_grid
            .visible_work_range_and_count()
            .unwrap_or((0..0, 0));
        Self {
            generation,
            view_mode,
            model_data_generation,
            source_revision,
            item_count,
            visible_start: visible_range.start,
            visible_end: visible_range.end,
            visible_count,
        }
    }
}

impl RawFileGridSnapshot {
    pub(crate) fn visible_layout_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                layout_index_range_and_count(
                    items
                        .iter()
                        .filter(|item| item.visible)
                        .map(|item| item.layout.model_index),
                )
            }
            Self::Details { .. } => None,
        }
    }

    pub(crate) fn visible_work_range_and_count(&self) -> Option<(Range<usize>, usize)> {
        match self {
            Self::Compact { items, .. } | Self::Icons { items, .. } => {
                raw_work_layout_range_and_count(items)
            }
            Self::Details { items, .. } => {
                layout_index_range_and_count(items.iter().map(|item| item.row_index))
            }
        }
    }
}

pub(super) fn layout_index_range_and_count(
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

fn raw_work_layout_range_and_count(
    items: &[RawVisibleItemSnapshot],
) -> Option<(Range<usize>, usize)> {
    layout_index_range_and_count(items.iter().map(|item| item.layout.model_index))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::sync::Arc;

    use fika_core::{DirectoryModel, Generation, PaneId, SelectionState, ViewMode, ViewState};

    use super::super::super::layout::CompactColumnWidthCache;
    use super::super::{RawFileGridSnapshot, RawFileGridSnapshotInput, raw_file_grid_snapshot};

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
    fn raw_icon_snapshot_keeps_read_ahead_work_items_out_of_visible_range() {
        let directory = PathBuf::from("/tmp/fika-icon-work-window");
        let entries = (0..80)
            .map(|index| {
                test_entry(
                    &format!("item-{index:02}.txt"),
                    Some("text/plain"),
                    true,
                    Some(index),
                )
            })
            .collect::<Vec<_>>();
        let mut model = DirectoryModel::for_directory(directory.clone());
        model.replace_listing(directory, Arc::new(entries));
        let view = ViewState {
            view_mode: ViewMode::Icons,
            viewport_width: 260.0,
            viewport_height: 180.0,
            scroll_y: 360.0,
            ..ViewState::default()
        };

        let raw_file_grid = raw_file_grid_snapshot(RawFileGridSnapshotInput {
            pane_id: PaneId(1),
            model: &model,
            selection: &SelectionState::default(),
            view: &view,
            filtered: None,
            source_revision: 0,
            rename_draft: None,
            item_drop_target: None,
            compact_column_widths: &mut CompactColumnWidthCache::default(),
        });

        let RawFileGridSnapshot::Icons { items, .. } = &raw_file_grid else {
            panic!("expected icons snapshot");
        };
        let visible_count = items.iter().filter(|item| item.visible).count();

        assert!(visible_count > 0);
        assert!(items.len() > visible_count);
        assert!(items.iter().any(|item| !item.visible));
        assert_eq!(
            raw_file_grid.visible_layout_range_and_count(),
            layout_index_range_and_count(
                items
                    .iter()
                    .filter(|item| item.visible)
                    .map(|item| item.layout.model_index)
            )
        );
        assert_eq!(
            raw_file_grid.visible_work_range_and_count(),
            layout_index_range_and_count(items.iter().map(|item| item.layout.model_index))
        );
    }

    #[test]
    fn pane_visible_work_key_tracks_raw_work_range() {
        let directory = PathBuf::from("/tmp/fika-visible-work-key");
        let entries = (0..80)
            .map(|index| {
                test_entry(
                    &format!("item-{index:02}.txt"),
                    Some("text/plain"),
                    true,
                    Some(index),
                )
            })
            .collect::<Vec<_>>();
        let mut model = DirectoryModel::for_directory(directory.clone());
        model.replace_listing(directory, Arc::new(entries));
        let mut compact_column_widths = CompactColumnWidthCache::default();

        let first_raw = raw_file_grid_snapshot(RawFileGridSnapshotInput {
            pane_id: PaneId(1),
            model: &model,
            selection: &SelectionState::default(),
            view: &ViewState {
                view_mode: ViewMode::Icons,
                viewport_width: 260.0,
                viewport_height: 180.0,
                scroll_y: 0.0,
                ..ViewState::default()
            },
            filtered: None,
            source_revision: 0,
            rename_draft: None,
            item_drop_target: None,
            compact_column_widths: &mut compact_column_widths,
        });
        let second_raw = raw_file_grid_snapshot(RawFileGridSnapshotInput {
            pane_id: PaneId(1),
            model: &model,
            selection: &SelectionState::default(),
            view: &ViewState {
                view_mode: ViewMode::Icons,
                viewport_width: 260.0,
                viewport_height: 180.0,
                scroll_y: 360.0,
                ..ViewState::default()
            },
            filtered: None,
            source_revision: 0,
            rename_draft: None,
            item_drop_target: None,
            compact_column_widths: &mut compact_column_widths,
        });

        assert_ne!(
            PaneVisibleWorkKey::new(Generation(1), ViewMode::Icons, 1, 0, 80, &first_raw),
            PaneVisibleWorkKey::new(Generation(1), ViewMode::Icons, 1, 0, 80, &second_raw)
        );
        assert_eq!(
            PaneVisibleWorkKey::new(Generation(1), ViewMode::Icons, 1, 0, 80, &second_raw),
            PaneVisibleWorkKey::new(Generation(1), ViewMode::Icons, 1, 0, 80, &second_raw)
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
}
