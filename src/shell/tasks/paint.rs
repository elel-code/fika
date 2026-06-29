use std::collections::VecDeque;

use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::metrics::{TASK_DETAIL_TITLE_HEIGHT, scaled_dialog_metric};
use crate::shell::popup::style::{
    POPUP_BACKDROP, POPUP_BORDER, POPUP_BUTTON_DANGER, POPUP_BUTTON_PRIMARY,
    POPUP_BUTTON_SECONDARY, POPUP_DIVIDER, POPUP_HEADER, POPUP_PANEL, POPUP_STATUS_CANCELLED,
    POPUP_STATUS_COMPLETED, POPUP_STATUS_FAILED, POPUP_STATUS_RUNNING, POPUP_SURFACE,
    popup_body_text, popup_inverse_text, popup_soft_text, popup_status_text, popup_title_text,
};
use crate::shell::tasks::geometry::{
    task_detail_cancel_button_rect_scaled, task_detail_clear_button_rect_scaled,
    task_detail_dialog_rect_scaled, task_detail_dismiss_button_rect_scaled,
    task_detail_row_rect_scaled,
};
use crate::shell::tasks::{ShellTaskStatus, ShellTaskStatusKind};
use crate::{
    LabelAlignment, QuadVertex, TextFrameBuilder, push_clipped_rect_outline,
    push_clipped_rounded_rect, push_rect,
};

pub(crate) fn push_task_detail_dialog_overlay(
    statuses: &VecDeque<ShellTaskStatus>,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let screen = ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    };
    push_rect(vertices, screen, POPUP_BACKDROP, size);
    let rect = task_detail_dialog_rect_scaled(statuses.len(), size, scale);
    let title_height = scaled_dialog_metric(TASK_DETAIL_TITLE_HEIGHT, scale);
    let margin = scaled_dialog_metric(18.0, scale);
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
    text.push_label_aligned(
        "Task Details",
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(13.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        popup_title_text(),
        LabelAlignment::Start,
    );

    for (index, status) in statuses.iter().take(4).enumerate() {
        let row = task_detail_row_rect_scaled(rect, index, scale);
        push_clipped_rounded_rect(
            vertices,
            row,
            rect,
            scaled_dialog_metric(6.0, scale),
            POPUP_PANEL,
            size,
        );
        push_clipped_rect_outline(vertices, row, rect, 1.0, POPUP_DIVIDER, size);
        let accent_color = match status.kind {
            ShellTaskStatusKind::Running => POPUP_STATUS_RUNNING,
            ShellTaskStatusKind::Completed => POPUP_STATUS_COMPLETED,
            ShellTaskStatusKind::Failed => POPUP_STATUS_FAILED,
            ShellTaskStatusKind::Cancelled => POPUP_STATUS_CANCELLED,
        };
        let strip_width = scaled_dialog_metric(3.0, scale).max(1.0);
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: row.x,
                y: row.y,
                width: strip_width,
                height: row.height,
            },
            row,
            scaled_dialog_metric(2.0, scale),
            accent_color,
            size,
        );
        let dot = ViewRect {
            x: row.x + scaled_dialog_metric(12.0, scale),
            y: row.y + scaled_dialog_metric(11.0, scale),
            width: scaled_dialog_metric(8.0, scale),
            height: scaled_dialog_metric(8.0, scale),
        };
        push_clipped_rounded_rect(vertices, dot, row, dot.width / 2.0, accent_color, size);

        let text_x = dot.right() + scaled_dialog_metric(10.0, scale);
        let button = task_detail_dismiss_button_rect_scaled(rect, index, scale);
        text.push_label_aligned(
            &status.label,
            ViewRect {
                x: text_x,
                y: row.y + scaled_dialog_metric(6.0, scale),
                width: (button.x - text_x - scaled_dialog_metric(10.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            row,
            popup_body_text(),
            LabelAlignment::Start,
        );
        let detail = status.detail_label();
        text.push_label_aligned(
            detail.as_ref(),
            ViewRect {
                x: text_x,
                y: row.y + scaled_dialog_metric(28.0, scale),
                width: (button.x - text_x - scaled_dialog_metric(10.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            row,
            popup_soft_text(),
            LabelAlignment::Start,
        );
        text.push_label_aligned(
            status.kind.label(),
            ViewRect {
                x: text_x,
                y: row.y + scaled_dialog_metric(47.0, scale),
                width: (button.x - text_x - scaled_dialog_metric(10.0, scale)).max(1.0),
                height: scaled_dialog_metric(16.0, scale),
            },
            row,
            popup_status_text(status.kind),
            LabelAlignment::Start,
        );

        let row_action_is_cancel =
            status.kind == ShellTaskStatusKind::Running && status.cancellable;
        push_clipped_rounded_rect(
            vertices,
            button,
            rect,
            scaled_dialog_metric(5.0, scale),
            if row_action_is_cancel {
                POPUP_BUTTON_DANGER
            } else {
                POPUP_BUTTON_SECONDARY
            },
            size,
        );
        push_clipped_rect_outline(vertices, button, rect, 1.0, POPUP_BORDER, size);
        text.push_label_aligned(
            if row_action_is_cancel {
                "Cancel"
            } else {
                "Dismiss"
            },
            ViewRect {
                x: button.x + scaled_dialog_metric(8.0, scale),
                y: button.y + scaled_dialog_metric(4.0, scale),
                width: (button.width - scaled_dialog_metric(16.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            if row_action_is_cancel {
                popup_inverse_text()
            } else {
                popup_body_text()
            },
            LabelAlignment::Center,
        );
    }

    let clear = task_detail_clear_button_rect_scaled(rect, scale);
    let cancel = task_detail_cancel_button_rect_scaled(rect, scale);
    for (label, button, active) in [("Clear", clear, false), ("Close", cancel, true)] {
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
