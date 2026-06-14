mod identity;
mod matching;
mod search;

use crate::FikaApp;
use fika_core::{MimeApplication, PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    Bounds, Context, Div, Hitbox, HitboxBehavior, MouseButton, ParentElement, Pixels, Stateful,
    Styled, UniformListScrollHandle, canvas, div, fill, point, px, rgb, rgba, size, uniform_list,
};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use super::icons::{FileIconCache, FileIconSnapshot, cached_icon_or_fallback};
use identity::{application_marker, sanitize_element_id};
pub(crate) use matching::{
    application_chooser_filtered_applications, dedup_application_chooser_applications,
};
use search::{
    application_chooser_search_caret_for_local_x, application_chooser_search_clamped_caret,
    application_chooser_search_next_boundary, application_chooser_search_parts,
    application_chooser_search_previous_boundary,
};

const APPLICATION_CHOOSER_ROW_HEIGHT: f32 = 44.0;
const APPLICATION_CHOOSER_LIST_MAX_HEIGHT: f32 = 360.0;
const APPLICATION_CHOOSER_SCROLLBAR_WIDTH: f32 = 12.0;
const APPLICATION_CHOOSER_SCROLLBAR_THUMB_WIDTH: f32 = 6.0;
const APPLICATION_CHOOSER_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 36.0;

#[derive(Clone, Debug)]
pub(crate) struct ApplicationChooserState {
    pub(crate) pane_id: PaneId,
    pub(crate) path: PathBuf,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) applications: Vec<MimeApplication>,
    pub(crate) query: String,
    pub(crate) query_caret: usize,
    pub(crate) query_text_rect: Option<ViewRect>,
    pub(crate) scroll_handle: UniformListScrollHandle,
    pub(crate) scrollbar_drag_grab_y: Option<f32>,
    pub(crate) set_default_on_choose: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ApplicationChooserScrollbarMetrics {
    pub(crate) handle_top: f32,
    pub(crate) handle_height: f32,
    pub(crate) max_scroll_y: f32,
    pub(crate) track_height: f32,
}

impl ApplicationChooserState {
    pub(crate) fn set_query_text_rect(&mut self, rect: ViewRect) -> bool {
        if self.query_text_rect == Some(rect) {
            return false;
        }
        self.query_text_rect = Some(rect);
        true
    }

    pub(crate) fn move_query_caret_to_window_x(&mut self, window_x: f32) -> bool {
        let Some(rect) = self.query_text_rect else {
            return self.move_query_caret_to_end();
        };
        let caret = application_chooser_search_caret_for_local_x(&self.query, window_x - rect.x);
        self.set_query_caret(caret)
    }

    pub(crate) fn clear_query(&mut self) -> bool {
        if self.query.is_empty() && self.query_caret == 0 {
            return false;
        }
        self.query.clear();
        self.query_caret = 0;
        true
    }

