mod details;
mod layout;
mod projection;
mod slots;
mod snapshot;

pub(crate) use details::{
    DETAILS_ICON_SIZE, DetailsItemSnapshot, details_content_height, details_content_width,
};
pub(crate) use layout::{CompactColumnWidthCache, compact_text_width, compact_text_width_for_name};
pub(crate) use projection::{
    ContentItemHit, PaneLayoutProjection, PaneLayoutProjectionInput, content_item_hit_at_point,
    model_indexes_intersecting_visual_rect, pane_layout_projection,
};
pub(crate) use slots::VisibleItemSlotPool;
pub(crate) use snapshot::{
    RawFileGridSnapshotInput, VisibleItemSnapshot, deferred_icon_candidates_for_model,
    deferred_thumbnail_candidates_for_model, raw_file_grid_snapshot,
};

use crate::FikaApp;
use fika_core::{
    CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions, ItemLayout, PaneId,
    ViewRect, ViewState, normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    Context, Div, Empty, ExternalPaths, MouseButton, NavigationDirection, ParentElement, Render,
    Rgba, ScrollHandle, Stateful, Styled, Window, div, img, px, rgb, rgba,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::drag_drop::{
    FileTransferMode, ItemDragPayload, file_transfer_mode_for_modifiers,
    refresh_active_drag_cursor_for_transfer_mode, refresh_active_drag_cursor_not_allowed,
};
use super::icons::{FileIconSnapshot, cached_icon_or_fallback};
use super::item_view::{
    ITEM_VIEW_SCROLLBAR_RESERVED_EXTENT, ItemViewScrollbarAxis, item_view_scrollbar_container,
};
use super::places::PlaceDrag;
use super::rename::RENAME_TEXT_INSET_X;
use super::rubber_band::RubberBandDrag;
use details::{
    DETAILS_HEADER_HEIGHT, DETAILS_ROW_HEIGHT, DetailsColumn, DetailsColumnKind, details_columns,
};

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
    pub(crate) drop_target: Option<FileTransferMode>,
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
    },
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
    label: String,
    cursor_offset_x: f32,
    cursor_offset_y: f32,
}

const DRAG_PREVIEW_CURSOR_GAP: f32 = 10.0;
const DRAG_PREVIEW_MIN_WIDTH: f32 = 220.0;
const DRAG_PREVIEW_MIN_HEIGHT: f32 = 36.0;

fn drag_move_hits_item_path<T>(
    app: &mut FikaApp,
    pane_id: PaneId,
    path: &Path,
    event: &gpui::DragMoveEvent<T>,
) -> bool {
    event.bounds.contains(&event.event.position)
        && app.window_position_hits_item_path_in_pane(pane_id, event.event.position, path)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RenameTextLayout {
    name_height: f32,
    helper_height: f32,
}

const RENAME_NAME_HEIGHT: f32 = 20.0;

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
                    .children(
                        items
                            .into_iter()
                            .map(|item| item_tile(pane_id, item, mode, cx)),
                    ),
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
                    .children(
                        items
                            .into_iter()
                            .map(|item| item_tile(pane_id, item, mode, cx)),
                    ),
            );
            (content_size.width, content_size.height, viewport)
        }
        FileGridSnapshot::Details { items, row_count } => {
            let content_width = details_content_width(trash_view).max(1.0);
            let content_height = details_content_height(row_count).max(1.0);
            let viewport =
                file_grid_viewport_shell(pane_id, drop_target, mode, cx).child(details_table(
                    pane_id,
                    items,
                    row_count,
                    trash_view,
                    content_width,
                    content_height,
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
    drop_target: Option<FileTransferMode>,
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("items-viewport-{}", pane_id.0))
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .bg(drop_target.map_or(rgba(0x00000000), drop_target_viewport_background))
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
                let contains = event.bounds.contains(&event.event.position)
                    && this.window_position_is_blank_in_pane(pane_id, event.event.position);
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                let changed = if contains {
                    this.set_item_drag_drop_target_for_pane(pane_id, mode)
                } else {
                    this.clear_item_drop_target_for_pane(pane_id)
                };
                if contains {
                    refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                    this.schedule_drop_target_stale_clear(cx);
                }
                if changed {
                    cx.notify();
                }
                if contains {
                    cx.stop_propagation();
                }
            },
        ))
        .on_drag_move::<ExternalPaths>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                let contains = event.bounds.contains(&event.event.position)
                    && this.window_position_is_blank_in_pane(pane_id, event.event.position);
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                let changed = if contains {
                    this.set_item_drag_drop_target_for_pane(pane_id, mode)
                } else {
                    this.clear_item_drop_target_for_pane(pane_id)
                };
                if contains {
                    refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                    this.schedule_drop_target_stale_clear(cx);
                }
                if changed {
                    cx.notify();
                }
                if contains {
                    cx.stop_propagation();
                }
            },
        ))
        .on_drag_move::<PlaceDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                let contains = event.bounds.contains(&event.event.position)
                    && this.window_position_is_blank_in_pane(pane_id, event.event.position);
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                let changed = if contains {
                    this.set_item_drag_drop_target_for_pane(pane_id, mode)
                } else {
                    this.clear_item_drop_target_for_pane(pane_id)
                };
                if contains {
                    refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                    this.schedule_drop_target_stale_clear(cx);
                }
                if changed {
                    cx.notify();
                }
                if contains {
                    cx.stop_propagation();
                }
            },
        ))
        .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
            let mode = file_transfer_mode_for_modifiers(window.modifiers());
            this.drop_item_drag_to_pane(pane_id, drag.payload(), mode, cx);
            cx.stop_propagation();
            cx.notify();
        }))
        .on_drop::<ExternalPaths>(cx.listener(
            move |this, external_paths: &ExternalPaths, window, cx| {
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                this.drop_external_paths_to_pane(
                    pane_id,
                    external_paths.paths().to_vec(),
                    mode,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
            this.drop_place_drag_to_pane(pane_id, drag.path());
            cx.stop_propagation();
            cx.notify();
        }))
}

