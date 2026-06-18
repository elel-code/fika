mod autosmoke;
mod controller;
mod details;
mod details_shell;
mod details_visual;
mod dnd;
mod hover;
mod icon_work;
mod image_layer;
mod interaction;
mod item_shell;
mod layout;
mod lifecycle;
mod paint_slots;
mod perf;
mod projection;
mod rename_overlay;
mod renderer_policy;
mod retained;
mod slots;
mod snapshot;
mod static_visual;
mod style;
mod surface;
mod text;
mod types;
mod viewport;

pub(crate) use autosmoke::{
    ItemViewAutosmokeAction, ItemViewAutosmokeScenario, emit_item_view_autosmoke_complete,
    emit_item_view_autosmoke_scroll_action, emit_item_view_autosmoke_start,
    emit_item_view_autosmoke_zoom_action,
};
pub(crate) use details::DetailsLayoutMetrics;
pub(crate) use details_visual::DetailsTextShapeCache;
pub(crate) use dnd::ItemDrag;
pub(crate) use hover::RetainedHoveredItem;
pub(crate) use icon_work::FileIconResolveQueue;
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
    ItemViewPerfState, StaticItemVisualPerfStats, item_view_perf_enabled,
};
pub(crate) use projection::{
    ContentItemHit, PaneLayoutProjection, PaneLayoutProjectionInput,
    pane_content_item_hit_at_point, pane_layout_projection,
    pane_model_indexes_intersecting_visual_rect,
};
#[cfg(test)]
pub(crate) use retained::THUMBNAIL_PROBE_BATCH_SIZE;
pub(crate) use slots::VisibleItemSlotPool;
#[cfg(test)]
pub(crate) use snapshot::QueuedVisibleModelWork;
#[cfg(test)]
pub(crate) use snapshot::RawFileGridSnapshot;
pub(crate) use snapshot::{PaneVisibleWorkKey, VisibleItemSnapshotCache};
pub(crate) use static_visual::StaticItemTextShapeCache;
pub(crate) use surface::file_grid;
pub(crate) use types::{
    FileGridMode, FileGridProps, FileGridRenderSnapshot, FileGridSnapshot, PaneViewportGeometry,
};
pub(crate) use viewport::{
    clamped_content_point_from_window_position, content_point_from_window_position,
    pane_at_window_position,
};

use style::{
    ItemTileTextAlignment, TextShapeCacheStats, details_row_background, item_identity_element_id,
    item_tile_background,
};

use paint_slots::ItemPaintContent;

#[cfg(test)]
mod tests;
