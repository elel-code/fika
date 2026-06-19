use std::sync::Arc;

use fika_core::{PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Element, ElementId, Font, FontWeight, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, RenderImage, SharedString, Style, StyleRefinement, Styled,
    TextAlign, TextRun, WeakEntity, Window, fill, point, px, rgb, size,
};

use crate::FikaApp;
use crate::ui::icons::{FileIconSnapshot, theme_icon_image_size_px};
use crate::ui::retained::{
    RetainedImageLayerState, RetainedImageReady, RetainedImageRequest, RetainedShapeCache,
};

use super::details::{DetailsColumn, DetailsColumnKind};
use super::image_layer::{
    ItemImageFallbackPaintState, paint_item_image_fallback, paint_theme_icon_image,
    theme_icon_placeholder_fallback,
};
use super::paint_slots::DetailsPaintSnapshot;
use super::renderer_policy::{DetailsRowVisualRenderer, details_row_renderer_policy};
use super::text::static_paint_single_line_text;
use super::{GlyphRasterMissBudget, ITEM_NAME_LINE_HEIGHT, TextShapeCacheStats};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct DetailsTextShapeCacheKey {
    pub(super) text: SharedString,
    pub(super) font: Font,
    pub(super) font_size_bits: u32,
    pub(super) line_height_bits: u32,
    pub(super) scale_factor_bits: u32,
    pub(super) color: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct DetailsGlyphRasterCacheKey {
    pub(super) text: DetailsTextShapeCacheKey,
    pub(super) origin_x_bits: u32,
    pub(super) origin_y_bits: u32,
    pub(super) line_height_bits: u32,
    pub(super) align_width_bits: Option<u32>,
    pub(super) scale_factor_bits: u32,
}

pub(crate) struct DetailsTextShapeCache {
    cache: RetainedShapeCache<DetailsTextShapeCacheKey, Arc<gpui::ShapedLine>>,
    glyph_cache: RetainedShapeCache<DetailsGlyphRasterCacheKey, Arc<gpui::GlyphRasterData>>,
}

impl DetailsTextShapeCache {
    const MAX_ENTRIES: usize = 4096;

    fn shape_for(
        &mut self,
        key: &DetailsTextShapeCacheKey,
        window: &mut Window,
    ) -> Arc<gpui::ShapedLine> {
        self.cache
            .get_or_insert_with(key, |key| Arc::new(shape_details_visual_text(key, window)))
    }

    fn glyph_raster_for(
        &mut self,
        key: &DetailsGlyphRasterCacheKey,
    ) -> Option<Arc<gpui::GlyphRasterData>> {
        self.glyph_cache.get(key)
    }

    fn insert_glyph_raster(
        &mut self,
        key: DetailsGlyphRasterCacheKey,
        raster_data: Arc<gpui::GlyphRasterData>,
    ) {
        self.glyph_cache.insert(key, raster_data);
    }

    pub(super) fn take_stats(&mut self) -> TextShapeCacheStats {
        self.cache.take_stats()
    }

    pub(super) fn take_glyph_stats(&mut self) -> TextShapeCacheStats {
        self.glyph_cache.take_stats()
    }

    pub(super) fn begin_retention_frame(&mut self) {
        self.cache.begin_retention_frame();
        self.glyph_cache.begin_retention_frame();
    }

    pub(super) fn finish_retention_frame(&mut self) {
        self.cache.finish_retention_frame();
        self.glyph_cache.finish_retention_frame();
    }
}

impl Default for DetailsTextShapeCache {
    fn default() -> Self {
        Self {
            cache: RetainedShapeCache::new(Self::MAX_ENTRIES),
            glyph_cache: RetainedShapeCache::new(Self::MAX_ENTRIES),
        }
    }
}

pub(super) fn details_visual_layer_view(
    pane_id: PaneId,
    items: &[DetailsPaintSnapshot],
    columns: &[DetailsColumn],
    header_height: f32,
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<DetailsVisualLayerElement> {
    let items = details_visual_layer_items(items, columns);
    let header = details_visual_header(columns, header_height);
    (!items.is_empty() || !header.columns.is_empty()).then(|| {
        DetailsVisualLayerElement {
            pane_id,
            app,
            header,
            items,
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

pub(super) fn details_visual_header(columns: &[DetailsColumn], height: f32) -> DetailsVisualHeader {
    let mut x = 0.0;
    let columns = columns
        .iter()
        .map(|column| {
            let column_x = x;
            x += column.width;
            DetailsVisualHeaderColumn {
                x: column_x,
                width: column.width,
                title: SharedString::from(column.title),
            }
        })
        .collect();
    DetailsVisualHeader {
        height: height.max(1.0),
        columns,
    }
}

pub(super) fn details_visual_layer_items(
    items: &[DetailsPaintSnapshot],
    columns: &[DetailsColumn],
) -> Vec<DetailsVisualLayerItem> {
    items
        .iter()
        .filter_map(|item| {
            let policy = details_row_renderer_policy(item);
            if !matches!(policy.visual, DetailsRowVisualRenderer::ContentLayer) {
                return None;
            }
            let mut x = 0.0;
            let cells = columns
                .iter()
                .map(|column| {
                    let cell_x = x;
                    x += column.width;
                    DetailsVisualCell {
                        x: cell_x,
                        width: column.width,
                        content: match column.kind {
                            DetailsColumnKind::Name => DetailsVisualCellContent::Name {
                                name: SharedString::from(item.content.name.as_ref()),
                                icon: item.content.icon.clone(),
                            },
                            DetailsColumnKind::Size => DetailsVisualCellContent::Text {
                                text: SharedString::from(item.content.size_label.as_str()),
                            },
                            DetailsColumnKind::Modified => DetailsVisualCellContent::Text {
                                text: SharedString::from(item.content.modified_label.as_str()),
                            },
                            DetailsColumnKind::OriginalPath => DetailsVisualCellContent::Text {
                                text: SharedString::from(item.content.original_path_label.as_str()),
                            },
                            DetailsColumnKind::DeletionTime => DetailsVisualCellContent::Text {
                                text: SharedString::from(item.content.deletion_time_label.as_str()),
                            },
                        },
                    }
                })
                .collect();
            Some(DetailsVisualLayerItem {
                row_index: item.row_index,
                row_top: f32::from_bits(item.geometry.row_top),
                row_height: f32::from_bits(item.geometry.row_height),
                icon_size: f32::from_bits(item.geometry.icon_size),
                selected: item.visual.selected,
                hovered: item.visual.hovered,
                drop_target: item.visual.drop_target,
                cells,
            })
        })
        .collect()
}

#[derive(Clone)]
pub(super) struct DetailsVisualLayerItem {
    pub(super) row_index: usize,
    pub(super) row_top: f32,
    pub(super) row_height: f32,
    icon_size: f32,
    pub(super) selected: bool,
    pub(super) hovered: bool,
    pub(super) drop_target: bool,
    pub(super) cells: Vec<DetailsVisualCell>,
}

#[derive(Clone)]
pub(super) struct DetailsVisualHeader {
    height: f32,
    pub(super) columns: Vec<DetailsVisualHeaderColumn>,
}

#[derive(Clone)]
pub(super) struct DetailsVisualHeaderColumn {
    x: f32,
    width: f32,
    pub(super) title: SharedString,
}

#[derive(Clone)]
pub(super) struct DetailsVisualCell {
    x: f32,
    width: f32,
    pub(super) content: DetailsVisualCellContent,
}

#[derive(Clone)]
pub(super) enum DetailsVisualCellContent {
    Name {
        name: SharedString,
        icon: FileIconSnapshot,
    },
    Text {
        text: SharedString,
    },
}

pub(super) struct DetailsVisualLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    header: DetailsVisualHeader,
    items: Vec<DetailsVisualLayerItem>,
    style: StyleRefinement,
}

pub(super) struct DetailsVisualLayerPaintState {
    header: DetailsVisualHeaderPaintState,
    rows: Vec<DetailsVisualPaintState>,
}

pub(super) struct DetailsVisualHeaderPaintState {
    height: f32,
    columns: Vec<DetailsVisualHeaderColumnPaintState>,
}

struct DetailsVisualHeaderColumnPaintState {
    x: f32,
    width: f32,
    title: DetailsVisualTextPaintState,
}

pub(super) struct DetailsVisualPaintState {
    row_index: usize,
    row_top: f32,
    row_height: f32,
    selected: bool,
    hovered: bool,
    drop_target: bool,
    cells: Vec<DetailsVisualCellPaintState>,
}

enum DetailsVisualCellPaintState {
    Name {
        icon: DetailsVisualIconPaintState,
        text: DetailsVisualTextPaintState,
    },
    Text(DetailsVisualTextPaintState),
}

struct DetailsVisualIconPaintState {
    rect: ViewRect,
    image: Option<Arc<RenderImage>>,
    fallback: Option<ItemImageFallbackPaintState>,
}

struct DetailsVisualTextPaintState {
    rect: ViewRect,
    line: Arc<gpui::ShapedLine>,
    raster_data: Option<Arc<gpui::GlyphRasterData>>,
    line_height: Pixels,
}

impl IntoElement for DetailsVisualLayerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DetailsVisualLayerElement {
    type RequestLayoutState = Style;
    type PrepaintState = DetailsVisualLayerPaintState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(details_visual_layer_element_id(
            self.pane_id,
        )))
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style.clone(), [], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let _ = self.app.update(cx, |this, _cx| {
            this.begin_details_text_shape_cache_retention_frame(self.pane_id);
        });
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let mut glyph_budget = GlyphRasterMissBudget::visible();
        let (header, rows) = if let Some(id) = id {
            window.with_element_state::<RetainedImageLayerState, _>(id, |state, window| {
                let mut state = state.unwrap_or_else(|| RetainedImageLayerState::new(cx));
                let mut ready_images = Vec::new();
                let header = details_visual_prepaint_header(
                    self.pane_id,
                    &self.header,
                    bounds,
                    &self.app,
                    &mut glyph_budget,
                    window,
                    cx,
                );
                let mut rows = Vec::with_capacity(self.items.len());
                for item in &self.items {
                    rows.push(details_visual_prepaint_item(
                        self.pane_id,
                        item,
                        bounds,
                        Some(&mut state),
                        &self.app,
                        &mut glyph_budget,
                        Some(&mut ready_images),
                        window,
                        cx,
                    ));
                }
                if !ready_images.is_empty() {
                    let _ = self.app.update(cx, |this, cx| {
                        let mut readiness_changed = false;
                        for ready in ready_images {
                            readiness_changed |= this.mark_retained_image_ready(ready);
                        }
                        if readiness_changed {
                            cx.notify();
                        }
                    });
                }
                ((header, rows), state)
            })
        } else {
            let header = details_visual_prepaint_header(
                self.pane_id,
                &self.header,
                bounds,
                &self.app,
                &mut glyph_budget,
                window,
                cx,
            );
            let rows = self
                .items
                .iter()
                .map(|item| {
                    details_visual_prepaint_item(
                        self.pane_id,
                        item,
                        bounds,
                        None,
                        &self.app,
                        &mut glyph_budget,
                        None,
                        window,
                        cx,
                    )
                })
                .collect::<Vec<_>>();
            (header, rows)
        };
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let count = rows.len();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_details_visual_prepaint(self.pane_id, elapsed, count);
            });
        }
        let glyph_budget_stats = glyph_budget.stats();
        if glyph_budget_stats.has_activity() {
            let _ = self.app.update(cx, |this, cx| {
                this.record_details_glyph_budget_stats(self.pane_id, glyph_budget_stats);
                if glyph_budget_stats.deferred > 0 {
                    cx.notify();
                }
            });
        }
        let _ = self.app.update(cx, |this, _cx| {
            this.finish_details_text_shape_cache_retention_frame(self.pane_id);
        });
        DetailsVisualLayerPaintState { header, rows }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.rows.len();
        request_layout.paint(bounds, window, cx, |window, cx| {
            details_visual_paint_header(bounds, &prepaint.header, window, cx);
            for state in prepaint.rows.iter() {
                details_visual_paint_item(bounds, state, window, cx);
            }
        });
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_details_visual_paint(self.pane_id, elapsed, count);
            });
        }
    }
}

