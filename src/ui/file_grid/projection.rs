use std::path::PathBuf;

use fika_core::{
    CompactLayout, DirectoryModel, FilteredModel, IconsLayout, ItemLayout, ViewMode, ViewPoint,
    ViewRect, ViewState,
};

use crate::ui::rename::RenameDraft;

use super::details::{DETAILS_HEADER_HEIGHT, DETAILS_ROW_HEIGHT, details_content_width};
use super::icons_layout_options;
use super::layout::{
    CompactColumnWidthCache, compact_layout_for_filtered_model_with_text_override,
    compact_layout_for_model_with_text_override, model_index_for_layout_index,
};
use super::snapshot::{rename_text_override_for_model, required_text_width_for_entry};

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

pub(crate) struct PaneLayoutProjectionInput<'a> {
    pub(crate) model: &'a DirectoryModel,
    pub(crate) view: &'a ViewState,
    pub(crate) filtered: Option<&'a FilteredModel>,
    pub(crate) source_revision: u64,
    pub(crate) rename_draft: Option<&'a RenameDraft>,
    pub(crate) trash_view: bool,
    pub(crate) compact_column_widths: &'a mut CompactColumnWidthCache,
}

pub(crate) fn pane_layout_projection(input: PaneLayoutProjectionInput<'_>) -> PaneLayoutProjection {
    let PaneLayoutProjectionInput {
        model,
        view,
        filtered,
        source_revision,
        rename_draft,
        trash_view,
        compact_column_widths,
    } = input;
    let item_count = filtered.map_or_else(|| model.len(), FilteredModel::len);
    let layout = match view.view_mode {
        ViewMode::Icons => PaneLayout::Icons(IconsLayout::new(
            item_count,
            icons_layout_options(view, 0.0),
        )),
        ViewMode::Compact => {
            let rename_text_override = rename_text_override_for_model(model, rename_draft);
            PaneLayout::Compact(match filtered {
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
            })
        }
        ViewMode::Details => PaneLayout::Details {
            row_count: item_count,
            content_width: details_content_width(trash_view).max(1.0),
        },
    };
    PaneLayoutProjection::new(layout, filtered.cloned())
}

pub(crate) fn content_item_hit_at_point(
    projection: &PaneLayoutProjection,
    model: &DirectoryModel,
    rename_draft: Option<&RenameDraft>,
    point: ViewPoint,
) -> Option<ContentItemHit> {
    let layout_index = projection.layout.hit_test_content_point(point)?;
    let model_index = projection.model_index_for_layout_index(layout_index)?;
    let entry = model.get(model_index)?;
    let path = model.path_for_index(model_index)?;
    let active_rename_draft = active_rename_draft_for_path(rename_draft, &path);
    let item_layout = projection.layout.item_with_required_text_width(
        layout_index,
        Some(required_text_width_for_entry(entry, active_rename_draft)),
    )?;
    if !item_layout.visual_rect.contains(point) {
        return None;
    }
    Some(ContentItemHit {
        model_index,
        path,
        is_dir: entry.is_dir,
    })
}

pub(crate) fn model_indexes_intersecting_visual_rect(
    projection: &PaneLayoutProjection,
    model: &DirectoryModel,
    rename_draft: Option<&RenameDraft>,
    rect: ViewRect,
) -> Vec<usize> {
    projection
        .layout
        .indexes_intersecting(rect)
        .into_iter()
        .filter_map(|layout_index| {
            let model_index = projection.model_index_for_layout_index(layout_index)?;
            let entry = model.get(model_index)?;
            let path = model.path_for_index(model_index)?;
            let active_rename_draft = active_rename_draft_for_path(rename_draft, &path);
            projection
                .layout
                .item_with_required_text_width(
                    layout_index,
                    Some(required_text_width_for_entry(entry, active_rename_draft)),
                )
                .is_some_and(|item| item.visual_rect.intersects(rect))
                .then_some(model_index)
        })
        .collect()
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

fn active_rename_draft_for_path<'a>(
    rename_draft: Option<&'a RenameDraft>,
    path: &std::path::Path,
) -> Option<&'a RenameDraft> {
    rename_draft.filter(|draft| draft.original_path == path)
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
    use fika_core::{
        CompactLayoutOptions, DirectoryModel, Entry, EntryData, NameFilter, ViewMode, ViewState,
    };
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

    #[test]
    fn pane_layout_projection_builder_keeps_filtered_mapping_and_details_width() {
        let entries = Arc::new(vec![
            test_entry("alpha.txt"),
            test_entry("beta.txt"),
            test_entry("gamma.txt"),
        ]);
        let mut model = DirectoryModel::for_directory("/tmp/fika-projection-builder".into());
        model.replace_listing("/tmp/fika-projection-builder".into(), entries);
        let filtered = FilteredModel::from_model(&model, &NameFilter::glob("beta.txt"));
        let view = ViewState {
            view_mode: ViewMode::Details,
            ..ViewState::default()
        };
        let mut compact_column_widths = CompactColumnWidthCache::default();

        let projection = pane_layout_projection(PaneLayoutProjectionInput {
            model: &model,
            view: &view,
            filtered: Some(&filtered),
            source_revision: 7,
            rename_draft: None,
            trash_view: true,
            compact_column_widths: &mut compact_column_widths,
        });

        assert_eq!(projection.model_index_for_layout_index(0), Some(1));
        assert_eq!(projection.layout_index_for_model_index(1), Some(0));
        let PaneLayout::Details {
            row_count,
            content_width,
        } = projection.layout
        else {
            panic!("expected details projection");
        };
        assert_eq!(row_count, 1);
        assert_eq!(content_width, details_content_width(true).max(1.0));
    }

    #[test]
    fn model_indexes_intersecting_visual_rect_uses_projection_and_model_mapping() {
        let entries = Arc::new(vec![
            test_entry("alpha.txt"),
            test_entry("beta.txt"),
            test_entry("gamma.txt"),
        ]);
        let mut model = DirectoryModel::for_directory("/tmp/fika-projection-intersect".into());
        model.replace_listing("/tmp/fika-projection-intersect".into(), entries);
        let projection = PaneLayoutProjection::new(
            PaneLayout::Details {
                row_count: 3,
                content_width: 480.0,
            },
            None,
        );

        assert_eq!(
            model_indexes_intersecting_visual_rect(
                &projection,
                &model,
                None,
                ViewRect {
                    x: 0.0,
                    y: 56.0,
                    width: 480.0,
                    height: 40.0,
                },
            ),
            vec![1, 2]
        );
    }

    fn test_entry(name: &str) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }
}
