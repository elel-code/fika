use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Window, div, px};

use crate::FikaApp;

use super::details::{details_content_height, details_content_width};
use super::details_shell::details_table;
use super::image_layer::{
    item_image_layer_view, item_renderer_policy_input_for_theme_handoff,
    visible_theme_icon_handoff_ready,
};
use super::interaction::item_interaction_layer_view;
use super::item_shell::item_tile;
use super::renderer_policy::{
    details_renderer_policy_stats, item_renderer_policy_stats_with_input,
};
use super::static_visual::{static_item_visual_layer_view, static_item_visual_warm_layer_view};
use super::viewport::{
    file_grid_viewport_shell, measured_viewport_for_scrollbar_axis, scrollbar_axis_for_snapshot,
    view_mode_for_snapshot, viewport_bounds_update_requires_notify,
};
use super::{
    DetailsVisualPerfStats, FileGridProps, FileGridRenderSnapshot, GlyphRasterBudgetStats,
    ItemImagePerfStats, ItemInteractionPerfStats, ItemTileTextAlignment, StaticItemVisualPerfStats,
    TextShapeCacheStats,
};
use crate::ui::item_view::item_view_scrollbar_container;

pub(crate) fn file_grid(
    props: FileGridProps,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let perf_enabled = super::item_view_perf_enabled();
    let build_started = perf_enabled.then(std::time::Instant::now);
    let FileGridProps {
        pane_id,
        snapshot,
        warm_static_visual_snapshot,
        theme_icon_readiness,
        trash_view,
        scroll_handle,
        rubber_band,
        drop_target,
        mode,
    } = props;
    let app = cx.weak_entity();
    let scale_factor = window.scale_factor();
    let scrollbar_axis = scrollbar_axis_for_snapshot(&snapshot);
    let view_mode = view_mode_for_snapshot(&snapshot);

    let (content_width, content_height, visible_count, renderer_policy_stats, viewport) =
        match snapshot {
            FileGridRenderSnapshot::Icons {
                layout: icons_layout,
                items,
            } => {
                let content_size = icons_layout.content_size();
                let visible_items = items
                    .iter()
                    .filter(|item| item.visible)
                    .cloned()
                    .collect::<Vec<_>>();
                let visible_count = visible_items.len();
                let theme_icon_handoff_ready = visible_theme_icon_handoff_ready(
                    &visible_items,
                    &theme_icon_readiness,
                    scale_factor,
                );
                let renderer_policy_stats =
                    item_renderer_policy_stats_with_input(&visible_items, |item| {
                        item_renderer_policy_input_for_theme_handoff(
                            item,
                            &theme_icon_readiness,
                            scale_factor,
                            theme_icon_handoff_ready,
                        )
                    });
                let warm_static_visual_layer =
                    warm_static_visual_snapshot.as_ref().and_then(|snapshot| {
                        static_item_visual_warm_layer_view(pane_id, snapshot, app.clone())
                    });
                let static_visual_layer = static_item_visual_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    ItemTileTextAlignment::Center,
                    true,
                    app.clone(),
                );
                let image_layer = item_image_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    &theme_icon_readiness,
                    scale_factor,
                    theme_icon_handoff_ready,
                    app.clone(),
                );
                let interaction_layer = item_interaction_layer_view(
                    pane_id,
                    &visible_items,
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
                let content = if let Some(layer) = warm_static_visual_layer {
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
                    content.children(visible_items.into_iter().map(|item| {
                        let renderer_policy_input = item_renderer_policy_input_for_theme_handoff(
                            &item,
                            &theme_icon_readiness,
                            scale_factor,
                            theme_icon_handoff_ready,
                        );
                        item_tile(
                            pane_id,
                            item,
                            ItemTileTextAlignment::Center,
                            renderer_policy_input,
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
                let visible_items = items
                    .iter()
                    .filter(|item| item.visible)
                    .cloned()
                    .collect::<Vec<_>>();
                let visible_count = visible_items.len();
                let theme_icon_handoff_ready = visible_theme_icon_handoff_ready(
                    &visible_items,
                    &theme_icon_readiness,
                    scale_factor,
                );
                let renderer_policy_stats =
                    item_renderer_policy_stats_with_input(&visible_items, |item| {
                        item_renderer_policy_input_for_theme_handoff(
                            item,
                            &theme_icon_readiness,
                            scale_factor,
                            theme_icon_handoff_ready,
                        )
                    });
                let warm_static_visual_layer =
                    warm_static_visual_snapshot.as_ref().and_then(|snapshot| {
                        static_item_visual_warm_layer_view(pane_id, snapshot, app.clone())
                    });
                let static_visual_layer = static_item_visual_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    ItemTileTextAlignment::Start,
                    true,
                    app.clone(),
                );
                let image_layer = item_image_layer_view(
                    pane_id,
                    &items,
                    content_size.width,
                    content_size.height,
                    &theme_icon_readiness,
                    scale_factor,
                    theme_icon_handoff_ready,
                    app.clone(),
                );
                let interaction_layer = item_interaction_layer_view(
                    pane_id,
                    &visible_items,
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
                let content = if let Some(layer) = warm_static_visual_layer {
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
                    content.children(visible_items.into_iter().map(|item| {
                        let renderer_policy_input = item_renderer_policy_input_for_theme_handoff(
                            &item,
                            &theme_icon_readiness,
                            scale_factor,
                            theme_icon_handoff_ready,
                        );
                        item_tile(
                            pane_id,
                            item,
                            ItemTileTextAlignment::Start,
                            renderer_policy_input,
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
            FileGridRenderSnapshot::Details {
                items,
                row_count,
                metrics,
                name_column_width,
            } => {
                let content_width = details_content_width(trash_view, name_column_width).max(1.0);
                let content_height = details_content_height(row_count, metrics).max(1.0);
                let visible_count = items.len();
                let mut renderer_policy_stats = details_renderer_policy_stats(&items);
                renderer_policy_stats.details_header_visual_layer = 1;
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
            let mut glyph_cache_stats = TextShapeCacheStats::default();
            let mut details_shape_cache_stats = TextShapeCacheStats::default();
            let mut details_glyph_cache_stats = TextShapeCacheStats::default();
            let mut static_visual_stats = StaticItemVisualPerfStats::default();
            let mut static_glyph_budget_stats = GlyphRasterBudgetStats::default();
            let mut image_stats = ItemImagePerfStats::default();
            let mut details_visual_stats = DetailsVisualPerfStats::default();
            let mut details_glyph_budget_stats = GlyphRasterBudgetStats::default();
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
                    glyph_cache_stats = this.take_static_item_glyph_raster_cache_stats(pane_id);
                    details_shape_cache_stats = this.take_details_text_shape_cache_stats(pane_id);
                    details_glyph_cache_stats =
                        this.take_details_glyph_raster_cache_stats(pane_id);
                    static_visual_stats = this.take_static_item_visual_perf_stats(pane_id);
                    static_glyph_budget_stats =
                        this.take_static_item_glyph_budget_stats(pane_id);
                    image_stats = this.take_item_image_perf_stats(pane_id);
                    details_visual_stats = this.take_details_visual_perf_stats(pane_id);
                    details_glyph_budget_stats = this.take_details_glyph_budget_stats(pane_id);
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
                        "[fika item-shape-cache] pane={} mode={:?} hits={} misses={} evicted={} compute={}us entries={}",
                        pane_id.0,
                        view_mode,
                        shape_cache_stats.hits,
                        shape_cache_stats.misses,
                        shape_cache_stats.evicted,
                        shape_cache_stats.compute_us,
                        shape_cache_stats.entries,
                    );
                }
                if glyph_cache_stats.has_activity() {
                    eprintln!(
                        "[fika item-glyph-cache] pane={} mode={:?} hits={} misses={} evicted={} entries={}",
                        pane_id.0,
                        view_mode,
                        glyph_cache_stats.hits,
                        glyph_cache_stats.misses,
                        glyph_cache_stats.evicted,
                        glyph_cache_stats.entries,
                    );
                }
                if static_glyph_budget_stats.has_activity() {
                    eprintln!(
                        "[fika item-glyph-budget] pane={} mode={:?} requested={} hits={} misses={} computed={} deferred={} failed={} budget_exhausted={} compute={}us",
                        pane_id.0,
                        view_mode,
                        static_glyph_budget_stats.requested,
                        static_glyph_budget_stats.cache_hits,
                        static_glyph_budget_stats.cache_misses,
                        static_glyph_budget_stats.computed,
                        static_glyph_budget_stats.deferred,
                        static_glyph_budget_stats.failed,
                        static_glyph_budget_stats.budget_exhausted,
                        static_glyph_budget_stats.compute_us,
                    );
                }
                if details_shape_cache_stats.has_activity() {
                    eprintln!(
                        "[fika details-shape-cache] pane={} mode={:?} hits={} misses={} evicted={} compute={}us entries={}",
                        pane_id.0,
                        view_mode,
                        details_shape_cache_stats.hits,
                        details_shape_cache_stats.misses,
                        details_shape_cache_stats.evicted,
                        details_shape_cache_stats.compute_us,
                        details_shape_cache_stats.entries,
                    );
                }
                if details_glyph_cache_stats.has_activity() {
                    eprintln!(
                        "[fika details-glyph-cache] pane={} mode={:?} hits={} misses={} evicted={} entries={}",
                        pane_id.0,
                        view_mode,
                        details_glyph_cache_stats.hits,
                        details_glyph_cache_stats.misses,
                        details_glyph_cache_stats.evicted,
                        details_glyph_cache_stats.entries,
                    );
                }
                if details_glyph_budget_stats.has_activity() {
                    eprintln!(
                        "[fika details-glyph-budget] pane={} mode={:?} requested={} hits={} misses={} computed={} deferred={} failed={} budget_exhausted={} compute={}us",
                        pane_id.0,
                        view_mode,
                        details_glyph_budget_stats.requested,
                        details_glyph_budget_stats.cache_hits,
                        details_glyph_budget_stats.cache_misses,
                        details_glyph_budget_stats.computed,
                        details_glyph_budget_stats.deferred,
                        details_glyph_budget_stats.failed,
                        details_glyph_budget_stats.budget_exhausted,
                        details_glyph_budget_stats.compute_us,
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
                        "[fika item-image] pane={} mode={:?} prepaint_count={} prepaint={}us paint_count={} paint={}us theme_loaded={} theme_decoded={} theme_retained={} theme_placeholder={} theme_prewarm_loaded={} theme_prewarm_decoded={} theme_prewarm_retained={} theme_prewarm_pending={} thumb_loaded={} thumb_decoded={} thumb_retained={} thumb_fallback={}",
                        pane_id.0,
                        view_mode,
                        image_stats.prepaint_count,
                        image_stats.prepaint_us,
                        image_stats.paint_count,
                        image_stats.paint_us,
                        image_stats.sources.theme_loaded,
                        image_stats.sources.theme_decoded,
                        image_stats.sources.theme_retained,
                        image_stats.sources.theme_placeholder,
                        image_stats.sources.theme_prewarm_loaded,
                        image_stats.sources.theme_prewarm_decoded,
                        image_stats.sources.theme_prewarm_retained,
                        image_stats.sources.theme_prewarm_pending,
                        image_stats.sources.thumbnail_loaded,
                        image_stats.sources.thumbnail_decoded,
                        image_stats.sources.thumbnail_retained,
                        image_stats.sources.thumbnail_fallback,
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
            "[fika renderer-policy] pane={} mode={:?} items={} visual_layer={} image_layer={} gpui_image_element={} retained_interaction={} retained_directory_drop_target={} gpui_drag_shell={} gpui_directory_drop_shell={} details_header_visual_layer={} gpui_details_header={} rename_overlay={}",
            pane_id.0,
            view_mode,
            renderer_policy_stats.items,
            renderer_policy_stats.visual_layer,
            renderer_policy_stats.image_layer,
            renderer_policy_stats.gpui_image_element,
            renderer_policy_stats.retained_interaction,
            renderer_policy_stats.retained_directory_drop_target,
            renderer_policy_stats.gpui_drag_shell,
            renderer_policy_stats.gpui_directory_drop_shell,
            renderer_policy_stats.details_header_visual_layer,
            renderer_policy_stats.gpui_details_header,
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