impl Styled for DetailsVisualLayerElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

pub(super) fn details_visual_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("details-visual-layer", pane_id.0)
}

const DETAILS_CELL_PADDING_X: f32 = 8.0;
const DETAILS_NAME_ICON_GAP: f32 = 8.0;
const DETAILS_HEADER_TEXT_LINE_HEIGHT: f32 = 16.0;

fn details_visual_prepaint_header(
    pane_id: PaneId,
    header: &DetailsVisualHeader,
    layer_bounds: Bounds<Pixels>,
    app: &WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) -> DetailsVisualHeaderPaintState {
    let mut font = window.text_style().font();
    font.weight = FontWeight::SEMIBOLD;
    let font_size = px(window.rem_size().as_f32() * 0.75);
    let line_height = px(DETAILS_HEADER_TEXT_LINE_HEIGHT);
    let columns = header
        .columns
        .iter()
        .map(|column| {
            let text_rect = ViewRect {
                x: column.x + DETAILS_CELL_PADDING_X,
                y: ((header.height - DETAILS_HEADER_TEXT_LINE_HEIGHT).max(0.0) * 0.5).floor(),
                width: (column.width - DETAILS_CELL_PADDING_X * 2.0).max(1.0),
                height: DETAILS_HEADER_TEXT_LINE_HEIGHT,
            };
            DetailsVisualHeaderColumnPaintState {
                x: column.x,
                width: column.width,
                title: details_visual_text_prepaint(
                    layer_bounds,
                    text_rect,
                    column.title.clone(),
                    0x4b5563,
                    font.clone(),
                    font_size,
                    line_height,
                    pane_id,
                    app,
                    glyph_budget,
                    window,
                    cx,
                ),
            }
        })
        .collect();
    DetailsVisualHeaderPaintState {
        height: header.height,
        columns,
    }
}

