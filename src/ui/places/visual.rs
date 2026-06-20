use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    App, Bounds, Corners, Entity, Font, IntoElement, Pixels, RenderImage, SharedString, Styled,
    TextAlign, TextRun, WeakEntity, Window, canvas, fill, point, px, rgb, rgba, size,
};

use crate::FikaApp;
use crate::ui::icons::{FileIconSnapshot, IconPaintMode};
use crate::ui::retained::{RetainedImageLayerState, RetainedImageRequest, RetainedShapeCache};

use super::perf::{
    PlacesRowGlyphRasterCachePerfLog, PlacesRowGlyphRasterCacheStats,
    PlacesRowTextShapeCachePerfLog, PlacesRowTextShapeCacheStats, PlacesRowVisualPerfLog,
    emit_places_row_glyph_raster_cache_perf_log, emit_places_row_text_shape_cache_perf_log,
    emit_places_row_visual_perf_log, places_perf_enabled,
};
use super::snapshot::PlaceSnapshot;
use super::style::{place_row_background, place_row_border_color};

pub(super) const PLACE_ROW_HEIGHT: f32 = 30.0;
pub(super) const PLACE_ROW_ICON_SIZE: f32 = 22.0;
pub(super) const PLACE_SECTION_HEADING_HEIGHT: f32 = 24.0;

