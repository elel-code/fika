mod controller;
mod details;
mod dnd;
mod interaction;
mod layout;
mod paint_slots;
mod perf;
mod projection;
mod renderer_policy;
mod slots;
mod snapshot;
mod viewport;

pub(crate) use details::{
    DetailsItemSnapshot, DetailsLayoutMetrics, details_content_height, details_content_width,
};
pub(crate) use dnd::ItemDrag;
pub(crate) use layout::{
    CompactColumnWidthCache, compact_text_width, compact_text_width_for_name,
    rename_editor_required_text_width,
};
pub(crate) use paint_slots::{
    DetailsPaintSnapshot, ItemPaintSlotCache, ItemPaintSlotStats, ItemPaintSnapshot,
};
pub(crate) use perf::{
    DetailsVisualPerfStats, ItemImagePerfStats, ItemInteractionPerfStats, StaticItemVisualPerfStats,
};
pub(crate) use projection::{
    ContentItemHit, PaneLayoutProjection, PaneLayoutProjectionInput, content_item_hit_at_point,
    model_indexes_intersecting_visual_rect, pane_layout_projection,
};
pub(crate) use slots::VisibleItemSlotPool;
pub(crate) use snapshot::{
    RawFileGridSnapshot, RawFileGridSnapshotInput, VisibleItemSnapshot, VisibleItemSnapshotCache,
    deferred_thumbnail_candidates_for_model, raw_file_grid_snapshot,
};