fn details_visual_prepaint_item(
    pane_id: PaneId,
    item: &DetailsVisualLayerItem,
    layer_bounds: Bounds<Pixels>,
    mut image_state: Option<&mut RetainedImageLayerState>,
    app: &WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    mut ready_images: Option<&mut Vec<RetainedImageReady>>,
    window: &mut Window,
    cx: &mut App,
) -> DetailsVisualPaintState {
    let font = window.text_style().font();
    let font_size = px(window.rem_size().as_f32() * 0.875);
    let line_height = px(ITEM_NAME_LINE_HEIGHT);
    let mut cells = Vec::with_capacity(item.cells.len());
    for cell in &item.cells {
        let paint_state = match &cell.content {
            DetailsVisualCellContent::Name { name, icon } => {
                let icon_rect = details_visual_name_icon_rect(item, cell);
                let text_rect = details_visual_name_text_rect(item, cell);
                DetailsVisualCellPaintState::Name {
                    icon: details_visual_icon_prepaint(
                        icon_rect,
                        icon,
                        image_state.as_mut().map(|state| &mut **state),
                        app,
                        ready_images.as_deref_mut(),
                        window,
                        cx,
                    ),
                    text: details_visual_text_prepaint(
                        layer_bounds,
                        text_rect,
                        name.clone(),
                        if item.selected { 0x0f172a } else { 0x1f2937 },
                        font.clone(),
                        font_size,
                        line_height,
                        pane_id,
                        app,
                        glyph_budget,
                        window,
                        cx,
                    ),
                }
            }
            DetailsVisualCellContent::Text { text } => {
                DetailsVisualCellPaintState::Text(details_visual_text_prepaint(
                    layer_bounds,
                    details_visual_text_rect(item, cell),
                    text.clone(),
                    0x4b5563,
                    font.clone(),
                    font_size,
                    line_height,
                    pane_id,
                    app,
                    glyph_budget,
                    window,
                    cx,
                ))
            }
        };
        cells.push(paint_state);
    }
    DetailsVisualPaintState {
        row_index: item.row_index,
        row_top: item.row_top,
        row_height: item.row_height,
        selected: item.selected,
        hovered: item.hovered,
        drop_target: item.drop_target,
        cells,
    }
}

