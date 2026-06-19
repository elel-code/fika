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

use super::paint_slots::ItemPaintSnapshot;
use super::renderer_policy::{item_paints_fallback_icon, item_uses_layer_visual_paint};
use super::text::static_paint_single_line_text;
use super::{
    FileGridRenderSnapshot, ITEM_NAME_LINE_HEIGHT, ItemTileTextAlignment, TextShapeCacheStats,
};

pub(super) struct StaticItemVisualPaintState {
    visible: bool,
    layout: ItemLayout,
    marker_line_height: Pixels,
    shapes: Arc<StaticItemTextShapes>,
    label_line_height: Pixels,
    background: Option<Rgba>,
    paint_fallback_icon: bool,
    fallback_bg: u32,
}

struct StaticItemTextShapes {
    marker_line: Option<gpui::ShapedLine>,
    label: StaticItemLabelPaintState,
}

enum StaticItemLabelPaintState {
    Start {
        lines: Arc<[gpui::WrappedLine]>,
        height: f32,
    },
    Center {
        lines: Arc<[gpui::ShapedLine]>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct StaticItemTextShapeCacheKey {
    pub(super) text_alignment: ItemTileTextAlignment,
    pub(super) paint_fallback_icon: bool,
    pub(super) text_font: Font,
    pub(super) marker_font: Font,
    pub(super) text_font_size_bits: u32,
    pub(super) marker_font_size_bits: u32,
    pub(super) label_line_height_bits: u32,
    pub(super) marker_line_height_bits: u32,
    pub(super) text_width_bits: u32,
    pub(super) text_height_bits: u32,
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
}

impl StaticItemTextShapeCache {
    const MAX_ENTRIES: usize = 2048;

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

    pub(super) fn take_stats(&mut self) -> TextShapeCacheStats {
        self.cache.take_stats()
    }
}

impl Default for StaticItemTextShapeCache {
    fn default() -> Self {
        Self {
            cache: RetainedShapeCache::new(Self::MAX_ENTRIES),
        }
    }
}

pub(super) fn static_item_visual_layer_view(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
    width: f32,
    height: f32,
    text_alignment: ItemTileTextAlignment,
    app: WeakEntity<FikaApp>,
) -> Option<StaticItemVisualLayerElement> {
    let items = static_item_visual_layer_items(items, text_alignment);
    (!items.is_empty()).then(|| {
        StaticItemVisualLayerElement {
            pane_id,
            app,
            items,
            warm_only: false,
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
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let perf_started = if self.warm_only {
            None
        } else {
            super::item_view_perf_enabled().then(std::time::Instant::now)
        };
        let states = self
            .items
            .iter()
            .map(|item| {
                static_item_visual_prepaint(
                    self.pane_id,
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
                    window,
                    cx,
                )
            })
            .collect::<Vec<_>>();
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let count = states.len();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_static_item_visual_prepaint(self.pane_id, elapsed, count);
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

fn static_item_visual_prepaint(
    pane_id: PaneId,
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
    StaticItemVisualPaintState {
        visible,
        layout,
        marker_line_height: style.marker_line_height,
        shapes,
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
    let (text_width_bits, text_height_bits) = match text_alignment {
        ItemTileTextAlignment::Start => (
            layout.text_rect.width.to_bits(),
            layout.text_rect.height.to_bits(),
        ),
        ItemTileTextAlignment::Center => (0, 0),
    };
    StaticItemTextShapeCacheKey {
        text_alignment,
        paint_fallback_icon,
        text_font: style.text_font.clone(),
        marker_font: style.marker_font.clone(),
        text_font_size_bits: style.text_font_size.as_f32().to_bits(),
        marker_font_size_bits: style.marker_font_size.as_f32().to_bits(),
        label_line_height_bits: style.label_line_height.as_f32().to_bits(),
        marker_line_height_bits: if paint_fallback_icon {
            style.marker_line_height.as_f32().to_bits()
        } else {
            0
        },
        text_width_bits,
        text_height_bits,
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
            let run = TextRun {
                len: display_name.len(),
                font: style.text_font.clone(),
                color: rgb(style.text_color).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let lines = window
                .text_system()
                .shape_text(
                    display_name.clone(),
                    style.text_font_size,
                    &[run],
                    Some(px(f32::from_bits(key.text_width_bits).max(1.0))),
                    Some(
                        (f32::from_bits(key.text_height_bits) / ITEM_NAME_LINE_HEIGHT)
                            .round()
                            .max(1.0) as usize,
                    ),
                )
                .map(|lines| lines.into_iter().collect::<Vec<_>>())
                .unwrap_or_default();
            let height = static_paint_wrapped_lines_height(
                &lines,
                style.label_line_height,
                f32::from_bits(key.text_height_bits),
            );
            StaticItemLabelPaintState::Start {
                lines: lines.into(),
                height,
            }
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

    let text_bounds =
        static_item_local_bounds(bounds, state.layout.visual_rect, state.layout.text_rect);
    match &state.shapes.label {
        StaticItemLabelPaintState::Start { lines, height } => {
            let y_offset = ((text_bounds.size.height.as_f32() - *height).max(0.0) * 0.5).floor();
            let mut y = text_bounds.origin.y + px(y_offset);
            for line in lines.iter() {
                let line_height = line.size(state.label_line_height).height;
                line.paint(
                    point(text_bounds.origin.x, y),
                    state.label_line_height,
                    TextAlign::Left,
                    Some(text_bounds),
                    window,
                    cx,
                )
                .ok();
                y += line_height;
            }
        }
        StaticItemLabelPaintState::Center { lines } => {
            let height =
                (lines.len() as f32 * ITEM_NAME_LINE_HEIGHT).min(text_bounds.size.height.as_f32());
            let mut y = text_bounds.origin.y
                + px(((text_bounds.size.height.as_f32() - height).max(0.0) * 0.5).floor());
            for line in lines.iter() {
                line.paint(
                    point(text_bounds.origin.x, y),
                    state.label_line_height,
                    TextAlign::Center,
                    Some(text_bounds.size.width),
                    window,
                    cx,
                )
                .ok();
                y += state.label_line_height;
            }
        }
    }
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

fn static_paint_wrapped_lines_height(
    lines: &[gpui::WrappedLine],
    line_height: Pixels,
    max_height: f32,
) -> f32 {
    lines
        .iter()
        .map(|line| line.size(line_height).height.as_f32())
        .sum::<f32>()
        .min(max_height)
}
