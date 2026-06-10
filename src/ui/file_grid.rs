use crate::{FikaApp, RubberBandDrag, VisibleItemSnapshot};
use fika_core::{
    CompactLayout, CompactLayoutOptions, HorizontalScrollBarLayout, ItemLayout, PaneId, ViewPoint,
    ViewRect, ViewState,
};
use gpui::prelude::*;
use gpui::{
    Context, Div, Empty, MouseButton, ParentElement, Render, Stateful, Styled, Window, div, px,
    rgb, rgba,
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
    pub(crate) mode: FileGridMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollBarDrag {
    pane_id: PaneId,
    bar: HorizontalScrollBarLayout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ItemDrag {
    pane_id: PaneId,
    path: PathBuf,
    name: Arc<str>,
}

struct DragPreview {
    name: Arc<str>,
}

pub(crate) fn file_grid(props: FileGridProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let FileGridProps {
        pane_id,
        layout,
        visible_items,
        view,
        rubber_band,
        mode,
    } = props;
    let content_size = layout.content_size();
    let max_scroll_x = (content_size.width - layout.viewport_rect().width).max(0.0);
    let max_scroll_y = (content_size.height - layout.viewport_rect().height).max(0.0);
    let scroll_bar = layout.horizontal_scroll_bar(SCROLLBAR_THICKNESS, SCROLLBAR_MIN_HANDLE_WIDTH);
    let app = cx.weak_entity();
    let drag_view = view.clone();

    div()
        .on_children_prepainted(move |bounds, _window, cx| {
            let Some(bounds) = bounds.first() else {
                return;
            };
            let width = bounds.size.width.as_f32();
            let height = bounds.size.height.as_f32();
            let origin = ViewPoint {
                x: bounds.origin.x.as_f32(),
                y: bounds.origin.y.as_f32(),
            };
            let _ = app.update(cx, |this, cx| {
                let origin_changed = this.set_viewport_origin(pane_id, origin);
                let bounds_changed = this
                    .panes
                    .set_viewport_bounds(pane_id, width, height, max_scroll_x, max_scroll_y)
                    .unwrap_or(false);
                if origin_changed || bounds_changed {
                    cx.notify();
                }
            });
        })
        .id(format!("items-{}", pane_id.0))
        .relative()
        .overflow_hidden()
        .flex_1()
        .child(
            div()
                .id(format!("items-viewport-{}", pane_id.0))
                .relative()
                .size_full()
                .overflow_hidden()
                .on_scroll_wheel(cx.listener(
                    move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                        let delta = event.delta.pixel_delta(window.line_height());
                        let horizontal_delta = -(delta.x.as_f32() + delta.y.as_f32());
                        this.panes.scroll_view(
                            pane_id,
                            horizontal_delta,
                            0.0,
                            max_scroll_x,
                            max_scroll_y,
                        );
                        cx.stop_propagation();
                        cx.notify();
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
                .child(
                    div()
                        .absolute()
                        .left(px(-view.scroll_x))
                        .top(px(-view.scroll_y))
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
                })
                .when_some(scroll_bar, |viewport, bar| {
                    viewport.child(horizontal_scroll_bar(pane_id, bar, cx))
                }),
        )
}

fn horizontal_scroll_bar(
    pane_id: PaneId,
    bar: HorizontalScrollBarLayout,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let track = bar.track_rect;
    let handle = bar.handle_rect;
    div()
        .id(format!("scrollbar-x-{}", pane_id.0))
        .absolute()
        .left(px(track.x))
        .top(px(track.y))
        .w(px(track.width))
        .h(px(track.height))
        .bg(rgb(0xe6e9ef))
        .on_drag(ScrollBarDrag { pane_id, bar }, |_, _, _, cx| {
            cx.new(|_| Empty)
        })
        .on_drag_move::<ScrollBarDrag>(cx.listener(
            move |this, event: &gpui::DragMoveEvent<ScrollBarDrag>, _window, cx| {
                let drag = *event.drag(cx);
                let track_x = (event.event.position.x - event.bounds.origin.x).as_f32();
                let scroll_x = drag.bar.scroll_x_for_track_x(track_x);
                this.panes
                    .set_view_scroll(drag.pane_id, scroll_x, 0.0, drag.bar.max_scroll_x, 0.0);
                cx.stop_propagation();
                cx.notify();
            },
        ))
        .on_drop::<ScrollBarDrag>(
            cx.listener(move |_this, _drag: &ScrollBarDrag, _window, cx| {
                cx.stop_propagation();
            }),
        )
        .child(
            div()
                .id(format!("scrollbar-x-handle-{}", pane_id.0))
                .absolute()
                .left(px(handle.x))
                .top(px(handle.y - track.y))
                .w(px(handle.width))
                .h(px(handle.height))
                .rounded_md()
                .bg(rgb(0x7a8494))
                .hover(|handle| handle.bg(rgb(0x5f6b7a)))
                .cursor_pointer(),
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
    let is_dir_for_click = item.is_dir;
    let is_dir_for_menu = item.is_dir;
    let selected = item.selected;
    let drag_value = ItemDrag {
        pane_id,
        path: path_for_drag,
        name: item.name.clone(),
    };

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
                .bg(if selected {
                    rgb(0xdbeafe)
                } else {
                    rgba(0x00000000)
                })
                .hover(move |tile| {
                    if selected {
                        tile.bg(rgb(0xdbeafe))
                    } else {
                        tile.bg(rgb(0xf1f5f9))
                    }
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
                .on_drag(drag_value, |drag, _, _, cx| {
                    cx.new(|_| DragPreview {
                        name: drag.name.clone(),
                    })
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

fn icon_view(item: &VisibleItemSnapshot, layout: ItemLayout) -> Div {
    let visual = layout.visual_rect;
    let icon = layout.icon_rect;
    div()
        .absolute()
        .left(px(icon.x - visual.x))
        .top(px(icon.y - visual.y))
        .w(px(icon.width))
        .h(px(icon.height))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(item.icon.fg))
        .bg(rgb(item.icon.bg))
        .child(item.icon.marker.clone())
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
            .child(self.name.to_string())
    }
}

pub(crate) fn compact_layout_options(
    view: &ViewState,
    reserved_bottom: f32,
) -> CompactLayoutOptions {
    CompactLayoutOptions {
        viewport_width: view.viewport_width.max(1.0),
        viewport_height: view.viewport_height.max(1.0),
        reserved_bottom,
        scroll_x: view.scroll_x,
        scroll_y: view.scroll_y,
        icon_size: view.icon_size.max(32.0),
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