fn details_visual_name_icon_rect(
    item: &DetailsVisualLayerItem,
    cell: &DetailsVisualCell,
) -> ViewRect {
    ViewRect {
        x: cell.x + DETAILS_CELL_PADDING_X,
        y: item.row_top + ((item.row_height - item.icon_size).max(0.0) * 0.5).floor(),
        width: item.icon_size.max(1.0),
        height: item.icon_size.max(1.0),
    }
}

fn details_visual_name_text_rect(
    item: &DetailsVisualLayerItem,
    cell: &DetailsVisualCell,
) -> ViewRect {
    let x = cell.x + DETAILS_CELL_PADDING_X + item.icon_size + DETAILS_NAME_ICON_GAP;
    ViewRect {
        x,
        y: item.row_top + ((item.row_height - ITEM_NAME_LINE_HEIGHT).max(0.0) * 0.5).floor(),
        width: (cell.width - (x - cell.x) - DETAILS_CELL_PADDING_X).max(1.0),
        height: ITEM_NAME_LINE_HEIGHT,
    }
}

fn details_visual_text_rect(item: &DetailsVisualLayerItem, cell: &DetailsVisualCell) -> ViewRect {
    ViewRect {
        x: cell.x + DETAILS_CELL_PADDING_X,
        y: item.row_top + ((item.row_height - ITEM_NAME_LINE_HEIGHT).max(0.0) * 0.5).floor(),
        width: (cell.width - DETAILS_CELL_PADDING_X * 2.0).max(1.0),
        height: ITEM_NAME_LINE_HEIGHT,
    }
}