use crate::FikaApp;
use fika_core::{
    CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions, ItemId, ItemLayout,
    PaneId, ViewRect, ViewState,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Corners, Div, Element, ElementId, Entity, Font, FontWeight,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton, ObjectFit,
    ParentElement, Pixels, RenderImage, Resource, RetainAllImageCache, Rgba, ScrollHandle,
    SharedString, Stateful, Style, StyleRefinement, Styled, TextAlign, TextRun, WeakEntity, Window,
    div, fill, img, point, px, retain_all, rgb, rgba, size,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::icons::FileIconSnapshot;
use super::item_view::item_view_scrollbar_container;
use super::rename::RENAME_TEXT_INSET_X;
#[cfg(test)]
use controller::item_mouse_down_opens_directory;
use details::{DetailsColumn, DetailsColumnKind, details_columns};
#[cfg(test)]
use dnd::drag_preview_label;
use dnd::{
    install_directory_drop_target_shell, install_item_drag_start_shell,
    item_drag_from_details_snapshot, item_drag_from_item_snapshot,
};
#[cfg(test)]
use interaction::{
    details_interaction_layer_items, item_interaction_hitbox_bounds,
    item_interaction_layer_element_id, item_interaction_layer_items,
};
use interaction::{details_interaction_layer_view, item_interaction_layer_view};
#[cfg(test)]
use paint_slots::DetailsPaintContent;
use paint_slots::ItemPaintContent;
use renderer_policy::{
    DetailsRowDragStartRenderer, DetailsRowVisualRenderer, ItemBaseVisualRenderer,
    ItemDragStartRenderer, ItemInteractionRenderer, ItemRenameEditorRenderer,
    details_renderer_policy_stats, details_row_renderer_policy, item_paints_fallback_icon,
    item_renderer_policy, item_renderer_policy_stats, item_uses_image_layer,
    item_uses_layer_visual_paint,
};
#[cfg(test)]
use renderer_policy::{
    DetailsRowInteractionRenderer, DetailsRowRendererPolicy, ItemImageRenderer, ItemRendererPolicy,
    RendererPolicyStats,
};
use viewport::{
    file_grid_viewport_shell, measured_viewport_for_scrollbar_axis, scrollbar_axis_for_snapshot,
    view_mode_for_snapshot, viewport_bounds_update_requires_notify,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileGridMode {
    Manager,
    Chooser { directories: bool, multiple: bool },
}

pub(crate) struct FileGridProps {
    pub(crate) pane_id: PaneId,
    pub(crate) snapshot: FileGridRenderSnapshot,
    pub(crate) trash_view: bool,
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) rubber_band: Option<ViewRect>,
    pub(crate) drop_target: bool,
    pub(crate) mode: FileGridMode,
}

#[derive(Clone, Debug)]
pub(crate) enum FileGridSnapshot {
    Compact {
        layout: CompactLayout,
        items: Vec<VisibleItemSnapshot>,
    },
    Icons {
        layout: IconsLayout,
        items: Vec<VisibleItemSnapshot>,
    },
    Details {
        items: Vec<DetailsItemSnapshot>,
        row_count: usize,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum FileGridRenderSnapshot {
    Compact {
        layout: CompactLayout,
        items: Vec<ItemPaintSnapshot>,
    },
    Icons {
        layout: IconsLayout,
        items: Vec<ItemPaintSnapshot>,
    },
    Details {
        items: Vec<DetailsPaintSnapshot>,
        row_count: usize,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ItemTileTextAlignment {
    Start,
    Center,
}

struct StaticItemVisualPaintState {
    layout: ItemLayout,
    marker_line_height: Pixels,
    shapes: Arc<StaticItemTextShapes>,
    label_line_height: Pixels,
    background: Rgba,
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
struct StaticItemTextShapeCacheKey {
    item_id: ItemId,
    text_alignment: ItemTileTextAlignment,
    paint_fallback_icon: bool,
    text_font: Font,
    marker_font: Font,
    text_font_size_bits: u32,
    marker_font_size_bits: u32,
    label_line_height_bits: u32,
    marker_line_height_bits: u32,
    text_width_bits: u32,
    text_height_bits: u32,
    scale_factor_bits: u32,
    text_color: u32,
    fallback_fg: u32,
    fallback_marker: SharedString,
    label: StaticItemLabelTextKey,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum StaticItemLabelTextKey {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TextShapeCacheStats {
    hits: usize,
    misses: usize,
    evicted: usize,
    entries: usize,
}

impl TextShapeCacheStats {
    fn has_activity(self) -> bool {
        self.hits > 0 || self.misses > 0 || self.evicted > 0
    }
}

#[derive(Default)]
pub(crate) struct StaticItemTextShapeCache {
    entries: HashMap<StaticItemTextShapeCacheKey, Arc<StaticItemTextShapes>>,
    stats: TextShapeCacheStats,
}

impl StaticItemTextShapeCache {
    const MAX_ENTRIES: usize = 2048;

    fn shape_for(
        &mut self,
        key: &StaticItemTextShapeCacheKey,
        style: &StaticItemTextShapeStyle,
        window: &mut Window,
    ) -> Arc<StaticItemTextShapes> {
        if let Some(shapes) = self.entries.get(key) {
            self.stats.hits += 1;
            return shapes.clone();
        }

        self.stats.misses += 1;
        if self.entries.len() >= Self::MAX_ENTRIES {
            self.stats.evicted += self.entries.len();
            self.entries.clear();
        }

        let shapes = Arc::new(shape_static_item_text(key, style, window));
        self.entries.insert(key.clone(), shapes.clone());
        shapes
    }

    fn take_stats(&mut self) -> TextShapeCacheStats {
        let mut stats = std::mem::take(&mut self.stats);
        stats.entries = self.entries.len();
        stats
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct DetailsTextShapeCacheKey {
    text: SharedString,
    font: Font,
    font_size_bits: u32,
    line_height_bits: u32,
    scale_factor_bits: u32,
    color: u32,
}

#[derive(Default)]
pub(crate) struct DetailsTextShapeCache {
    entries: HashMap<DetailsTextShapeCacheKey, Arc<gpui::ShapedLine>>,
    stats: TextShapeCacheStats,
}

impl DetailsTextShapeCache {
    const MAX_ENTRIES: usize = 4096;

    fn shape_for(
        &mut self,
        key: &DetailsTextShapeCacheKey,
        window: &mut Window,
    ) -> Arc<gpui::ShapedLine> {
        if let Some(line) = self.entries.get(key) {
            self.stats.hits += 1;
            return line.clone();
        }

        self.stats.misses += 1;
        if self.entries.len() >= Self::MAX_ENTRIES {
            self.stats.evicted += self.entries.len();
            self.entries.clear();
        }

        let line = Arc::new(shape_details_visual_text(key, window));
        self.entries.insert(key.clone(), line.clone());
        line
    }

    fn take_stats(&mut self) -> TextShapeCacheStats {
        let mut stats = std::mem::take(&mut self.stats);
        stats.entries = self.entries.len();
        stats
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneViewportGeometry {
    pub(crate) window_rect: ViewRect,
}

fn item_identity_element_id(prefix: &'static str, item_id: ItemId) -> (&'static str, u64) {
    (prefix, item_id.0)
}

fn item_image_element_id(slot_id: u64) -> (&'static str, u64) {
    ("item-image", slot_id)
}

fn static_item_visual_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("static-item-visual-layer", pane_id.0)
}

fn item_image_paint_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("item-image-paint-layer", pane_id.0)
}

fn details_visual_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("details-visual-layer", pane_id.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RenameTextLayout {
    name_height: f32,
    helper_height: f32,
}

const RENAME_NAME_HEIGHT: f32 = 20.0;
pub(crate) const ITEM_NAME_LINE_HEIGHT: f32 = 18.0;
const DEFAULT_TILE_TEXT_HEIGHT: f32 = 40.0;
const DOLPHIN_ITEM_PADDING: f32 = 2.0;
const DOLPHIN_ICON_TEXT_WIDTH_INDEX: f32 = 1.0;
const DOLPHIN_ICON_FONT_FACTOR: f32 = 1.0;
const DOLPHIN_ICON_MARGIN: f32 = 8.0;
pub(crate) const DOLPHIN_ICON_MAX_TEXT_LINES: usize = 3;
const DOLPHIN_COMPACT_SIDE_PADDING: f32 = 8.0;
const DOLPHIN_COMPACT_COLUMN_GAP: f32 = 8.0;
const DOLPHIN_COMPACT_TEXT_GAP: f32 = DOLPHIN_ITEM_PADDING * 2.0;
const DOLPHIN_COMPACT_BASE_TEXT_WIDTH: f32 = ITEM_NAME_LINE_HEIGHT * 5.0;

pub(crate) fn file_grid(
    props: FileGridProps,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let perf_enabled = crate::item_view_perf_enabled();
    let build_started = perf_enabled.then(std::time::Instant::now);
    let FileGridProps {
        pane_id,
        snapshot,
        trash_view,
        scroll_handle,
        rubber_band,
        drop_target,
        mode,
    } = props;
    let app = cx.weak_entity();
    let scrollbar_axis = scrollbar_axis_for_snapshot(&snapshot);
    let view_mode = view_mode_for_snapshot(&snapshot);

    let (content_width, content_height, visible_count, renderer_policy_stats, viewport) =
        match snapshot {
            FileGridRenderSnapshot::Icons {
                layout: icons_layout,
                items,
            } => {
                let content_size = icons_layout.content_size();
                let visible_count = items.len();
                let renderer_policy_stats = item_renderer_policy_stats(&items);
                let static_visual_layer = static_item_visual_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    ItemTileTextAlignment::Center,
                    app.clone(),
                );
                let image_layer = item_image_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    app.clone(),
                );
                let interaction_layer = item_interaction_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    app.clone(),
                );
                let content = div()
                    .relative()
                    .w(px(content_size.width))
                    .h(px(content_size.height));
                let content = if let Some(layer) = static_visual_layer {
                    content.child(layer)
                } else {
                    content
                };
                let content = if let Some(layer) = image_layer {
                    content.child(layer)
                } else {
                    content
                };
                let content = if let Some(layer) = interaction_layer {
                    content.child(layer)
                } else {
                    content
                };
                let viewport = file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(
                    content.children(items.into_iter().map(|item| {
                        item_tile(
                            pane_id,
                            item,
                            ItemTileTextAlignment::Center,
                            app.clone(),
                            cx,
                        )
                    })),
                );
                (
                    content_size.width,
                    content_size.height,
                    visible_count,
                    renderer_policy_stats,
                    viewport,
                )
            }
            FileGridRenderSnapshot::Compact { layout, items } => {
                let content_size = layout.content_size();
                let visible_count = items.len();
                let renderer_policy_stats = item_renderer_policy_stats(&items);
                let static_visual_layer = static_item_visual_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    ItemTileTextAlignment::Start,
                    app.clone(),
                );
                let image_layer = item_image_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    app.clone(),
                );
                let interaction_layer = item_interaction_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    app.clone(),
                );
                let content = div()
                    .relative()
                    .w(px(content_size.width))
                    .h(px(content_size.height));
                let content = if let Some(layer) = static_visual_layer {
                    content.child(layer)
                } else {
                    content
                };
                let content = if let Some(layer) = image_layer {
                    content.child(layer)
                } else {
                    content
                };
                let content = if let Some(layer) = interaction_layer {
                    content.child(layer)
                } else {
                    content
                };
                let viewport = file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(
                    content.children(items.into_iter().map(|item| {
                        item_tile(pane_id, item, ItemTileTextAlignment::Start, app.clone(), cx)
                    })),
                );
                (
                    content_size.width,
                    content_size.height,
                    visible_count,
                    renderer_policy_stats,
                    viewport,
                )
            }
            FileGridRenderSnapshot::Details {
                items,
                row_count,
                metrics,
                name_column_width,
            } => {
                let content_width = details_content_width(trash_view, name_column_width).max(1.0);
                let content_height = details_content_height(row_count, metrics).max(1.0);
                let visible_count = items.len();
                let renderer_policy_stats = details_renderer_policy_stats(&items);
                let viewport =
                    file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(details_table(
                        pane_id,
                        items,
                        row_count,
                        trash_view,
                        content_width,
                        content_height,
                        metrics,
                        name_column_width,
                        app.clone(),
                        cx,
                    ));
                (
                    content_width,
                    content_height,
                    visible_count,
                    renderer_policy_stats,
                    viewport,
                )
            }
        };

    let root = div()
        .image_cache(retain_all(("file-grid-image-cache", pane_id.0)))
        .on_children_prepainted(move |bounds, _window, cx| {
            let prepaint_started = perf_enabled.then(std::time::Instant::now);
            let Some(bounds) = bounds.first() else {
                return;
            };
            let measured = measured_viewport_for_scrollbar_axis(
                *bounds,
                content_width,
                content_height,
                scrollbar_axis,
            );
            let mut bounds_changed = false;
            let mut notify_requested = false;
            let mut shape_cache_stats = TextShapeCacheStats::default();
            let mut details_shape_cache_stats = TextShapeCacheStats::default();
            let mut static_visual_stats = StaticItemVisualPerfStats::default();
            let mut image_stats = ItemImagePerfStats::default();
            let mut details_visual_stats = DetailsVisualPerfStats::default();
            let mut interaction_stats = ItemInteractionPerfStats::default();
            let _ = app.update(cx, |this, cx| {
                let previous_view = this.panes.pane(pane_id).map(|pane| pane.view.clone());
                this.set_pane_viewport_geometry(pane_id, measured.rect);
                bounds_changed = this.set_pane_viewport_bounds(
                    pane_id,
                    measured.rect.width,
                    measured.rect.height,
                    measured.max_scroll_x,
                    measured.max_scroll_y,
                );
                let next_view = this.panes.pane(pane_id).map(|pane| pane.view.clone());
                let projected_width = this.projected_item_viewport_width(pane_id, view_mode);
                if bounds_changed
                    && viewport_bounds_update_requires_notify(
                        previous_view.as_ref(),
                        next_view.as_ref(),
                        projected_width,
                        measured.rect,
                    )
                {
                    notify_requested = true;
                    cx.notify();
                }
                if perf_enabled {
                    shape_cache_stats = this.take_static_item_text_shape_cache_stats(pane_id);
                    details_shape_cache_stats = this.take_details_text_shape_cache_stats(pane_id);
                    static_visual_stats = this.take_static_item_visual_perf_stats(pane_id);
                    image_stats = this.take_item_image_perf_stats(pane_id);
                    details_visual_stats = this.take_details_visual_perf_stats(pane_id);
                    interaction_stats = this.take_item_interaction_perf_stats(pane_id);
                }
            });
            if let Some(started) = prepaint_started {
                eprintln!(
                    "[fika viewport] pane={} mode={:?} measured={}x{} content={}x{} changed={} notify={} total={}us",
                    pane_id.0,
                    view_mode,
                    measured.rect.width,
                    measured.rect.height,
                    content_width,
                    content_height,
                    bounds_changed,
                    notify_requested,
                    started.elapsed().as_micros(),
                );
                if shape_cache_stats.has_activity() {
                    eprintln!(
                        "[fika item-shape-cache] pane={} mode={:?} hits={} misses={} evicted={} entries={}",
                        pane_id.0,
                        view_mode,
                        shape_cache_stats.hits,
                        shape_cache_stats.misses,
                        shape_cache_stats.evicted,
                        shape_cache_stats.entries,
                    );
                }
                if details_shape_cache_stats.has_activity() {
                    eprintln!(
                        "[fika details-shape-cache] pane={} mode={:?} hits={} misses={} evicted={} entries={}",
                        pane_id.0,
                        view_mode,
                        details_shape_cache_stats.hits,
                        details_shape_cache_stats.misses,
                        details_shape_cache_stats.evicted,
                        details_shape_cache_stats.entries,
                    );
                }
                if static_visual_stats.has_activity() {
                    eprintln!(
                        "[fika static-item-visual] pane={} mode={:?} prepaint_count={} prepaint={}us paint_count={} paint={}us",
                        pane_id.0,
                        view_mode,
                        static_visual_stats.prepaint_count,
                        static_visual_stats.prepaint_us,
                        static_visual_stats.paint_count,
                        static_visual_stats.paint_us,
                    );
                }
                if interaction_stats.has_activity() {
                    eprintln!(
                        "[fika item-interaction] pane={} mode={:?} prepaint_count={} prepaint={}us paint_count={} paint={}us",
                        pane_id.0,
                        view_mode,
                        interaction_stats.prepaint_count,
                        interaction_stats.prepaint_us,
                        interaction_stats.paint_count,
                        interaction_stats.paint_us,
                    );
                }
                if image_stats.has_activity() {
                    eprintln!(
                        "[fika item-image] pane={} mode={:?} prepaint_count={} prepaint={}us paint_count={} paint={}us",
                        pane_id.0,
                        view_mode,
                        image_stats.prepaint_count,
                        image_stats.prepaint_us,
                        image_stats.paint_count,
                        image_stats.paint_us,
                    );
                }
                if details_visual_stats.has_activity() {
                    eprintln!(
                        "[fika details-visual] pane={} mode={:?} prepaint_count={} prepaint={}us paint_count={} paint={}us",
                        pane_id.0,
                        view_mode,
                        details_visual_stats.prepaint_count,
                        details_visual_stats.prepaint_us,
                        details_visual_stats.paint_count,
                        details_visual_stats.paint_us,
                    );
                }
            }
        })
        .id(format!("items-{}", pane_id.0))
        .relative()
        .flex()
        .flex_col()
        .min_w_0()
        .min_h_0()
        .w_full()
        .max_w_full()
        .overflow_hidden()
        .flex_1()
        .child(item_view_scrollbar_container(
            pane_id,
            &scroll_handle,
            scrollbar_axis,
            rubber_band,
            viewport,
            window,
            cx,
        ));
    if let Some(started) = build_started {
        eprintln!(
            "[fika renderer-policy] pane={} mode={:?} items={} visual_layer={} image_layer={} retained_interaction={} gpui_drag_shell={} rename_overlay={}",
            pane_id.0,
            view_mode,
            renderer_policy_stats.items,
            renderer_policy_stats.visual_layer,
            renderer_policy_stats.image_layer,
            renderer_policy_stats.retained_interaction,
            renderer_policy_stats.gpui_drag_shell,
            renderer_policy_stats.rename_overlay,
        );
        eprintln!(
            "[fika file-grid] pane={} mode={:?} visible={} content={}x{} build={}us",
            pane_id.0,
            view_mode,
            visible_count,
            content_width,
            content_height,
            started.elapsed().as_micros(),
        );
    }
    root
}

fn details_table(
    pane_id: PaneId,
    items: Vec<DetailsPaintSnapshot>,
    row_count: usize,
    trash_view: bool,
    content_width: f32,
    content_height: f32,
    metrics: DetailsLayoutMetrics,
    name_column_width: f32,
    app: WeakEntity<FikaApp>,
    cx: &mut Context<FikaApp>,
) -> Div {
    let columns = details_columns(trash_view, name_column_width);
    let visual_layer = details_visual_layer_view(
        pane_id,
        &items,
        &columns,
        content_width,
        content_height,
        app.clone(),
    );
    let interaction_layer =
        details_interaction_layer_view(pane_id, &items, content_width, content_height, app.clone());
    let table = div()
        .relative()
        .w(px(content_width))
        .h(px(content_height))
        .child(details_header(&columns, content_width, metrics));
    let table = if let Some(layer) = visual_layer {
        table.child(layer)
    } else {
        table
    };
    let table = if let Some(layer) = interaction_layer {
        table.child(layer)
    } else {
        table
    };
    table
        .children(
            items
                .into_iter()
                .map(|item| details_row(pane_id, item, content_width, cx)),
        )
        .when(row_count == 0, |table| {
            table.child(
                div()
                    .absolute()
                    .top(px(metrics.header_height))
                    .left_0()
                    .w(px(content_width))
                    .h(px(metrics.row_height))
                    .px_2()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(rgb(0x6b7280))
                    .child("No items"),
            )
        })
}

fn details_header(
    columns: &[DetailsColumn],
    content_width: f32,
    metrics: DetailsLayoutMetrics,
) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(content_width))
        .h(px(metrics.header_height))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgb(0xd5d9df))
        .bg(rgb(0xf3f5f8))
        .children(columns.iter().map(|column| {
            div()
                .w(px(column.width))
                .h_full()
                .px_2()
                .flex()
                .items_center()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0x4b5563))
                .border_r_1()
                .border_color(rgb(0xe1e5eb))
                .child(column.title)
        }))
}

