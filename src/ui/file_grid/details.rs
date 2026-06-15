use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use fika_core::{
    EntryData, ModelEntry, format_modified_secs, format_size, format_trash_deletion_time,
};

use crate::ui::icons::FileIconSnapshot;

pub(crate) const DETAILS_HEADER_HEIGHT: f32 = 28.0;
pub(crate) const DETAILS_ROW_HEIGHT: f32 = 28.0;
pub(crate) const DETAILS_ICON_SIZE: f32 = 18.0;
pub(crate) const DETAILS_NAME_COLUMN_MIN_WIDTH: f32 = 260.0;
pub(crate) const DETAILS_NAME_CELL_HORIZONTAL_PADDING: f32 = 16.0;
pub(crate) const DETAILS_NAME_CELL_GAP: f32 = 8.0;
const DETAILS_ICON_SCALE: f32 = DETAILS_ICON_SIZE / 48.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DetailsLayoutMetrics {
    pub(crate) header_height: f32,
    pub(crate) row_height: f32,
    pub(crate) icon_size: f32,
}

pub(crate) fn details_layout_metrics(view_icon_size: f32) -> DetailsLayoutMetrics {
    let icon_size = (view_icon_size * DETAILS_ICON_SCALE)
        .round()
        .clamp(16.0, 96.0);
    DetailsLayoutMetrics {
        header_height: DETAILS_HEADER_HEIGHT,
        row_height: DETAILS_ROW_HEIGHT.max(icon_size + 10.0),
        icon_size,
    }
}

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
    pub(crate) item_id: fika_core::ItemId,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) selected: bool,
    pub(crate) selection_count: usize,
    pub(crate) drop_target: bool,
    pub(crate) size_label: String,
    pub(crate) modified_label: String,
    pub(crate) original_path_label: String,
    pub(crate) deletion_time_label: String,
}

pub(crate) fn details_name_column_width(text_width: f32, metrics: DetailsLayoutMetrics) -> f32 {
    (DETAILS_NAME_CELL_HORIZONTAL_PADDING
        + metrics.icon_size
        + DETAILS_NAME_CELL_GAP
        + text_width.max(0.0)
        + 8.0)
        .ceil()
        .max(DETAILS_NAME_COLUMN_MIN_WIDTH)
}

pub(crate) fn details_columns(trash_view: bool, name_width: f32) -> Vec<DetailsColumn> {
    let mut columns = vec![
        DetailsColumn {
            title: "Name",
            width: name_width.max(DETAILS_NAME_COLUMN_MIN_WIDTH),
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

pub(crate) fn details_content_width(trash_view: bool, name_width: f32) -> f32 {
    details_columns(trash_view, name_width)
        .iter()
        .map(|column| column.width)
        .sum()
}

pub(crate) fn details_content_height(row_count: usize, metrics: DetailsLayoutMetrics) -> f32 {
    metrics.header_height + row_count as f32 * metrics.row_height
}

pub(crate) fn details_visible_row_range(
    row_count: usize,
    viewport_height: f32,
    scroll_y: f32,
    metrics: DetailsLayoutMetrics,
) -> Range<usize> {
    if row_count == 0 {
        return 0..0;
    }
    let visible_top = (scroll_y.max(0.0) - metrics.header_height).max(0.0);
    let visible_bottom =
        (scroll_y.max(0.0) + viewport_height.max(1.0) - metrics.header_height).max(visible_top);
    let start = (visible_top / metrics.row_height).floor() as usize;
    let end = (visible_bottom / metrics.row_height).ceil() as usize + 1;
    start.min(row_count)..end.min(row_count)
}

pub(crate) fn details_size_label(entry: &ModelEntry) -> String {
    if entry.is_dir {
        "Folder".to_string()
    } else if !entry.effective_metadata_complete()
        && entry.effective_size_bytes() == 0
        && entry.effective_modified_secs().is_none()
    {
        "-".to_string()
    } else {
        format_size(entry.effective_size_bytes())
    }
}

pub(crate) fn details_modified_label(entry: &ModelEntry) -> String {
    format_modified_secs(entry.effective_modified_secs())
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
    use std::sync::Arc;

    fn model_entry(entry: EntryData) -> ModelEntry {
        ModelEntry {
            id: fika_core::ItemId(1),
            entry: fika_core::Entry::new(entry),
            metadata_role: None,
            metadata_refresh_pending: false,
            thumbnail_path: None,
            thumbnail_failed: false,
        }
    }

    #[test]
    fn trash_details_columns_include_original_path_and_deletion_time() {
        let columns = details_columns(true, DETAILS_NAME_COLUMN_MIN_WIDTH)
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
    fn details_name_column_width_expands_for_long_names() {
        let metrics = details_layout_metrics(48.0);

        assert_eq!(
            details_name_column_width(24.0, metrics),
            DETAILS_NAME_COLUMN_MIN_WIDTH
        );
        assert!(details_name_column_width(420.0, metrics) > DETAILS_NAME_COLUMN_MIN_WIDTH);
    }

    #[test]
    fn details_visible_row_range_uses_vertical_scroll_and_viewport_height() {
        let metrics = details_layout_metrics(48.0);
        assert_eq!(details_visible_row_range(100, 56.0, 0.0, metrics), 0..2);
        assert_eq!(details_visible_row_range(100, 56.0, 28.0, metrics), 0..3);
        assert_eq!(details_visible_row_range(100, 56.0, 84.0, metrics), 2..5);
        assert_eq!(details_visible_row_range(3, 500.0, 0.0, metrics), 0..3);
        assert_eq!(details_visible_row_range(0, 500.0, 0.0, metrics), 0..0);
    }

    #[test]
    fn details_layout_metrics_scale_with_zoom_without_changing_default() {
        let default = details_layout_metrics(48.0);
        assert_eq!(default.icon_size, DETAILS_ICON_SIZE);
        assert_eq!(default.row_height, DETAILS_ROW_HEIGHT);

        let zoomed = details_layout_metrics(128.0);
        assert!(zoomed.icon_size > default.icon_size);
        assert!(zoomed.row_height > default.row_height);
    }

    #[test]
    fn incomplete_file_metadata_uses_unknown_size_and_modified_labels() {
        let entry = model_entry(EntryData {
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

        assert_eq!(details_size_label(&entry), "-");
        assert_eq!(details_modified_label(&entry), "-");
    }

    #[test]
    fn pending_metadata_keeps_last_known_size_and_modified_labels() {
        let entry = model_entry(EntryData {
            name: Arc::from("payload"),
            name_width_units: 7,
            size_bytes: 1536,
            modified_secs: Some(42),
            metadata_complete: false,
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        });

        assert_eq!(details_size_label(&entry), "1.5 KB");
        assert_eq!(details_modified_label(&entry), "1970-01-01 00:00");
    }
}