fn details_visual_icon_prepaint(
    rect: ViewRect,
    icon: &FileIconSnapshot,
    image_state: Option<&mut RetainedImageLayerState>,
    app: &WeakEntity<FikaApp>,
    mut ready_images: Option<&mut Vec<RetainedImageReady>>,
    window: &mut Window,
    cx: &mut App,
) -> DetailsVisualIconPaintState {
    let image = icon.path.as_ref().and_then(|path| {
        let state = image_state?;
        let request = RetainedImageRequest::theme_icon_for_snapshot(
            icon,
            theme_icon_image_size_px(rect.width, rect.height),
            window.scale_factor(),
        )?;
        debug_assert_eq!(request.source_path(), path);
        let load = state.load_request_or_retained_with_outcome(request, app, window, cx);
        if let Some(ready) = load.ready {
            if let Some(ready_images) = ready_images.as_deref_mut() {
                ready_images.push(ready);
            }
        }
        load.image
    });
    let fallback = image.is_none().then(|| {
        if icon.path.is_some() {
            theme_icon_placeholder_fallback(icon)
        } else {
            details_visual_icon_fallback_prepaint(rect, icon, window)
        }
    });
    DetailsVisualIconPaintState {
        rect,
        image,
        fallback,
    }
}

fn details_visual_icon_fallback_prepaint(
    rect: ViewRect,
    icon: &FileIconSnapshot,
    window: &mut Window,
) -> ItemImageFallbackPaintState {
    let text_style = window.text_style();
    let mut marker_font = text_style.font();
    marker_font.weight = FontWeight::SEMIBOLD;
    let marker = static_paint_single_line_text(SharedString::from(icon.fallback_marker.as_ref()));
    let marker_run = TextRun {
        len: marker.len(),
        font: marker_font,
        color: rgb(icon.fallback_fg).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let marker_font_size = px(window.rem_size().as_f32() * 0.75);
    ItemImageFallbackPaintState {
        marker_line: Some(window.text_system().shape_line(
            marker,
            marker_font_size,
            &[marker_run],
            None,
        )),
        marker_line_height: px(rect.height.min(ITEM_NAME_LINE_HEIGHT).max(1.0)),
        fallback_bg: icon.fallback_bg,
        placeholder: None,
    }
}

fn details_visual_text_prepaint(
    layer_bounds: Bounds<Pixels>,
    rect: ViewRect,
    text: SharedString,
    color: u32,
    font: Font,
    font_size: Pixels,
    line_height: Pixels,
    pane_id: PaneId,
    app: &WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) -> DetailsVisualTextPaintState {
    let key = details_text_shape_cache_key(text, color, font, font_size, line_height, window);
    let line = app
        .update(cx, |this, _cx| {
            this.details_text_shape_caches
                .entry(pane_id)
                .or_default()
                .shape_for(&key, window)
        })
        .ok()
        .unwrap_or_else(|| Arc::new(shape_details_visual_text(&key, window)));
    let text_bounds = details_visual_bounds(layer_bounds, rect);
    let origin = point(text_bounds.origin.x, text_bounds.origin.y);
    let align_width = Some(text_bounds.size.width);
    let raster_key = details_glyph_raster_cache_key(
        key,
        origin,
        line_height,
        align_width,
        window.scale_factor(),
    );
    let raster_data = details_visual_glyph_raster(
        pane_id,
        raster_key,
        &line,
        origin,
        line_height,
        align_width,
        app,
        glyph_budget,
        window,
        cx,
    );
    DetailsVisualTextPaintState {
        rect,
        line,
        raster_data,
        line_height,
    }
}

#[allow(clippy::too_many_arguments)]
fn details_visual_glyph_raster(
    pane_id: PaneId,
    raster_key: DetailsGlyphRasterCacheKey,
    line: &gpui::ShapedLine,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align_width: Option<Pixels>,
    app: &WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) -> Option<Arc<gpui::GlyphRasterData>> {
    let cached = app
        .update(cx, |this, _cx| {
            this.details_text_shape_caches
                .entry(pane_id)
                .or_default()
                .glyph_raster_for(&raster_key)
        })
        .ok()
        .flatten();
    if cached.is_some() {
        glyph_budget.record_cache_hit();
        return cached;
    }

    glyph_budget.record_cache_miss();
    if !glyph_budget.allow_compute() {
        glyph_budget.record_deferred();
        return None;
    }

    let started = std::time::Instant::now();
    let raster_data = line.compute_glyph_raster_data(
        origin,
        line_height,
        TextAlign::Left,
        align_width,
        window,
        cx,
    );
    let elapsed = started.elapsed();
    let raster_data = match raster_data {
        Ok(raster_data) => {
            glyph_budget.record_compute(elapsed);
            Arc::new(raster_data)
        }
        Err(_) => {
            glyph_budget.record_failed(elapsed);
            return None;
        }
    };
    let _ = app.update(cx, |this, _cx| {
        this.details_text_shape_caches
            .entry(pane_id)
            .or_default()
            .insert_glyph_raster(raster_key, raster_data.clone());
    });
    Some(raster_data)
}

fn details_text_shape_cache_key(
    text: SharedString,
    color: u32,
    font: Font,
    font_size: Pixels,
    line_height: Pixels,
    window: &Window,
) -> DetailsTextShapeCacheKey {
    DetailsTextShapeCacheKey {
        text: static_paint_single_line_text(text),
        font,
        font_size_bits: font_size.as_f32().to_bits(),
        line_height_bits: line_height.as_f32().to_bits(),
        scale_factor_bits: window.scale_factor().to_bits(),
        color,
    }
}

fn shape_details_visual_text(
    key: &DetailsTextShapeCacheKey,
    window: &mut Window,
) -> gpui::ShapedLine {
    let run = TextRun {
        len: key.text.len(),
        font: key.font.clone(),
        color: rgb(key.color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        key.text.clone(),
        px(f32::from_bits(key.font_size_bits)),
        &[run],
        None,
    )
}

fn details_glyph_raster_cache_key(
    text: DetailsTextShapeCacheKey,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align_width: Option<Pixels>,
    scale_factor: f32,
) -> DetailsGlyphRasterCacheKey {
    DetailsGlyphRasterCacheKey {
        text,
        origin_x_bits: origin.x.as_f32().to_bits(),
        origin_y_bits: origin.y.as_f32().to_bits(),
        line_height_bits: line_height.as_f32().to_bits(),
        align_width_bits: align_width.map(|width| width.as_f32().to_bits()),
        scale_factor_bits: scale_factor.to_bits(),
    }
}

fn details_visual_paint_item(
    layer_bounds: Bounds<Pixels>,
    state: &DetailsVisualPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let row_bounds = Bounds::new(
        point(
            layer_bounds.origin.x,
            layer_bounds.origin.y + px(state.row_top),
        ),
        size(layer_bounds.size.width, px(state.row_height.max(1.0))),
    );
    window.paint_quad(fill(
        row_bounds,
        super::details_row_background(
            state.selected,
            state.hovered,
            state.drop_target,
            state.row_index,
        ),
    ));
    for cell in state.cells.iter() {
        match cell {
            DetailsVisualCellPaintState::Name { icon, text } => {
                details_visual_paint_icon(layer_bounds, icon, window, cx);
                details_visual_paint_text(layer_bounds, text, window, cx);
            }
            DetailsVisualCellPaintState::Text(text) => {
                details_visual_paint_text(layer_bounds, text, window, cx);
            }
        }
    }
}

fn details_visual_paint_header(
    layer_bounds: Bounds<Pixels>,
    state: &DetailsVisualHeaderPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let header_bounds = Bounds::new(
        layer_bounds.origin,
        size(layer_bounds.size.width, px(state.height.max(1.0))),
    );
    window.paint_quad(fill(header_bounds, rgb(0xf3f5f8)));
    let bottom = header_bounds.origin.y + header_bounds.size.height - px(1.0);
    window.paint_quad(fill(
        Bounds::new(
            point(header_bounds.origin.x, bottom),
            size(header_bounds.size.width, px(1.0)),
        ),
        rgb(0xd5d9df),
    ));
    for column in &state.columns {
        let right = layer_bounds.origin.x + px((column.x + column.width).round()) - px(1.0);
        window.paint_quad(fill(
            Bounds::new(
                point(right, header_bounds.origin.y),
                size(px(1.0), header_bounds.size.height),
            ),
            rgb(0xe1e5eb),
        ));
        details_visual_paint_text(layer_bounds, &column.title, window, cx);
    }
}

fn details_visual_paint_icon(
    layer_bounds: Bounds<Pixels>,
    state: &DetailsVisualIconPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let icon_bounds = details_visual_bounds(layer_bounds, state.rect);
    if let Some(image) = state.image.as_ref() {
        if paint_theme_icon_image(icon_bounds, image, px(4.0), window) {
            return;
        }
    }
    if let Some(fallback) = state.fallback.as_ref() {
        paint_item_image_fallback(icon_bounds, fallback, px(4.0), window, cx);
    }
}

fn details_visual_paint_text(
    layer_bounds: Bounds<Pixels>,
    state: &DetailsVisualTextPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let text_bounds = details_visual_bounds(layer_bounds, state.rect);
    let origin = point(text_bounds.origin.x, text_bounds.origin.y);
    let align_width = Some(text_bounds.size.width);
    if let Some(raster_data) = &state.raster_data {
        if state
            .line
            .paint_with_raster_data(
                origin,
                state.line_height,
                TextAlign::Left,
                align_width,
                raster_data,
                window,
                cx,
            )
            .is_ok()
        {
            return;
        }
    }
    state
        .line
        .paint(
            origin,
            state.line_height,
            TextAlign::Left,
            align_width,
            window,
            cx,
        )
        .ok();
}

fn details_visual_bounds(layer_bounds: Bounds<Pixels>, rect: ViewRect) -> Bounds<Pixels> {
    Bounds::new(
        point(
            layer_bounds.origin.x + px(rect.x.round()),
            layer_bounds.origin.y + px(rect.y.round()),
        ),
        size(
            px(rect.width.round().max(1.0)),
            px(rect.height.round().max(1.0)),
        ),
    )
}
