use crate::app::geometry::ItemViewItemBounds;
use crate::app::item_view_metrics::CompactItemVisualMetrics;
use crate::{ItemViewEntry, ItemViewPaintEntry};
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
pub(crate) struct ItemViewMediaToken {
    pub(crate) slice_index: i32,
    pub(crate) media_token: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewMediaSource {
    pub(crate) slice_index: i32,
    pub(crate) media: Image,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct ItemViewRasterMediaEntry {
    pub(crate) media: Image,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewMetadataOverlaySource {
    pub(crate) slice_index: i32,
    pub(crate) text: SharedString,
    pub(crate) item_x: f32,
    pub(crate) item_y: f32,
    pub(crate) text_x: f32,
    pub(crate) text_width: f32,
    pub(crate) y: f32,
    pub(crate) line_height: f32,
    pub(crate) font_size: f32,
    pub(crate) is_group: bool,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewTileFrameRasterInput {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) content_origin_x: f32,
    pub(crate) drop_target_slice_index: i32,
    pub(crate) dark: bool,
    pub(crate) tile_height: f32,
    pub(crate) media_x: f32,
    pub(crate) media_y: f32,
    pub(crate) media_width: f32,
    pub(crate) media_height: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct ItemViewTileFrameRaster {
    pub(crate) image: Image,
    pub(crate) width: u32,
    pub(crate) height: u32,
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

    #[cfg(test)]
    pub(crate) fn sources(&self) -> &[ItemViewTileFrameSource] {
        &self.sources
    }

    #[cfg(test)]
    pub(crate) fn plans(&self) -> &[ItemViewTileFramePlan] {
        &self.plans
    }

    pub(crate) fn paint_entries(&self) -> Vec<ItemViewPaintEntry> {
        self.plans
            .iter()
            .map(|plan| ItemViewPaintEntry {
                name: plan.text.name.clone(),
                x: plan.text.x,
                y: plan.text.y,
                width: plan.text.width,
                text_width: plan.text.text_width,
            })
            .collect()
    }

    pub(crate) fn media_token_for_slice_index(&self, slice_index: i32) -> i32 {
        usize::try_from(slice_index)
            .ok()
            .and_then(|row| self.sources.get(row))
            .map_or(0, |source| source.media_token)
    }

    pub(crate) fn media_tokens_for_sources(
        &self,
        media_entries: &[ItemViewMediaSource],
    ) -> Vec<ItemViewMediaToken> {
        media_entries
            .iter()
            .map(|media| ItemViewMediaToken {
                slice_index: media.slice_index,
                media_token: self.media_token_for_slice_index(media.slice_index),
            })
            .collect()
    }

    pub(crate) fn render_raster_layer(
        &self,
        input: ItemViewTileFrameRasterInput,
        media_entries: &[ItemViewRasterMediaEntry],
    ) -> ItemViewTileFrameRaster {
        let buffer = self.render_raster_buffer(input, media_entries);
        ItemViewTileFrameRaster {
            image: Image::from_rgba8(buffer),
            width: input.width,
            height: input.height,
        }
    }

    fn render_raster_buffer(
        &self,
        input: ItemViewTileFrameRasterInput,
        media_entries: &[ItemViewRasterMediaEntry],
    ) -> SharedPixelBuffer<Rgba8Pixel> {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(input.width, input.height);
        buffer
            .make_mut_slice()
            .fill(GlyphColor::rgba(0, 0, 0, 0).pixel());

        for plan in &self.plans {
            if let Some(highlight) = plan.highlight {
                draw_tile_highlight(
                    &mut buffer,
                    highlight.x - input.content_origin_x,
                    highlight.y,
                    highlight.width,
                    input.tile_height,
                    input.dark,
                );
            }
            draw_fallback_media_glyph_at(
                &mut buffer,
                plan.fallback_media.is_dir,
                input.dark,
                plan.fallback_media.x - input.content_origin_x + input.media_x,
                plan.fallback_media.y + input.media_y,
                input.media_width,
                input.media_height,
            );
        }
        for media in media_entries {
            draw_image_contain_at(
                &mut buffer,
                &media.media,
                media.x - input.content_origin_x + input.media_x,
                media.y + input.media_y,
                input.media_width,
                input.media_height,
            );
        }
        if input.drop_target_slice_index >= 0 {
            let drop_target_slice_index = input.drop_target_slice_index as usize;
            if let Some(plan) = self
                .plans
                .iter()
                .find(|plan| plan.slice_index == drop_target_slice_index)
            {
                draw_drop_target(
                    &mut buffer,
                    plan.text.x - input.content_origin_x,
                    plan.text.y,
                    plan.text.width,
                    input.tile_height,
                    input.dark,
                );
            }
        }

        buffer
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

fn draw_tile_highlight(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    dark: bool,
) {
    let background = if dark {
        GlyphColor::rgba(34, 63, 79, 255)
    } else {
        GlyphColor::rgba(228, 240, 248, 255)
    };
    draw_absolute_rect(buffer, x, y, width, height, background);
}

fn draw_drop_target(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    dark: bool,
) {
    let background = if dark {
        GlyphColor::rgba(58, 42, 18, 255)
    } else {
        GlyphColor::rgba(255, 247, 221, 255)
    };
    let border = if dark {
        GlyphColor::rgba(245, 158, 11, 255)
    } else {
        GlyphColor::rgba(217, 119, 6, 255)
    };
    draw_rounded_rect(buffer, x, y, width, height, 7.0, background, border, 1.0);
}

fn draw_fallback_media_glyph_at(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    is_dir: bool,
    dark: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    if is_dir {
        draw_folder_glyph_at(buffer, dark, x, y, width, height);
    } else {
        draw_file_glyph_at(buffer, dark, x, y, width, height);
    }
}

fn draw_folder_glyph_at(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    dark: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
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
    draw_relative_rect(buffer, x, y, width, height, 0.0, 0.14, 0.48, 0.26, tab);
    draw_relative_rect(buffer, x, y, width, height, 0.0, 0.29, 1.0, 0.69, body);
    draw_relative_rect(
        buffer, x, y, width, height, 0.08, 0.37, 0.82, 0.10, highlight,
    );
}

fn draw_file_glyph_at(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    dark: bool,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
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
    draw_relative_rect(buffer, x, y, width, height, 0.18, 0.10, 0.64, 0.82, body);
    draw_relative_rect(buffer, x, y, width, height, 0.58, 0.10, 0.24, 0.24, shade);
    draw_relative_rect(buffer, x, y, width, height, 0.30, 0.52, 0.40, 0.06, line);
    draw_relative_rect(buffer, x, y, width, height, 0.30, 0.66, 0.32, 0.06, line);
}

fn draw_relative_rect(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    origin_x: f32,
    origin_y: f32,
    origin_width: f32,
    origin_height: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: GlyphColor,
) {
    draw_absolute_rect(
        buffer,
        origin_x + x * origin_width,
        origin_y + y * origin_height,
        width * origin_width,
        height * origin_height,
        color,
    );
}

fn draw_rounded_rect(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    background: GlyphColor,
    border: GlyphColor,
    border_width: f32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let left = x.floor().max(0.0) as u32;
    let top = y.floor().max(0.0) as u32;
    let right = (x + width).ceil().min(buffer.width() as f32) as u32;
    let bottom = (y + height).ceil().min(buffer.height() as f32) as u32;
    if left >= right || top >= bottom {
        return;
    }

    let radius = radius.max(0.0).min(width / 2.0).min(height / 2.0);
    let inner_radius = (radius - border_width).max(0.0);
    let inner_left = x + border_width;
    let inner_top = y + border_width;
    let inner_width = (width - border_width * 2.0).max(0.0);
    let inner_height = (height - border_width * 2.0).max(0.0);
    let stride = buffer.width() as usize;
    let pixels = buffer.make_mut_slice();
    for dest_y in top..bottom {
        let py = dest_y as f32 + 0.5;
        for dest_x in left..right {
            let px = dest_x as f32 + 0.5;
            if !point_in_rounded_rect(px, py, x, y, width, height, radius) {
                continue;
            }
            let inside_inner = inner_width > 0.0
                && inner_height > 0.0
                && point_in_rounded_rect(
                    px,
                    py,
                    inner_left,
                    inner_top,
                    inner_width,
                    inner_height,
                    inner_radius,
                );
            let dest_index = dest_y as usize * stride + dest_x as usize;
            pixels[dest_index] = if inside_inner {
                background.pixel()
            } else {
                border.pixel()
            };
        }
    }
}

fn point_in_rounded_rect(
    px: f32,
    py: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> bool {
    if px < x || px >= x + width || py < y || py >= y + height {
        return false;
    }
    if radius <= 0.0 {
        return true;
    }
    let inner_left = x + radius;
    let inner_right = x + width - radius;
    let inner_top = y + radius;
    let inner_bottom = y + height - radius;
    if (px >= inner_left && px < inner_right) || (py >= inner_top && py < inner_bottom) {
        return true;
    }
    let cx = if px < inner_left {
        inner_left
    } else {
        inner_right
    };
    let cy = if py < inner_top {
        inner_top
    } else {
        inner_bottom
    };
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}

fn draw_image_contain_at(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    image: &Image,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    let Some(source) = image.to_rgba8() else {
        return;
    };
    let source_width = source.width();
    let source_height = source.height();
    if source_width == 0 || source_height == 0 || width <= 0.0 || height <= 0.0 {
        return;
    }

    let scale = (width / source_width as f32).min(height / source_height as f32);
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let draw_width = (source_width as f32 * scale).max(1.0);
    let draw_height = (source_height as f32 * scale).max(1.0);
    let draw_x = x + (width - draw_width) / 2.0;
    let draw_y = y + (height - draw_height) / 2.0;
    let dest_left = draw_x.floor().max(0.0) as u32;
    let dest_top = draw_y.floor().max(0.0) as u32;
    let dest_right = (draw_x + draw_width).ceil().min(buffer.width() as f32) as u32;
    let dest_bottom = (draw_y + draw_height).ceil().min(buffer.height() as f32) as u32;
    if dest_left >= dest_right || dest_top >= dest_bottom {
        return;
    }

    let source_pixels = source.as_slice();
    let dest_width = buffer.width() as usize;
    let dest_pixels = buffer.make_mut_slice();
    for dest_y in dest_top..dest_bottom {
        let local_y = ((dest_y as f32 + 0.5 - draw_y) / scale)
            .floor()
            .clamp(0.0, (source_height - 1) as f32) as u32;
        for dest_x in dest_left..dest_right {
            let local_x = ((dest_x as f32 + 0.5 - draw_x) / scale)
                .floor()
                .clamp(0.0, (source_width - 1) as f32) as u32;
            let source_pixel = source_pixels[(local_y * source_width + local_x) as usize];
            let dest_index = dest_y as usize * dest_width + dest_x as usize;
            dest_pixels[dest_index] = alpha_blend(source_pixel, dest_pixels[dest_index]);
        }
    }
}

fn alpha_blend(source: Rgba8Pixel, dest: Rgba8Pixel) -> Rgba8Pixel {
    let alpha = source.a as u32;
    if alpha == 255 {
        return source;
    }
    if alpha == 0 {
        return dest;
    }
    let inv_alpha = 255 - alpha;
    let out_alpha = alpha + (dest.a as u32 * inv_alpha + 127) / 255;
    let blend = |source_channel: u8, dest_channel: u8| -> u8 {
        ((source_channel as u32 * alpha + dest_channel as u32 * inv_alpha + 127) / 255) as u8
    };
    Rgba8Pixel::new(
        blend(source.r, dest.r),
        blend(source.g, dest.g),
        blend(source.b, dest.b),
        out_alpha as u8,
    )
}

fn draw_absolute_rect(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: GlyphColor,
) {
    let buffer_width = buffer.width() as usize;
    let buffer_height = buffer.height() as usize;
    if buffer_width == 0 || buffer_height == 0 || width <= 0.0 || height <= 0.0 {
        return;
    }

    let start_x = x.floor().max(0.0) as usize;
    let start_y = y.floor().max(0.0) as usize;
    let end_x = (x + width).ceil().max(start_x as f32) as usize;
    let end_y = (y + height).ceil().max(start_y as f32) as usize;
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
        let paint = batch.paint_entries();
        assert_eq!(paint.len(), 1);
        assert_eq!(paint[0].name, "Report");
        assert_eq!(paint[0].x, 120.0);
        assert_eq!(paint[0].y, 40.0);
        assert_eq!(paint[0].width, 180.0);
        assert_eq!(paint[0].text_width, 96.0);
    }

    #[test]
    fn tile_frame_batch_owns_media_token_lookup() {
        let mut first = test_entry(0);
        first.media_token = 21;
        let mut second = test_entry(1);
        second.media_token = 42;

        let batch = ItemViewTileFrameBatch::from_sources(vec![
            ItemViewTileFrameSource::from_entry_without_bounds(0, &first, false),
            ItemViewTileFrameSource::from_entry_without_bounds(1, &second, false),
        ]);

        assert_eq!(batch.media_token_for_slice_index(0), 21);
        assert_eq!(batch.media_token_for_slice_index(1), 42);
        assert_eq!(batch.media_token_for_slice_index(-1), 0);
        assert_eq!(batch.media_token_for_slice_index(2), 0);
        let media_tokens = batch.media_tokens_for_sources(&[
            ItemViewMediaSource {
                slice_index: 0,
                media: Image::default(),
                x: 0.0,
                y: 0.0,
            },
            ItemViewMediaSource {
                slice_index: 1,
                media: Image::default(),
                x: 0.0,
                y: 0.0,
            },
            ItemViewMediaSource {
                slice_index: 4,
                media: Image::default(),
                x: 0.0,
                y: 0.0,
            },
        ]);
        assert_eq!(
            media_tokens
                .iter()
                .map(|token| (token.slice_index, token.media_token))
                .collect::<Vec<_>>(),
            vec![(0, 21), (1, 42), (4, 0)]
        );
    }

    #[test]
    fn tile_frame_batch_renders_raster_layer_for_highlight_and_fallback_media() {
        let mut folder = test_entry(0);
        folder.is_dir = true;
        let mut file = test_entry(1);
        file.is_dir = false;
        let bounds = vec![
            ItemViewItemBounds {
                slice_index: 0,
                x: 5.0,
                y: 5.0,
                width: 40.0,
                text_width: 20.0,
            },
            ItemViewItemBounds {
                slice_index: 1,
                x: 5.0,
                y: 30.0,
                width: 40.0,
                text_width: 20.0,
            },
        ];
        let batch = ItemViewTileFrameBatch::from_entries_and_bounds(
            &[folder, file],
            &bounds,
            &["/tmp/item-0".to_string()],
        );

        let buffer = batch.render_raster_buffer(
            ItemViewTileFrameRasterInput {
                width: 64,
                height: 64,
                content_origin_x: 0.0,
                drop_target_slice_index: -1,
                dark: false,
                tile_height: 20.0,
                media_x: 2.0,
                media_y: 2.0,
                media_width: 8.0,
                media_height: 8.0,
            },
            &[],
        );

        assert_eq!(
            pixel_at(&buffer, 43, 6),
            Rgba8Pixel::new(228, 240, 248, 255)
        );
        assert_eq!(
            pixel_at(&buffer, 11, 11),
            Rgba8Pixel::new(96, 159, 224, 255)
        );
        assert_eq!(
            pixel_at(&buffer, 9, 34),
            Rgba8Pixel::new(174, 180, 186, 255)
        );
    }

    #[test]
    fn tile_frame_batch_renders_drop_target_into_raster_layer() {
        let first = test_entry(0);
        let second = test_entry(1);
        let bounds = vec![
            ItemViewItemBounds {
                slice_index: 0,
                x: 5.0,
                y: 5.0,
                width: 40.0,
                text_width: 20.0,
            },
            ItemViewItemBounds {
                slice_index: 1,
                x: 5.0,
                y: 30.0,
                width: 40.0,
                text_width: 20.0,
            },
        ];
        let batch = ItemViewTileFrameBatch::from_entries_and_bounds(&[first, second], &bounds, &[]);

        let buffer = batch.render_raster_buffer(
            ItemViewTileFrameRasterInput {
                width: 64,
                height: 64,
                content_origin_x: 0.0,
                drop_target_slice_index: 1,
                dark: false,
                tile_height: 20.0,
                media_x: 2.0,
                media_y: 2.0,
                media_width: 8.0,
                media_height: 8.0,
            },
            &[],
        );

        assert_eq!(pixel_at(&buffer, 5, 40), Rgba8Pixel::new(217, 119, 6, 255));
        assert_eq!(
            pixel_at(&buffer, 20, 40),
            Rgba8Pixel::new(255, 247, 221, 255)
        );
    }

    #[test]
    fn fallback_media_source_is_stable_across_zoom_metrics() {
        let small = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(0, 1);
        let large = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(4, 1);

        assert_ne!(small.media_width, large.media_width);
        assert_eq!(large.media_width, 72.0);
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

    fn pixel_at(buffer: &SharedPixelBuffer<Rgba8Pixel>, x: usize, y: usize) -> Rgba8Pixel {
        buffer.as_slice()[y * buffer.width() as usize + x]
    }
}