    pub(crate) fn insert_query_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.query_caret = application_chooser_search_clamped_caret(&self.query, self.query_caret);
        self.query.insert_str(self.query_caret, text);
        self.query_caret += text.len();
        true
    }

    pub(crate) fn backspace_query(&mut self) -> bool {
        let caret = application_chooser_search_clamped_caret(&self.query, self.query_caret);
        if caret == 0 {
            self.query_caret = 0;
            return false;
        }
        let previous = application_chooser_search_previous_boundary(&self.query, caret);
        self.query.replace_range(previous..caret, "");
        self.query_caret = previous;
        true
    }

    pub(crate) fn delete_query_forward(&mut self) -> bool {
        let caret = application_chooser_search_clamped_caret(&self.query, self.query_caret);
        if caret >= self.query.len() {
            self.query_caret = self.query.len();
            return false;
        }
        let next = application_chooser_search_next_boundary(&self.query, caret);
        self.query.replace_range(caret..next, "");
        self.query_caret = caret;
        true
    }

    pub(crate) fn move_query_caret_to_start(&mut self) -> bool {
        self.set_query_caret(0)
    }

    pub(crate) fn move_query_caret_to_end(&mut self) -> bool {
        self.set_query_caret(self.query.len())
    }

    pub(crate) fn move_query_caret_backward(&mut self) -> bool {
        let caret = application_chooser_search_clamped_caret(&self.query, self.query_caret);
        self.set_query_caret(application_chooser_search_previous_boundary(
            &self.query,
            caret,
        ))
    }

    pub(crate) fn move_query_caret_forward(&mut self) -> bool {
        let caret = application_chooser_search_clamped_caret(&self.query, self.query_caret);
        self.set_query_caret(application_chooser_search_next_boundary(&self.query, caret))
    }

    fn set_query_caret(&mut self, caret: usize) -> bool {
        let caret = application_chooser_search_clamped_caret(&self.query, caret);
        if self.query_caret == caret {
            return false;
        }
        self.query_caret = caret;
        true
    }

    pub(crate) fn begin_scrollbar_drag(
        &mut self,
        local_y: f32,
        application_count: usize,
        list_height: f32,
    ) -> bool {
        let Some(metrics) = application_chooser_scrollbar_metrics(
            application_count,
            list_height,
            &self.scroll_handle,
        ) else {
            return false;
        };
        let grab_y = if local_y >= metrics.handle_top
            && local_y <= metrics.handle_top + metrics.handle_height
        {
            local_y - metrics.handle_top
        } else {
            metrics.handle_height / 2.0
        };
        self.scrollbar_drag_grab_y = Some(grab_y);
        self.scrollbar_drag_to(local_y, application_count, list_height)
    }

    pub(crate) fn update_scrollbar_drag(
        &mut self,
        local_y: f32,
        application_count: usize,
        list_height: f32,
    ) -> bool {
        if self.scrollbar_drag_grab_y.is_none() {
            return false;
        }
        self.scrollbar_drag_to(local_y, application_count, list_height)
    }

    pub(crate) fn finish_scrollbar_drag(&mut self) -> bool {
        self.scrollbar_drag_grab_y.take().is_some()
    }

    fn scrollbar_drag_to(
        &mut self,
        local_y: f32,
        application_count: usize,
        list_height: f32,
    ) -> bool {
        let Some(metrics) = application_chooser_scrollbar_metrics(
            application_count,
            list_height,
            &self.scroll_handle,
        ) else {
            return false;
        };
        let grab_y = self
            .scrollbar_drag_grab_y
            .unwrap_or(metrics.handle_height / 2.0);
        let scroll_y = application_chooser_scroll_y_for_local_y(local_y, grab_y, metrics);
        set_application_chooser_scroll_y(&self.scroll_handle, scroll_y);
        true
    }
}

fn application_chooser_icon_snapshot(
    cache: &mut FileIconCache,
    app: &MimeApplication,
) -> FileIconSnapshot {
    let mut candidates = Vec::new();
    if let Some(icon) = app
        .icon
        .as_ref()
        .map(|icon| icon.trim())
        .filter(|icon| !icon.is_empty())
    {
        candidates.push(icon.to_string());
    }
    candidates.extend([
        "application-x-executable".to_string(),
        "system-run".to_string(),
        "application-default-icon".to_string(),
    ]);

    let cache_name = format!("application-chooser-{}", sanitize_element_id(&app.id));
    let marker = application_marker(&app.name);
    let candidate_refs = candidates.iter().map(String::as_str).collect::<Vec<_>>();
    cache.named_icon(
        &cache_name,
        &candidate_refs,
        &marker,
        0x2f6fed,
        0xe8eef7,
        28.0,
    )
}

pub(crate) fn application_chooser_visible_range(total: usize, range: Range<usize>) -> Range<usize> {
    let start = range.start.min(total);
    let end = range.end.min(total).max(start);
    start..end
}

pub(crate) fn application_chooser_visible_icon_snapshots(
    cache: &mut FileIconCache,
    applications: &[MimeApplication],
    range: Range<usize>,
) -> HashMap<usize, FileIconSnapshot> {
    let visible_range = application_chooser_visible_range(applications.len(), range);
    let mut snapshots =
        HashMap::with_capacity(visible_range.end.saturating_sub(visible_range.start));
    for index in visible_range {
        if let Some(app) = applications.get(index) {
            snapshots.insert(index, application_chooser_icon_snapshot(cache, app));
        }
    }
    snapshots
}