const ROW_PADDING_X: f32 = 8.0;
const ICON_TEXT_GAP: f32 = 8.0;
const TRASH_DOT_SIZE: f32 = 7.0;
const INSERT_INDICATOR_HEIGHT: f32 = 2.0;
const SECTION_TEXT_X: f32 = 8.0;
const SECTION_LINE_HEIGHT: f32 = 16.0;
pub(super) fn places_row_visual_layer(
    places: Arc<[PlaceSnapshot]>,
    app: WeakEntity<FikaApp>,
    icon_cache: Option<Entity<RetainedImageLayerState>>,
    paint_text: bool,
    warm_text_shapes: bool,
    paint_icon: bool,
) -> impl IntoElement {
    let (rows, sections, height) = place_row_visual_layer_rows_and_height(places.as_ref());
    let rows = Arc::new(rows);
    let sections = Arc::new(sections);
    let total_rows = rows.len();
    let height = height.max(1.0);
    canvas(
        move |bounds, window, cx| {
            places_row_visual_prepaint(
                rows.as_ref(),
                sections.as_ref(),
                total_rows,
                app.clone(),
                icon_cache.clone(),
                paint_text,
                warm_text_shapes,
                paint_icon,
                bounds,
                window,
                cx,
            )
        },
        move |bounds, paint_state, window, cx| {
            let paint_started = Instant::now();
            for section in &paint_state.sections {
                paint_place_section_visual(bounds, section, window, cx);
            }
            for row in &paint_state.rows {
                paint_place_row_visual(bounds, row, window, cx);
            }
            if places_perf_enabled() {
                emit_places_row_visual_perf_log(PlacesRowVisualPerfLog {
                    rows: paint_state.total_rows,
                    painted_rows: paint_state.rows.len(),
                    prepaint_elapsed: paint_state.prepaint_elapsed,
                    paint_elapsed: paint_started.elapsed(),
                });
                if paint_state.shape_cache_stats.has_activity() {
                    emit_places_row_text_shape_cache_perf_log(PlacesRowTextShapeCachePerfLog {
                        stats: paint_state.shape_cache_stats,
                    });
                }
                if paint_state.glyph_cache_stats.has_activity() {
                    emit_places_row_glyph_raster_cache_perf_log(PlacesRowGlyphRasterCachePerfLog {
                        stats: paint_state.glyph_cache_stats,
                    });
                }
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .w_full()
    .h(px(height))
}

#[derive(Clone)]
struct PlaceRowVisualState {
    y: f32,
    label: SharedString,
    icon: PlaceRowIconVisualState,
    active: bool,
    mounted: bool,
    drop_target: bool,
    insert_before: bool,
    insert_after: bool,
    trash_place: bool,
    trash_has_items: bool,
}

#[derive(Clone)]
struct PlaceSectionVisualState {
    y: f32,
    label: SharedString,
}

impl PlaceSectionVisualState {
    fn new(label: &'static str, y: f32) -> Self {
        Self {
            y,
            label: SharedString::from(label),
        }
    }

    fn intersects_y_range(&self, top: f32, bottom: f32) -> bool {
        self.y < bottom && self.y + PLACE_SECTION_HEADING_HEIGHT > top
    }
}

impl PlaceRowVisualState {
    fn from_place(place: &PlaceSnapshot, y: f32) -> Self {
        let insert_target = place.insert_before || place.insert_after;
        Self {
            y,
            label: SharedString::from(place.label.as_str()),
            icon: PlaceRowIconVisualState::from_icon(&place.icon, place.active && !insert_target),
            active: place.active && !insert_target,
            mounted: place.mounted,
            drop_target: place.drop_target && !insert_target,
            insert_before: place.insert_before,
            insert_after: place.insert_after,
            trash_place: place.trash_place,
            trash_has_items: place.trash_has_items,
        }
    }

    fn intersects_y_range(&self, top: f32, bottom: f32) -> bool {
        self.y < bottom && self.y + PLACE_ROW_HEIGHT > top
    }
}

#[derive(Clone)]
struct PlaceRowIconVisualState {
    icon_name: Arc<str>,
    path: Option<Arc<Path>>,
    marker: SharedString,
    fallback_fg: u32,
    fallback_bg: u32,
    mode: IconPaintMode,
}

impl PlaceRowIconVisualState {
    fn from_icon(icon: &FileIconSnapshot, active: bool) -> Self {
        Self {
            icon_name: icon.icon_name.clone(),
            path: icon.path.clone(),
            marker: SharedString::from(icon.fallback_marker.as_ref()),
            fallback_fg: if active { 0x1f4fbf } else { icon.fallback_fg },
            fallback_bg: if active { 0xeaf1ff } else { icon.fallback_bg },
            mode: if active {
                IconPaintMode::Active
            } else {
                IconPaintMode::Normal
            },
        }
    }
}

struct PlaceRowVisualLayerPaintState {
    rows: Vec<PlaceRowVisualPaintState>,
    sections: Vec<PlaceSectionVisualPaintState>,
    total_rows: usize,
    prepaint_elapsed: std::time::Duration,
    shape_cache_stats: PlacesRowTextShapeCacheStats,
    glyph_cache_stats: PlacesRowGlyphRasterCacheStats,
}

struct PlaceRowVisualPaintState {
    input: PlaceRowVisualState,
    text: Option<PlaceTextVisualPaintState>,
    line_height: Pixels,
    paint_icon: bool,
    icon_image: Option<Arc<RenderImage>>,
}

struct PlaceSectionVisualPaintState {
    input: PlaceSectionVisualState,
    text: Option<PlaceTextVisualPaintState>,
    line_height: Pixels,
}

struct PlaceTextVisualPaintState {
    line: Arc<gpui::ShapedLine>,
    raster_data: Option<Arc<gpui::GlyphRasterData>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PlacesRowTextShapeCacheKey {
    label: SharedString,
    font: Font,
    font_size_bits: u32,
    text_color: u32,
}

pub(crate) struct PlacesRowTextShapeCache {
    cache: RetainedShapeCache<PlacesRowTextShapeCacheKey, Arc<gpui::ShapedLine>>,
    glyph_cache: RetainedShapeCache<PlacesRowGlyphRasterCacheKey, Arc<gpui::GlyphRasterData>>,
}

impl PlacesRowTextShapeCache {
    const MAX_ENTRIES: usize = 512;

    fn shape_for(
        &mut self,
        key: &PlacesRowTextShapeCacheKey,
        window: &mut Window,
    ) -> Arc<gpui::ShapedLine> {
        self.cache.get_or_insert_with(key, |key| {
            Arc::new(shape_place_row_visual_text(key, window))
        })
    }

    pub(crate) fn take_stats(&mut self) -> PlacesRowTextShapeCacheStats {
        self.cache.take_stats()
    }

    fn glyph_raster_for(
        &mut self,
        key: &PlacesRowGlyphRasterCacheKey,
    ) -> Option<Arc<gpui::GlyphRasterData>> {
        self.glyph_cache.get(key)
    }

    fn insert_glyph_raster(
        &mut self,
        key: PlacesRowGlyphRasterCacheKey,
        raster_data: Arc<gpui::GlyphRasterData>,
    ) {
        self.glyph_cache.insert(key, raster_data);
    }

    pub(crate) fn take_glyph_stats(&mut self) -> PlacesRowGlyphRasterCacheStats {
        self.glyph_cache.take_stats()
    }
}

impl Default for PlacesRowTextShapeCache {
    fn default() -> Self {
        Self {
            cache: RetainedShapeCache::new(Self::MAX_ENTRIES),
            glyph_cache: RetainedShapeCache::new(Self::MAX_ENTRIES),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PlacesRowGlyphRasterCacheKey {
    text: PlacesRowTextShapeCacheKey,
    origin_x_bits: u32,
    origin_y_bits: u32,
    line_height_bits: u32,
    align_width_bits: Option<u32>,
    scale_factor_bits: u32,
}

fn places_row_visual_prepaint(
    rows: &[PlaceRowVisualState],
    sections: &[PlaceSectionVisualState],
    total_rows: usize,
    app: WeakEntity<FikaApp>,
    icon_cache: Option<Entity<RetainedImageLayerState>>,
    paint_text: bool,
    warm_text_shapes: bool,
    paint_icon: bool,
    layer_bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> PlaceRowVisualLayerPaintState {
    let started = Instant::now();
    let sections =
        visible_place_section_visuals(sections, layer_bounds, window.content_mask().bounds)
            .into_iter()
            .map(|input| {
                place_section_visual_prepaint(
                    input,
                    paint_text,
                    warm_text_shapes,
                    layer_bounds,
                    &app,
                    window,
                    cx,
                )
            })
            .collect();
    let rows = visible_place_row_visuals(rows, layer_bounds, window.content_mask().bounds)
        .into_iter()
        .map(|input| {
            place_row_visual_prepaint(
                input,
                paint_text,
                warm_text_shapes,
                paint_icon,
                layer_bounds,
                &app,
                icon_cache.as_ref(),
                window,
                cx,
            )
        })
        .collect();
    let shape_cache_stats = app
        .update(cx, |this, _cx| this.place_row_text_shape_cache.take_stats())
        .ok()
        .unwrap_or_default();
    let glyph_cache_stats = app
        .update(cx, |this, _cx| {
            this.place_row_text_shape_cache.take_glyph_stats()
        })
        .ok()
        .unwrap_or_default();
    PlaceRowVisualLayerPaintState {
        rows,
        sections,
        total_rows,
        prepaint_elapsed: started.elapsed(),
        shape_cache_stats,
        glyph_cache_stats,
    }
}

fn visible_place_section_visuals(
    sections: &[PlaceSectionVisualState],
    layer_bounds: Bounds<Pixels>,
    content_mask_bounds: Bounds<Pixels>,
) -> Vec<PlaceSectionVisualState> {
    let visible_bounds = layer_bounds.intersect(&content_mask_bounds);
    if visible_bounds.is_empty() {
        return Vec::new();
    }

    let visible_top = (visible_bounds.origin.y - layer_bounds.origin.y)
        .as_f32()
        .max(0.0);
    let visible_bottom = visible_top + visible_bounds.size.height.as_f32().max(0.0);
    sections
        .iter()
        .filter(|section| section.intersects_y_range(visible_top, visible_bottom))
        .cloned()
        .collect()
}

fn visible_place_row_visuals(
    rows: &[PlaceRowVisualState],
    layer_bounds: Bounds<Pixels>,
    content_mask_bounds: Bounds<Pixels>,
) -> Vec<PlaceRowVisualState> {
    let visible_bounds = layer_bounds.intersect(&content_mask_bounds);
    if visible_bounds.is_empty() {
        return Vec::new();
    }

    let visible_top = (visible_bounds.origin.y - layer_bounds.origin.y)
        .as_f32()
        .max(0.0);
    let visible_bottom = visible_top + visible_bounds.size.height.as_f32().max(0.0);
    rows.iter()
        .filter(|row| row.intersects_y_range(visible_top, visible_bottom))
        .cloned()
        .collect()
}

fn place_section_visual_prepaint(
    input: PlaceSectionVisualState,
    paint_text: bool,
    warm_text_shapes: bool,
    layer_bounds: Bounds<Pixels>,
    app: &WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut App,
) -> PlaceSectionVisualPaintState {
    let line = (paint_text || warm_text_shapes).then(|| {
        let text_style = window.text_style();
        let font_size = px(window.rem_size().as_f32() * 0.75);
        let key = PlacesRowTextShapeCacheKey {
            label: input.label.clone(),
            font: text_style.font(),
            font_size_bits: font_size.as_f32().to_bits(),
            text_color: 0x6b7280,
        };
        let line = app
            .update(cx, |this, _cx| {
                this.place_row_text_shape_cache.shape_for(&key, window)
            })
            .ok()
            .unwrap_or_else(|| Arc::new(shape_place_row_visual_text(&key, window)));
        (key, line)
    });
    let text = if paint_text {
        line.map(|(key, line)| {
            let line_height = px(SECTION_LINE_HEIGHT);
            let (origin, align_width) =
                place_section_text_origin_and_width(layer_bounds, &input, line_height);
            place_text_visual_paint_state(
                key,
                line,
                origin,
                line_height,
                align_width,
                app,
                window,
                cx,
            )
        })
    } else {
        None
    };
    PlaceSectionVisualPaintState {
        input,
        text,
        line_height: px(SECTION_LINE_HEIGHT),
    }
}

fn place_row_visual_prepaint(
    input: PlaceRowVisualState,
    paint_text: bool,
    warm_text_shapes: bool,
    paint_icon: bool,
    layer_bounds: Bounds<Pixels>,
    app: &WeakEntity<FikaApp>,
    icon_cache: Option<&Entity<RetainedImageLayerState>>,
    window: &mut Window,
    cx: &mut App,
) -> PlaceRowVisualPaintState {
    let line = (paint_text || warm_text_shapes).then(|| {
        let text_style = window.text_style();
        let font_size = px(window.rem_size().as_f32() * 0.875);
        let text_color = if input.active {
            0x1f4fbf
        } else if !input.mounted {
            0x6b7280
        } else {
            0x24292f
        };
        let key = PlacesRowTextShapeCacheKey {
            label: input.label.clone(),
            font: text_style.font(),
            font_size_bits: font_size.as_f32().to_bits(),
            text_color,
        };
        let line = app
            .update(cx, |this, _cx| {
                this.place_row_text_shape_cache.shape_for(&key, window)
            })
            .ok()
            .unwrap_or_else(|| Arc::new(shape_place_row_visual_text(&key, window)));
        (key, line)
    });
    let text = if paint_text {
        line.map(|(key, line)| {
            let line_height = px(20.0);
            let (origin, align_width) =
                place_row_text_origin_and_width(layer_bounds, &input, line_height);
            place_text_visual_paint_state(
                key,
                line,
                origin,
                line_height,
                align_width,
                app,
                window,
                cx,
            )
        })
    } else {
        None
    };
    let icon_image = if paint_icon {
        icon_cache.and_then(|cache| {
            cache.update(cx, |cache, cx| {
                load_place_icon_or_retained(cache, &input.icon, app, window, cx)
            })
        })
    } else {
        None
    };
    PlaceRowVisualPaintState {
        input,
        text,
        line_height: px(20.0),
        paint_icon,
        icon_image,
    }
}

fn load_place_icon_or_retained(
    retained_images: &mut RetainedImageLayerState,
    icon: &PlaceRowIconVisualState,
    app: &WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    let request = RetainedImageRequest::theme_icon_for_parts_with_mode(
        icon.path.clone(),
        icon.icon_name.clone(),
        PLACE_ROW_ICON_SIZE.round() as u32,
        window.scale_factor(),
        icon.mode,
    )?;
    let load = retained_images.load_request_or_retained_with_outcome(request, app, window, cx);
    load.image
}

fn shape_place_row_visual_text(
    key: &PlacesRowTextShapeCacheKey,
    window: &mut Window,
) -> gpui::ShapedLine {
    let run = TextRun {
        len: key.label.len(),
        font: key.font.clone(),
        color: rgb(key.text_color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        key.label.clone(),
        px(f32::from_bits(key.font_size_bits)),
        &[run],
        None,
    )
}

fn paint_place_section_visual(
    layer_bounds: Bounds<Pixels>,
    state: &PlaceSectionVisualPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(text) = &state.text else {
        return;
    };
    let (origin, align_width) =
        place_section_text_origin_and_width(layer_bounds, &state.input, state.line_height);
    paint_place_visual_text(text, origin, state.line_height, align_width, window, cx);
}

fn paint_place_row_visual(
    layer_bounds: Bounds<Pixels>,
    state: &PlaceRowVisualPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let input = &state.input;
    let row_bounds = Bounds::new(
        point(layer_bounds.origin.x, layer_bounds.origin.y + px(input.y)),
        size(layer_bounds.size.width, px(PLACE_ROW_HEIGHT)),
    );
    if input.active || input.drop_target {
        let background = place_row_background(input.active, input.drop_target);
        let border_color = place_row_border_color(input.active, input.drop_target);
        window.paint_quad(fill(row_bounds, background).corner_radii(px(6.0)));
        window.paint_quad(
            fill(row_bounds, rgba(0x00000000))
                .corner_radii(px(6.0))
                .border_widths(px(1.0))
                .border_color(border_color),
        );
    }

    if state.paint_icon {
        let icon_bounds = Bounds::new(
            point(
                row_bounds.origin.x + px(ROW_PADDING_X),
                row_bounds.origin.y
                    + ((row_bounds.size.height - px(PLACE_ROW_ICON_SIZE)) / 2.0).floor(),
            ),
            size(px(PLACE_ROW_ICON_SIZE), px(PLACE_ROW_ICON_SIZE)),
        );
        paint_place_row_visual_icon(icon_bounds, &input.icon, state.icon_image.as_ref(), window);
    }

    if let Some(text) = &state.text {
        let (origin, align_width) =
            place_row_text_origin_and_width(layer_bounds, input, state.line_height);
        paint_place_visual_text(text, origin, state.line_height, align_width, window, cx);
    }

    if input.trash_place {
        let dot_x =
            row_bounds.origin.x + row_bounds.size.width - px(ROW_PADDING_X + TRASH_DOT_SIZE);
        let dot_y = row_bounds.origin.y + (row_bounds.size.height - px(TRASH_DOT_SIZE)) / 2.0;
        window.paint_quad(
            fill(
                Bounds::new(
                    point(dot_x, dot_y),
                    size(px(TRASH_DOT_SIZE), px(TRASH_DOT_SIZE)),
                ),
                if input.trash_has_items {
                    rgb(0x2f6fed)
                } else {
                    rgb(0xc8ced6)
                },
            )
            .corner_radii(px(TRASH_DOT_SIZE / 2.0)),
        );
    }

    if input.insert_before || input.insert_after {
        let y = if input.insert_before {
            row_bounds.origin.y
        } else {
            row_bounds.origin.y + row_bounds.size.height - px(INSERT_INDICATOR_HEIGHT)
        };
        window.paint_quad(
            fill(
                Bounds::new(
                    point(row_bounds.origin.x + px(ROW_PADDING_X), y),
                    size(
                        (row_bounds.size.width - px(ROW_PADDING_X * 2.0)).max(px(1.0)),
                        px(INSERT_INDICATOR_HEIGHT),
                    ),
                ),
                rgb(0xd97706),
            )
            .corner_radii(px(1.0)),
        );
    }
}

fn place_section_text_origin_and_width(
    layer_bounds: Bounds<Pixels>,
    input: &PlaceSectionVisualState,
    line_height: Pixels,
) -> (gpui::Point<Pixels>, Option<Pixels>) {
    let section_bounds = Bounds::new(
        point(layer_bounds.origin.x, layer_bounds.origin.y + px(input.y)),
        size(layer_bounds.size.width, px(PLACE_SECTION_HEADING_HEIGHT)),
    );
    let text_y = section_bounds.origin.y
        + ((section_bounds.size.height - line_height).max(px(0.0)) / 2.0).floor();
    (
        point(section_bounds.origin.x + px(SECTION_TEXT_X), text_y),
        Some((section_bounds.size.width - px(SECTION_TEXT_X * 2.0)).max(px(1.0))),
    )
}

fn place_row_text_origin_and_width(
    layer_bounds: Bounds<Pixels>,
    input: &PlaceRowVisualState,
    line_height: Pixels,
) -> (gpui::Point<Pixels>, Option<Pixels>) {
    let row_bounds = Bounds::new(
        point(layer_bounds.origin.x, layer_bounds.origin.y + px(input.y)),
        size(layer_bounds.size.width, px(PLACE_ROW_HEIGHT)),
    );
    let text_left = ROW_PADDING_X + PLACE_ROW_ICON_SIZE + ICON_TEXT_GAP;
    let reserved_right = if input.trash_place {
        ROW_PADDING_X + TRASH_DOT_SIZE + ICON_TEXT_GAP
    } else {
        ROW_PADDING_X
    };
    let text_bounds = Bounds::new(
        point(row_bounds.origin.x + px(text_left), row_bounds.origin.y),
        size(
            (row_bounds.size.width - px(text_left + reserved_right)).max(px(1.0)),
            row_bounds.size.height,
        ),
    );
    let text_y =
        text_bounds.origin.y + ((row_bounds.size.height - line_height).max(px(0.0)) / 2.0).floor();
    (
        point(text_bounds.origin.x, text_y),
        Some(text_bounds.size.width),
    )
}

fn place_text_visual_paint_state(
    text_key: PlacesRowTextShapeCacheKey,
    line: Arc<gpui::ShapedLine>,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align_width: Option<Pixels>,
    app: &WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut App,
) -> PlaceTextVisualPaintState {
    let raster_key = places_row_glyph_raster_cache_key(
        text_key,
        origin,
        line_height,
        align_width,
        window.scale_factor(),
    );
    let raster_data = app
        .update(cx, |this, _cx| {
            this.place_row_text_shape_cache
                .glyph_raster_for(&raster_key)
        })
        .ok()
        .flatten()
        .or_else(|| {
            let raster_data = Arc::new(
                line.compute_glyph_raster_data(
                    origin,
                    line_height,
                    TextAlign::Left,
                    align_width,
                    window,
                    cx,
                )
                .ok()?,
            );
            let _ = app.update(cx, |this, _cx| {
                this.place_row_text_shape_cache
                    .insert_glyph_raster(raster_key, raster_data.clone());
            });
            Some(raster_data)
        });
    PlaceTextVisualPaintState { line, raster_data }
}

fn places_row_glyph_raster_cache_key(
    text: PlacesRowTextShapeCacheKey,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align_width: Option<Pixels>,
    scale_factor: f32,
) -> PlacesRowGlyphRasterCacheKey {
    PlacesRowGlyphRasterCacheKey {
        text,
        origin_x_bits: origin.x.as_f32().to_bits(),
        origin_y_bits: origin.y.as_f32().to_bits(),
        line_height_bits: line_height.as_f32().to_bits(),
        align_width_bits: align_width.map(|width| width.as_f32().to_bits()),
        scale_factor_bits: scale_factor.to_bits(),
    }
}

fn paint_place_visual_text(
    text: &PlaceTextVisualPaintState,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align_width: Option<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(raster_data) = &text.raster_data {
        text.line
            .paint_with_raster_data(
                origin,
                line_height,
                TextAlign::Left,
                align_width,
                raster_data,
                window,
                cx,
            )
            .ok();
    } else {
        text.line
            .paint(
                origin,
                line_height,
                TextAlign::Left,
                align_width,
                window,
                cx,
            )
            .ok();
    }
}

fn paint_place_row_visual_icon(
    icon_bounds: Bounds<Pixels>,
    icon: &PlaceRowIconVisualState,
    image: Option<&Arc<RenderImage>>,
    window: &mut Window,
) {
    if let Some(image) = image
        && image.frame_count() > 0
        && u32::from(image.size(0).width) > 0
        && u32::from(image.size(0).height) > 0
    {
        window
            .paint_image(icon_bounds, Corners::all(px(6.0)), image.clone(), 0, false)
            .ok();
        return;
    }

    window.paint_quad(fill(icon_bounds, rgb(icon.fallback_bg)).corner_radii(px(6.0)));
    match icon.marker.as_ref() {
        "T" => paint_place_trash_icon(icon_bounds, icon.fallback_fg, window),
        "/" | "D" => paint_place_drive_icon(icon_bounds, icon.fallback_fg, window),
        _ => paint_place_folder_icon(icon_bounds, icon.fallback_fg, window),
    }
}

fn paint_place_folder_icon(bounds: Bounds<Pixels>, fg: u32, window: &mut Window) {
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(5.0), bounds.origin.y + px(6.0)),
                size(px(7.0), px(3.0)),
            ),
            rgb(fg),
        )
        .corner_radii(px(1.0)),
    );
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(4.0), bounds.origin.y + px(9.0)),
                size(px(14.0), px(8.0)),
            ),
            rgb(fg),
        )
        .corner_radii(px(2.0)),
    );
}

fn paint_place_drive_icon(bounds: Bounds<Pixels>, fg: u32, window: &mut Window) {
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(5.0), bounds.origin.y + px(6.0)),
                size(px(12.0), px(11.0)),
            ),
            rgb(fg),
        )
        .corner_radii(px(2.0)),
    );
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(7.0), bounds.origin.y + px(13.0)),
                size(px(8.0), px(1.5)),
            ),
            rgb(0xffffff),
        )
        .corner_radii(px(1.0)),
    );
}

