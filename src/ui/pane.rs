use crate::{
    BreadcrumbSegment, FikaApp, FilterBarSnapshot, LocationDraftSnapshot, PaneSnapshot,
    file_transfer_mode_for_modifiers,
};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, ExternalPaths, MouseButton, NavigationDirection, ParentElement, Pixels,
    SharedString, Stateful, Styled, TextRun, Window, canvas, div, fill, point, px, rgb, rgba, size,
};

use super::file_grid::{FileGridMode, FileGridProps, ItemDrag, file_grid};
use super::status_bar::status_bar;

pub(crate) struct PaneProps {
    pub snapshot: PaneSnapshot,
    pub file_grid_mode: FileGridMode,
    pub mouse_overlay_active: bool,
}

pub(crate) fn pane_view(props: PaneProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let PaneProps {
        snapshot,
        file_grid_mode,
        mouse_overlay_active,
    } = props;
    let PaneSnapshot {
        id: pane_id,
        split_ratio,
        breadcrumbs,
        location_draft,
        filter_bar,
        status_bar: status_bar_snapshot,
        layout,
        visible_items,
        view,
        rubber_band,
        drop_target,
        scrollbar_drag_active,
        focused,
    } = snapshot;
    let visible_width = view.viewport_width;
    let border = if focused {
        rgb(0x2f6fed)
    } else {
        rgb(0xb6bcc6)
    };
    div()
        .id(format!("pane-{}", pane_id.0))
        .flex()
        .flex_col()
        .flex_grow_0()
        .flex_shrink_1()
        .flex_basis(gpui::relative(split_ratio.max(0.001)))
        .min_w_0()
        .max_w_full()
        .overflow_hidden()
        .border_1()
        .rounded_md()
        .border_color(border)
        .bg(rgb(0xffffff))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.panes.focus(pane_id);
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.panes.focus(pane_id);
                this.go_back(pane_id);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.panes.focus(pane_id);
                this.go_forward(pane_id);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .min_w_0()
                .max_w_full()
                .overflow_hidden()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(rgb(0xd5d9df))
                .bg(if focused {
                    rgb(0xeaf1ff)
                } else {
                    rgb(0xf6f7f9)
                })
                .child(location_bar(
                    pane_id,
                    breadcrumbs,
                    location_draft,
                    focused,
                    cx,
                )),
        )
        .when_some(filter_bar, |pane, filter| {
            pane.child(filter_bar_view(pane_id, filter, cx))
        })
        .child(file_grid(
            FileGridProps {
                pane_id,
                layout,
                visible_items,
                view,
                rubber_band,
                drop_target,
                scrollbar_drag_active,
                mode: file_grid_mode,
                mouse_overlay_active,
            },
            cx,
        ))
        .child(status_bar(pane_id, visible_width, status_bar_snapshot, cx))
}

