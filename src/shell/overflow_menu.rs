pub(crate) mod paint;

use fika_core::{ViewPoint, ViewRect};
use winit::dpi::PhysicalSize;

use crate::shell::menu_geometry::scaled_context_menu_metric;
use crate::shell::metrics::{
    CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_VERTICAL_PADDING, CONTEXT_MENU_VIEWPORT_MARGIN,
    CONTEXT_MENU_WIDTH,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellOverflowMenuAction {
    ToggleHiddenFiles,
    TogglePlaces,
    ToggleDarkMode,
}

impl ShellOverflowMenuAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ToggleHiddenFiles => "toggle-hidden-files",
            Self::TogglePlaces => "toggle-places",
            Self::ToggleDarkMode => "toggle-dark-mode",
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
) -> [ShellOverflowMenuItem; 3] {
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
    let height = (vertical_padding * 2.0
        + overflow_menu_items(false, false, false).len() as f32 * row_height)
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
    (row < overflow_menu_items(false, false, false).len()).then(|| {
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
    (row < overflow_menu_items(false, false, false).len()).then_some(row)
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
        let items = overflow_menu_items(true, false, true);
        assert_eq!(items[0].label, "Hide Hidden Files");
        assert_eq!(items[1].label, "Show Places");
        assert_eq!(items[2].label, "Use Light Theme");
    }
}
