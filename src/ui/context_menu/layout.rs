use fika_core::ViewPoint;

use super::ContextMenuOpenSubmenu;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContextMenuOverlayRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) max_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContextMenuOverlayLayout {
    pub(crate) root: ContextMenuOverlayRect,
    pub(crate) submenu: Option<ContextMenuOverlayRect>,
    pub(crate) nested_submenu: Option<ContextMenuOverlayRect>,
}

impl ContextMenuOverlayLayout {
    pub(crate) fn contains(self, point: ViewPoint) -> bool {
        self.root.contains(point)
            || self.submenu.is_some_and(|rect| rect.contains(point))
            || self.nested_submenu.is_some_and(|rect| rect.contains(point))
    }
}

impl ContextMenuOverlayRect {
    fn contains(self, point: ViewPoint) -> bool {
        point.x >= self.x
            && point.x < self.x + self.width
            && point.y >= self.y
            && point.y < self.y + self.max_height
    }
}

const CONTEXT_MENU_WIDTH: f32 = 196.0;
pub(crate) const CONTEXT_MENU_ROW_HEIGHT: f32 = 28.0;
pub(crate) const CONTEXT_MENU_VERTICAL_PADDING: f32 = 4.0;
pub(crate) const CONTEXT_MENU_VIEWPORT_MARGIN: f32 = 8.0;

pub(crate) fn context_menu_overlay_layout(
    position: ViewPoint,
    action_count: usize,
    active_submenu: Option<ContextMenuOpenSubmenu>,
    submenu_count: usize,
    nested_submenu_count: usize,
    viewport_width: f32,
    viewport_height: f32,
) -> ContextMenuOverlayLayout {
    let root_width = context_menu_width_for_viewport(viewport_width);
    let root_height = context_menu_height(action_count);
    let root_max_height = context_menu_max_height_for_viewport(viewport_height).min(root_height);
    let root = ContextMenuOverlayRect {
        x: popup_menu_axis(position.x, root_width, viewport_width),
        y: popup_menu_axis(position.y, root_max_height, viewport_height),
        width: root_width,
        max_height: root_max_height,
    };
    let submenu = active_submenu.map(|open| {
        cascading_menu_rect(
            root,
            open.parent_index,
            submenu_count,
            viewport_width,
            viewport_height,
        )
    });
    let nested_submenu = active_submenu
        .and_then(|open| open.nested)
        .zip(submenu)
        .map(|(nested, parent)| {
            cascading_menu_rect(
                parent,
                nested.parent_index,
                nested_submenu_count,
                viewport_width,
                viewport_height,
            )
        });

    ContextMenuOverlayLayout {
        root,
        submenu,
        nested_submenu,
    }
}

fn cascading_menu_rect(
    parent: ContextMenuOverlayRect,
    parent_index: usize,
    child_count: usize,
    viewport_width: f32,
    viewport_height: f32,
) -> ContextMenuOverlayRect {
    let width = context_menu_width_for_viewport(viewport_width);
    let height = context_menu_height(child_count);
    let max_height = context_menu_max_height_for_viewport(viewport_height).min(height);
    let right_x = parent.x + parent.width - 1.0;
    let left_x = parent.x - width + 1.0;
    let right_edge_limit = (viewport_width - CONTEXT_MENU_VIEWPORT_MARGIN).max(0.0);
    let x = if right_x + width <= right_edge_limit {
        right_x
    } else {
        left_x
    };
    let parent_y =
        parent.y + CONTEXT_MENU_VERTICAL_PADDING + parent_index as f32 * CONTEXT_MENU_ROW_HEIGHT;
    ContextMenuOverlayRect {
        x: clamp_menu_axis(x, width, viewport_width),
        y: clamp_menu_axis(parent_y, max_height, viewport_height),
        width,
        max_height,
    }
}

fn context_menu_height(row_count: usize) -> f32 {
    CONTEXT_MENU_VERTICAL_PADDING * 2.0 + row_count as f32 * CONTEXT_MENU_ROW_HEIGHT
}

fn context_menu_width_for_viewport(viewport_width: f32) -> f32 {
    (viewport_width - CONTEXT_MENU_VIEWPORT_MARGIN * 2.0)
        .max(1.0)
        .min(CONTEXT_MENU_WIDTH)
}

fn context_menu_max_height_for_viewport(viewport_height: f32) -> f32 {
    (viewport_height - CONTEXT_MENU_VIEWPORT_MARGIN * 2.0).max(1.0)
}

fn clamp_menu_axis(position: f32, size: f32, viewport_size: f32) -> f32 {
    let min = CONTEXT_MENU_VIEWPORT_MARGIN.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - CONTEXT_MENU_VIEWPORT_MARGIN).max(min);
    position.clamp(min, max)
}

fn popup_menu_axis(anchor: f32, size: f32, viewport_size: f32) -> f32 {
    let min = CONTEXT_MENU_VIEWPORT_MARGIN.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - CONTEXT_MENU_VIEWPORT_MARGIN).max(min);
    let forward = anchor.clamp(min, max);
    if anchor + size <= viewport_size - CONTEXT_MENU_VIEWPORT_MARGIN {
        return forward;
    }
    let flipped = anchor - size;
    if flipped >= min {
        return flipped.min(max);
    }
    forward
}