fn paint_place_trash_icon(bounds: Bounds<Pixels>, fg: u32, window: &mut Window) {
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(6.0), bounds.origin.y + px(6.0)),
                size(px(10.0), px(2.0)),
            ),
            rgb(fg),
        )
        .corner_radii(px(1.0)),
    );
    window.paint_quad(
        fill(
            Bounds::new(
                point(bounds.origin.x + px(7.0), bounds.origin.y + px(9.0)),
                size(px(8.0), px(9.0)),
            ),
            rgb(fg),
        )
        .corner_radii(px(2.0)),
    );
}

fn place_row_visual_layer_rows_and_height(
    places: &[PlaceSnapshot],
) -> (Vec<PlaceRowVisualState>, Vec<PlaceSectionVisualState>, f32) {
    let mut rows = Vec::with_capacity(places.len());
    let mut sections = Vec::new();
    let mut current_group = None;
    let mut y = 0.0;
    for place in places {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                sections.push(PlaceSectionVisualState::new(place.group, y));
                y += PLACE_SECTION_HEADING_HEIGHT;
            }
        }
        rows.push(PlaceRowVisualState::from_place(place, y));
        y += PLACE_ROW_HEIGHT;
    }
    (rows, sections, y)
}

#[cfg(test)]
fn place_row_visual_layer_rows(places: &[PlaceSnapshot]) -> Vec<PlaceRowVisualState> {
    place_row_visual_layer_rows_and_height(places).0
}