fn details_visual_layer_view(
    pane_id: PaneId,
    items: &[DetailsPaintSnapshot],
    columns: &[DetailsColumn],
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<DetailsVisualLayerElement> {
    let items = details_visual_layer_items(items, columns);
    (!items.is_empty()).then(|| {
        DetailsVisualLayerElement {
            pane_id,
            app,
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

fn details_visual_layer_items(
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
struct DetailsVisualLayerItem {
    row_index: usize,
    row_top: f32,
    row_height: f32,
    icon_size: f32,
    selected: bool,
    hovered: bool,
    drop_target: bool,
    cells: Vec<DetailsVisualCell>,
}

#[derive(Clone)]
struct DetailsVisualCell {
    x: f32,
    width: f32,
    content: DetailsVisualCellContent,
}

#[derive(Clone)]
enum DetailsVisualCellContent {
    Name {
        name: SharedString,
        icon: FileIconSnapshot,
    },
    Text {
        text: SharedString,
    },
}

struct DetailsVisualLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<DetailsVisualLayerItem>,
    style: StyleRefinement,
}

struct DetailsVisualPaintState {
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
    type PrepaintState = Vec<DetailsVisualPaintState>;

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
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        let states = if let Some(id) = id {
            window.with_element_state::<Entity<RetainAllImageCache>, _>(id, |cache, window| {
                let cache = cache.unwrap_or_else(|| RetainAllImageCache::new(cx));
                let states = self
                    .items
                    .iter()
                    .map(|item| {
                        details_visual_prepaint_item(
                            self.pane_id,
                            item,
                            Some(&cache),
                            &self.app,
                            window,
                            cx,
                        )
                    })
                    .collect::<Vec<_>>();
                (states, cache)
            })
        } else {
            self.items
                .iter()
                .map(|item| {
                    details_visual_prepaint_item(self.pane_id, item, None, &self.app, window, cx)
                })
                .collect::<Vec<_>>()
        };
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let count = states.len();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_details_visual_prepaint(self.pane_id, elapsed, count);
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
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.len();
        request_layout.paint(bounds, window, cx, |window, cx| {
            for state in prepaint.iter() {
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

const DETAILS_CELL_PADDING_X: f32 = 8.0;
const DETAILS_NAME_ICON_GAP: f32 = 8.0;

fn details_visual_prepaint_item(
    pane_id: PaneId,
    item: &DetailsVisualLayerItem,
    cache: Option<&Entity<RetainAllImageCache>>,
    app: &WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut App,
) -> DetailsVisualPaintState {
    let font = window.text_style().font();
    let font_size = px(window.rem_size().as_f32() * 0.875);
    let line_height = px(ITEM_NAME_LINE_HEIGHT);
    let cells = item
        .cells
        .iter()
        .map(|cell| match &cell.content {
            DetailsVisualCellContent::Name { name, icon } => {
                let icon_rect = details_visual_name_icon_rect(item, cell);
                let text_rect = details_visual_name_text_rect(item, cell);
                DetailsVisualCellPaintState::Name {
                    icon: details_visual_icon_prepaint(icon_rect, icon, cache, window, cx),
                    text: details_visual_text_prepaint(
                        text_rect,
                        name.clone(),
                        if item.selected { 0x0f172a } else { 0x1f2937 },
                        font.clone(),
                        font_size,
                        line_height,
                        pane_id,
                        app,
                        window,
                        cx,
                    ),
                }
            }
            DetailsVisualCellContent::Text { text } => {
                DetailsVisualCellPaintState::Text(details_visual_text_prepaint(
                    details_visual_text_rect(item, cell),
                    text.clone(),
                    0x4b5563,
                    font.clone(),
                    font_size,
                    line_height,
                    pane_id,
                    app,
                    window,
                    cx,
                ))
            }
        })
        .collect();
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
    cache: Option<&Entity<RetainAllImageCache>>,
    window: &mut Window,
    cx: &mut App,
) -> DetailsVisualIconPaintState {
    let image = icon.path.as_ref().and_then(|path| {
        let cache = cache?;
        let resource = Resource::Path(path.clone());
        cache
            .update(cx, |cache, cx| cache.load(&resource, window, cx))
            .and_then(Result::ok)
    });
    let fallback = image
        .is_none()
        .then(|| details_visual_icon_fallback_prepaint(rect, icon, window));
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
        marker_line: window
            .text_system()
            .shape_line(marker, marker_font_size, &[marker_run], None),
        marker_line_height: px(rect.height.min(ITEM_NAME_LINE_HEIGHT).max(1.0)),
        fallback_bg: icon.fallback_bg,
    }
}

fn details_visual_text_prepaint(
    rect: ViewRect,
    text: SharedString,
    color: u32,
    font: Font,
    font_size: Pixels,
    line_height: Pixels,
    pane_id: PaneId,
    app: &WeakEntity<FikaApp>,
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
    DetailsVisualTextPaintState {
        rect,
        line,
        line_height,
    }
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
        details_row_background(
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

fn details_visual_paint_icon(
    layer_bounds: Bounds<Pixels>,
    state: &DetailsVisualIconPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let icon_bounds = details_visual_bounds(layer_bounds, state.rect);
    if let Some(image) = state.image.as_ref() {
        if image.frame_count() > 0 {
            let image_size = image.size(0);
            if u32::from(image_size.width) > 0 && u32::from(image_size.height) > 0 {
                let image_bounds = ObjectFit::Contain.get_bounds(icon_bounds, image_size);
                window
                    .paint_image(image_bounds, Corners::all(px(4.0)), image.clone(), 0, false)
                    .ok();
                return;
            }
        }
    }
    if let Some(fallback) = state.fallback.as_ref() {
        window.paint_quad(fill(icon_bounds, rgb(fallback.fallback_bg)).corner_radii(px(4.0)));
        let marker_origin = point(
            icon_bounds.origin.x,
            icon_bounds.origin.y
                + ((icon_bounds.size.height - fallback.marker_line_height).max(px(0.0)) / 2.0),
        );
        fallback
            .marker_line
            .paint(
                marker_origin,
                fallback.marker_line_height,
                TextAlign::Center,
                Some(icon_bounds.size.width),
                window,
                cx,
            )
            .ok();
    }
}

fn details_visual_paint_text(
    layer_bounds: Bounds<Pixels>,
    state: &DetailsVisualTextPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let text_bounds = details_visual_bounds(layer_bounds, state.rect);
    window.paint_layer(text_bounds, |window| {
        state
            .line
            .paint(
                point(text_bounds.origin.x, text_bounds.origin.y),
                state.line_height,
                TextAlign::Left,
                Some(text_bounds.size.width),
                window,
                cx,
            )
            .ok();
    });
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

fn details_row(
    pane_id: PaneId,
    item: DetailsPaintSnapshot,
    content_width: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let top = f32::from_bits(item.geometry.row_top);
    let row_height = f32::from_bits(item.geometry.row_height);
    let item_id = item.item_id;
    let policy = details_row_renderer_policy(&item);
    let drag_value = item_drag_from_details_snapshot(pane_id, &item);
    let app = cx.weak_entity();
    let directory_drop_target = item.content.is_dir.then(|| item.content.path.clone());

    let row = div()
        .id(item_identity_element_id("details-row", item_id))
        .absolute()
        .left_0()
        .top(px(top))
        .w(px(content_width))
        .h(px(row_height))
        .flex()
        .items_center()
        .bg(rgba(0x00000000));
    let row = match directory_drop_target {
        Some(target_dir) => install_directory_drop_target_shell(row, pane_id, target_dir, cx),
        None => row,
    };

    // The viewport owns click/menu/navigation hit testing from retained
    // geometry; this row remains only as GPUI's drag-start boundary.
    match policy.drag_start {
        DetailsRowDragStartRenderer::GpuiShell => {
            install_item_drag_start_shell(row, drag_value, app)
        }
    }
}

fn details_row_background(
    selected: bool,
    hovered: bool,
    drop_target: bool,
    row_index: usize,
) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected && hovered {
        rgb(0xcfe3ff)
    } else if selected {
        rgb(0xdbeafe)
    } else if hovered {
        rgb(0xeaf1ff)
    } else if row_index % 2 == 0 {
        rgb(0xffffff)
    } else {
        rgb(0xf8fafc)
    }
}

fn item_tile(
    pane_id: PaneId,
    item: ItemPaintSnapshot,
    text_alignment: ItemTileTextAlignment,
    app: WeakEntity<FikaApp>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let item_rect = item.layout.item_rect;
    let visual = item.layout.visual_rect;
    let item_id = item.item_id;
    let content = item.content.as_ref();
    let selected = item.visual.selected;
    let hovered = item.visual.hovered;
    let drop_target = item.visual.drop_target;
    let renderer_policy = item_renderer_policy(content);
    let use_layer_visual_paint = matches!(
        renderer_policy.base_visual,
        ItemBaseVisualRenderer::ContentLayer
    );
    let use_layer_interaction = matches!(
        renderer_policy.interaction,
        ItemInteractionRenderer::RetainedLayer
    );
    let drag_app = app.clone();
    let drag_value = item_drag_from_item_snapshot(pane_id, &item);
    let directory_drop_target = content.is_dir.then(|| content.drag_path.clone());
    let shell_background = if use_layer_visual_paint {
        rgba(0x00000000)
    } else {
        item_tile_background(selected, drop_target, hovered)
    };

    // Temporary migration boundary: GPUI drag starts are still tied to a Div
    // until a public custom-element drag-start API exists.
    let core = div()
        .id(item_identity_element_id("item-core", item_id))
        .absolute()
        .left(px(visual.x - item_rect.x))
        .top(px(visual.y - item_rect.y))
        .w(px(visual.width))
        .h(px(visual.height))
        .rounded_md()
        .bg(shell_background);
    let core = match directory_drop_target {
        Some(target_dir) => install_directory_drop_target_shell(core, pane_id, target_dir, cx),
        None => core,
    };
    let core = match renderer_policy.drag_start {
        ItemDragStartRenderer::GpuiShell => {
            install_item_drag_start_shell(core, drag_value, drag_app)
        }
    };
    let core = if use_layer_interaction {
        core
    } else {
        core.cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                let changed = if *hovered {
                    this.set_hovered_item(pane_id, item_id)
                } else {
                    this.clear_hovered_item(pane_id, item_id)
                };
                if changed {
                    cx.notify();
                }
            }))
    };
    let core = if !use_layer_visual_paint {
        let text = {
            static_text_view(
                content.display_name.clone(),
                &content.icon_name_lines,
                item.layout,
                text_alignment,
                selected,
            )
            .into_any_element()
        };
        core.child(icon_view(item.slot_id, content, item.layout))
            .child(text)
    } else {
        core
    };
    let core = match renderer_policy.rename_editor {
        ItemRenameEditorRenderer::None => core,
        ItemRenameEditorRenderer::GpuiOverlay => {
            let draft_name = content
                .draft_name
                .as_deref()
                .expect("rename renderer policy requires draft text");
            core.child(rename_text_view(
                pane_id,
                SharedString::from(draft_name),
                item.layout,
                text_alignment,
                selected,
                content.draft_caret,
                content.draft_selection,
                content.draft_error.as_deref(),
                content.draft_warning.as_deref(),
                cx,
            ))
        }
    };

    div()
        .id(("item-slot", item.slot_id))
        .absolute()
        .left(px(item_rect.x))
        .top(px(item_rect.y))
        .w(px(item_rect.width))
        .h(px(item_rect.height))
        .child(core)
}

fn item_tile_background(selected: bool, drop_target: bool, hovered: bool) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected && hovered {
        rgb(0xcfe3ff)
    } else if selected {
        rgb(0xdbeafe)
    } else if hovered {
        rgb(0xeaf1ff)
    } else {
        rgba(0x00000000)
    }
}

fn drop_target_item_background() -> Rgba {
    rgba(0xf59e0b4a)
}

fn static_item_visual_layer_view(
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
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

fn static_item_visual_layer_items(
    items: &[ItemPaintSnapshot],
    text_alignment: ItemTileTextAlignment,
) -> Vec<StaticItemVisualLayerItem> {
    items
        .iter()
        .filter_map(|item| {
            let content = item.content.as_ref();
            item_uses_layer_visual_paint(content).then(|| StaticItemVisualLayerItem {
                item_id: item.item_id,
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

struct StaticItemVisualLayerItem {
    item_id: ItemId,
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
}

struct StaticItemVisualLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<StaticItemVisualLayerItem>,
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
        Some(ElementId::from(static_item_visual_layer_element_id(
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
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        let states = self
            .items
            .iter()
            .map(|item| {
                static_item_visual_prepaint(
                    self.pane_id,
                    item.item_id,
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
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.len();
        request_layout.paint(bounds, window, cx, |window, cx| {
            for state in prepaint.iter() {
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

fn item_image_layer_view(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<ItemImageLayerElement> {
    let items = item_image_layer_items(items);
    (!items.is_empty()).then(|| {
        ItemImageLayerElement {
            pane_id,
            app,
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

fn item_image_layer_items(items: &[ItemPaintSnapshot]) -> Vec<ItemImageLayerItem> {
    items
        .iter()
        .filter_map(|item| {
            let content = item.content.as_ref();
            if !item_uses_layer_visual_paint(content) || !item_uses_image_layer(content) {
                return None;
            }
            Some(ItemImageLayerItem {
                layout: item.layout,
                thumbnail_path: content.thumbnail_path.clone(),
                icon: content.icon.clone(),
                fallback_marker: content.fallback_marker.clone(),
            })
        })
        .collect()
}

struct ItemImageLayerItem {
    layout: ItemLayout,
    thumbnail_path: Option<Arc<Path>>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
}

fn item_image_layer_item_source_path(item: &ItemImageLayerItem) -> Option<Arc<Path>> {
    item.thumbnail_path
        .clone()
        .or_else(|| item.icon.path.clone())
}

fn item_image_load_failure_paints_fallback(item: &ItemImageLayerItem) -> bool {
    item.thumbnail_path.is_none()
}

struct ItemImageLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<ItemImageLayerItem>,
    style: StyleRefinement,
}

struct ItemImagePaintState {
    icon_rect: ViewRect,
    image: Option<Arc<RenderImage>>,
    fallback: Option<ItemImageFallbackPaintState>,
}

struct ItemImageFallbackPaintState {
    marker_line: gpui::ShapedLine,
    marker_line_height: Pixels,
    fallback_bg: u32,
}

impl IntoElement for ItemImageLayerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ItemImageLayerElement {
    type RequestLayoutState = Style;
    type PrepaintState = Vec<ItemImagePaintState>;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(item_image_paint_layer_element_id(
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
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let Some(id) = id else {
            return Vec::new();
        };
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        window.with_element_state::<Entity<RetainAllImageCache>, _>(id, |cache, window| {
            let cache = cache.unwrap_or_else(|| RetainAllImageCache::new(cx));
            let states = self
                .items
                .iter()
                .filter_map(|item| item_image_layer_prepaint_item(item, &cache, window, cx))
                .collect::<Vec<_>>();
            if let Some(started) = perf_started {
                let elapsed = started.elapsed();
                let count = states.len();
                let _ = self.app.update(cx, |this, _cx| {
                    this.record_item_image_prepaint(self.pane_id, elapsed, count);
                });
            }
            (states, cache)
        })
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
        let perf_started = crate::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.len();
        request_layout.paint(bounds, window, cx, |window, cx| {
            for state in prepaint.iter() {
                item_image_layer_paint_item(bounds, state, window, cx);
            }
        });
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_item_image_paint(self.pane_id, elapsed, count);
            });
        }
    }
}

impl Styled for ItemImageLayerElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn item_image_layer_prepaint_item(
    item: &ItemImageLayerItem,
    cache: &Entity<RetainAllImageCache>,
    window: &mut Window,
    cx: &mut App,
) -> Option<ItemImagePaintState> {
    let source_path = item_image_layer_item_source_path(item)?;
    let resource = Resource::Path(source_path);
    let load_result = cache.update(cx, |cache, cx| cache.load(&resource, window, cx));
    let (image, fallback) = match load_result {
        Some(Ok(image)) => (Some(image), None),
        Some(Err(_)) if item_image_load_failure_paints_fallback(item) => {
            (None, Some(item_image_fallback_prepaint(item, window)))
        }
        _ => (None, None),
    };
    Some(ItemImagePaintState {
        icon_rect: item.layout.icon_rect,
        image,
        fallback,
    })
}

fn item_image_fallback_prepaint(
    item: &ItemImageLayerItem,
    window: &mut Window,
) -> ItemImageFallbackPaintState {
    let text_style = window.text_style();
    let mut marker_font = text_style.font();
    marker_font.weight = FontWeight::SEMIBOLD;
    let marker = static_paint_single_line_text(item.fallback_marker.clone());
    let marker_run = TextRun {
        len: marker.len(),
        font: marker_font,
        color: rgb(item.icon.fallback_fg).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let marker_font_size = px(window.rem_size().as_f32() * 0.75);
    ItemImageFallbackPaintState {
        marker_line: window
            .text_system()
            .shape_line(marker, marker_font_size, &[marker_run], None),
        marker_line_height: px(item
            .layout
            .icon_rect
            .height
            .min(ITEM_NAME_LINE_HEIGHT)
            .max(1.0)),
        fallback_bg: item.icon.fallback_bg,
    }
}

fn item_image_layer_paint_item(
    layer_bounds: Bounds<Pixels>,
    state: &ItemImagePaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let icon_bounds = item_image_layer_icon_bounds(layer_bounds, state.icon_rect);
    if let Some(image) = state.image.as_ref() {
        if image.frame_count() == 0 {
            return;
        }
        let image_size = image.size(0);
        if u32::from(image_size.width) == 0 || u32::from(image_size.height) == 0 {
            return;
        }
        let image_bounds = ObjectFit::Contain.get_bounds(icon_bounds, image_size);
        window
            .paint_image(image_bounds, Corners::all(px(6.0)), image.clone(), 0, false)
            .ok();
        return;
    }

    if let Some(fallback) = state.fallback.as_ref() {
        window.paint_quad(fill(icon_bounds, rgb(fallback.fallback_bg)).corner_radii(px(6.0)));
        let marker_origin = point(
            icon_bounds.origin.x,
            icon_bounds.origin.y
                + ((icon_bounds.size.height - fallback.marker_line_height).max(px(0.0)) / 2.0),
        );
        fallback
            .marker_line
            .paint(
                marker_origin,
                fallback.marker_line_height,
                TextAlign::Center,
                Some(icon_bounds.size.width),
                window,
                cx,
            )
            .ok();
    }
}

fn item_image_layer_icon_bounds(
    layer_bounds: Bounds<Pixels>,
    icon_rect: ViewRect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            layer_bounds.origin.x + px(icon_rect.x.round()),
            layer_bounds.origin.y + px(icon_rect.y.round()),
        ),
        size(
            px(icon_rect.width.round().max(1.0)),
            px(icon_rect.height.round().max(1.0)),
        ),
    )
}

fn static_item_visual_prepaint(
    pane_id: PaneId,
    item_id: ItemId,
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
        item_id,
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
        layout,
        marker_line_height: style.marker_line_height,
        shapes,
        label_line_height: style.label_line_height,
        background: item_tile_background(selected, drop_target, hovered),
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
    item_id: ItemId,
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
    StaticItemTextShapeCacheKey {
        item_id,
        text_alignment,
        paint_fallback_icon,
        text_font: style.text_font.clone(),
        marker_font: style.marker_font.clone(),
        text_font_size_bits: style.text_font_size.as_f32().to_bits(),
        marker_font_size_bits: style.marker_font_size.as_f32().to_bits(),
        label_line_height_bits: style.label_line_height.as_f32().to_bits(),
        marker_line_height_bits: style.marker_line_height.as_f32().to_bits(),
        text_width_bits: layout.text_rect.width.to_bits(),
        text_height_bits: layout.text_rect.height.to_bits(),
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
    window.paint_quad(fill(bounds, state.background).corner_radii(px(6.0)));
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
    window.paint_layer(text_bounds, |window| match &state.shapes.label {
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
    });
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

fn static_paint_single_line_text(text: SharedString) -> SharedString {
    if text.as_ref().contains('\n') {
        SharedString::from(text.as_ref().replace('\n', " "))
    } else {
        text
    }
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

fn icon_view(slot_id: u64, content: &ItemPaintContent, layout: ItemLayout) -> Div {
    let visual = layout.visual_rect;
    let icon = layout.icon_rect;
    let icon_left = (icon.x - visual.x).round();
    let icon_top = (icon.y - visual.y).round();
    let icon_width = icon.width.round().max(1.0);
    let icon_height = icon.height.round().max(1.0);
    let thumbnail_path = content.thumbnail_path.clone();
    let icon_snapshot = content.icon.clone();
    let fallback_marker = content.fallback_marker.clone();
    let icon_container = div()
        .absolute()
        .left(px(icon_left))
        .top(px(icon_top))
        .w(px(icon_width))
        .h(px(icon_height))
        .flex()
        .items_center()
        .justify_center();

    match thumbnail_path {
        Some(path) => icon_container.child(
            div()
                .size_full()
                .rounded_md()
                .overflow_hidden()
                .child(img(path).id(item_image_element_id(slot_id)).size_full()),
        ),
        None => icon_container.child(item_image_or_fallback(
            slot_id,
            icon_snapshot,
            fallback_marker,
        )),
    }
}

fn item_image_or_fallback(
    slot_id: u64,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
) -> gpui::AnyElement {
    match icon.path.clone() {
        Some(path) => img(path)
            .id(item_image_element_id(slot_id))
            .size_full()
            .with_fallback({
                let fallback_fg = icon.fallback_fg;
                let fallback_bg = icon.fallback_bg;
                move || fallback_icon_element(fallback_marker.clone(), fallback_fg, fallback_bg)
            })
            .into_any_element(),
        None => fallback_icon_element(fallback_marker, icon.fallback_fg, icon.fallback_bg),
    }
}

fn fallback_icon_element(marker: SharedString, fg: u32, bg: u32) -> gpui::AnyElement {
    div()
        .size_full()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(fg))
        .bg(rgb(bg))
        .child(marker)
        .into_any_element()
}

fn static_text_view(
    display_name: SharedString,
    icon_name_lines: &[SharedString],
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    selected: bool,
) -> Div {
    let visual = layout.visual_rect;
    let text = layout.text_rect;
    let text_left = text.x - visual.x;
    let text_top = text.y - visual.y;
    let text_color = if selected {
        rgb(0x0f172a)
    } else {
        rgb(0x24292f)
    };

    match text_alignment {
        ItemTileTextAlignment::Start => {
            let name_height = display_text_layout(
                display_name.as_ref(),
                text.width,
                text.height,
                text_alignment,
            )
            .name_height;
            let centered_top = text_top + ((text.height - name_height).max(0.0) * 0.5);
            div()
                .absolute()
                .left(px(text_left))
                .top(px(centered_top))
                .w(px(text.width))
                .h(px(name_height))
                .min_w_0()
                .overflow_hidden()
                .text_sm()
                .line_height(px(ITEM_NAME_LINE_HEIGHT))
                .text_color(text_color)
                .whitespace_normal()
                .child(display_name)
        }
        ItemTileTextAlignment::Center => {
            let max_lines = (text.height / ITEM_NAME_LINE_HEIGHT).round().max(1.0) as usize;
            let label = div()
                .absolute()
                .left(px(text_left))
                .top(px(text_top))
                .w(px(text.width))
                .h(px(text.height))
                .min_w_0()
                .overflow_hidden()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .text_sm()
                .line_height(px(ITEM_NAME_LINE_HEIGHT))
                .text_center()
                .whitespace_nowrap()
                .text_color(text_color);

            if icon_name_lines.is_empty() {
                label.child(display_name)
            } else {
                icon_name_lines
                    .iter()
                    .take(max_lines)
                    .fold(label, |label, line| label.child(line.clone()))
            }
        }
    }
}

fn rename_text_view(
    pane_id: PaneId,
    display_name: SharedString,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
    rename_error: Option<&str>,
    rename_warning: Option<&str>,
    cx: &mut Context<FikaApp>,
) -> Div {
    let display_name_ref = display_name.as_ref();
    let visual = layout.visual_rect;
    let text = layout.text_rect;
    let show_helper = rename_error.is_some() || rename_warning.is_some();
    let rename_layout = rename_text_layout(text.height, show_helper);
    let helper_text = rename_error.or(rename_warning).unwrap_or_default();
    let helper_color = if rename_error.is_some() {
        rgb(0xdc2626)
    } else if rename_warning.is_some() {
        rgb(0xb45309)
    } else {
        rgb(0x6b7280)
    };
    let border_color = if rename_error.is_some() {
        rgb(0xdc2626)
    } else if rename_warning.is_some() {
        rgb(0xd97706)
    } else {
        rgb(0x2f6fed)
    };
    div()
        .absolute()
        .left(px(text.x - visual.x))
        .top(px(text.y - visual.y))
        .w(px(text.width))
        .h(px(text.height))
        .flex()
        .flex_col()
        .when(
            matches!(text_alignment, ItemTileTextAlignment::Start) && !show_helper,
            |view| view.justify_center(),
        )
        .child(
            rename_editor_view(
                pane_id,
                display_name_ref,
                selected,
                rename_caret,
                rename_selection,
                border_color,
                rename_layout.name_height,
                cx,
            )
            .when(
                matches!(text_alignment, ItemTileTextAlignment::Start),
                |editor| editor.relative().left(px(-1.0)).top(px(1.0)),
            ),
        )
        .when(show_helper, |view| {
            view.child(item_helper_label_view(
                helper_text,
                helper_color,
                rename_layout.helper_height,
                text_alignment,
            ))
        })
}

fn item_helper_label_view(
    helper_text: &str,
    helper_color: Rgba,
    height: f32,
    text_alignment: ItemTileTextAlignment,
) -> Div {
    match text_alignment {
        ItemTileTextAlignment::Start => div()
            .h(px(height))
            .min_h_0()
            .text_xs()
            .text_color(helper_color)
            .truncate()
            .child(helper_text.to_string()),
        ItemTileTextAlignment::Center => div()
            .h(px(height))
            .w_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .max_w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(helper_color)
                    .truncate()
                    .child(helper_text.to_string()),
            ),
    }
}

fn rename_editor_view(
    pane_id: PaneId,
    display_name: &str,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
    border_color: Rgba,
    height: f32,
    cx: &mut Context<FikaApp>,
) -> Div {
    div()
        .h(px(height))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .border_1()
        .rounded_sm()
        .border_color(border_color)
        .bg(rgb(0xffffff))
        .px(px(RENAME_TEXT_INSET_X))
        .cursor_text()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if this.set_rename_caret_from_window_position(pane_id, event.position) {
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .child(rename_name_view(
            display_name,
            SharedString::from(display_name),
            true,
            selected,
            rename_caret,
            rename_selection,
        ))
}

fn rename_name_view(
    display_name: &str,
    display_name_text: SharedString,
    renaming: bool,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
) -> Div {
    let text_color = if selected {
        rgb(0x0f172a)
    } else {
        rgb(0x24292f)
    };
    let base = div()
        .h_full()
        .min_w_0()
        .overflow_hidden()
        .text_sm()
        .line_height(px(ITEM_NAME_LINE_HEIGHT))
        .text_color(text_color)
        .when(renaming, |name| name.cursor_text());
    if !renaming {
        return base.whitespace_normal().child(display_name_text);
    }

    let base = base.whitespace_nowrap();
    if let Some((start, end)) = normalized_text_range(display_name, rename_selection) {
        return base
            .flex()
            .items_center()
            .child(display_name[..start].to_string())
            .child(
                div()
                    .bg(rgb(0xbfdbfe))
                    .text_color(rgb(0x0f172a))
                    .child(display_name[start..end].to_string()),
            )
            .child(display_name[end..].to_string());
    }

    let caret = clamp_text_boundary(display_name, rename_caret.unwrap_or(display_name.len()));
    base.flex()
        .items_center()
        .child(display_name[..caret].to_string())
        .child(rename_caret_view())
        .child(display_name[caret..].to_string())
}

fn rename_caret_view() -> Div {
    div().w(px(1.0)).h(px(16.0)).flex_none().bg(rgb(0x2f6fed))
}

fn rename_text_layout(text_height: f32, show_helper: bool) -> RenameTextLayout {
    let text_height = text_height.max(0.0);
    let name_height = text_height.min(RENAME_NAME_HEIGHT);
    RenameTextLayout {
        name_height,
        helper_height: if show_helper {
            (text_height - name_height).max(0.0)
        } else {
            0.0
        },
    }
}

fn display_text_layout(
    display_name: &str,
    text_width: f32,
    text_height: f32,
    text_alignment: ItemTileTextAlignment,
) -> RenameTextLayout {
    let text_height = text_height.max(0.0);
    if matches!(text_alignment, ItemTileTextAlignment::Center) {
        return RenameTextLayout {
            name_height: text_height,
            helper_height: 0.0,
        };
    }

    let required_name_height =
        layout::item_name_text_height_for_name(display_name, text_width).min(text_height);
    RenameTextLayout {
        name_height: required_name_height,
        helper_height: 0.0,
    }
}

fn normalized_text_range(text: &str, range: Option<(usize, usize)>) -> Option<(usize, usize)> {
    let (raw_start, raw_end) = range?;
    let start = clamp_text_boundary(text, raw_start.min(raw_end));
    let end = clamp_text_boundary(text, raw_start.max(raw_end));
    (start < end).then_some((start, end))
}

fn clamp_text_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{
        DetailsItemSnapshot, DetailsLayoutMetrics, DetailsPaintContent,
        DetailsRowDragStartRenderer, DetailsRowInteractionRenderer, DetailsRowRendererPolicy,
        DetailsRowVisualRenderer, DetailsTextShapeCacheKey, FileGridMode, FileGridRenderSnapshot,
        FileGridSnapshot, ItemBaseVisualRenderer, ItemDragStartRenderer, ItemImageRenderer,
        ItemInteractionRenderer, ItemPaintContent, ItemPaintSlotCache, ItemRenameEditorRenderer,
        ItemRendererPolicy, ItemTileTextAlignment, RendererPolicyStats, StaticItemLabelTextKey,
        StaticItemTextShapeCacheKey, VisibleItemSnapshot, details_columns,
        details_interaction_layer_items, details_renderer_policy_stats,
        details_row_renderer_policy, details_visual_layer_element_id, details_visual_layer_items,
        display_text_layout, drag_preview_label, item_drag_from_details_snapshot,
        item_identity_element_id, item_image_element_id, item_image_layer_item_source_path,
        item_image_layer_items, item_image_load_failure_paints_fallback,
        item_image_paint_layer_element_id, item_interaction_hitbox_bounds,
        item_interaction_layer_element_id, item_interaction_layer_items,
        item_mouse_down_opens_directory, item_renderer_policy, item_renderer_policy_stats,
        measured_viewport_for_scrollbar_axis, normalized_text_range, rename_text_layout,
        static_item_visual_layer_element_id, static_item_visual_layer_items,
        viewport_bounds_update_requires_notify,
    };
    use crate::ui::drag_drop::drag_preview_content_origin_for_cursor_offset;
    use crate::ui::icons::FileIconSnapshot;
    use crate::ui::item_view::ItemViewScrollbarAxis;
    use fika_core::{
        CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions, ItemId, ItemLayout,
        ViewRect, ViewState,
    };
    use gpui::{Bounds, Font, SharedString, point, px, size};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[test]
    fn drag_preview_uses_selection_count_only_for_selected_items() {
        assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
        assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
        assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
    }

    #[test]
    fn drag_preview_stays_near_cursor_independent_of_item_offset() {
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(point(px(48.0), px(12.0))),
            (56.0, 20.0)
        );
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(point(px(-4.0), px(-2.0))),
            (4.0, 6.0)
        );
        assert_eq!(
            drag_preview_content_origin_for_cursor_offset(point(px(-12.0), px(-10.0))),
            (-4.0, -2.0)
        );
    }

    #[test]
    fn item_interaction_id_is_keyed_by_item_identity_not_virtual_slot() {
        assert_eq!(
            item_identity_element_id("item-core", ItemId(7)),
            ("item-core", 7)
        );
        assert_ne!(
            item_identity_element_id("item-core", ItemId(7)),
            item_identity_element_id("item-core", ItemId(8))
        );
    }

    #[test]
    fn item_image_id_is_keyed_by_visual_slot_for_retained_image_cache() {
        assert_eq!(item_image_element_id(4), ("item-image", 4));
        assert_ne!(item_image_element_id(4), item_image_element_id(5));
    }

    #[test]
    fn item_image_paint_layer_id_is_keyed_by_pane_identity() {
        assert_eq!(
            item_image_paint_layer_element_id(fika_core::PaneId(7)),
            ("item-image-paint-layer", 7)
        );
        assert_ne!(
            item_image_paint_layer_element_id(fika_core::PaneId(7)),
            item_image_paint_layer_element_id(fika_core::PaneId(8))
        );
    }

    #[test]
    fn static_item_visual_layer_id_is_keyed_by_pane_identity() {
        assert_eq!(
            static_item_visual_layer_element_id(fika_core::PaneId(7)),
            ("static-item-visual-layer", 7)
        );
        assert_ne!(
            static_item_visual_layer_element_id(fika_core::PaneId(7)),
            static_item_visual_layer_element_id(fika_core::PaneId(8))
        );
    }

    #[test]
    fn item_interaction_layer_id_is_keyed_by_pane_identity() {
        assert_eq!(
            item_interaction_layer_element_id(fika_core::PaneId(7)),
            ("item-interaction-layer", 7)
        );
        assert_ne!(
            item_interaction_layer_element_id(fika_core::PaneId(7)),
            item_interaction_layer_element_id(fika_core::PaneId(8))
        );
    }

    #[test]
    fn details_visual_layer_id_is_keyed_by_pane_identity() {
        assert_eq!(
            details_visual_layer_element_id(fika_core::PaneId(7)),
            ("details-visual-layer", 7)
        );
        assert_ne!(
            details_visual_layer_element_id(fika_core::PaneId(7)),
            details_visual_layer_element_id(fika_core::PaneId(8))
        );
    }

    #[test]
    fn content_layers_split_base_visuals_from_image_visuals() {
        let mut cache = ItemPaintSlotCache::default();
        let static_item =
            test_visible_item(1, ItemId(7), "alpha.txt", test_item_layout(0.0), false);
        let mut thumbnail_item =
            test_visible_item(2, ItemId(8), "photo.png", test_item_layout(96.0), false);
        thumbnail_item.thumbnail_path = Some(Arc::from(Path::new("/tmp/photo.png")));
        let mut theme_icon_item =
            test_visible_item(3, ItemId(9), "app.desktop", test_item_layout(192.0), false);
        theme_icon_item.icon.path = Some(Arc::from(Path::new("/tmp/app.svg")));
        let mut rename_item =
            test_visible_item(4, ItemId(10), "draft.txt", test_item_layout(288.0), false);
        rename_item.draft_name = Some("draft-2.txt".to_string());
        let mut rename_thumbnail_item =
            test_visible_item(5, ItemId(11), "rename.png", test_item_layout(384.0), false);
        rename_thumbnail_item.thumbnail_path = Some(Arc::from(Path::new("/tmp/rename.png")));
        rename_thumbnail_item.draft_name = Some("rename-2.png".to_string());

        let projection = cache.project_file_grid_snapshot(
            icons_snapshot(vec![
                static_item,
                thumbnail_item,
                theme_icon_item,
                rename_item,
                rename_thumbnail_item,
            ]),
            None,
        );
        let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
            panic!("expected icons snapshot");
        };
        let visual_items = static_item_visual_layer_items(&items, ItemTileTextAlignment::Center);
        let image_items = item_image_layer_items(&items);
        let interaction_items = item_interaction_layer_items(&items);
        let policies = items
            .iter()
            .map(|item| item_renderer_policy(item.content.as_ref()))
            .collect::<Vec<_>>();
        let renderer_stats = item_renderer_policy_stats(&items);

        assert_eq!(
            policies,
            vec![
                ItemRendererPolicy {
                    base_visual: ItemBaseVisualRenderer::ContentLayer,
                    image: ItemImageRenderer::None,
                    interaction: ItemInteractionRenderer::RetainedLayer,
                    drag_start: ItemDragStartRenderer::GpuiShell,
                    rename_editor: ItemRenameEditorRenderer::None,
                },
                ItemRendererPolicy {
                    base_visual: ItemBaseVisualRenderer::ContentLayer,
                    image: ItemImageRenderer::ContentLayer,
                    interaction: ItemInteractionRenderer::RetainedLayer,
                    drag_start: ItemDragStartRenderer::GpuiShell,
                    rename_editor: ItemRenameEditorRenderer::None,
                },
                ItemRendererPolicy {
                    base_visual: ItemBaseVisualRenderer::ContentLayer,
                    image: ItemImageRenderer::ContentLayer,
                    interaction: ItemInteractionRenderer::RetainedLayer,
                    drag_start: ItemDragStartRenderer::GpuiShell,
                    rename_editor: ItemRenameEditorRenderer::None,
                },
                ItemRendererPolicy {
                    base_visual: ItemBaseVisualRenderer::ContentLayer,
                    image: ItemImageRenderer::None,
                    interaction: ItemInteractionRenderer::RenameShell,
                    drag_start: ItemDragStartRenderer::GpuiShell,
                    rename_editor: ItemRenameEditorRenderer::GpuiOverlay,
                },
                ItemRendererPolicy {
                    base_visual: ItemBaseVisualRenderer::ContentLayer,
                    image: ItemImageRenderer::ContentLayer,
                    interaction: ItemInteractionRenderer::RenameShell,
                    drag_start: ItemDragStartRenderer::GpuiShell,
                    rename_editor: ItemRenameEditorRenderer::GpuiOverlay,
                },
            ]
        );
        assert_eq!(
            renderer_stats,
            RendererPolicyStats {
                items: 5,
                visual_layer: 5,
                image_layer: 3,
                retained_interaction: 3,
                gpui_drag_shell: 5,
                rename_overlay: 2,
            }
        );

        assert_eq!(
            visual_items
                .iter()
                .map(|item| (item.item_id, item.paint_fallback_icon))
                .collect::<Vec<_>>(),
            vec![
                (ItemId(7), true),
                (ItemId(8), false),
                (ItemId(9), false),
                (ItemId(10), true),
                (ItemId(11), false)
            ]
        );
        assert_eq!(
            image_items
                .iter()
                .map(|item| item_image_layer_item_source_path(item)
                    .unwrap()
                    .as_ref()
                    .to_path_buf())
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("/tmp/photo.png"),
                PathBuf::from("/tmp/app.svg"),
                PathBuf::from("/tmp/rename.png")
            ]
        );
        assert!(!item_image_load_failure_paints_fallback(&image_items[0]));
        assert!(item_image_load_failure_paints_fallback(&image_items[1]));
        assert!(!item_image_load_failure_paints_fallback(&image_items[2]));
        assert_eq!(
            interaction_items
                .iter()
                .map(|item| item.item_id)
                .collect::<Vec<_>>(),
            vec![ItemId(7), ItemId(8), ItemId(9)]
        );
    }

    #[test]
    fn item_interaction_hitbox_bounds_are_layer_relative_visual_rects() {
        let bounds = item_interaction_hitbox_bounds(
            Bounds::new(point(px(20.0), px(30.0)), size(px(400.0), px(300.0))),
            ViewRect {
                x: 5.0,
                y: 7.0,
                width: 40.0,
                height: 24.0,
            },
        );

        assert_eq!(bounds.origin, point(px(25.0), px(37.0)));
        assert_eq!(bounds.size, size(px(40.0), px(24.0)));
    }

    #[test]
    fn static_text_shape_cache_key_ignores_item_origin_for_resize_reuse() {
        let font = Font::default();
        let key = StaticItemTextShapeCacheKey {
            item_id: ItemId(7),
            text_alignment: ItemTileTextAlignment::Start,
            paint_fallback_icon: true,
            text_font: font.clone(),
            marker_font: font,
            text_font_size_bits: 14.0f32.to_bits(),
            marker_font_size_bits: 12.0f32.to_bits(),
            label_line_height_bits: 20.0f32.to_bits(),
            marker_line_height_bits: 20.0f32.to_bits(),
            text_width_bits: 96.0f32.to_bits(),
            text_height_bits: 20.0f32.to_bits(),
            scale_factor_bits: 1.0f32.to_bits(),
            text_color: 0x24292f,
            fallback_fg: 0xffffff,
            fallback_marker: SharedString::from("TXT"),
            label: StaticItemLabelTextKey::Start(SharedString::from("alpha.txt")),
        };

        let moved_without_resize = key.clone();
        assert_eq!(key, moved_without_resize);

        let resized_text_rect = StaticItemTextShapeCacheKey {
            text_width_bits: 112.0f32.to_bits(),
            ..key.clone()
        };
        assert_ne!(key, resized_text_rect);

        let renamed_label = StaticItemTextShapeCacheKey {
            label: StaticItemLabelTextKey::Start(SharedString::from("beta.txt")),
            ..key.clone()
        };
        assert_ne!(key, renamed_label);
    }

    #[test]
    fn details_text_shape_cache_key_ignores_cell_geometry_for_resize_reuse() {
        let font = Font::default();
        let key = DetailsTextShapeCacheKey {
            text: SharedString::from("alpha.txt"),
            font,
            font_size_bits: 14.0f32.to_bits(),
            line_height_bits: 20.0f32.to_bits(),
            scale_factor_bits: 1.0f32.to_bits(),
            color: 0x1f2937,
        };

        let moved_or_resized_cell = key.clone();
        assert_eq!(key, moved_or_resized_cell);

        let selected_color = DetailsTextShapeCacheKey {
            color: 0x0f172a,
            ..key.clone()
        };
        assert_ne!(key, selected_color);

        let renamed_text = DetailsTextShapeCacheKey {
            text: SharedString::from("beta.txt"),
            ..key.clone()
        };
        assert_ne!(key, renamed_text);
    }

    #[test]
    fn item_paint_slot_cache_separates_content_geometry_and_visual_changes() {
        let mut cache = ItemPaintSlotCache::default();
        let base = test_visible_item(1, ItemId(7), "alpha.txt", test_item_layout(0.0), false);

        let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![base.clone()]), None);
        let stats = projection.stats;
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.entries, 1);
        let first_content = first_icon_paint_content(&projection.snapshot);

        let stats = cache
            .project_file_grid_snapshot(icons_snapshot(vec![base.clone()]), None)
            .stats;
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.entries, 1);

        let mut moved = base.clone();
        moved.layout = test_item_layout(18.0);
        let stats = cache
            .project_file_grid_snapshot(icons_snapshot(vec![moved.clone()]), None)
            .stats;
        assert_eq!(stats.geometry_changed, 1);
        assert_eq!(stats.entries, 1);

        let projection =
            cache.project_file_grid_snapshot(icons_snapshot(vec![moved.clone()]), Some(ItemId(7)));
        let stats = projection.stats;
        assert_eq!(stats.visual_changed, 1);
        assert_eq!(stats.entries, 1);
        assert!(Arc::ptr_eq(
            &first_content,
            &first_icon_paint_content(&projection.snapshot)
        ));

        let mut selected = moved.clone();
        selected.selected = true;
        let projection =
            cache.project_file_grid_snapshot(icons_snapshot(vec![selected.clone()]), None);
        let stats = projection.stats;
        assert_eq!(stats.visual_changed, 1);
        assert_eq!(stats.entries, 1);
        assert!(Arc::ptr_eq(
            &first_content,
            &first_icon_paint_content(&projection.snapshot)
        ));

        let mut renamed = selected.clone();
        renamed.display_name = SharedString::from("beta.txt");
        renamed.icon_name_lines = vec![SharedString::from("beta.txt")].into();
        let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![renamed]), None);
        let stats = projection.stats;
        assert_eq!(stats.content_changed, 1);
        assert_eq!(stats.entries, 1);
        assert!(!Arc::ptr_eq(
            &first_content,
            &first_icon_paint_content(&projection.snapshot)
        ));

        let stats = cache
            .project_file_grid_snapshot(icons_snapshot(Vec::new()), None)
            .stats;
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.entries, 0);
    }

    #[test]
    fn rename_overlay_changes_only_target_slot_content() {
        let mut cache = ItemPaintSlotCache::default();
        let alpha = test_visible_item(1, ItemId(7), "alpha.txt", test_item_layout(0.0), false);
        let beta = test_visible_item(2, ItemId(8), "beta.txt", test_item_layout(96.0), false);

        let projection = cache
            .project_file_grid_snapshot(icons_snapshot(vec![alpha.clone(), beta.clone()]), None);
        let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
            panic!("expected icons render snapshot");
        };
        let alpha_content = items[0].content.clone();
        let beta_content = items[1].content.clone();

        let mut beta_renaming = beta.clone();
        beta_renaming.draft_name = Some("beta-2.txt".to_string());
        beta_renaming.draft_caret = Some("beta".len());
        beta_renaming.draft_selection = Some((0, "beta".len()));
        beta_renaming.draft_error = Some("Name cannot be empty".to_string());
        beta_renaming.draft_warning =
            Some("Changing file extension may make it unusable".to_string());
        let projection = cache
            .project_file_grid_snapshot(icons_snapshot(vec![alpha.clone(), beta_renaming]), None);
        let stats = projection.stats;
        assert_eq!(stats.content_changed, 1);
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.entries, 2);

        let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
            panic!("expected icons render snapshot");
        };
        assert!(Arc::ptr_eq(&alpha_content, &items[0].content));
        assert!(!Arc::ptr_eq(&beta_content, &items[1].content));
        assert_eq!(items[1].content.draft_name.as_deref(), Some("beta-2.txt"));
        assert_eq!(items[1].content.draft_caret, Some("beta".len()));
        assert_eq!(items[1].content.draft_selection, Some((0, "beta".len())));
        assert_eq!(
            items[1].content.draft_error.as_deref(),
            Some("Name cannot be empty")
        );
        assert_eq!(
            items[1].content.draft_warning.as_deref(),
            Some("Changing file extension may make it unusable")
        );

        assert_eq!(
            static_item_visual_layer_items(&items, ItemTileTextAlignment::Center)
                .iter()
                .map(|item| item.item_id)
                .collect::<Vec<_>>(),
            vec![ItemId(7), ItemId(8)]
        );
        assert_eq!(
            item_interaction_layer_items(&items)
                .iter()
                .map(|item| item.item_id)
                .collect::<Vec<_>>(),
            vec![ItemId(7)]
        );

        let beta_renaming_content = items[1].content.clone();
        let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![alpha, beta]), None);
        let stats = projection.stats;
        assert_eq!(stats.content_changed, 1);
        assert_eq!(stats.unchanged, 1);
        assert_eq!(stats.entries, 2);

        let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
            panic!("expected icons render snapshot");
        };
        assert!(Arc::ptr_eq(&alpha_content, &items[0].content));
        assert!(!Arc::ptr_eq(&beta_renaming_content, &items[1].content));
        assert_eq!(
            item_interaction_layer_items(&items)
                .iter()
                .map(|item| item.item_id)
                .collect::<Vec<_>>(),
            vec![ItemId(7), ItemId(8)]
        );
    }

    #[test]
    fn details_rows_project_into_retained_paint_slots() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let alpha = test_details_item(0, ItemId(7), "alpha.txt");
        let beta = test_details_item(1, ItemId(8), "beta.txt");

        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![alpha.clone(), beta.clone()], metrics, 260.0),
            None,
        );
        assert_eq!(projection.stats.inserted, 2);
        assert_eq!(projection.stats.entries, 2);
        let FileGridRenderSnapshot::Details { items, .. } = &projection.snapshot else {
            panic!("expected details render snapshot");
        };
        assert_eq!(
            items
                .iter()
                .map(|item| (item.item_id, item.row_index))
                .collect::<Vec<_>>(),
            vec![(ItemId(7), 0), (ItemId(8), 1)]
        );
        let alpha_content = items[0].content.clone();

        let resized_metrics = DetailsLayoutMetrics {
            row_height: metrics.row_height + 4.0,
            ..metrics
        };
        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![alpha, beta], resized_metrics, 320.0),
            None,
        );
        assert_eq!(projection.stats.geometry_changed, 2);
        assert_eq!(projection.stats.entries, 2);
        assert!(Arc::ptr_eq(
            &alpha_content,
            &first_details_paint_content(&projection.snapshot)
        ));
    }

    #[test]
    fn details_selection_and_drop_target_are_visual_changes() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let base = test_details_item(0, ItemId(7), "alpha.txt");

        let projection = cache
            .project_file_grid_snapshot(details_snapshot(vec![base.clone()], metrics, 260.0), None);
        let first_content = first_details_paint_content(&projection.snapshot);

        let mut selected = base.clone();
        selected.selected = true;
        selected.selection_count = 3;
        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![selected.clone()], metrics, 260.0),
            None,
        );
        assert_eq!(projection.stats.visual_changed, 1);
        assert_eq!(projection.stats.entries, 1);
        assert!(Arc::ptr_eq(
            &first_content,
            &first_details_paint_content(&projection.snapshot)
        ));

        let mut drop_target = selected;
        drop_target.drop_target = true;
        let projection = cache
            .project_file_grid_snapshot(details_snapshot(vec![drop_target], metrics, 260.0), None);
        assert_eq!(projection.stats.visual_changed, 1);
        assert_eq!(projection.stats.entries, 1);
        assert!(Arc::ptr_eq(
            &first_content,
            &first_details_paint_content(&projection.snapshot)
        ));

        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![base.clone()], metrics, 260.0),
            Some(ItemId(7)),
        );
        assert_eq!(projection.stats.visual_changed, 1);
        assert_eq!(projection.stats.entries, 1);
        assert!(Arc::ptr_eq(
            &first_content,
            &first_details_paint_content(&projection.snapshot)
        ));
        let FileGridRenderSnapshot::Details { items, .. } = &projection.snapshot else {
            panic!("expected details render snapshot");
        };
        assert!(items[0].visual.hovered);
    }

    #[test]
    fn details_content_changes_replace_retained_content() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let base = test_details_item(0, ItemId(7), "alpha.txt");

        let projection = cache
            .project_file_grid_snapshot(details_snapshot(vec![base.clone()], metrics, 260.0), None);
        let first_content = first_details_paint_content(&projection.snapshot);

        let mut renamed = base.clone();
        renamed.name = Arc::from("beta.txt");
        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![renamed.clone()], metrics, 260.0),
            None,
        );
        assert_eq!(projection.stats.content_changed, 1);
        let renamed_content = first_details_paint_content(&projection.snapshot);
        assert!(!Arc::ptr_eq(&first_content, &renamed_content));

        let mut relabeled = renamed.clone();
        relabeled.size_label = "42 B".to_string();
        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![relabeled.clone()], metrics, 260.0),
            None,
        );
        assert_eq!(projection.stats.content_changed, 1);
        let relabeled_content = first_details_paint_content(&projection.snapshot);
        assert!(!Arc::ptr_eq(&renamed_content, &relabeled_content));

        let mut icon_changed = relabeled;
        icon_changed.icon.fallback_marker = Arc::from("BIN");
        let projection = cache
            .project_file_grid_snapshot(details_snapshot(vec![icon_changed], metrics, 260.0), None);
        assert_eq!(projection.stats.content_changed, 1);
        assert!(!Arc::ptr_eq(
            &relabeled_content,
            &first_details_paint_content(&projection.snapshot)
        ));
    }

    #[test]
    fn switching_from_details_clears_retained_details_slots() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let alpha = test_details_item(0, ItemId(7), "alpha.txt");
        let beta = test_details_item(1, ItemId(8), "beta.txt");

        let stats = cache
            .project_file_grid_snapshot(details_snapshot(vec![alpha, beta], metrics, 260.0), None)
            .stats;
        assert_eq!(stats.inserted, 2);
        assert_eq!(stats.entries, 2);

        let icon_item = test_visible_item(1, ItemId(9), "gamma.txt", test_item_layout(0.0), false);
        let stats = cache
            .project_file_grid_snapshot(icons_snapshot(vec![icon_item]), None)
            .stats;
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.removed, 2);
        assert_eq!(stats.entries, 1);

        let details_item = test_details_item(0, ItemId(10), "delta.txt");
        let stats = cache
            .project_file_grid_snapshot(details_snapshot(vec![details_item], metrics, 260.0), None)
            .stats;
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn details_visual_layer_items_project_rows_and_cells() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let mut item = test_details_item(2, ItemId(7), "alpha.txt");
        item.selected = true;
        item.size_label = "42 B".to_string();
        item.modified_label = "Today".to_string();
        let projection = cache.project_file_grid_snapshot(
            details_snapshot(vec![item], metrics, 260.0),
            Some(ItemId(7)),
        );
        let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
            panic!("expected details render snapshot");
        };
        let columns = details_columns(false, 260.0);
        let visual_items = details_visual_layer_items(&items, &columns);
        let policy = details_row_renderer_policy(&items[0]);
        let renderer_stats = details_renderer_policy_stats(&items);

        assert_eq!(
            policy,
            DetailsRowRendererPolicy {
                visual: DetailsRowVisualRenderer::ContentLayer,
                interaction: DetailsRowInteractionRenderer::RetainedLayer,
                drag_start: DetailsRowDragStartRenderer::GpuiShell,
            }
        );
        assert_eq!(
            renderer_stats,
            RendererPolicyStats {
                items: 1,
                visual_layer: 1,
                image_layer: 0,
                retained_interaction: 1,
                gpui_drag_shell: 1,
                rename_overlay: 0,
            }
        );

        assert_eq!(visual_items.len(), 1);
        assert_eq!(visual_items[0].row_index, 2);
        assert_eq!(
            visual_items[0].row_top,
            metrics.header_height + 2.0 * metrics.row_height
        );
        assert!(visual_items[0].selected);
        assert!(visual_items[0].hovered);
        assert_eq!(visual_items[0].cells.len(), 3);
        match &visual_items[0].cells[0].content {
            super::DetailsVisualCellContent::Name { name, icon } => {
                assert_eq!(name.as_ref(), "alpha.txt");
                assert_eq!(icon.fallback_marker.as_ref(), "TXT");
            }
            _ => panic!("expected name cell"),
        }
        match &visual_items[0].cells[1].content {
            super::DetailsVisualCellContent::Text { text } => {
                assert_eq!(text.as_ref(), "42 B");
            }
            _ => panic!("expected size text cell"),
        }
        match &visual_items[0].cells[2].content {
            super::DetailsVisualCellContent::Text { text } => {
                assert_eq!(text.as_ref(), "Today");
            }
            _ => panic!("expected modified text cell"),
        }
    }

    #[test]
    fn details_interaction_layer_items_use_retained_row_geometry() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let projection = cache.project_file_grid_snapshot(
            details_snapshot(
                vec![
                    test_details_item(0, ItemId(7), "alpha.txt"),
                    test_details_item(2, ItemId(9), "gamma.txt"),
                ],
                metrics,
                260.0,
            ),
            None,
        );
        let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
            panic!("expected details render snapshot");
        };

        let interaction_items = details_interaction_layer_items(&items, 320.0);

        assert_eq!(
            interaction_items
                .iter()
                .map(|item| item.item_id)
                .collect::<Vec<_>>(),
            vec![ItemId(7), ItemId(9)]
        );
        assert_eq!(
            interaction_items
                .iter()
                .map(|item| item.visual_rect)
                .collect::<Vec<_>>(),
            vec![
                ViewRect {
                    x: 0.0,
                    y: metrics.header_height,
                    width: 320.0,
                    height: metrics.row_height,
                },
                ViewRect {
                    x: 0.0,
                    y: metrics.header_height + metrics.row_height * 2.0,
                    width: 320.0,
                    height: metrics.row_height,
                },
            ]
        );
    }

    #[test]
    fn details_visual_layer_items_project_trash_columns_and_drop_state() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let mut item = test_details_item(0, ItemId(7), "trash.txt");
        item.drop_target = true;
        item.original_path_label = "/home/yk/trash.txt".to_string();
        item.deletion_time_label = "2026-06-17 10:00".to_string();
        let projection =
            cache.project_file_grid_snapshot(details_snapshot(vec![item], metrics, 260.0), None);
        let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
            panic!("expected details render snapshot");
        };
        let columns = details_columns(true, 260.0);
        let visual_items = details_visual_layer_items(&items, &columns);

        assert_eq!(visual_items[0].cells.len(), 5);
        assert!(visual_items[0].drop_target);
        match &visual_items[0].cells[3].content {
            super::DetailsVisualCellContent::Text { text } => {
                assert_eq!(text.as_ref(), "/home/yk/trash.txt");
            }
            _ => panic!("expected original path text cell"),
        }
        match &visual_items[0].cells[4].content {
            super::DetailsVisualCellContent::Text { text } => {
                assert_eq!(text.as_ref(), "2026-06-17 10:00");
            }
            _ => panic!("expected deletion time text cell"),
        }
    }

    #[test]
    fn details_item_drag_projection_preserves_retained_drag_start_fields() {
        let mut cache = ItemPaintSlotCache::default();
        let metrics = test_details_metrics();
        let mut item = test_details_item(0, ItemId(7), "folder");
        item.path = PathBuf::from("/tmp/folder");
        item.selected = true;
        item.selection_count = 4;
        item.icon.fallback_marker = Arc::from("DIR");
        let projection =
            cache.project_file_grid_snapshot(details_snapshot(vec![item], metrics, 260.0), None);
        let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
            panic!("expected details render snapshot");
        };

        let drag = item_drag_from_details_snapshot(fika_core::PaneId(3), &items[0]);

        assert_eq!(drag.pane_id, fika_core::PaneId(3));
        assert_eq!(drag.path.as_ref(), Path::new("/tmp/folder"));
        assert_eq!(drag.name.as_ref(), "folder");
        assert!(drag.selected);
        assert_eq!(drag.selection_count, 4);
        assert_eq!(drag.icon.fallback_marker.as_ref(), "DIR");
    }

    #[test]
    fn item_paint_content_preserves_directory_identity_for_drop_target_shells() {
        let mut cache = ItemPaintSlotCache::default();
        let mut item = test_visible_item(1, ItemId(7), "target", test_item_layout(0.0), false);
        item.is_dir = true;
        item.drag_path = Arc::from(Path::new("/tmp/target"));

        let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![item]), None);
        let content = first_icon_paint_content(&projection.snapshot);

        assert!(content.is_dir);
        assert_eq!(content.drag_path.as_ref(), Path::new("/tmp/target"));
    }

    fn icons_snapshot(items: Vec<VisibleItemSnapshot>) -> FileGridSnapshot {
        FileGridSnapshot::Icons {
            layout: IconsLayout::new(items.len(), IconsLayoutOptions::default()),
            items,
        }
    }

    fn details_snapshot(
        items: Vec<DetailsItemSnapshot>,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    ) -> FileGridSnapshot {
        FileGridSnapshot::Details {
            row_count: items.len(),
            items,
            metrics,
            name_column_width,
        }
    }

    fn first_icon_paint_content(snapshot: &FileGridRenderSnapshot) -> Arc<ItemPaintContent> {
        let FileGridRenderSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons render snapshot");
        };
        items[0].content.clone()
    }

    fn first_details_paint_content(snapshot: &FileGridRenderSnapshot) -> Arc<DetailsPaintContent> {
        let FileGridRenderSnapshot::Details { items, .. } = snapshot else {
            panic!("expected details render snapshot");
        };
        items[0].content.clone()
    }

    fn test_visible_item(
        slot_id: u64,
        item_id: ItemId,
        name: &str,
        layout: ItemLayout,
        selected: bool,
    ) -> VisibleItemSnapshot {
        VisibleItemSnapshot {
            slot_id,
            item_id,
            layout,
            is_dir: false,
            name: Arc::from(name),
            display_name: SharedString::from(name),
            thumbnail_path: None,
            icon: FileIconSnapshot {
                icon_name: Arc::from("text-x-generic"),
                path: None,
                fallback_marker: Arc::from("TXT"),
                fallback_fg: 0xffffff,
                fallback_bg: 0x2563eb,
            },
            fallback_marker: SharedString::from("TXT"),
            icon_name_lines: vec![SharedString::from(name)].into(),
            drag_path: Arc::from(Path::new("/tmp/alpha.txt")),
            selected,
            selection_count: if selected { 1 } else { 0 },
            drop_target: false,
            draft_name: None,
            draft_caret: None,
            draft_selection: None,
            draft_error: None,
            draft_warning: None,
        }
    }

    fn test_item_layout(x: f32) -> ItemLayout {
        ItemLayout {
            model_index: 0,
            column: 0,
            row: 0,
            item_rect: ViewRect {
                x,
                y: 0.0,
                width: 96.0,
                height: 84.0,
            },
            visual_rect: ViewRect {
                x,
                y: 0.0,
                width: 96.0,
                height: 84.0,
            },
            icon_rect: ViewRect {
                x: x + 24.0,
                y: 2.0,
                width: 48.0,
                height: 48.0,
            },
            text_rect: ViewRect {
                x: x + 4.0,
                y: 54.0,
                width: 88.0,
                height: 30.0,
            },
        }
    }

    fn test_details_metrics() -> DetailsLayoutMetrics {
        DetailsLayoutMetrics {
            header_height: 28.0,
            row_height: 22.0,
            icon_size: 18.0,
        }
    }

    fn test_details_item(row_index: usize, item_id: ItemId, name: &str) -> DetailsItemSnapshot {
        DetailsItemSnapshot {
            row_index,
            item_id,
            path: PathBuf::from(format!("/tmp/{name}")),
            is_dir: false,
            name: Arc::from(name),
            icon: FileIconSnapshot {
                icon_name: Arc::from("text-x-generic"),
                path: None,
                fallback_marker: Arc::from("TXT"),
                fallback_fg: 0xffffff,
                fallback_bg: 0x2563eb,
            },
            selected: false,
            selection_count: 0,
            drop_target: false,
            size_label: "-".to_string(),
            modified_label: "-".to_string(),
            original_path_label: "-".to_string(),
            deletion_time_label: "-".to_string(),
        }
    }

    #[test]
    fn measured_viewport_reserves_scrollbar_on_primary_axis_only() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(300.0), px(200.0)));

        let vertical = measured_viewport_for_scrollbar_axis(
            bounds,
            500.0,
            800.0,
            ItemViewScrollbarAxis::Vertical,
        );
        assert_eq!(vertical.rect.x, 10.0);
        assert_eq!(vertical.rect.y, 20.0);
        assert_eq!(vertical.rect.width, 286.0);
        assert_eq!(vertical.rect.height, 200.0);
        assert_eq!(vertical.max_scroll_x, 0.0);
        assert_eq!(vertical.max_scroll_y, 600.0);

        let horizontal = measured_viewport_for_scrollbar_axis(
            bounds,
            500.0,
            800.0,
            ItemViewScrollbarAxis::Horizontal,
        );
        assert_eq!(horizontal.rect.width, 300.0);
        assert_eq!(horizontal.rect.height, 186.0);
        assert_eq!(horizontal.max_scroll_x, 200.0);
        assert_eq!(horizontal.max_scroll_y, 0.0);
    }

    #[test]
    fn measured_compact_empty_layout_has_no_horizontal_scroll_range() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(300.0), px(200.0)));
        let layout = CompactLayout::new(
            0,
            CompactLayoutOptions {
                viewport_width: 720.0,
                viewport_height: 520.0,
                ..CompactLayoutOptions::default()
            },
        );
        let content_size = layout.content_size();

        let measured = measured_viewport_for_scrollbar_axis(
            bounds,
            content_size.width,
            content_size.height,
            ItemViewScrollbarAxis::Horizontal,
        );

        assert_eq!(measured.max_scroll_x, 0.0);
        assert_eq!(measured.max_scroll_y, 0.0);
    }

    #[test]
    fn projected_width_prepaint_update_does_not_require_second_notify() {
        let previous = ViewState {
            viewport_width: 320.0,
            viewport_height: 200.0,
            ..ViewState::default()
        };
        let next = ViewState {
            viewport_width: 286.0,
            viewport_height: 200.0,
            max_scroll_y: 600.0,
            ..previous.clone()
        };
        let measured = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 286.0,
            height: 200.0,
        };

        assert!(!viewport_bounds_update_requires_notify(
            Some(&previous),
            Some(&next),
            Some(286.0),
            measured,
        ));
        assert!(viewport_bounds_update_requires_notify(
            Some(&previous),
            Some(&next),
            None,
            measured,
        ));

        let scrolled = ViewState {
            scroll_y: 120.0,
            ..next
        };
        assert!(viewport_bounds_update_requires_notify(
            Some(&previous),
            Some(&scrolled),
            Some(286.0),
            measured,
        ));
    }

    #[test]
    fn rename_text_range_clamps_to_utf8_boundaries() {
        assert_eq!(
            normalized_text_range("目录.txt", Some((1, 5))),
            Some((0, 3))
        );
        assert_eq!(
            normalized_text_range("alpha.txt", Some((5, 2))),
            Some((2, 5))
        );
        assert_eq!(normalized_text_range("alpha.txt", Some((3, 3))), None);
    }

    #[test]
    fn rename_text_layout_keeps_editor_on_name_line() {
        let layout = rename_text_layout(40.0, true);

        assert_eq!(layout.name_height, 20.0);
        assert_eq!(layout.helper_height, 20.0);

        let without_helper = rename_text_layout(40.0, false);
        assert_eq!(without_helper.name_height, 20.0);
        assert_eq!(without_helper.helper_height, 0.0);

        let compact = rename_text_layout(12.0, true);
        assert_eq!(compact.name_height, 12.0);
        assert_eq!(compact.helper_height, 0.0);
    }

    #[test]
    fn display_text_layout_keeps_dolphin_default_to_name_only() {
        let layout = display_text_layout("alpha.txt", 120.0, 40.0, ItemTileTextAlignment::Start);

        assert!(layout.name_height > 0.0);
        assert_eq!(layout.helper_height, 0.0);
    }

    #[test]
    fn double_mouse_down_opens_directory_before_click_synthesis() {
        assert!(item_mouse_down_opens_directory(
            true,
            FileGridMode::Manager,
            2
        ));
        assert!(!item_mouse_down_opens_directory(
            true,
            FileGridMode::Manager,
            1
        ));
        assert!(!item_mouse_down_opens_directory(
            false,
            FileGridMode::Manager,
            2
        ));
    }
}

