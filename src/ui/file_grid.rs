use crate::{FikaApp, RubberBandDrag, VisibleItemSnapshot};
use fika_core::{
    CompactLayout, CompactLayoutOptions, Entry, HorizontalScrollBarLayout, ItemLayout, PaneId,
    ViewPoint, ViewRect, ViewState,
};
use gpui::prelude::*;
use gpui::{Bounds, Context, Div, ParentElement, Stateful, Styled, div, px, rgb};
use std::path::PathBuf;

const SCROLLBAR_THICKNESS: f32 = 12.0;
const SCROLLBAR_MIN_HANDLE_WIDTH: f32 = 36.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileGridMode {
    Manager,
    Chooser { directories: bool, multiple: bool },
}

pub(crate) struct FileGridProps {
    pub pane_id: PaneId,
    pub item_count: usize,
    pub visible_items: Vec<VisibleItemSnapshot>,
    pub view: ViewState,
    pub rubber_band: Option<ViewRect>,
    pub mode: FileGridMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollBarDrag {
    pane_id: PaneId,
    bar: HorizontalScrollBarLayout,
}

pub(crate) fn file_grid(props: FileGridProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let FileGridProps {
        pane_id,
        item_count,
        visible_items,
        view,
        rubber_band,
        mode,
    } = props;
    let layout = compact_layout(item_count, &view);
    let content_size = layout.content_size();
    let max_scroll_x = (content_size.width - layout.viewport_rect().width).max(0.0);
    let max_scroll_y = (content_size.height - layout.viewport_rect().height).max(0.0);
    let scroll_bar = layout.horizontal_scroll_bar(SCROLLBAR_THICKNESS, SCROLLBAR_MIN_HANDLE_WIDTH);
    let app = cx.weak_entity();

    div()
        .on_children_prepainted(move |bounds, _window, cx| {
            let Some(bounds) = bounds.first() else {
                return;
            };
            let width = bounds.size.width.as_f32();
            let height = bounds.size.height.as_f32();
            let _ = app.update(cx, |this, cx| {
                if this
                    .panes
                    .set_viewport_bounds(pane_id, width, height, max_scroll_x, max_scroll_y)
                    .unwrap_or(false)
                {
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
                .on_drag(RubberBandDrag { pane_id }, |_, _, _, cx| {
                    cx.new(|_| gpui::Empty)
                })
                .on_drag_move::<RubberBandDrag>({
                    let layout = layout.clone();
                    let view = view.clone();
                    cx.listener(
                        move |this, event: &gpui::DragMoveEvent<RubberBandDrag>, _window, cx| {
                            let current = content_point_from_window(
                                event.event.position,
                                event.bounds,
                                &view,
                            );
                            if this
                                .rubber_band
                                .as_ref()
                                .is_none_or(|band| band.pane_id != pane_id)
                            {
                                this.start_rubber_band(pane_id, current);
                            }
                            this.update_rubber_band(pane_id, current, layout.clone());
                            cx.notify();
                        },
                    )
                })
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
                        .children(visible_items.into_iter().map(|item| {
                            let path = item.entry.path.clone();
                            let is_dir = item.entry.is_dir;
                            item_tile(
                                item.slot_id,
                                item.entry,
                                item.selected,
                                item.draft_name,
                                item.layout,
                            )
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, _window, cx| {
                                    handle_item_click(
                                        this,
                                        pane_id,
                                        path.clone(),
                                        is_dir,
                                        mode,
                                        event.modifiers().shift,
                                        event.modifiers().secondary(),
                                    );
                                    cx.notify();
                                },
                            ))
                        })),
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
            cx.new(|_| gpui::Empty)
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
    extend: bool,
    toggle: bool,
) {
    match mode {
        FileGridMode::Manager => {
            if extend {
                app.select_range_to(pane_id, path);
            } else if toggle {
                app.toggle_selection(pane_id, path);
            } else if is_dir {
                app.load_pane(pane_id, path);
            } else {
                app.select_only(pane_id, path);
            }
        }
        FileGridMode::Chooser {
            directories,
            multiple,
        } => {
            if directories {
                if !is_dir {
                    return;
                }
                if multiple {
                    if extend {
                        app.select_range_to(pane_id, path);
                    } else {
                        app.toggle_selection(pane_id, path);
                    }
                } else {
                    app.choose_path(path);
                }
            } else if is_dir {
                app.load_pane(pane_id, path);
            } else if multiple {
                if extend {
                    app.select_range_to(pane_id, path);
                } else {
                    app.toggle_selection(pane_id, path);
                }
            } else {
                app.choose_path(path);
            }
        }
    }
}

fn item_tile(
    slot_id: u64,
    entry: Entry,
    selected: bool,
    draft_name: Option<String>,
    layout: ItemLayout,
) -> Stateful<Div> {
    let marker = if entry.is_dir { "[D]" } else { "[F]" };
    let id = format!("item-slot-{slot_id}");
    let renaming = draft_name.is_some();
    let display_name = draft_name.unwrap_or_else(|| entry.name.clone());
    let item = layout.item_rect;
    let icon = layout.icon_rect;
    let text = layout.text_rect;
    div()
        .id(id)
        .absolute()
        .left(px(item.x))
        .top(px(item.y))
        .w(px(item.width))
        .h(px(item.height))
        .rounded_md()
        .border_1()
        .border_color(if selected {
            rgb(0x2f6fed)
        } else {
            rgb(0xd5d9df)
        })
        .bg(if selected {
            rgb(0xeaf1ff)
        } else {
            rgb(0xffffff)
        })
        .hover(|tile| tile.bg(rgb(0xf3f7ff)).border_color(rgb(0x7aa7ff)))
        .cursor_pointer()
        .child(
            div()
                .absolute()
                .left(px(icon.x - item.x))
                .top(px(icon.y - item.y))
                .w(px(icon.width))
                .h(px(icon.height))
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(if entry.is_dir {
                    rgb(0x0b5cad)
                } else {
                    rgb(0x59636e)
                })
                .bg(if entry.is_dir {
                    rgb(0xeaf4ff)
                } else {
                    rgb(0xf2f4f7)
                })
                .child(marker),
        )
        .child(
            div()
                .absolute()
                .left(px(text.x - item.x))
                .top(px(text.y - item.y))
                .w(px(text.width))
                .h(px(text.height))
                .when(renaming, |name| {
                    name.border_1()
                        .rounded_md()
                        .border_color(rgb(0x2f6fed))
                        .bg(rgb(0xffffff))
                        .px_1()
                })
                .child(div().text_sm().truncate().child(display_name))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x6b7280))
                        .truncate()
                        .child(entry.kind),
                ),
        )
}

pub(crate) fn compact_layout(item_count: usize, view: &ViewState) -> CompactLayout {
    let layout = CompactLayout::new(item_count, compact_layout_options(view, 0.0));
    if layout
        .horizontal_scroll_bar(SCROLLBAR_THICKNESS, SCROLLBAR_MIN_HANDLE_WIDTH)
        .is_some()
    {
        CompactLayout::new(
            item_count,
            compact_layout_options(view, SCROLLBAR_THICKNESS),
        )
    } else {
        layout
    }
}

fn compact_layout_options(view: &ViewState, reserved_bottom: f32) -> CompactLayoutOptions {
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
    bounds: Bounds<gpui::Pixels>,
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
