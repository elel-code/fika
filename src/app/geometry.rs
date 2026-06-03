use crate::{AppWindow, MenuGeometry};
use slint::ComponentHandle;
use std::ops::Range;

const SHELL_HEADER_HEIGHT: f32 = 56.0;
pub(crate) const PATH_BAR_HEIGHT: f32 = 56.0;
pub(crate) const STATUS_BAR_HEIGHT: f32 = 36.0;
const SPLIT_DIVIDER_WIDTH: f32 = 1.0;
const SEARCH_PANEL_WIDE_HEIGHT: f32 = 44.0;
const SEARCH_PANEL_NARROW_HEIGHT: f32 = 78.0;
const SEARCH_PANEL_NARROW_WIDTH: f32 = 760.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MainGridLayout {
    pub(crate) main_x: f32,
    pub(crate) main_y: f32,
    pub(crate) viewport_x: f32,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VirtualGridPlan {
    pub(crate) viewport_x: f32,
    pub(crate) scroll_max_x: f32,
    pub(crate) range: Range<usize>,
    pub(crate) visible_range: Range<usize>,
    pub(crate) start_column: usize,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MainPaneBounds {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
}

impl MainGridLayout {
    pub(crate) fn from_ui(ui: &AppWindow) -> Self {
        let cell_width = icon_cell_width(ui.get_icon_zoom_level());
        let row_height = icon_row_height(ui.get_icon_zoom_level());
        let padding = 14.0;
        let window_size = ui.window().size().to_logical(ui.window().scale_factor());
        let pane = main_pane_bounds(
            ui.get_sidebar_width_px(),
            window_size.width,
            window_size.height,
        );
        let active_width = active_main_pane_width(
            pane.right - pane.left,
            ui.get_split_view_open(),
            ui.get_split_pane_ratio(),
        );
        let search_panel_height = search_panel_height(
            ui.get_search_bar_open(),
            ui.get_search_query().as_str(),
            ui.get_search_kind_filter(),
            ui.get_search_modified_filter(),
            ui.get_search_size_filter(),
            active_width,
        );
        let available_grid_height = (pane.bottom
            - pane.top
            - PATH_BAR_HEIGHT
            - STATUS_BAR_HEIGHT
            - search_panel_height
            - 2.0 * padding)
            .max(row_height);
        let rows_per_column = (available_grid_height / row_height).floor().max(1.0) as usize;

        Self {
            main_x: pane.left,
            main_y: pane.top + PATH_BAR_HEIGHT + search_panel_height,
            viewport_x: ui.get_main_viewport_x(),
            rows_per_column,
            cell_width,
            row_height,
            padding,
        }
    }

    pub(crate) fn index_at_point(self, x: f32, y: f32) -> Option<usize> {
        let local_x = x - self.main_x - self.padding + self.viewport_x;
        let local_y = y - self.main_y - self.padding;
        if local_x < 0.0 || local_y < 0.0 {
            return None;
        }

        let column = (local_x / self.cell_width).floor() as usize;
        let row = (local_y / self.row_height).floor() as usize;
        if row >= self.rows_per_column {
            return None;
        }

        let inside_tile_x = local_x - column as f32 * self.cell_width;
        if inside_tile_x > (self.cell_width - 12.0).max(1.0) {
            return None;
        }

        Some(column * self.rows_per_column + row)
    }
}

pub(crate) fn active_main_pane_width(
    main_pane_width: f32,
    split_open: bool,
    split_pane_ratio: f32,
) -> f32 {
    let main_pane_width = main_pane_width.max(1.0);
    if split_open {
        let content_width = split_content_width(main_pane_width);
        let min_width = split_pane_min_width(content_width);
        let ratio_width = (content_width * clamped_split_pane_ratio(split_pane_ratio))
            .floor()
            .max(1.0);
        ratio_width.min(content_width - min_width).max(min_width)
    } else {
        main_pane_width
    }
}

pub(crate) fn inactive_main_pane_width(
    main_pane_width: f32,
    split_open: bool,
    split_pane_ratio: f32,
) -> f32 {
    if !split_open {
        return 0.0;
    }
    let main_pane_width = main_pane_width.max(1.0);
    let active_width = active_main_pane_width(main_pane_width, true, split_pane_ratio);
    (main_pane_width - active_width - SPLIT_DIVIDER_WIDTH).max(1.0)
}

pub(crate) fn clamped_split_pane_ratio(split_pane_ratio: f32) -> f32 {
    if split_pane_ratio.is_finite() {
        split_pane_ratio.clamp(0.1, 0.9)
    } else {
        0.5
    }
}

fn split_content_width(main_pane_width: f32) -> f32 {
    (main_pane_width.max(1.0) - SPLIT_DIVIDER_WIDTH).max(1.0)
}

fn split_pane_min_width(content_width: f32) -> f32 {
    260.0_f32.min((content_width / 2.0).max(1.0))
}

pub(crate) fn main_pane_bounds(
    sidebar_width_px: f32,
    window_width: f32,
    window_height: f32,
) -> MainPaneBounds {
    MainPaneBounds {
        left: sidebar_width_px,
        top: SHELL_HEADER_HEIGHT,
        right: window_width.max(sidebar_width_px),
        bottom: window_height.max(SHELL_HEADER_HEIGHT),
    }
}

pub(crate) fn virtual_grid_plan(
    entry_count: usize,
    rows_per_column: usize,
    requested_viewport_x: f32,
    viewport_width: f32,
    cell_width: f32,
    padding: f32,
    overscan_columns: usize,
) -> VirtualGridPlan {
    let scroll_max_x = main_scroll_max_x(
        entry_count,
        rows_per_column,
        viewport_width,
        cell_width,
        padding,
    );
    let viewport_x = requested_viewport_x.clamp(0.0, scroll_max_x);
    let range = virtual_entry_range(
        entry_count,
        rows_per_column,
        viewport_x,
        viewport_width,
        cell_width,
        padding,
        overscan_columns,
    );
    let visible_range = virtual_entry_range(
        entry_count,
        rows_per_column,
        viewport_x,
        viewport_width,
        cell_width,
        padding,
        0,
    );
    let start_column = range.start / rows_per_column.max(1);

    VirtualGridPlan {
        viewport_x,
        scroll_max_x,
        range,
        visible_range,
        start_column,
        rows_per_column,
        cell_width,
    }
}

pub(crate) fn split_preview_plan(
    entry_count: usize,
    pane_width: f32,
    pane_height: f32,
    requested_viewport_x: f32,
    zoom_level: i32,
) -> VirtualGridPlan {
    let cell_width = icon_cell_width(zoom_level);
    let row_height = icon_row_height(zoom_level);
    let padding = 14.0;
    let available_height = (pane_height - 2.0 * padding).max(row_height);
    let rows_per_column = (available_height / row_height).floor().max(1.0) as usize;

    virtual_grid_plan(
        entry_count,
        rows_per_column,
        requested_viewport_x,
        pane_width.max(1.0),
        cell_width,
        padding,
        2,
    )
}

pub(crate) fn icon_cell_width(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 172.0,
        1 => 208.0,
        2 => 248.0,
        3 => 292.0,
        _ => 340.0,
    }
}

pub(crate) fn icon_row_height(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 78.0,
        1 => 90.0,
        2 => 104.0,
        3 => 124.0,
        _ => 146.0,
    }
}

pub(crate) fn search_panel_height(
    search_bar_open: bool,
    search_query: &str,
    search_kind_filter: i32,
    search_modified_filter: i32,
    search_size_filter: i32,
    main_pane_width: f32,
) -> f32 {
    let filters_active =
        search_kind_filter != 0 || search_modified_filter != 0 || search_size_filter != 0;
    if search_bar_open || !search_query.is_empty() || filters_active {
        if main_pane_width < SEARCH_PANEL_NARROW_WIDTH {
            SEARCH_PANEL_NARROW_HEIGHT
        } else {
            SEARCH_PANEL_WIDE_HEIGHT
        }
    } else {
        0.0
    }
}

pub(crate) fn virtual_entry_range(
    entry_count: usize,
    rows_per_column: usize,
    viewport_x: f32,
    viewport_width: f32,
    cell_width: f32,
    padding: f32,
    overscan_columns: usize,
) -> Range<usize> {
    if entry_count == 0 {
        return 0..0;
    }

    let rows_per_column = rows_per_column.max(1);
    let cell_width = cell_width.max(1.0);
    let viewport_x = viewport_x.max(0.0);
    let viewport_width = viewport_width.max(1.0);
    let content_x = (viewport_x - padding.max(0.0)).max(0.0);
    let content_end_x = (viewport_x + viewport_width - padding.max(0.0)).max(content_x + 1.0);
    let first_visible_column = (content_x / cell_width).floor() as usize;
    let visible_end_column = (content_end_x / cell_width)
        .ceil()
        .max(first_visible_column as f32 + 1.0) as usize;
    let start_column = first_visible_column.saturating_sub(overscan_columns);
    let end_column = visible_end_column + overscan_columns;

    let start = (start_column * rows_per_column).min(entry_count);
    let end = (end_column * rows_per_column).min(entry_count);
    start..end.max(start)
}

pub(crate) fn main_scroll_max_x(
    entry_count: usize,
    rows_per_column: usize,
    viewport_width: f32,
    cell_width: f32,
    padding: f32,
) -> f32 {
    let rows_per_column = rows_per_column.max(1);
    let cell_width = cell_width.max(1.0);
    let column_count = entry_count.div_ceil(rows_per_column).max(1);
    let content_width = 2.0 * padding.max(0.0) + column_count as f32 * cell_width;
    (content_width - viewport_width.max(1.0)).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MenuMetricsInput {
    pub(crate) kind: i32,
    pub(crate) selected_count: i32,
    pub(crate) is_dir: bool,
    pub(crate) default_open_visible: bool,
    pub(crate) add_to_places_visible: bool,
    pub(crate) clipboard_has_paths: bool,
    pub(crate) in_trash: bool,
    pub(crate) place_builtin: bool,
    pub(crate) device_mounted: bool,
    pub(crate) device_pending: bool,
    pub(crate) device_can_mount: bool,
    pub(crate) device_can_unmount: bool,
    pub(crate) device_can_eject: bool,
    pub(crate) item_height: f32,
    pub(crate) separator_height: f32,
    pub(crate) title_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MenuMetrics {
    pub(crate) height: f32,
    pub(crate) open_with_row_y_offset: f32,
    pub(crate) create_new_row_y_offset: f32,
}

pub(crate) fn context_menu_metrics(input: MenuMetricsInput) -> MenuMetrics {
    let item = input.item_height.max(1.0);
    let separator = input.separator_height.max(0.0);
    let title = input.title_height.max(0.0);

    match input.kind {
        1 => file_context_menu_metrics(input, item, separator, title),
        2 => MenuMetrics {
            height: title
                + item
                + if input.place_builtin {
                    0.0
                } else {
                    separator + 2.0 * item
                },
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        },
        3 => viewport_context_menu_metrics(input, item, separator),
        4 => MenuMetrics {
            height: title + 2.0 * item + separator,
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        },
        5 => MenuMetrics {
            height: device_context_menu_height(input, item, separator, title),
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        },
        _ => MenuMetrics {
            height: 0.0,
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        },
    }
}

pub(crate) fn register_menu_geometry_callbacks(ui: &AppWindow) {
    let menu_geometry = ui.global::<MenuGeometry>();

    menu_geometry.on_root_menu_left(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap| {
            RootMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
            }
            .popup()
            .x
        },
    );

    menu_geometry.on_root_menu_top(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap| {
            RootMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
            }
            .popup()
            .y
        },
    );

    menu_geometry.on_anchored_menu_left(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap,
         gap| {
            AnchoredMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
                gap,
            }
            .popup()
            .x
        },
    );

    menu_geometry.on_anchored_menu_top(
        |view_width,
         view_height,
         anchor_x,
         anchor_y,
         menu_width,
         menu_height,
         margin,
         pointer_gap,
         gap| {
            AnchoredMenuGeometry {
                view_width,
                view_height,
                anchor_x,
                anchor_y,
                menu_width,
                menu_height,
                margin,
                pointer_gap,
                gap,
            }
            .popup()
            .y
        },
    );

    menu_geometry.on_child_menu_left(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         row_y,
         child_width,
         child_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildMenuGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                row_y,
                child_width,
                child_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .popup()
            .x
        },
    );

    menu_geometry.on_child_menu_top(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         row_y,
         child_width,
         child_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildMenuGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                row_y,
                child_width,
                child_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .popup()
            .y
        },
    );

    menu_geometry.on_child_bridge_left(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .x
        },
    );

    menu_geometry.on_child_bridge_top(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .y
        },
    );

    menu_geometry.on_child_bridge_width(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .width
        },
    );

    menu_geometry.on_child_bridge_height(
        |view_width,
         view_height,
         parent_left,
         parent_width,
         child_left,
         child_width,
         row_y,
         child_top,
         row_height,
         title_height,
         margin,
         pointer_gap,
         child_gap| {
            ChildBridgeGeometry {
                view_width,
                view_height,
                parent_left,
                parent_width,
                child_left,
                child_width,
                row_y,
                child_top,
                row_height,
                title_height,
                margin,
                pointer_gap,
                child_gap,
            }
            .rect()
            .height
        },
    );

    macro_rules! register_context_metric_callback {
        ($method:ident, $field:ident) => {
            menu_geometry.$method(
                |kind,
                 selected_count,
                 is_dir,
                 default_open_visible,
                 add_to_places_visible,
                 clipboard_has_paths,
                 in_trash,
                 place_builtin,
                 device_mounted,
                 device_pending,
                 device_can_mount,
                 device_can_unmount,
                 device_can_eject,
                 item_height,
                 separator_height,
                 title_height| {
                    context_menu_metrics(MenuMetricsInput {
                        kind,
                        selected_count,
                        is_dir,
                        default_open_visible,
                        add_to_places_visible,
                        clipboard_has_paths,
                        in_trash,
                        place_builtin,
                        device_mounted,
                        device_pending,
                        device_can_mount,
                        device_can_unmount,
                        device_can_eject,
                        item_height,
                        separator_height,
                        title_height,
                    })
                    .$field
                },
            );
        };
    }

    register_context_metric_callback!(on_context_menu_height, height);
    register_context_metric_callback!(on_context_menu_open_with_row_offset, open_with_row_y_offset);
    register_context_metric_callback!(
        on_context_menu_create_new_row_offset,
        create_new_row_y_offset
    );
}

