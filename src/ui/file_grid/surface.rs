use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, Window, div, px};

use crate::FikaApp;
use crate::ui::icons::theme_icon_image_size_px;
use crate::ui::retained::{RetainedImageRequest, RetainedThemeIconCacheRefreshStats};

use super::details::{details_content_height, details_content_width};
use super::details_shell::details_table;
use super::image_layer::{icon_paint_mode_for_selected, item_image_layer_view};
use super::interaction::item_interaction_layer_view;
use super::item_shell::item_tile;
use super::renderer_policy::{
    DetailsRowVisualRenderer, ItemRendererPolicyInput, details_renderer_policy_stats,
    details_row_renderer_policy, item_renderer_policy_stats, item_uses_image_layer_with_input,
    item_uses_layer_visual_paint,
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
    app_state: &mut FikaApp,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let perf_enabled = super::item_view_perf_enabled();
    let build_started = perf_enabled.then(std::time::Instant::now);
    let FileGridProps {
        pane_id,
        snapshot,
        warm_static_visual_snapshot,
        trash_view,
        scroll_handle,
        rubber_band,
        drop_target,
        mode,
    } = props;
    let app = cx.weak_entity();
    let scrollbar_axis = scrollbar_axis_for_snapshot(&snapshot);
    let view_mode = view_mode_for_snapshot(&snapshot);

    let (
        content_width,
        content_height,
        visible_count,
        renderer_policy_stats,
        viewport,
        theme_icon_cache_refresh_stats,
    ) = match snapshot {
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
            let renderer_policy_stats = item_renderer_policy_stats(&visible_items);
            let theme_icon_cache_refresh_stats =
                refresh_visible_item_theme_icon_cache(app_state, &items, window, cx);
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
                    item_tile(
                        pane_id,
                        item,
                        ItemTileTextAlignment::Center,
                        ItemRendererPolicyInput::default(),
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
                theme_icon_cache_refresh_stats,
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
            let renderer_policy_stats = item_renderer_policy_stats(&visible_items);
            let theme_icon_cache_refresh_stats =
                refresh_visible_item_theme_icon_cache(app_state, &items, window, cx);
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
                    item_tile(
                        pane_id,
                        item,
                        ItemTileTextAlignment::Start,
                        ItemRendererPolicyInput::default(),
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
                theme_icon_cache_refresh_stats,
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
            let theme_icon_cache_refresh_stats =
                refresh_visible_details_theme_icon_cache(app_state, &items, window, cx);
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
                theme_icon_cache_refresh_stats,
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
                        "[fika item-image] pane={} mode={:?} prepaint_count={} prepaint={}us paint_count={} paint={}us theme_loaded={} theme_decoded={} theme_retained={} theme_placeholder={} thumb_loaded={} thumb_decoded={} thumb_retained={} thumb_fallback={}",
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
        if theme_icon_cache_refresh_stats.has_activity() {
            eprintln!(
                "[fika item-image-cache-refresh] pane={} mode={:?} requested={} retained={} loaded={} decoded={} missing={} non_svg={} total={}us entries={} bytes={} evicted={}",
                pane_id.0,
                view_mode,
                theme_icon_cache_refresh_stats.requested,
                theme_icon_cache_refresh_stats.retained,
                theme_icon_cache_refresh_stats.loaded,
                theme_icon_cache_refresh_stats.decoded,
                theme_icon_cache_refresh_stats.missing,
                theme_icon_cache_refresh_stats.non_svg,
                theme_icon_cache_refresh_stats.elapsed_us,
                theme_icon_cache_refresh_stats.cache_entries,
                theme_icon_cache_refresh_stats.cache_bytes,
                theme_icon_cache_refresh_stats.evicted,
            );
        }
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

fn refresh_visible_item_theme_icon_cache(
    app_state: &mut FikaApp,
    items: &[super::ItemPaintSnapshot],
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> RetainedThemeIconCacheRefreshStats {
    let scale_factor = window.scale_factor();
    app_state.refresh_retained_theme_icon_requests_retained_only(
        items
            .iter()
            .filter_map(|item| item_theme_icon_cache_refresh_request(item, scale_factor)),
        cx,
        window,
    )
}

fn refresh_visible_details_theme_icon_cache(
    app_state: &mut FikaApp,
    items: &[super::DetailsPaintSnapshot],
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> RetainedThemeIconCacheRefreshStats {
    let scale_factor = window.scale_factor();
    app_state.refresh_retained_theme_icon_requests_retained_only(
        items
            .iter()
            .filter_map(|item| details_theme_icon_cache_refresh_request(item, scale_factor)),
        cx,
        window,
    )
}

pub(super) fn item_theme_icon_cache_refresh_request(
    item: &super::ItemPaintSnapshot,
    scale_factor: f32,
) -> Option<RetainedImageRequest> {
    if !item.visible {
        return None;
    }
    let content = item.content.as_ref();
    if content.thumbnail_path.is_some() {
        return None;
    }
    if !item_uses_layer_visual_paint(content) {
        return None;
    }
    if !item_uses_image_layer_with_input(content, ItemRendererPolicyInput::default()) {
        return None;
    }
    RetainedImageRequest::theme_icon_for_snapshot_with_mode(
        &content.icon,
        theme_icon_image_size_px(item.layout.icon_rect.width, item.layout.icon_rect.height),
        scale_factor,
        icon_paint_mode_for_selected(item.visual.selected),
    )
}

pub(super) fn details_theme_icon_cache_refresh_request(
    item: &super::DetailsPaintSnapshot,
    scale_factor: f32,
) -> Option<RetainedImageRequest> {
    let policy = details_row_renderer_policy(item);
    if !matches!(policy.visual, DetailsRowVisualRenderer::ContentLayer) {
        return None;
    }
    RetainedImageRequest::theme_icon_for_snapshot_with_mode(
        &item.content.icon,
        theme_icon_image_size_px(
            f32::from_bits(item.geometry.icon_size),
            f32::from_bits(item.geometry.icon_size),
        ),
        scale_factor,
        icon_paint_mode_for_selected(item.visual.selected),
    )
}
