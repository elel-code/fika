use crate::app::item_view_metrics::CompactItemVisualMetrics;
use crate::{ItemViewEntry, ItemViewMetadataEntry};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer, SharedString};
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

impl ItemViewMetadataSource {
    pub(crate) fn new(group: impl Into<SharedString>, location: impl Into<SharedString>) -> Self {
        Self {
            group: group.into(),
            location: location.into(),
        }
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

    pub(crate) fn renderer_key(self, dark: bool) -> ItemViewRendererKey {
        ItemViewRendererKey::new(self, dark)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemViewRendererKey {
    dark: bool,
    tile_height: u32,
    media_padding_x: u32,
    media_text_gap: u32,
    media_width: u32,
    media_height: u32,
    metadata_font_size: u32,
    metadata_line_height: u32,
    title_font_size: u32,
    title_line_height: u32,
}

impl ItemViewRendererKey {
    fn new(metrics: ItemViewRenderMetrics, dark: bool) -> Self {
        Self {
            dark,
            tile_height: renderer_metric_px(metrics.tile_height),
            media_padding_x: renderer_metric_px(metrics.media_padding_x),
            media_text_gap: renderer_metric_px(metrics.media_text_gap),
            media_width: renderer_metric_px(metrics.media_width),
            media_height: renderer_metric_px(metrics.media_height),
            metadata_font_size: renderer_metric_px(metrics.metadata_font_size),
            metadata_line_height: renderer_metric_px(metrics.metadata_line_height),
            title_font_size: renderer_metric_px(metrics.title_font_size),
            title_line_height: renderer_metric_px(metrics.title_line_height),
        }
    }
}

fn renderer_metric_px(value: f32) -> u32 {
    value.round().max(0.0) as u32
}

#[derive(Clone)]
pub(crate) struct ItemViewMediaCache {
    key: ItemViewRendererKey,
    folder: Image,
    file: Image,
}

impl ItemViewMediaCache {
    pub(crate) fn new(metrics: ItemViewRenderMetrics, dark: bool) -> Self {
        let key = metrics.renderer_key(dark);
        Self {
            key,
            folder: fallback_media_image(true, key.dark, key.media_width, key.media_height),
            file: fallback_media_image(false, key.dark, key.media_width, key.media_height),
        }
    }

    pub(crate) fn matches(&self, metrics: ItemViewRenderMetrics, dark: bool) -> bool {
        self.key == metrics.renderer_key(dark)
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
            .field("key", &self.key)
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
) -> Vec<ItemViewMetadataEntry> {
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
                metadata_entries.push(ItemViewMetadataEntry {
                    slice_index: row as i32,
                    text: metadata.group.clone(),
                    text_x: geometry.text_x,
                    text_width: geometry.text_width,
                    y: text_plan.group_y,
                    line_height: text_plan.metadata_line_height,
                    font_size: render_metrics.metadata_font_size,
                    is_group: true,
                });
            }
            if has_location {
                metadata_entries.push(ItemViewMetadataEntry {
                    slice_index: row as i32,
                    text: metadata.location.clone(),
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
            media: Image::default(),
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
    fn renderer_key_tracks_full_compact_render_state() {
        let plain = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1);
        let same_plain = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1);
        let location = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 3);
        let zoomed = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(4, 1);

        assert_eq!(plain.renderer_key(false), same_plain.renderer_key(false));
        assert_ne!(plain.renderer_key(false), plain.renderer_key(true));
        assert_ne!(
            plain.renderer_key(false),
            location.renderer_key(false),
            "text line count changes tile/text metrics even when media size is unchanged"
        );
        assert_ne!(plain.renderer_key(false), zoomed.renderer_key(false));
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
                ItemViewMetadataEntry {
                    slice_index: 0,
                    text: "Documents".into(),
                    text_x: 52.0,
                    text_width: 75.0,
                    y: 2.0,
                    line_height: 14.0,
                    font_size: 11.0,
                    is_group: true,
                },
                ItemViewMetadataEntry {
                    slice_index: 0,
                    text: "/home/user/Documents".into(),
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
        let metrics = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(1, 1);
        let cache = ItemViewMediaCache::new(metrics, false);
        let mut thumbnail_buffer = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
        thumbnail_buffer
            .make_mut_slice()
            .fill(Rgba8Pixel::new(255, 0, 0, 255));
        let thumbnail = Image::from_rgba8(thumbnail_buffer);
        let thumbnail_entry = ItemViewEntry {
            thumbnail_state: 2,
            media: thumbnail,
            ..test_entry(1)
        };

        let folder_media = cache
            .folder_image()
            .to_rgba8()
            .expect("folder fallback media");
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
        let thumbnail_media = thumbnail_entry.media.to_rgba8().expect("thumbnail media");
        assert!(
            thumbnail_media
                .as_slice()
                .iter()
                .all(|pixel| *pixel == Rgba8Pixel::new(255, 0, 0, 255))
        );
    }
}
