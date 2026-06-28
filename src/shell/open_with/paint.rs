use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::dolphin::style::{
    BREEZE_ITEM_ROUNDNESS, details_row_background_color, item_background_color,
};
use crate::shell::icon_roles::NamedIconFallback;
use crate::shell::metrics::{
    OPEN_WITH_CHOOSER_MAX_ROWS, OPEN_WITH_CHOOSER_ROW_HEIGHT, OPEN_WITH_CHOOSER_TITLE_HEIGHT,
    scaled_dialog_metric,
};
use crate::shell::open_with::ShellOpenWithChooser;
use crate::shell::open_with::geometry::{
    open_with_chooser_cancel_button_rect_scaled, open_with_chooser_default_checkbox_rect_scaled,
    open_with_chooser_list_rect_scaled, open_with_chooser_open_button_rect_scaled,
    open_with_chooser_query_rect_scaled, open_with_chooser_rect_scaled,
    open_with_chooser_scrollbar_rects_scaled,
};
use crate::{
    IconDrawLayer, IconFrameBuilder, LabelAlignment, LabelWrap, POPUP_BACKDROP, POPUP_BORDER,
    POPUP_BUTTON_PRIMARY, POPUP_BUTTON_SECONDARY, POPUP_DIVIDER, POPUP_FIELD_FOCUS, POPUP_HEADER,
    POPUP_INPUT, POPUP_MARKER_NEUTRAL, POPUP_SURFACE, QuadVertex, TextFrameBuilder,
    path_name_or_display, popup_body_text, popup_error_text, popup_inverse_text, popup_muted_text,
    popup_soft_text, popup_title_text, push_clipped_rect_outline, push_clipped_rounded_highlight,
    push_clipped_rounded_rect, push_rect, push_scrollbar,
};