fn details_table(
    pane_id: PaneId,
    items: Vec<DetailsItemSnapshot>,
    row_count: usize,
    trash_view: bool,
    content_width: f32,
    content_height: f32,
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Div {
    let columns = details_columns(trash_view);
    div()
        .relative()
        .w(px(content_width))
        .h(px(content_height))
        .child(details_header(&columns, content_width))
        .children(
            items
                .into_iter()
                .map(|item| details_row(pane_id, item, &columns, content_width, mode, cx)),
        )
        .when(row_count == 0, |table| {
            table.child(
                div()
                    .absolute()
                    .top(px(DETAILS_HEADER_HEIGHT))
                    .left_0()
                    .w(px(content_width))
                    .h(px(DETAILS_ROW_HEIGHT))
                    .px_2()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(rgb(0x6b7280))
                    .child("No items"),
            )
        })
}

fn details_header(columns: &[DetailsColumn], content_width: f32) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(content_width))
        .h(px(DETAILS_HEADER_HEIGHT))
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
    mode: FileGridMode,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let top = DETAILS_HEADER_HEIGHT + item.row_index as f32 * DETAILS_ROW_HEIGHT;
    let selected = item.selected;
    let drop_target = item.drop_target;
    let path_for_mouse_down = item.path.clone();
    let path_for_menu = item.path.clone();
    let path_for_drag = item.path.clone();
    let target_dir_for_drop = item.path.clone();
    let path_for_place_drag_hit = item.path.clone();
    let path_for_directory_item_drag_hit = item.path.clone();
    let path_for_directory_external_drag_hit = item.path.clone();
    let path_for_file_item_drag_hit = item.path.clone();
    let path_for_file_external_drag_hit = item.path.clone();
    let is_dir_for_click = item.is_dir;
    let is_dir_for_menu = item.is_dir;
    let is_dir_for_drop = item.is_dir;

    let drag_value = ItemDrag {
        pane_id,
        path: path_for_drag,
        name: item.name.clone(),
        selected,
        selection_count: item.selection_count,
    };
    let app = cx.weak_entity();

    div()
        .id(format!("details-row-{}-{}", pane_id.0, item.row_index))
        .absolute()
        .left_0()
        .top(px(top))
        .w(px(content_width))
        .h(px(DETAILS_ROW_HEIGHT))
        .flex()
        .items_center()
        .bg(details_row_background(
            selected,
            drop_target,
            item.row_index,
        ))
        .when(drop_target.is_some(), |row| row.shadow_md())
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
                ) {
                    cx.stop_propagation();
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
            let (cursor_offset_x, cursor_offset_y) = drag_preview_cursor_offset(cursor_offset);
            cx.new(|_| DragPreview {
                label: drag_preview_label(drag.name.as_ref(), drag.selected, drag.selection_count),
                cursor_offset_x,
                cursor_offset_y,
            })
        })
        .on_drag_move::<PlaceDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                let contains =
                    drag_move_hits_item_path(this, pane_id, &path_for_place_drag_hit, event);
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                let changed = contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                if contains {
                    refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                    this.schedule_drop_target_stale_clear(cx);
                }
                if changed {
                    cx.notify();
                }
                if contains {
                    cx.stop_propagation();
                }
            },
        ))
        .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
            this.drop_place_drag_to_pane(pane_id, drag.path());
            cx.stop_propagation();
            cx.notify();
        }))
        .when(is_dir_for_drop, |row| {
            let target_dir_for_move = target_dir_for_drop.clone();
            let target_dir_for_external_move = target_dir_for_drop.clone();
            let target_dir_for_external_drop = target_dir_for_drop.clone();
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
            .on_drag_move::<ItemDrag>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                    let contains = drag_move_hits_item_path(
                        this,
                        pane_id,
                        &path_for_directory_item_drag_hit,
                        event,
                    );
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    let valid_target =
                        contains && this.item_drag_can_drop_to_directory(&target_dir_for_move);
                    let changed = if valid_target {
                        this.set_item_drag_drop_target_for_directory(
                            pane_id,
                            target_dir_for_move.clone(),
                            mode,
                        )
                    } else if contains {
                        this.clear_drag_drop_targets()
                    } else {
                        this.clear_item_drop_target_for_directory(pane_id, &target_dir_for_move)
                    };
                    if contains {
                        if valid_target {
                            refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                            this.schedule_drop_target_stale_clear(cx);
                        } else {
                            refresh_active_drag_cursor_not_allowed(window, cx);
                        }
                    }
                    if changed {
                        cx.notify();
                    }
                    if contains {
                        cx.stop_propagation();
                    }
                },
            ))
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                    let contains = drag_move_hits_item_path(
                        this,
                        pane_id,
                        &path_for_directory_external_drag_hit,
                        event,
                    );
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    let changed = if contains {
                        this.set_item_drag_drop_target_for_directory(
                            pane_id,
                            target_dir_for_external_move.clone(),
                            mode,
                        )
                    } else {
                        this.clear_item_drop_target_for_directory(
                            pane_id,
                            &target_dir_for_external_move,
                        )
                    };
                    if contains {
                        refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                        this.schedule_drop_target_stale_clear(cx);
                    }
                    if changed {
                        cx.notify();
                    }
                    if contains {
                        cx.stop_propagation();
                    }
                },
            ))
            .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                this.drop_item_drag_to_directory(
                    pane_id,
                    drag.payload(),
                    target_dir_for_drop.clone(),
                    mode,
                    cx,
                );
                cx.stop_propagation();
                cx.notify();
            }))
            .on_drop::<ExternalPaths>(cx.listener(
                move |this, external_paths: &ExternalPaths, window, cx| {
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    this.drop_external_paths_to_directory(
                        pane_id,
                        external_paths.paths().to_vec(),
                        target_dir_for_external_drop.clone(),
                        mode,
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
        })
        .when(!is_dir_for_drop, |row| {
            row.on_drag_move::<ItemDrag>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                    let contains = drag_move_hits_item_path(
                        this,
                        pane_id,
                        &path_for_file_item_drag_hit,
                        event,
                    );
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    let changed =
                        contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                    if contains {
                        refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                        this.schedule_drop_target_stale_clear(cx);
                    }
                    if changed {
                        cx.notify();
                    }
                    if contains {
                        cx.stop_propagation();
                    }
                },
            ))
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                    let contains = drag_move_hits_item_path(
                        this,
                        pane_id,
                        &path_for_file_external_drag_hit,
                        event,
                    );
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    let changed =
                        contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                    if contains {
                        refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                        this.schedule_drop_target_stale_clear(cx);
                    }
                    if changed {
                        cx.notify();
                    }
                    if contains {
                        cx.stop_propagation();
                    }
                },
            ))
            .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                let mode = file_transfer_mode_for_modifiers(window.modifiers());
                this.drop_item_drag_to_pane(pane_id, drag.payload(), mode, cx);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_drop::<ExternalPaths>(cx.listener(
                move |this, external_paths: &ExternalPaths, window, cx| {
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    this.drop_external_paths_to_pane(
                        pane_id,
                        external_paths.paths().to_vec(),
                        mode,
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                },
            ))
        })
        .children(
            columns
                .iter()
                .map(|column| details_cell(&item, *column, selected)),
        )
}

