use cosmic_text::Color as TextColor;
use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::metrics::PLACES_TASK_ROW_HEIGHT;
use crate::shell::status::{ShellPaneStatus, ShellTaskStatusStore};
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::theme::ShellTheme;
use crate::{
    LabelAlignment, QuadVertex, TextFrameBuilder, inset_rect, push_clipped_rounded_rect, push_rect,
};

pub(crate) struct PaneStatusBarPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) status: &'a ShellPaneStatus,
    pub(crate) active: bool,
    pub(crate) zoom_percent: i32,
    pub(crate) zoom_fraction: f32,
    pub(crate) theme: ShellTheme,
    pub(crate) scale: f32,
    pub(crate) line_height: f32,
    pub(crate) size: PhysicalSize<u32>,
}

pub(crate) fn push_pane_status_bar(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: PaneStatusBarPaint<'_>,
) {
    push_rect(vertices, paint.rect, paint.theme.chrome(), paint.size);
    push_rect(
        vertices,
        ViewRect {
            x: paint.rect.x,
            y: paint.rect.y,
            width: paint.rect.width,
            height: 1.0,
        },
        paint.theme.divider(),
        paint.size,
    );
    if paint.active {
        push_rect(
            vertices,
            ViewRect {
                x: paint.rect.x,
                y: paint.rect.y,
                width: scale_metric(3.0, paint.scale).max(1.0),
                height: paint.rect.height,
            },
            paint.theme.accent(),
            paint.size,
        );
    }

    let left_x = paint.rect.x + scale_metric(12.0, paint.scale);
    let qualifier = paint.status.qualifier_text();
    let zoom_visible = paint.rect.width >= scale_metric(460.0, paint.scale);
    let zoom_width = if zoom_visible {
        scale_metric(132.0, paint.scale)
    } else {
        0.0
    };
    let right_edge = paint.rect.right() - scale_metric(12.0, paint.scale);
    if zoom_visible {
        let zoom_rect = ViewRect {
            x: right_edge - zoom_width,
            y: paint.rect.y + (paint.rect.height - scale_metric(18.0, paint.scale)) / 2.0,
            width: zoom_width,
            height: scale_metric(18.0, paint.scale),
        };
        push_zoom_indicator(vertices, text, &paint, zoom_rect);
    }
    let right_width = if qualifier.is_empty() {
        0.0
    } else {
        (paint.rect.width * 0.44)
            .min(scale_metric(260.0, paint.scale))
            .min((paint.rect.width - zoom_width - scale_metric(48.0, paint.scale)).max(0.0))
            .max(1.0)
    };
    text.push_label_aligned_no_wrap(
        &paint.status.primary,
        ViewRect {
            x: left_x,
            y: paint.rect.y + scale_metric(5.0, paint.scale),
            width: (paint.rect.width
                - scale_metric(24.0, paint.scale)
                - right_width
                - zoom_width
                - if zoom_visible {
                    scale_metric(10.0, paint.scale)
                } else {
                    0.0
                })
            .max(1.0),
            height: paint.line_height,
        },
        paint.rect,
        paint.theme.primary_text(),
        LabelAlignment::Start,
    );
    if !qualifier.is_empty() {
        text.push_label_aligned_no_wrap(
            qualifier,
            ViewRect {
                x: right_edge
                    - zoom_width
                    - if zoom_visible {
                        scale_metric(10.0, paint.scale)
                    } else {
                        0.0
                    }
                    - right_width,
                y: paint.rect.y + scale_metric(5.0, paint.scale),
                width: right_width,
                height: paint.line_height,
            },
            paint.rect,
            paint.theme.muted_text(),
            LabelAlignment::End,
        );
    }
}

fn push_zoom_indicator(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: &PaneStatusBarPaint<'_>,
    rect: ViewRect,
) {
    let label_width = scale_metric(42.0, paint.scale);
    let gap = scale_metric(9.0, paint.scale);
    let track_height = scale_metric(4.0, paint.scale).max(2.0);
    let track = ViewRect {
        x: rect.x + label_width + gap,
        y: rect.y + (rect.height - track_height) / 2.0,
        width: (rect.width - label_width - gap).max(1.0),
        height: track_height,
    };
    let scrollbar = paint.theme.scrollbar();
    push_clipped_rounded_rect(
        vertices,
        track,
        paint.rect,
        track_height / 2.0,
        scrollbar.track,
        paint.size,
    );
    let fraction = paint.zoom_fraction.clamp(0.0, 1.0);
    let filled = ViewRect {
        width: (track.width * fraction).max(track_height),
        ..track
    };
    push_clipped_rounded_rect(
        vertices,
        filled,
        paint.rect,
        track_height / 2.0,
        paint.theme.accent(),
        paint.size,
    );
    let thumb_size = scale_metric(8.0, paint.scale).max(4.0);
    let thumb_center_x = track.x + track.width * fraction;
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: (thumb_center_x - thumb_size / 2.0).clamp(track.x, track.right() - thumb_size),
            y: track.y + (track.height - thumb_size) / 2.0,
            width: thumb_size,
            height: thumb_size,
        },
        paint.rect,
        thumb_size / 2.0,
        paint.theme.accent(),
        paint.size,
    );
    text.push_label_aligned_no_wrap(
        &format!("{}%", paint.zoom_percent),
        ViewRect {
            x: rect.x,
            y: rect.y,
            width: label_width,
            height: paint.line_height,
        },
        paint.rect,
        paint.theme.muted_text(),
        LabelAlignment::End,
    );
}

