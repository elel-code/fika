mod layout;
mod projection;
mod slots;
mod snapshot;

pub(crate) use layout::{
    CompactColumnWidthCache, CompactTextWidthOverride,
    compact_layout_for_filtered_model_with_text_override,
    compact_layout_for_model_with_text_override, compact_text_width, compact_text_width_for_name,
    model_index_for_layout_index,
};
pub(crate) use projection::{ContentItemHit, PaneLayoutProjection};
pub(crate) use slots::VisibleItemSlotPool;
pub(crate) use snapshot::{
    VisibleItemSnapshot, format_entry_kind_label, visible_item_thumbnail_path,
};

use crate::FikaApp;
use fika_core::{
    CompactLayout, CompactLayoutOptions, ItemLayout, PaneId, ViewRect, ViewState,
    normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    Context, Div, Empty, ExternalPaths, MouseButton, NavigationDirection, ParentElement, Render,
    Rgba, ScrollHandle, Stateful, Styled, StyledImage, Window, div, img, px, rgb, rgba,
};
use std::path::PathBuf;
use std::sync::Arc;

use super::drag_drop::{
    FileTransferMode, ItemDragPayload, file_transfer_mode_for_modifiers,
    refresh_active_drag_cursor_for_transfer_mode, refresh_active_drag_cursor_not_allowed,
};
use super::item_view::item_view_scrollbar_container;
use super::places::PlaceDrag;
use super::rename::RENAME_TEXT_INSET_X;
use super::rubber_band::RubberBandDrag;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileGridMode {
    Manager,
    Chooser { directories: bool, multiple: bool },
}

pub(crate) struct FileGridProps {
    pub(crate) pane_id: PaneId,
    pub(crate) layout: CompactLayout,
    pub(crate) visible_items: Vec<VisibleItemSnapshot>,
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) view: ViewState,
    pub(crate) rubber_band: Option<ViewRect>,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) mode: FileGridMode,
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
        layout,
        visible_items,
        scroll_handle,
        view: _view,
        rubber_band,
        drop_target,
        mode,
    } = props;
    let content_size = layout.content_size();
    let app = cx.weak_entity();

    let viewport = div()
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
                let started = this.start_rubber_band_from_window_if_blank(pane_id, event.position);
                cx.stop_propagation();
                if started {
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
                    return;
                }
                if this.update_rubber_band_from_window(pane_id, event.event.position) {
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
        .child(
            div()
                .relative()
                .w(px(content_size.width))
                .h(px(content_size.height))
                .children(
                    visible_items
                        .into_iter()
                        .map(|item| item_tile(pane_id, item, mode, cx)),
                ),
        );

    div()
        .on_children_prepainted(move |bounds, _window, cx| {
            let Some(bounds) = bounds.first() else {
                return;
            };
            let width = normalize_viewport_extent(bounds.size.width.as_f32());
            let height = normalize_viewport_extent(bounds.size.height.as_f32());
            let window_rect = ViewRect {
                x: bounds.origin.x.as_f32(),
                y: bounds.origin.y.as_f32(),
                width,
                height,
            };
            let max_scroll_x = (content_size.width - width).max(0.0);
            let max_scroll_y = (content_size.height - height).max(0.0);
            let _ = app.update(cx, |this, cx| {
                let geometry_changed = this.set_pane_viewport_geometry(pane_id, window_rect);
                let bounds_changed = this.set_pane_viewport_bounds(
                    pane_id,
                    width,
                    height,
                    max_scroll_x,
                    max_scroll_y,
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
            rubber_band,
            viewport,
            window,
            cx,
        ))
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
    match mode {
        FileGridMode::Manager => {
            if double_click && is_dir {
                app.load_pane(pane_id, path);
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
                app.load_pane(pane_id, path);
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
                .on_drag(drag_value, move |drag, _, _, cx| {
                    let _ = app.update(cx, |this, _cx| {
                        this.begin_item_drag(drag.payload());
                    });
                    cx.new(|_| DragPreview {
                        label: drag_preview_label(
                            drag.name.as_ref(),
                            drag.selected,
                            drag.selection_count,
                        ),
                    })
                })
                .on_drag_move::<PlaceDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<PlaceDrag>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position);
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
                            let contains = event.bounds.contains(&event.event.position);
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
                            let contains = event.bounds.contains(&event.event.position);
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
                            let contains = event.bounds.contains(&event.event.position);
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
                            let contains = event.bounds.contains(&event.event.position);
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
                    &item.kind_label,
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
    let icon_path = item.icon.path.clone();
    let fallback = item.icon.fallback_marker.clone();
    let fallback_fg = item.icon.fallback_fg;
    let fallback_bg = item.icon.fallback_bg;
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
        Some(path) => icon_container.child(img(path).size_full().with_fallback(move || {
            icon_image_or_fallback(
                icon_path.clone(),
                fallback.clone(),
                fallback_fg,
                fallback_bg,
            )
        })),
        None => icon_container.child(icon_image_or_fallback(
            icon_path,
            fallback,
            fallback_fg,
            fallback_bg,
        )),
    }
}

fn icon_image_or_fallback(
    path: Option<PathBuf>,
    fallback: String,
    fallback_fg: u32,
    fallback_bg: u32,
) -> gpui::AnyElement {
    match path {
        Some(path) => img(path)
            .size_full()
            .with_fallback(move || {
                fallback_icon_element(fallback.clone(), fallback_fg, fallback_bg)
            })
            .into_any_element(),
        None => fallback_icon_element(fallback, fallback_fg, fallback_bg),
    }
}

fn fallback_icon_element(marker: String, fg: u32, bg: u32) -> gpui::AnyElement {
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

fn text_view(
    pane_id: PaneId,
    display_name: &str,
    kind_label: &str,
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
    let helper_text = rename_error.or(rename_warning).unwrap_or(kind_label);
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
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x94a3b8))
            .bg(rgb(0xffffff))
            .text_sm()
            .text_color(rgb(0x1f2937))
            .child(self.label.clone())
    }
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
        FileGridMode, drag_preview_label, item_mouse_down_opens_directory, normalized_text_range,
        rename_text_layout,
    };

    #[test]
    fn drag_preview_uses_selection_count_only_for_selected_items() {
        assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
        assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
        assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
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
