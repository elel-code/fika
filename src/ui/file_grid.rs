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
    RawFileGridSnapshotInput, VisibleItemSnapshot, deferred_thumbnail_candidates_for_model,
    raw_file_grid_snapshot,
};

use crate::FikaApp;
use fika_core::{
    CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions, ItemId, ItemLayout,
    PaneId, ViewRect, ViewState, normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    Context, Div, Empty, ExternalPaths, MouseButton, NavigationDirection, ParentElement, Render,
    Rgba, ScrollHandle, Stateful, Styled, Window, div, img, px, rgb, rgba,
};
use std::path::PathBuf;
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
    pub(crate) snapshot: FileGridSnapshot,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemTileTextAlignment {
    Start,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneViewportGeometry {
    pub(crate) window_rect: ViewRect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemDrag {
    pane_id: PaneId,
    path: PathBuf,
    name: Arc<str>,
    icon: FileIconSnapshot,
    selected: bool,
    selection_count: usize,
}

impl ItemDrag {
    pub(crate) fn payload(&self) -> ItemDragPayload {
        ItemDragPayload {
            source_pane: self.pane_id,
            source_path: self.path.clone(),
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

fn item_interaction_id(prefix: &str, pane_id: PaneId, item_id: ItemId) -> String {
    format!("{prefix}-{}-{}", pane_id.0, item_id.0)
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

    let (content_width, content_height, viewport) = match snapshot {
        FileGridSnapshot::Icons {
            layout: icons_layout,
            items,
        } => {
            let content_size = icons_layout.content_size();
            let viewport = file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(
                div()
                    .relative()
                    .w(px(content_size.width))
                    .h(px(content_size.height))
                    .children(items.into_iter().map(|item| {
                        item_tile(pane_id, item, mode, ItemTileTextAlignment::Center, cx)
                    })),
            );
            (content_size.width, content_size.height, viewport)
        }
        FileGridSnapshot::Compact { layout, items } => {
            let content_size = layout.content_size();
            let viewport = file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(
                div()
                    .relative()
                    .w(px(content_size.width))
                    .h(px(content_size.height))
                    .children(items.into_iter().map(|item| {
                        item_tile(pane_id, item, mode, ItemTileTextAlignment::Start, cx)
                    })),
            );
            (content_size.width, content_size.height, viewport)
        }
        FileGridSnapshot::Details {
            items,
            row_count,
            metrics,
            name_column_width,
        } => {
            let content_width = details_content_width(trash_view, name_column_width).max(1.0);
            let content_height = details_content_height(row_count, metrics).max(1.0);
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
            (content_width, content_height, viewport)
        }
    };

    div()
        .on_children_prepainted(move |bounds, _window, cx| {
            let Some(bounds) = bounds.first() else {
                return;
            };
            let measured = measured_viewport_for_scrollbar_axis(
                *bounds,
                content_width,
                content_height,
                scrollbar_axis,
            );
            let _ = app.update(cx, |this, cx| {
                let geometry_changed = this.set_pane_viewport_geometry(pane_id, measured.rect);
                let bounds_changed = this.set_pane_viewport_bounds(
                    pane_id,
                    measured.rect.width,
                    measured.rect.height,
                    measured.max_scroll_x,
                    measured.max_scroll_y,
                );
                if geometry_changed || bounds_changed {
                    cx.notify();
                }
            });
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
        ))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MeasuredViewport {
    rect: ViewRect,
    max_scroll_x: f32,
    max_scroll_y: f32,
}

fn scrollbar_axis_for_snapshot(snapshot: &FileGridSnapshot) -> ItemViewScrollbarAxis {
    match snapshot {
        FileGridSnapshot::Compact { .. } => ItemViewScrollbarAxis::Horizontal,
        FileGridSnapshot::Icons { .. } | FileGridSnapshot::Details { .. } => {
            ItemViewScrollbarAxis::Vertical
        }
    }
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
                let _shown = this.show_blank_context_menu_if_blank(pane_id, event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Middle,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                if matches!(mode, FileGridMode::Manager)
                    && this.paste_primary_into_pane_if_blank(pane_id, event.position, cx)
                {
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
    let path_for_drag = item.path.clone();
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
        .id(item_interaction_id("details-row", pane_id, item_id))
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
                .child(icon_image_or_fallback(icon)),
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
    item: VisibleItemSnapshot,
    mode: FileGridMode,
    text_alignment: ItemTileTextAlignment,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let id = format!("item-slot-{}-{}", pane_id.0, item.slot_id);
    let visual_id = item_interaction_id("item-core", pane_id, item.item_id);
    let renaming = item.draft_name.is_some();
    let display_name = item
        .draft_name
        .clone()
        .unwrap_or_else(|| item.name.to_string());
    let item_rect = item.layout.item_rect;
    let visual = item.layout.visual_rect;
    let path_for_mouse_down = item.path.clone();
    let path_for_menu = item.path.clone();
    let path_for_drag = item.path.clone();
    let target_dir_for_drop = item.path.clone();
    let is_dir_for_click = item.is_dir;
    let is_dir_for_menu = item.is_dir;
    let is_dir_for_drop = item.is_dir;
    let selected = item.selected;
    let drop_target = item.drop_target;
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
        .id(id)
        .absolute()
        .left(px(item_rect.x))
        .top(px(item_rect.y))
        .w(px(item_rect.width))
        .h(px(item_rect.height))
        .child(
            div()
                .id(visual_id)
                .absolute()
                .left(px(visual.x - item_rect.x))
                .top(px(visual.y - item_rect.y))
                .w(px(visual.width))
                .h(px(visual.height))
                .rounded_md()
                .bg(item_tile_background(selected, drop_target))
                .block_mouse_except_scroll()
                .cursor_pointer()
                .hover(move |tile| tile.bg(item_tile_hover_background(selected, drop_target)))
                .on_scroll_wheel(cx.listener(
                    move |this, event: &gpui::ScrollWheelEvent, _window, cx| {
                        handle_file_grid_wheel(this, pane_id, event, cx);
                    },
                ))
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
                        handle_pane_navigation_mouse_down(
                            this,
                            pane_id,
                            NavigationDirection::Forward,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .on_drag(drag_value, move |drag, cursor_offset, _, cx| {
                    let _ = app.update(cx, |this, _cx| {
                        this.begin_item_drag(drag.payload());
                    });
                    let (content_origin_x, content_origin_y) =
                        drag_preview_content_origin(cursor_offset);
                    cx.new(|_| DragPreview {
                        icon: drag.icon.clone(),
                        label: drag_preview_label(
                            drag.name.as_ref(),
                            drag.selected,
                            drag.selection_count,
                        ),
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
                .when(is_dir_for_drop, |tile| {
                    let target_dir_for_primary_paste = target_dir_for_drop.clone();
                    tile.on_mouse_down(
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
                .child(icon_view(&item, item.layout))
                .child(text_view(
                    pane_id,
                    &display_name,
                    item.layout,
                    text_alignment,
                    renaming,
                    selected,
                    item.draft_caret,
                    item.draft_selection,
                    item.draft_error.as_deref(),
                    item.draft_warning.as_deref(),
                    cx,
                )),
        )
}

fn item_tile_background(selected: bool, drop_target: bool) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected {
        rgb(0xdbeafe)
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

fn icon_view(item: &VisibleItemSnapshot, layout: ItemLayout) -> Div {
    let visual = layout.visual_rect;
    let icon = layout.icon_rect;
    let icon_left = (icon.x - visual.x).round();
    let icon_top = (icon.y - visual.y).round();
    let icon_width = icon.width.round().max(1.0);
    let icon_height = icon.height.round().max(1.0);
    let thumbnail_path = item.thumbnail_path.clone();
    let icon_snapshot = item.icon.clone();
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
                .child(img(path).size_full()),
        ),
        None => icon_container.child(icon_image_or_fallback(icon_snapshot)),
    }
}

fn icon_image_or_fallback(icon: FileIconSnapshot) -> gpui::AnyElement {
    let fallback = icon.fallback_marker.clone();
    let fallback_fg = icon.fallback_fg;
    let fallback_bg = icon.fallback_bg;
    cached_icon_or_fallback(&icon, move || {
        fallback_icon_element(fallback.clone(), fallback_fg, fallback_bg)
    })
}

fn fallback_icon_element(marker: Arc<str>, fg: u32, bg: u32) -> gpui::AnyElement {
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
        .child(marker.as_ref().to_string())
        .into_any_element()
}

fn text_view(
    pane_id: PaneId,
    display_name: &str,
    layout: ItemLayout,
    text_alignment: ItemTileTextAlignment,
    renaming: bool,
    selected: bool,
    rename_caret: Option<usize>,
    rename_selection: Option<(usize, usize)>,
    rename_error: Option<&str>,
    rename_warning: Option<&str>,
    cx: &mut Context<FikaApp>,
) -> Div {
    let visual = layout.visual_rect;
    let text = layout.text_rect;
    let show_helper = rename_error.is_some() || rename_warning.is_some();
    let rename_layout = if !renaming {
        display_text_layout(display_name, text.width, text.height, text_alignment)
    } else {
        rename_text_layout(text.height, show_helper)
    };
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
        .when(!renaming, |view| view.overflow_hidden())
        .when(
            (!renaming && matches!(text_alignment, ItemTileTextAlignment::Start))
                || (renaming && !show_helper),
            |view| view.justify_center(),
        )
        .child(if renaming {
            rename_editor_view(
                pane_id,
                display_name,
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
            )
            .into_any_element()
        } else {
            match text_alignment {
                ItemTileTextAlignment::Start => {
                    rename_name_view(display_name, false, selected, None, None)
                        .h(px(rename_layout.name_height))
                        .into_any_element()
                }
                ItemTileTextAlignment::Center => {
                    item_name_label_view(display_name, selected, rename_layout.name_height)
                        .into_any_element()
                }
            }
        })
        .child(item_helper_label_view(
            helper_text,
            helper_color,
            rename_layout.helper_height,
            text_alignment,
        ))
}

fn item_name_label_view(display_name: &str, selected: bool, height: f32) -> Div {
    let text_color = if selected {
        rgb(0x0f172a)
    } else {
        rgb(0x24292f)
    };
    let max_lines = (height / ITEM_NAME_LINE_HEIGHT).round().max(1.0) as usize;
    let display_name = layout::dolphin_preprocess_wrap(display_name);
    div()
        .h(px(height))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w_full()
                .max_w_full()
                .min_w_0()
                .text_sm()
                .line_height(px(ITEM_NAME_LINE_HEIGHT))
                .text_center()
                .whitespace_normal()
                .line_clamp(max_lines)
                .text_color(text_color)
                .child(display_name),
        )
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
            true,
            selected,
            rename_caret,
            rename_selection,
        ))
}

fn rename_name_view(
    display_name: &str,
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
        return base.whitespace_normal().child(display_name.to_string());
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
            .w(px(left + DRAG_PREVIEW_MIN_WIDTH))
            .h(px(top + DRAG_PREVIEW_MIN_HEIGHT + 6.0))
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
                            .child(icon_image_or_fallback(icon))
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

fn drag_preview_content_origin(_offset: gpui::Point<gpui::Pixels>) -> (f32, f32) {
    (0.0, 0.0)
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
        FileGridMode, ItemTileTextAlignment, display_text_layout, drag_preview_content_origin,
        drag_preview_label, item_interaction_id, item_mouse_down_opens_directory,
        measured_viewport_for_scrollbar_axis, normalized_text_range, rename_text_layout,
    };
    use crate::ui::item_view::ItemViewScrollbarAxis;
    use fika_core::{CompactLayout, CompactLayoutOptions, ItemId, PaneId};
    use gpui::{Bounds, point, px, size};

    #[test]
    fn drag_preview_uses_selection_count_only_for_selected_items() {
        assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
        assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
        assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
    }

    #[test]
    fn drag_preview_does_not_apply_gpui_cursor_offset_twice() {
        assert_eq!(
            drag_preview_content_origin(point(px(48.0), px(12.0))),
            (0.0, 0.0)
        );
        assert_eq!(
            drag_preview_content_origin(point(px(-4.0), px(-2.0))),
            (0.0, 0.0)
        );
    }

    #[test]
    fn item_interaction_id_is_keyed_by_item_identity_not_virtual_slot() {
        assert_eq!(
            item_interaction_id("item-core", PaneId(2), ItemId(7)),
            "item-core-2-7"
        );
        assert_ne!(
            item_interaction_id("item-core", PaneId(2), ItemId(7)),
            item_interaction_id("item-core", PaneId(2), ItemId(8))
        );
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
    let text_height = ITEM_NAME_LINE_HEIGHT;
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