pub(crate) struct PlacesTaskAreaPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) sidebar: ViewRect,
    pub(crate) statuses: &'a ShellTaskStatusStore,
    pub(crate) theme: ShellTheme,
    pub(crate) scale: f32,
    pub(crate) small_line_height: f32,
    pub(crate) size: PhysicalSize<u32>,
}

pub(crate) fn push_places_task_area(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: PlacesTaskAreaPaint<'_>,
) {
    let radius = scale_metric(10.0, paint.scale);
    push_clipped_rounded_rect(
        vertices,
        paint.rect,
        paint.sidebar,
        radius,
        paint.theme.divider(),
        paint.size,
    );
    let Some(inner) = inset_rect(paint.rect, scale_metric(1.0, paint.scale)) else {
        return;
    };
    push_clipped_rounded_rect(
        vertices,
        inner,
        paint.sidebar,
        (radius - scale_metric(1.0, paint.scale)).max(1.0),
        paint.theme.sidebar(),
        paint.size,
    );

    let padding = scale_metric(10.0, paint.scale);
    text.push_label_aligned(
        "Tasks",
        ViewRect {
            x: inner.x + padding + scale_metric(8.0, paint.scale),
            y: inner.y + scale_metric(7.0, paint.scale),
            width: (inner.width - padding * 2.0 - scale_metric(8.0, paint.scale)).max(1.0),
            height: paint.small_line_height,
        },
        inner,
        paint.theme.section_text(),
        LabelAlignment::Start,
    );
    let mark_height = scale_metric(10.0, paint.scale);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: inner.x + padding,
            y: inner.y + scale_metric(9.0, paint.scale),
            width: scale_metric(3.0, paint.scale).max(2.0),
            height: mark_height,
        },
        inner,
        scale_metric(1.5, paint.scale),
        paint.theme.accent(),
        paint.size,
    );

    let row_height = scale_metric(PLACES_TASK_ROW_HEIGHT, paint.scale);
    let mut y = inner.y + scale_metric(26.0, paint.scale);
    let max_rows = ((inner.bottom() - y - scale_metric(4.0, paint.scale)) / row_height)
        .floor()
        .max(0.0) as usize;
    for status in paint.statuses.iter().take(max_rows) {
        push_places_task_status_row(vertices, text, &paint, inner, y, padding, status);
        y += row_height;
    }
}

fn push_places_task_status_row(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: &PlacesTaskAreaPaint<'_>,
    inner: ViewRect,
    y: f32,
    padding: f32,
    status: &ShellTaskStatus,
) {
    let dot_size = scale_metric(7.0, paint.scale);
    let dot = ViewRect {
        x: inner.x + padding,
        y: y + scale_metric(5.0, paint.scale),
        width: dot_size,
        height: dot_size,
    };
    push_clipped_rounded_rect(
        vertices,
        dot,
        inner,
        dot_size / 2.0,
        paint.theme.task_status_color(status.kind),
        paint.size,
    );

    let text_x = dot.right() + scale_metric(8.0, paint.scale);
    let text_width = (inner.right() - text_x - padding).max(1.0);
    push_status_label(
        text,
        &status.label,
        ViewRect {
            x: text_x,
            y,
            width: text_width,
            height: paint.small_line_height,
        },
        inner,
        paint.theme.primary_text(),
    );
    let detail = status.detail_label();
    push_status_label(
        text,
        detail.as_ref(),
        ViewRect {
            x: text_x,
            y: y + scale_metric(16.0, paint.scale),
            width: text_width,
            height: paint.small_line_height,
        },
        inner,
        paint.theme.section_text(),
    );
}

fn push_status_label(
    text: &mut TextFrameBuilder<'_>,
    label: &str,
    rect: ViewRect,
    clip: ViewRect,
    color: TextColor,
) {
    text.push_label_aligned(label, rect, clip, color, LabelAlignment::Start);
}

fn scale_metric(value: f32, scale: f32) -> f32 {
    (value * scale).round()
}
