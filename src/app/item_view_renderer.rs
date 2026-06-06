use crate::ItemViewEntry;
use crate::app::geometry::ItemViewItemBounds;
use crate::app::item_view_metrics::CompactItemVisualMetrics;
use crate::app::model_update::ItemViewMetadataOverlaySource;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewRenderMetrics {
    pub(crate) tile_height: f32,
    pub(crate) media_padding_x: f32,
    pub(crate) media_text_gap: f32,
    pub(crate) media_width: f32,
    pub(crate) media_height: f32,
    pub(crate) metadata_font_size: f32,
    pub(crate) metadata_line_height: f32,
    pub(crate) title_font_size: f32,
    pub(crate) title_line_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewRenderPlanInput {
    pub(crate) cell_width: f32,
    pub(crate) render_metrics: ItemViewRenderMetrics,
    pub(crate) show_location: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewRenderGeometry {
    pub(crate) media_x: f32,
    pub(crate) media_y: f32,
    pub(crate) media_width: f32,
    pub(crate) media_height: f32,
    pub(crate) text_x: f32,
    pub(crate) text_width: f32,
    pub(crate) title_y: f32,
    pub(crate) title_line_height: f32,
    pub(crate) title_font_size: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ItemViewMetadataSource {
    pub(crate) group: SharedString,
    pub(crate) location: SharedString,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewTileFrameSource {
    pub(crate) slice_index: usize,
    pub(crate) name: SharedString,
    pub(crate) is_dir: bool,
    pub(crate) selected: bool,
    pub(crate) media_token: i32,
    pub(crate) has_bounds: bool,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) text_width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewTileFramePlan {
    pub(crate) slice_index: usize,
    pub(crate) text: ItemViewTileTextFrame,
    pub(crate) fallback_media: ItemViewTileFallbackMediaFrame,
    pub(crate) highlight: Option<ItemViewTileHighlightFrame>,
    pub(crate) media_token: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ItemViewTileFrameBatch {
    sources: Vec<ItemViewTileFrameSource>,
    plans: Vec<ItemViewTileFramePlan>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewTileTextFrame {
    pub(crate) name: SharedString,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) text_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewTileFallbackMediaFrame {
    pub(crate) is_dir: bool,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewTileHighlightFrame {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
}

pub(crate) trait ItemViewFrameEntry {
    fn frame_name(&self) -> SharedString;
    fn frame_path(&self) -> &str;
    fn frame_is_dir(&self) -> bool;
    fn frame_media_token(&self) -> i32;
    fn frame_selected(&self) -> bool {
        false
    }
}

impl ItemViewMetadataSource {
    pub(crate) fn new(group: impl Into<SharedString>, location: impl Into<SharedString>) -> Self {
        Self {
            group: group.into(),
            location: location.into(),
        }
    }
}

impl ItemViewFrameEntry for ItemViewEntry {
    fn frame_name(&self) -> SharedString {
        self.name.clone()
    }

    fn frame_path(&self) -> &str {
        self.path.as_str()
    }

    fn frame_is_dir(&self) -> bool {
        self.is_dir
    }

    fn frame_media_token(&self) -> i32 {
        self.media_token
    }
}

impl ItemViewTileFrameSource {
    pub(crate) fn from_entry_and_bounds(
        entry: &impl ItemViewFrameEntry,
        bounds: &ItemViewItemBounds,
        selected: bool,
    ) -> Self {
        Self {
            slice_index: bounds.slice_index,
            name: entry.frame_name(),
            is_dir: entry.frame_is_dir(),
            selected,
            media_token: entry.frame_media_token(),
            has_bounds: true,
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            text_width: bounds.text_width,
        }
    }

    pub(crate) fn from_entry_without_bounds(
        slice_index: usize,
        entry: &impl ItemViewFrameEntry,
        selected: bool,
    ) -> Self {
        Self {
            slice_index,
            name: entry.frame_name(),
            is_dir: entry.frame_is_dir(),
            selected,
            media_token: entry.frame_media_token(),
            has_bounds: false,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            text_width: 0.0,
        }
    }
}

impl ItemViewTileFramePlan {
    pub(crate) fn from_source(source: &ItemViewTileFrameSource) -> Option<Self> {
        source.has_bounds.then(|| Self {
            slice_index: source.slice_index,
            text: ItemViewTileTextFrame {
                name: source.name.clone(),
                x: source.x,
                y: source.y,
                width: source.width,
                text_width: source.text_width,
            },
            fallback_media: ItemViewTileFallbackMediaFrame {
                is_dir: source.is_dir,
                x: source.x,
                y: source.y,
            },
            highlight: source.selected.then_some(ItemViewTileHighlightFrame {
                x: source.x,
                y: source.y,
                width: source.width,
            }),
            media_token: source.media_token,
        })
    }
}

impl ItemViewTileFrameBatch {
    pub(crate) fn from_sources(sources: Vec<ItemViewTileFrameSource>) -> Self {
        let plans = sources
            .iter()
            .filter_map(ItemViewTileFramePlan::from_source)
            .collect();
        Self { sources, plans }
    }

    pub(crate) fn from_entries_and_bounds<T: ItemViewFrameEntry>(
        entries: &[T],
        bounds_entries: &[ItemViewItemBounds],
        selected_paths: &[String],
    ) -> Self {
        let selected = selected_paths
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();

        if bounds_entries.is_empty() {
            return Self::from_sources(
                entries
                    .iter()
                    .enumerate()
                    .map(|(slice_index, entry)| {
                        ItemViewTileFrameSource::from_entry_without_bounds(
                            slice_index,
                            entry,
                            entry.frame_selected() || selected.contains(entry.frame_path()),
                        )
                    })
                    .collect(),
            );
        }

        Self::from_sources(
            bounds_entries
                .iter()
                .filter_map(|bounds| {
                    let entry = entries.get(bounds.slice_index)?;
                    Some(ItemViewTileFrameSource::from_entry_and_bounds(
                        entry,
                        bounds,
                        entry.frame_selected() || selected.contains(entry.frame_path()),
                    ))
                })
                .collect(),
        )
    }

    pub(crate) fn from_bounded_entries<T: ItemViewFrameEntry>(
        entries: &[T],
        bounds_entries: &[ItemViewItemBounds],
    ) -> Self {
        Self::from_sources(
            bounds_entries
                .iter()
                .filter_map(|bounds| {
                    let entry = entries.get(bounds.slice_index)?;
                    Some(ItemViewTileFrameSource::from_entry_and_bounds(
                        entry,
                        bounds,
                        entry.frame_selected(),
                    ))
                })
                .collect(),
        )
    }

    pub(crate) fn sources(&self) -> &[ItemViewTileFrameSource] {
        &self.sources
    }

    pub(crate) fn plans(&self) -> &[ItemViewTileFramePlan] {
        &self.plans
    }
}

impl ItemViewRenderMetrics {
    pub(crate) fn from_zoom_level_with_text_line_count(
        zoom_level: i32,
        text_line_count: usize,
    ) -> Self {
        let compact = CompactItemVisualMetrics::from_zoom_level_with_text_line_count(
            zoom_level,
            text_line_count,
        );
        Self {
            tile_height: compact.row_height,
            media_padding_x: compact.item_padding,
            media_text_gap: compact.media_text_gap,
            media_width: compact.media_size,
            media_height: compact.media_size,
            metadata_font_size: compact.metadata_font_size,
            metadata_line_height: compact.metadata_line_height,
            title_font_size: compact.title_font_size,
            title_line_height: compact.title_line_height,
        }
    }
}

impl ItemViewRenderGeometry {
    pub(crate) fn from_plan_input(input: ItemViewRenderPlanInput) -> Self {
        let render_metrics = input.render_metrics;
        let cell_width = input
            .cell_width
            .max(compact_min_cell_width(render_metrics))
            .max(1.0);
        let text_x = render_metrics.media_padding_x
            + render_metrics.media_width
            + render_metrics.media_text_gap;
        let text_plan =
            ItemTextRenderPlan::new(render_metrics, input.show_location, input.show_location);

        Self {
            media_x: render_metrics.media_padding_x,
            media_y: ((render_metrics.tile_height - render_metrics.media_height) / 2.0).max(0.0),
            media_width: render_metrics.media_width,
            media_height: render_metrics.media_height,
            text_x,
            text_width: (cell_width - text_x - render_metrics.media_padding_x).max(1.0),
            title_y: text_plan.title_y,
            title_line_height: text_plan.title_line_height,
            title_font_size: render_metrics.title_font_size,
        }
    }
}

const FALLBACK_MEDIA_SOURCE_SIZE_PX: u32 = 72;

#[derive(Clone)]
pub(crate) struct ItemViewMediaCache {
    dark: bool,
    folder: Image,
    file: Image,
}

impl ItemViewMediaCache {
    pub(crate) fn new(dark: bool) -> Self {
        Self {
            dark,
            folder: fallback_media_image(
                true,
                dark,
                FALLBACK_MEDIA_SOURCE_SIZE_PX,
                FALLBACK_MEDIA_SOURCE_SIZE_PX,
            ),
            file: fallback_media_image(
                false,
                dark,
                FALLBACK_MEDIA_SOURCE_SIZE_PX,
                FALLBACK_MEDIA_SOURCE_SIZE_PX,
            ),
        }
    }

    pub(crate) fn dark(&self) -> bool {
        self.dark
    }

    pub(crate) fn folder_image(&self) -> Image {
        self.folder.clone()
    }

    pub(crate) fn file_image(&self) -> Image {
        self.file.clone()
    }
}

impl std::fmt::Debug for ItemViewMediaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemViewMediaCache")
            .field("dark", &self.dark)
            .field("source_size_px", &FALLBACK_MEDIA_SOURCE_SIZE_PX)
            .finish()
    }
}

#[cfg(test)]
pub(crate) fn decorate_render_plan(entries: &mut [ItemViewEntry], input: ItemViewRenderPlanInput) {
    let _ = decorate_render_plan_with_metadata(entries, input, &[]);
}

pub(crate) fn decorate_render_plan_with_metadata(
    entries: &mut [ItemViewEntry],
    input: ItemViewRenderPlanInput,
    metadata_sources: &[ItemViewMetadataSource],
) -> Vec<ItemViewMetadataOverlaySource> {
    let render_metrics = input.render_metrics;
    let geometry = ItemViewRenderGeometry::from_plan_input(input);
    let text_plan =
        ItemTextRenderPlan::new(render_metrics, input.show_location, input.show_location);
    let mut metadata_entries = Vec::new();

    for (row, entry) in entries.iter_mut().enumerate() {
        ensure_renderable_entry_name(entry);
        let metadata = metadata_sources.get(row);
        let has_group =
            input.show_location && metadata.is_some_and(|metadata| !metadata.group.is_empty());
        let has_location =
            input.show_location && metadata.is_some_and(|metadata| !metadata.location.is_empty());

        if let Some(metadata) = metadata.filter(|_| input.show_location) {
            if has_group {
                metadata_entries.push(ItemViewMetadataOverlaySource {
                    slice_index: row as i32,
                    text: metadata.group.clone(),
                    item_x: 0.0,
                    item_y: 0.0,
                    text_x: geometry.text_x,
                    text_width: geometry.text_width,
                    y: text_plan.group_y,
                    line_height: text_plan.metadata_line_height,
                    font_size: render_metrics.metadata_font_size,
                    is_group: true,
                });
            }
            if has_location {
                metadata_entries.push(ItemViewMetadataOverlaySource {
                    slice_index: row as i32,
                    text: metadata.location.clone(),
                    item_x: 0.0,
                    item_y: 0.0,
                    text_x: geometry.text_x,
                    text_width: geometry.text_width,
                    y: text_plan.location_y,
                    line_height: text_plan.metadata_line_height,
                    font_size: render_metrics.metadata_font_size,
                    is_group: false,
                });
            }
        }
    }

    metadata_entries
}

fn compact_min_cell_width(metrics: ItemViewRenderMetrics) -> f32 {
    metrics.media_padding_x * 4.0 + metrics.media_width + metrics.title_font_size * 5.0
}

fn ensure_renderable_entry_name(entry: &mut ItemViewEntry) {
    if !entry.name.trim().is_empty() {
        return;
    }

    let fallback = Path::new(entry.path.as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| entry.path.to_string());
    entry.name = fallback.into();
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ItemTextRenderPlan {
    group_y: f32,
    title_y: f32,
    location_y: f32,
    metadata_line_height: f32,
    title_line_height: f32,
}

impl ItemTextRenderPlan {
    fn new(metrics: ItemViewRenderMetrics, has_group: bool, has_location: bool) -> Self {
        let metadata_line_height = metrics.metadata_line_height;
        let title_line_height = metrics.title_line_height;
        if !has_group && !has_location {
            return Self {
                group_y: 0.0,
                title_y: 0.0,
                location_y: metrics.tile_height,
                metadata_line_height,
                title_line_height: metrics.tile_height,
            };
        }

        let spacing = 2.0;
        let mut line_count = 1;
        if has_group {
            line_count += 1;
        }
        if has_location {
            line_count += 1;
        }

        let mut block_height = title_line_height;
        if has_group {
            block_height += metadata_line_height;
        }
        if has_location {
            block_height += metadata_line_height;
        }
        block_height += spacing * (line_count - 1) as f32;

        let mut y = ((metrics.tile_height - block_height) / 2.0)
            .round()
            .max(0.0);
        let group_y = y;
        if has_group {
            y += metadata_line_height + spacing;
        }
        let title_y = y;
        y += title_line_height + spacing;
        let location_y = y;

        Self {
            group_y,
            title_y,
            location_y,
            metadata_line_height,
            title_line_height,
        }
    }
}

#[derive(Clone, Copy)]
struct GlyphColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl GlyphColor {
    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn pixel(self) -> Rgba8Pixel {
        Rgba8Pixel::new(self.r, self.g, self.b, self.a)
    }
}

fn fallback_media_image(is_dir: bool, dark: bool, width: u32, height: u32) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    buffer
        .make_mut_slice()
        .fill(GlyphColor::rgba(0, 0, 0, 0).pixel());
    if is_dir {
        draw_folder_glyph(&mut buffer, dark);
    } else {
        draw_file_glyph(&mut buffer, dark);
    }
    Image::from_rgba8(buffer)
}

fn draw_folder_glyph(buffer: &mut SharedPixelBuffer<Rgba8Pixel>, dark: bool) {
    let tab = if dark {
        GlyphColor::rgba(59, 102, 139, 255)
    } else {
        GlyphColor::rgba(114, 174, 230, 255)
    };
    let body = if dark {
        GlyphColor::rgba(63, 111, 152, 255)
    } else {
        GlyphColor::rgba(96, 159, 224, 255)
    };
    let highlight = if dark {
        GlyphColor::rgba(169, 184, 196, 255)
    } else {
        GlyphColor::rgba(237, 244, 250, 255)
    };
    draw_rect(buffer, 0.0, 0.14, 0.48, 0.26, tab);
    draw_rect(buffer, 0.0, 0.29, 1.0, 0.69, body);
    draw_rect(buffer, 0.08, 0.37, 0.82, 0.10, highlight);
}

fn draw_file_glyph(buffer: &mut SharedPixelBuffer<Rgba8Pixel>, dark: bool) {
    let body = if dark {
        GlyphColor::rgba(139, 145, 151, 255)
    } else {
        GlyphColor::rgba(174, 180, 186, 255)
    };
    let shade = if dark {
        GlyphColor::rgba(113, 119, 126, 255)
    } else {
        GlyphColor::rgba(151, 158, 165, 255)
    };
    let line = if dark {
        GlyphColor::rgba(48, 48, 48, 255)
    } else {
        GlyphColor::rgba(85, 85, 85, 255)
    };
    draw_rect(buffer, 0.18, 0.10, 0.64, 0.82, body);
    draw_rect(buffer, 0.58, 0.10, 0.24, 0.24, shade);
    draw_rect(buffer, 0.30, 0.52, 0.40, 0.06, line);
    draw_rect(buffer, 0.30, 0.66, 0.32, 0.06, line);
}

fn draw_rect(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: GlyphColor,
) {
    let buffer_width = buffer.width() as usize;
    let buffer_height = buffer.height() as usize;
    if buffer_width == 0 || buffer_height == 0 {
        return;
    }
    let start_x = (x * buffer_width as f32).round().max(0.0) as usize;
    let start_y = (y * buffer_height as f32).round().max(0.0) as usize;
    let end_x = ((x + width) * buffer_width as f32)
        .round()
        .max(start_x as f32) as usize;
    let end_y = ((y + height) * buffer_height as f32)
        .round()
        .max(start_y as f32) as usize;
    let end_x = end_x.min(buffer_width);
    let end_y = end_y.min(buffer_height);
    let pixel = color.pixel();
    let pixels = buffer.make_mut_slice();
    for row in start_y..end_y {
        let row_start = row * buffer_width;
        for col in start_x..end_x {
            pixels[row_start + col] = pixel;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_entry(index: usize) -> ItemViewEntry {
        ItemViewEntry {
            name: format!("item-{index}").into(),
            path: format!("/tmp/item-{index}").into(),
            is_dir: false,
            thumbnail_state: 0,
            media_token: 0,
        }
    }

    #[test]
    fn render_geometry_keeps_compact_pane_tokens_stable() {
        let mut entries = (4..9).map(test_entry).collect::<Vec<_>>();
        let input = ItemViewRenderPlanInput {
            cell_width: 129.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1),
            show_location: false,
        };

        decorate_render_plan(&mut entries, input);

        let geometry = ItemViewRenderGeometry::from_plan_input(input);
        assert_eq!(
            geometry,
            ItemViewRenderGeometry {
                media_x: 2.0,
                media_y: 2.0,
                media_width: 46.0,
                media_height: 46.0,
                text_x: 52.0,
                text_width: 75.0,
                title_y: 0.0,
                title_line_height: 50.0,
                title_font_size: 15.0,
            }
        );
        assert!(
            entries.iter().all(|entry| !entry.name.is_empty()),
            "visible icon rows must keep a title while geometry lives at pane level"
        );
    }

    #[test]
    fn tile_frame_source_collects_render_identity_and_geometry() {
        let entry = ItemViewEntry {
            name: "Report".into(),
            path: "/tmp/report.txt".into(),
            is_dir: false,
            thumbnail_state: 2,
            media_token: 42,
        };
        let bounds = ItemViewItemBounds {
            slice_index: 3,
            x: 120.0,
            y: 40.0,
            width: 180.0,
            text_width: 96.0,
        };

        let frame = ItemViewTileFrameSource::from_entry_and_bounds(&entry, &bounds, true);

        assert_eq!(frame.slice_index, 3);
        assert_eq!(frame.name, "Report");
        assert!(!frame.is_dir);
        assert!(frame.selected);
        assert_eq!(frame.media_token, 42);
        assert_eq!(frame.x, 120.0);
        assert_eq!(frame.y, 40.0);
        assert_eq!(frame.width, 180.0);
        assert_eq!(frame.text_width, 96.0);
    }

    #[test]
    fn tile_frame_batch_keeps_sources_and_collects_visible_primitives_only() {
        let entry = ItemViewEntry {
            name: "Report".into(),
            path: "/tmp/report.txt".into(),
            is_dir: true,
            thumbnail_state: 2,
            media_token: 42,
        };
        let bounds = ItemViewItemBounds {
            slice_index: 0,
            x: 120.0,
            y: 40.0,
            width: 180.0,
            text_width: 96.0,
        };
        let visible = ItemViewTileFrameSource::from_entry_and_bounds(&entry, &bounds, true);
        let hidden = ItemViewTileFrameSource::from_entry_without_bounds(1, &entry, false);

        let batch = ItemViewTileFrameBatch::from_sources(vec![visible, hidden]);

        assert_eq!(batch.sources().len(), 2);
        assert_eq!(batch.plans().len(), 1);
        assert_eq!(batch.plans()[0].text.name, "Report");
        assert_eq!(batch.plans()[0].text.x, 120.0);
        assert!(batch.plans()[0].fallback_media.is_dir);
        assert_eq!(batch.plans()[0].media_token, 42);
        assert_eq!(
            batch.plans()[0].highlight,
            Some(ItemViewTileHighlightFrame {
                x: 120.0,
                y: 40.0,
                width: 180.0,
            })
        );
    }

    #[test]
    fn fallback_media_source_is_stable_across_zoom_metrics() {
        let small = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(0, 1);
        let large = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(4, 1);

        assert_ne!(small.media_width, large.media_width);
        assert_eq!(FALLBACK_MEDIA_SOURCE_SIZE_PX as f32, large.media_width);
    }

    #[test]
    fn render_plan_precomputes_location_text_lines() {
        let mut entries = vec![test_entry(0)];
        let metadata = vec![ItemViewMetadataSource::new(
            "Documents",
            "/home/user/Documents",
        )];

        let input = ItemViewRenderPlanInput {
            cell_width: 129.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 3),
            show_location: true,
        };

        let metadata_entries = decorate_render_plan_with_metadata(&mut entries, input, &metadata);

        let geometry = ItemViewRenderGeometry::from_plan_input(input);
        assert_eq!(geometry.media_x, 2.0);
        assert_eq!(geometry.media_y, 5.5);
        assert_eq!(geometry.text_x, 52.0);
        assert_eq!(geometry.text_width, 75.0);
        assert_eq!(geometry.title_line_height, 21.0);
        assert_eq!(geometry.title_y, 18.0);
        assert_eq!(
            metadata_entries,
            vec![
                ItemViewMetadataOverlaySource {
                    slice_index: 0,
                    text: "Documents".into(),
                    item_x: 0.0,
                    item_y: 0.0,
                    text_x: 52.0,
                    text_width: 75.0,
                    y: 2.0,
                    line_height: 14.0,
                    font_size: 11.0,
                    is_group: true,
                },
                ItemViewMetadataOverlaySource {
                    slice_index: 0,
                    text: "/home/user/Documents".into(),
                    item_x: 0.0,
                    item_y: 0.0,
                    text_x: 52.0,
                    text_width: 75.0,
                    y: 41.0,
                    line_height: 14.0,
                    font_size: 11.0,
                    is_group: false,
                },
            ]
        );
    }

    #[test]
    fn render_geometry_reserves_location_title_frame_at_pane_level() {
        let mut entries = vec![test_entry(0)];
        let input = ItemViewRenderPlanInput {
            cell_width: 129.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 3),
            show_location: true,
        };

        decorate_render_plan(&mut entries, input);

        let geometry = ItemViewRenderGeometry::from_plan_input(input);
        assert_eq!(geometry.media_x, 2.0);
        assert_eq!(geometry.text_x, 52.0);
        assert_eq!(geometry.text_width, 75.0);
        assert_eq!(geometry.title_y, 18.0);
        assert_eq!(geometry.title_line_height, 21.0);
        assert!(entries.iter().all(|entry| !entry.name.is_empty()));
    }

    #[test]
    fn render_plan_supplies_name_fallback_before_slint_paints_text() {
        let mut entries = vec![ItemViewEntry {
            name: SharedString::new(),
            path: "/tmp/visible-name.txt".into(),
            ..test_entry(0)
        }];

        decorate_render_plan(
            &mut entries,
            ItemViewRenderPlanInput {
                cell_width: 0.0,
                render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1),
                show_location: false,
            },
        );

        let entry = &entries[0];
        assert_eq!(entry.name, "visible-name.txt");
        let geometry = ItemViewRenderGeometry::from_plan_input(ItemViewRenderPlanInput {
            cell_width: 0.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1),
            show_location: false,
        });
        assert_eq!(geometry.text_x, 52.0);
        assert_eq!(geometry.text_width, 75.0);
    }

    #[test]
    fn render_plan_keeps_titles_renderable_at_max_zoom() {
        let mut entries = vec![test_entry(0)];

        decorate_render_plan(
            &mut entries,
            ItemViewRenderPlanInput {
                cell_width: 0.0,
                render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(4, 1),
                show_location: false,
            },
        );

        let geometry = ItemViewRenderGeometry::from_plan_input(ItemViewRenderPlanInput {
            cell_width: 0.0,
            render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(4, 1),
            show_location: false,
        });
        assert_eq!(geometry.media_width, 72.0);
        assert_eq!(geometry.media_height, 72.0);
        assert_eq!(geometry.text_x, 78.0);
        assert_eq!(geometry.text_width, 75.0);
        assert_eq!(geometry.title_y, 0.0);
        assert_eq!(geometry.title_line_height, 76.0);
        assert!(entries.iter().all(|entry| !entry.name.is_empty()));
    }

    #[test]
    fn fallback_media_renderer_supplies_pane_level_icons() {
        let cache = ItemViewMediaCache::new(false);

        let folder_media = cache
            .folder_image()
            .to_rgba8()
            .expect("folder fallback media");
        assert_eq!(folder_media.width(), FALLBACK_MEDIA_SOURCE_SIZE_PX);
        assert_eq!(folder_media.height(), FALLBACK_MEDIA_SOURCE_SIZE_PX);
        assert!(
            folder_media
                .as_slice()
                .iter()
                .any(|pixel| pixel.a != 0 && (pixel.r != 0 || pixel.g != 0 || pixel.b != 0))
        );
        let file_media = cache.file_image().to_rgba8().expect("file fallback media");
        assert!(
            file_media
                .as_slice()
                .iter()
                .any(|pixel| pixel.a != 0 && (pixel.r != 0 || pixel.g != 0 || pixel.b != 0))
        );
    }
}
