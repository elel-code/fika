use cosmic_text::Color as TextColor;
use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::metrics::PLACES_TASK_ROW_HEIGHT;
use crate::shell::status::{ShellPaneStatus, ShellTaskStatusStore};
use crate::shell::tasks::{ShellTaskStatus, ShellTaskStatusKind};
use crate::{
    LabelAlignment, QuadVertex, TextFrameBuilder, chrome_color, divider_color, inset_rect,
    muted_text_color, primary_text_color, push_clipped_rounded_rect, push_rect, section_text_color,
    sidebar_color,
};

pub(crate) struct PaneStatusBarPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) status: &'a ShellPaneStatus,
    pub(crate) active: bool,
    pub(crate) dark_mode: bool,
    pub(crate) scale: f32,
    pub(crate) line_height: f32,
    pub(crate) size: PhysicalSize<u32>,
}

pub(crate) fn push_pane_status_bar(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    paint: PaneStatusBarPaint<'_>,
) {
    push_rect(
        vertices,
        paint.rect,
        chrome_color(paint.dark_mode),
        paint.size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: paint.rect.x,
            y: paint.rect.y,
            width: paint.rect.width,
            height: 1.0,
        },
        divider_color(paint.dark_mode),
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
            [0.184, 0.435, 0.929, 1.0],
            paint.size,
        );
    }

    let left_x = paint.rect.x + scale_metric(12.0, paint.scale);
    let qualifier = paint.status.qualifier_text();
    let right_width = if qualifier.is_empty() {
        0.0
    } else {
        (paint.rect.width * 0.44)
            .min(scale_metric(260.0, paint.scale))
            .max(1.0)
    };
    text.push_label_aligned_no_wrap(
        &paint.status.primary,
        ViewRect {
            x: left_x,
            y: paint.rect.y + scale_metric(5.0, paint.scale),
            width: (paint.rect.width - scale_metric(24.0, paint.scale) - right_width).max(1.0),
            height: paint.line_height,
        },
        paint.rect,
        primary_text_color(paint.dark_mode),
        LabelAlignment::Start,
    );
    if !qualifier.is_empty() {
        text.push_label_aligned_no_wrap(
            qualifier,
            ViewRect {
                x: paint.rect.right() - scale_metric(12.0, paint.scale) - right_width,
                y: paint.rect.y + scale_metric(5.0, paint.scale),
                width: right_width,
                height: paint.line_height,
            },
            paint.rect,
            muted_text_color(paint.dark_mode),
            LabelAlignment::End,
        );
    }
}

pub(crate) struct PlacesTaskAreaPaint<'a> {
    pub(crate) rect: ViewRect,
    pub(crate) sidebar: ViewRect,
    pub(crate) statuses: &'a ShellTaskStatusStore,
    pub(crate) dark_mode: bool,
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
        divider_color(paint.dark_mode),
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
        sidebar_color(paint.dark_mode),
        paint.size,
    );

    let padding = scale_metric(10.0, paint.scale);
    text.push_label_aligned(
        "Tasks",
        ViewRect {
            x: inner.x + padding,
            y: inner.y + scale_metric(7.0, paint.scale),
            width: (inner.width - padding * 2.0).max(1.0),
            height: paint.small_line_height,
        },
        inner,
        section_text_color(paint.dark_mode),
        LabelAlignment::Start,
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
        task_status_color(status.kind),
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
        primary_text_color(paint.dark_mode),
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
        section_text_color(paint.dark_mode),
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

fn task_status_color(kind: ShellTaskStatusKind) -> [f32; 4] {
    match kind {
        ShellTaskStatusKind::Running => [0.184, 0.435, 0.929, 1.0],
        ShellTaskStatusKind::Completed => [0.102, 0.514, 0.286, 1.0],
        ShellTaskStatusKind::Failed => [0.820, 0.184, 0.184, 1.0],
        ShellTaskStatusKind::Cancelled => [0.475, 0.514, 0.565, 1.0],
    }
}

fn scale_metric(value: f32, scale: f32) -> f32 {
    (value * scale).round()
}
