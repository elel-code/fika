use std::sync::Arc;

use fika_core::{ItemId, ItemLayout, PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Element, ElementId, Font, FontWeight, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Rgba, SharedString, Style, StyleRefinement, Styled, TextAlign,
    TextRun, WeakEntity, Window, fill, point, px, rgb, size,
};

use crate::FikaApp;
use crate::ui::icons::FileIconSnapshot;
use crate::ui::retained::RetainedShapeCache;

use super::layout::elide_middle_text_for_width;
use super::paint_slots::ItemPaintSnapshot;
use super::renderer_policy::{item_paints_fallback_icon, item_uses_layer_visual_paint};
use super::text::static_paint_single_line_text;
use super::{
    FileGridRenderSnapshot, GlyphRasterMissBudget, ITEM_NAME_LINE_HEIGHT, ItemTileTextAlignment,
    TextShapeCacheStats,
};

pub(super) struct StaticItemVisualPaintState {
    visible: bool,
    layout: ItemLayout,
    marker_line_height: Pixels,
    shapes: Arc<StaticItemTextShapes>,
    glyph_rasters: StaticItemGlyphRasterPaintState,
    label_line_height: Pixels,
    background: Option<Rgba>,
    paint_fallback_icon: bool,
    fallback_bg: u32,
}

struct StaticItemTextShapes {
    marker_line: Option<gpui::ShapedLine>,
    label: StaticItemLabelPaintState,
}

struct StaticItemGlyphRasterPaintState {
    marker_line: Option<Arc<gpui::GlyphRasterData>>,
    label: StaticItemLabelGlyphRasterPaintState,
}

enum StaticItemLabelPaintState {
    Start { line: gpui::ShapedLine },
    Center { lines: Arc<[gpui::ShapedLine]> },
}