fn device_context_menu_height(
    input: MenuMetricsInput,
    item: f32,
    separator: f32,
    title: f32,
) -> f32 {
    if input.device_pending {
        return title + item;
    }

    let open_rows = i32::from(input.device_mounted)
        + i32::from(!input.device_mounted && input.device_can_mount)
        + i32::from(input.device_can_unmount)
        + i32::from(input.device_can_eject);
    if open_rows == 0 {
        return title + item;
    }

    title
        + open_rows as f32 * item
        + if input.device_can_unmount || input.device_can_eject {
            separator
        } else {
            0.0
        }
}

fn file_context_menu_metrics(
    input: MenuMetricsInput,
    item: f32,
    separator: f32,
    title: f32,
) -> MenuMetrics {
    if input.selected_count > 1 {
        let action_rows = if input.in_trash { 4.0 } else { 3.0 };
        return MenuMetrics {
            height: title + action_rows * item + separator,
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        };
    }

    let mut item_count = if input.is_dir {
        9 + i32::from(!input.in_trash) + input.add_to_places_visible as i32
    } else {
        8 + input.default_open_visible as i32
    };
    if input.in_trash {
        item_count += 1;
    }
    MenuMetrics {
        height: item_count as f32 * item + 2.0 * separator,
        open_with_row_y_offset: if input.is_dir {
            0.0
        } else if input.default_open_visible {
            item
        } else {
            0.0
        },
        create_new_row_y_offset: 0.0,
    }
}

