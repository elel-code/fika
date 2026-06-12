use crate::{
    FikaApp, FileTransferMode, ItemDragPayload, RubberBandDrag, VisibleItemSnapshot,
    file_transfer_mode_for_modifiers,
};
use fika_core::{
    CompactLayout, CompactLayoutOptions, HorizontalScrollBarLayout, ItemLayout, PaneId, ViewPoint,
    ViewRect, ViewState, horizontal_scroll_bar_layout, normalize_viewport_extent,
};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, Empty, ExternalPaths, MouseButton, NavigationDirection, ParentElement,
    Pixels, Render, Rgba, ScrollDelta, Stateful, Styled, StyledImage, Window, canvas, div, fill,
    img, point, px, rgb, rgba, size,
};
use std::path::PathBuf;
use std::sync::Arc;

use super::places::PlaceDrag;

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
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) mode: FileGridMode,
    pub(crate) mouse_overlay_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ActiveScrollBarDrag {
    pub(crate) pane_id: PaneId,
    pub(crate) content_width: f32,
    pub(crate) initial_scroll_x: f32,
    pub(crate) start_track_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HorizontalScrollBarTrack {
    pub(crate) origin_x: f32,
    pub(crate) width: f32,
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

pub(crate) fn file_grid(props: FileGridProps, cx: &mut Context<FikaApp>) -> Stateful<Div> {
    let FileGridProps {
        pane_id,
        layout,
        visible_items,
        view,
        rubber_band,
        drop_target,
        mode,
        mouse_overlay_active,
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
    let viewport_wheel_layout = layout.clone();
    let scrollbar_wheel_layout = layout.clone();
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
                .bg(drop_target.map_or(rgba(0x00000000), drop_target_viewport_background))
                .occlude()
                .overflow_hidden()
                .on_scroll_wheel(cx.listener(
                    move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                        handle_file_grid_wheel(
                            this,
                            pane_id,
                            event,
                            window,
                            &viewport_wheel_layout,
                            visible_width,
                            max_scroll_x,
                            max_scroll_y,
                            cx,
                        );
                    },
                ))
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                        let started =
                            this.start_rubber_band_from_window_if_blank(pane_id, event.position);
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
                        if event.standard_click()
                            && this.handle_blank_click(pane_id, event.position())
                        {
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
                        let shown = this.show_blank_context_menu_if_blank(pane_id, event.position);
                        cx.stop_propagation();
                        if shown {
                            cx.notify();
                        }
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
                        let current = content_point_from_window(
                            event.event.position,
                            event.bounds,
                            &drag_view,
                        );
                        if !this
                            .rubber_band
                            .as_ref()
                            .is_some_and(|band| band.pane_id == pane_id)
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
                    move |this, event: &gpui::DragMoveEvent<ItemDrag>, window, cx| {
                        let contains = event.bounds.contains(&event.event.position)
                            && this.window_position_is_blank_in_pane(pane_id, event.event.position);
                        let mode = file_transfer_mode_for_modifiers(window.modifiers());
                        let changed =
                            contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                        if contains {
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
                        let changed =
                            contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                        if contains {
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
                        let changed =
                            contains && this.set_item_drag_drop_target_for_pane(pane_id, mode);
                        if contains {
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
                        .absolute()
                        .left(px(-view.scroll_x.round()))
                        .top(px(-view.scroll_y.round()))
                        .w(px(content_size.width))
                        .h(px(content_size.height))
                        .children(
                            visible_items.into_iter().map(|item| {
                                item_tile(pane_id, item, mode, mouse_overlay_active, cx)
                            }),
                        ),
                )
                .when_some(rubber_band, |viewport, rect| {
                    viewport.child(rubber_band_overlay(rect, view.scroll_x, view.scroll_y))
                }),
        )
        .when(scroll_bar_visible, |grid| {
            let scrollbar_track_app = cx.weak_entity();
            grid.child(
                div()
                    .on_children_prepainted(move |bounds, _window, cx| {
                        let Some(bounds) = bounds.first() else {
                            return;
                        };
                        let _ = scrollbar_track_app.update(cx, |this, _cx| {
                            this.set_horizontal_scrollbar_track(
                                pane_id,
                                bounds.origin.x.as_f32(),
                                bounds.size.width.as_f32(),
                            );
                        });
                    })
                    .id(format!("scrollbar-x-reserve-{}", pane_id.0))
                    .h(px(SCROLLBAR_THICKNESS))
                    .w_full()
                    .max_w_full()
                    .min_w_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Navigate(NavigationDirection::Back),
                        cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                            handle_pane_navigation_mouse_down(
                                this,
                                pane_id,
                                NavigationDirection::Back,
                            );
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
                    .on_scroll_wheel(cx.listener(
                        move |this, event: &gpui::ScrollWheelEvent, window, cx| {
                            handle_file_grid_wheel(
                                this,
                                pane_id,
                                event,
                                window,
                                &scrollbar_wheel_layout,
                                visible_width,
                                max_scroll_x,
                                max_scroll_y,
                                cx,
                            );
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                            let started = this.begin_horizontal_scrollbar_drag_from_window(
                                pane_id,
                                content_size.width,
                                view.scroll_x,
                                event.position,
                            );
                            cx.stop_propagation();
                            if started {
                                cx.notify();
                            }
                        }),
                    )
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
        .occlude()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                let started = this.begin_horizontal_scrollbar_drag_from_window(
                    pane_id,
                    content_width,
                    scroll_x,
                    event.position,
                );
                cx.stop_propagation();
                if started {
                    cx.notify();
                }
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
        .child(scroll_bar_handle_canvas(
            pane_id,
            content_width,
            scroll_x,
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

fn handle_file_grid_wheel(
    app: &mut FikaApp,
    pane_id: PaneId,
    event: &gpui::ScrollWheelEvent,
    window: &mut Window,
    layout: &CompactLayout,
    visible_width: f32,
    max_scroll_x: f32,
    max_scroll_y: f32,
    cx: &mut Context<FikaApp>,
) {
    if event.modifiers.control {
        app.finish_rubber_band(pane_id);
        app.zoom_pane_from_wheel(pane_id, event.delta);
        cx.stop_propagation();
        cx.notify();
        return;
    }

    app.finish_rubber_band(pane_id);
    let horizontal_delta =
        horizontal_wheel_scroll_delta(event.delta, window.line_height(), layout, visible_width);
    app.scroll_pane_smooth(
        pane_id,
        horizontal_delta,
        0.0,
        max_scroll_x,
        max_scroll_y,
        cx,
    );
    cx.stop_propagation();
}

fn horizontal_wheel_scroll_delta(
    delta: ScrollDelta,
    line_height: Pixels,
    layout: &CompactLayout,
    visible_width: f32,
) -> f32 {
    match delta {
        ScrollDelta::Pixels(delta) => -(delta.x.as_f32() + delta.y.as_f32()),
        ScrollDelta::Lines(delta) => {
            let step = compact_wheel_line_step(layout, visible_width, line_height);
            -(delta.x + delta.y) * step
        }
    }
}

fn compact_wheel_line_step(layout: &CompactLayout, visible_width: f32, line_height: Pixels) -> f32 {
    let line_height = line_height.as_f32().max(1.0);
    let content_width = layout.content_size().width.max(0.0);
    let average_column_width = if layout.column_count() > 0 {
        content_width / layout.column_count() as f32
    } else {
        line_height * 8.0
    };
    let pane_step_cap = (visible_width.max(1.0) * 0.72).max(line_height * 4.0);
    (average_column_width / 3.0)
        .clamp(line_height * 3.0, pane_step_cap)
        .round()
}

fn scroll_x_for_scrollbar_drag(
    content_width: f32,
    initial_scroll_x: f32,
    start_track_x: f32,
    current_track_x: f32,
    track_width: f32,
) -> Option<(f32, f32)> {
    let initial_bar = horizontal_scroll_bar_layout(
        content_width,
        initial_scroll_x,
        track_width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    let mapping_bar = horizontal_scroll_bar_layout(
        content_width,
        0.0,
        track_width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    let handle_x = initial_bar.handle_rect.x + current_track_x - start_track_x;
    Some((
        mapping_bar.scroll_x_for_handle_x(handle_x),
        mapping_bar.max_scroll_x,
    ))
}

fn scroll_x_for_scrollbar_drag_start(
    content_width: f32,
    scroll_x: f32,
    start_track_x: f32,
    track_width: f32,
) -> Option<f32> {
    let bar = horizontal_scroll_bar_layout(
        content_width,
        scroll_x,
        track_width,
        SCROLLBAR_THICKNESS,
        SCROLLBAR_MIN_HANDLE_WIDTH,
    )?;
    if start_track_x >= bar.handle_rect.x && start_track_x <= bar.handle_rect.right() {
        return Some(scroll_x.clamp(0.0, bar.max_scroll_x));
    }
    Some(bar.scroll_x_for_track_x(start_track_x))
}

impl FikaApp {
    pub(crate) fn set_horizontal_scrollbar_track(
        &mut self,
        pane_id: PaneId,
        origin_x: f32,
        width: f32,
    ) -> bool {
        let width = normalize_viewport_extent(width).max(0.0);
        let track = HorizontalScrollBarTrack { origin_x, width };
        if self.horizontal_scrollbar_tracks.get(&pane_id) == Some(&track) {
            return false;
        }
        self.horizontal_scrollbar_tracks.insert(pane_id, track);
        true
    }

    pub(crate) fn begin_horizontal_scrollbar_drag_from_window(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        position: gpui::Point<gpui::Pixels>,
    ) -> bool {
        let Some(track) = self.horizontal_scrollbar_tracks.get(&pane_id).copied() else {
            return false;
        };
        let track_x = (position.x.as_f32() - track.origin_x).clamp(0.0, track.width);
        self.begin_horizontal_scrollbar_drag(pane_id, content_width, scroll_x, track_x, track.width)
    }

    pub(crate) fn begin_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        content_width: f32,
        scroll_x: f32,
        start_track_x: f32,
        track_width: f32,
    ) -> bool {
        let Some(initial_scroll_x) =
            scroll_x_for_scrollbar_drag_start(content_width, scroll_x, start_track_x, track_width)
        else {
            return false;
        };
        let max_scroll_x = (content_width - track_width).max(0.0);
        self.finish_rubber_band(pane_id);
        self.active_scrollbar_drag = Some(ActiveScrollBarDrag {
            pane_id,
            content_width,
            initial_scroll_x,
            start_track_x,
        });
        self.set_pane_scroll_immediate(pane_id, initial_scroll_x, 0.0, max_scroll_x, 0.0);
        true
    }

    pub(crate) fn update_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        track_x: f32,
        track_width: f32,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        let Some((scroll_x, max_scroll_x)) = scroll_x_for_scrollbar_drag(
            drag.content_width,
            drag.initial_scroll_x,
            drag.start_track_x,
            track_x,
            track_width,
        ) else {
            return false;
        };
        let previous_scroll_x = self
            .panes
            .pane(drag.pane_id)
            .map(|pane| pane.view.scroll_x)
            .unwrap_or_default();
        self.set_pane_scroll_immediate(drag.pane_id, scroll_x, 0.0, max_scroll_x, 0.0);
        self.panes
            .pane(drag.pane_id)
            .is_some_and(|pane| (pane.view.scroll_x - previous_scroll_x).abs() > f32::EPSILON)
    }

    pub(crate) fn finish_horizontal_scrollbar_drag(
        &mut self,
        pane_id: PaneId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.active_scrollbar_drag else {
            return false;
        };
        if drag.pane_id != pane_id {
            return false;
        }
        self.active_scrollbar_drag = None;
        self.finish_scrollbar_drag_for_content_width(drag.pane_id, drag.content_width, cx);
        true
    }

    pub(crate) fn clear_horizontal_scrollbar_drag_for_pane(&mut self, pane_id: PaneId) {
        if self
            .active_scrollbar_drag
            .is_some_and(|drag| drag.pane_id == pane_id)
        {
            self.active_scrollbar_drag = None;
        }
        self.horizontal_scrollbar_tracks.remove(&pane_id);
    }
}

fn scroll_bar_handle_canvas(
    pane_id: PaneId,
    content_width: f32,
    scroll_x: f32,
    cx: &mut Context<FikaApp>,
) -> impl IntoElement {
    let app = cx.weak_entity();
    let app_for_prepaint = app.clone();
    canvas(
        move |bounds, _window, cx| {
            let _ = app_for_prepaint.update(cx, |this, _cx| {
                this.set_horizontal_scrollbar_track(
                    pane_id,
                    bounds.origin.x.as_f32(),
                    bounds.size.width.as_f32(),
                );
            });
            scroll_bar_layout_for_bounds(content_width, scroll_x, bounds)
        },
        move |bounds, bar, window, _cx| {
            let Some(bar) = bar else {
                return;
            };
            let track_origin_x = bounds.origin.x;
            let track_width = normalize_viewport_extent(bounds.size.width.as_f32());
            let app_for_down = app.clone();
            window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, _window, cx| {
                if !phase.capture()
                    || event.button != MouseButton::Left
                    || !bounds.contains(&event.position)
                {
                    return;
                }
                let track_x = (event.position.x - track_origin_x).as_f32();
                let _ = app_for_down.update(cx, |this, cx| {
                    if this.begin_horizontal_scrollbar_drag(
                        pane_id,
                        content_width,
                        scroll_x,
                        track_x,
                        track_width,
                    ) {
                        cx.notify();
                    }
                });
                cx.stop_propagation();
            });

            let app_for_move = app.clone();
            window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                if !phase.capture() {
                    return;
                }
                let track_x = (event.position.x - track_origin_x).as_f32();
                let handled = app_for_move
                    .update(cx, |this, cx| {
                        let active = this
                            .active_scrollbar_drag
                            .is_some_and(|drag| drag.pane_id == pane_id);
                        if active
                            && this.update_horizontal_scrollbar_drag(pane_id, track_x, track_width)
                        {
                            cx.notify();
                        }
                        active
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            });

            let app_for_up = app.clone();
            window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _window, cx| {
                if !phase.capture() || event.button != MouseButton::Left {
                    return;
                }
                let handled = app_for_up
                    .update(cx, |this, cx| {
                        if this.finish_horizontal_scrollbar_drag(pane_id, cx) {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            });
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
    mouse_overlay_active: bool,
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
                .border_1()
                .border_color(item_tile_border_color(selected, drop_target))
                .bg(item_tile_background(selected, drop_target))
                .occlude()
                .when(!mouse_overlay_active, |tile| {
                    tile.hover(move |tile| {
                        tile.bg(item_tile_hover_background(selected, drop_target))
                    })
                })
                .cursor_pointer()
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
                            let changed = contains
                                && this.set_item_drag_drop_target_for_directory(
                                    pane_id,
                                    target_dir_for_move.clone(),
                                    mode,
                                );
                            if contains {
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
                            let changed = contains
                                && this.set_item_drag_drop_target_for_directory(
                                    pane_id,
                                    target_dir_for_external_move.clone(),
                                    mode,
                                );
                            if contains {
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
                    &display_name,
                    &item.kind_label,
                    item.layout,
                    renaming,
                    selected,
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

fn item_tile_border_color(selected: bool, drop_target: Option<FileTransferMode>) -> Rgba {
    if let Some(mode) = drop_target {
        drop_target_border_color(mode)
    } else if selected {
        rgb(0xbfdbfe)
    } else {
        rgba(0x00000000)
    }
}

fn item_tile_hover_background(selected: bool, drop_target: Option<FileTransferMode>) -> Rgba {
    if let Some(mode) = drop_target {
        drop_target_item_hover_background(mode)
    } else if selected {
        rgb(0xdbeafe)
    } else {
        rgb(0xf1f5f9)
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
        FileTransferMode::Copy => rgba(0x16a34a34),
        FileTransferMode::Move => rgba(0xd9770634),
        FileTransferMode::Link => rgba(0x7c3aed34),
    }
}

fn drop_target_item_hover_background(mode: FileTransferMode) -> Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a4a),
        FileTransferMode::Move => rgba(0xd977064a),
        FileTransferMode::Link => rgba(0x7c3aed4a),
    }
}

fn drop_target_border_color(mode: FileTransferMode) -> Rgba {
    match mode {
        FileTransferMode::Copy => rgb(0x16a34a),
        FileTransferMode::Move => rgb(0xd97706),
        FileTransferMode::Link => rgb(0x7c3aed),
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
    use super::{
        FileGridMode, drag_preview_label, horizontal_wheel_scroll_delta,
        item_mouse_down_opens_directory, rubber_band_viewport_rect, scroll_x_for_scrollbar_drag,
    };
    use fika_core::{CompactLayout, CompactLayoutOptions, ViewRect};
    use gpui::{ScrollDelta, point, px};

    #[test]
    fn drag_preview_uses_selection_count_only_for_selected_items() {
        assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
        assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
        assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
    }

    #[test]
    fn wheel_lines_scroll_by_column_scaled_steps() {
        let layout = CompactLayout::new(
            120,
            CompactLayoutOptions {
                viewport_width: 240.0,
                viewport_height: 240.0,
                ..CompactLayoutOptions::default()
            },
        );

        let line_delta = horizontal_wheel_scroll_delta(
            ScrollDelta::Lines(point(0.0, -3.0)),
            px(16.0),
            &layout,
            240.0,
        );
        let precise_delta = horizontal_wheel_scroll_delta(
            ScrollDelta::Pixels(point(px(0.0), px(-32.0))),
            px(16.0),
            &layout,
            240.0,
        );

        assert!(
            line_delta > 140.0,
            "line wheel events should move by compact-view columns, not text lines"
        );
        assert_eq!(precise_delta, 32.0);
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

    #[test]
    fn scrollbar_drag_preserves_initial_handle_grab_offset() {
        let content_width = 1200.0;
        let track_width = 180.0;
        let initial_scroll_x = 240.0;
        let start_track_x = 60.0;

        let (same_scroll_x, max_scroll_x) = scroll_x_for_scrollbar_drag(
            content_width,
            initial_scroll_x,
            start_track_x,
            start_track_x,
            track_width,
        )
        .unwrap();
        let (moved_scroll_x, moved_max_scroll_x) = scroll_x_for_scrollbar_drag(
            content_width,
            initial_scroll_x,
            start_track_x,
            start_track_x + 24.0,
            track_width,
        )
        .unwrap();

        assert_eq!(max_scroll_x, moved_max_scroll_x);
        assert!(
            (same_scroll_x - initial_scroll_x).abs() <= 0.5,
            "drag start should not re-center the handle under the cursor"
        );
        assert!(moved_scroll_x > initial_scroll_x + 100.0);
    }

    #[test]
    fn rubber_band_overlay_uses_viewport_coordinates_after_scroll() {
        let rect = rubber_band_viewport_rect(
            ViewRect {
                x: 320.0,
                y: 24.0,
                width: 80.0,
                height: 48.0,
            },
            128.4,
            2.0,
        );

        assert_eq!(
            rect,
            ViewRect {
                x: 192.0,
                y: 22.0,
                width: 80.0,
                height: 48.0,
            }
        );
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

fn rubber_band_overlay(rect: ViewRect, scroll_x: f32, scroll_y: f32) -> Stateful<Div> {
    let rect = rubber_band_viewport_rect(rect, scroll_x, scroll_y);
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

fn rubber_band_viewport_rect(rect: ViewRect, scroll_x: f32, scroll_y: f32) -> ViewRect {
    ViewRect {
        x: rect.x - scroll_x.round(),
        y: rect.y - scroll_y.round(),
        width: rect.width,
        height: rect.height,
    }
}
