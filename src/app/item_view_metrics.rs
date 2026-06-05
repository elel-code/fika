pub(crate) const COMPACT_ITEM_PADDING: f32 = 2.0;
pub(crate) const COMPACT_MEDIA_TEXT_GAP: f32 = COMPACT_ITEM_PADDING * 2.0;
pub(crate) const COMPACT_COLUMN_MARGIN_WIDTH: f32 = 8.0;

const COMPACT_METADATA_LINE_SPACING: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CompactItemVisualMetrics {
    pub(crate) item_padding: f32,
    pub(crate) media_text_gap: f32,
    pub(crate) media_size: f32,
    pub(crate) title_font_size: f32,
    pub(crate) title_line_height: f32,
    pub(crate) metadata_font_size: f32,
    pub(crate) metadata_line_height: f32,
    pub(crate) text_block_height: f32,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
}

impl CompactItemVisualMetrics {
    pub(crate) fn from_zoom_level_with_text_line_count(
        zoom_level: i32,
        text_line_count: usize,
    ) -> Self {
        let media_size = compact_media_size(zoom_level);
        let title_font_size = compact_title_font_size(zoom_level);
        let title_line_height = compact_title_line_height(title_font_size);
        let metadata_font_size = compact_metadata_font_size(zoom_level);
        let metadata_line_height = compact_metadata_line_height(metadata_font_size);
        let text_block_height =
            compact_text_block_height(text_line_count, title_line_height, metadata_line_height);
        let cell_width = COMPACT_ITEM_PADDING * 4.0 + media_size + title_font_size * 5.0;
        let row_height = COMPACT_ITEM_PADDING * 2.0 + media_size.max(text_block_height);

        Self {
            item_padding: COMPACT_ITEM_PADDING,
            media_text_gap: COMPACT_MEDIA_TEXT_GAP,
            media_size,
            title_font_size,
            title_line_height,
            metadata_font_size,
            metadata_line_height,
            text_block_height,
            cell_width,
            row_height,
        }
    }
}

pub(crate) fn compact_cell_width(zoom_level: i32) -> f32 {
    CompactItemVisualMetrics::from_zoom_level_with_text_line_count(zoom_level, 1).cell_width
}

pub(crate) fn compact_row_height(zoom_level: i32, text_line_count: usize) -> f32 {
    CompactItemVisualMetrics::from_zoom_level_with_text_line_count(zoom_level, text_line_count)
        .row_height
}

fn compact_media_size(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 28.0,
        1 => 36.0,
        2 => 46.0,
        3 => 58.0,
        _ => 72.0,
    }
}

fn compact_title_font_size(_zoom_level: i32) -> f32 {
    15.0
}

fn compact_title_line_height(title_font_size: f32) -> f32 {
    title_font_size + 6.0
}

fn compact_metadata_font_size(_zoom_level: i32) -> f32 {
    11.0
}

fn compact_metadata_line_height(metadata_font_size: f32) -> f32 {
    metadata_font_size + 3.0
}

fn compact_text_block_height(
    text_line_count: usize,
    title_line_height: f32,
    metadata_line_height: f32,
) -> f32 {
    let text_line_count = text_line_count.max(1);
    if text_line_count == 1 {
        return title_line_height;
    }

    let metadata_lines = text_line_count.saturating_sub(1) as f32;
    let spacing = COMPACT_METADATA_LINE_SPACING * text_line_count.saturating_sub(1) as f32;
    title_line_height + metadata_lines * metadata_line_height + spacing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_visual_metrics_follow_dolphin_compact_formula() {
        let mid = CompactItemVisualMetrics::from_zoom_level_with_text_line_count(2, 3);
        assert_eq!(mid.cell_width, 129.0);
        assert_eq!(mid.row_height, 57.0);
        assert_eq!(mid.media_size, 46.0);
        assert_eq!(mid.title_font_size, 15.0);
        assert_eq!(mid.title_line_height, 21.0);
        assert_eq!(mid.metadata_font_size, 11.0);
        assert_eq!(mid.metadata_line_height, 14.0);

        let max = CompactItemVisualMetrics::from_zoom_level_with_text_line_count(4, 1);
        assert_eq!(max.cell_width, 155.0);
        assert_eq!(max.row_height, 76.0);
        assert_eq!(max.media_size, 72.0);
        assert_eq!(max.title_font_size, 15.0);
        assert_eq!(max.title_line_height, 21.0);
    }

    #[test]
    fn compact_text_metrics_stay_stable_across_icon_zoom() {
        let small = CompactItemVisualMetrics::from_zoom_level_with_text_line_count(0, 1);
        let large = CompactItemVisualMetrics::from_zoom_level_with_text_line_count(4, 1);

        assert_ne!(small.media_size, large.media_size);
        assert_eq!(small.title_font_size, large.title_font_size);
        assert_eq!(small.title_line_height, large.title_line_height);
        assert_eq!(small.metadata_font_size, large.metadata_font_size);
        assert_eq!(small.metadata_line_height, large.metadata_line_height);
    }

    #[test]
    fn compact_visual_metrics_are_the_only_zoom_formula_owner() {
        let geometry = include_str!("geometry.rs");
        let item_view_renderer = include_str!("item_view_renderer.rs");

        for (name, source) in [
            ("geometry", geometry),
            ("item_view_renderer", item_view_renderer),
        ] {
            assert!(
                !source.contains("fn compact_media_size")
                    && !source.contains("fn compact_title_font_size")
                    && !source.contains("fn compact_title_line_height")
                    && !source.contains("fn compact_metadata_line_height")
                    && !source.contains("fn compact_text_block_height"),
                "{name} should consume CompactItemVisualMetrics instead of owning compact zoom formulas"
            );
        }

        assert!(
            geometry.contains("compact_cell_width")
                && geometry.contains("compact_row_height")
                && item_view_renderer
                    .contains("CompactItemVisualMetrics::from_zoom_level_with_text_line_count"),
            "layout and render plan should both consume the shared compact visual metrics"
        );
    }
}
