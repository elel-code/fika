mod details;
mod layout;
mod projection;
mod slots;
mod snapshot;

pub(crate) use details::{
    DetailsItemSnapshot, DetailsLayoutMetrics, details_content_height, details_content_width,
};
pub(crate) use layout::{
    CompactColumnWidthCache, compact_text_width, compact_text_width_for_name,
    rename_editor_required_text_width,
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
    PaneId, ViewMode, ViewRect, ViewState, normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Div, Element, ElementId, Empty, ExternalPaths, Font, FontWeight,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, MouseButton, NavigationDirection,
    ParentElement, Pixels, Render, Rgba, ScrollHandle, SharedString, Stateful, Style,
    StyleRefinement, Styled, TextAlign, TextRun, WeakEntity, Window, div, fill, img, point, px,
    retain_all, rgb, rgba, size,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::drag_drop::{
    FileTransferMode, ItemDragPayload, PathListDropTargetKind, PathListDropTargetUpdate,
    refresh_active_drag_cursor_for_drop_menu, refresh_active_drag_cursor_for_transfer_mode,
    refresh_active_drag_cursor_not_allowed,
};
use super::icons::{FileIconSnapshot, cached_icon_or_fallback};
use super::item_view::{
    ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT, ItemViewScrollbarAxis, item_view_scrollbar_container,
};
use super::places::PlaceDrag;
use super::rename::RENAME_TEXT_INSET_X;
use super::rubber_band::RubberBandDrag;
use details::{DetailsColumn, DetailsColumnKind, details_columns};

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
        items: Vec<DetailsItemSnapshot>,
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
    fallback_bg: u32,
}

struct StaticItemTextShapes {
    marker_line: gpui::ShapedLine,
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
struct StaticItemTextShapeCacheStats {
    hits: usize,
    misses: usize,
    evicted: usize,
    entries: usize,
}

impl StaticItemTextShapeCacheStats {
    fn has_activity(self) -> bool {
        self.hits > 0 || self.misses > 0 || self.evicted > 0
    }
}

#[derive(Default)]
pub(crate) struct StaticItemTextShapeCache {
    entries: HashMap<StaticItemTextShapeCacheKey, Arc<StaticItemTextShapes>>,
    stats: StaticItemTextShapeCacheStats,
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