fn viewport_context_menu_metrics(
    input: MenuMetricsInput,
    item: f32,
    separator: f32,
) -> MenuMetrics {
    if input.in_trash {
        return MenuMetrics {
            height: 2.0 * item + separator,
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        };
    }

    MenuMetrics {
        height: 5.0 * item + separator,
        open_with_row_y_offset: 3.0 * item + separator,
        create_new_row_y_offset: 0.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlaceDropGeometry {
    pub(crate) target_index: i32,
    pub(crate) slot: i32,
    pub(crate) row_offset: f32,
    pub(crate) over_gap: bool,
    pub(crate) over_item: bool,
}

pub(crate) fn place_drop_geometry(
    y: f32,
    place_count: usize,
    place_list_y: f32,
    place_row_stride: f32,
) -> PlaceDropGeometry {
    let place_row_stride = place_row_stride.max(1.0);
    let local_y = y - place_list_y;
    if place_count == 0 || local_y <= 0.0 {
        return PlaceDropGeometry {
            target_index: if place_count == 0 { -1 } else { 0 },
            slot: 0,
            row_offset: 0.0,
            over_gap: true,
            over_item: false,
        };
    }

    let list_height = place_count as f32 * place_row_stride;
    if local_y >= list_height {
        return PlaceDropGeometry {
            target_index: place_count as i32 - 1,
            slot: place_count as i32,
            row_offset: place_row_stride,
            over_gap: true,
            over_item: false,
        };
    }

    let row = (local_y / place_row_stride).floor();
    let row_offset = local_y - row * place_row_stride;
    let target_index = row.max(0.0).min(place_count.saturating_sub(1) as f32) as i32;
    let over_gap = row_offset < 6.0 || row_offset > (place_row_stride - 6.0);
    let over_item = !over_gap && target_index >= 0 && target_index < place_count as i32;
    let slot = (target_index + (row_offset > place_row_stride / 2.0) as i32)
        .max(0)
        .min(place_count as i32);

    PlaceDropGeometry {
        target_index,
        slot,
        row_offset,
        over_gap,
        over_item,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PopupPoint {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub(crate) struct PopupRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PopupPlacement {
    pub(crate) safe_min: f32,
    pub(crate) safe_max_x: f32,
    pub(crate) safe_max_y: f32,
    pub(crate) pointer_gap: f32,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct ChildPopupInput {
    pub(crate) parent_left: f32,
    pub(crate) parent_width: f32,
    pub(crate) row_y: f32,
    pub(crate) child_width: f32,
    pub(crate) child_height: f32,
    pub(crate) child_gap: f32,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct HoverBridgeInput {
    pub(crate) parent_left: f32,
    pub(crate) parent_width: f32,
    pub(crate) child_left: f32,
    pub(crate) child_width: f32,
    pub(crate) row_y: f32,
    pub(crate) child_top: f32,
    pub(crate) row_height: f32,
    pub(crate) title_height: f32,
    pub(crate) child_gap: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RootMenuGeometry {
    pub(crate) view_width: f32,
    pub(crate) view_height: f32,
    pub(crate) anchor_x: f32,
    pub(crate) anchor_y: f32,
    pub(crate) menu_width: f32,
    pub(crate) menu_height: f32,
    pub(crate) margin: f32,
    pub(crate) pointer_gap: f32,
}

impl RootMenuGeometry {
    pub(crate) fn popup(self) -> PopupPoint {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .root_popup(
            self.anchor_x,
            self.anchor_y,
            self.menu_width,
            self.menu_height,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AnchoredMenuGeometry {
    pub(crate) view_width: f32,
    pub(crate) view_height: f32,
    pub(crate) anchor_x: f32,
    pub(crate) anchor_y: f32,
    pub(crate) menu_width: f32,
    pub(crate) menu_height: f32,
    pub(crate) margin: f32,
    pub(crate) pointer_gap: f32,
    pub(crate) gap: f32,
}

impl AnchoredMenuGeometry {
    pub(crate) fn popup(self) -> PopupPoint {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .anchored_popup_above(
            self.anchor_x,
            self.anchor_y,
            self.menu_width,
            self.menu_height,
            self.gap,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChildMenuGeometry {
    pub(crate) view_width: f32,
    pub(crate) view_height: f32,
    pub(crate) parent_left: f32,
    pub(crate) parent_width: f32,
    pub(crate) row_y: f32,
    pub(crate) child_width: f32,
    pub(crate) child_height: f32,
    pub(crate) margin: f32,
    pub(crate) pointer_gap: f32,
    pub(crate) child_gap: f32,
}

impl ChildMenuGeometry {
    pub(crate) fn popup(self) -> PopupPoint {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .child_popup(ChildPopupInput {
            parent_left: self.parent_left,
            parent_width: self.parent_width,
            row_y: self.row_y,
            child_width: self.child_width,
            child_height: self.child_height,
            child_gap: self.child_gap,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ChildBridgeGeometry {
    pub(crate) view_width: f32,
    pub(crate) view_height: f32,
    pub(crate) parent_left: f32,
    pub(crate) parent_width: f32,
    pub(crate) child_left: f32,
    pub(crate) child_width: f32,
    pub(crate) row_y: f32,
    pub(crate) child_top: f32,
    pub(crate) row_height: f32,
    pub(crate) title_height: f32,
    pub(crate) margin: f32,
    pub(crate) pointer_gap: f32,
    pub(crate) child_gap: f32,
}

impl ChildBridgeGeometry {
    pub(crate) fn rect(self) -> PopupRect {
        PopupPlacement::new(
            self.view_width,
            self.view_height,
            self.margin,
            self.pointer_gap,
        )
        .hover_bridge(HoverBridgeInput {
            parent_left: self.parent_left,
            parent_width: self.parent_width,
            child_left: self.child_left,
            child_width: self.child_width,
            row_y: self.row_y,
            child_top: self.child_top,
            row_height: self.row_height,
            title_height: self.title_height,
            child_gap: self.child_gap,
        })
    }
}

impl PopupPlacement {
    pub(crate) fn new(view_width: f32, view_height: f32, margin: f32, pointer_gap: f32) -> Self {
        let safe_min = margin.max(0.0);
        Self {
            safe_min,
            safe_max_x: safe_min.max(view_width - safe_min),
            safe_max_y: safe_min.max(view_height - safe_min),
            pointer_gap: pointer_gap.max(0.0),
        }
    }

    pub(crate) fn root_popup(
        self,
        anchor_x: f32,
        anchor_y: f32,
        width: f32,
        height: f32,
    ) -> PopupPoint {
        PopupPoint {
            x: self.root_axis(anchor_x, width, self.safe_max_x),
            y: self.root_axis(anchor_y, height, self.safe_max_y),
        }
    }

    pub(crate) fn anchored_popup_above(
        self,
        anchor_x: f32,
        anchor_y: f32,
        width: f32,
        height: f32,
        gap: f32,
    ) -> PopupPoint {
        PopupPoint {
            x: clamp_popup(anchor_x, width, self.safe_min, self.safe_max_x),
            y: clamp_popup(
                anchor_y - height - gap.max(0.0),
                height,
                self.safe_min,
                self.safe_max_y,
            ),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn child_popup(self, input: ChildPopupInput) -> PopupPoint {
        PopupPoint {
            x: self.child_axis(
                input.parent_left,
                input.parent_width,
                input.child_width,
                input.child_gap,
                self.safe_max_x,
            ),
            y: clamp_popup(
                input.row_y,
                input.child_height,
                self.safe_min,
                self.safe_max_y,
            ),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn hover_bridge(self, input: HoverBridgeInput) -> PopupRect {
        let (x, width) = if input.child_left < input.parent_left {
            (
                input.child_left + input.child_width,
                (input.parent_left - (input.child_left + input.child_width)).max(input.child_gap),
            )
        } else {
            (
                input.parent_left + input.parent_width,
                (input.child_left - (input.parent_left + input.parent_width)).max(input.child_gap),
            )
        };
        let y = self.safe_min.max(input.row_y.min(input.child_top) - 4.0);
        let bottom = self.safe_max_y.min(
            (input.row_y + input.row_height)
                .max(input.child_top + input.title_height + input.row_height)
                + 4.0,
        );

        PopupRect {
            x,
            width,
            y,
            height: (bottom - y).max(input.row_height + 8.0),
        }
    }

    fn root_axis(self, anchor: f32, popup_size: f32, safe_max: f32) -> f32 {
        let preferred = if anchor + popup_size + self.pointer_gap <= safe_max {
            anchor + self.pointer_gap
        } else {
            anchor - popup_size - self.pointer_gap
        };
        clamp_popup(preferred, popup_size, self.safe_min, safe_max)
    }

    #[allow(dead_code)]
    fn child_axis(
        self,
        parent_start: f32,
        parent_size: f32,
        popup_size: f32,
        gap: f32,
        safe_max: f32,
    ) -> f32 {
        let preferred = if parent_start + parent_size + gap + popup_size <= safe_max {
            parent_start + parent_size + gap
        } else {
            parent_start - popup_size - gap
        };
        clamp_popup(preferred, popup_size, self.safe_min, safe_max)
    }
}

pub(crate) fn clamp_popup(position: f32, popup_size: f32, safe_min: f32, safe_max: f32) -> f32 {
    safe_min.max(position.min(safe_max - popup_size))
}

#[cfg(test)]
mod tests {
    use super::{
        AnchoredMenuGeometry, ChildBridgeGeometry, ChildMenuGeometry, ChildPopupInput,
        HoverBridgeInput, MenuMetricsInput, PlaceDropGeometry, PopupPlacement, PopupPoint,
        PopupRect, RootMenuGeometry, SHELL_HEADER_HEIGHT, active_main_pane_width,
        context_menu_metrics, inactive_main_pane_width, main_pane_bounds, main_scroll_max_x,
        place_drop_geometry, search_panel_height, split_preview_plan, virtual_entry_range,
        virtual_grid_plan,
    };

    const MENU_ITEM_HEIGHT: f32 = 38.0;
    const MENU_SEPARATOR_HEIGHT: f32 = 8.0;
    const MENU_TITLE_HEIGHT: f32 = 30.0;

    #[test]
    fn menu_geometry_callbacks_are_global_owned() {
        let app = include_str!("../../ui/app.slint");
        let menus = include_str!("../../ui/menus.slint");
        let menu_geometry = include_str!("../../ui/menu_geometry.slint");
        let callbacks = [
            "root_menu_left",
            "root_menu_top",
            "anchored_menu_left",
            "anchored_menu_top",
            "child_menu_left",
            "child_menu_top",
            "child_bridge_left",
            "child_bridge_top",
            "child_bridge_width",
            "child_bridge_height",
            "context_menu_height",
            "context_menu_open_with_row_offset",
            "context_menu_create_new_row_offset",
        ];

        assert!(app.contains("export { MenuGeometry } from \"menu_geometry.slint\";"));
        assert!(menus.contains("import { MenuGeometry } from \"menu_geometry.slint\";"));
        assert!(menu_geometry.contains("export global MenuGeometry"));

        for callback in callbacks {
            assert!(
                menu_geometry.contains(&format!("callback {callback}(")),
                "MenuGeometry should declare {callback}"
            );
            assert!(
                !app.contains(&format!("callback {callback}(")),
                "AppWindow should not declare {callback}"
            );
            assert!(
                !app.contains(&format!("root.{callback}(")),
                "AppWindow should not forward {callback}"
            );
            assert!(
                menus.contains(&format!("MenuGeometry.{callback}(")),
                "menus.slint should consume {callback} through MenuGeometry"
            );
        }
    }

    #[test]
    fn menu_lifecycle_state_is_global_owned() {
        let app = include_str!("../../ui/app.slint");
        let menu_lifecycle = include_str!("../../ui/menu_lifecycle.slint");
        let state_properties = [
            ("bool", "open-with-open"),
            ("length", "open-with-row-y"),
            ("bool", "create-new-open"),
            ("length", "create-new-row-y"),
            ("int", "close-kind"),
        ];
        let lifecycle_functions = [
            "cancel-close",
            "close-child-submenu",
            "close-child-submenus",
            "begin-close",
            "show-open-with",
            "show-create-new",
            "close-pending-child-submenu",
        ];
        let controller_functions = [
            "stop-close-timer",
            "close-child-submenus",
            "set-child-submenu-hover",
            "show-child-submenu",
            "show-open-with-submenu",
            "show-create-new-submenu",
            "open-with-submenu-hover",
            "create-new-submenu-hover",
        ];

        assert!(app.contains("export { MenuLifecycle } from \"menu_lifecycle.slint\";"));
        assert!(app.contains(
            "import { MenuLifecycle, MenuLifecycleController } from \"menu_lifecycle.slint\";"
        ));
        assert!(menu_lifecycle.contains("export global MenuLifecycle"));
        assert!(menu_lifecycle.contains("export component MenuLifecycleController"));

        for (kind, property) in state_properties {
            assert!(
                menu_lifecycle.contains(&format!("property <{kind}> {property}")),
                "MenuLifecycle should own {property}"
            );
            assert!(
                !app.contains(&format!("private property <bool> {property}")),
                "AppWindow should not own {property}"
            );
            assert!(
                !app.contains(&format!("private property <length> {property}")),
                "AppWindow should not own {property}"
            );
            assert!(
                !app.contains(&format!("private property <int> {property}")),
                "AppWindow should not own {property}"
            );
        }

        for function in lifecycle_functions {
            assert!(
                menu_lifecycle.contains(&format!("public function {function}(")),
                "MenuLifecycle should expose {function}"
            );
            assert!(
                !app.contains(&format!("MenuLifecycle.{function}(")),
                "AppWindow should not mutate low-level MenuLifecycle state through {function}"
            );
        }

        for function in controller_functions {
            assert!(
                menu_lifecycle.contains(&format!("public function {function}(")),
                "MenuLifecycleController should expose {function}"
            );
        }
        for function in [
            "close-child-submenus",
            "set-child-submenu-hover",
            "show-open-with-submenu",
            "show-create-new-submenu",
            "open-with-submenu-hover",
            "create-new-submenu-hover",
        ] {
            assert!(
                app.contains(&format!("menu-lifecycle.{function}(")),
                "AppWindow should route {function} through MenuLifecycleController"
            );
        }
        assert!(
            menu_lifecycle.contains("close-timer := Timer"),
            "MenuLifecycleController should own the delayed-close Timer"
        );
        assert!(
            !app.contains("child-submenu-close-timer"),
            "AppWindow should not own the child submenu close timer"
        );
        assert!(
            !app.contains("interval: 240ms"),
            "AppWindow should not own the child submenu delayed-close interval"
        );
        assert!(
            app.contains("menu-lifecycle := MenuLifecycleController"),
            "AppWindow should instantiate the menu lifecycle controller"
        );
    }

    #[test]
    fn context_menu_rows_are_componentized_in_menus_layer() {
        let app = include_str!("../../ui/app.slint");
        let menus = include_str!("../../ui/menus.slint");

        for component in [
            "ActionMenuRow",
            "HoverActionMenuRow",
            "SubmenuMenuRow",
            "PasteMenuRow",
            "CutCopyMenuRows",
        ] {
            assert!(
                menus.contains(&format!("component {component} inherits Rectangle")),
                "menus.slint should keep reusable {component} row component"
            );
            assert!(
                !app.contains(&format!("{component} {{")),
                "AppWindow should not compose low-level menu row components"
            );
        }

        assert_eq!(
            menus.matches("submenu: true;").count(),
            1,
            "submenu indicator wiring should live in SubmenuMenuRow instead of being repeated"
        );
        assert_eq!(
            menus.matches("MenuItem {").count(),
            3,
            "raw MenuItem usage should stay limited to ActionMenuRow, HoverActionMenuRow, and SubmenuMenuRow"
        );
        assert_eq!(
            menus.matches("shortcut: \"Ctrl+V\";").count(),
            1,
            "Paste shortcut wiring should live in PasteMenuRow instead of being repeated"
        );
        assert_eq!(
            menus.matches("shortcut: \"Ctrl+X\";").count(),
            1,
            "Cut shortcut wiring should live in CutCopyMenuRows instead of being repeated"
        );
        assert_eq!(
            menus.matches("shortcut: \"Ctrl+C\";").count(),
            1,
            "Copy shortcut wiring should live in CutCopyMenuRows instead of being repeated"
        );
        assert_eq!(
            menus.matches("SubmenuMenuRow {").count(),
            4,
            "file and viewport menus should reuse the submenu row for Open With/Create New entries"
        );
        assert_eq!(
            menus.matches("PasteMenuRow {").count(),
            2,
            "file and viewport menus should reuse the paste row"
        );
        assert_eq!(
            menus.matches("CutCopyMenuRows {").count(),
            2,
            "single and multi-selection file menus should reuse the cut/copy row group"
        );
        assert!(
            menus.matches("ActionMenuRow {").count() >= 30,
            "ordinary menu actions should use ActionMenuRow instead of duplicating raw MenuItem wiring"
        );
        assert!(
            !app.contains("MenuItem {"),
            "AppWindow should not regain direct context-menu row layout"
        );
    }

    #[test]
    fn child_submenu_delayed_close_is_limited_to_child_menu_paths() {
        let app = include_str!("../../ui/app.slint");
        let menus = include_str!("../../ui/menus.slint");
        let menu_lifecycle = include_str!("../../ui/menu_lifecycle.slint");

        let action_row_start = menus
            .find("component ActionMenuRow")
            .expect("ActionMenuRow should exist");
        let hover_action_row_start = menus
            .find("component HoverActionMenuRow")
            .expect("HoverActionMenuRow should exist after ActionMenuRow");
        let paste_row_start = menus
            .find("component PasteMenuRow")
            .expect("PasteMenuRow should exist after HoverActionMenuRow");
        let action_row = &menus[action_row_start..hover_action_row_start];
        let hover_action_row = &menus[hover_action_row_start..paste_row_start];
        assert!(
            !action_row.contains("callback hovered(bool);")
                && !action_row.contains("hovered(is-hovered) =>"),
            "ordinary ActionMenuRow must not participate in child-submenu keep-alive or delayed close"
        );
        assert!(
            hover_action_row.contains("callback hovered(bool);")
                && hover_action_row
                    .contains("hovered(is-hovered) => { root.hovered(is-hovered); }"),
            "HoverActionMenuRow should be the only ordinary action row variant that forwards passive hover"
        );

        let file_menu_start = menus
            .find("export component FileContextMenu")
            .expect("FileContextMenu should exist");
        let viewport_menu_start = menus
            .find("export component ViewportContextMenu")
            .expect("ViewportContextMenu should exist");
        let root_layer_start = menus
            .find("export component RootContextMenuLayer")
            .expect("RootContextMenuLayer should exist");
        let file_menu = &menus[file_menu_start..viewport_menu_start];
        let viewport_menu = &menus[viewport_menu_start..root_layer_start];
        let root_layer = &menus[root_layer_start..];

        assert_eq!(
            file_menu.matches("hovered(is-hovered) =>").count(),
            2,
            "file context menu hover should only be wired for Open Folder With and Open With submenu parents"
        );
        assert!(file_menu.contains("root.open_folder_with_hover(is-hovered);"));
        assert!(file_menu.contains("root.open_with_hover(is-hovered);"));
        assert!(!file_menu.contains("ActionMenuRow {\n            label: \"Open Terminal Here\";\n            dark: root.dark;\n            hovered"));
        assert!(!file_menu.contains("ActionMenuRow {\n            label: \"Rename\";\n            dark: root.dark;\n            hovered"));

        assert_eq!(
            viewport_menu.matches("hovered(is-hovered) =>").count(),
            2,
            "viewport context menu hover should only be wired for Create New and Open Folder With submenu parents"
        );
        assert!(viewport_menu.contains("root.create_new_hover(is-hovered);"));
        assert!(viewport_menu.contains("root.open_folder_with_hover(is-hovered);"));
        assert!(!viewport_menu.contains("ActionMenuRow {\n            label: \"Select All\";\n            shortcut: \"Ctrl+A\";\n            dark: root.dark;\n            hovered"));
        assert!(!viewport_menu.contains("ActionMenuRow {\n            label: \"Open Terminal Here\";\n            dark: root.dark;\n            hovered"));

        for callback in [
            "file_open_folder_with_hover",
            "file_open_with_hover",
            "viewport_create_new_hover",
            "viewport_open_folder_with_hover",
        ] {
            assert!(
                root_layer.contains(callback),
                "RootContextMenuLayer should keep submenu parent callback {callback}"
            );
        }
        assert!(
            !root_layer.contains("ActionMenuRow") && !root_layer.contains("MenuItem"),
            "RootContextMenuLayer should only forward menu events, not own row-level hover behavior"
        );
        assert!(
            app.contains("child_hovered(kind, is-hovered) => {\n            root.set-child-submenu-hover(kind, is-hovered);\n        }"),
            "ChildSubmenuLayer hover bridge/panel should be the route that keeps or closes a child submenu"
        );
        assert!(
            menu_lifecycle.contains("} else if (menu == 1 || menu == 2) {\n            MenuLifecycle.begin-close(menu);\n            close-timer.start();\n        }"),
            "delayed close should only start from explicit child submenu hover loss"
        );
        assert!(
            !file_menu.contains("HoverActionMenuRow {")
                && !viewport_menu.contains("HoverActionMenuRow {"),
            "root context menus should not use passive-hover action rows; only child submenu bodies should keep alive on ordinary row hover"
        );
    }

    #[test]
    fn chooser_popup_layers_share_anchored_popup_shell() {
        let menus = include_str!("../../ui/menus.slint");
        let anchored_component_start = menus
            .find("component AnchoredChooserPopup")
            .expect("AnchoredChooserPopup should exist");
        let choice_layer_start = menus
            .find("export component ChooserChoicePopupLayer")
            .expect("ChooserChoicePopupLayer should exist");
        assert!(
            menus.contains("export component ChooserOptionPopupLayer"),
            "ChooserOptionPopupLayer should still exist"
        );
        let anchored_component = &menus[anchored_component_start..choice_layer_start];
        let chooser_layers = &menus[choice_layer_start..];

        assert!(
            anchored_component.contains("MenuGeometry.anchored_menu_left("),
            "anchored chooser shell should own horizontal placement"
        );
        assert!(
            anchored_component.contains("MenuGeometry.anchored_menu_top("),
            "anchored chooser shell should own vertical placement"
        );
        assert_eq!(
            chooser_layers
                .matches("MenuGeometry.anchored_menu_")
                .count(),
            0,
            "chooser layers should delegate placement to AnchoredChooserPopup"
        );
        assert_eq!(
            chooser_layers.matches("AnchoredChooserPopup {").count(),
            2,
            "filter and choice chooser layers should both use the shared shell"
        );
    }

    #[test]
    fn cosmic_shell_chrome_separates_top_tools_from_main_path_bar() {
        let app = include_str!("../../ui/app.slint");
        let bars = include_str!("../../ui/top_bar.slint");
        let search_panel = include_str!("../../ui/search_panel.slint");
        let status_bar = include_str!("../../ui/status_bar.slint");
        let path_bar_marker = "export component PathBar inherits Rectangle";
        let (top_bar_component, path_bar_component) = bars
            .split_once(path_bar_marker)
            .map(|(top_bar, path_bar)| (top_bar, path_bar))
            .expect("top_bar.slint should export TopBar followed by PathBar");

        assert!(
            app.contains("private property <color> shell-base-color"),
            "AppWindow should own a shared shell base color"
        );
        assert!(
            app.contains("private property <color> sidebar-surface-color"),
            "AppWindow should own the raised sidebar surface color"
        );
        assert!(
            app.contains("private property <color> shell-separator-color"),
            "AppWindow should own one separator color for the shared shell"
        );
        assert!(
            app.contains(
                "private property <length> main-content-left: root.sidebar_width_px * 1px;"
            ),
            "AppWindow should expose the sidebar panel right edge as the main content edge"
        );
        assert!(
            app.contains("private property <length> main-pane-width: max(1px, root.width - root.main-content-left);"),
            "main pane width should derive from the same content edge as the header"
        );
        assert!(
            app.contains("private property <length> sidebar-resize-hit-width: 8px;"),
            "sidebar resize should keep a transparent edge hit area without taking layout width"
        );
        assert!(
            app.contains("private property <string> sidebar-selected-path: root.focused_pane == 1 && root.split_view_open ? root.inactive_pane_path : root.left_pane_path;")
                && app.contains("selected: root.sidebar-selected-path == place.path;")
                && app.contains("selected: root.sidebar-selected-path == device.path;")
                && !app.contains("selected: root.current_path == place.path;")
                && !app.contains("selected: root.current_path == device.path;"),
            "sidebar places/devices highlight should follow the focused pane path immediately"
        );
        assert!(
            app.contains("shell-layout := Rectangle"),
            "AppWindow should own one explicit shell surface"
        );
        assert!(
            app.contains("private property <length> shell-header-height: 56px;")
                && app.contains("shell-header := Rectangle")
                && app.contains("content-row := HorizontalLayout"),
            "AppWindow should keep a shell/header row separate from the main-pane content"
        );
        assert!(
            !app.contains("sidebar-splitter-width")
                && !app.contains("sidebar-divider-offset")
                && app.contains("resize-touch := TouchArea"),
            "sidebar panel border should be the visible divider instead of a separate splitter line"
        );
        assert!(
            app.matches("background: root.shell-base-color;").count() >= 3,
            "window, main pane, and empty view should share the shell base"
        );
        assert!(
            !app.contains("sidebar-foreground := Rectangle"),
            "sidebar panel should not be a window-level full-height foreground overlay"
        );
        assert!(
            app.contains("sidebar-surface := Rectangle"),
            "sidebar panel should be explicit in the content row below the shell header"
        );
        assert!(
            app.contains("private property <length> sidebar-bottom-gap: 14px;")
                && app.contains(
                    "private property <length> sidebar-content-bottom-padding: 22px;"
                )
                && app.contains("private property <length> sidebar-content-height: 10px + 30px + root.places.length * 38px + 30px + 30px + root.devices.length * 38px + (root.places.length + root.devices.length + 2) * 4px + root.sidebar-content-bottom-padding;"),
            "sidebar should define explicit outer and inner bottom spacing"
        );
        let sidebar_content = app
            .split_once("sidebar-surface := Rectangle {")
            .expect("sidebar content panel should be present")
            .1
            .split_once("places-folder-drop := DropArea {")
            .expect("sidebar list should be before the places drop area")
            .0;
        assert!(
            sidebar_content.contains("width: parent.width;")
                && sidebar_content
                    .contains("height: max(1px, parent.height - root.sidebar-bottom-gap);")
                && sidebar_content
                    .contains("viewport-height: max(parent.height, root.sidebar-content-height);")
                && sidebar_content
                    .contains("padding-bottom: root.sidebar-content-bottom-padding;")
                && !sidebar_content.contains("Rectangle { vertical-stretch: 1; }")
                && !app.contains(
                    "viewport-height: max(parent.height, 480px + (root.places.length + root.devices.length) * 38px);"
                ),
            "sidebar panel should leave a visible bottom gap and avoid stretching the list to fill the window"
        );
        assert!(
            !app.contains("changed main_viewport_x => { root.main_view_changed(); }"),
            "main viewport scrolling should not separately trigger a duplicate virtual refresh"
        );
        assert!(
            status_bar.contains("label: \"Admin Save\";")
                && status_bar.contains("width: 104px;")
                && !status_bar.contains("label: \"Save Back\";"),
            "status bar should expose the clearer admin write-back save action"
        );
        let main_pane_index = app
            .find("main-pane := Rectangle")
            .expect("AppWindow should instantiate the main pane");
        let shell_header_index = app
            .find("shell-header := Rectangle")
            .expect("AppWindow should reserve the top shell/header row");
        let top_bar_index = app[shell_header_index..]
            .find("TopBar {")
            .map(|index| shell_header_index + index)
            .expect("AppWindow should instantiate the global shell TopBar");
        let content_row_index = app
            .find("content-row := HorizontalLayout")
            .expect("AppWindow should place sidebar and main pane below the shell header");
        let pane_shells_index = app
            .find("pane-shells := Rectangle")
            .expect("AppWindow should instantiate the reusable file panes");
        let sidebar_surface_index = app
            .find("sidebar-surface := Rectangle")
            .expect("sidebar content panel should be present");
        assert!(
            shell_header_index < content_row_index
                && shell_header_index < top_bar_index
                && top_bar_index < content_row_index
                && content_row_index < sidebar_surface_index
                && content_row_index < main_pane_index
                && main_pane_index < pane_shells_index
                && app.contains("main-pane := Rectangle {\n            horizontal-stretch: 1;"),
            "global search/tools should live in the shell header while address/navigation stay inside the right main pane"
        );
        assert!(
            !app.contains("SidebarSection { label: \"Remote\""),
            "sidebar should not show an unimplemented Remote section"
        );
        assert!(
            !app.contains("label: \"Network\""),
            "sidebar should not expose a no-op Network placeholder"
        );
        assert!(
            app.contains("background: root.sidebar-surface-color;"),
            "sidebar panel should use the raised sidebar surface"
        );
        assert!(
            app.contains("border-color: root.sidebar-border-color;"),
            "sidebar panel should keep a subtle border"
        );
        assert!(
            app.contains("private property <length> path-bar-height: 56px;"),
            "AppWindow path bar height should match Rust main-pane geometry"
        );
        assert!(
            bars.contains("export component TopBar inherits Rectangle")
                && bars.contains("export component PathBar inherits Rectangle")
                && bars.matches("height: 56px;").count() >= 2,
            "TopBar and PathBar should both expose the shared 56px chrome rhythm"
        );
        assert!(
            !bars.contains("callback go_parent") && !bars.contains("label: \"^\""),
            "visible Home/up navigation should be removed from the chrome"
        );
        assert!(
            app.contains("private property <length> main-content-height: max(1px, root.height - root.shell-header-height);")
                && app.contains("component FilePane inherits Rectangle")
                && app.contains("pane-content := Rectangle")
                && app.contains("height: max(1px, parent.height - root.path-bar-height - root.status-bar-height - (root.search-panel-visible ? root.search-panel-height : 0px));")
                && app.contains("left-pane-shell := Rectangle")
                && app.contains("current-path: root.left_pane_path;")
                && app.contains("if (root.search-panel-visible) : SearchPanel")
                && app.contains("SplitPaneView {"),
            "pane content height should subtract the pane-local path bar, search filters, and status bar inside the reusable file pane"
        );
        assert!(
            app.contains("private property <length> search-panel-height: root.search-panel-visible ? (root.active-pane-width < 760px ? 78px : 44px) : 0px;"),
            "search filters should size against the active pane instead of squeezing the inactive split pane"
        );
        assert!(
            app.contains(
                "private property <color> shell-base-color: root.dark_mode ? #101418 : #f6f8fa;"
            ),
            "top bar, main pane, search strip, and status bar should share one calm base surface"
        );
        assert!(
            app.contains("private property <color> sidebar-surface-color: root.dark_mode ? #181e24 : #ffffff;"),
            "light theme sidebar should be a raised white foreground surface"
        );
        assert!(
            app.contains(
                "private property <color> sidebar-border-color: root.dark_mode ? #313a43 : #d8e1ea;"
            ),
            "sidebar border should stay slightly stronger than the flat shell separators"
        );
        assert!(
            app.contains("padding-left: 8px;") && app.contains("padding-right: 8px;"),
            "sidebar rows should be inset inside the rounded content-row sidebar panel"
        );
        assert!(
            app.contains("in-out property <float> sidebar_width_px: 280;"),
            "default sidebar width should follow COSMIC's narrower nav rhythm"
        );
        assert!(
            app.contains("private property <float> sidebar-resize-start-width-px: 280;"),
            "sidebar resize state should initialize from the same COSMIC-like default"
        );
        assert!(
            app.contains("private property <length> sidebar-panel-radius: 16px;")
                && !app.contains("sidebar-panel-margin"),
            "sidebar content panel should keep the rounded COSMIC-like treatment without drifting back to a window-level overlay"
        );

        for (name, slint) in [
            ("TopBar/PathBar", bars),
            ("SearchPanel", search_panel),
            ("StatusBar", status_bar),
        ] {
            assert!(
                slint.contains("background: transparent;"),
                "{name} should stay transparent over the shared shell base"
            );
        }

        assert!(
            !top_bar_component.contains("background: root.separator-color;"),
            "TopBar should not draw a separator between the shell tool area and main content"
        );

        for (name, slint) in [
            ("PathBar", path_bar_component),
            ("SearchPanel", search_panel),
            ("StatusBar", status_bar),
        ] {
            assert!(
                slint.contains("private property <color> separator-color"),
                "{name} should define a local separator color instead of a panel surface"
            );
            assert!(
                slint.contains("background: root.separator-color;"),
                "{name} should draw only a separator line"
            );
        }

        assert!(
            bars.contains(
                "private property <color> field-background: root.dark ? #151b20 : #ffffff;"
            ),
            "TopBar and PathBar fields should use the quiet COSMIC-like input surface"
        );
        assert!(
            bars.contains(
                "private property <color> field-text-color: root.dark ? #eef3f7 : #24303b;"
            ),
            "TopBar and PathBar input text should remain readable in light theme"
        );
        assert!(
            bars.matches("height: 32px;").count() >= 2,
            "TopBar search and PathBar address inputs should keep the lighter COSMIC-style 32px height"
        );
        assert!(
            bars.contains("min-width: 96px;"),
            "PathBar should keep the address field flexible inside the main pane"
        );
        assert!(
            bars.contains("min-width: 180px;")
                && bars.contains("preferred-width: 320px;")
                && bars.contains("max-width: 420px;"),
            "TopBar active search field should live in the global toolbar with bounded flexible width"
        );
        assert!(
            bars.contains("width: max(1px, parent.width - 70px);"),
            "TopBar search input text should clamp narrow available widths instead of overflowing"
        );
        assert!(
            !bars.contains("\n                width: 240px;"),
            "TopBar active search field must not return to a fixed width that can squeeze the main pane"
        );
        assert!(
            !bars.contains("root.width - root.sidebar-width-px"),
            "TopBar layout constraints should not depend on root width because that can create Slint layoutinfo binding loops"
        );
        let widgets = include_str!("../../ui/widgets.slint");
        let tool_button = widgets
            .split("export component ActionButton")
            .next()
            .expect("ToolButton component should be before ActionButton");
        assert!(
            tool_button.contains("width: 32px;") && tool_button.contains("height: 32px;"),
            "shared ToolButton should keep the lighter 32px header control size"
        );
        assert!(
            tool_button.contains("border-radius: 8px;") && tool_button.contains("font-size: 13px;"),
            "shared ToolButton should keep COSMIC-like icon-button weight"
        );
    }

    #[test]
    fn split_pane_ui_has_equal_left_and_right_panel_chrome() {
        let app = include_str!("../../ui/app.slint");
        let split_pane = include_str!("../../ui/split_pane.slint");

        let pane_routing = app
            .split_once("export global PaneRouting {")
            .expect("app.slint should export a global pane routing surface")
            .1
            .split_once("import { DragKind }")
            .expect("PaneRouting should be declared before imports")
            .0;
        let file_pane = app
            .split_once("component FilePane inherits Rectangle {")
            .expect("reusable FilePane component should exist")
            .1
            .split_once("component PaneSlot inherits FilePane")
            .expect("FilePane component should be before PaneSlot")
            .0;
        let pane_slot = app
            .split_once("component PaneSlot inherits FilePane {")
            .expect("PaneSlot should wrap FilePane with shared routing")
            .1
            .split_once("export component AppWindow inherits Window")
            .expect("PaneSlot should be defined before AppWindow")
            .0;
        let route_functions = app
            .split_once("public function route-pane-focus(side: int) {")
            .expect("AppWindow should expose shared pane route functions")
            .1
            .split_once("title: chooser_mode")
            .expect("pane route functions should be defined before the window body")
            .0;

        assert!(app.contains("import { SplitPaneView } from \"split_pane.slint\";"));
        assert!(app.contains("private property <length> active-pane-width"));
        assert!(app.contains("width: root.active-pane-width;"));
        assert!(app.contains("in-out property <float> inactive_pane_viewport_x"));
        assert!(app.contains("pane-shells := Rectangle"));
        assert!(app.contains("left-pane-shell := Rectangle"));
        assert!(app.contains("if (root.split_view_open) : right-pane-shell := Rectangle"));
        assert!(app.contains("virtual-start-index: root.inactive_pane_virtual_start_index;"));
        assert!(app.contains("viewport-offset <=> root.inactive_pane_viewport_offset;"));
        assert!(!app.contains("callback left_pane_path_submitted(string);"));
        assert!(!app.contains("callback left_pane_go_back();"));
        assert!(!app.contains("callback left_pane_go_forward();"));
        assert!(!app.contains("callback inactive_path_submitted(string);"));
        assert!(!app.contains("callback inactive_go_back();"));
        assert!(!app.contains("callback inactive_go_forward();"));
        assert!(!app.contains("callback open_inactive_path(string);"));
        assert!(!app.contains("callback inactive_pane_view_changed();"));
        assert!(!app.contains("callback main_view_changed();"));
        assert!(
            !app.contains("callback prepare_inactive_pane_transfer(string, float, float) -> bool;")
        );
        assert!(
            !app.contains(
                "callback inactive_pane_drop_target_path(float, float, string) -> string;"
            )
        );
        assert!(
            !app.contains("callback inactive_pane_drop_allowed(float, float, string) -> bool;")
        );
        assert!(!app.contains("right-pane-content := Rectangle"));
        assert!(!app.contains("active-grid-clip := Rectangle"));
        assert!(!app.contains("active-pane-clip := Rectangle"));
        assert!(!app.contains("split-pane-clip := Rectangle"));
        assert!(!app.contains("split-pane-content := Rectangle"));
        assert!(!app.contains("callback focus_inactive_pane();"));
        assert!(!app.contains("root.focus_inactive_pane();"));
        assert!(!app.contains("in-out property <string> path_input_text;"));
        assert!(!app.contains("in-out property <bool> path-input-focused"));
        assert!(!app.contains("current-path: root.current_path;"));
        assert!(
            !app.contains("main-blank-touch := TouchArea"),
            "active pane blank input layer should be scoped inside the active pane shell"
        );
        assert!(
            pane_routing.contains("callback focus(int);")
                && pane_routing.contains("callback path-submitted(int, string);")
                && pane_routing.contains("callback go-back(int);")
                && pane_routing.contains("callback go-forward(int);")
                && pane_routing.contains("callback view-changed(int);")
                && pane_routing.contains("callback activated(int, string);")
                && pane_routing.contains("callback request-select(int, string, bool, bool);")
                && pane_routing.contains("callback select-rect(int, float, float, float, float, int, float, float, float, bool);")
                && pane_routing.contains("callback clear-selection(int);")
                && pane_routing.contains("callback request-context-menu(int, string, string, string, string, bool, length, length);")
                && pane_routing.contains("callback request-blank-context-menu(int, length, length);")
                && pane_routing.contains("callback drop-target-path(int, float, float, string) -> string;")
                && pane_routing.contains("callback drop-allowed(int, float, float, string) -> bool;")
                && pane_routing.contains("callback prepare-transfer(int, string, float, float) -> bool;")
                && pane_routing.contains("pure callback is-selected(int, string) -> bool;"),
            "PaneRouting should expose one side-aware surface for every pane interaction"
        );
        assert_eq!(
            file_pane.matches("PathBar {").count(),
            1,
            "FilePane should own one address bar"
        );
        assert_eq!(
            file_pane.matches("SplitPaneView {").count(),
            1,
            "FilePane should own one file content view"
        );
        assert_eq!(
            file_pane.matches("DropArea {").count(),
            1,
            "FilePane should own one drop target layer"
        );
        assert_eq!(
            file_pane.matches("StatusBar {").count(),
            1,
            "FilePane should own one status bar"
        );
        assert!(
            file_pane.contains("height: max(1px, parent.height - root.path-bar-height - root.status-bar-height - (root.search-panel-visible ? root.search-panel-height : 0px));")
                && file_pane.contains("path-text <=> root.path-text;")
                && file_pane.contains("path-focused <=> root.path-focused;")
                && file_pane.contains("viewport-x <=> root.viewport-x;")
                && file_pane.contains("viewport-offset <=> root.viewport-offset;")
                && file_pane.contains("drag-active: root.drag-active;")
                && file_pane.contains("status: root.status;")
                && file_pane.contains("selected-count: root.selected-count;")
                && file_pane.contains("selected-status: root.selected-status;"),
            "FilePane should expose address, content, drag, selection, and status through pane-local bindings"
        );
        assert!(
            !file_pane.contains("if (root.drag-active): Rectangle")
                && !file_pane.contains("background: root.drag-rejected ?")
                && !file_pane.contains("border-color: root.drag-rejected ?"),
            "FilePane should not tint the entire pane during drag; only concrete drop targets should show feedback"
        );
        assert!(
            file_pane.contains("callback request_context_menu")
                && file_pane.contains("callback request_blank_context_menu")
                && file_pane.contains("callback select_rect")
                && file_pane.contains("callback clear_selection")
                && file_pane.contains("callback drop_target_path")
                && file_pane.contains("callback drop_allowed")
                && file_pane.contains("callback prepare_transfer")
                && file_pane.contains("callback make_drag_data"),
            "FilePane should expose the full interactive surface shared by both panes"
        );
        assert!(
            file_pane.contains("in property <int> pane-side: 0;")
                && file_pane.contains("callback focus_requested(int);")
                && file_pane.contains("callback path_submitted(int, string);")
                && file_pane.contains("callback go_back(int);")
                && file_pane.contains("callback go_forward(int);")
                && file_pane.contains("callback view_changed(int);")
                && file_pane.contains("callback activated(int, string);")
                && file_pane.contains("callback request_select(int, string, bool, bool);")
                && file_pane.contains("callback request_context_menu(int,")
                && file_pane.contains("pure callback is_selected(int, string) -> bool;")
                && file_pane
                    .contains("pure callback make_drag_data(int, string, bool) -> data-transfer;"),
            "FilePane callbacks should carry the pane side instead of baking in left/right behavior"
        );
        assert!(
            file_pane.contains("focus_requested => { root.focus_requested(root.pane-side); }")
                && file_pane.contains("go_back => { root.go_back(root.pane-side); }")
                && file_pane.contains("go_forward => { root.go_forward(root.pane-side); }")
                && file_pane
                    .contains("path_submitted(path) => { root.path_submitted(root.pane-side, path); }")
                && file_pane.contains("root.request_context_menu(root.pane-side, path, name, size, modified, is-dir, x, y);")
                && file_pane.contains("navigate_back => { root.go_back(root.pane-side); }")
                && file_pane.contains("navigate_forward => { root.go_forward(root.pane-side); }")
                && file_pane.contains("root.is_selected(root.pane-side, path)")
                && file_pane.contains(
                    "commit_external_edit => { root.commit_external_edit(root.pane-side); }"
                )
                && file_pane.contains(
                    "discard_external_edit => { root.discard_external_edit(root.pane-side); }"
                )
                && file_pane
                    .contains("save_focus_changed(focused) => { root.save_focus_changed(root.pane-side, focused); }"),
            "FilePane should route address bar, content, side buttons, context menus, selection, and status through pane-side"
        );
        let pane_slot_bindings = [
            "focus_requested(side) => { PaneRouting.focus(side); }",
            "path_submitted(side, path) => { PaneRouting.path-submitted(side, path); }",
            "go_back(side) => { PaneRouting.go-back(side); }",
            "go_forward(side) => { PaneRouting.go-forward(side); }",
            "search_submitted(query) => { PaneRouting.search-submitted(query); }",
            "cancel_search => { PaneRouting.cancel-search(); }",
            "search_close_requested => { PaneRouting.search-close-requested(); }",
            "view_changed(side) => { PaneRouting.view-changed(side); }",
            "activated(side, path) => { PaneRouting.activated(side, path); }",
            "request_select(side, path, toggle, range) => {\n        PaneRouting.request-select(side, path, toggle, range);\n    }",
            "clear_selection(side) => { PaneRouting.clear-selection(side); }",
            "request_context_menu(side, path, name, size, modified, is-dir, x, y) => {\n        PaneRouting.request-context-menu(side, path, name, size, modified, is-dir, x, y);\n    }",
            "request_blank_context_menu(side, x, y) => {\n        PaneRouting.request-blank-context-menu(side, x, y);\n    }",
            "zoom_in(side) => { PaneRouting.zoom-in(side); }",
            "zoom_out(side) => { PaneRouting.zoom-out(side); }",
            "drop_target_path(side, x, y, source) => {\n        PaneRouting.drop-target-path(side, x, y, source)\n    }",
            "drop_allowed(side, x, y, source) => {\n        PaneRouting.drop-allowed(side, x, y, source)\n    }",
            "prepare_transfer(side, source, x, y) => {\n        PaneRouting.prepare-transfer(side, source, x, y)\n    }",
            "transfer_menu_requested(side) => { PaneRouting.transfer-menu-requested(side); }",
            "trace_drop(action, kind, path, x, y, rejected, target) => {\n        PaneRouting.trace-drop(action, kind, path, x, y, rejected, target);\n    }",
            "save_focus_changed(side, focused) => { PaneRouting.save-focus-changed(side, focused); }",
            "commit_external_edit(side) => { PaneRouting.commit-external-edit(side); }",
            "discard_external_edit(side) => { PaneRouting.discard-external-edit(side); }",
            "undo_last_operation => { PaneRouting.undo-last-operation(); }",
            "chooser_accept(value) => { PaneRouting.chooser-accept(value); }",
            "chooser_filter_requested(side, x, y) => { PaneRouting.chooser-filter-requested(side, x, y); }",
            "chooser_choice_requested(side, index, x, y) => {\n        PaneRouting.chooser-choice-requested(side, index, x, y);\n    }",
            "is_selected(side, path) => {\n        PaneRouting.is-selected(side, path)\n    }",
            "make_drag_data(side, path, is-dir) => {\n        is-dir ? DndApi.make-drag-folder(path) : DndApi.make-drag-file(path)\n    }",
        ];
        for binding in pane_slot_bindings {
            assert!(
                pane_slot.contains(binding),
                "PaneSlot should own shared pane event routing: {binding}"
            );
        }
        let pane_row_header = app
            .split_once("pane-row := Rectangle {")
            .expect("split panes should live inside an explicit row")
            .1
            .split_once("pane-shells := Rectangle {")
            .expect("pane row should contain the physical pane shells")
            .0;
        assert!(
            pane_row_header.contains("width: parent.width;")
                && pane_row_header.contains("height: parent.height;")
                && pane_row_header.contains("clip: true;")
                && !pane_row_header.contains("root.status-bar-height"),
            "split pane row should give each physical pane its own full-height chrome"
        );
        assert!(
            app.contains("left-pane-shell := Rectangle {\n                        x: 0px;\n                        width: root.active-pane-width;\n                        height: parent.height;")
                && app.contains("if (root.split_view_open) : right-pane-shell := Rectangle {\n                        x: root.inactive-pane-x;\n                        width: root.inactive-pane-width;\n                        height: parent.height;"),
            "split view should use two equal sibling pane shells anchored on either side of the divider"
        );
        assert!(
            app.contains("private property <length> split-divider-width: root.split_view_open ? 1px : 0px;")
                && app.contains("private property <length> inactive-pane-x: root.active-pane-width + root.split-divider-width;")
                && app.contains("private property <length> inactive-pane-width: root.split_view_open ? max(1px, root.main-pane-width - root.inactive-pane-x) : 0px;")
                && app.contains("if (root.split_view_open) : split-divider := Rectangle {\n                        x: root.active-pane-width;\n                        width: root.split-divider-width;\n                        height: parent.height;\n                        background: root.split-resize-active ?"),
            "split divider should be a single pane-level line between the active and inactive shells with drag feedback"
        );
        assert!(
            app.contains("if (root.split_view_open) : right-pane-shell := Rectangle {\n                        x: root.inactive-pane-x;\n                        width: root.inactive-pane-width;\n                        height: parent.height;\n                        background: root.shell-base-color;\n                        clip: true;"),
            "inactive split pane should start after the divider and use the remaining width"
        );
        let pane_shells = app
            .split_once("pane-shells := Rectangle {")
            .expect("split panes should live inside one explicit shell row")
            .1
            .split_once("DragOverlayLayer {")
            .expect("split pane shell row should be before overlay layers")
            .0;
        assert_eq!(
            pane_shells.matches("PaneSlot {").count(),
            2,
            "split view must render both physical panes through the same reusable PaneSlot component"
        );
        assert_eq!(
            pane_shells.matches("FilePane {").count(),
            0,
            "physical pane instances should not bypass PaneSlot routing"
        );
        assert!(
            !pane_shells.contains("PathBar {")
                && !pane_shells.contains("StatusBar {")
                && !pane_shells.contains("SplitPaneView {")
                && !pane_shells.contains("right-pane-content := Rectangle"),
            "pane shells should not hand-roll pane chrome or content outside FilePane"
        );
        assert!(
            route_functions.contains("main-focus.focus();")
                && route_functions.contains("root.pane_focus(side);")
                && route_functions
                    .contains("public function route-pane-path-submitted(side: int, path: string)")
                && route_functions.contains("root.pane_path_submitted(side, path);")
                && route_functions.contains("public function route-pane-go-back(side: int)")
                && route_functions.contains("root.pane_go_back(side);")
                && route_functions.contains("public function route-pane-go-forward(side: int)")
                && route_functions.contains("root.pane_go_forward(side);"),
            "shared pane route functions should focus and navigate any pane from the same side-aware code"
        );
        assert!(
            route_functions.contains("public function route-pane-view-changed(side: int)")
                && route_functions.contains("root.pane_view_changed(side);")
                && route_functions.contains("public function route-pane-activated(side: int, path: string)")
                && route_functions.contains("root.pane_activated(side, path);")
                && route_functions.contains(
                    "public function route-pane-request-select(side: int, path: string, toggle: bool, range: bool)"
                )
                && route_functions.contains("root.pane_request_select(side, path, toggle, range);")
                && route_functions.contains("public function route-pane-select-rect(side: int,")
                && route_functions.contains("root.pane_select_rect(side, x1, y1, x2, y2, rows-per-column, cell-width, row-height, padding, toggle);"),
            "shared pane route functions should dispatch activation, selection, and view state by side"
        );
        assert!(
            route_functions.contains("public function route-pane-request-context-menu(side: int,")
                && route_functions.contains("root.refresh_clipboard_availability();")
                && route_functions.contains("if (!root.pane_is_selected(side, path))")
                && route_functions.contains("root.pane_request_select(side, path, false, false);")
                && route_functions.contains("root.show-context-menu(1, x, y);")
                && route_functions
                    .contains("public function route-pane-request-blank-context-menu(side: int,")
                && route_functions.contains("root.show-context-menu(3, x, y);")
                && route_functions
                    .contains("public function route-pane-drop-target-path(side: int,")
                && route_functions
                    .contains("return root.pane_drop_target_path(side, x, y, source);")
                && route_functions.contains("public function route-pane-drop-allowed(side: int,")
                && route_functions.contains("return root.pane_drop_allowed(side, x, y, source);")
                && route_functions
                    .contains("public function route-pane-prepare-transfer(side: int,")
                && route_functions
                    .contains("return root.pane_prepare_transfer(side, source, x, y);")
                && route_functions
                    .contains("public function route-pane-transfer-menu-requested(side: int)"),
            "shared pane route functions should dispatch context menus and drag/drop by side"
        );
        assert!(
            route_functions.contains("public function route-pane-save-focus-changed(side: int, focused: bool)")
                && route_functions
                    .contains("public function route-pane-chooser-filter-requested(side: int, x: length, y: length)")
                && route_functions.contains(
                    "public function route-pane-chooser-choice-requested(side: int, index: int, x: length, y: length)"
                ),
            "shared pane route functions should dispatch status bar and chooser controls by side"
        );
        assert!(app.contains("in-out property <string> left_pane_path;"));
        assert!(app.contains("in-out property <string> left_pane_path_input_text;"));
        assert!(app.contains("in-out property <bool> left_pane_path_focused: false;"));
        assert!(app.contains("in-out property <bool> left_pane_can_go_back: false;"));
        assert!(app.contains("in-out property <bool> left_pane_can_go_forward: false;"));
        let left_pane = app
            .split_once("left-pane-shell := Rectangle {")
            .expect("active pane shell should exist")
            .1
            .split_once("if (root.split_view_open) : split-divider")
            .expect("left pane should be before the split divider")
            .0;
        assert!(
            left_pane.contains("PaneSlot {")
                && left_pane.contains("pane-side: 0;")
                && left_pane.contains("current-path: root.left_pane_path;")
                && left_pane.contains("path-text <=> root.left_pane_path_input_text;")
                && left_pane.contains("path-focused <=> root.left_pane_path_focused;")
                && left_pane.contains("can-go-back: root.left_pane_can_go_back;")
                && left_pane.contains("can-go-forward: root.left_pane_can_go_forward;")
                && left_pane.contains("status: root.left_pane_status;")
                && left_pane.contains("selected-count: root.left_pane_selected_count;")
                && left_pane.contains("selected-status: root.left_pane_selected_status;")
                && left_pane.contains("external-edit-active: root.left_pane_external_edit_active;")
                && left_pane.contains("external-edit-status: root.left_pane_external_edit_status;")
                && left_pane
                    .contains("selected-path: root.focused_pane == 0 ? root.selected_path : \"\";"),
            "pane slot 0 should bind only pane-local address, status, selection, and focus-owned state"
        );
        assert!(
            !left_pane.contains("current-path: root.inactive_pane_path;")
                && !left_pane.contains("path-text <=> root.inactive_pane_path_input_text;")
                && !left_pane.contains("path-focused <=> root.inactive_pane_path_focused;")
                && !left_pane.contains("status: root.inactive_pane_status;")
                && !left_pane.contains("selected-count: root.inactive_pane_selected_count;")
                && !left_pane.contains("selected-status: root.inactive_pane_selected_status;")
                && !left_pane.contains("PaneRouting."),
            "pane slot 0 must not bind another pane's data or duplicate shared routing"
        );
        assert!(app.contains("current-path: root.inactive_pane_path;"));
        assert!(app.contains("path-text <=> root.inactive_pane_path_input_text;"));
        assert!(app.contains("path-focused <=> root.inactive_pane_path_focused;"));
        let inactive_pane = app
            .split_once("if (root.split_view_open) : right-pane-shell := Rectangle {")
            .expect("right pane shell should exist")
            .1
            .split_once("if (root.split_view_open) : split-divider-touch")
            .expect("right pane should be before the divider touch area")
            .0;
        assert!(
            inactive_pane.contains("PaneSlot {")
                && inactive_pane.contains("pane-side: 1;")
                && inactive_pane.contains("current-path: root.inactive_pane_path;")
                && inactive_pane.contains("path-text <=> root.inactive_pane_path_input_text;")
                && inactive_pane.contains("path-focused <=> root.inactive_pane_path_focused;")
                && inactive_pane.contains("can-go-back: root.inactive_pane_can_go_back;")
                && inactive_pane.contains("can-go-forward: root.inactive_pane_can_go_forward;")
                && inactive_pane.contains("status: root.inactive_pane_status;")
                && inactive_pane.contains("selected-count: root.inactive_pane_selected_count;")
                && inactive_pane.contains("selected-status: root.inactive_pane_selected_status;")
                && inactive_pane
                    .contains("external-edit-active: root.inactive_pane_external_edit_active;")
                && inactive_pane
                    .contains("external-edit-status: root.inactive_pane_external_edit_status;")
                && inactive_pane
                    .contains("selected-path: root.focused_pane == 1 ? root.selected_path : \"\";"),
            "pane slot 1 should bind only pane-local address, status, selection, and focus-owned state"
        );
        assert!(
            !inactive_pane.contains("root.go_back();")
                && !inactive_pane.contains("root.go_forward();")
                && !inactive_pane.contains("root.current_path")
                && !inactive_pane.contains("root.path_input_text")
                && !inactive_pane.contains("root.focus_inactive_pane();")
                && !inactive_pane.contains("PaneRouting."),
            "pane slot 1 must not use shared active/global address state or duplicate shared routing"
        );
        assert!(app.contains("callback pane_prepare_transfer(int, string, float, float) -> bool;"));
        assert!(
            app.contains("callback pane_drop_target_path(int, float, float, string) -> string;")
        );
        assert!(app.contains("callback pane_drop_allowed(int, float, float, string) -> bool;"));
        assert!(app.contains("root.pane_prepare_transfer(side, source, x, y)"));
        assert!(app.contains("root.pane_drop_target_path(side, x, y, source)"));
        assert!(app.contains("root.pane_drop_allowed(side, x, y, source)"));
        assert!(app.contains("inactive-pane-drag-active"));
        assert!(app.contains("in-out property <bool> left_pane_in_trash: false;"));
        assert!(app.contains("in-out property <bool> inactive_pane_in_trash: false;"));
        assert!(app.contains(
            "show-location: root.left_pane_in_trash || (root.recursive_search && root.search_query != \"\");"
        ));
        assert!(app.contains("show-location: root.inactive_pane_in_trash;"));
        assert!(split_pane.contains("export component SplitPaneView"));
        assert!(split_pane.contains("import { FileTile } from \"file_tile.slint\";"));
        assert!(!split_pane.contains("SplitPreviewTile"));
        assert!(split_pane.contains("callback view_changed();"));
        assert!(split_pane.contains("callback focus_requested();"));
        assert!(split_pane.contains(
            "root.focus_requested();\n        root.pan-target-viewport-x = root.entry-count == 0"
        ));
        assert!(
            split_pane
                .contains("root.focus_requested();\n                    root.activated(path);")
        );
        assert!(!split_pane.contains("Click to focus it."));
        assert!(split_pane.contains("callback activated(string);"));
        assert!(split_pane.contains("callback zoom_in();"));
        assert!(split_pane.contains("callback zoom_out();"));
        assert!(split_pane.contains("function handle-scroll("));
        assert!(
            split_pane
                .contains("function scroll-pan-delta(delta-x: length, delta-y: length) -> length")
                && split_pane
                    .contains("root.pan-horizontal(root.scroll-pan-delta(delta-x, delta-y));")
                && !split_pane.contains("delta-y + delta-x"),
            "pane scrolling should use the dominant wheel axis instead of adding touchpad cross-axis jitter"
        );
        let file_tile = include_str!("../../ui/file_tile.slint");
        assert!(
            file_tile
                .contains("function scroll-pan-delta(delta-x: length, delta-y: length) -> length")
                && file_tile.contains(
                    "root.pan_horizontal(root.scroll-pan-delta(event.delta-x, event.delta-y));"
                )
                && !file_tile.contains("delta-y + delta-x"),
            "tile scrolling should use the same dominant-axis wheel rule as blank pane scrolling"
        );
        assert!(
            split_pane.contains(
                "function pan-horizontal(delta: length) {\n        root.focus_requested();\n        root.pan-target-viewport-x = root.entry-count == 0"
            ),
            "ordinary pane scrolling should request focus through pan-horizontal"
        );
        assert!(
            split_pane.contains("private property <float> pan-target-viewport-x: 0;")
                && split_pane.contains("if (root.pan-target-viewport-x != root.viewport-x) {")
                && split_pane.contains("root.view_changed();"),
            "pane scrolling should skip virtual refreshes when wheel input clamps to the current viewport"
        );
        assert!(
            !split_pane.contains(
                "function handle-scroll(delta-x: length, delta-y: length, control: bool) {\n        root.focus_requested();"
            ),
            "ordinary wheel events should not request pane focus twice before panning"
        );
        assert!(
            split_pane.contains(
                "if (control && delta-y < 0px) {\n            root.focus_requested();\n            root.zoom_out();"
            ) && split_pane.contains(
                "} else if (control && delta-y > 0px) {\n            root.focus_requested();\n            root.zoom_in();"
            ),
            "Ctrl+wheel zoom should still request pane focus before changing zoom"
        );
        assert!(split_pane.contains("scroll-event(event)"));
        assert!(split_pane.contains("for item[index] in root.entries: FileTile"));
        assert!(
            split_pane.contains("drag-data-source: root.make_drag_data(item.path, item.is_dir);")
        );
        assert!(split_pane.contains("show-location: root.show-location;"));
        assert!(split_pane.contains(
            "drop-target: root.drag-active && !root.drag-rejected && root.drag-target-path == item.path;"
        ));
        assert!(split_pane.contains(
            "root.request_context_menu(path, name, item.size, item.modified, item.is_dir, x, y);"
        ));
        assert!(split_pane.contains("root.pan-horizontal(delta);"));
        assert!(split_pane.contains("viewport-x <=> root.viewport-offset;"));
        assert!(split_pane.contains("private property <float> viewport-sync-epsilon: 0.5;"));
        assert!(
            split_pane.contains("root.viewport-x + root.viewport-sync-epsilon <")
                && split_pane.contains(
                    "root.viewport-x > max(0, -self.viewport-x / 1px) + root.viewport-sync-epsilon"
                )
                && !split_pane.contains("root.viewport-x != max(0, -self.viewport-x / 1px)"),
            "ScrollView viewport writeback should ignore sub-pixel drift instead of churning virtual slices"
        );
        assert!(split_pane.contains("virtual-layer := Rectangle"));
        assert!(split_pane.contains(
            "private property <length> virtual-layer-width: root.viewport-content-width;"
        ));
        assert!(
            split_pane.contains("x: 0px;")
                && split_pane.contains("width: root.virtual-layer-width;")
                && split_pane
                    .contains("property <int> global-index: index + root.virtual-start-index;")
                && split_pane.contains(
                    "x: root.preview-padding + floor(global-index / root.rows-per-column) * root.cell-width;"
                )
                && split_pane.contains(
                    "y: root.preview-padding + mod(global-index, root.rows-per-column) * root.row-height;"
                )
                && !split_pane.contains("property <int> local-index:")
                && !split_pane.contains("root.virtual-start-column * root.cell-width"),
            "virtualized pane slices should keep a stable full-width layer and move only tile positions"
        );
        assert!(!split_pane.contains("root.viewport-content-width - self.x"));
        assert!(split_pane.contains("clip: true;"));
        assert!(
            !split_pane.contains("import { PathBar } from \"top_bar.slint\";"),
            "SplitPaneView should be content-only; pane chrome belongs to app.slint"
        );
        assert!(!split_pane.contains("PathBar {"));
        assert!(!split_pane.contains("current-path"));
        assert!(!split_pane.contains("path-text"));
        assert!(!split_pane.contains("path-bar-height"));
        assert!(split_pane.contains("root.height - 2 * root.preview-padding"));
        assert!(!split_pane.contains("header-height"));
        assert!(!split_pane.contains("Split Pane"));
        assert!(!split_pane.contains("text: root.path"));
        assert!(!split_pane.contains("x: 1px;"));
        assert!(!split_pane.contains("parent.width - 1px"));
        assert!(!split_pane.contains("root.width - 1px"));
        assert!(
            app.contains("pane-row := Rectangle")
                && app.contains("height: parent.height;")
                && app.contains("left-pane-shell := Rectangle")
                && app.contains("right-pane-shell := Rectangle")
                && app.contains("height: parent.height;"),
            "split panes should keep their chrome inside each clipped physical pane"
        );
        assert!(
            app.matches("clip: true;").count() >= 2,
            "active and inactive split pane shells must be clipped at their pane boundaries"
        );
        assert!(
            !app.contains(
                "StatusBar {\n                    y: max(0px, parent.height - root.status-bar-height);\n                    width: parent.width;"
            ),
            "split view must not use a shared full-width status bar"
        );
    }

    #[test]
    fn split_pane_divider_is_draggable_and_persistent() {
        let app = include_str!("../../ui/app.slint");

        assert!(app.contains("private property <length> split-resize-hit-width: 15px;"));
        assert!(app.contains("private property <float> split-resize-start-ratio: 0.5;"));
        assert!(app.contains("private property <length> split-resize-press-x: 0px;"));
        assert!(app.contains("private property <bool> split-resize-active: false;"));
        assert!(
            app.contains("changed split_view_open => {\n        if (!root.split_view_open) {\n            root.split-resize-active = false;\n        }\n    }"),
            "closing split view should clear any active divider drag state"
        );
        assert!(
            app.contains("changed split_pane_ratio => {\n        root.sync-main-viewport();\n        root.pane_view_changed(0);\n        root.pane_view_changed(1);\n    }"),
            "dragging the divider should resync both pane virtual views as the ratio changes"
        );

        let divider = app
            .split_once("if (root.split_view_open) : split-divider := Rectangle {")
            .expect("split divider should exist")
            .1
            .split_once("if (root.split_view_open) : right-pane-shell := Rectangle")
            .expect("split divider should be before the right pane shell")
            .0;
        assert!(
            divider.contains("x: root.active-pane-width;")
                && divider.contains("width: root.split-divider-width;")
                && divider.contains("background: root.split-resize-active ?"),
            "visible divider should stay between panes and expose drag feedback"
        );

        let divider_touch = app
            .split_once("if (root.split_view_open) : split-divider-touch := TouchArea {")
            .expect("split divider touch area should exist")
            .1
            .split_once("}\n                    }\n                }\n        }")
            .expect("split divider touch area should be scoped to pane shells")
            .0;
        assert!(
            divider_touch
                .contains("x: max(0px, root.active-pane-width - root.split-resize-hit-width / 2);")
                && divider_touch.contains("width: root.split-resize-hit-width;")
                && divider_touch.contains("height: parent.height;")
                && divider_touch.contains("mouse-cursor: ew-resize;"),
            "divider touch area should be wider than the 1px visual line"
        );
        assert!(
            divider_touch.contains("root.split-resize-start-ratio = root.split_pane_ratio;")
                && divider_touch.contains(
                    "root.split-resize-press-x = self.absolute-position.x + self.mouse-x;"
                )
                && divider_touch.contains("root.split-resize-active = true;"),
            "divider drag should remember the starting ratio and press position"
        );
        assert!(
            divider_touch.contains("root.split-resize-active = false;")
                && divider_touch.contains("root.persist_ui_state();"),
            "divider drag should clear active state and persist the new ratio on release"
        );
        assert!(
            divider_touch.contains("if (self.pressed) {")
                && divider_touch.contains("root.split_pane_ratio = max(0.1, min(0.9, root.split-resize-start-ratio + ((self.absolute-position.x + self.mouse-x - root.split-resize-press-x) / 1px) / max(1, root.split-content-width / 1px)));"),
            "divider drag should update the split ratio continuously while clamped"
        );
    }

    #[test]
    fn active_main_pane_width_uses_split_ratio_only_when_split_is_open() {
        assert_eq!(active_main_pane_width(900.0, false, 0.25), 900.0);
        assert_eq!(active_main_pane_width(900.0, true, 0.5), 449.0);
        assert_eq!(inactive_main_pane_width(900.0, true, 0.5), 450.0);
        assert_eq!(active_main_pane_width(900.0, true, 0.25), 260.0);
        assert_eq!(inactive_main_pane_width(900.0, true, 0.25), 639.0);
        assert_eq!(active_main_pane_width(900.0, true, 0.75), 639.0);
        assert_eq!(inactive_main_pane_width(900.0, true, 0.75), 260.0);
        assert_eq!(
            active_main_pane_width(900.0, true, 0.5)
                + 1.0
                + inactive_main_pane_width(900.0, true, 0.5),
            900.0
        );
        assert_eq!(inactive_main_pane_width(900.0, false, 0.5), 0.0);
        assert_eq!(active_main_pane_width(0.0, false, 0.5), 1.0);
        assert_eq!(active_main_pane_width(0.0, true, 0.5), 1.0);
        assert_eq!(inactive_main_pane_width(0.0, true, 0.5), 1.0);
    }

    fn menu_metrics_input(kind: i32) -> MenuMetricsInput {
        MenuMetricsInput {
            kind,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height: MENU_ITEM_HEIGHT,
            separator_height: MENU_SEPARATOR_HEIGHT,
            title_height: MENU_TITLE_HEIGHT,
        }
    }

    #[test]
    fn search_panel_height_matches_slint_visibility_rules() {
        assert_eq!(search_panel_height(false, "", 0, 0, 0, 900.0), 0.0);
        assert_eq!(search_panel_height(true, "", 0, 0, 0, 900.0), 44.0);
        assert_eq!(search_panel_height(true, "", 0, 0, 0, 700.0), 78.0);
        assert_eq!(search_panel_height(false, "png", 0, 0, 0, 900.0), 44.0);
        assert_eq!(search_panel_height(false, "", 1, 0, 0, 900.0), 44.0);
        assert_eq!(search_panel_height(false, "", 0, 2, 0, 900.0), 44.0);
        assert_eq!(search_panel_height(false, "", 0, 0, 3, 900.0), 44.0);
    }

    #[test]
    fn main_pane_bounds_match_slint_shell_layout() {
        let bounds = main_pane_bounds(320.0, 1100.0, 760.0);

        assert_eq!(bounds.left, 320.0);
        assert_eq!(bounds.top, SHELL_HEADER_HEIGHT);
        assert_eq!(bounds.right, 1100.0);
        assert_eq!(bounds.bottom, 760.0);
    }

    #[test]
    fn main_pane_bounds_do_not_invert_for_tiny_windows() {
        let bounds = main_pane_bounds(320.0, 100.0, 20.0);

        assert_eq!(bounds.left, bounds.right);
        assert_eq!(bounds.top, bounds.bottom);
    }

    #[test]
    fn virtual_entry_range_keeps_visible_columns_with_overscan() {
        assert_eq!(
            virtual_entry_range(100, 4, 0.0, 250.0, 100.0, 10.0, 1),
            0..16
        );
        assert_eq!(
            virtual_entry_range(100, 4, 350.0, 250.0, 100.0, 10.0, 1),
            8..28
        );
        assert_eq!(
            virtual_entry_range(10, 4, 800.0, 250.0, 100.0, 10.0, 1),
            10..10
        );
    }

    #[test]
    fn main_scroll_max_x_matches_column_content_width() {
        assert_eq!(main_scroll_max_x(0, 4, 300.0, 100.0, 10.0), 0.0);
        assert_eq!(main_scroll_max_x(8, 4, 300.0, 100.0, 10.0), 0.0);
        assert_eq!(main_scroll_max_x(12, 4, 300.0, 100.0, 10.0), 20.0);
        assert_eq!(main_scroll_max_x(13, 4, 300.0, 100.0, 10.0), 120.0);
    }

    #[test]
    fn context_menu_metrics_track_visible_menu_rows() {
        let item_height = 38.0;
        let separator_height = 8.0;
        let title_height = 30.0;

        let single_file = context_menu_metrics(MenuMetricsInput {
            kind: 1,
            selected_count: 1,
            is_dir: false,
            default_open_visible: true,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            single_file.height,
            9.0 * item_height + 2.0 * separator_height
        );
        assert_eq!(single_file.open_with_row_y_offset, item_height);

        let single_file_in_trash = context_menu_metrics(MenuMetricsInput {
            kind: 1,
            selected_count: 1,
            is_dir: false,
            default_open_visible: true,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: true,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            single_file_in_trash.height,
            10.0 * item_height + 2.0 * separator_height
        );

        let single_folder = context_menu_metrics(MenuMetricsInput {
            kind: 1,
            selected_count: 1,
            is_dir: true,
            default_open_visible: false,
            add_to_places_visible: true,
            clipboard_has_paths: true,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            single_folder.height,
            11.0 * item_height + 2.0 * separator_height
        );
        assert_eq!(single_folder.open_with_row_y_offset, 0.0);

        let single_folder_without_paste = context_menu_metrics(MenuMetricsInput {
            kind: 1,
            selected_count: 1,
            is_dir: true,
            default_open_visible: false,
            add_to_places_visible: true,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(single_folder_without_paste.height, single_folder.height);

        let single_folder_in_trash = context_menu_metrics(MenuMetricsInput {
            kind: 1,
            selected_count: 1,
            is_dir: true,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: true,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            single_folder_in_trash.height,
            10.0 * item_height + 2.0 * separator_height
        );

        let viewport_with_paste = context_menu_metrics(MenuMetricsInput {
            kind: 3,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: true,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            viewport_with_paste.height,
            5.0 * item_height + separator_height
        );
        assert_eq!(
            viewport_with_paste.open_with_row_y_offset,
            3.0 * item_height + separator_height
        );
        let viewport_without_paste = context_menu_metrics(MenuMetricsInput {
            kind: 3,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(viewport_without_paste.height, viewport_with_paste.height);
        assert_eq!(
            viewport_without_paste.open_with_row_y_offset,
            viewport_with_paste.open_with_row_y_offset
        );

        let trash_viewport = context_menu_metrics(MenuMetricsInput {
            kind: 3,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: true,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(trash_viewport.height, 2.0 * item_height + separator_height);
        assert_eq!(trash_viewport.open_with_row_y_offset, 0.0);

        let builtin_place = context_menu_metrics(MenuMetricsInput {
            kind: 2,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: true,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(builtin_place.height, title_height + item_height);

        let places_blank_with_add_current = context_menu_metrics(MenuMetricsInput {
            kind: 4,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: true,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            places_blank_with_add_current.height,
            title_height + 2.0 * item_height + separator_height
        );

        let places_blank_without_add_current = context_menu_metrics(MenuMetricsInput {
            kind: 4,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            places_blank_without_add_current.height,
            places_blank_with_add_current.height
        );

        let filesystem_device = context_menu_metrics(MenuMetricsInput {
            kind: 5,
            selected_count: 0,
            is_dir: true,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: true,
            device_mounted: true,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(filesystem_device.height, title_height + item_height);

        let unavailable_device = context_menu_metrics(MenuMetricsInput {
            kind: 5,
            selected_count: 0,
            is_dir: false,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: true,
            device_mounted: false,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: false,
            device_can_eject: false,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(unavailable_device.height, title_height + item_height);

        let mounted_ejectable_device = context_menu_metrics(MenuMetricsInput {
            kind: 5,
            selected_count: 0,
            is_dir: true,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: false,
            device_mounted: true,
            device_pending: false,
            device_can_mount: false,
            device_can_unmount: true,
            device_can_eject: true,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(
            mounted_ejectable_device.height,
            title_height + 3.0 * item_height + separator_height
        );

        let pending_device = context_menu_metrics(MenuMetricsInput {
            kind: 5,
            selected_count: 0,
            is_dir: true,
            default_open_visible: false,
            add_to_places_visible: false,
            clipboard_has_paths: false,
            in_trash: false,
            place_builtin: true,
            device_mounted: true,
            device_pending: true,
            device_can_mount: false,
            device_can_unmount: true,
            device_can_eject: true,
            item_height,
            separator_height,
            title_height,
        });
        assert_eq!(pending_device.height, title_height + item_height);
    }

    #[test]
    fn context_menu_submenu_offsets_match_parent_rows() {
        let mut single_file = menu_metrics_input(1);
        single_file.selected_count = 1;
        single_file.default_open_visible = true;
        let metrics = context_menu_metrics(single_file);
        assert_eq!(metrics.open_with_row_y_offset, MENU_ITEM_HEIGHT);
        assert_eq!(metrics.create_new_row_y_offset, 0.0);

        single_file.default_open_visible = false;
        let metrics_without_default = context_menu_metrics(single_file);
        assert_eq!(metrics_without_default.open_with_row_y_offset, 0.0);

        let mut single_folder = menu_metrics_input(1);
        single_folder.selected_count = 1;
        single_folder.is_dir = true;
        single_folder.add_to_places_visible = true;
        single_folder.clipboard_has_paths = true;
        let folder_metrics = context_menu_metrics(single_folder);
        assert_eq!(folder_metrics.open_with_row_y_offset, 0.0);
        assert_eq!(folder_metrics.create_new_row_y_offset, 0.0);

        let viewport_metrics = context_menu_metrics(menu_metrics_input(3));
        assert_eq!(viewport_metrics.create_new_row_y_offset, 0.0);
        assert_eq!(
            viewport_metrics.open_with_row_y_offset,
            3.0 * MENU_ITEM_HEIGHT + MENU_SEPARATOR_HEIGHT
        );

        let mut trash_viewport = menu_metrics_input(3);
        trash_viewport.in_trash = true;
        let trash_metrics = context_menu_metrics(trash_viewport);
        assert_eq!(trash_metrics.open_with_row_y_offset, 0.0);
        assert_eq!(trash_metrics.create_new_row_y_offset, 0.0);
    }

    #[test]
    fn virtual_grid_plan_clamps_viewport_and_reports_anchor_column() {
        let plan = virtual_grid_plan(100, 4, 350.0, 250.0, 100.0, 10.0, 2);
        assert_eq!(plan.viewport_x, 350.0);
        assert_eq!(plan.scroll_max_x, 2270.0);
        assert_eq!(plan.visible_range, 12..24);
        assert_eq!(plan.range, 4..32);
        assert_eq!(plan.start_column, 1);

        let clamped = virtual_grid_plan(10, 4, 800.0, 250.0, 100.0, 10.0, 2);
        assert_eq!(clamped.viewport_x, 70.0);
        assert_eq!(clamped.scroll_max_x, 70.0);
        assert_eq!(clamped.visible_range, 0..10);
        assert_eq!(clamped.range, 0..10);
        assert_eq!(clamped.start_column, 0);
    }

    #[test]
    fn split_preview_plan_uses_bounded_virtual_slice() {
        let plan = split_preview_plan(1_000, 420.0, 704.0, 1_200.0, 1);

        assert_eq!(plan.viewport_x, 1200.0);
        assert_eq!(plan.visible_range, 35..56);
        assert_eq!(plan.range, 21..70);
        assert_eq!(plan.start_column, 3);

        let clamped = split_preview_plan(12, 420.0, 704.0, 4_000.0, 1);
        assert_eq!(clamped.viewport_x, 24.0);
        assert_eq!(clamped.range, 0..12);
        assert_eq!(clamped.start_column, 0);
    }

    #[test]
    fn qmenu_style_root_popup_flips_and_clamps() {
        let placement = PopupPlacement::new(512.0, 512.0, 12.0, 8.0);

        assert_eq!(placement.root_popup(100.0, 100.0, 180.0, 180.0).x, 108.0);
        assert_eq!(placement.root_popup(480.0, 100.0, 180.0, 180.0).x, 292.0);
        assert_eq!(placement.root_popup(20.0, 100.0, 600.0, 180.0).x, 12.0);
    }

    #[test]
    fn qmenu_style_child_popup_anchors_to_parent_row() {
        let placement = PopupPlacement::new(512.0, 512.0, 12.0, 8.0);

        assert_eq!(
            placement.child_popup(ChildPopupInput {
                parent_left: 80.0,
                parent_width: 220.0,
                row_y: 160.0,
                child_width: 160.0,
                child_height: 100.0,
                child_gap: 3.0,
            }),
            PopupPoint { x: 303.0, y: 160.0 }
        );
        assert_eq!(
            placement
                .child_popup(ChildPopupInput {
                    parent_left: 320.0,
                    parent_width: 160.0,
                    row_y: 160.0,
                    child_width: 170.0,
                    child_height: 100.0,
                    child_gap: 3.0,
                })
                .x,
            147.0
        );
        assert_eq!(
            placement
                .child_popup(ChildPopupInput {
                    parent_left: 20.0,
                    parent_width: 80.0,
                    row_y: 160.0,
                    child_width: 600.0,
                    child_height: 100.0,
                    child_gap: 3.0,
                })
                .x,
            12.0
        );
    }

    #[test]
    fn anchored_popup_above_clamps_without_pointer_flip() {
        let placement = PopupPlacement::new(512.0, 512.0, 12.0, 8.0);

        assert_eq!(
            placement.anchored_popup_above(100.0, 300.0, 180.0, 120.0, 3.0),
            PopupPoint { x: 100.0, y: 177.0 }
        );
        assert_eq!(
            placement.anchored_popup_above(480.0, 300.0, 180.0, 120.0, 3.0),
            PopupPoint { x: 320.0, y: 177.0 }
        );
        assert_eq!(
            placement.anchored_popup_above(100.0, 40.0, 180.0, 120.0, 3.0),
            PopupPoint { x: 100.0, y: 12.0 }
        );
    }

    #[test]
    fn submenu_hover_bridge_spans_parent_child_gap() {
        let placement = PopupPlacement::new(512.0, 512.0, 12.0, 8.0);

        assert_eq!(
            placement.hover_bridge(HoverBridgeInput {
                parent_left: 100.0,
                parent_width: 220.0,
                child_left: 340.0,
                child_width: 180.0,
                row_y: 160.0,
                child_top: 340.0,
                row_height: 38.0,
                title_height: 30.0,
                child_gap: 3.0,
            }),
            PopupRect {
                x: 320.0,
                width: 20.0,
                y: 156.0,
                height: 256.0,
            }
        );
        assert_eq!(
            placement.hover_bridge(HoverBridgeInput {
                parent_left: 300.0,
                parent_width: 180.0,
                child_left: 110.0,
                child_width: 170.0,
                row_y: 420.0,
                child_top: 110.0,
                row_height: 38.0,
                title_height: 30.0,
                child_gap: 3.0,
            }),
            PopupRect {
                x: 280.0,
                width: 20.0,
                y: 106.0,
                height: 356.0,
            }
        );
    }

    #[test]
    fn submenu_hover_bridge_keeps_clamped_child_reachable() {
        let placement = PopupPlacement::new(420.0, 220.0, 12.0, 8.0);
        let row_y = 150.0;
        let row_height = 38.0;
        let child = placement.child_popup(ChildPopupInput {
            parent_left: 40.0,
            parent_width: 180.0,
            row_y,
            child_width: 160.0,
            child_height: 180.0,
            child_gap: 3.0,
        });
        assert_eq!(child, PopupPoint { x: 223.0, y: 28.0 });

        let bridge = placement.hover_bridge(HoverBridgeInput {
            parent_left: 40.0,
            parent_width: 180.0,
            child_left: child.x,
            child_width: 160.0,
            row_y,
            child_top: child.y,
            row_height,
            title_height: 0.0,
            child_gap: 3.0,
        });

        assert_eq!(bridge.x, 220.0);
        assert_eq!(bridge.width, 3.0);
        assert!(bridge.y <= child.y);
        assert!(bridge.y <= row_y);
        assert!(bridge.y + bridge.height >= row_y + row_height);
    }

    fn assert_bridge_covers_parent_row_and_child_first_row(
        bridge: PopupRect,
        row_y: f32,
        child_top: f32,
        row_height: f32,
    ) {
        assert!(
            bridge.y <= row_y,
            "bridge should cover the parent submenu row top"
        );
        assert!(
            bridge.y + bridge.height >= row_y + row_height,
            "bridge should cover the parent submenu row bottom"
        );
        assert!(
            bridge.y <= child_top,
            "bridge should cover the child menu top after vertical clamp"
        );
        assert!(
            bridge.y + bridge.height >= child_top + row_height,
            "bridge should cover the first child menu action row"
        );
    }

    #[test]
    fn submenu_hover_bridge_covers_real_paths_after_flip_and_clamp() {
        let cases = [
            (80.0, 140.0, 120.0, 260.0, 520.0, 520.0),
            (80.0, 140.0, 460.0, 260.0, 520.0, 520.0),
            (330.0, 150.0, 120.0, 220.0, 520.0, 520.0),
            (330.0, 150.0, 20.0, 260.0, 520.0, 210.0),
        ];
        let row_height = MENU_ITEM_HEIGHT;

        for (parent_left, parent_width, row_y, child_height, view_width, view_height) in cases {
            let placement = PopupPlacement::new(view_width, view_height, 12.0, 8.0);
            let child = placement.child_popup(ChildPopupInput {
                parent_left,
                parent_width,
                row_y,
                child_width: 170.0,
                child_height,
                child_gap: 3.0,
            });
            let bridge = placement.hover_bridge(HoverBridgeInput {
                parent_left,
                parent_width,
                child_left: child.x,
                child_width: 170.0,
                row_y,
                child_top: child.y,
                row_height,
                title_height: 0.0,
                child_gap: 3.0,
            });

            assert!(
                bridge.width >= 3.0,
                "bridge should keep at least the child menu gap hittable"
            );
            assert_bridge_covers_parent_row_and_child_first_row(bridge, row_y, child.y, row_height);
        }
    }

    #[test]
    fn menu_geometry_wrappers_match_popup_placement() {
        assert_eq!(
            RootMenuGeometry {
                view_width: 512.0,
                view_height: 512.0,
                anchor_x: 480.0,
                anchor_y: 100.0,
                menu_width: 180.0,
                menu_height: 180.0,
                margin: 12.0,
                pointer_gap: 8.0,
            }
            .popup(),
            PopupPoint { x: 292.0, y: 108.0 }
        );

        assert_eq!(
            AnchoredMenuGeometry {
                view_width: 512.0,
                view_height: 512.0,
                anchor_x: 480.0,
                anchor_y: 300.0,
                menu_width: 180.0,
                menu_height: 120.0,
                margin: 12.0,
                pointer_gap: 8.0,
                gap: 3.0,
            }
            .popup(),
            PopupPoint { x: 320.0, y: 177.0 }
        );

        assert_eq!(
            ChildMenuGeometry {
                view_width: 512.0,
                view_height: 512.0,
                parent_left: 320.0,
                parent_width: 160.0,
                row_y: 160.0,
                child_width: 170.0,
                child_height: 100.0,
                margin: 12.0,
                pointer_gap: 8.0,
                child_gap: 3.0,
            }
            .popup(),
            PopupPoint { x: 147.0, y: 160.0 }
        );

        assert_eq!(
            ChildBridgeGeometry {
                view_width: 512.0,
                view_height: 512.0,
                parent_left: 300.0,
                parent_width: 180.0,
                child_left: 110.0,
                child_width: 170.0,
                row_y: 420.0,
                child_top: 110.0,
                row_height: 38.0,
                title_height: 30.0,
                margin: 12.0,
                pointer_gap: 8.0,
                child_gap: 3.0,
            }
            .rect(),
            PopupRect {
                x: 280.0,
                width: 20.0,
                y: 106.0,
                height: 356.0,
            }
        );
    }

    #[test]
    fn transfer_menu_geometry_uses_shared_root_popup_rules() {
        let menu_width = 240.0;
        let menu_height = 30.0 + 4.0 * 38.0 + 8.0;

        assert_eq!(
            RootMenuGeometry {
                view_width: 800.0,
                view_height: 600.0,
                anchor_x: 100.0,
                anchor_y: 100.0,
                menu_width,
                menu_height,
                margin: 12.0,
                pointer_gap: 8.0,
            }
            .popup(),
            PopupPoint { x: 108.0, y: 108.0 }
        );
        assert_eq!(
            RootMenuGeometry {
                view_width: 800.0,
                view_height: 600.0,
                anchor_x: 790.0,
                anchor_y: 590.0,
                menu_width,
                menu_height,
                margin: 12.0,
                pointer_gap: 8.0,
            }
            .popup(),
            PopupPoint { x: 542.0, y: 392.0 }
        );
        assert_eq!(
            RootMenuGeometry {
                view_width: 200.0,
                view_height: 120.0,
                anchor_x: 10.0,
                anchor_y: 10.0,
                menu_width,
                menu_height,
                margin: 12.0,
                pointer_gap: 8.0,
            }
            .popup(),
            PopupPoint { x: 12.0, y: 12.0 }
        );
    }

    #[test]
    fn place_drop_geometry_distinguishes_gaps_and_items() {
        assert_eq!(
            place_drop_geometry(108.0, 3, 108.0, 38.0),
            PlaceDropGeometry {
                target_index: 0,
                slot: 0,
                row_offset: 0.0,
                over_gap: true,
                over_item: false,
            }
        );
        assert_eq!(
            place_drop_geometry(126.0, 3, 108.0, 38.0),
            PlaceDropGeometry {
                target_index: 0,
                slot: 0,
                row_offset: 18.0,
                over_gap: false,
                over_item: true,
            }
        );
        assert_eq!(
            place_drop_geometry(143.0, 3, 108.0, 38.0),
            PlaceDropGeometry {
                target_index: 0,
                slot: 1,
                row_offset: 35.0,
                over_gap: true,
                over_item: false,
            }
        );
    }

    #[test]
    fn place_drop_geometry_clamps_outside_list() {
        assert_eq!(
            place_drop_geometry(90.0, 3, 108.0, 38.0),
            PlaceDropGeometry {
                target_index: 0,
                slot: 0,
                row_offset: 0.0,
                over_gap: true,
                over_item: false,
            }
        );
        assert_eq!(
            place_drop_geometry(500.0, 3, 108.0, 38.0),
            PlaceDropGeometry {
                target_index: 2,
                slot: 3,
                row_offset: 38.0,
                over_gap: true,
                over_item: false,
            }
        );
        assert_eq!(
            place_drop_geometry(120.0, 0, 108.0, 38.0),
            PlaceDropGeometry {
                target_index: -1,
                slot: 0,
                row_offset: 0.0,
                over_gap: true,
                over_item: false,
            }
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectionRect {
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
    pub(crate) rows_per_column: i32,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RectBounds {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl RectBounds {
    pub(crate) fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }
}