#[cfg(test)]
fn place_row_visual_layer_sections(places: &[PlaceSnapshot]) -> Vec<PlaceSectionVisualState> {
    place_row_visual_layer_rows_and_height(places).1
}

#[cfg(test)]
fn places_row_visual_content_height(places: &[PlaceSnapshot]) -> f32 {
    place_row_visual_layer_rows_and_height(places).2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icons::FileIconSnapshot;
    use std::path::PathBuf;

    #[test]
    fn place_row_visual_suppresses_ordinary_highlight_on_insert_target() {
        let mut place = test_place();
        place.active = true;
        place.drop_target = true;
        place.insert_before = true;
        let state = PlaceRowVisualState::from_place(&place, 0.0);
        assert!(!state.active);
        assert!(!state.drop_target);
        assert!(state.insert_before);
    }

    #[test]
    fn place_row_visual_keeps_trash_marker_state() {
        let mut place = test_place();
        place.trash_place = true;
        place.trash_has_items = true;
        let state = PlaceRowVisualState::from_place(&place, 0.0);
        assert!(state.trash_place);
        assert!(state.trash_has_items);
    }

    #[test]
    fn place_row_visual_layer_offsets_skip_section_headings() {
        let mut first = test_place();
        first.group = "";
        first.label = "Home".to_string();
        let mut second = test_place();
        second.group = "Devices";
        second.label = "Root".to_string();
        let places = [first, second];
        let rows = place_row_visual_layer_rows(&places);
        let sections = place_row_visual_layer_sections(&places);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].y, 0.0);
        assert_eq!(rows[1].y, PLACE_ROW_HEIGHT + PLACE_SECTION_HEADING_HEIGHT);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].label.as_ref(), "Devices");
        assert_eq!(sections[0].y, PLACE_ROW_HEIGHT);
    }

    #[test]
    fn places_row_visual_content_height_matches_rows_and_sections() {
        let mut first = test_place();
        first.group = "";
        let mut second = test_place();
        second.group = "Devices";

        assert_eq!(
            places_row_visual_content_height(&[first, second]),
            PLACE_ROW_HEIGHT * 2.0 + PLACE_SECTION_HEADING_HEIGHT
        );
    }

    #[test]
    fn place_row_visual_prepaint_filters_to_content_mask() {
        let rows = (0..5)
            .map(|index| {
                let mut place = test_place();
                place.label = format!("Place {index}");
                PlaceRowVisualState::from_place(&place, index as f32 * PLACE_ROW_HEIGHT)
            })
            .collect::<Vec<_>>();
        let layer_bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(200.0), px(150.0)));
        let content_mask_bounds = Bounds::new(point(px(10.0), px(50.0)), size(px(200.0), px(60.0)));

        let visible = visible_place_row_visuals(&rows, layer_bounds, content_mask_bounds);

        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].label.as_ref(), "Place 1");
        assert_eq!(visible[1].label.as_ref(), "Place 2");
    }

    fn test_place() -> PlaceSnapshot {
        PlaceSnapshot {
            index: 0,
            group: "",
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: "Home".to_string(),
            path: PathBuf::from("/home/test"),
            device_id: None,
            mounted: true,
            device: false,
            network: false,
            device_ejectable: false,
            device_can_power_off: false,
            active: false,
            drop_target: false,
            insert_before: false,
            insert_after: false,
            trash_place: false,
            trash_has_items: false,
            editable: true,
            removable: true,
        }
    }
}