    fn take_stats(&mut self) -> StaticItemTextShapeCacheStats {
        let mut stats = std::mem::take(&mut self.stats);
        stats.entries = self.entries.len();
        stats
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ItemPaintSlotStats {
    pub(crate) inserted: usize,
    pub(crate) content_changed: usize,
    pub(crate) geometry_changed: usize,
    pub(crate) visual_changed: usize,
    pub(crate) unchanged: usize,
    pub(crate) removed: usize,
    pub(crate) entries: usize,
}

impl ItemPaintSlotStats {
    pub(crate) fn has_activity(self) -> bool {
        self.inserted > 0
            || self.content_changed > 0
            || self.geometry_changed > 0
            || self.visual_changed > 0
            || self.unchanged > 0
            || self.removed > 0
    }
}

#[derive(Default)]
pub(crate) struct ItemPaintSlotCache {
    slots: HashMap<u64, ItemPaintSlot>,
    visible_epoch: u64,
}

pub(crate) struct ItemPaintSlotProjection {
    pub(crate) stats: ItemPaintSlotStats,
    pub(crate) snapshot: FileGridRenderSnapshot,
}

impl ItemPaintSlotCache {
    pub(crate) fn project_file_grid_snapshot(
        &mut self,
        snapshot: FileGridSnapshot,
        hovered_item: Option<ItemId>,
    ) -> ItemPaintSlotProjection {
        match snapshot {
            FileGridSnapshot::Compact { layout, items } => {
                let (stats, items) = self.project_visible_items(items, hovered_item);
                ItemPaintSlotProjection {
                    stats,
                    snapshot: FileGridRenderSnapshot::Compact { layout, items },
                }
            }
            FileGridSnapshot::Icons { layout, items } => {
                let (stats, items) = self.project_visible_items(items, hovered_item);
                ItemPaintSlotProjection {
                    stats,
                    snapshot: FileGridRenderSnapshot::Icons { layout, items },
                }
            }
            FileGridSnapshot::Details {
                items,
                row_count,
                metrics,
                name_column_width,
            } => {
                let (stats, _) = self.project_visible_items(Vec::new(), None);
                ItemPaintSlotProjection {
                    stats,
                    snapshot: FileGridRenderSnapshot::Details {
                        items,
                        row_count,
                        metrics,
                        name_column_width,
                    },
                }
            }
        }
    }

    fn project_visible_items(
        &mut self,
        items: Vec<VisibleItemSnapshot>,
        hovered_item: Option<ItemId>,
    ) -> (ItemPaintSlotStats, Vec<ItemPaintSnapshot>) {
        self.visible_epoch = self.visible_epoch.wrapping_add(1).max(1);
        let mut stats = ItemPaintSlotStats::default();
        let mut snapshots = Vec::with_capacity(items.len());
        for item in items {
            let slot_id = item.slot_id;
            let item_id = item.item_id;
            let geometry = ItemPaintGeometry::from_layout(item.layout);
            let next_content = ItemPaintContent::from_item(&item);
            let visual = ItemPaintVisualState::from_item(&item, hovered_item);
            match self.slots.get_mut(&item.slot_id) {
                Some(slot) => {
                    if slot.content.as_ref() != &next_content {
                        stats.content_changed += 1;
                        slot.content = Arc::new(next_content);
                    } else if slot.geometry != geometry {
                        stats.geometry_changed += 1;
                    } else if slot.visual != visual {
                        stats.visual_changed += 1;
                    } else {
                        stats.unchanged += 1;
                    }
                    slot.item_id = item_id;
                    slot.geometry = geometry;
                    slot.visual = visual;
                    slot.visible_epoch = self.visible_epoch;
                    snapshots.push(slot.snapshot(slot_id, item.layout));
                }
                None => {
                    stats.inserted += 1;
                    let slot = ItemPaintSlot {
                        item_id,
                        geometry,
                        content: Arc::new(next_content),
                        visual,
                        visible_epoch: self.visible_epoch,
                    };
                    snapshots.push(slot.snapshot(slot_id, item.layout));
                    self.slots.insert(slot_id, slot);
                }
            }
        }

        let visible_epoch = self.visible_epoch;
        let before_retain = self.slots.len();
        self.slots
            .retain(|_, slot| slot.visible_epoch == visible_epoch);
        stats.removed = before_retain.saturating_sub(self.slots.len());
        stats.entries = self.slots.len();
        (stats, snapshots)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ItemPaintSlot {
    item_id: ItemId,
    geometry: ItemPaintGeometry,
    content: Arc<ItemPaintContent>,
    visual: ItemPaintVisualState,
    visible_epoch: u64,
}

impl ItemPaintSlot {
    fn snapshot(&self, slot_id: u64, layout: ItemLayout) -> ItemPaintSnapshot {
        ItemPaintSnapshot {
            slot_id,
            item_id: self.item_id,
            layout,
            content: self.content.clone(),
            visual: self.visual,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ItemPaintGeometry {
    item_x: u32,
    item_y: u32,
    item_width: u32,
    item_height: u32,
    visual_x: u32,
    visual_y: u32,
    visual_width: u32,
    visual_height: u32,
    icon_x: u32,
    icon_y: u32,
    icon_width: u32,
    icon_height: u32,
    text_x: u32,
    text_y: u32,
    text_width: u32,
    text_height: u32,
}

impl ItemPaintGeometry {
    fn from_layout(layout: ItemLayout) -> Self {
        let item = layout.item_rect;
        let visual = layout.visual_rect;
        let icon = layout.icon_rect;
        let text = layout.text_rect;
        Self {
            item_x: item.x.to_bits(),
            item_y: item.y.to_bits(),
            item_width: item.width.to_bits(),
            item_height: item.height.to_bits(),
            visual_x: visual.x.to_bits(),
            visual_y: visual.y.to_bits(),
            visual_width: visual.width.to_bits(),
            visual_height: visual.height.to_bits(),
            icon_x: icon.x.to_bits(),
            icon_y: icon.y.to_bits(),
            icon_width: icon.width.to_bits(),
            icon_height: icon.height.to_bits(),
            text_x: text.x.to_bits(),
            text_y: text.y.to_bits(),
            text_width: text.width.to_bits(),
            text_height: text.height.to_bits(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ItemPaintSnapshot {
    slot_id: u64,
    item_id: ItemId,
    layout: ItemLayout,
    content: Arc<ItemPaintContent>,
    visual: ItemPaintVisualState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ItemPaintContent {
    item_id: ItemId,
    name: Arc<str>,
    display_name: SharedString,
    thumbnail_path: Option<Arc<Path>>,
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
    icon_name_lines: Arc<[SharedString]>,
    drag_path: Arc<Path>,
    draft_name: Option<String>,
    draft_caret: Option<usize>,
    draft_selection: Option<(usize, usize)>,
    draft_error: Option<String>,
    draft_warning: Option<String>,
}

impl ItemPaintContent {
    fn from_item(item: &VisibleItemSnapshot) -> Self {
        Self {
            item_id: item.item_id,
            name: item.name.clone(),
            display_name: item.display_name.clone(),
            thumbnail_path: item.thumbnail_path.clone(),
            icon: item.icon.clone(),
            fallback_marker: item.fallback_marker.clone(),
            icon_name_lines: item.icon_name_lines.clone(),
            drag_path: item.drag_path.clone(),
            draft_name: item.draft_name.clone(),
            draft_caret: item.draft_caret,
            draft_selection: item.draft_selection,
            draft_error: item.draft_error.clone(),
            draft_warning: item.draft_warning.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ItemPaintVisualState {
    selected: bool,
    selection_count: usize,
    hovered: bool,
    drop_target: bool,
}

impl ItemPaintVisualState {
    fn from_item(item: &VisibleItemSnapshot, hovered_item: Option<ItemId>) -> Self {
        Self {
            selected: item.selected,
            selection_count: item.selection_count,
            hovered: hovered_item == Some(item.item_id),
            drop_target: item.drop_target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneViewportGeometry {
    pub(crate) window_rect: ViewRect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemDrag {
    pane_id: PaneId,
    path: Arc<Path>,
    name: Arc<str>,
    icon: FileIconSnapshot,
    selected: bool,
    selection_count: usize,
}

impl ItemDrag {
    pub(crate) fn payload(&self) -> ItemDragPayload {
        ItemDragPayload {
            source_pane: self.pane_id,
            source_path: self.path.as_ref().to_path_buf(),
            source_selected: self.selected,
        }
    }
}

struct DragPreview {
    icon: FileIconSnapshot,
    label: String,
    count: usize,
    content_origin_x: f32,
    content_origin_y: f32,
}

const DRAG_PREVIEW_MIN_WIDTH: f32 = 220.0;
const DRAG_PREVIEW_MIN_HEIGHT: f32 = 36.0;
const DRAG_PREVIEW_CURSOR_GAP: f32 = 8.0;

fn item_identity_element_id(prefix: &'static str, item_id: ItemId) -> (&'static str, u64) {
    (prefix, item_id.0)
}

fn item_image_element_id(slot_id: u64) -> (&'static str, u64) {
    ("item-image", slot_id)
}

fn static_item_visual_element_id(item_id: ItemId) -> (&'static str, u64) {
    ("static-item-visual", item_id.0)
}

fn handle_file_grid_item_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<ItemDrag>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let payload = event.drag(cx).payload();
    let source_paths = app.item_drag_source_paths(&payload);
    handle_file_grid_path_list_drag_move(
        app,
        pane_id,
        contains,
        event.event.position,
        &source_paths,
        window,
        cx,
    );
}

fn handle_file_grid_external_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<ExternalPaths>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let source_paths = app.external_drag_source_paths(event.drag(cx).paths());
    handle_file_grid_path_list_drag_move(
        app,
        pane_id,
        contains,
        event.event.position,
        &source_paths,
        window,
        cx,
    );
}

fn handle_file_grid_path_list_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    contains: bool,
    position: gpui::Point<gpui::Pixels>,
    source_paths: &[PathBuf],
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let update = if contains {
        app.update_dragged_paths_drop_target_from_window_position(pane_id, position, source_paths)
    } else {
        PathListDropTargetUpdate {
            changed: app.clear_item_drop_target_for_pane(pane_id),
            kind: None,
        }
    };
    if contains {
        if update.accepted() {
            refresh_active_drag_cursor_for_drop_menu(window, cx);
            app.refresh_drop_target_lease(cx);
        } else {
            refresh_active_drag_cursor_not_allowed(window, cx);
        }
    }
    if update.changed {
        cx.notify();
    }
    if contains {
        cx.stop_propagation();
    }
}

fn handle_file_grid_place_drag_move(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::DragMoveEvent<PlaceDrag>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let contains = event.bounds.contains(&event.event.position);
    let source_path = event.drag(cx).path();
    let source_paths = std::slice::from_ref(&source_path);
    let update = if contains {
        app.update_dragged_paths_drop_target_from_window_position(
            pane_id,
            event.event.position,
            source_paths,
        )
    } else {
        PathListDropTargetUpdate {
            changed: app.clear_item_drop_target_for_pane(pane_id),
            kind: None,
        }
    };
    if contains {
        match update.kind {
            Some(PathListDropTargetKind::Directory) => {
                refresh_active_drag_cursor_for_drop_menu(window, cx);
                app.refresh_drop_target_lease(cx);
            }
            Some(PathListDropTargetKind::Pane) => {
                refresh_active_drag_cursor_for_transfer_mode(FileTransferMode::Move, window, cx);
                app.refresh_drop_target_lease(cx);
            }
            None => {
                refresh_active_drag_cursor_not_allowed(window, cx);
            }
        }
    }
    if update.changed {
        cx.notify();
    }
    if contains {
        cx.stop_propagation();
    }
}

fn handle_file_grid_item_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    drag: &ItemDrag,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    let payload = drag.payload();
    app.drop_item_drag_to_position_in_pane(pane_id, payload, position, cx);
    cx.stop_propagation();
    cx.notify();
}

fn handle_file_grid_external_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    external_paths: &ExternalPaths,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    app.drop_external_paths_to_position_in_pane(
        pane_id,
        external_paths.paths().to_vec(),
        position,
        cx,
    );
    cx.stop_propagation();
    cx.notify();
}

fn handle_file_grid_place_drop(
    app: &mut FikaApp,
    pane_id: PaneId,
    drag: &PlaceDrag,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let position = window.mouse_position();
    app.drop_place_drag_to_position_in_pane(pane_id, drag.path(), position, cx);
    cx.stop_propagation();
    cx.notify();
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

    let (content_width, content_height, visible_count, viewport) = match snapshot {
        FileGridRenderSnapshot::Icons {
            layout: icons_layout,
            items,
        } => {
            let content_size = icons_layout.content_size();
            let visible_count = items.len();
            let viewport = file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(
                div()
                    .relative()
                    .w(px(content_size.width))
                    .h(px(content_size.height))
                    .children(items.into_iter().map(|item| {
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
                viewport,
            )
        }
        FileGridRenderSnapshot::Compact { layout, items } => {
            let content_size = layout.content_size();
            let visible_count = items.len();
            let viewport = file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(
                div()
                    .relative()
                    .w(px(content_size.width))
                    .h(px(content_size.height))
                    .children(items.into_iter().map(|item| {
                        item_tile(pane_id, item, ItemTileTextAlignment::Start, app.clone(), cx)
                    })),
            );
            (
                content_size.width,
                content_size.height,
                visible_count,
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
                    mode,
                    cx,
                ));
            (content_width, content_height, visible_count, viewport)
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
            let mut shape_cache_stats = StaticItemTextShapeCacheStats::default();
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

impl FikaApp {
    fn take_static_item_text_shape_cache_stats(
        &mut self,
        pane_id: PaneId,
    ) -> StaticItemTextShapeCacheStats {
        self.static_item_text_shape_caches
            .get_mut(&pane_id)
            .map(StaticItemTextShapeCache::take_stats)
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MeasuredViewport {
    rect: ViewRect,
    max_scroll_x: f32,
    max_scroll_y: f32,
}

fn scrollbar_axis_for_snapshot(snapshot: &FileGridRenderSnapshot) -> ItemViewScrollbarAxis {
    match snapshot {
        FileGridRenderSnapshot::Compact { .. } => ItemViewScrollbarAxis::Horizontal,
        FileGridRenderSnapshot::Icons { .. } | FileGridRenderSnapshot::Details { .. } => {
            ItemViewScrollbarAxis::Vertical
        }
    }
}

fn view_mode_for_snapshot(snapshot: &FileGridRenderSnapshot) -> ViewMode {
    match snapshot {
        FileGridRenderSnapshot::Compact { .. } => ViewMode::Compact,
        FileGridRenderSnapshot::Icons { .. } => ViewMode::Icons,
        FileGridRenderSnapshot::Details { .. } => ViewMode::Details,
    }
}

fn viewport_bounds_update_requires_notify(
    previous: Option<&ViewState>,
    next: Option<&ViewState>,
    projected_width: Option<f32>,
    measured_rect: ViewRect,
) -> bool {
    let (Some(previous), Some(next)) = (previous, next) else {
        return true;
    };
    if !viewport_value_eq(previous.scroll_x, next.scroll_x)
        || !viewport_value_eq(previous.scroll_y, next.scroll_y)
    {
        return true;
    }
    if !viewport_value_eq(previous.viewport_height, measured_rect.height) {
        return true;
    }
    if projected_width.is_some_and(|width| viewport_value_eq(width, measured_rect.width)) {
        return false;
    }
    !viewport_value_eq(previous.viewport_width, measured_rect.width)
}

fn viewport_value_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.5
}

fn measured_viewport_for_scrollbar_axis(
    bounds: gpui::Bounds<gpui::Pixels>,
    content_width: f32,
    content_height: f32,
    axis: ItemViewScrollbarAxis,
) -> MeasuredViewport {
    let wrapper_width = normalize_viewport_extent(bounds.size.width.as_f32());
    let wrapper_height = normalize_viewport_extent(bounds.size.height.as_f32());
    let (width, height) = match axis {
        ItemViewScrollbarAxis::Horizontal => (
            wrapper_width,
            normalize_viewport_extent(
                (wrapper_height - ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT).max(1.0),
            ),
        ),
        ItemViewScrollbarAxis::Vertical => (
            normalize_viewport_extent(
                (wrapper_width - ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT).max(1.0),
            ),
            wrapper_height,
        ),
    };
    let (max_scroll_x, max_scroll_y) = match axis {
        ItemViewScrollbarAxis::Horizontal => ((content_width - width).max(0.0), 0.0),
        ItemViewScrollbarAxis::Vertical => (0.0, (content_height - height).max(0.0)),
    };
    MeasuredViewport {
        rect: ViewRect {
            x: bounds.origin.x.as_f32(),
            y: bounds.origin.y.as_f32(),
            width,
            height,
        },
        max_scroll_x,
        max_scroll_y,
    }
}

fn file_grid_viewport_shell(
    pane_id: PaneId,
    _drop_target: bool,
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("items-viewport-{}", pane_id.0))
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .bg(rgba(0x00000000))
        .occlude()
        .overflow_hidden()
        .on_scroll_wheel(
            cx.listener(move |this, event: &gpui::ScrollWheelEvent, _window, cx| {
                handle_file_grid_wheel(this, pane_id, event, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                handle_pane_navigation_mouse_down(this, pane_id, NavigationDirection::Back);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                handle_pane_navigation_mouse_down(this, pane_id, NavigationDirection::Forward);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(hit) = this.item_at_window_position(pane_id, event.position) {
                    if handle_item_mouse_down(this, pane_id, hit.path, hit.is_dir, mode, event, cx)
                    {
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }

                let pressed = this.press_rubber_band_from_window_if_blank(pane_id, event.position);
                cx.stop_propagation();
                if pressed {
                    cx.notify();
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, _event: &gpui::MouseUpEvent, _window, cx| {
                this.finish_rubber_band(pane_id);
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |this, _event: &gpui::MouseUpEvent, _window, cx| {
                this.finish_rubber_band(pane_id);
                cx.notify();
            }),
        )
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() && this.handle_blank_click(pane_id, event.position()) {
                    cx.notify();
                }
                if event.standard_click() {
                    cx.stop_propagation();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                let shown = if let Some(hit) = this.item_at_window_position(pane_id, event.position)
                {
                    this.show_item_context_menu(pane_id, hit.path, hit.is_dir, event.position)
                } else {
                    this.show_blank_context_menu_if_blank(pane_id, event.position)
                };
                cx.stop_propagation();
                if shown {
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Middle,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if !matches!(mode, FileGridMode::Manager) {
                    return;
                }
                if let Some(hit) = this.item_at_window_position(pane_id, event.position) {
                    if hit.is_dir {
                        this.paste_primary_into_directory(pane_id, hit.path, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }
                } else if this.paste_primary_into_pane_if_blank(pane_id, event.position, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .on_drag(RubberBandDrag { pane_id }, |_, _, _, cx| cx.new(|_| Empty))
        .on_drag_move::<RubberBandDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<RubberBandDrag>, _window, cx| {
                if !this
                    .rubber_band
                    .as_ref()
                    .is_some_and(|band| band.pane_id == pane_id)
                {
                    if this.activate_pending_rubber_band_from_window(pane_id, event.event.position)
                    {
                        cx.stop_propagation();
                        cx.notify();
                    }
                    return;
                }
                if this.update_rubber_band_from_window(pane_id, event.event.position) {
                    cx.stop_propagation();
                    cx.notify();
                }
            },
        ))
        .on_drop::<RubberBandDrag>(
            cx.listener(move |this, _drag: &RubberBandDrag, _window, cx| {
                this.finish_rubber_band(pane_id);
                cx.notify();
            }),
        )
        .on_drag_move::<ItemDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                handle_file_grid_item_drag_move(this, pane_id, event, window, cx);
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                handle_file_grid_external_drag_move(this, pane_id, event, window, cx);
            },
        ))
        .on_drag_move::<PlaceDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                handle_file_grid_place_drag_move(this, pane_id, event, window, cx);
            },
        ))
        .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
            handle_file_grid_item_drop(this, pane_id, drag, window, cx);
        }))
        .on_drop::<ExternalPaths>(cx.listener(
            move |this, external_paths: &ExternalPaths, window, cx| {
                handle_file_grid_external_drop(this, pane_id, external_paths, window, cx);
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, window, cx| {
            handle_file_grid_place_drop(this, pane_id, drag, window, cx);
        }))
}

fn details_table(
    pane_id: PaneId,
    items: Vec<DetailsItemSnapshot>,
    row_count: usize,
    trash_view: bool,
    content_width: f32,
    content_height: f32,
    metrics: DetailsLayoutMetrics,
    name_column_width: f32,
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Div {
    let columns = details_columns(trash_view, name_column_width);
    div()
        .relative()
        .w(px(content_width))
        .h(px(content_height))
        .child(details_header(&columns, content_width, metrics))
        .children(
            items
                .into_iter()
                .map(|item| details_row(pane_id, item, &columns, content_width, metrics, mode, cx)),
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

fn details_row(
    pane_id: PaneId,
    item: DetailsItemSnapshot,
    columns: &[DetailsColumn],
    content_width: f32,
    metrics: DetailsLayoutMetrics,
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let top = metrics.header_height + item.row_index as f32 * metrics.row_height;
    let selected = item.selected;
    let drop_target = item.drop_target;
    let item_id = item.item_id;
    let path_for_mouse_down = item.path.clone();
    let path_for_menu = item.path.clone();
    let path_for_drag = Arc::<Path>::from(item.path.as_path());
    let target_dir_for_drop = item.path.clone();
    let is_dir_for_click = item.is_dir;
    let is_dir_for_menu = item.is_dir;
    let is_dir_for_drop = item.is_dir;

    let drag_value = ItemDrag {
        pane_id,
        path: path_for_drag,
        name: item.name.clone(),
        icon: item.icon.clone(),
        selected,
        selection_count: item.selection_count,
    };
    let app = cx.weak_entity();

    div()
        .id(item_identity_element_id("details-row", item_id))
        .absolute()
        .left_0()
        .top(px(top))
        .w(px(content_width))
        .h(px(metrics.row_height))
        .flex()
        .items_center()
        .bg(details_row_background(
            selected,
            drop_target,
            item.row_index,
        ))
        .block_mouse_except_scroll()
        .cursor_pointer()
        .hover(move |row| row.bg(item_tile_hover_background(selected, drop_target)))
        .on_scroll_wheel(
            cx.listener(move |this, event: &gpui::ScrollWheelEvent, _window, cx| {
                handle_file_grid_wheel(this, pane_id, event, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if handle_item_mouse_down(
                    this,
                    pane_id,
                    path_for_mouse_down.clone(),
                    is_dir_for_click,
                    mode,
                    event,
                    cx,
                ) {
                    cx.notify();
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_item_context_menu(
                    pane_id,
                    path_for_menu.clone(),
                    is_dir_for_menu,
                    event.position,
                );
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                handle_pane_navigation_mouse_down(this, pane_id, NavigationDirection::Back);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                handle_pane_navigation_mouse_down(this, pane_id, NavigationDirection::Forward);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_drag(drag_value, move |drag, cursor_offset, _, cx| {
            let _ = app.update(cx, |this, _cx| {
                this.begin_item_drag(drag.payload());
            });
            let (content_origin_x, content_origin_y) = drag_preview_content_origin(cursor_offset);
            cx.new(|_| DragPreview {
                icon: drag.icon.clone(),
                label: drag_preview_label(drag.name.as_ref(), drag.selected, drag.selection_count),
                count: drag.selection_count,
                content_origin_x,
                content_origin_y,
            })
        })
        .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
            handle_file_grid_item_drop(this, pane_id, drag, window, cx);
        }))
        .on_drop::<ExternalPaths>(cx.listener(
            move |this, external_paths: &ExternalPaths, window, cx| {
                handle_file_grid_external_drop(this, pane_id, external_paths, window, cx);
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, window, cx| {
            handle_file_grid_place_drop(this, pane_id, drag, window, cx);
        }))
        .when(is_dir_for_drop, directory_drag_over_styles)
        .when(is_dir_for_drop, |row| {
            let target_dir_for_primary_paste = target_dir_for_drop.clone();
            row.on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                    if matches!(mode, FileGridMode::Manager) {
                        this.paste_primary_into_directory(
                            pane_id,
                            target_dir_for_primary_paste.clone(),
                            cx,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
        })
        .children(
            columns
                .iter()
                .map(|column| details_cell(&item, *column, selected, metrics)),
        )
}

fn details_row_background(selected: bool, drop_target: bool, row_index: usize) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected {
        rgb(0xdbeafe)
    } else if row_index % 2 == 0 {
        rgb(0xffffff)
    } else {
        rgb(0xf8fafc)
    }
}

fn details_cell(
    item: &DetailsItemSnapshot,
    column: DetailsColumn,
    selected: bool,
    metrics: DetailsLayoutMetrics,
) -> gpui::AnyElement {
    match column.kind {
        DetailsColumnKind::Name => details_name_cell(item, column.width, selected, metrics),
        DetailsColumnKind::Size => details_text_cell(column.width, item.size_label.clone()),
        DetailsColumnKind::Modified => details_text_cell(column.width, item.modified_label.clone()),
        DetailsColumnKind::OriginalPath => {
            details_text_cell(column.width, item.original_path_label.clone())
        }
        DetailsColumnKind::DeletionTime => {
            details_text_cell(column.width, item.deletion_time_label.clone())
        }
    }
}

fn details_name_cell(
    item: &DetailsItemSnapshot,
    width: f32,
    selected: bool,
    metrics: DetailsLayoutMetrics,
) -> gpui::AnyElement {
    let icon = item.icon.clone();
    div()
        .w(px(width))
        .h_full()
        .min_w_0()
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(metrics.icon_size))
                .h(px(metrics.icon_size))
                .rounded_sm()
                .overflow_hidden()
                .child({
                    let fallback_marker = SharedString::from(icon.fallback_marker.as_ref());
                    icon_image_or_fallback(icon, fallback_marker)
                }),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_sm()
                .text_color(if selected {
                    rgb(0x0f172a)
                } else {
                    rgb(0x1f2937)
                })
                .whitespace_nowrap()
                .child(item.name.to_string()),
        )
        .into_any_element()
}

fn details_text_cell(width: f32, text: String) -> gpui::AnyElement {
    div()
        .w(px(width))
        .h_full()
        .min_w_0()
        .px_2()
        .flex()
        .items_center()
        .text_sm()
        .text_color(rgb(0x4b5563))
        .truncate()
        .child(text)
        .into_any_element()
}

fn handle_pane_navigation_mouse_down(
    app: &mut FikaApp,
    pane_id: PaneId,
    direction: NavigationDirection,
) {
    app.panes.focus(pane_id);
    match direction {
        NavigationDirection::Back => app.go_back(pane_id),
        NavigationDirection::Forward => app.go_forward(pane_id),
    }
}

pub(crate) fn handle_file_grid_wheel(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::ScrollWheelEvent,
    cx: &mut Context<FikaApp>,
) {
    if wheel_modifiers_request_zoom(event.modifiers) {
        app.finish_rubber_band(pane_id);
        app.zoom_pane_from_wheel(pane_id, event.delta);
        cx.stop_propagation();
        cx.notify();
    } else if app.scroll_pane_from_wheel(pane_id, event.delta) {
        cx.stop_propagation();
        cx.notify();
    }
}

fn wheel_modifiers_request_zoom(modifiers: gpui::Modifiers) -> bool {
    modifiers.control || modifiers.secondary()
}

fn handle_item_mouse_down(
    app: &mut FikaApp,
    pane_id: PaneId,
    path: PathBuf,
    is_dir: bool,
    mode: FileGridMode,
    event: &gpui::MouseDownEvent,
    cx: &mut Context<FikaApp>,
) -> bool {
    app.dismiss_context_menu();
    app.panes.focus(pane_id);

    let extend = event.modifiers.shift;
    let toggle = event.modifiers.secondary();
    let double_click = event.click_count >= 2;
    let is_dir = app.item_path_is_directory(pane_id, &path, is_dir);
    match mode {
        FileGridMode::Manager => {
            if double_click && is_dir {
                app.open_directory_from_item(pane_id, path, true);
            } else if double_click {
                app.open_default_application_for_item(pane_id, path, cx);
            } else if !double_click {
                if extend {
                    app.select_range_to(pane_id, path);
                } else if toggle {
                    app.toggle_selection(pane_id, path);
                } else {
                    app.select_only(pane_id, path);
                }
            }
        }
        FileGridMode::Chooser {
            directories,
            multiple,
        } => {
            if double_click && is_dir {
                app.open_directory_from_item(pane_id, path, true);
            } else if directories {
                if !is_dir {
                    return true;
                }
                if !double_click {
                    if extend {
                        app.select_range_to(pane_id, path);
                    } else if toggle || multiple {
                        app.toggle_selection(pane_id, path);
                    } else {
                        app.select_only(pane_id, path);
                    }
                }
            } else if is_dir {
                if !double_click {
                    app.select_only(pane_id, path);
                }
            } else if double_click && !multiple {
                app.choose_path(path);
            } else if !double_click {
                if extend {
                    app.select_range_to(pane_id, path);
                } else if toggle || multiple {
                    app.toggle_selection(pane_id, path);
                } else {
                    app.select_only(pane_id, path);
                }
            }
        }
    }
    true
}

#[cfg(test)]
fn item_mouse_down_opens_directory(is_dir: bool, _mode: FileGridMode, click_count: usize) -> bool {
    is_dir && click_count >= 2
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
    let selection_count = item.visual.selection_count;
    let hovered = item.visual.hovered;
    let drop_target = item.visual.drop_target;
    let use_static_visual_paint = content.draft_name.is_none()
        && content.thumbnail_path.is_none()
        && content.icon.path.is_none();
    let drag_app = app.clone();
    let drag_value = ItemDrag {
        pane_id,
        path: content.drag_path.clone(),
        name: content.name.clone(),
        icon: content.icon.clone(),
        selected,
        selection_count,
    };
    let shell_background = if use_static_visual_paint {
        rgba(0x00000000)
    } else {
        item_tile_background(selected, drop_target, hovered)
    };

    // Temporary migration boundary: this Div remains the interaction shell while
    // static fallback item visuals move toward retained custom painting.
    let core = div()
        .id(item_identity_element_id("item-core", item_id))
        .absolute()
        .left(px(visual.x - item_rect.x))
        .top(px(visual.y - item_rect.y))
        .w(px(visual.width))
        .h(px(visual.height))
        .rounded_md()
        .bg(shell_background)
        .cursor_pointer()
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
        .on_drag(drag_value, move |drag, cursor_offset, _, cx| {
            let _ = drag_app.update(cx, |this, _cx| {
                this.begin_item_drag(drag.payload());
            });
            let (content_origin_x, content_origin_y) = drag_preview_content_origin(cursor_offset);
            cx.new(|_| DragPreview {
                icon: drag.icon.clone(),
                label: drag_preview_label(drag.name.as_ref(), drag.selected, drag.selection_count),
                count: drag.selection_count,
                content_origin_x,
                content_origin_y,
            })
        });
    let core = if use_static_visual_paint {
        core.child(static_item_visual_view(
            pane_id,
            item_id,
            content.display_name.clone(),
            content.icon_name_lines.clone(),
            content.icon.clone(),
            content.fallback_marker.clone(),
            item.layout,
            text_alignment,
            selected,
            hovered,
            drop_target,
            app,
        ))
    } else {
        let text = if let Some(draft_name) = content.draft_name.as_deref() {
            rename_text_view(
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
            )
            .into_any_element()
        } else {
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

fn item_tile_hover_background(selected: bool, drop_target: bool) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected {
        rgb(0xcfe3ff)
    } else {
        rgb(0xeaf1ff)
    }
}

fn drop_target_item_background() -> Rgba {
    rgba(0xf59e0b4a)
}

fn directory_drag_over_styles(item: Stateful<Div>) -> Stateful<Div> {
    item.drag_over::<ItemDrag>(|style, _, _, _| style.bg(drop_target_item_background()))
        .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(drop_target_item_background()))
        .drag_over::<PlaceDrag>(|style, _, _, _| style.bg(drop_target_item_background()))
}

fn static_item_visual_view(
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
    app: WeakEntity<FikaApp>,
) -> impl IntoElement {
    StaticItemVisualElement {
        pane_id,
        item_id,
        display_name,
        icon_name_lines,
        icon,
        fallback_marker,
        layout,
        text_alignment,
        selected,
        hovered,
        drop_target,
        app,
        style: StyleRefinement::default(),
    }
    .absolute()
    .left_0()
    .top_0()
    .size_full()
}

struct StaticItemVisualElement {
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
    app: WeakEntity<FikaApp>,
    style: StyleRefinement,
}

impl IntoElement for StaticItemVisualElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for StaticItemVisualElement {
    type RequestLayoutState = Style;
    type PrepaintState = StaticItemVisualPaintState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(static_item_visual_element_id(self.item_id)))
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
        static_item_visual_prepaint(
            self.pane_id,
            self.item_id,
            self.display_name.clone(),
            self.icon_name_lines.clone(),
            self.icon.clone(),
            self.fallback_marker.clone(),
            self.layout,
            self.text_alignment,
            self.selected,
            self.hovered,
            self.drop_target,
            self.app.clone(),
            window,
            cx,
        )
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
        request_layout.paint(bounds, window, cx, |window, cx| {
            static_item_visual_paint(bounds, prepaint, window, cx);
        });
    }
}

impl Styled for StaticItemVisualElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
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
        fallback_fg: icon.fallback_fg,
        fallback_marker,
        label,
    }
}

fn shape_static_item_text(
    key: &StaticItemTextShapeCacheKey,
    style: &StaticItemTextShapeStyle,
    window: &mut Window,
) -> StaticItemTextShapes {
    let fallback_marker = static_paint_single_line_text(key.fallback_marker.clone());
    let marker_run = TextRun {
        len: fallback_marker.len(),
        font: style.marker_font.clone(),
        color: rgb(style.fallback_fg).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let marker_line = window.text_system().shape_line(
        fallback_marker,
        style.marker_font_size,
        &[marker_run],
        None,
    );
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
    window.paint_quad(fill(icon_bounds, rgb(state.fallback_bg)).corner_radii(px(6.0)));
    let marker_origin = point(
        icon_bounds.origin.x,
        icon_bounds.origin.y
            + ((icon_bounds.size.height - state.marker_line_height).max(px(0.0)) / 2.0),
    );
    state
        .shapes
        .marker_line
        .paint(
            marker_origin,
            state.marker_line_height,
            TextAlign::Center,
            Some(icon_bounds.size.width),
            window,
            cx,
        )
        .ok();

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

fn icon_image_or_fallback(
    icon: FileIconSnapshot,
    fallback_marker: SharedString,
) -> gpui::AnyElement {
    let fallback_fg = icon.fallback_fg;
    let fallback_bg = icon.fallback_bg;
    cached_icon_or_fallback(&icon, move || {
        fallback_icon_element(fallback_marker.clone(), fallback_fg, fallback_bg)
    })
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

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let left = self.content_origin_x;
        let top = self.content_origin_y;
        let icon = self.icon.clone();
        let show_count = self.count > 1;
        let count = self.count;
        div()
            .relative()
            .w(px(left.max(0.0) + DRAG_PREVIEW_MIN_WIDTH))
            .h(px(top.max(0.0) + DRAG_PREVIEW_MIN_HEIGHT + 6.0))
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .h(px(DRAG_PREVIEW_MIN_HEIGHT))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x94a3b8))
                    .bg(rgb(0xffffff))
                    .shadow_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(0x1f2937))
                    .child(
                        div()
                            .relative()
                            .w(px(26.0))
                            .h(px(26.0))
                            .rounded_sm()
                            .overflow_hidden()
                            .child({
                                let fallback_marker =
                                    SharedString::from(icon.fallback_marker.as_ref());
                                icon_image_or_fallback(icon, fallback_marker)
                            })
                            .when(show_count, |icon| {
                                icon.child(
                                    div()
                                        .absolute()
                                        .right(px(-1.0))
                                        .bottom(px(-1.0))
                                        .min_w(px(14.0))
                                        .h(px(14.0))
                                        .px(px(3.0))
                                        .rounded_full()
                                        .bg(rgb(0xd97706))
                                        .text_xs()
                                        .text_color(rgb(0xffffff))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(count.to_string()),
                                )
                            }),
                    )
                    .child(div().max_w(px(170.0)).truncate().child(self.label.clone())),
            )
    }
}

fn drag_preview_content_origin(offset: gpui::Point<gpui::Pixels>) -> (f32, f32) {
    (
        offset.x.as_f32() + DRAG_PREVIEW_CURSOR_GAP,
        offset.y.as_f32() + DRAG_PREVIEW_CURSOR_GAP,
    )
}

fn drag_preview_label(name: &str, selected: bool, selection_count: usize) -> String {
    if selected && selection_count > 1 {
        format!("{selection_count} items")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FileGridMode, FileGridRenderSnapshot, FileGridSnapshot, ItemPaintContent,
        ItemPaintSlotCache, ItemTileTextAlignment, VisibleItemSnapshot, display_text_layout,
        drag_preview_content_origin, drag_preview_label, item_identity_element_id,
        item_image_element_id, item_mouse_down_opens_directory,
        measured_viewport_for_scrollbar_axis, normalized_text_range, rename_text_layout,
        static_item_visual_element_id, viewport_bounds_update_requires_notify,
    };
    use crate::ui::icons::FileIconSnapshot;
    use crate::ui::item_view::ItemViewScrollbarAxis;
    use fika_core::{
        CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions, ItemId, ItemLayout,
        ViewRect, ViewState,
    };
    use gpui::{Bounds, SharedString, point, px, size};
    use std::path::Path;
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
            drag_preview_content_origin(point(px(48.0), px(12.0))),
            (56.0, 20.0)
        );
        assert_eq!(
            drag_preview_content_origin(point(px(-4.0), px(-2.0))),
            (4.0, 6.0)
        );
        assert_eq!(
            drag_preview_content_origin(point(px(-12.0), px(-10.0))),
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
    fn static_item_visual_id_is_keyed_by_item_identity() {
        assert_eq!(
            static_item_visual_element_id(ItemId(7)),
            ("static-item-visual", 7)
        );
        assert_ne!(
            static_item_visual_element_id(ItemId(7)),
            static_item_visual_element_id(ItemId(8))
        );
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

    fn icons_snapshot(items: Vec<VisibleItemSnapshot>) -> FileGridSnapshot {
        FileGridSnapshot::Icons {
            layout: IconsLayout::new(items.len(), IconsLayoutOptions::default()),
            items,
        }
    }

    fn first_icon_paint_content(snapshot: &FileGridRenderSnapshot) -> Arc<ItemPaintContent> {
        let FileGridRenderSnapshot::Icons { items, .. } = snapshot else {
            panic!("expected icons render snapshot");
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