fn filter_bar_view(
    pane_id: fika_core::PaneId,
    filter: FilterBarSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let mode_label = match filter.mode {
        fika_core::NameFilterMode::PlainText => "Plain",
        fika_core::NameFilterMode::Glob => "Glob",
    };
    let case_label = if filter.case_sensitive { "Aa" } else { "aa" };
    let query_empty = filter.query.is_empty();
    let query = if query_empty {
        "Filter".to_string()
    } else {
        filter.query
    };
    let match_count = filter.match_count;

    div()
        .id(format!("filter-bar-{}", pane_id.0))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(rgb(0xd5d9df))
        .bg(rgb(0xf8fafc))
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .id(format!("filter-input-{}", pane_id.0))
                .flex()
                .items_center()
                .flex_1()
                .h(px(26.0))
                .px_2()
                .border_1()
                .rounded_md()
                .border_color(if filter.focused {
                    rgb(0x2f6fed)
                } else {
                    rgb(0xb6bcc6)
                })
                .bg(rgb(0xffffff))
                .overflow_hidden()
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, _event: &gpui::ClickEvent, _window, cx| {
                        this.focus_filter_bar(pane_id);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .text_color(if query_empty {
                            rgb(0x8b95a1)
                        } else {
                            rgb(0x111827)
                        })
                        .child(query),
                )
                .when(filter.focused, |input| {
                    input.child(div().w(px(1.0)).h(px(18.0)).bg(rgb(0x2f6fed)))
                }),
        )
        .child(
            filter_button(format!("filter-mode-{}", pane_id.0), mode_label).on_click(cx.listener(
                move |this, event: &gpui::ClickEvent, _window, cx| {
                    if event.standard_click() {
                        this.toggle_filter_mode(pane_id);
                        cx.stop_propagation();
                        cx.notify();
                    }
                },
            )),
        )
        .child(
            filter_button(format!("filter-case-{}", pane_id.0), case_label).on_click(cx.listener(
                move |this, event: &gpui::ClickEvent, _window, cx| {
                    if event.standard_click() {
                        this.toggle_filter_case_sensitive(pane_id);
                        cx.stop_propagation();
                        cx.notify();
                    }
                },
            )),
        )
        .child(
            div()
                .w(px(72.0))
                .truncate()
                .text_xs()
                .text_color(rgb(0x59636e))
                .child(format!("{match_count} match")),
        )
        .child(
            filter_button(format!("filter-close-{}", pane_id.0), "Close").on_click(cx.listener(
                move |this, event: &gpui::ClickEvent, _window, cx| {
                    if event.standard_click() {
                        this.close_filter_bar(pane_id);
                        cx.stop_propagation();
                        cx.notify();
                    }
                },
            )),
        )
}

fn filter_button(id: String, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(26.0))
        .px_2()
        .rounded_md()
        .text_xs()
        .text_color(rgb(0x1f2937))
        .bg(rgb(0xe8eef7))
        .hover(|button| button.bg(rgb(0xdbe7fb)))
        .cursor_pointer()
        .child(label)
}

fn location_bar(
    pane_id: fika_core::PaneId,
    breadcrumbs: Vec<BreadcrumbSegment>,
    location_draft: Option<LocationDraftSnapshot>,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    match location_draft {
        Some(draft) => editable_location_bar(pane_id, draft, focused, cx),
        None => breadcrumb_location_bar(pane_id, breadcrumbs, focused, cx),
    }
}

fn editable_location_bar(
    pane_id: fika_core::PaneId,
    draft: LocationDraftSnapshot,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id(format!("location-edit-{}", pane_id.0))
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .max_w_full()
        .h(px(28.0))
        .px_2()
        .border_1()
        .rounded_md()
        .border_color(if focused {
            rgb(0x2f6fed)
        } else {
            rgb(0xb6bcc6)
        })
        .bg(rgb(0xffffff))
        .overflow_hidden()
        .cursor_text()
        .text_sm()
        .text_color(rgb(0x111827))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                this.set_location_caret_from_window_x(pane_id, event.position.x.as_f32());
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(location_edit_text(pane_id, draft, focused, cx))
}

