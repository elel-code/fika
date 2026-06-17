mod controller;
mod details;
mod details_shell;
mod details_visual;
mod dnd;
mod image_layer;
mod interaction;
mod item_shell;
mod layout;
mod paint_slots;
mod perf;
mod projection;
mod rename_overlay;
mod renderer_policy;
mod slots;
mod snapshot;
mod static_visual;
mod style;
mod surface;
mod text;
mod types;
mod viewport;

#[cfg(test)]
pub(crate) use details::DetailsItemSnapshot;
pub(crate) use details::DetailsLayoutMetrics;
pub(crate) use details_visual::DetailsTextShapeCache;
pub(crate) use dnd::ItemDrag;
pub(crate) use layout::{
    CompactColumnWidthCache, ITEM_NAME_LINE_HEIGHT, compact_text_width,
    compact_text_width_for_name, rename_editor_required_text_width,
};
#[cfg(test)]
pub(crate) use layout::{
    DOLPHIN_ICON_MAX_TEXT_LINES, compact_layout_options, icons_layout_options,
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
#[cfg(test)]
pub(crate) use snapshot::VisibleItemSnapshot;
pub(crate) use snapshot::{
    RawFileGridSnapshot, RawFileGridSnapshotInput, VisibleItemSnapshotCache,
    deferred_thumbnail_candidates_for_model, raw_file_grid_snapshot,
};
pub(crate) use static_visual::StaticItemTextShapeCache;
pub(crate) use surface::file_grid;
pub(crate) use types::{
    FileGridMode, FileGridProps, FileGridRenderSnapshot, FileGridSnapshot, PaneViewportGeometry,
};

use style::{
    ItemTileTextAlignment, TextShapeCacheStats, details_row_background, item_identity_element_id,
    item_tile_background,
};

#[cfg(test)]
use controller::item_mouse_down_opens_directory;
#[cfg(test)]
use details::details_columns;
#[cfg(test)]
use details_visual::{
    DetailsTextShapeCacheKey, DetailsVisualCellContent, details_visual_layer_element_id,
    details_visual_layer_items,
};
#[cfg(test)]
use dnd::drag_preview_label;
#[cfg(test)]
use dnd::item_drag_from_details_snapshot;
#[cfg(test)]
use image_layer::{
    item_image_layer_item_source_path, item_image_layer_items,
    item_image_load_failure_paints_fallback, item_image_paint_layer_element_id,
    item_image_pending_load_paints_fallback,
};
#[cfg(test)]
use interaction::{
    details_interaction_layer_items, item_interaction_hitbox_bounds,
    item_interaction_layer_element_id, item_interaction_layer_items,
};
#[cfg(test)]
use paint_slots::DetailsPaintContent;
use paint_slots::ItemPaintContent;
#[cfg(test)]
use rename_overlay::{display_text_layout, normalized_text_range, rename_text_layout};
#[cfg(test)]
use renderer_policy::{
    DetailsRowDragStartRenderer, DetailsRowInteractionRenderer, DetailsRowRendererPolicy,
    DetailsRowVisualRenderer, ItemBaseVisualRenderer, ItemDragStartRenderer, ItemImageRenderer,
    ItemInteractionRenderer, ItemRenameEditorRenderer, ItemRendererPolicy, RendererPolicyStats,
    details_renderer_policy_stats, details_row_renderer_policy, item_renderer_policy,
    item_renderer_policy_stats,
};
#[cfg(test)]
use static_visual::{
    StaticItemLabelTextKey, StaticItemTextShapeCacheKey, static_item_visual_layer_element_id,
    static_item_visual_layer_items,
};
#[cfg(test)]
use viewport::{measured_viewport_for_scrollbar_axis, viewport_bounds_update_requires_notify};

#[cfg(test)]
mod tests;
