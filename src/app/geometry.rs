use crate::AppWindow;
use slint::ComponentHandle;
use std::ops::Range;

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
        let search_panel_height = search_panel_height(
            ui.get_search_bar_open(),
            ui.get_search_query().as_str(),
            ui.get_search_kind_filter(),
            ui.get_search_modified_filter(),
            ui.get_search_size_filter(),
        );
        let available_grid_height =
            (pane.bottom - pane.top - 36.0 - search_panel_height - 2.0 * padding).max(row_height);
        let rows_per_column = (available_grid_height / row_height).floor().max(1.0) as usize;

        Self {
            main_x: pane.left,
            main_y: pane.top + search_panel_height,
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

pub(crate) fn main_pane_bounds(
    sidebar_width_px: f32,
    window_width: f32,
    window_height: f32,
) -> MainPaneBounds {
    MainPaneBounds {
        left: sidebar_width_px + 8.0,
        top: 64.0,
        right: window_width.max(sidebar_width_px + 8.0),
        bottom: window_height.max(64.0),
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
        overscan_columns,
    );
    let visible_range = virtual_entry_range(
        entry_count,
        rows_per_column,
        viewport_x,
        viewport_width,
        cell_width,
        0,
    );
    let start_column = range.start / rows_per_column.max(1);

    VirtualGridPlan {
        viewport_x,
        scroll_max_x,
        range,
        visible_range,
        start_column,
    }
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
) -> f32 {
    let filters_active =
        search_kind_filter != 0 || search_modified_filter != 0 || search_size_filter != 0;
    if search_bar_open || !search_query.is_empty() || filters_active {
        134.0
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
    overscan_columns: usize,
) -> Range<usize> {
    if entry_count == 0 {
        return 0..0;
    }

    let rows_per_column = rows_per_column.max(1);
    let cell_width = cell_width.max(1.0);
    let first_visible_column = (viewport_x.max(0.0) / cell_width).floor().max(0.0) as usize;
    let visible_columns = (viewport_width.max(1.0) / cell_width).ceil().max(1.0) as usize;
    let start_column = first_visible_column.saturating_sub(overscan_columns);
    let end_column = first_visible_column + visible_columns + overscan_columns + 1;

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
    ui.on_root_menu_left(
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

    ui.on_root_menu_top(
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

    ui.on_anchored_menu_left(
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

    ui.on_anchored_menu_top(
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

    ui.on_child_menu_left(
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

    ui.on_child_menu_top(
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

    ui.on_child_bridge_left(
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

    ui.on_child_bridge_top(
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

    ui.on_child_bridge_width(
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

    ui.on_child_bridge_height(
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

    ui.on_context_menu_height(
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
            .height
        },
    );

    ui.on_context_menu_open_with_row_offset(
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
            .open_with_row_y_offset
        },
    );

    ui.on_context_menu_create_new_row_offset(
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
            .create_new_row_y_offset
        },
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
        PopupRect, RootMenuGeometry, context_menu_metrics, main_pane_bounds, main_scroll_max_x,
        place_drop_geometry, search_panel_height, virtual_entry_range, virtual_grid_plan,
    };

    #[test]
    fn search_panel_height_matches_slint_visibility_rules() {
        assert_eq!(search_panel_height(false, "", 0, 0, 0), 0.0);
        assert_eq!(search_panel_height(true, "", 0, 0, 0), 134.0);
        assert_eq!(search_panel_height(false, "png", 0, 0, 0), 134.0);
        assert_eq!(search_panel_height(false, "", 1, 0, 0), 134.0);
        assert_eq!(search_panel_height(false, "", 0, 2, 0), 134.0);
        assert_eq!(search_panel_height(false, "", 0, 0, 3), 134.0);
    }

    #[test]
    fn main_pane_bounds_match_slint_shell_layout() {
        let bounds = main_pane_bounds(320.0, 1100.0, 760.0);

        assert_eq!(bounds.left, 328.0);
        assert_eq!(bounds.top, 64.0);
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
        assert_eq!(virtual_entry_range(100, 4, 0.0, 250.0, 100.0, 1), 0..20);
        assert_eq!(virtual_entry_range(100, 4, 350.0, 250.0, 100.0, 1), 8..32);
        assert_eq!(virtual_entry_range(10, 4, 800.0, 250.0, 100.0, 1), 10..10);
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
    fn virtual_grid_plan_clamps_viewport_and_reports_anchor_column() {
        let plan = virtual_grid_plan(100, 4, 350.0, 250.0, 100.0, 10.0, 2);
        assert_eq!(plan.viewport_x, 350.0);
        assert_eq!(plan.scroll_max_x, 2270.0);
        assert_eq!(plan.visible_range, 12..28);
        assert_eq!(plan.range, 4..36);
        assert_eq!(plan.start_column, 1);

        let clamped = virtual_grid_plan(10, 4, 800.0, 250.0, 100.0, 10.0, 2);
        assert_eq!(clamped.viewport_x, 70.0);
        assert_eq!(clamped.scroll_max_x, 70.0);
        assert_eq!(clamped.visible_range, 0..10);
        assert_eq!(clamped.range, 0..10);
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