fn location_edit_text(
    pane_id: fika_core::PaneId,
    draft: LocationDraftSnapshot,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> impl IntoElement {
    let app = cx.weak_entity();
    canvas(
        move |bounds, window, cx| {
            location_edit_prepaint(bounds, window, pane_id, draft, focused, app, cx)
        },
        move |bounds, state, window, cx| {
            let origin = point(
                bounds.origin.x + px(state.x_offset),
                bounds.origin.y + px(state.y_offset),
            );
            state
                .line
                .paint(
                    origin,
                    state.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .unwrap();
            if let Some(cursor) = state.cursor {
                window.paint_quad(cursor);
            }
        },
    )
    .size_full()
}

struct LocationEditPrepaint {
    line: gpui::ShapedLine,
    cursor: Option<gpui::PaintQuad>,
    x_offset: f32,
    y_offset: f32,
    line_height: Pixels,
}

fn location_edit_prepaint(
    bounds: Bounds<Pixels>,
    window: &mut Window,
    pane_id: fika_core::PaneId,
    draft: LocationDraftSnapshot,
    focused: bool,
    app: gpui::WeakEntity<FikaApp>,
    cx: &mut gpui::App,
) -> LocationEditPrepaint {
    let style = window.text_style();
    let caret = draft.caret.min(draft.value.len());
    let text = SharedString::from(draft.value.clone());
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let font_size = style.font_size.to_pixels(window.rem_size());
    let line = window
        .text_system()
        .shape_line(text.clone(), font_size, &[run], None);
    let line_height = window.line_height();
    let caret_x = line.x_for_index(caret).as_f32();
    let available_width = bounds.size.width.as_f32().max(1.0);
    let scroll_x = location_scroll_for_caret(
        caret_x,
        line.width().as_f32(),
        available_width,
        draft.scroll_x,
    );
    let x_offset = -scroll_x;
    let y_offset = ((bounds.size.height - line_height).as_f32() / 2.0)
        .max(0.0)
        .floor();
    let byte_positions = location_byte_positions(&draft.value, &line);
    let _ = app.update(cx, |this, _cx| {
        this.update_location_edit_metrics(
            pane_id,
            draft.value.clone(),
            bounds.origin.x.as_f32(),
            scroll_x,
            available_width,
            byte_positions,
        );
    });
    let cursor = focused.then(|| {
        let cursor_x = location_cursor_x(caret_x, scroll_x, available_width);
        fill(
            Bounds::new(
                point(
                    bounds.origin.x + px(cursor_x),
                    bounds.origin.y + px(y_offset),
                ),
                size(px(2.0), line_height),
            ),
            rgb(0x2f6fed),
        )
    });
    LocationEditPrepaint {
        line,
        cursor,
        x_offset,
        y_offset,
        line_height,
    }
}

fn location_scroll_for_caret(
    caret_x: f32,
    line_width: f32,
    available_width: f32,
    current_scroll_x: f32,
) -> f32 {
    if available_width <= 1.0 {
        return 0.0;
    }
    let padding = 6.0_f32.min((available_width / 2.0).max(0.0));
    let max_scroll_x = (line_width - available_width + padding).max(0.0);
    let mut scroll_x = current_scroll_x.clamp(0.0, max_scroll_x);
    if caret_x < scroll_x + padding {
        scroll_x = (caret_x - padding).max(0.0);
    }
    if caret_x > scroll_x + available_width - padding {
        scroll_x = (caret_x - available_width + padding).clamp(0.0, max_scroll_x);
    }
    scroll_x.floor()
}

fn location_cursor_x(caret_x: f32, scroll_x: f32, available_width: f32) -> f32 {
    if available_width <= 2.0 {
        return 0.0;
    }
    (caret_x - scroll_x).clamp(0.0, available_width - 2.0)
}

fn location_byte_positions(value: &str, line: &gpui::ShapedLine) -> Vec<(usize, f32)> {
    let mut positions = Vec::with_capacity(value.chars().count() + 1);
    positions.push((0, 0.0));
    for (index, _) in value.char_indices().skip(1) {
        positions.push((index, line.x_for_index(index).as_f32()));
    }
    positions.push((value.len(), line.x_for_index(value.len()).as_f32()));
    positions
}

fn breadcrumb_location_bar(
    pane_id: fika_core::PaneId,
    breadcrumbs: Vec<BreadcrumbSegment>,
    focused: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let segment_count = breadcrumbs.len();
    div()
        .id(format!("location-bar-{}", pane_id.0))
        .flex()
        .items_center()
        .gap_1()
        .flex_1()
        .min_w_0()
        .max_w_full()
        .h(px(28.0))
        .px_1()
        .rounded_md()
        .bg(if focused {
            rgb(0xf8fbff)
        } else {
            rgb(0xffffff)
        })
        .overflow_hidden()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.start_location_edit(pane_id);
            cx.stop_propagation();
            cx.notify();
        }))
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .children(breadcrumbs.into_iter().enumerate().map(|(index, segment)| {
            breadcrumb_segment(pane_id, index, index + 1 < segment_count, segment, cx)
        }))
}