pub(crate) fn application_chooser_list_height(application_count: usize) -> f32 {
    (application_count as f32 * APPLICATION_CHOOSER_ROW_HEIGHT)
        .min(APPLICATION_CHOOSER_LIST_MAX_HEIGHT)
        .max(APPLICATION_CHOOSER_ROW_HEIGHT)
}

pub(crate) fn application_chooser_overlay(
    chooser: ApplicationChooserState,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let title = chooser
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("Open With - {name}"))
        .unwrap_or_else(|| "Open With".to_string());
    let detail = chooser
        .mime_type
        .as_deref()
        .map(|mime| format!("{} - {}", chooser.path.display(), mime))
        .unwrap_or_else(|| chooser.path.display().to_string());
    let can_set_default = chooser.mime_type.is_some();
    let set_default_on_choose = chooser.set_default_on_choose;
    let query = chooser.query.clone();
    let query_caret = chooser.query_caret;
    let applications = Arc::new(application_chooser_filtered_applications(
        &chooser.applications,
        &query,
    ));
    let application_count = applications.len();
    let list_height = application_chooser_list_height(application_count);
    let scroll_handle = chooser.scroll_handle.clone();

    div()
        .id("application-chooser-layer")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .bg(rgba(0x00000066))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                this.dismiss_application_chooser();
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_move(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_scroll_wheel(|_event, _window, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .id("application-chooser-dialog")
                .w(px(520.0))
                .max_h(px(560.0))
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xc8ced6))
                .bg(rgb(0xffffff))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_scroll_wheel(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(0xd5d9df))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .truncate()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(0x1f2328))
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .text_xs()
                                        .text_color(rgb(0x59636e))
                                        .child(detail),
                                ),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(0x59636e))
                                .hover(|button| button.bg(rgb(0xeaf1ff)))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        |this, _event: &gpui::MouseDownEvent, _window, cx| {
                                            this.dismiss_application_chooser();
                                            cx.notify();
                                        },
                                    ),
                                )
                                .child("Close"),
                        ),
                )
                .child(application_chooser_search_box(&query, query_caret, cx))
                .child(div().h(px(list_height)).overflow_hidden().map(|list| {
                    if application_count == 0 {
                        return list
                            .child(application_chooser_empty_state())
                            .into_any_element();
                    }

                    list.child(
                        uniform_list("application-chooser-list", application_count, {
                            let applications = applications.clone();
                            cx.processor(move |this, range: Range<usize>, _window, cx| {
                                let visible_range = application_chooser_visible_range(
                                    applications.len(),
                                    range.clone(),
                                );
                                let icons = application_chooser_visible_icon_snapshots(
                                    &mut this.file_icons,
                                    applications.as_slice(),
                                    range,
                                );
                                visible_range
                                    .filter_map(|index| {
                                        let app = applications.get(index)?.clone();
                                        let icon = icons.get(&index).cloned();
                                        Some(application_chooser_row(
                                            app,
                                            icon,
                                            can_set_default,
                                            cx,
                                        ))
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .size_full()
                        .track_scroll(&scroll_handle),
                    )
                    .relative()
                    .child(application_chooser_scrollbar(
                        application_count,
                        list_height,
                        &scroll_handle,
                        cx,
                    ))
                    .into_any_element()
                }))
                .when(can_set_default, |dialog| {
                    dialog.child(application_chooser_default_footer(
                        set_default_on_choose,
                        cx,
                    ))
                }),
        )
}

fn application_chooser_default_footer(enabled: bool, cx: &mut Context<FikaApp>) -> Div {
    div()
        .px_4()
        .py_3()
        .border_t_1()
        .border_color(rgb(0xe1e5ea))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                        this.toggle_application_chooser_set_default();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(16.0))
                        .h(px(16.0))
                        .rounded_sm()
                        .border_1()
                        .border_color(if enabled {
                            rgb(0x2f6fed)
                        } else {
                            rgb(0x9aa4b2)
                        })
                        .bg(if enabled {
                            rgb(0x2f6fed)
                        } else {
                            rgb(0xffffff)
                        })
                        .when(enabled, |mark| {
                            mark.child(
                                div()
                                    .m_auto()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded_sm()
                                    .bg(rgb(0xffffff)),
                            )
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x1f2328))
                        .child("Set as default for this file type"),
                ),
        )
}

fn application_chooser_search_box(
    query: &str,
    query_caret: usize,
    cx: &mut Context<FikaApp>,
) -> Div {
    let has_query = !query.is_empty();
    let (prefix, suffix) = application_chooser_search_parts(query, query_caret);
    let app = cx.weak_entity();
    let app_for_click = cx.weak_entity();
    let caret = || div().w(px(1.0)).h(px(18.0)).flex_none().bg(rgb(0x2f6fed));
    div()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(0xe1e5ea))
        .child(
            div()
                .on_children_prepainted(move |bounds, _window, cx| {
                    let Some(bounds) = bounds.first() else {
                        return;
                    };
                    let rect = view_rect_from_bounds(*bounds);
                    let _ = app.update(cx, |this, _cx| {
                        if let Some(chooser) = &mut this.application_chooser {
                            chooser.set_query_text_rect(rect);
                        }
                    });
                })
                .id("application-chooser-search")
                .flex()
                .items_center()
                .h(px(30.0))
                .px_2()
                .border_1()
                .rounded_md()
                .border_color(rgb(0x2f6fed))
                .bg(rgb(0xffffff))
                .overflow_hidden()
                .cursor_text()
                .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                    let _ = app_for_click.update(cx, |this, cx| {
                        if let Some(chooser) = &mut this.application_chooser
                            && chooser.move_query_caret_to_window_x(event.position.x.as_f32())
                        {
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                })
                .text_sm()
                .text_color(rgb(0x111827))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .when(has_query, |row| {
                            row.child(
                                div()
                                    .flex_none()
                                    .max_w_full()
                                    .overflow_hidden()
                                    .truncate()
                                    .text_color(rgb(0x111827))
                                    .child(prefix.to_string()),
                            )
                            .child(caret())
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(rgb(0x111827))
                                    .child(suffix.to_string()),
                            )
                        })
                        .when(!has_query, |row| {
                            row.child(caret()).child(
                                div()
                                    .ml_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(rgb(0x6b7280))
                                    .child("Search applications"),
                            )
                        }),
                ),
        )
}

