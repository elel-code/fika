mod controller;
mod details;
mod details_shell;
mod details_visual;
mod dnd;
mod icon_work;
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

pub(crate) use details::DetailsLayoutMetrics;
pub(crate) use details_visual::DetailsTextShapeCache;
pub(crate) use dnd::ItemDrag;
pub(crate) use icon_work::{
    DOLPHIN_VISIBLE_ICON_SYNC_BUDGET, FileIconResolveQueue,
    queue_file_icon_resolve_work_for_raw_grid, resolve_visible_file_icons_for_raw_grid,
};
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
    DetailsVisualPerfStats, ItemImagePerfStats, ItemImageSourcePerfStats, ItemInteractionPerfStats,
    ItemViewPerfFrameState, StaticItemVisualPerfStats, item_view_perf_enabled,
};
pub(crate) use projection::{
    ContentItemHit, PaneLayoutProjection, PaneLayoutProjectionInput, content_item_hit_at_point,
    model_indexes_intersecting_visual_rect, pane_layout_projection,
};
pub(crate) use slots::VisibleItemSlotPool;
pub(crate) use snapshot::{
    PaneVisibleWorkKey, RawFileGridSnapshot, RawFileGridSnapshotInput, VisibleItemSnapshotCache,
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

use paint_slots::ItemPaintContent;

#[cfg(test)]
mod tests;
