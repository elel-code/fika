use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use fika_core::{EntryData, format_modified_secs, format_size, format_trash_deletion_time};

use crate::ui::drag_drop::FileTransferMode;
use crate::ui::icons::FileIconSnapshot;

pub(crate) const DETAILS_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const DETAILS_ROW_HEIGHT: f32 = 28.0;
pub(crate) const DETAILS_ICON_SIZE: f32 = 18.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DetailsColumnKind {
    Name,
    Size,
    Modified,
    OriginalPath,
    DeletionTime,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DetailsColumn {
    pub(crate) title: &'static str,
    pub(crate) width: f32,
    pub(crate) kind: DetailsColumnKind,
}

#[derive(Clone, Debug)]
pub(crate) struct DetailsItemSnapshot {
    pub(crate) row_index: usize,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) selected: bool,
    pub(crate) selection_count: usize,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) size_label: String,
    pub(crate) modified_label: String,
    pub(crate) original_path_label: String,
    pub(crate) deletion_time_label: String,
}

pub(crate) fn details_columns(trash_view: bool) -> Vec<DetailsColumn> {
    let mut columns = vec![
        DetailsColumn {
            title: "Name",
            width: 260.0,
            kind: DetailsColumnKind::Name,
        },
        DetailsColumn {
            title: "Size",
            width: 96.0,
            kind: DetailsColumnKind::Size,
        },
        DetailsColumn {
            title: "Modified",
            width: 152.0,
            kind: DetailsColumnKind::Modified,
        },
    ];
    if trash_view {
        columns.extend([
            DetailsColumn {
                title: "Original Path",
                width: 360.0,
                kind: DetailsColumnKind::OriginalPath,
            },
            DetailsColumn {
                title: "Deletion Time",
                width: 160.0,
                kind: DetailsColumnKind::DeletionTime,
            },
        ]);
    }
    columns
}

pub(crate) fn details_content_width(trash_view: bool) -> f32 {
    details_columns(trash_view)
        .iter()
        .map(|column| column.width)
        .sum()
}

pub(crate) fn details_content_height(row_count: usize) -> f32 {
    DETAILS_HEADER_HEIGHT + row_count as f32 * DETAILS_ROW_HEIGHT
}

pub(crate) fn details_visible_row_range(
    row_count: usize,
    viewport_height: f32,
    scroll_y: f32,
) -> Range<usize> {
    if row_count == 0 {
        return 0..0;
    }
    let visible_top = (scroll_y.max(0.0) - DETAILS_HEADER_HEIGHT).max(0.0);
    let visible_bottom =
        (scroll_y.max(0.0) + viewport_height.max(1.0) - DETAILS_HEADER_HEIGHT).max(visible_top);
    let start = (visible_top / DETAILS_ROW_HEIGHT).floor() as usize;
    let end = (visible_bottom / DETAILS_ROW_HEIGHT).ceil() as usize + 1;
    start.min(row_count)..end.min(row_count)
}

pub(crate) fn details_size_label(entry: &EntryData) -> String {
    if entry.is_dir {
        "Folder".to_string()
    } else {
        format_size(entry.size_bytes)
    }
}

pub(crate) fn details_modified_label(entry: &EntryData) -> String {
    format_modified_secs(entry.modified_secs)
}

pub(crate) fn details_original_path_label(entry: &EntryData) -> String {
    entry
        .trash_original_path
        .as_deref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "-".to_string())
}

pub(crate) fn details_deletion_time_label(entry: &EntryData) -> String {
    entry
        .trash_deletion_time
        .as_deref()
        .map(format_trash_deletion_time)
        .unwrap_or_else(|| "-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_details_columns_include_original_path_and_deletion_time() {
        let columns = details_columns(true)
            .into_iter()
            .map(|column| (column.title, column.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                ("Name", DetailsColumnKind::Name),
                ("Size", DetailsColumnKind::Size),
                ("Modified", DetailsColumnKind::Modified),
                ("Original Path", DetailsColumnKind::OriginalPath),
                ("Deletion Time", DetailsColumnKind::DeletionTime),
            ]
        );
    }

    #[test]
    fn details_visible_row_range_uses_vertical_scroll_and_viewport_height() {
        assert_eq!(details_visible_row_range(100, 56.0, 0.0), 0..2);
        assert_eq!(details_visible_row_range(100, 56.0, 28.0), 0..3);
        assert_eq!(details_visible_row_range(100, 56.0, 84.0), 2..5);
        assert_eq!(details_visible_row_range(3, 500.0, 0.0), 0..3);
        assert_eq!(details_visible_row_range(0, 500.0, 0.0), 0..0);
    }
}