fn breadcrumb_segment(
    pane_id: fika_core::PaneId,
    index: usize,
    show_separator: bool,
    segment: BreadcrumbSegment,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let path_for_click = segment.path.clone();
    let path_for_internal_move = segment.path.clone();
    let path_for_internal_drop = segment.path.clone();
    let path_for_external_move = segment.path.clone();
    let path_for_external_drop = segment.path.clone();
    div()
        .id(format!("location-segment-{}-{index}", pane_id.0))
        .flex()
        .items_center()
        .gap_1()
        .min_w_0()
        .flex_shrink_1()
        .overflow_hidden()
        .child(
            div()
                .id(format!("location-segment-button-{}-{index}", pane_id.0))
                .min_w_0()
                .flex_shrink_1()
                .px_2()
                .py_1()
                .rounded_md()
                .text_sm()
                .truncate()
                .text_color(rgb(0x1f2937))
                .hover(|button| button.bg(rgb(0xe8eef7)))
                .drag_over::<ItemDrag>(|style, _, _, _| style.bg(rgba(0x16a34a2e)))
                .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(rgba(0x16a34a2e)))
                .cursor_pointer()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.standard_click() {
                            this.open_location_segment(pane_id, path_for_click.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                )
                .on_drag_move::<ItemDrag>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position);
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        let changed = contains
                            && this.set_item_drag_drop_target_for_directory(
                                pane_id,
                                path_for_internal_move.clone(),
                                mode,
                            );
                        if contains {
                            this.schedule_drop_target_stale_clear(cx);
                        }
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drag_move::<ExternalPaths>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position);
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        let changed = contains
                            && this.set_item_drag_drop_target_for_directory(
                                pane_id,
                                path_for_external_move.clone(),
                                mode,
                            );
                        if contains {
                            this.schedule_drop_target_stale_clear(cx);
                        }
                        if changed {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    },
                ))
                .on_drop::<ItemDrag>(cx.listener(move |this, drag: &ItemDrag, window, cx| {
                    let mode = file_transfer_mode_for_modifiers(window.modifiers());
                    this.drop_item_drag_to_location(
                        pane_id,
                        drag.payload(),
                        path_for_internal_drop.clone(),
                        mode,
                        cx,
                    );
                    cx.stop_propagation();
                    cx.notify();
                }))
                .on_drop::<ExternalPaths>(cx.listener(
                    move |this, external_paths: &ExternalPaths, window, cx| {
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        this.drop_external_paths_to_location(
                            pane_id,
                            external_paths.paths().to_vec(),
                            path_for_external_drop.clone(),
                            mode,
                            cx,
                        );
                        cx.stop_propagation();
                        cx.notify();
                    },
                ))
                .child(segment.label),
        )
        .when(show_separator, |row| {
            row.child(div().text_sm().text_color(rgb(0x94a3b8)).child(">"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_cursor_x_handles_extremely_narrow_widths() {
        assert_eq!(location_cursor_x(24.0, 0.0, 1.0), 0.0);
        assert_eq!(location_cursor_x(24.0, 0.0, 2.0), 0.0);
        assert_eq!(location_cursor_x(24.0, 0.0, 10.0), 8.0);
    }

    #[test]
    fn location_scroll_for_caret_uses_narrow_safe_padding() {
        assert_eq!(location_scroll_for_caret(50.0, 100.0, 4.0, 0.0), 48.0);
        assert_eq!(location_scroll_for_caret(0.0, 100.0, 1.0, 80.0), 0.0);
    }
}
