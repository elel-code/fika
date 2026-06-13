mod identity;
mod matching;

use crate::FikaApp;
use fika_core::{MimeApplication, PaneId};
use gpui::prelude::*;
use gpui::{
    Context, Div, MouseButton, ParentElement, Stateful, Styled, UniformListScrollHandle, div, img,
    px, rgb, rgba, uniform_list,
};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use super::icons::{FileIconCache, FileIconSnapshot};
use identity::{application_marker, sanitize_element_id};
pub(crate) use matching::{
    application_chooser_filtered_applications, dedup_application_chooser_applications,
};

const APPLICATION_CHOOSER_ROW_HEIGHT: f32 = 44.0;
const APPLICATION_CHOOSER_LIST_MAX_HEIGHT: f32 = 480.0;

#[derive(Clone, Debug)]
pub(crate) struct ApplicationChooserState {
    pub(crate) pane_id: PaneId,
    pub(crate) path: PathBuf,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) applications: Vec<MimeApplication>,
    pub(crate) query: String,
    pub(crate) scroll_handle: UniformListScrollHandle,
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
    let query = chooser.query.clone();
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
                .child(application_chooser_search_box(&query))
                .child(div().h(px(list_height)).overflow_y_hidden().map(|list| {
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
                    ))
                    .into_any_element()
                })),
        )
}

fn application_chooser_search_box(query: &str) -> Div {
    let has_query = !query.is_empty();
    div()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(0xe1e5ea))
        .child(
            div()
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
                .text_sm()
                .text_color(rgb(0x111827))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if has_query {
                            rgb(0x111827)
                        } else {
                            rgb(0x6b7280)
                        })
                        .child(if has_query {
                            query.to_string()
                        } else {
                            "Search applications".to_string()
                        }),
                )
                .when(has_query, |field| {
                    field.child(div().w(px(1.0)).h(px(18.0)).bg(rgb(0x2f6fed)))
                }),
        )
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
) -> Div {
    let content_height = application_count as f32 * APPLICATION_CHOOSER_ROW_HEIGHT;
    if content_height <= list_height {
        return div();
    }

    let base_handle = scroll_handle.0.borrow().base_handle.clone();
    let offset_y = base_handle.offset().y.as_f32().max(0.0);
    let max_offset_y = base_handle
        .max_offset()
        .y
        .as_f32()
        .max((content_height - list_height).max(0.0));
    let handle_height = (list_height * (list_height / content_height))
        .clamp(36.0, list_height)
        .floor();
    let available = (list_height - handle_height).max(0.0);
    let handle_top = if max_offset_y > 0.0 {
        (offset_y / max_offset_y).clamp(0.0, 1.0) * available
    } else {
        0.0
    };

    div()
        .absolute()
        .top(px(handle_top))
        .right(px(3.0))
        .w(px(6.0))
        .h(px(handle_height))
        .rounded_md()
        .bg(rgba(0x5b647080))
}

fn application_chooser_row(
    app: MimeApplication,
    icon: Option<FileIconSnapshot>,
    can_set_default: bool,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let desktop_id = app.id.clone();
    let default_desktop_id = app.id.clone();
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
        .when(can_set_default && !is_default, |row| {
            row.child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_xs()
                    .text_color(rgb(0x1f4fbf))
                    .bg(rgb(0xeaf1ff))
                    .hover(|button| button.bg(rgb(0xdbe7fb)))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &gpui::MouseDownEvent, _window, cx| {
                            this.set_default_open_with_application(default_desktop_id.clone());
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child("Set Default"),
            )
        })
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
        icon_name: "application-x-executable".to_string(),
        path: None,
        fallback_marker: application_marker(app_name),
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

    match snapshot.path {
        Some(path) => container.child(img(path).size_full().with_fallback(move || {
            application_chooser_fallback_icon(fallback.clone(), fallback_fg, fallback_bg)
        })),
        None => container.child(application_chooser_fallback_icon(
            fallback,
            fallback_fg,
            fallback_bg,
        )),
    }
}

fn application_chooser_fallback_icon(marker: String, fg: u32, bg: u32) -> gpui::AnyElement {
    div()
        .size_full()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg))
        .text_sm()
        .text_color(rgb(fg))
        .child(marker)
        .into_any_element()
}