enum StaticItemLabelGlyphRasterPaintState {
    Start(Option<Arc<gpui::GlyphRasterData>>),
    Center(Vec<Option<Arc<gpui::GlyphRasterData>>>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct StaticItemTextShapeCacheKey {
    pub(super) text_alignment: ItemTileTextAlignment,
    pub(super) paint_fallback_icon: bool,
    pub(super) text_font: Font,
    pub(super) marker_font: Font,
    pub(super) text_font_size_bits: u32,
    pub(super) marker_font_size_bits: u32,
    pub(super) text_width_bits: u32,
    pub(super) scale_factor_bits: u32,
    pub(super) text_color: u32,
    pub(super) fallback_fg: u32,
    pub(super) fallback_marker: SharedString,
    pub(super) label: StaticItemLabelTextKey,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum StaticItemLabelTextKey {
    Start(SharedString),
    Center(Vec<SharedString>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct StaticItemGlyphRasterCacheKey {
    pub(super) text: StaticItemTextShapeCacheKey,
    pub(super) segment: StaticItemGlyphRasterSegmentKey,
    pub(super) origin_x_bits: u32,
    pub(super) origin_y_bits: u32,
    pub(super) line_height_bits: u32,
    pub(super) align_width_bits: Option<u32>,
    pub(super) scale_factor_bits: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum StaticItemGlyphRasterSegmentKey {
    Marker,
    StartLabel { line_index: usize },
    CenterLabel { line_index: usize },
}

#[derive(Clone, Debug)]
struct StaticItemTextShapeStyle {
    text_font: Font,
    marker_font: Font,
    text_font_size: Pixels,
    marker_font_size: Pixels,
    label_line_height: Pixels,
    marker_line_height: Pixels,
    text_color: u32,
    fallback_fg: u32,
}

pub(crate) struct StaticItemTextShapeCache {
    cache: RetainedShapeCache<StaticItemTextShapeCacheKey, Arc<StaticItemTextShapes>>,
    glyph_cache: RetainedShapeCache<StaticItemGlyphRasterCacheKey, Arc<gpui::GlyphRasterData>>,
}

impl StaticItemTextShapeCache {
    const MAX_ENTRIES: usize = 2048;
    const MAX_GLYPH_ENTRIES: usize = 8192;
    const SHAPE_RETENTION_FRAMES: u64 = 6;

    fn shape_for(
        &mut self,
        key: &StaticItemTextShapeCacheKey,
        style: &StaticItemTextShapeStyle,
        window: &mut Window,
    ) -> Arc<StaticItemTextShapes> {
        self.cache.get_or_insert_with(key, |key| {
            Arc::new(shape_static_item_text(key, style, window))
        })
    }

    fn shape_if_cached(
        &mut self,
        key: &StaticItemTextShapeCacheKey,
    ) -> Option<Arc<StaticItemTextShapes>> {
        self.cache.peek_and_touch(key)
    }

    fn glyph_raster_for(
        &mut self,
        key: &StaticItemGlyphRasterCacheKey,
    ) -> Option<Arc<gpui::GlyphRasterData>> {
        self.glyph_cache.get(key)
    }

    fn insert_glyph_raster(
        &mut self,
        key: StaticItemGlyphRasterCacheKey,
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

impl Default for StaticItemTextShapeCache {
    fn default() -> Self {
        Self {
            cache: RetainedShapeCache::new_with_retention_frame_window(
                Self::MAX_ENTRIES,
                Self::SHAPE_RETENTION_FRAMES,
            ),
            glyph_cache: RetainedShapeCache::new(Self::MAX_GLYPH_ENTRIES),
        }
    }
}

pub(super) fn static_item_visual_layer_view(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
    width: f32,
    height: f32,
    text_alignment: ItemTileTextAlignment,
    finish_retention_after_prepaint: bool,
    app: WeakEntity<FikaApp>,
) -> Option<StaticItemVisualLayerElement> {
    let items = static_item_visual_layer_items(items, text_alignment);
    (!items.is_empty()).then(|| {
        StaticItemVisualLayerElement {
            pane_id,
            app,
            items,
            warm_only: false,
            finish_retention_after_prepaint,
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

pub(super) fn static_item_visual_warm_layer_view(
    pane_id: PaneId,
    snapshot: &FileGridRenderSnapshot,
    app: WeakEntity<FikaApp>,
) -> Option<StaticItemVisualLayerElement> {
    let (items, width, height, text_alignment) = match snapshot {
        FileGridRenderSnapshot::Compact { layout, items } => (
            items,
            layout.content_size().width,
            layout.content_size().height,
            ItemTileTextAlignment::Start,
        ),
        FileGridRenderSnapshot::Icons { layout, items } => (
            items,
            layout.content_size().width,
            layout.content_size().height,
            ItemTileTextAlignment::Center,
        ),
        FileGridRenderSnapshot::Details { .. } => return None,
    };
    let items = static_item_visual_layer_items(items, text_alignment);
    (!items.is_empty()).then(|| {
        StaticItemVisualLayerElement {
            pane_id,
            app,
            items,
            warm_only: true,
            finish_retention_after_prepaint: true,
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

pub(super) fn static_item_visual_layer_items(
    items: &[ItemPaintSnapshot],
    text_alignment: ItemTileTextAlignment,
) -> Vec<StaticItemVisualLayerItem> {
    items
        .iter()
        .filter(|item| item.visible)
        .filter_map(|item| {
            let content = item.content.as_ref();
            item_uses_layer_visual_paint(content).then(|| StaticItemVisualLayerItem {
                item_id: item.item_id,
                visible: item.visible,
                display_name: content.display_name.clone(),
                icon_name_lines: content.icon_name_lines.clone(),
                icon: content.icon.clone(),
                fallback_marker: content.fallback_marker.clone(),
                layout: item.layout,
                text_alignment,
                selected: item.visual.selected,
                hovered: item.visual.hovered,
                drop_target: item.visual.drop_target,
                paint_fallback_icon: item_paints_fallback_icon(content),
            })
        })
        .collect()
}

pub(super) struct StaticItemVisualLayerItem {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) item_id: ItemId,
    visible: bool,
    display_name: SharedString,
    icon_name_lines: Arc<[SharedString]>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    selected: bool,
    hovered: bool,
    drop_target: bool,
    pub(super) paint_fallback_icon: bool,
}

pub(super) struct StaticItemVisualLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<StaticItemVisualLayerItem>,
    warm_only: bool,
    finish_retention_after_prepaint: bool,
    style: StyleRefinement,
}

impl IntoElement for StaticItemVisualLayerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for StaticItemVisualLayerElement {
    type RequestLayoutState = Style;
    type PrepaintState = Vec<StaticItemVisualPaintState>;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(if self.warm_only {
            static_item_visual_warm_layer_element_id(self.pane_id)
        } else {
            static_item_visual_layer_element_id(self.pane_id)
        }))
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
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if !self.warm_only {
            let _ = self.app.update(cx, |this, _cx| {
                this.begin_static_item_text_shape_cache_retention_frame(self.pane_id);
            });
        }
        if self.warm_only {
            let mut glyph_budget = GlyphRasterMissBudget::read_ahead();
            for item in &self.items {
                static_item_visual_warm_prepaint(
                    self.pane_id,
                    bounds,
                    item.display_name.clone(),
                    item.icon_name_lines.clone(),
                    item.icon.clone(),
                    item.fallback_marker.clone(),
                    item.layout,
                    item.text_alignment,
                    item.selected,
                    item.paint_fallback_icon,
                    self.app.clone(),
                    &mut glyph_budget,
                    window,
                    cx,
                );
            }
            let glyph_budget_stats = glyph_budget.stats();
            if glyph_budget_stats.has_activity() {
                let _ = self.app.update(cx, |this, cx| {
                    this.record_static_item_glyph_budget_stats(self.pane_id, glyph_budget_stats);
                    if glyph_budget_stats.deferred > 0 {
                        cx.notify();
                    }
                });
            }
            if self.finish_retention_after_prepaint {
                let _ = self.app.update(cx, |this, _cx| {
                    this.finish_static_item_text_shape_cache_retention_frame(self.pane_id);
                });
            }
            return Vec::new();
        }
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let mut glyph_budget = GlyphRasterMissBudget::visible();
        let states = self
            .items
            .iter()
            .map(|item| {
                static_item_visual_prepaint(
                    self.pane_id,
                    bounds,
                    item.visible,
                    item.display_name.clone(),
                    item.icon_name_lines.clone(),
                    item.icon.clone(),
                    item.fallback_marker.clone(),
                    item.layout,
                    item.text_alignment,
                    item.selected,
                    item.hovered,
                    item.drop_target,
                    item.paint_fallback_icon,
                    self.app.clone(),
                    &mut glyph_budget,
                    window,
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let glyph_budget_stats = glyph_budget.stats();
        if glyph_budget_stats.has_activity() {
            let _ = self.app.update(cx, |this, cx| {
                this.record_static_item_glyph_budget_stats(self.pane_id, glyph_budget_stats);
                if glyph_budget_stats.deferred > 0 {
                    cx.notify();
                }
            });
        }
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let count = states.len();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_static_item_visual_prepaint(self.pane_id, elapsed, count);
            });
        }
        if self.finish_retention_after_prepaint {
            let _ = self.app.update(cx, |this, _cx| {
                this.finish_static_item_text_shape_cache_retention_frame(self.pane_id);
            });
        }
        states
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
        if self.warm_only {
            return;
        }
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.len();
        request_layout.paint(bounds, window, cx, |window, cx| {
            for state in prepaint.iter() {
                if !state.visible {
                    continue;
                }
                let visual = state.layout.visual_rect;
                let item_bounds = Bounds::new(
                    point(
                        bounds.origin.x + px(visual.x),
                        bounds.origin.y + px(visual.y),
                    ),
                    size(px(visual.width.max(1.0)), px(visual.height.max(1.0))),
                );
                static_item_visual_paint(item_bounds, state, window, cx);
            }
        });
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_static_item_visual_paint(self.pane_id, elapsed, count);
            });
        }
    }
}

impl Styled for StaticItemVisualLayerElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

pub(super) fn static_item_visual_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("static-item-visual-layer", pane_id.0)
}

pub(super) fn static_item_visual_warm_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("static-item-visual-warm-layer", pane_id.0)
}

#[allow(clippy::too_many_arguments)]
fn static_item_visual_warm_prepaint(
    pane_id: PaneId,
    layer_bounds: Bounds<Pixels>,
    display_name: SharedString,
    icon_name_lines: Arc<[SharedString]>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    selected: bool,
    paint_fallback_icon: bool,
    app: WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) {
    let style = static_item_text_shape_style(layout, selected, &icon, window);
    let key = static_item_text_shape_cache_key(
        display_name,
        icon_name_lines,
        fallback_marker,
        paint_fallback_icon,
        &icon,
        layout,
        text_alignment,
        &style,
        window,
    );
    let Some(shapes) = app
        .update(cx, |this, _cx| {
            this.static_item_text_shape_caches
                .entry(pane_id)
                .or_default()
                .shape_if_cached(&key)
        })
        .ok()
        .flatten()
    else {
        return;
    };
    let item_bounds = static_item_item_bounds(layer_bounds, layout);
    let _ = static_item_glyph_raster_prepaint(
        pane_id,
        &key,
        &shapes,
        layout,
        item_bounds,
        &style,
        &app,
        glyph_budget,
        window,
        cx,
    );
}

fn static_item_visual_prepaint(
    pane_id: PaneId,
    layer_bounds: Bounds<Pixels>,
    visible: bool,
    display_name: SharedString,
    icon_name_lines: Arc<[SharedString]>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    selected: bool,
    hovered: bool,
    drop_target: bool,
    paint_fallback_icon: bool,
    app: WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) -> StaticItemVisualPaintState {
    let style = static_item_text_shape_style(layout, selected, &icon, window);
    let key = static_item_text_shape_cache_key(
        display_name,
        icon_name_lines,
        fallback_marker,
        paint_fallback_icon,
        &icon,
        layout,
        text_alignment,
        &style,
        window,
    );
    let shapes = app
        .update(cx, |this, _cx| {
            this.static_item_text_shape_caches
                .entry(pane_id)
                .or_default()
                .shape_for(&key, &style, window)
        })
        .ok()
        .unwrap_or_else(|| Arc::new(shape_static_item_text(&key, &style, window)));
    let item_bounds = static_item_item_bounds(layer_bounds, layout);
    let glyph_rasters = static_item_glyph_raster_prepaint(
        pane_id,
        &key,
        &shapes,
        layout,
        item_bounds,
        &style,
        &app,
        glyph_budget,
        window,
        cx,
    );
    StaticItemVisualPaintState {
        visible,
        layout,
        marker_line_height: style.marker_line_height,
        shapes,
        glyph_rasters,
        label_line_height: style.label_line_height,
        background: (selected || drop_target || hovered)
            .then(|| super::item_tile_background(selected, drop_target, hovered)),
        paint_fallback_icon,
        fallback_bg: icon.fallback_bg,
    }
}

fn static_item_text_shape_style(
    layout: ItemLayout,
    selected: bool,
    icon: &FileIconSnapshot,
    window: &Window,
) -> StaticItemTextShapeStyle {
    let text_style = window.text_style();
    let text_font = text_style.font();
    let mut marker_font = text_style.font();
    marker_font.weight = FontWeight::SEMIBOLD;
    StaticItemTextShapeStyle {
        text_font,
        marker_font,
        text_font_size: px(window.rem_size().as_f32() * 0.875),
        marker_font_size: px(window.rem_size().as_f32() * 0.75),
        label_line_height: px(ITEM_NAME_LINE_HEIGHT),
        marker_line_height: px(layout.icon_rect.height.min(ITEM_NAME_LINE_HEIGHT).max(1.0)),
        text_color: if selected { 0x0f172a } else { 0x24292f },
        fallback_fg: icon.fallback_fg,
    }
}

fn static_item_text_shape_cache_key(
    display_name: SharedString,
    icon_name_lines: Arc<[SharedString]>,
    fallback_marker: SharedString,
    paint_fallback_icon: bool,
    icon: &FileIconSnapshot,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    style: &StaticItemTextShapeStyle,
    window: &Window,
) -> StaticItemTextShapeCacheKey {
    let max_lines = (layout.text_rect.height / ITEM_NAME_LINE_HEIGHT)
        .round()
        .max(1.0) as usize;
    let label = match text_alignment {
        ItemTileTextAlignment::Start => StaticItemLabelTextKey::Start(display_name),
        ItemTileTextAlignment::Center => {
            let lines = if icon_name_lines.is_empty() {
                vec![display_name]
            } else {
                icon_name_lines.iter().take(max_lines).cloned().collect()
            };
            StaticItemLabelTextKey::Center(lines)
        }
    };
    let text_width_bits = match text_alignment {
        ItemTileTextAlignment::Start => layout.text_rect.width.to_bits(),
        ItemTileTextAlignment::Center => 0,
    };
    StaticItemTextShapeCacheKey {
        text_alignment,
        paint_fallback_icon,
        text_font: style.text_font.clone(),
        marker_font: style.marker_font.clone(),
        text_font_size_bits: style.text_font_size.as_f32().to_bits(),
        marker_font_size_bits: style.marker_font_size.as_f32().to_bits(),
        text_width_bits,
        scale_factor_bits: window.scale_factor().to_bits(),
        text_color: style.text_color,
        fallback_fg: if paint_fallback_icon {
            icon.fallback_fg
        } else {
            0
        },
        fallback_marker: if paint_fallback_icon {
            fallback_marker
        } else {
            SharedString::from("")
        },
        label,
    }
}

fn shape_static_item_text(
    key: &StaticItemTextShapeCacheKey,
    style: &StaticItemTextShapeStyle,
    window: &mut Window,
) -> StaticItemTextShapes {
    let marker_line = key.paint_fallback_icon.then(|| {
        let fallback_marker = static_paint_single_line_text(key.fallback_marker.clone());
        let marker_run = TextRun {
            len: fallback_marker.len(),
            font: style.marker_font.clone(),
            color: rgb(style.fallback_fg).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(
            fallback_marker,
            style.marker_font_size,
            &[marker_run],
            None,
        )
    });
    let label = match &key.label {
        StaticItemLabelTextKey::Start(display_name) => {
            let max_width = f32::from_bits(key.text_width_bits).max(1.0);
            let display_name = static_paint_single_line_text(display_name.clone());
            let display_name =
                SharedString::from(elide_middle_text_for_width(&display_name, max_width));
            let run = TextRun {
                len: display_name.len(),
                font: style.text_font.clone(),
                color: rgb(style.text_color).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line =
                window
                    .text_system()
                    .shape_line(display_name, style.text_font_size, &[run], None);
            StaticItemLabelPaintState::Start { line }
        }
        StaticItemLabelTextKey::Center(label_texts) => {
            let lines = label_texts
                .iter()
                .cloned()
                .map(static_paint_single_line_text)
                .map(|line| {
                    let run = TextRun {
                        len: line.len(),
                        font: style.text_font.clone(),
                        color: rgb(style.text_color).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(line, style.text_font_size, &[run], None)
                })
                .collect::<Vec<_>>();
            StaticItemLabelPaintState::Center {
                lines: lines.into(),
            }
        }
    };
    StaticItemTextShapes { marker_line, label }
}

fn static_item_glyph_raster_prepaint(
    pane_id: PaneId,
    text_key: &StaticItemTextShapeCacheKey,
    shapes: &StaticItemTextShapes,
    layout: ItemLayout,
    item_bounds: Bounds<Pixels>,
    style: &StaticItemTextShapeStyle,
    app: &WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) -> StaticItemGlyphRasterPaintState {
    let icon_bounds = static_item_local_bounds(item_bounds, layout.visual_rect, layout.icon_rect);
    let marker_line = shapes.marker_line.as_ref().and_then(|line| {
        let marker_origin = point(
            icon_bounds.origin.x,
            icon_bounds.origin.y
                + ((icon_bounds.size.height - style.marker_line_height).max(px(0.0)) / 2.0),
        );
        static_item_shaped_glyph_raster(
            pane_id,
            text_key,
            StaticItemGlyphRasterSegmentKey::Marker,
            line,
            marker_origin,
            style.marker_line_height,
            TextAlign::Center,
            Some(icon_bounds.size.width),
            app,
            glyph_budget,
            window,
            cx,
        )
    });

    let text_bounds = static_item_local_bounds(item_bounds, layout.visual_rect, layout.text_rect);
    let label = match &shapes.label {
        StaticItemLabelPaintState::Start { line } => {
            let origin = static_item_start_label_origin(text_bounds, style.label_line_height);
            let raster = static_item_shaped_glyph_raster(
                pane_id,
                text_key,
                StaticItemGlyphRasterSegmentKey::StartLabel { line_index: 0 },
                line,
                origin,
                style.label_line_height,
                TextAlign::Left,
                Some(text_bounds.size.width),
                app,
                glyph_budget,
                window,
                cx,
            );
            StaticItemLabelGlyphRasterPaintState::Start(raster)
        }
        StaticItemLabelPaintState::Center { lines } => {
            let height =
                (lines.len() as f32 * ITEM_NAME_LINE_HEIGHT).min(text_bounds.size.height.as_f32());
            let mut y = text_bounds.origin.y
                + px(((text_bounds.size.height.as_f32() - height).max(0.0) * 0.5).floor());
            let mut rasters = Vec::with_capacity(lines.len());
            for (line_index, line) in lines.iter().enumerate() {
                rasters.push(static_item_shaped_glyph_raster(
                    pane_id,
                    text_key,
                    StaticItemGlyphRasterSegmentKey::CenterLabel { line_index },
                    line,
                    point(text_bounds.origin.x, y),
                    style.label_line_height,
                    TextAlign::Center,
                    Some(text_bounds.size.width),
                    app,
                    glyph_budget,
                    window,
                    cx,
                ));
                y += style.label_line_height;
            }
            StaticItemLabelGlyphRasterPaintState::Center(rasters)
        }
    };

    StaticItemGlyphRasterPaintState { marker_line, label }
}

#[allow(clippy::too_many_arguments)]
fn static_item_shaped_glyph_raster(
    pane_id: PaneId,
    text_key: &StaticItemTextShapeCacheKey,
    segment: StaticItemGlyphRasterSegmentKey,
    line: &gpui::ShapedLine,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align: TextAlign,
    align_width: Option<Pixels>,
    app: &WeakEntity<FikaApp>,
    glyph_budget: &mut GlyphRasterMissBudget,
    window: &mut Window,
    cx: &mut App,
) -> Option<Arc<gpui::GlyphRasterData>> {
    let raster_key = static_item_glyph_raster_cache_key(
        text_key.clone(),
        segment,
        origin,
        line_height,
        align_width,
        window.scale_factor(),
    );
    let cached = app
        .update(cx, |this, _cx| {
            this.static_item_text_shape_caches
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
    let raster_data =
        line.compute_glyph_raster_data(origin, line_height, align, align_width, window, cx);
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
        this.static_item_text_shape_caches
            .entry(pane_id)
            .or_default()
            .insert_glyph_raster(raster_key, raster_data.clone());
    });
    Some(raster_data)
}

fn static_item_glyph_raster_cache_key(
    text: StaticItemTextShapeCacheKey,
    segment: StaticItemGlyphRasterSegmentKey,
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    align_width: Option<Pixels>,
    scale_factor: f32,
) -> StaticItemGlyphRasterCacheKey {
    StaticItemGlyphRasterCacheKey {
        text,
        segment,
        origin_x_bits: origin.x.as_f32().to_bits(),
        origin_y_bits: origin.y.as_f32().to_bits(),
        line_height_bits: line_height.as_f32().to_bits(),
        align_width_bits: align_width.map(|width| width.as_f32().to_bits()),
        scale_factor_bits: scale_factor.to_bits(),
    }
}

fn static_item_visual_paint(
    bounds: Bounds<Pixels>,
    state: &StaticItemVisualPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(background) = state.background {
        window.paint_quad(fill(bounds, background).corner_radii(px(6.0)));
    }
    let icon_bounds =
        static_item_local_bounds(bounds, state.layout.visual_rect, state.layout.icon_rect);
    if state.paint_fallback_icon {
        window.paint_quad(fill(icon_bounds, rgb(state.fallback_bg)).corner_radii(px(6.0)));
        let marker_origin = point(
            icon_bounds.origin.x,
            icon_bounds.origin.y
                + ((icon_bounds.size.height - state.marker_line_height).max(px(0.0)) / 2.0),
        );
        if let Some(marker_line) = &state.shapes.marker_line {
            let painted_with_raster =
                state
                    .glyph_rasters
                    .marker_line
                    .as_ref()
                    .is_some_and(|raster_data| {
                        marker_line
                            .paint_with_raster_data(
                                marker_origin,
                                state.marker_line_height,
                                TextAlign::Center,
                                Some(icon_bounds.size.width),
                                raster_data,
                                window,
                                cx,
                            )
                            .is_ok()
                    });
            if !painted_with_raster {
                marker_line
                    .paint(
                        marker_origin,
                        state.marker_line_height,
                        TextAlign::Center,
                        Some(icon_bounds.size.width),
                        window,
                        cx,
                    )
                    .ok();
            }
        }
    }

    let text_bounds =
        static_item_local_bounds(bounds, state.layout.visual_rect, state.layout.text_rect);
    match &state.shapes.label {
        StaticItemLabelPaintState::Start { line } => {
            let raster = match &state.glyph_rasters.label {
                StaticItemLabelGlyphRasterPaintState::Start(raster) => raster.as_ref(),
                StaticItemLabelGlyphRasterPaintState::Center(_) => None,
            };
            let origin = static_item_start_label_origin(text_bounds, state.label_line_height);
            let painted_with_raster = raster.is_some_and(|raster_data| {
                line.paint_with_raster_data(
                    origin,
                    state.label_line_height,
                    TextAlign::Left,
                    Some(text_bounds.size.width),
                    raster_data,
                    window,
                    cx,
                )
                .is_ok()
            });
            if !painted_with_raster {
                line.paint(
                    origin,
                    state.label_line_height,
                    TextAlign::Left,
                    Some(text_bounds.size.width),
                    window,
                    cx,
                )
                .ok();
            }
        }
        StaticItemLabelPaintState::Center { lines } => {
            let rasters = match &state.glyph_rasters.label {
                StaticItemLabelGlyphRasterPaintState::Center(rasters) => Some(rasters),
                StaticItemLabelGlyphRasterPaintState::Start(_) => None,
            };
            let height =
                (lines.len() as f32 * ITEM_NAME_LINE_HEIGHT).min(text_bounds.size.height.as_f32());
            let mut y = text_bounds.origin.y
                + px(((text_bounds.size.height.as_f32() - height).max(0.0) * 0.5).floor());
            for (line_index, line) in lines.iter().enumerate() {
                let origin = point(text_bounds.origin.x, y);
                let painted_with_raster = rasters
                    .and_then(|rasters| rasters.get(line_index))
                    .and_then(Option::as_ref)
                    .is_some_and(|raster_data| {
                        line.paint_with_raster_data(
                            origin,
                            state.label_line_height,
                            TextAlign::Center,
                            Some(text_bounds.size.width),
                            raster_data,
                            window,
                            cx,
                        )
                        .is_ok()
                    });
                if !painted_with_raster {
                    line.paint(
                        origin,
                        state.label_line_height,
                        TextAlign::Center,
                        Some(text_bounds.size.width),
                        window,
                        cx,
                    )
                    .ok();
                }
                y += state.label_line_height;
            }
        }
    }
}

fn static_item_start_label_origin(
    text_bounds: Bounds<Pixels>,
    line_height: Pixels,
) -> gpui::Point<Pixels> {
    let y_offset =
        ((text_bounds.size.height.as_f32() - line_height.as_f32()).max(0.0) * 0.5).floor();
    point(text_bounds.origin.x, text_bounds.origin.y + px(y_offset))
}

fn static_item_item_bounds(layer_bounds: Bounds<Pixels>, layout: ItemLayout) -> Bounds<Pixels> {
    let visual = layout.visual_rect;
    Bounds::new(
        point(
            layer_bounds.origin.x + px(visual.x),
            layer_bounds.origin.y + px(visual.y),
        ),
        size(px(visual.width.max(1.0)), px(visual.height.max(1.0))),
    )
}

fn static_item_local_bounds(
    base: Bounds<Pixels>,
    visual_rect: ViewRect,
    rect: ViewRect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            base.origin.x + px(rect.x - visual_rect.x),
            base.origin.y + px(rect.y - visual_rect.y),
        ),
        size(px(rect.width.max(1.0)), px(rect.height.max(1.0))),
    )
}
