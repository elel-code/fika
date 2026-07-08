use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::options::ShellViewMode;
use crate::shell::pane::ShellPaneId;
use crate::shell::render::quad::{
    QuadVertex, push_clipped_rect, push_clipped_rect_outline, push_clipped_rounded_rect, push_rect,
};
use crate::shell::theme::ShellTheme;
use crate::{ShellScene, shell::toolbar::ShellToolbarViewModeControl};

impl ShellScene {
    pub(crate) fn push_app_toolbar(
        &self,
        vertices: &mut Vec<QuadVertex>,
        size: PhysicalSize<u32>,
        theme: ShellTheme,
    ) {
        let layout = self.app_toolbar_layout(size);
        let toolbar = layout.toolbar;
        push_rect(vertices, toolbar, theme.details_header(), size);
        push_rect(
            vertices,
            ViewRect {
                x: toolbar.x,
                y: toolbar.bottom() - self.scale_metric(1.0).max(1.0),
                width: toolbar.width,
                height: self.scale_metric(1.0).max(1.0),
            },
            theme.divider(),
            size,
        );

        let button = layout.places_toggle;
        let split_button = layout.split_view;
        let view_mode = layout.view_mode;
        let places_hovered = self.pointer.is_some_and(|point| button.contains(point));
        let places_active = self.places_visible || places_hovered;
        let button_colors = theme.toolbar_button(places_active);
        if places_active {
            push_clipped_rounded_rect(
                vertices,
                button,
                toolbar,
                self.scale_metric(6.0),
                button_colors.fill,
                size,
            );
        }

        let icon = ViewRect {
            x: button.x + (button.width - self.scale_metric(18.0)) / 2.0,
            y: button.y + (button.height - self.scale_metric(18.0)) / 2.0,
            width: self.scale_metric(18.0),
            height: self.scale_metric(18.0),
        };
        let rail = self.scale_metric(2.0);
        push_clipped_rect(
            vertices,
            ViewRect {
                x: icon.x + self.scale_metric(2.0),
                y: icon.y + self.scale_metric(2.0),
                width: rail,
                height: icon.height - self.scale_metric(4.0),
            },
            toolbar,
            button_colors.icon,
            size,
        );
        push_clipped_rect_outline(
            vertices,
            ViewRect {
                x: icon.x + self.scale_metric(1.0),
                y: icon.y + self.scale_metric(3.0),
                width: icon.width - self.scale_metric(2.0),
                height: icon.height - self.scale_metric(6.0),
            },
            toolbar,
            self.scale_metric(1.0),
            button_colors.icon,
            size,
        );

        if let Some(control) = view_mode {
            self.push_toolbar_view_mode_control(vertices, control, toolbar, theme, size);
        }

        let split_open = self.panes.is_open(ShellPaneId::SLOT_1);
        let split_hovered = self
            .pointer
            .is_some_and(|point| split_button.contains(point));
        let split_active = split_open || split_hovered;
        let split_colors = theme.toolbar_button(split_active);
        if split_active {
            push_clipped_rounded_rect(
                vertices,
                split_button,
                toolbar,
                self.scale_metric(6.0),
                split_colors.fill,
                size,
            );
        }
        let split_icon = ViewRect {
            x: split_button.x + (split_button.width - self.scale_metric(18.0)) / 2.0,
            y: split_button.y + (split_button.height - self.scale_metric(18.0)) / 2.0,
            width: self.scale_metric(18.0),
            height: self.scale_metric(18.0),
        };
        push_clipped_rect_outline(
            vertices,
            ViewRect {
                x: split_icon.x + self.scale_metric(1.0),
                y: split_icon.y + self.scale_metric(2.0),
                width: split_icon.width - self.scale_metric(2.0),
                height: split_icon.height - self.scale_metric(4.0),
            },
            toolbar,
            self.scale_metric(1.0),
            split_colors.icon,
            size,
        );
        push_clipped_rect(
            vertices,
            ViewRect {
                x: split_icon.x + split_icon.width / 2.0 - self.scale_metric(0.5),
                y: split_icon.y + self.scale_metric(2.0),
                width: self.scale_metric(1.0),
                height: split_icon.height - self.scale_metric(4.0),
            },
            toolbar,
            split_colors.icon,
            size,
        );
        if split_open {
            let close_center_x = if self.active_pane() == ShellPaneId::SLOT_0 {
                split_icon.x + split_icon.width * 0.25
            } else {
                split_icon.x + split_icon.width * 0.75
            };
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: close_center_x - self.scale_metric(3.0),
                    y: split_icon.y + split_icon.height / 2.0 - self.scale_metric(0.5),
                    width: self.scale_metric(6.0),
                    height: self.scale_metric(1.0),
                },
                toolbar,
                split_colors.icon,
                size,
            );
        }
    }

    fn push_toolbar_view_mode_control(
        &self,
        vertices: &mut Vec<QuadVertex>,
        control: ShellToolbarViewModeControl,
        clip: ViewRect,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        let rect = control.outer;
        let hovered = self.pointer.is_some_and(|point| rect.contains(point));
        let colors = theme.toolbar_button(hovered);
        if hovered {
            push_clipped_rounded_rect(
                vertices,
                rect,
                clip,
                self.scale_metric(7.0),
                colors.fill,
                size,
            );
        }
        let segments = control.segments;
        for segment in segments {
            let segment_hovered = self
                .pointer
                .is_some_and(|point| segment.rect.contains(point));
            let active = segment.mode == self.active_view_mode();
            if active || segment_hovered {
                let fill = theme.toolbar_button(active || segment_hovered).fill;
                push_clipped_rounded_rect(
                    vertices,
                    segment.rect,
                    rect,
                    self.scale_metric(5.0),
                    fill,
                    size,
                );
            }
            let glyph_size = self.scale_metric(15.0).min(segment.rect.height).max(1.0);
            let icon_rect = ViewRect {
                x: segment.rect.x + (segment.rect.width - glyph_size) / 2.0,
                y: segment.rect.y + (segment.rect.height - glyph_size) / 2.0,
                width: glyph_size,
                height: glyph_size,
            };
            self.push_view_mode_glyph(vertices, segment.mode, icon_rect, rect, theme, size);
        }
    }

    fn push_view_mode_glyph(
        &self,
        vertices: &mut Vec<QuadVertex>,
        mode: ShellViewMode,
        rect: ViewRect,
        clip: ViewRect,
        theme: ShellTheme,
        size: PhysicalSize<u32>,
    ) {
        let active = mode == self.active_view_mode();
        let color = if active {
            theme.accent()
        } else {
            theme.toolbar_button(false).icon
        };
        match mode {
            ShellViewMode::Icons => {
                let dot = self.scale_metric(4.0).max(2.0);
                let gap = self.scale_metric(3.0).max(1.0);
                let content_size = dot * 2.0 + gap;
                let origin_x = rect.x + (rect.width - content_size) / 2.0;
                let origin_y = rect.y + (rect.height - content_size) / 2.0;
                for row in 0..2 {
                    for column in 0..2 {
                        push_clipped_rounded_rect(
                            vertices,
                            ViewRect {
                                x: origin_x + column as f32 * (dot + gap),
                                y: origin_y + row as f32 * (dot + gap),
                                width: dot,
                                height: dot,
                            },
                            clip,
                            dot / 2.0,
                            color,
                            size,
                        );
                    }
                }
            }
            ShellViewMode::Compact => {
                let row_height = self.scale_metric(3.0).max(2.0);
                let row_step = self.scale_metric(5.0);
                let content_height = row_height + row_step * 2.0;
                let content_y = rect.y + (rect.height - content_height) / 2.0;
                let marker_width = self.scale_metric(5.0).min(rect.width).max(1.0);
                let marker_gap = self
                    .scale_metric(3.0)
                    .min((rect.width - marker_width).max(0.0));
                let line_x = rect.x + marker_width + marker_gap;
                let line_width = (rect.right() - line_x).max(1.0);
                for row in 0..3 {
                    let y = content_y + row as f32 * row_step;
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: rect.x,
                            y,
                            width: marker_width,
                            height: row_height,
                        },
                        clip,
                        self.scale_metric(1.5),
                        color,
                        size,
                    );
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: line_x,
                            y,
                            width: line_width,
                            height: row_height,
                        },
                        clip,
                        self.scale_metric(1.5),
                        if active {
                            theme.field_separator()
                        } else {
                            color
                        },
                        size,
                    );
                }
            }
            ShellViewMode::Details => {
                let row_height = self.scale_metric(3.0).max(2.0);
                let row_step = self.scale_metric(5.0);
                let content_height = row_height + row_step * 2.0;
                let content_y = rect.y + (rect.height - content_height) / 2.0;
                for row in 0..3 {
                    let y = content_y + row as f32 * row_step;
                    push_clipped_rounded_rect(
                        vertices,
                        ViewRect {
                            x: rect.x,
                            y,
                            width: rect.width,
                            height: row_height,
                        },
                        clip,
                        self.scale_metric(1.5),
                        if row == 0 || active {
                            color
                        } else {
                            theme.field_separator()
                        },
                        size,
                    );
                }
            }
        }
    }
}