fn details_row_background(
    selected: bool,
    drop_target: Option<FileTransferMode>,
    row_index: usize,
) -> Rgba {
    if let Some(mode) = drop_target {
        drop_target_item_background(mode)
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
) -> gpui::AnyElement {
    match column.kind {
        DetailsColumnKind::Name => details_name_cell(item, column.width, selected),
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

fn details_name_cell(item: &DetailsItemSnapshot, width: f32, selected: bool) -> gpui::AnyElement {
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
                .w(px(DETAILS_ICON_SIZE))
                .h(px(DETAILS_ICON_SIZE))
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
                .truncate()
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
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let id = format!("item-slot-{}-{}", pane_id.0, item.slot_id);
    let visual_id = format!("item-core-{}-{}", pane_id.0, item.slot_id);
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
    let path_for_place_drag_hit = item.path.clone();
    let path_for_directory_item_drag_hit = item.path.clone();
    let path_for_directory_external_drag_hit = item.path.clone();
    let path_for_file_item_drag_hit = item.path.clone();
    let path_for_file_external_drag_hit = item.path.clone();
    let is_dir_for_click = item.is_dir;
    let is_dir_for_menu = item.is_dir;
    let is_dir_for_drop = item.is_dir;
    let selected = item.selected;
    let drop_target = item.drop_target;
    let drag_value = ItemDrag {
        pane_id,
        path: path_for_drag,
        name: item.name.clone(),
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
                .when(drop_target.is_some(), |tile| tile.shadow_md())
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
                        ) {
                            cx.stop_propagation();
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
                    let (cursor_offset_x, cursor_offset_y) =
                        drag_preview_cursor_offset(cursor_offset);
                    cx.new(|_| DragPreview {
                        label: drag_preview_label(
                            drag.name.as_ref(),
                            drag.selected,
                            drag.selection_count,
                        ),
                        cursor_offset_x,
                        cursor_offset_y,
                    })
                })
                .on_drag_move::<PlaceDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                        let contains = drag_move_hits_item_path(
                            this,
                            pane_id,
                            &path_for_place_drag_hit,
                            event,
                        );
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        let changed =
                            contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                        if contains {
                            refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                            this.schedule_drop_target_stale_clear(cx);
                        }
                        if changed {
                            cx.notify();
                        }
                        if contains {
                            cx.stop_propagation();
                        }
                    },
                ))
                .on_drop::<PlaceDrag>(cx.listener(move |this, drag: &PlaceDrag, _window, cx| {
                    this.drop_place_drag_to_pane(pane_id, drag.path());
                    cx.stop_propagation();
                    cx.notify();
                }))
                .when(is_dir_for_drop, |tile| {
                    let target_dir_for_move = target_dir_for_drop.clone();
                    let target_dir_for_external_move = target_dir_for_drop.clone();
                    let target_dir_for_external_drop = target_dir_for_drop.clone();
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
                    .on_drag_move::<ItemDrag>(cx.listener(
                        move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                            let contains = drag_move_hits_item_path(
                                this,
                                pane_id,
                                &path_for_directory_item_drag_hit,
                                event,
                            );
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            let valid_target = contains
                                && this.item_drag_can_drop_to_directory(&target_dir_for_move);
                            let changed = if valid_target {
                                this.set_item_drag_drop_target_for_directory(
                                    pane_id,
                                    target_dir_for_move.clone(),
                                    mode,
                                )
                            } else if contains {
                                this.clear_drag_drop_targets()
                            } else {
                                this.clear_item_drop_target_for_directory(
                                    pane_id,
                                    &target_dir_for_move,
                                )
                            };
                            if contains {
                                if valid_target {
                                    refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                                    this.schedule_drop_target_stale_clear(cx);
                                } else {
                                    refresh_active_drag_cursor_not_allowed(window, cx);
                                }
                            }
                            if changed {
                                cx.notify();
                            }
                            if contains {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .on_drag_move::<ExternalPaths>(cx.listener(
                        move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                            let contains = drag_move_hits_item_path(
                                this,
                                pane_id,
                                &path_for_directory_external_drag_hit,
                                event,
                            );
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            let changed = if contains {
                                this.set_item_drag_drop_target_for_directory(
                                    pane_id,
                                    target_dir_for_external_move.clone(),
                                    mode,
                                )
                            } else {
                                this.clear_item_drop_target_for_directory(
                                    pane_id,
                                    &target_dir_for_external_move,
                                )
                            };
                            if contains {
                                refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                                this.schedule_drop_target_stale_clear(cx);
                            }
                            if changed {
                                cx.notify();
                            }
                            if contains {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        this.drop_item_drag_to_directory(
                            pane_id,
                            drag.payload(),
                            target_dir_for_drop.clone(),
                            mode,
                            cx,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_drop::<ExternalPaths>(cx.listener(
                        move |this, external_paths: &ExternalPaths, window, cx| {
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            this.drop_external_paths_to_directory(
                                pane_id,
                                external_paths.paths().to_vec(),
                                target_dir_for_external_drop.clone(),
                                mode,
                                cx,
                            );
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ))
                })
                .when(!is_dir_for_drop, |tile| {
                    tile.on_drag_move::<ItemDrag>(cx.listener(
                        move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                            let contains = drag_move_hits_item_path(
                                this,
                                pane_id,
                                &path_for_file_item_drag_hit,
                                event,
                            );
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            let changed =
                                contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                            if contains {
                                refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                                this.schedule_drop_target_stale_clear(cx);
                            }
                            if changed {
                                cx.notify();
                            }
                            if contains {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .on_drag_move::<ExternalPaths>(cx.listener(
                        move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                            let contains = drag_move_hits_item_path(
                                this,
                                pane_id,
                                &path_for_file_external_drag_hit,
                                event,
                            );
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            let changed =
                                contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                            if contains {
                                refresh_active_drag_cursor_for_transfer_mode(mode, window, cx);
                                this.schedule_drop_target_stale_clear(cx);
                            }
                            if changed {
                                cx.notify();
                            }
                            if contains {
                                cx.stop_propagation();
                            }
                        },
                    ))
                    .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        this.drop_item_drag_to_pane(pane_id, drag.payload(), mode, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }))
                    .on_drop::<ExternalPaths>(cx.listener(
                        move |this, external_paths: &ExternalPaths, window, cx| {
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            this.drop_external_paths_to_pane(
                                pane_id,
                                external_paths.paths().to_vec(),
                                mode,
                                cx,
                            );
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ))
                })
                .child(icon_view(&item, item.layout))
                .child(text_view(
                    pane_id,
                    &display_name,
                    &item.detail_label,
                    item.layout,
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

fn item_tile_background(selected: bool, drop_target: Option<FileTransferMode>) -> Rgba {
    if let Some(mode) = drop_target {
        drop_target_item_background(mode)
    } else if selected {
        rgb(0xdbeafe)
    } else {
        rgba(0x00000000)
    }
}

fn item_tile_hover_background(selected: bool, drop_target: Option<FileTransferMode>) -> Rgba {
    if let Some(mode) = drop_target {
        drop_target_item_background(mode)
    } else if selected {
        rgb(0xcfe3ff)
    } else {
        rgb(0xeaf1ff)
    }
}

fn drop_target_viewport_background(mode: FileTransferMode) -> Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a24),
        FileTransferMode::Move => rgba(0xd9770624),
        FileTransferMode::Link => rgba(0x7c3aed24),
    }
}

fn drop_target_item_background(mode: FileTransferMode) -> Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a4a),
        FileTransferMode::Move => rgba(0xd977064a),
        FileTransferMode::Link => rgba(0x7c3aed4a),
    }
}

fn icon_view(item: &VisibleItemSnapshot, layout: ItemLayout) -> Div {
    let visual = layout.visual_rect;
    let icon = layout.icon_rect;
    let thumbnail_path = item.thumbnail_path.clone();
    let icon_snapshot = item.icon.clone();
    let icon_container = div()
        .absolute()
        .left(px(icon.x - visual.x))
        .top(px(icon.y - visual.y))
        .w(px(icon.width))
        .h(px(icon.height))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden();

    match thumbnail_path {
        Some(path) => icon_container.child(img(path).size_full()),
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
    detail_label: &str,
    layout: ItemLayout,
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
    let rename_layout = rename_text_layout(text.height);
    let helper_text = rename_error.or(rename_warning).unwrap_or(detail_label);
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
        .overflow_hidden()
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
            .into_any_element()
        } else {
            rename_name_view(display_name, false, selected, None, None)
                .h(px(rename_layout.name_height))
                .into_any_element()
        })
        .child(
            div()
                .h(px(rename_layout.helper_height))
                .min_h_0()
                .text_xs()
                .text_color(helper_color)
                .truncate()
                .child(helper_text.to_string()),
        )
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
        .truncate()
        .text_color(text_color)
        .when(renaming, |name| name.cursor_text());
    if !renaming {
        return base.child(display_name.to_string());
    }

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

fn rename_text_layout(text_height: f32) -> RenameTextLayout {
    let text_height = text_height.max(0.0);
    let name_height = text_height.min(RENAME_NAME_HEIGHT);
    RenameTextLayout {
        name_height,
        helper_height: (text_height - name_height).max(0.0),
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
        let left = self.cursor_offset_x + DRAG_PREVIEW_CURSOR_GAP;
        let top = self.cursor_offset_y + DRAG_PREVIEW_CURSOR_GAP;
        div()
            .relative()
            .w(px(left + DRAG_PREVIEW_MIN_WIDTH))
            .h(px(top + DRAG_PREVIEW_MIN_HEIGHT))
            .child(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0x94a3b8))
                    .bg(rgb(0xffffff))
                    .text_sm()
                    .text_color(rgb(0x1f2937))
                    .child(self.label.clone()),
            )
    }
}

fn drag_preview_cursor_offset(offset: gpui::Point<gpui::Pixels>) -> (f32, f32) {
    (offset.x.as_f32().max(0.0), offset.y.as_f32().max(0.0))
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
        FileGridMode, drag_preview_cursor_offset, drag_preview_label,
        item_mouse_down_opens_directory, measured_viewport_for_scrollbar_axis,
        normalized_text_range, rename_text_layout,
    };
    use crate::ui::item_view::ItemViewScrollbarAxis;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn drag_preview_uses_selection_count_only_for_selected_items() {
        assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
        assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
        assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
    }

    #[test]
    fn drag_preview_keeps_label_anchored_to_cursor_offset() {
        assert_eq!(
            drag_preview_cursor_offset(point(px(48.0), px(12.0))),
            (48.0, 12.0)
        );
        assert_eq!(
            drag_preview_cursor_offset(point(px(-4.0), px(-2.0))),
            (0.0, 0.0)
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
        let layout = rename_text_layout(40.0);

        assert_eq!(layout.name_height, 20.0);
        assert_eq!(layout.helper_height, 20.0);

        let compact = rename_text_layout(12.0);
        assert_eq!(compact.name_height, 12.0);
        assert_eq!(compact.helper_height, 0.0);
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
    let padding = 8.0;
    let gap = 8.0;
    let text_height = 40.0;
    CompactLayoutOptions {
        viewport_width: view.viewport_width.max(1.0),
        viewport_height: view.viewport_height.max(1.0),
        reserved_bottom,
        scroll_x: view.scroll_x,
        scroll_y: view.scroll_y,
        padding,
        gap,
        item_width: icon_size + 120.0,
        item_height: (icon_size + 32.0).max(text_height + padding * 2.0),
        icon_size,
        text_height,
        ..CompactLayoutOptions::default()
    }
}

pub(crate) fn icons_layout_options(view: &ViewState, reserved_bottom: f32) -> IconsLayoutOptions {
    let icon_size = view.icon_size();
    let padding = 8.0;
    let gap = 8.0;
    let text_height = 40.0;
    let item_width = (icon_size * 2.25).max(128.0);
    IconsLayoutOptions {
        viewport_width: view.viewport_width.max(1.0),
        viewport_height: view.viewport_height.max(1.0),
        reserved_bottom,
        scroll_x: view.scroll_x,
        scroll_y: view.scroll_y,
        padding,
        gap,
        item_width,
        item_height: padding * 3.0 + icon_size + gap + text_height,
        icon_size,
        text_height,
        ..IconsLayoutOptions::default()
    }
}