fn view_rect_from_bounds(bounds: Bounds<Pixels>) -> ViewRect {
    ViewRect {
        x: bounds.origin.x.as_f32(),
        y: bounds.origin.y.as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
    }
}

fn application_chooser_empty_state() -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .text_sm()
        .text_color(rgb(0x59636e))
        .child("No matching applications")
}

fn application_chooser_scrollbar(
    application_count: usize,
    list_height: f32,
    scroll_handle: &UniformListScrollHandle,
    cx: &mut Context<FikaApp>,
) -> gpui::AnyElement {
    if application_chooser_scrollbar_metrics(application_count, list_height, scroll_handle)
        .is_none()
    {
        return div().into_any_element();
    }

    let app = cx.weak_entity();
    let scroll_handle = scroll_handle.clone();
    div()
        .absolute()
        .top_0()
        .right_0()
        .w(px(APPLICATION_CHOOSER_SCROLLBAR_WIDTH))
        .h(px(list_height))
        .occlude()
        .cursor_pointer()
        .child(
            canvas(
                {
                    let scroll_handle = scroll_handle.clone();
                    move |bounds, window, _cx| ApplicationChooserScrollbarPaintState {
                        metrics: application_chooser_scrollbar_metrics(
                            application_count,
                            list_height,
                            &scroll_handle,
                        ),
                        hitbox: window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
                    }
                },
                move |bounds, state, window, _cx| {
                    let track_bounds = bounds;
                    if let Some(metrics) = state.metrics {
                        paint_application_chooser_scrollbar(track_bounds, metrics, window);
                    }

                    let hitbox_for_down = state.hitbox.clone();
                    let app_for_down = app.clone();
                    window.on_mouse_event(
                        move |event: &gpui::MouseDownEvent, phase, _window, cx| {
                            if !phase.capture() || event.button != MouseButton::Left {
                                return;
                            }
                            let local_y = (event.position.y - track_bounds.origin.y).as_f32();
                            if !(0.0..=list_height).contains(&local_y) {
                                return;
                            }
                            let handled = app_for_down
                                .update(cx, |this, cx| {
                                    let handled =
                                        this.application_chooser.as_mut().is_some_and(|chooser| {
                                            chooser.begin_scrollbar_drag(
                                                local_y,
                                                application_count,
                                                list_height,
                                            )
                                        });
                                    if handled {
                                        cx.notify();
                                    }
                                    handled
                                })
                                .unwrap_or(false);
                            if handled {
                                _window.capture_pointer(hitbox_for_down.id);
                                cx.stop_propagation();
                            }
                        },
                    );

                    let hitbox_for_move = state.hitbox.clone();
                    let app_for_move = app.clone();
                    window.on_mouse_event(
                        move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                            if !phase.capture() || !event.dragging() {
                                return;
                            }
                            let local_y = (event.position.y - track_bounds.origin.y).as_f32();
                            let handled = app_for_move
                                .update(cx, |this, cx| {
                                    let Some(chooser) = this.application_chooser.as_mut() else {
                                        return false;
                                    };
                                    if chooser.scrollbar_drag_grab_y.is_none() {
                                        return false;
                                    }
                                    let handled = chooser.update_scrollbar_drag(
                                        local_y,
                                        application_count,
                                        list_height,
                                    );
                                    _window.capture_pointer(hitbox_for_move.id);
                                    if handled {
                                        cx.notify();
                                    }
                                    true
                                })
                                .unwrap_or(false);
                            if handled {
                                cx.stop_propagation();
                            }
                        },
                    );

                    let app_for_up = app.clone();
                    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _window, cx| {
                        if !phase.capture() || event.button != MouseButton::Left {
                            return;
                        }
                        let handled = app_for_up
                            .update(cx, |this, cx| {
                                let handled = this
                                    .application_chooser
                                    .as_mut()
                                    .is_some_and(ApplicationChooserState::finish_scrollbar_drag);
                                if handled {
                                    cx.notify();
                                }
                                handled
                            })
                            .unwrap_or(false);
                        if handled {
                            _window.release_pointer();
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .size_full(),
        )
        .into_any_element()
}

struct ApplicationChooserScrollbarPaintState {
    metrics: Option<ApplicationChooserScrollbarMetrics>,
    hitbox: Hitbox,
}

pub(crate) fn application_chooser_scrollbar_metrics(
    application_count: usize,
    list_height: f32,
    scroll_handle: &UniformListScrollHandle,
) -> Option<ApplicationChooserScrollbarMetrics> {
    let content_height = application_count as f32 * APPLICATION_CHOOSER_ROW_HEIGHT;
    if content_height <= list_height {
        return None;
    }

    let base_handle = scroll_handle.0.borrow().base_handle.clone();
    let computed_max_scroll_y = (content_height - list_height).max(0.0);
    let max_offset_y = base_handle
        .max_offset()
        .y
        .as_f32()
        .max(computed_max_scroll_y);
    let scroll_y = (-base_handle.offset().y.as_f32()).clamp(0.0, max_offset_y);
    let handle_height = (list_height * (list_height / content_height))
        .clamp(APPLICATION_CHOOSER_SCROLLBAR_MIN_THUMB_HEIGHT, list_height)
        .floor();
    let available = (list_height - handle_height).max(0.0);
    let handle_top = if max_offset_y > 0.0 {
        (scroll_y / max_offset_y).clamp(0.0, 1.0) * available
    } else {
        0.0
    };

    Some(ApplicationChooserScrollbarMetrics {
        handle_top,
        handle_height,
        max_scroll_y: max_offset_y,
        track_height: list_height,
    })
}

pub(crate) fn application_chooser_scroll_y_for_local_y(
    local_y: f32,
    grab_y: f32,
    metrics: ApplicationChooserScrollbarMetrics,
) -> f32 {
    let available = (metrics.track_height - metrics.handle_height).max(0.0);
    if available <= 0.0 || metrics.max_scroll_y <= 0.0 {
        return 0.0;
    }
    let handle_top = (local_y - grab_y).clamp(0.0, available);
    (handle_top / available * metrics.max_scroll_y).clamp(0.0, metrics.max_scroll_y)
}

fn set_application_chooser_scroll_y(scroll_handle: &UniformListScrollHandle, scroll_y: f32) {
    let base_handle = scroll_handle.0.borrow().base_handle.clone();
    let current = base_handle.offset();
    base_handle.set_offset(point(current.x, px(-scroll_y.max(0.0))));
}

fn paint_application_chooser_scrollbar(
    bounds: Bounds<Pixels>,
    metrics: ApplicationChooserScrollbarMetrics,
    window: &mut gpui::Window,
) {
    let track_x = bounds.origin.x
        + px(
            (APPLICATION_CHOOSER_SCROLLBAR_WIDTH - APPLICATION_CHOOSER_SCROLLBAR_THUMB_WIDTH) / 2.0,
        );
    window.paint_quad(fill(
        Bounds::new(
            point(track_x, bounds.origin.y + px(metrics.handle_top)),
            size(
                px(APPLICATION_CHOOSER_SCROLLBAR_THUMB_WIDTH),
                px(metrics.handle_height),
            ),
        ),
        rgba(0x5b6470a8),
    ));
}

fn application_chooser_row(
    app: MimeApplication,
    icon: Option<FileIconSnapshot>,
    can_set_default: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let desktop_id = app.id.clone();
    let is_default = app.is_default;
    div()
        .id(format!(
            "application-choice-{}",
            sanitize_element_id(&app.id)
        ))
        .flex()
        .items_center()
        .gap_3()
        .h(px(APPLICATION_CHOOSER_ROW_HEIGHT))
        .px_4()
        .min_w_0()
        .hover(|row| row.bg(rgb(0xeaf1ff)))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.choose_application_for_open_with(desktop_id.clone(), cx);
            cx.notify();
        }))
        .child(application_chooser_icon_slot(&app.name, icon))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .text_color(rgb(0x1f2328))
                        .child(app.name),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(rgb(0x59636e))
                        .child(app.desktop_file.display().to_string()),
                ),
        )
        .when(can_set_default && is_default, |row| {
            row.child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .text_color(rgb(0x047857))
                    .bg(rgb(0xe7f8ef))
                    .child("Default"),
            )
        })
}