pub(crate) fn compact_layout_options(
    view: &ViewState,
    reserved_bottom: f32,
) -> CompactLayoutOptions {
    let icon_size = view.icon_size();
    let padding = DOLPHIN_ITEM_PADDING;
    let side_padding = DOLPHIN_COMPACT_SIDE_PADDING;
    let gap = DOLPHIN_COMPACT_COLUMN_GAP;
    let text_gap = DOLPHIN_COMPACT_TEXT_GAP;
    let text_height = DEFAULT_TILE_TEXT_HEIGHT;
    CompactLayoutOptions {
        viewport_width: view.viewport_width.max(1.0),
        viewport_height: view.viewport_height.max(1.0),
        reserved_bottom,
        scroll_x: view.scroll_x,
        scroll_y: view.scroll_y,
        padding,
        side_padding,
        gap,
        text_gap,
        item_width: icon_size + DOLPHIN_COMPACT_BASE_TEXT_WIDTH + padding * 2.0 + text_gap,
        item_height: padding * 2.0 + icon_size.max(text_height),
        icon_size,
        text_height,
        ..CompactLayoutOptions::default()
    }
}

pub(crate) fn icons_layout_options(view: &ViewState, reserved_bottom: f32) -> IconsLayoutOptions {
    let icon_size = view.icon_size();
    let padding = DOLPHIN_ITEM_PADDING;
    let gap = DOLPHIN_ICON_MARGIN;
    let text_height = ITEM_NAME_LINE_HEIGHT * DOLPHIN_ICON_MAX_TEXT_LINES as f32;
    let zoom_factor = (view.zoom_level as f32 / 13.0).exp();
    let item_width = (16.0
        + DOLPHIN_ICON_TEXT_WIDTH_INDEX * 64.0 * DOLPHIN_ICON_FONT_FACTOR * zoom_factor)
        .max(icon_size + padding * 2.0 * zoom_factor)
        .floor();
    IconsLayoutOptions {
        viewport_width: view.viewport_width.max(1.0),
        viewport_height: view.viewport_height.max(1.0),
        reserved_bottom,
        scroll_x: view.scroll_x,
        scroll_y: view.scroll_y,
        padding,
        gap,
        item_width,
        item_height: padding * 3.0 + icon_size + text_height,
        icon_size,
        text_height,
        ..IconsLayoutOptions::default()
    }
}
