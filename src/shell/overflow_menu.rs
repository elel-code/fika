pub(crate) mod paint;

use fika_core::{ViewPoint, ViewRect};
use winit::dpi::PhysicalSize;

use crate::shell::menu_geometry::scaled_context_menu_metric;
use crate::shell::metrics::{
    CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_VERTICAL_PADDING, CONTEXT_MENU_VIEWPORT_MARGIN,
    CONTEXT_MENU_WIDTH,
};

pub(crate) const WINDOW_OPACITY_MIN_PERCENT: u8 = 20;
pub(crate) const WINDOW_OPACITY_MAX_PERCENT: u8 = 100;
const WINDOW_OPACITY_STEP_PERCENT: u8 = 5;
const OVERFLOW_MENU_ITEM_COUNT: usize = 5;
const WINDOW_OPACITY_ROW: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellOverflowMenuAction {
    ToggleHiddenFiles,
    TogglePlaces,
    ToggleDarkMode,
    ToggleBackgroundBlur,
    SetWindowOpacity(u8),
}

impl ShellOverflowMenuAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ToggleHiddenFiles => "toggle-hidden-files",
            Self::TogglePlaces => "toggle-places",
            Self::ToggleDarkMode => "toggle-dark-mode",
            Self::ToggleBackgroundBlur => "toggle-background-blur",
            Self::SetWindowOpacity(_) => "set-window-opacity",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellOverflowMenuItem {
    pub(crate) action: ShellOverflowMenuAction,
    pub(crate) label: &'static str,
    pub(crate) active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellOverflowMenu {
    pub(crate) anchor: ViewRect,
    pub(crate) hovered_row: Option<usize>,
}

impl ShellOverflowMenu {
    pub(crate) fn new(anchor: ViewRect) -> Self {
        Self {
            anchor,
            hovered_row: None,
        }
    }
}

pub(crate) fn overflow_menu_items(
    show_hidden: bool,
    places_visible: bool,
    dark_mode: bool,
    background_blur: bool,
    window_opacity: f32,
) -> [ShellOverflowMenuItem; OVERFLOW_MENU_ITEM_COUNT] {
    [
        ShellOverflowMenuItem {
            action: ShellOverflowMenuAction::ToggleHiddenFiles,
            label: if show_hidden {
                "Hide Hidden Files"
            } else {
                "Show Hidden Files"
            },
            active: show_hidden,
        },
        ShellOverflowMenuItem {
            action: ShellOverflowMenuAction::TogglePlaces,
            label: if places_visible {
                "Hide Places"
            } else {
                "Show Places"
            },
            active: places_visible,
        },
        ShellOverflowMenuItem {
            action: ShellOverflowMenuAction::ToggleDarkMode,
            label: if dark_mode {
                "Use Light Theme"
            } else {
                "Use Dark Theme"
            },
            active: dark_mode,
        },
        ShellOverflowMenuItem {
            action: ShellOverflowMenuAction::ToggleBackgroundBlur,
            label: "Background Blur",
            active: background_blur,
        },
        ShellOverflowMenuItem {
            action: ShellOverflowMenuAction::SetWindowOpacity(window_opacity_percent(
                window_opacity,
            )),
            label: "Window Opacity",
            active: false,
        },
    ]
}

pub(crate) fn overflow_menu_rect(
    menu: &ShellOverflowMenu,
    size: PhysicalSize<u32>,
    scale: f32,
) -> ViewRect {
    let viewport_width = size.width.max(1) as f32;
    let viewport_height = size.height.max(1) as f32;
    let margin = scaled_context_menu_metric(CONTEXT_MENU_VIEWPORT_MARGIN, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let vertical_padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let gap = scaled_context_menu_metric(4.0, scale);
    let width = scaled_context_menu_metric(CONTEXT_MENU_WIDTH, scale)
        .min((viewport_width - margin * 2.0).max(1.0));
    let height = (vertical_padding * 2.0 + OVERFLOW_MENU_ITEM_COUNT as f32 * row_height)
        .min((viewport_height - margin * 2.0).max(1.0));
    let max_x = (viewport_width - margin - width).max(margin);
    let x = (menu.anchor.right() - width).clamp(margin.min(max_x), max_x);
    let preferred_y = menu.anchor.bottom() + gap;
    let y = if preferred_y + height <= viewport_height - margin {
        preferred_y
    } else {
        (menu.anchor.y - gap - height).max(margin)
    };
    ViewRect {
        x,
        y: y.clamp(margin, (viewport_height - margin - height).max(margin)),
        width: width.max(1.0),
        height: height.max(1.0),
    }
}

pub(crate) fn overflow_menu_row_rect(
    menu: &ShellOverflowMenu,
    size: PhysicalSize<u32>,
    scale: f32,
    row: usize,
) -> Option<ViewRect> {
    (row < OVERFLOW_MENU_ITEM_COUNT).then(|| {
        let rect = overflow_menu_rect(menu, size, scale);
        let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
        let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
        ViewRect {
            x: rect.x,
            y: rect.y + padding + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        }
    })
}

pub(crate) fn overflow_menu_row_at_screen_point(
    menu: &ShellOverflowMenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
) -> Option<usize> {
    let rect = overflow_menu_rect(menu, size, scale);
    if !rect.contains(point) {
        return None;
    }
    let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_y = point.y - rect.y - padding;
    if row_y < 0.0 {
        return None;
    }
    let row = (row_y / row_height).floor() as usize;
    (row < OVERFLOW_MENU_ITEM_COUNT).then_some(row)
}

pub(crate) fn window_opacity_percent(opacity: f32) -> u8 {
    let percent = (opacity.clamp(
        WINDOW_OPACITY_MIN_PERCENT as f32 / 100.0,
        WINDOW_OPACITY_MAX_PERCENT as f32 / 100.0,
    ) * 100.0)
        .round() as u8;
    ((percent as f32 / WINDOW_OPACITY_STEP_PERCENT as f32).round() as u8
        * WINDOW_OPACITY_STEP_PERCENT)
        .clamp(WINDOW_OPACITY_MIN_PERCENT, WINDOW_OPACITY_MAX_PERCENT)
}

pub(crate) fn overflow_opacity_track_rect(
    menu: &ShellOverflowMenu,
    size: PhysicalSize<u32>,
    scale: f32,
) -> ViewRect {
    let row = overflow_menu_row_rect(menu, size, scale, WINDOW_OPACITY_ROW)
        .unwrap_or_else(|| overflow_menu_rect(menu, size, scale));
    let label_width = scaled_context_menu_metric(106.0, scale).min(row.width * 0.48);
    let value_width = scaled_context_menu_metric(40.0, scale).min(row.width * 0.2);
    let padding = scaled_context_menu_metric(12.0, scale);
    let height = scaled_context_menu_metric(4.0, scale);
    ViewRect {
        x: row.x + label_width,
        y: row.y + (row.height - height) / 2.0,
        width: (row.width - label_width - value_width - padding).max(1.0),
        height,
    }
}

pub(crate) fn opacity_percent_at_screen_point(
    menu: &ShellOverflowMenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
) -> Option<u8> {
    if overflow_menu_row_at_screen_point(menu, point, size, scale) != Some(WINDOW_OPACITY_ROW) {
        return None;
    }
    let track = overflow_opacity_track_rect(menu, size, scale);
    if point.x < track.x || point.x > track.right() {
        return None;
    }
    let fraction = ((point.x - track.x) / track.width.max(1.0)).clamp(0.0, 1.0);
    let range = WINDOW_OPACITY_MAX_PERCENT - WINDOW_OPACITY_MIN_PERCENT;
    let raw = WINDOW_OPACITY_MIN_PERCENT as f32 + fraction * range as f32;
    let stepped =
        (raw / WINDOW_OPACITY_STEP_PERCENT as f32).round() as u8 * WINDOW_OPACITY_STEP_PERCENT;
    Some(stepped.clamp(WINDOW_OPACITY_MIN_PERCENT, WINDOW_OPACITY_MAX_PERCENT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_is_right_aligned_to_anchor_and_rows_are_hittable() {
        let size = PhysicalSize::new(800, 600);
        let menu = ShellOverflowMenu::new(ViewRect {
            x: 760.0,
            y: 8.0,
            width: 28.0,
            height: 28.0,
        });
        let rect = overflow_menu_rect(&menu, size, 1.0);
        assert_eq!(rect.right(), menu.anchor.right());
        let row = overflow_menu_row_rect(&menu, size, 1.0, 1).unwrap();
        assert_eq!(
            overflow_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: row.x + 4.0,
                    y: row.y + 4.0,
                },
                size,
                1.0,
            ),
            Some(1)
        );
    }

    #[test]
    fn menu_labels_describe_the_next_action() {
        let items = overflow_menu_items(true, false, true, true, 0.8);
        assert_eq!(items[0].label, "Hide Hidden Files");
        assert_eq!(items[1].label, "Show Places");
        assert_eq!(items[2].label, "Use Light Theme");
        assert_eq!(
            items[3].action,
            ShellOverflowMenuAction::ToggleBackgroundBlur
        );
        assert_eq!(
            items[4].action,
            ShellOverflowMenuAction::SetWindowOpacity(80)
        );
    }

    #[test]
    fn opacity_track_maps_to_safe_five_percent_steps() {
        let size = PhysicalSize::new(800, 600);
        let menu = ShellOverflowMenu::new(ViewRect {
            x: 760.0,
            y: 8.0,
            width: 28.0,
            height: 28.0,
        });
        let track = overflow_opacity_track_rect(&menu, size, 1.0);
        assert_eq!(
            opacity_percent_at_screen_point(
                &menu,
                ViewPoint {
                    x: track.x,
                    y: track.y,
                },
                size,
                1.0,
            ),
            Some(WINDOW_OPACITY_MIN_PERCENT)
        );
        assert_eq!(
            opacity_percent_at_screen_point(
                &menu,
                ViewPoint {
                    x: track.right(),
                    y: track.y,
                },
                size,
                1.0,
            ),
            Some(WINDOW_OPACITY_MAX_PERCENT)
        );
    }
}
