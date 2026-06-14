use std::path::PathBuf;

use fika_core::{CompactLayout, FilteredModel, IconsLayout, ItemLayout, ViewPoint, ViewRect};

use super::details::{DETAILS_HEADER_HEIGHT, DETAILS_ROW_HEIGHT};
use super::layout::model_index_for_layout_index;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContentItemHit {
    pub(crate) model_index: usize,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PaneLayoutProjection {
    pub(crate) layout: PaneLayout,
    pub(crate) filtered: Option<FilteredModel>,
}

impl PaneLayoutProjection {
    pub(crate) fn new(layout: PaneLayout, filtered: Option<FilteredModel>) -> Self {
        Self { layout, filtered }
    }

    pub(crate) fn model_index_for_layout_index(&self, layout_index: usize) -> Option<usize> {
        model_index_for_layout_index(self.filtered.as_ref(), layout_index)
    }

    pub(crate) fn layout_index_for_model_index(&self, model_index: usize) -> Option<usize> {
        self.filtered
            .as_ref()
            .map_or(Some(model_index), |filtered| {
                filtered.layout_index_for_model_index(model_index)
            })
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PaneLayout {
    Compact(CompactLayout),
    Icons(IconsLayout),
    Details {
        row_count: usize,
        content_width: f32,
    },
}

impl PaneLayout {
    pub(crate) fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        match self {
            PaneLayout::Compact(layout) => layout.hit_test_content_point(point),
            PaneLayout::Icons(layout) => layout.hit_test_content_point(point),
            PaneLayout::Details { row_count, .. } => details_row_index_for_point(point, *row_count),
        }
    }

    pub(crate) fn item_with_required_text_width(
        &self,
        layout_index: usize,
        required_text_width: Option<f32>,
    ) -> Option<ItemLayout> {
        match self {
            PaneLayout::Compact(layout) => {
                layout.item_with_required_text_width(layout_index, required_text_width)
            }
            PaneLayout::Icons(layout) => {
                layout.item_with_required_text_width(layout_index, required_text_width)
            }
            PaneLayout::Details {
                row_count,
                content_width,
            } => details_item_layout(layout_index, *row_count, *content_width),
        }
    }

    pub(crate) fn indexes_intersecting(&self, rect: ViewRect) -> Vec<usize> {
        match self {
            PaneLayout::Compact(layout) => layout.indexes_intersecting(rect).indexes().to_vec(),
            PaneLayout::Icons(layout) => layout.indexes_intersecting(rect).indexes().to_vec(),
            PaneLayout::Details {
                row_count,
                content_width,
            } => details_indexes_intersecting(rect, *row_count, *content_width),
        }
    }
}

fn details_row_index_for_point(point: ViewPoint, row_count: usize) -> Option<usize> {
    if point.y < DETAILS_HEADER_HEIGHT {
        return None;
    }
    let row = ((point.y - DETAILS_HEADER_HEIGHT) / DETAILS_ROW_HEIGHT).floor();
    if row < 0.0 {
        return None;
    }
    let row = row as usize;
    (row < row_count).then_some(row)
}

fn details_item_layout(
    row_index: usize,
    row_count: usize,
    content_width: f32,
) -> Option<ItemLayout> {
    if row_index >= row_count {
        return None;
    }
    let y = DETAILS_HEADER_HEIGHT + row_index as f32 * DETAILS_ROW_HEIGHT;
    let item_rect = ViewRect {
        x: 0.0,
        y,
        width: content_width,
        height: DETAILS_ROW_HEIGHT,
    };
    Some(ItemLayout {
        model_index: row_index,
        column: 0,
        row: row_index,
        item_rect,
        visual_rect: item_rect,
        icon_rect: ViewRect {
            x: 8.0,
            y: y + 5.0,
            width: 18.0,
            height: 18.0,
        },
        text_rect: ViewRect {
            x: 34.0,
            y,
            width: (content_width - 34.0).max(0.0),
            height: DETAILS_ROW_HEIGHT,
        },
    })
}

fn details_indexes_intersecting(
    rect: ViewRect,
    row_count: usize,
    content_width: f32,
) -> Vec<usize> {
    if row_count == 0 || rect.right() <= 0.0 || rect.x >= content_width {
        return Vec::new();
    }
    let start = ((rect.y - DETAILS_HEADER_HEIGHT) / DETAILS_ROW_HEIGHT).floor() as isize;
    let end = ((rect.bottom() - DETAILS_HEADER_HEIGHT) / DETAILS_ROW_HEIGHT).ceil() as isize;
    (start.max(0) as usize..(end.max(0) as usize).min(row_count)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fika_core::{CompactLayoutOptions, DirectoryModel, Entry, EntryData, NameFilter};
    use std::sync::Arc;

    #[test]
    fn projection_maps_filtered_layout_indexes_to_model_indexes() {
        let entries = Arc::new(vec![
            test_entry("alpha.txt"),
            test_entry("beta.txt"),
            test_entry("gamma.txt"),
        ]);
        let mut model = DirectoryModel::for_directory("/tmp/fika-projection".into());
        model.replace_listing("/tmp/fika-projection".into(), entries);
        let filtered = FilteredModel::from_model(&model, &NameFilter::glob("beta.txt"));
        let projection = PaneLayoutProjection::new(
            PaneLayout::Compact(CompactLayout::new(1, CompactLayoutOptions::default())),
            Some(filtered),
        );

        assert_eq!(projection.model_index_for_layout_index(0), Some(1));
        assert_eq!(projection.layout_index_for_model_index(1), Some(0));
        assert_eq!(projection.layout_index_for_model_index(0), None);
    }

    #[test]
    fn pane_layout_projection_uses_icons_row_major_hit_testing() {
        let layout = IconsLayout::new(
            6,
            fika_core::IconsLayoutOptions {
                viewport_width: 230.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..fika_core::IconsLayoutOptions::default()
            },
        );
        let projection = PaneLayoutProjection::new(PaneLayout::Icons(layout), None);

        assert_eq!(
            projection
                .layout
                .hit_test_content_point(ViewPoint { x: 118.0, y: 8.0 }),
            Some(1)
        );
        assert_eq!(
            projection
                .layout
                .hit_test_content_point(ViewPoint { x: 8.0, y: 98.0 }),
            Some(2)
        );
    }

    #[test]
    fn pane_layout_projection_uses_details_rows_below_header() {
        let projection = PaneLayoutProjection::new(
            PaneLayout::Details {
                row_count: 3,
                content_width: 480.0,
            },
            None,
        );

        assert_eq!(
            projection
                .layout
                .hit_test_content_point(ViewPoint { x: 24.0, y: 12.0 }),
            None
        );
        assert_eq!(
            projection
                .layout
                .hit_test_content_point(ViewPoint { x: 24.0, y: 40.0 }),
            Some(0)
        );
        assert_eq!(
            projection.layout.indexes_intersecting(ViewRect {
                x: 0.0,
                y: 56.0,
                width: 480.0,
                height: 40.0,
            }),
            vec![1, 2]
        );
    }

    fn test_entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }
}