pub(crate) fn push_open_with_chooser_overlay(
    chooser: &ShellOpenWithChooser,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let screen = ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    };
    push_rect(vertices, screen, POPUP_BACKDROP, size);
    let rect = open_with_chooser_rect_scaled(chooser, size, scale);
    let title_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale);
    let margin = scaled_dialog_metric(16.0, scale);
    let row_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale);
    push_clipped_rounded_rect(
        vertices,
        rect,
        screen,
        scaled_dialog_metric(8.0, scale),
        POPUP_SURFACE,
        size,
    );
    push_clipped_rect_outline(vertices, rect, screen, 1.0, POPUP_BORDER, size);
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: title_height,
        },
        POPUP_HEADER,
        size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y + title_height - scaled_dialog_metric(1.0, scale).max(1.0),
            width: rect.width,
            height: scaled_dialog_metric(1.0, scale).max(1.0),
        },
        POPUP_DIVIDER,
        size,
    );
    text.push_label(
        &format!("Open With - {}", path_name_or_display(&chooser.path)),
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(8.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        popup_title_text(),
    );
    text.push_label(
        chooser.mime_type.as_deref().unwrap_or("unknown MIME"),
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(25.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(14.0, scale),
        },
        rect,
        popup_muted_text(),
    );

    let query = open_with_chooser_query_rect_scaled(rect, scale);
    push_clipped_rounded_rect(
        vertices,
        query,
        rect,
        scaled_dialog_metric(7.0, scale),
        POPUP_INPUT,
        size,
    );
    push_clipped_rect_outline(vertices, query, rect, 1.0, POPUP_DIVIDER, size);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: query.x + scaled_dialog_metric(10.0, scale),
            y: query.bottom() - scaled_dialog_metric(2.0, scale),
            width: (query.width - scaled_dialog_metric(20.0, scale)).max(1.0),
            height: scaled_dialog_metric(1.5, scale),
        },
        query,
        scaled_dialog_metric(1.0, scale),
        POPUP_FIELD_FOCUS,
        size,
    );
    let search_icon = ViewRect {
        x: query.x + scaled_dialog_metric(10.0, scale),
        y: query.y + (query.height - scaled_dialog_metric(14.0, scale)) / 2.0,
        width: scaled_dialog_metric(14.0, scale),
        height: scaled_dialog_metric(14.0, scale),
    };
    if !icons.push_named_theme_icon(
        "edit-find",
        NamedIconFallback::Service,
        search_icon,
        query,
        IconDrawLayer::Overlay,
    ) {
        push_open_with_search_icon(vertices, search_icon, query, scale, size);
    }
    let query_text_rect = ViewRect {
        x: search_icon.right() + scaled_dialog_metric(8.0, scale),
        y: query.y + (query.height - scaled_dialog_metric(18.0, scale)) / 2.0,
        width: (query.right() - search_icon.right() - scaled_dialog_metric(18.0, scale)).max(1.0),
        height: scaled_dialog_metric(18.0, scale),
    };
    if chooser.query.is_empty() {
        text.push_label(
            "Search applications",
            query_text_rect,
            query,
            popup_muted_text(),
        );
    } else {
        let cursor_x = text.measure_label_cursor_x(
            &chooser.query,
            query_text_rect,
            chooser.query.len(),
            LabelAlignment::Start,
            LabelWrap::None,
        );
        text.push_label(&chooser.query, query_text_rect, query, popup_body_text());
        let caret_width = scaled_dialog_metric(1.0, scale).max(1.0);
        let caret_height = scaled_dialog_metric(17.0, scale)
            .min(query.height - scaled_dialog_metric(10.0, scale))
            .max(1.0);
        let caret_x = (query_text_rect.x + cursor_x).clamp(
            query_text_rect.x,
            (query_text_rect.right() - caret_width).max(query_text_rect.x),
        );
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: caret_x,
                y: query.y + (query.height - caret_height) / 2.0,
                width: caret_width,
                height: caret_height,
            },
            query,
            caret_width / 2.0,
            POPUP_FIELD_FOCUS,
            size,
        );
    }

    let list = open_with_chooser_list_rect_scaled(rect, chooser, scale);
    let scrollbar = open_with_chooser_scrollbar_rects_scaled(list, chooser, scale);
    let row_content_right = scrollbar
        .as_ref()
        .map(|(track, _)| track.x - scaled_dialog_metric(8.0, scale))
        .unwrap_or_else(|| list.right() - scaled_dialog_metric(10.0, scale));
    push_clipped_rounded_rect(
        vertices,
        list,
        rect,
        scaled_dialog_metric(8.0, scale),
        POPUP_INPUT,
        size,
    );
    push_clipped_rect_outline(vertices, list, rect, 1.0, POPUP_DIVIDER, size);
    let visible = chooser.visible_filtered_indexes();
    if visible.is_empty() {
        text.push_label(
            "No matching applications",
            ViewRect {
                x: list.x + scaled_dialog_metric(12.0, scale),
                y: list.y + scaled_dialog_metric(10.0, scale),
                width: (list.width - scaled_dialog_metric(24.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            list,
            popup_muted_text(),
        );
    } else {
        for (visible_row, app_index) in visible.iter().copied().enumerate() {
            let row = chooser.scroll_row + visible_row;
            let Some(application) = chooser.applications.get(app_index) else {
                continue;
            };
            let row_rect = ViewRect {
                x: list.x + scaled_dialog_metric(4.0, scale),
                y: list.y + visible_row as f32 * row_height + scaled_dialog_metric(3.0, scale),
                width: (row_content_right - list.x - scaled_dialog_metric(8.0, scale)).max(1.0),
                height: (row_height - scaled_dialog_metric(6.0, scale)).max(1.0),
            };
            let row_clip = ViewRect {
                x: list.x,
                y: list.y + visible_row as f32 * row_height,
                width: list.width,
                height: row_height,
            };
            let selected = row == chooser.selected_index;
            let row_radius = scaled_dialog_metric(BREEZE_ITEM_ROUNDNESS, scale);
            let row_background = details_row_background_color(false, false, visible_row % 2 == 1);
            push_clipped_rounded_rect(vertices, row_rect, list, row_radius, row_background, size);
            if selected {
                push_clipped_rounded_highlight(
                    vertices,
                    row_rect,
                    list,
                    row_radius,
                    item_background_color(true, false),
                    POPUP_FIELD_FOCUS,
                    scaled_dialog_metric(1.25, scale),
                    size,
                );
            }
            let icon_rect = ViewRect {
                x: row_rect.x + scaled_dialog_metric(13.0, scale),
                y: row_rect.y + (row_rect.height - scaled_dialog_metric(28.0, scale)) / 2.0,
                width: scaled_dialog_metric(28.0, scale),
                height: scaled_dialog_metric(28.0, scale),
            };
            let icon_pushed = application
                .icon
                .as_deref()
                .filter(|icon| !icon.is_empty())
                .is_some_and(|icon| {
                    icons.push_named_theme_icon(
                        icon,
                        NamedIconFallback::Application,
                        icon_rect,
                        list,
                        IconDrawLayer::Overlay,
                    )
                });
            if !icon_pushed {
                icons.push_named_theme_icon(
                    "application-x-executable",
                    NamedIconFallback::Application,
                    icon_rect,
                    list,
                    IconDrawLayer::Overlay,
                );
            }
            let name = if application.is_default {
                format!("{} (default)", application.name)
            } else {
                application.name.clone()
            };
            text.push_label(
                &name,
                ViewRect {
                    x: icon_rect.right() + scaled_dialog_metric(12.0, scale),
                    y: row_rect.y + scaled_dialog_metric(7.0, scale),
                    width: (row_content_right
                        - icon_rect.right()
                        - scaled_dialog_metric(18.0, scale))
                    .max(1.0),
                    height: scaled_dialog_metric(18.0, scale),
                },
                row_clip,
                if selected {
                    popup_title_text()
                } else {
                    popup_body_text()
                },
            );
            text.push_label(
                &application.id,
                ViewRect {
                    x: icon_rect.right() + scaled_dialog_metric(12.0, scale),
                    y: row_rect.y + scaled_dialog_metric(25.0, scale),
                    width: (row_content_right
                        - icon_rect.right()
                        - scaled_dialog_metric(18.0, scale))
                    .max(1.0),
                    height: scaled_dialog_metric(14.0, scale),
                },
                row_clip,
                if selected {
                    popup_soft_text()
                } else {
                    popup_muted_text()
                },
            );
        }
    }

    if let Some((track, thumb)) = scrollbar {
        push_scrollbar(vertices, track, thumb, list, size);
    }

    let default_row = open_with_chooser_default_checkbox_rect_scaled(rect, chooser, scale);
    let default_enabled = chooser.mime_type.is_some();
    let checkbox_size = scaled_dialog_metric(16.0, scale);
    let checkbox = ViewRect {
        x: default_row.x,
        y: default_row.y + (default_row.height - checkbox_size) / 2.0,
        width: checkbox_size,
        height: checkbox_size,
    };
    push_clipped_rounded_rect(
        vertices,
        checkbox,
        rect,
        scaled_dialog_metric(4.0, scale),
        if chooser.set_as_default {
            POPUP_BUTTON_PRIMARY
        } else {
            POPUP_INPUT
        },
        size,
    );
    push_clipped_rect_outline(
        vertices,
        checkbox,
        rect,
        1.0,
        if default_enabled {
            POPUP_BORDER
        } else {
            POPUP_DIVIDER
        },
        size,
    );
    if chooser.set_as_default {
        push_open_with_checkbox_check(vertices, checkbox, rect, scale, size);
    }
    let default_label = chooser
        .mime_type
        .as_deref()
        .map(|mime| format!("Set as default application for {mime}"))
        .unwrap_or_else(|| "Set as default application".to_string());
    text.push_label(
        &default_label,
        ViewRect {
            x: checkbox.right() + scaled_dialog_metric(8.0, scale),
            y: default_row.y + scaled_dialog_metric(3.0, scale),
            width: (default_row.right() - checkbox.right() - scaled_dialog_metric(8.0, scale))
                .max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        if default_enabled {
            popup_body_text()
        } else {
            popup_muted_text()
        },
    );

    if chooser.filtered_count() > OPEN_WITH_CHOOSER_MAX_ROWS {
        let end = (chooser.scroll_row + visible.len()).min(chooser.filtered_count());
        text.push_label(
            &format!(
                "{}-{} of {}",
                chooser.scroll_row + 1,
                end,
                chooser.filtered_count()
            ),
            ViewRect {
                x: rect.x + margin,
                y: default_row.bottom() + scaled_dialog_metric(3.0, scale),
                width: scaled_dialog_metric(120.0, scale),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            popup_muted_text(),
        );
    }

    if let Some(error) = chooser.error.as_ref() {
        text.push_label(
            error,
            ViewRect {
                x: rect.x + margin,
                y: default_row.bottom() + scaled_dialog_metric(3.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            popup_error_text(),
        );
    }

    let cancel = open_with_chooser_cancel_button_rect_scaled(rect, scale);
    let open = open_with_chooser_open_button_rect_scaled(rect, scale);
    for (label, button, active) in [("Cancel", cancel, false), ("Open", open, true)] {
        push_clipped_rounded_rect(
            vertices,
            button,
            rect,
            scaled_dialog_metric(5.0, scale),
            if active {
                POPUP_BUTTON_PRIMARY
            } else {
                POPUP_BUTTON_SECONDARY
            },
            size,
        );
        push_clipped_rect_outline(vertices, button, rect, 1.0, POPUP_BORDER, size);
        text.push_label_aligned(
            label,
            ViewRect {
                x: button.x + scaled_dialog_metric(10.0, scale),
                y: button.y + scaled_dialog_metric(4.0, scale),
                width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            if active {
                popup_inverse_text()
            } else {
                popup_body_text()
            },
            LabelAlignment::Center,
        );
    }
}

fn push_open_with_search_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let stroke = scaled_dialog_metric(1.5, scale).max(1.0);
    let lens = ViewRect {
        x: rect.x,
        y: rect.y,
        width: rect.width * 0.72,
        height: rect.height * 0.72,
    };
    push_clipped_rect_outline(vertices, lens, clip, stroke, POPUP_MARKER_NEUTRAL, size);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: rect.x + rect.width * 0.62,
            y: rect.y + rect.height * 0.68,
            width: rect.width * 0.34,
            height: stroke,
        },
        clip,
        stroke / 2.0,
        POPUP_MARKER_NEUTRAL,
        size,
    );
}

fn push_open_with_checkbox_check(
    vertices: &mut Vec<QuadVertex>,
    checkbox: ViewRect,
    clip: ViewRect,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let stroke = scaled_dialog_metric(2.0, scale).max(1.0);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: checkbox.x + checkbox.width * 0.25,
            y: checkbox.y + checkbox.height * 0.55,
            width: checkbox.width * 0.22,
            height: stroke,
        },
        clip,
        stroke / 2.0,
        [1.0, 1.0, 1.0, 1.0],
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: checkbox.x + checkbox.width * 0.42,
            y: checkbox.y + checkbox.height * 0.30,
            width: stroke,
            height: checkbox.height * 0.44,
        },
        clip,
        stroke / 2.0,
        [1.0, 1.0, 1.0, 1.0],
        size,
    );
}