fn application_chooser_icon_slot(app_name: &str, icon: Option<FileIconSnapshot>) -> Div {
    let snapshot = icon.unwrap_or_else(|| FileIconSnapshot {
        icon_name: Arc::from("application-x-executable"),
        path: None,
        fallback_marker: Arc::from(application_marker(app_name)),
        fallback_fg: 0x2f6fed,
        fallback_bg: 0xe8eef7,
    });
    let fallback = snapshot.fallback_marker.clone();
    let fallback_fg = snapshot.fallback_fg;
    let fallback_bg = snapshot.fallback_bg;
    let container = div()
        .w(px(28.0))
        .h(px(28.0))
        .rounded_md()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden();

    container.child(cached_icon_or_fallback(&snapshot, move || {
        application_chooser_fallback_icon(fallback.clone(), fallback_fg, fallback_bg)
    }))
}

fn application_chooser_fallback_icon(marker: Arc<str>, fg: u32, bg: u32) -> gpui::AnyElement {
    div()
        .size_full()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg))
        .text_sm()
        .text_color(rgb(fg))
        .child(marker.as_ref().to_string())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_height_cap_leaves_room_for_dialog_chrome() {
        assert_eq!(application_chooser_list_height(0), 44.0);
        assert_eq!(application_chooser_list_height(3), 132.0);
        assert_eq!(application_chooser_list_height(100), 360.0);
    }

    #[test]
    fn scrollbar_drag_maps_local_position_to_scroll_offset() {
        let scroll_handle = UniformListScrollHandle::new();
        let metrics = application_chooser_scrollbar_metrics(100, 360.0, &scroll_handle).unwrap();

        assert_eq!(metrics.track_height, 360.0);
        assert_eq!(metrics.handle_top, 0.0);
        assert!(metrics.handle_height >= APPLICATION_CHOOSER_SCROLLBAR_MIN_THUMB_HEIGHT);

        let middle_scroll = application_chooser_scroll_y_for_local_y(
            metrics.track_height / 2.0,
            metrics.handle_height / 2.0,
            metrics,
        );
        assert!(middle_scroll > 0.0);
        assert!(middle_scroll < metrics.max_scroll_y);

        set_application_chooser_scroll_y(&scroll_handle, middle_scroll);
        assert_eq!(
            scroll_handle.0.borrow().base_handle.offset().y.as_f32(),
            -middle_scroll
        );
    }

    #[test]
    fn scrollbar_drag_state_survives_stationary_move_until_mouse_up() {
        let mut chooser = ApplicationChooserState {
            pane_id: PaneId(1),
            path: PathBuf::from("/tmp/example.txt"),
            mime_type: None,
            applications: Vec::new(),
            query: String::new(),
            query_caret: 0,
            query_text_rect: None,
            scroll_handle: UniformListScrollHandle::new(),
            scrollbar_drag_grab_y: None,
            set_default_on_choose: false,
        };

        assert!(chooser.begin_scrollbar_drag(0.0, 100, 360.0));
        assert!(chooser.scrollbar_drag_grab_y.is_some());
        assert!(chooser.update_scrollbar_drag(0.0, 100, 360.0));
        assert!(chooser.scrollbar_drag_grab_y.is_some());
        assert!(chooser.finish_scrollbar_drag());
        assert!(chooser.scrollbar_drag_grab_y.is_none());
    }

    #[test]
    fn search_query_edits_at_caret_and_preserves_utf8_boundaries() {
        let mut chooser = ApplicationChooserState {
            pane_id: PaneId(1),
            path: PathBuf::from("/tmp/example.txt"),
            mime_type: None,
            applications: Vec::new(),
            query: String::new(),
            query_caret: 0,
            query_text_rect: None,
            scroll_handle: UniformListScrollHandle::new(),
            scrollbar_drag_grab_y: None,
            set_default_on_choose: false,
        };

        assert!(chooser.insert_query_text("kde"));
        assert_eq!(chooser.query, "kde");
        assert_eq!(chooser.query_caret, "kde".len());
        assert!(chooser.move_query_caret_backward());
        assert!(chooser.insert_query_text("\u{76ee}"));
        assert_eq!(chooser.query, "kd\u{76ee}e");
        assert!(chooser.backspace_query());
        assert_eq!(chooser.query, "kde");
        assert!(chooser.move_query_caret_to_start());
        assert!(chooser.delete_query_forward());
        assert_eq!(chooser.query, "de");
        assert_eq!(chooser.query_caret, 0);
    }

    #[test]
    fn search_click_caret_uses_cached_text_rect() {
        let mut chooser = ApplicationChooserState {
            pane_id: PaneId(1),
            path: PathBuf::from("/tmp/example.txt"),
            mime_type: None,
            applications: Vec::new(),
            query: "writer".to_string(),
            query_caret: 0,
            query_text_rect: None,
            scroll_handle: UniformListScrollHandle::new(),
            scrollbar_drag_grab_y: None,
            set_default_on_choose: false,
        };

        chooser.set_query_text_rect(ViewRect {
            x: 200.0,
            y: 40.0,
            width: 300.0,
            height: 24.0,
        });
        assert!(chooser.move_query_caret_to_window_x(222.0));
        assert!(chooser.query_caret > 0);
        assert!(chooser.query_caret < chooser.query.len());
        assert!(chooser.query.is_char_boundary(chooser.query_caret));
    }
}
