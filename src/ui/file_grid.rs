use crate::{
    FikaApp, ItemDragPayload, RubberBandDrag, VisibleItemSnapshot, file_transfer_mode_for_modifiers,
};
use fika_core::{
    CompactLayout, CompactLayoutOptions, HorizontalScrollBarLayout, ItemLayout, PaneId, ViewPoint,
    ViewRect, ViewState, horizontal_scroll_bar_layout, normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, Empty, MouseButton, ParentElement, Pixels, Render, Rgba, Stateful, Styled,
    StyledImage, Window, canvas, div, fill, img, point, px, rgb, rgba, size,
};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) const SCROLLBAR_THICKNESS: f32 = 12.0;
pub(crate) const SCROLLBAR_MIN_HANDLE_WIDTH: f32 = 36.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileGridMode {
    Manager,
    Chooser { directories: bool, multiple: bool },
}

pub(crate) struct FileGridProps {
    pub(crate) pane_id: PaneId,
    pub(crate) layout: CompactLayout,
    pub(crate) visible_items: Vec<VisibleItemSnapshot>,
    pub(crate) view: ViewState,
    pub(crate) rubber_band: Option<ViewRect>,
    pub(crate) drop_target: bool,
    pub(crate) mode: FileGridMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollBarDrag {
    pane_id: PaneId,
    content_width: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ItemDrag {
    pane_id: PaneId,
    path: PathBuf,
    name: Arc<str>,
    selected: bool,
    selection_count: usize,
}

impl ItemDrag {
    fn payload(&self) -> ItemDragPayload {
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

pub(crate) fn file_grid(props: FileGridProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let FileGridProps {
        pane_id,
        layout,
        visible_items,
        view,
        rubber_band,
        drop_target,
        mode,
    } = props;
    let content_size = layout.content_size();
    let visible_width = view.viewport_width;
    let visible_height = view.viewport_height;
    let max_scroll_x = (content_size.width - visible_width).max(0.0);
    let max_scroll_y = (content_size.height - visible_height).max(0.0);
    let scroll_bar_visible = layout
        .horizontal_scroll_bar(
            visible_width,
            SCROLLBAR_THICKNESS,
            SCROLLBAR_MIN_HANDLE_WIDTH,
        )
        .is_some();
    let app = cx.weak_entity();
    let drag_view = view.clone();

    div()
        .on_children_prepainted(move |bounds, _window, cx| {
            let Some(bounds) = bounds.first() else {
                return;
            };
            let width = normalize_viewport_extent(bounds.size.width.as_f32());
            let height = normalize_viewport_extent(bounds.size.height.as_f32());
            let origin = ViewPoint {
                x: bounds.origin.x.as_f32(),
                y: bounds.origin.y.as_f32(),
            };
            let max_scroll_x = (content_size.width - width).max(0.0);
            let max_scroll_y = (content_size.height - height).max(0.0);
            let _ = app.update(cx, |this, cx| {
                let origin_changed = this.set_viewport_origin(pane_id, origin);
                let bounds_changed = this.set_pane_viewport_bounds(
                    pane_id,
                    width,
                    height,
                    max_scroll_x,
                    max_scroll_y,
                );
                if origin_changed || bounds_changed {
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
        .child(
            div()
                .id(format!("items-viewport-{}", pane_id.0))
                .relative()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .bg(if drop_target {
                    rgba(0x16a34a18)
                } else {
                    rgba(0x00000000)
                })
                .overflow_hidden()
                .on_scroll_wheel(cx.listener(
                    move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                        let delta = event.delta.pixel_delta(window.line_height());
                        let horizontal_delta = -(delta.x.as_f32() + delta.y.as_f32());
                        this.scroll_pane_smooth(
                            pane_id,
                            horizontal_delta,
                            0.0,
                            max_scroll_x,
                            max_scroll_y,
                            cx,
                        );
                        cx.stop_propagation();
                    },
                ))
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.standard_click()
                            && this.handle_blank_click(pane_id, event.position())
                        {
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        if this.show_blank_context_menu_if_blank(pane_id, event.position) {
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                )
                .on_drag(RubberBandDrag { pane_id }, |_, _, _, cx| cx.new(|_| Empty))
                .on_drag_move::<RubberBandDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<RubberBandDrag>, _window, cx| {
                        let current = content_point_from_window(
                            event.event.position,
                            event.bounds,
                            &drag_view,
                        );
                        if this
                            .rubber_band
                            .as_ref()
                            .is_none_or(|band| band.pane_id != pane_id)
                            && !this.start_rubber_band_from_blank(pane_id, current)
                        {
                            return;
                        }
                        this.update_rubber_band(pane_id, current);
                        cx.notify();
                    },
                ))
                .on_drop::<RubberBandDrag>(cx.listener(
                    move |this, _drag: &RubberBandDrag, _window, cx| {
                        this.finish_rubber_band(pane_id);
                        cx.notify();
                    },
                ))
                .on_drag_move::<ItemDrag>(cx.listener(
                    move |this,
                          _event: &gpui::DragMoveEvent<ItemDrag>,
                          _window,
                          cx| {
                        if this.set_item_drag_drop_target_for_pane(pane_id) {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    this.drop_item_drag_to_pane(pane_id, drag.payload(), mode, cx);
                    cx.stop_propagation();
                    cx.notify();
                }))
                .child(
                    div()
                        .absolute()
                        .left(px(-view.scroll_x.round()))
                        .top(px(-view.scroll_y.round()))
                        .w(px(content_size.width))
                        .h(px(content_size.height))
                        .children(
                            visible_items
                                .into_iter()
                                .map(|item| item_tile(pane_id, item, mode, cx)),
                        ),
                )
                .when_some(rubber_band, |viewport, rect| {
                    viewport.child(rubber_band_overlay(rect))
                }),
        )
        .when(scroll_bar_visible, |grid| {
            grid.child(
                div()
                    .id(format!("scrollbar-x-reserve-{}", pane_id.0))
                    .h(px(SCROLLBAR_THICKNESS))
                    .w_full()
                    .max_w_full()
                    .min_w_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .child(horizontal_scroll_bar(
                        pane_id,
                        content_size.width,
                        view.scroll_x,
                        cx,
                    )),
            )
        })
}

fn horizontal_scroll_bar(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("scrollbar-x-{}", pane_id.0))
        .relative()
        .w_full()
        .max_w_full()
        .min_w_0()
        .flex_shrink_1()
        .overflow_hidden()
        .h(px(SCROLLBAR_THICKNESS))
        .bg(rgb(0xe6e9ef))
        .on_drag(
            ScrollBarDrag {
                pane_id,
                content_width,
            },
            |_, _, _, cx| cx.new(|_| Empty),
        )
        .on_drag_move::<ScrollBarDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ScrollBarDrag>, _window, cx| {
                let drag = *event.drag(cx);
                let track_x = (event.event.position.x - event.bounds.origin.x).as_f32();
                let track_width = normalize_viewport_extent(event.bounds.size.width.as_f32());
                let bar = horizontal_scroll_bar_layout(
                    drag.content_width,
                    0.0,
                    track_width,
                    SCROLLBAR_THICKNESS,
                    SCROLLBAR_MIN_HANDLE_WIDTH,
                );
                let Some(bar) = bar else {
                    return;
                };
                let scroll_x = bar.scroll_x_for_track_x(track_x);
                this.set_pane_scroll_immediate(drag.pane_id, scroll_x, 0.0, bar.max_scroll_x, 0.0);
                cx.stop_propagation();
                cx.notify();
            },
        ))
        .on_drop::<ScrollBarDrag>(cx.listener(move |this, drag: &ScrollBarDrag, _window, cx| {
            this.finish_scrollbar_drag_for_content_width(drag.pane_id, drag.content_width, cx);
            cx.stop_propagation();
        }))
        .child(scroll_bar_handle_canvas(content_width, scroll_x))
}

fn scroll_bar_handle_canvas(content_width: f32, scroll_x: f32) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| scroll_bar_layout_for_bounds(content_width, scroll_x, bounds),
        move |bounds, bar, window, _cx| {
            let Some(bar) = bar else {
                return;
            };
            let handle = bar.handle_rect;
            window.paint_quad(
                fill(
                    Bounds::new(
                        point(
                            bounds.origin.x + px(handle.x),
                            bounds.origin.y + px(handle.y - bar.track_rect.y),
                        ),
                        size(px(handle.width), px(handle.height)),
                    ),
                    rgb(0x7a8494),
                )
                .corner_radii(px(6.0)),
            );
        },
    )
    .absolute()
    .size_full()
}

fn scroll_bar_layout_for_bounds(
    content_width: f32,
    scroll_x: f32,
    bounds: Bounds<Pixels>,
) -> Option<HorizontalScrollBarLayout> {
    horizontal_scroll_bar_layout(
        content_width,
        scroll_x,
        normalize_viewport_extent(bounds.size.width.as_f32()),
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )
}

fn handle_item_click(
    app: &mut FikaApp,
    pane_id: PaneId,
    path: PathBuf,
    is_dir: bool,
    mode: FileGridMode,
    event: &gpui::ClickEvent,
) {
    app.dismiss_context_menu();
    app.panes.focus(pane_id);

    let extend = event.modifiers().shift;
    let toggle = event.modifiers().secondary();
    let double_click = event.click_count() >= 2;

    match mode {
        FileGridMode::Manager => {
            if double_click && is_dir {
                app.load_pane(pane_id, path);
            } else if extend {
                app.select_range_to(pane_id, path);
            } else if toggle {
                app.toggle_selection(pane_id, path);
            } else {
                app.select_only(pane_id, path);
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
                    return;
                }
                if extend {
                    app.select_range_to(pane_id, path);
                } else if toggle || multiple {
                    app.toggle_selection(pane_id, path);
                } else {
                    app.select_only(pane_id, path);
                }
            } else if is_dir {
                app.select_only(pane_id, path);
            } else if double_click && !multiple {
                app.choose_path(path);
            } else if extend {
                app.select_range_to(pane_id, path);
            } else if toggle || multiple {
                app.toggle_selection(pane_id, path);
            } else {
                app.select_only(pane_id, path);
            }
        }
    }
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
    let path_for_click = item.path.clone();
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
                .hover(move |tile| {
                    tile.bg(item_tile_hover_background(selected, drop_target))
                })
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.standard_click() {
                            handle_item_click(
                                this,
                                pane_id,
                                path_for_click.clone(),
                                is_dir_for_click,
                                mode,
                                event,
                            );
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
                .when(is_dir_for_drop, |tile| {
                    let target_dir_for_move = target_dir_for_drop.clone();
                    tile.on_drag_move::<ItemDrag>(cx.listener(
                        move |this,
                              _event: &gpui::DragMoveEvent<ItemDrag>,
                              _window,
                              cx| {
                            if this.set_item_drag_drop_target_for_directory(
                                pane_id,
                                target_dir_for_move.clone(),
                            ) {
                                cx.notify();
                            }
                            cx.stop_propagation();
                        },
                    ))
                    .on_drop::<ItemDrag>(cx.listener(
                        move |this, drag: &ItemDrag, window, cx| {
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
                        },
                    ))
                })
                .when(!is_dir_for_drop, |tile| {
                    tile.on_drag_move::<ItemDrag>(cx.listener(
                        move |this,
                              _event: &gpui::DragMoveEvent<ItemDrag>,
                              _window,
                              cx| {
                            if this.set_item_drag_drop_target_for_pane(pane_id) {
                                cx.notify();
                            }
                            cx.stop_propagation();
                        },
                    ))
                    .on_drop::<ItemDrag>(cx.listener(
                        move |this, drag: &ItemDrag, window, cx| {
                            let mode = file_transfer_mode_for_modifiers(window.modifiers());
                            this.drop_item_drag_to_pane(pane_id, drag.payload(), mode, cx);
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ))
                })
                .child(icon_view(&item, item.layout))
                .child(text_view(
                    &display_name,
                    &item.kind_label,
                    item.layout,
                    renaming,
                    selected,
                )),
        )
}

fn item_tile_background(selected: bool, drop_target: bool) -> Rgba {
    if drop_target {
        rgba(0x16a34a2e)
    } else if selected {
        rgb(0xdbeafe)
    } else {
        rgba(0x00000000)
    }
}

fn item_tile_hover_background(selected: bool, drop_target: bool) -> Rgba {
    if drop_target {
        rgba(0x16a34a3d)
    } else if selected {
        rgb(0xdbeafe)
    } else {
        rgb(0xf1f5f9)
    }
}

fn icon_view(item: &VisibleItemSnapshot, layout: ItemLayout) -> Div {
    let visual = layout.visual_rect;
    let icon = layout.icon_rect;
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

    match &item.icon.path {
        Some(path) => {
            icon_container.child(img(path.clone()).size_full().with_fallback(move || {
                fallback_icon_element(fallback.clone(), fallback_fg, fallback_bg)
            }))
        }
        None => icon_container.child(fallback_icon_element(fallback, fallback_fg, fallback_bg)),
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
    display_name: &str,
    kind_label: &str,
    layout: ItemLayout,
    renaming: bool,
    selected: bool,
) -> Div {
    let visual = layout.visual_rect;
    let text = layout.text_rect;
    div()
        .absolute()
        .left(px(text.x - visual.x))
        .top(px(text.y - visual.y))
        .w(px(text.width))
        .h(px(text.height))
        .when(renaming, |name| {
            name.border_1()
                .rounded_md()
                .border_color(rgb(0x2f6fed))
                .bg(rgb(0xffffff))
                .px_1()
        })
        .child(
            div()
                .text_sm()
                .truncate()
                .text_color(if selected {
                    rgb(0x0f172a)
                } else {
                    rgb(0x24292f)
                })
                .child(display_name.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x6b7280))
                .truncate()
                .child(kind_label.to_string()),
        )
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
    use super::drag_preview_label;

    #[test]
    fn drag_preview_uses_selection_count_only_for_selected_items() {
        assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
        assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
        assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
    }
}

pub(crate) fn compact_layout_options(
    view: &ViewState,
    reserved_bottom: f32,
) -> CompactLayoutOptions {
    let icon_size = view.icon_size();
    let padding = 8.0;
    let gap = 8.0;
    let text_height = 32.0;
    CompactLayoutOptions {
        viewport_width: view.viewport_width.max(1.0),
        viewport_height: view.viewport_height.max(1.0),
        reserved_bottom,
        scroll_x: view.scroll_x,
        scroll_y: view.scroll_y,
        padding,
        gap,
        item_width: icon_size + 120.0,
        item_height: (icon_size + 28.0).max(text_height + padding * 2.0),
        icon_size,
        text_height,
        ..CompactLayoutOptions::default()
    }
}

fn content_point_from_window(
    position: gpui::Point<gpui::Pixels>,
    bounds: gpui::Bounds<gpui::Pixels>,
    view: &ViewState,
) -> ViewPoint {
    ViewPoint {
        x: (position.x - bounds.origin.x).as_f32() + view.scroll_x,
        y: (position.y - bounds.origin.y).as_f32() + view.scroll_y,
    }
}

fn rubber_band_overlay(rect: ViewRect) -> Stateful<Div> {
    div()
        .id("rubber-band")
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width.max(1.0)))
        .h(px(rect.height.max(1.0)))
        .border_1()
        .border_color(rgb(0x2f6fed))
        .bg(rgb(0x2f6fed))
        .opacity(0.18)
}
