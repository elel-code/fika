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
const COMPACT_ITEM_PADDING: f32 = 2.0;
const COMPACT_COLUMN_MARGIN_WIDTH: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MainItemViewLayout {
    pub(crate) viewport_x: f32,
    pub(crate) viewport_width: f32,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CompactItemViewLayout {
    pub(crate) entry_count: usize,
    pub(crate) rows_per_column: usize,
    pub(crate) viewport_width: f32,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
    pub(crate) content_width: f32,
    pub(crate) scroll_max_x: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VirtualItemViewPlan {
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

impl MainItemViewLayout {
    pub(crate) fn from_ui_for_pane_width_with_text_lines(
        ui: &AppWindow,
        pane_width: f32,
        search_panel_visible: bool,
        text_line_count: usize,
    ) -> Self {
        let cell_width = compact_cell_width(ui.get_icon_zoom_level());
        let row_height = compact_row_height(ui.get_icon_zoom_level(), text_line_count);
        let padding = COMPACT_COLUMN_MARGIN_WIDTH;
        let window_size = ui.window().size().to_logical(ui.window().scale_factor());
        let pane = main_pane_bounds(
            ui.get_sidebar_width_px(),
            window_size.width,
            window_size.height,
        );
        let search_panel_height = if search_panel_visible {
            search_panel_height(
                ui.get_search_bar_open(),
                ui.get_search_query().as_str(),
                ui.get_search_kind_filter(),
                ui.get_search_modified_filter(),
                ui.get_search_size_filter(),
                pane_width,
            )
        } else {
            0.0
        };
        let viewport_height = (pane.bottom
            - pane.top
            - PATH_BAR_HEIGHT
            - STATUS_BAR_HEIGHT
            - search_panel_height
            - 2.0 * padding)
            .max(row_height);
        let rows_per_column = (viewport_height / row_height).floor().max(1.0) as usize;
        Self {
            viewport_x: 0.0,
            viewport_width: pane_width.max(1.0),
            rows_per_column,
            cell_width,
            row_height,
            padding,
        }
    }

    pub(crate) fn compact_item_view(self, entry_count: usize) -> CompactItemViewLayout {
        compact_item_view_layout(
            self.viewport_width,
            entry_count,
            self.rows_per_column,
            self.cell_width,
            self.row_height,
            self.padding,
        )
    }
}

impl CompactItemViewLayout {
    pub(crate) fn virtual_plan(
        self,
        requested_viewport_x: f32,
        overscan_columns: usize,
    ) -> VirtualItemViewPlan {
        let viewport_x = requested_viewport_x.clamp(0.0, self.scroll_max_x);
        let (range, visible_range) = virtual_entry_ranges(
            self.entry_count,
            self.rows_per_column,
            viewport_x,
            self.viewport_width,
            self.column_width,
            self.column_offset,
            self.padding,
            overscan_columns,
        );
        let start_column = range.start / self.rows_per_column.max(1);

        VirtualItemViewPlan {
            viewport_x,
            scroll_max_x: self.scroll_max_x,
            range,
            visible_range,
            start_column,
            rows_per_column: self.rows_per_column,
            cell_width: self.cell_width,
        }
    }

    pub(crate) fn virtual_slice_width(self, virtual_slice_count: usize) -> f32 {
        compact_item_view_virtual_slice_width(
            virtual_slice_count,
            self.rows_per_column,
            self.column_width,
            self.column_offset,
            self.cell_width,
        )
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

pub(crate) fn compact_cell_width(zoom_level: i32) -> f32 {
    let icon_size = compact_media_size(zoom_level);
    let font_height = compact_title_font_height(zoom_level);
    COMPACT_ITEM_PADDING * 4.0 + icon_size + font_height * 5.0
}

pub(crate) fn compact_item_view_layout(
    viewport_width: f32,
    entry_count: usize,
    rows_per_column: usize,
    cell_width: f32,
    row_height: f32,
    padding: f32,
) -> CompactItemViewLayout {
    let viewport_width = viewport_width.max(1.0);
    let rows_per_column = rows_per_column.max(1);
    let cell_width = cell_width.max(1.0);
    let row_height = row_height.max(1.0);
    let padding = padding.max(0.0);

    // Dolphin CompactLayout scrolls horizontally: rows fill the physical height,
    // then each completed column advances on the X axis by item width + margin.
    let item_margin = COMPACT_COLUMN_MARGIN_WIDTH;
    let column_width = (cell_width + item_margin).max(1.0);
    let column_offset = 0.0;
    let column_count = entry_count.div_ceil(rows_per_column).max(1);
    let content_width = compact_item_view_content_width(
        column_count,
        column_width,
        column_offset,
        cell_width,
        padding,
    );
    let scroll_max_x = (content_width - viewport_width).max(0.0);

    CompactItemViewLayout {
        entry_count,
        rows_per_column,
        viewport_width,
        cell_width,
        column_width,
        column_offset,
        row_height,
        padding,
        content_width,
        scroll_max_x,
    }
}

pub(crate) fn compact_item_view_virtual_slice_width(
    virtual_slice_count: usize,
    rows_per_column: usize,
    column_width: f32,
    column_offset: f32,
    cell_width: f32,
) -> f32 {
    let rows_per_column = rows_per_column.max(1);
    let column_count = virtual_slice_count.div_ceil(rows_per_column).max(1);
    (column_offset.max(0.0)
        + (column_count.saturating_sub(1)) as f32 * column_width.max(1.0)
        + cell_width.max(1.0))
    .max(1.0)
}

fn compact_item_view_content_width(
    column_count: usize,
    column_width: f32,
    column_offset: f32,
    cell_width: f32,
    padding: f32,
) -> f32 {
    (2.0 * padding.max(0.0)
        + column_offset.max(0.0)
        + (column_count.saturating_sub(1)) as f32 * column_width.max(1.0)
        + cell_width.max(1.0))
    .max(1.0)
}

pub(crate) fn compact_row_height(zoom_level: i32, text_line_count: usize) -> f32 {
    let icon_size = compact_media_size(zoom_level);
    let text_block_height = compact_text_block_height(zoom_level, text_line_count);
    COMPACT_ITEM_PADDING * 2.0 + icon_size.max(text_block_height)
}

fn compact_media_size(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 28.0,
        1 => 36.0,
        2 => 46.0,
        3 => 58.0,
        _ => 72.0,
    }
}

fn compact_title_line_height(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 18.0,
        1 => 19.0,
        2 => 21.0,
        3 => 22.0,
        _ => 24.0,
    }
}

fn compact_title_font_height(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 12.0,
        1 => 13.0,
        2 => 15.0,
        3 => 16.0,
        _ => 18.0,
    }
}

fn compact_metadata_line_height(zoom_level: i32) -> f32 {
    if zoom_level < 2 { 13.0 } else { 14.0 }
}

fn compact_text_block_height(zoom_level: i32, text_line_count: usize) -> f32 {
    let text_line_count = text_line_count.max(1);
    let title_line_height = compact_title_line_height(zoom_level);
    if text_line_count == 1 {
        return title_line_height;
    }

    let metadata_lines = text_line_count.saturating_sub(1) as f32;
    let spacing = 2.0 * text_line_count.saturating_sub(1) as f32;
    title_line_height + metadata_lines * compact_metadata_line_height(zoom_level) + spacing
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

fn virtual_entry_ranges(
    entry_count: usize,
    rows_per_column: usize,
    viewport_x: f32,
    viewport_width: f32,
    column_width: f32,
    column_offset: f32,
    padding: f32,
    overscan_columns: usize,
) -> (Range<usize>, Range<usize>) {
    if entry_count == 0 {
        return (0..0, 0..0);
    }

    let rows_per_column = rows_per_column.max(1);
    let column_width = column_width.max(1.0);
    let viewport_x = viewport_x.max(0.0);
    let viewport_width = viewport_width.max(1.0);
    let content_x = (viewport_x - padding.max(0.0) - column_offset.max(0.0)).max(0.0);
    let content_end_x = (viewport_x + viewport_width - padding.max(0.0) - column_offset.max(0.0))
        .max(content_x + 1.0);
    let first_visible_column = (content_x / column_width).floor() as usize;
    let visible_end_column = (content_end_x / column_width)
        .ceil()
        .max(first_visible_column as f32 + 1.0) as usize;
    let start_column = first_visible_column.saturating_sub(overscan_columns);
    let end_column = visible_end_column + overscan_columns;

    (
        entry_range_for_columns(start_column, end_column, rows_per_column, entry_count),
        entry_range_for_columns(
            first_visible_column,
            visible_end_column,
            rows_per_column,
            entry_count,
        ),
    )
}

fn entry_range_for_columns(
    start_column: usize,
    end_column: usize,
    rows_per_column: usize,
    entry_count: usize,
) -> Range<usize> {
    let start = (start_column * rows_per_column).min(entry_count);
    let end = (end_column * rows_per_column).min(entry_count);
    start..end.max(start)
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
    pub(crate) service_action_count: i32,
    pub(crate) service_submenu_count: i32,
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
                 service_action_count,
                 service_submenu_count,
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
                        service_action_count,
                        service_submenu_count,
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
    let service_rows = service_rows_height(input, item, separator, title);
    if input.selected_count > 1 {
        let action_rows = if input.in_trash { 4.0 } else { 3.0 };
        return MenuMetrics {
            height: title + action_rows * item + service_rows + separator,
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
        height: item_count as f32 * item + service_rows + 2.0 * separator,
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
    let service_rows = service_rows_height(input, item, separator, input.title_height.max(0.0));
    if input.in_trash {
        return MenuMetrics {
            height: 2.0 * item + service_rows + separator,
            open_with_row_y_offset: 0.0,
            create_new_row_y_offset: 0.0,
        };
    }

    MenuMetrics {
        height: 5.0 * item + service_rows + separator,
        open_with_row_y_offset: 3.0 * item + separator,
        create_new_row_y_offset: 0.0,
    }
}

fn service_rows_height(input: MenuMetricsInput, item: f32, separator: f32, title: f32) -> f32 {
    let _ = (separator, title);
    (input.service_action_count.max(0) + input.service_submenu_count.max(0)) as f32 * item
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
        compact_cell_width, compact_item_view_layout, compact_row_height, context_menu_metrics,
        inactive_main_pane_width, main_pane_bounds, place_drop_geometry, search_panel_height,
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
            ("bool", "service-menu-open"),
            ("length", "service-menu-row-y"),
            ("int", "close-kind"),
        ];
        let lifecycle_functions = [
            "cancel-close",
            "close-child-submenu",
            "close-child-submenus",
            "begin-close",
            "show-open-with",
            "show-create-new",
            "show-service-menu",
            "close-pending-child-submenu",
        ];
        let controller_functions = [
            "stop-close-timer",
            "close-child-submenus",
            "set-child-submenu-hover",
            "show-child-submenu",
            "show-open-with-submenu",
            "show-create-new-submenu",
            "show-context-service-submenu",
            "open-with-submenu-hover",
            "create-new-submenu-hover",
            "context-service-submenu-hover",
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
            "show-context-service-submenu",
            "open-with-submenu-hover",
            "create-new-submenu-hover",
            "context-service-submenu-hover",
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
            "ServiceActionMenuRow",
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
            5,
            "file, viewport, and service-menu group rows should reuse the submenu row"
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
        let service_action_row_start = menus
            .find("component ServiceActionMenuRow")
            .expect("ServiceActionMenuRow should exist after ActionMenuRow");
        let hover_action_row_start = menus
            .find("component HoverActionMenuRow")
            .expect("HoverActionMenuRow should exist after ActionMenuRow");
        let paste_row_start = menus
            .find("component PasteMenuRow")
            .expect("PasteMenuRow should exist after HoverActionMenuRow");
        let action_row = &menus[action_row_start..service_action_row_start];
        let service_action_row = &menus[service_action_row_start..hover_action_row_start];
        let hover_action_row = &menus[hover_action_row_start..paste_row_start];
        assert!(
            !action_row.contains("callback hovered(bool);")
                && !action_row.contains("hovered(is-hovered) =>"),
            "ordinary ActionMenuRow must not participate in child-submenu keep-alive or delayed close"
        );
        assert!(
            service_action_row.contains("callback submenu_hovered(string, length, bool);")
                && service_action_row.contains("SubmenuMenuRow {")
                && service_action_row.contains(
                    "root.submenu_hovered(root.action.group, self.absolute-position.y, is-hovered);"
                ),
            "ServiceActionMenuRow should forward hover only for service-menu submenu parents"
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
            "file_service_submenu_hovered",
            "viewport_service_submenu_hovered",
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
            menu_lifecycle.contains("} else if (menu == 1 || menu == 2 || menu == 3) {\n            MenuLifecycle.begin-close(menu);\n            close-timer.start();\n        }"),
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
            app.contains("in-out property <string> focused_pane_path;")
                && app.contains(
                    "private property <string> sidebar-selected-path: root.focused_pane_path;"
                )
                && app.contains("selected: root.sidebar-selected-path == place.path;")
                && app.contains("selected: root.sidebar-selected-path == device.path;")
                && !app.contains("selected: root.current_path == place.path;")
                && !app.contains("selected: root.current_path == device.path;")
                && !app.contains("sidebar-selected-path: root.focused_pane == 1"),
            "sidebar places/devices highlight should follow the Rust-synced focused pane path immediately"
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
            app.contains("private property <length> sidebar-bottom-gap: 22px;")
                && app.contains(
                    "private property <length> sidebar-content-bottom-padding: 28px;"
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
            !app.contains("changed viewport_x => { root.main_view_changed(); }"),
            "main viewport scrolling should not separately trigger a duplicate virtual refresh"
        );
        assert!(
            status_bar.contains("label: \"Admin Save\";")
                && status_bar.contains("width: 104px;")
                && status_bar.contains("text: \"ADMIN\";")
                && !status_bar.contains("text: \"ADMIN EDIT\";")
                && status_bar.contains("private property <color> admin-badge-bg")
                && status_bar.contains("private property <color> admin-badge-border")
                && !status_bar.contains("label: \"Save Back\";"),
            "status bar should expose a clear admin write-back marker and save action"
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
                && app.contains("in property <[PaneSlotData]> pane_slots;")
                && app.contains("for pane[index] in root.pane_slots : PaneSlotSurface")
                && app.contains("current-path: root.pane.current_path;")
                && app.contains("if (root.search-panel-visible) : SearchPanel")
                && app.contains("SplitPaneView {"),
            "pane content height should subtract the pane-local path bar, search filters, and status bar inside the reusable file pane"
        );
        assert!(
            app.contains("private property <length> search-panel-height: root.search-panel-visible ? (root.pane-slot-0-width < 760px ? 78px : 44px) : 0px;"),
            "search filters should size against the rendered pane slot width instead of squeezing another split pane"
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
        let path_text_focus_body = path_bar_component
            .split_once("changed has-focus => {")
            .expect("PathBar TextInput should track focus changes")
            .1
            .split_once("if (self.has-focus && root.editor-text == \"\")")
            .expect("PathBar focus block should prepare empty path text after focus handling")
            .0;
        assert!(
            path_text_focus_body.contains("root.path_focus_changed(self.has-focus);")
                && !path_text_focus_body.contains("root.focus_requested();"),
            "PathBar TextInput focus should mark pane-local path focus without stealing keyboard focus from the address editor"
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
    fn split_pane_ui_uses_equal_reusable_pane_slot_surfaces() {
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
            .split_once("component PaneSlotSurface inherits Rectangle")
            .expect("PaneSlot should be defined before AppWindow")
            .0;
        let pane_slot_surface = app
            .split_once("component PaneSlotSurface inherits Rectangle {")
            .expect("PaneSlotSurface should wrap PaneSlot with pane data bindings")
            .1
            .split_once("export component AppWindow inherits Window")
            .expect("PaneSlotSurface should be defined before AppWindow")
            .0;
        let route_functions = app
            .split_once("public function route-pane-focus(slot: int) {")
            .expect("AppWindow should expose shared pane route functions")
            .1
            .split_once("title: chooser_mode")
            .expect("pane route functions should be defined before the window body")
            .0;
        let context_menu_route = route_functions
            .split_once("public function route-pane-request-context-menu(slot: int,")
            .expect("pane route functions should include the item context menu route")
            .1
            .split_once("public function route-pane-request-blank-context-menu(slot: int,")
            .expect("item context menu route should be before the blank context menu route")
            .0;
        let blank_context_menu_route = route_functions
            .split_once("public function route-pane-request-blank-context-menu(slot: int,")
            .expect("pane route functions should include the blank context menu route")
            .1
            .split_once("public function route-pane-zoom-in(slot: int)")
            .expect("blank context menu route should be before pane zoom routes")
            .0;

        assert!(app.contains(
            "private property <bool> file-operation-shortcuts-blocked: root.search-input-focused || root.chooser-save-input-focused || root.transient-popup-open;"
        ));
        let paste_shortcut = app
            .split_once("keys: @keys(Control + V);")
            .expect("AppWindow should keep a Ctrl+V paste shortcut")
            .1
            .split_once("keys: @keys(Control + Z);")
            .expect("Ctrl+V shortcut should be before Ctrl+Z")
            .0;
        assert!(
            paste_shortcut.contains("root.paste_into(root.current_path);")
                && !paste_shortcut.contains("root.refresh_clipboard_availability();"),
            "Ctrl+V should request paste directly; the Rust paste path owns async clipboard import"
        );
        assert!(app.contains("import { SplitPaneView } from \"split_pane.slint\";"));
        assert!(app.contains("component PaneSlotSurface inherits Rectangle"));
        assert!(app.contains("private property <length> pane-slot-0-width"));
        assert!(app.contains("in property <[PaneSlotData]> pane_slots;"));
        assert!(app.contains(
            "private property <int> visible-pane-count: max(1, root.pane_slots.length);"
        ));
        assert!(app.contains("for pane[index] in root.pane_slots : PaneSlotSurface"));
        assert!(app.contains("private property <int> slot: pane.slot;"));
        assert!(app.contains("x: root.pane-slot-x(slot);"));
        assert!(app.contains("width: root.pane-slot-width(slot);"));
        assert!(app.contains("pane: pane;"));
        assert!(app.contains("focused: root.focused_pane == slot;"));
        assert!(app.contains(
            "private property <length> pane-slot-1-x: root.pane-slot-0-width + root.split-divider-width;"
        ));
        assert!(app.contains(
            "private property <length> pane-slot-1-width: root.split_view_open ? max(1px, root.main-pane-width - root.pane-slot-1-x) : 0px;"
        ));
        assert!(!app.contains("inactive_pane_viewport_x"));
        assert!(app.contains("pane-shells := Rectangle"));
        assert!(!app.contains("function pane-slot-current-path(slot: int) -> string"));
        assert!(!app.contains("pane-current-path: root.pane-slot-current-path(slot);"));
        assert!(
            !app.contains("pane-virtual-start-index: root.pane-slot-virtual-start-index(slot);")
        );
        assert!(!app.contains("pane-viewport-offset: root.pane-slot-viewport-offset(slot);"));
        assert!(!app.contains("function set-pane-slot-path-text(slot: int, text: string)"));
        assert!(!app.contains("function set-pane-slot-path-focused(slot: int, focused: bool)"));
        assert!(!app.contains(
            "function set-pane-slot-viewport(slot: int, viewport-x: float, viewport-offset: length)"
        ));
        assert!(!app.contains("pane-slot-0-shell := PaneSlotSurface"));
        assert!(!app.contains("pane-slot-1-shell := PaneSlotSurface"));
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
            "pane blank input layer should be scoped inside the pane slot surface"
        );
        assert!(
            pane_routing.contains("callback focus(int);")
                && pane_routing.contains("callback path-submitted(int, string);")
                && pane_routing.contains("callback go-back(int);")
                && pane_routing.contains("callback go-forward(int);")
                && pane_routing.contains("callback view-changed(int);")
                && pane_routing.contains(
                    "callback item-view-item-pressed(int, float, float, bool, bool) -> bool;"
                )
                && pane_routing.contains("callback item-view-item-activated(int, float, float);")
                && pane_routing.contains(
                    "callback item-view-item-context-menu(int, float, float, length, length) -> bool;"
                )
                && pane_routing.contains(
                    "callback item-view-blank-pressed(int, float, float, int, float, float, float, float, float, bool);"
                )
                && pane_routing.contains("callback item-view-blank-moved(int, float, float) -> bool;")
                && pane_routing.contains("callback item-view-blank-released(int, float, float);")
                && pane_routing.contains("callback item-view-blank-cancelled(int);")
                && pane_routing.contains("callback request-blank-context-menu(int, length, length);")
                && pane_routing.contains("callback drop-target-path(int, float, float, string) -> string;")
                && pane_routing.contains("callback drop-target-slice-index(int, float, float, string) -> int;")
                && pane_routing.contains("callback drop-allowed(int, float, float, string) -> bool;")
                && pane_routing.contains("callback prepare-transfer(int, string, float, float) -> bool;")
                && !pane_routing.contains("callback activated")
                && !pane_routing.contains("request-select")
                && !pane_routing.contains("callback request-context-menu")
                && !pane_routing.contains("select-rect")
                && !pane_routing.contains("clear-selection")
                && !pane_routing.contains("is-selected"),
            "PaneRouting should expose one slot-aware surface for every pane interaction"
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
                && file_pane.contains("path-text: root.path-text;")
                && file_pane.contains("path-focused: root.path-focused;")
                && file_pane.contains("root.path-text = text;")
                && file_pane.contains("root.path-focused = focused;")
                && file_pane.contains("viewport-x <=> root.viewport-x;")
                && file_pane.contains("callback viewport_changed(int, float);")
                && file_pane.contains(
                    "root.viewport_changed(root.pane-slot, root.viewport-x);"
                )
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
            file_pane.contains("callback item_view_item_pressed")
                && file_pane.contains("callback item_view_item_activated")
                && file_pane.contains("callback item_view_item_context_menu")
                && file_pane.contains("callback request_blank_context_menu")
                && file_pane.contains("callback item_view_blank_pressed")
                && file_pane.contains("callback item_view_blank_moved")
                && file_pane.contains("callback item_view_blank_released")
                && file_pane.contains("callback item_view_blank_cancelled")
                && file_pane.contains("callback drop_target_path")
                && file_pane.contains("callback drop_target_slice_index")
                && file_pane.contains("callback drop_allowed")
                && file_pane.contains("callback prepare_transfer")
                && file_pane.contains("callback make_drag_data_at")
                && !file_pane.contains("callback request_context_menu")
                && !file_pane.contains("callback activated")
                && !file_pane.contains("callback request_select"),
            "FilePane should expose the full interactive surface shared by both panes"
        );
        assert!(
            file_pane.contains("in property <int> pane-slot: 0;")
                && file_pane.contains("callback focus_requested(int);")
                && file_pane.contains("callback path_submitted(int, string);")
                && file_pane.contains("callback go_back(int);")
                && file_pane.contains("callback go_forward(int);")
                && file_pane.contains("callback view_changed(int);")
                && file_pane.contains(
                    "callback item_view_item_pressed(int, float, float, bool, bool) -> bool;"
                )
                && file_pane.contains("callback item_view_item_activated(int, float, float);")
                && file_pane.contains(
                    "callback item_view_item_context_menu(int, float, float, length, length) -> bool;"
                )
                && file_pane.contains("callback item_view_blank_pressed(int,")
                && file_pane
                    .contains("pure callback make_drag_data_at(int, float, float) -> data-transfer;")
                && !file_pane.contains("pure callback is_selected"),
            "FilePane callbacks should carry the pane slot instead of baking in left/right behavior"
        );
        assert!(
            file_pane.contains("focus_requested => { root.focus_requested(root.pane-slot); }")
                && file_pane.contains("go_back => { root.go_back(root.pane-slot); }")
                && file_pane.contains("go_forward => { root.go_forward(root.pane-slot); }")
                && file_pane
                    .contains("path_submitted(path) => { root.path_submitted(root.pane-slot, path); }")
                && file_pane.contains("make_drag_data_at(x, y) => {\n                    root.make_drag_data_at(root.pane-slot, x, y)\n                }")
                && file_pane.contains("item_pressed(x, y, toggle, range) => {\n                    root.item_view_item_pressed(root.pane-slot, x, y, toggle, range)\n                }")
                && file_pane.contains("item_activated(x, y) => {\n                    root.item_view_item_activated(root.pane-slot, x, y);")
                && file_pane.contains("item_context_menu(x, y, abs-x, abs-y) => {\n                    root.item_view_item_context_menu(root.pane-slot, x, y, abs-x, abs-y)\n                }")
                && file_pane.contains("navigate_back => { root.go_back(root.pane-slot); }")
                && file_pane.contains("navigate_forward => { root.go_forward(root.pane-slot); }")
                && file_pane.contains(
                    "commit_external_edit => { root.commit_external_edit(root.pane-slot); }"
                )
                && file_pane.contains(
                    "discard_external_edit => { root.discard_external_edit(root.pane-slot); }"
                )
                && file_pane
                    .contains("save_focus_changed(focused) => { root.save_focus_changed(root.pane-slot, focused); }"),
            "FilePane should route address bar, content, side buttons, context menus, selection, and status through pane-slot"
        );
        let pane_slot_bindings = [
            "focus_requested(slot) => { PaneRouting.focus(slot); }",
            "path_submitted(slot, path) => { PaneRouting.path-submitted(slot, path); }",
            "go_back(slot) => { PaneRouting.go-back(slot); }",
            "go_forward(slot) => { PaneRouting.go-forward(slot); }",
            "search_submitted(query) => { PaneRouting.search-submitted(query); }",
            "cancel_search => { PaneRouting.cancel-search(); }",
            "search_close_requested => { PaneRouting.search-close-requested(); }",
            "view_changed(slot) => { PaneRouting.view-changed(slot); }",
            "item_view_item_pressed(slot, x, y, toggle, range) => {\n        PaneRouting.item-view-item-pressed(slot, x, y, toggle, range)\n    }",
            "item_view_item_activated(slot, x, y) => {\n        PaneRouting.item-view-item-activated(slot, x, y);\n    }",
            "item_view_item_context_menu(slot, x, y, abs-x, abs-y) => {\n        PaneRouting.item-view-item-context-menu(slot, x, y, abs-x, abs-y)\n    }",
            "item_view_blank_pressed(slot, x, y, rows-per-column, cell-width, column-width, column-offset, row-height, padding, toggle) => {\n        PaneRouting.item-view-blank-pressed(slot, x, y, rows-per-column, cell-width, column-width, column-offset, row-height, padding, toggle);\n    }",
            "item_view_blank_moved(slot, x, y) => {\n        PaneRouting.item-view-blank-moved(slot, x, y)\n    }",
            "item_view_blank_released(slot, x, y) => {\n        PaneRouting.item-view-blank-released(slot, x, y);\n    }",
            "item_view_blank_cancelled(slot) => {\n        PaneRouting.item-view-blank-cancelled(slot);\n    }",
            "request_blank_context_menu(slot, x, y) => {\n        PaneRouting.request-blank-context-menu(slot, x, y);\n    }",
            "zoom_in(slot) => { PaneRouting.zoom-in(slot); }",
            "zoom_out(slot) => { PaneRouting.zoom-out(slot); }",
            "drop_target_path(slot, x, y, source) => {\n        PaneRouting.drop-target-path(slot, x, y, source)\n    }",
            "drop_target_slice_index(slot, x, y, source) => {\n        PaneRouting.drop-target-slice-index(slot, x, y, source)\n    }",
            "drop_allowed(slot, x, y, source) => {\n        PaneRouting.drop-allowed(slot, x, y, source)\n    }",
            "prepare_transfer(slot, source, x, y) => {\n        PaneRouting.prepare-transfer(slot, source, x, y)\n    }",
            "transfer_menu_requested(slot) => { PaneRouting.transfer-menu-requested(slot); }",
            "trace_drop(action, kind, path, x, y, rejected, target) => {\n        PaneRouting.trace-drop(action, kind, path, x, y, rejected, target);\n    }",
            "save_focus_changed(slot, focused) => { PaneRouting.save-focus-changed(slot, focused); }",
            "commit_external_edit(slot) => { PaneRouting.commit-external-edit(slot); }",
            "discard_external_edit(slot) => { PaneRouting.discard-external-edit(slot); }",
            "undo_last_operation => { PaneRouting.undo-last-operation(); }",
            "chooser_accept(value) => { PaneRouting.chooser-accept(value); }",
            "chooser_filter_requested(slot, x, y) => { PaneRouting.chooser-filter-requested(slot, x, y); }",
            "chooser_choice_requested(slot, index, x, y) => {\n        PaneRouting.chooser-choice-requested(slot, index, x, y);\n    }",
            "make_drag_data_at(slot, x, y) => {\n        DndApi.make-drag-at(slot, x, y)\n    }",
        ];
        for binding in pane_slot_bindings {
            assert!(
                pane_slot.contains(binding),
                "PaneSlot should own shared pane event routing: {binding}"
            );
        }
        assert!(
            pane_slot_surface.contains("PaneSlot {")
                && pane_slot_surface.contains("in property <PaneSlotData> pane;")
                && pane_slot_surface.contains("in property <PaneViewData> view;")
                && pane_slot_surface.contains("in property <[ItemViewEntry]> entries;")
                && pane_slot_surface.contains("in property <[ItemViewHighlightEntry]> highlights;")
                && pane_slot_surface.contains("pane-slot: root.pane.slot;")
                && pane_slot_surface.contains("current-path: root.pane.current_path;")
                && pane_slot_surface
                    .contains("private property <string> live-path-text: root.pane.path_text;")
                && pane_slot_surface
                    .contains("private property <bool> live-path-focused: root.pane.path_focused;")
                && pane_slot_surface.contains("path-text <=> root.live-path-text;")
                && pane_slot_surface.contains("path-focused <=> root.live-path-focused;")
                && pane_slot_surface.contains("can-go-back: root.pane.can_go_back;")
                && pane_slot_surface.contains("can-go-forward: root.pane.can_go_forward;")
                && pane_slot_surface
                    .contains("private property <float> live-viewport-x: root.view.viewport_x;")
                && pane_slot_surface.contains(
                    "root.live-slot != root.pane.slot || root.live-current-path != root.pane.current_path"
                )
                && pane_slot_surface.contains("changed view => {")
                && pane_slot_surface.contains(
                    "root.live-slot != root.view.slot || root.live-viewport-x != root.view.viewport_x"
                )
                && pane_slot_surface.contains("viewport-x <=> root.live-viewport-x;")
                && pane_slot_surface.contains("entries: root.entries;")
                && pane_slot_surface.contains("highlights: root.highlights;")
                && pane_slot_surface.contains("callback viewport_changed(int, float);")
                && pane_slot_surface.contains(
                    "viewport_changed(slot, viewport-x) => {\n            root.viewport_changed(slot, viewport-x);\n        }"
                )
                && pane_slot_surface.contains("private property <bool> live-drag-active: false;")
                && pane_slot_surface
                    .contains("private property <int> live-drag-target-slice-index: -1;")
                && pane_slot_surface.contains("drag-active <=> root.live-drag-active;")
                && pane_slot_surface
                    .contains("drag-target-slice-index <=> root.live-drag-target-slice-index;")
                && pane_slot_surface.contains("status: root.pane.status;")
                && pane_slot_surface.contains("selected-count: root.pane.selected_count;")
                && pane_slot_surface.contains("selected-status: root.pane.selected_status;")
                && pane_slot_surface
                    .contains("external-edit-active: root.pane.external_edit_active;")
                && pane_slot_surface
                    .contains("external-edit-status: root.pane.external_edit_status;")
                && pane_slot_surface
                    .contains("undo-available: root.focused && root.pane.undo_available;")
                && pane_slot_surface
                    .contains("chooser-mode: root.focused && root.pane.chooser_mode;")
                && pane_slot_surface.contains(
                    "selected-path: root.focused ? root.pane.focused_selected_path : \"\";"
                ),
            "PaneSlotSurface should own the reusable pane data-to-FilePane binding surface"
        );
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
            app.contains("for pane[index] in root.pane_slots : PaneSlotSurface {\n                        private property <int> slot: pane.slot;\n                        x: root.pane-slot-x(slot);\n                        width: root.pane-slot-width(slot);\n                        height: parent.height;")
                && app.contains("pane: pane;")
                && app.contains("view: root.pane_views[index];")
                && app.contains("entries: slot == 0 ? root.pane_slot_0_entries : root.pane_slot_1_entries;")
                && app.contains("highlights: slot == 0 ? root.pane_slot_0_highlights : root.pane_slot_1_highlights;")
                && app.contains("focused: root.focused_pane == slot;"),
            "split view should render every physical pane through one slot-driven PaneSlotSurface template"
        );
        assert!(
            app.contains("private property <length> split-divider-width: root.split_view_open ? 1px : 0px;")
                && app.contains("private property <length> pane-slot-1-x: root.pane-slot-0-width + root.split-divider-width;")
                && app.contains("private property <length> pane-slot-1-width: root.split_view_open ? max(1px, root.main-pane-width - root.pane-slot-1-x) : 0px;")
                && app.contains("for pane in root.pane_slots : Rectangle {\n                        private property <int> slot: pane.slot;\n                        visible: root.split_view_open && slot > 0;\n                        x: root.pane-slot-x(slot) - root.split-divider-width;")
                && app.contains("background: root.split-resize-active && slot == 1 ?"),
            "split dividers should be generated from pane slot boundaries instead of a hand-coded side pair"
        );
        assert!(
            app.contains("function pane-slot-x(slot: int) -> length {")
                && app.contains("return floor((root.main-pane-width * slot / root.visible-pane-count) / 1px) * 1px;")
                && app.contains("function pane-slot-width(slot: int) -> length {")
                && app.contains("return max(1px, root.pane-slot-x(slot + 1) - root.pane-slot-x(slot));"),
            "pane slot geometry should be resolved by shared slot/count geometry functions"
        );
        let pane_shells = app
            .split_once("pane-shells := Rectangle {")
            .expect("split panes should live inside one explicit shell row")
            .1
            .split_once("DragOverlayLayer {")
            .expect("split pane shell row should be before overlay layers")
            .0;
        assert_eq!(
            pane_shells.matches("PaneSlotSurface {").count(),
            1,
            "split view must render physical panes through one reusable PaneSlotSurface repeater"
        );
        assert!(pane_shells.contains("for pane[index] in root.pane_slots : PaneSlotSurface"));
        assert_eq!(
            pane_shells.matches("PaneSlot {").count(),
            0,
            "physical pane instances should not bypass PaneSlotSurface"
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
                && !pane_shells.contains("right-pane-content := Rectangle")
                && !pane_shells.contains("pane-slot-0-shell")
                && !pane_shells.contains("pane-slot-1-shell"),
            "pane shells should not hand-roll pane chrome or content outside FilePane"
        );
        assert!(
            route_functions.contains("app-focus.focus();")
                && !app.contains("main-focus := FocusScope")
                && route_functions.contains("root.pane_focus(slot);")
                && route_functions
                    .contains("public function route-pane-path-submitted(slot: int, path: string)")
                && route_functions.contains("root.pane_path_submitted(slot, path);")
                && route_functions.contains("public function route-pane-go-back(slot: int)")
                && route_functions.contains("root.pane_go_back(slot);")
                && route_functions.contains("public function route-pane-go-forward(slot: int)")
                && route_functions.contains("root.pane_go_forward(slot);"),
            "shared pane route functions should return keyboard focus to the global shortcut scope and navigate any pane from the same slot-aware code"
        );
        assert!(
            route_functions.contains("public function route-pane-view-changed(slot: int)")
                && route_functions.contains("root.pane_view_changed(slot);")
                && route_functions.contains(
                    "public function route-pane-item-view-item-pressed(slot: int, x: float, y: float, toggle: bool, range: bool) -> bool"
                )
                && route_functions
                    .contains("return root.pane_item_view_item_pressed(slot, x, y, toggle, range);")
                && route_functions.contains(
                    "public function route-pane-item-view-item-activated(slot: int, x: float, y: float)"
                )
                && route_functions.contains("root.pane_item_view_item_activated(slot, x, y);")
                && route_functions.contains(
                    "public function route-pane-item-view-item-context-menu(slot: int, x: float, y: float, abs-x: length, abs-y: length) -> bool"
                )
                && route_functions.contains(
                    "return root.pane_item_view_item_context_menu(slot, x, y, abs-x, abs-y);"
                )
                && route_functions.contains(
                    "public function route-pane-item-view-blank-pressed(slot: int,"
                )
                && route_functions.contains(
                    "root.pane_item_view_blank_pressed(slot, x, y, rows-per-column, cell-width, column-width, column-offset, row-height, padding, toggle);"
                )
                && route_functions.contains(
                    "public function route-pane-item-view-blank-moved(slot: int, x: float, y: float) -> bool"
                )
                && route_functions.contains(
                    "return root.pane_item_view_blank_moved(slot, x, y);"
                )
                && !route_functions.contains("route-pane-select-rect")
                && !route_functions.contains("root.pane_select_rect")
                && !route_functions.contains("route-pane-activated")
                && !route_functions.contains("route-pane-request-select")
                && !route_functions.contains("root.pane_request_select"),
            "shared pane route functions should dispatch activation, item-view input, selection, and view state by slot"
        );
        assert!(
            route_functions.contains("public function route-pane-request-context-menu(slot: int,")
                && context_menu_route.contains("root.sync_clipboard_state();")
                && !context_menu_route.contains("root.refresh_clipboard_availability();")
                && !context_menu_route.contains("root.pane_is_selected")
                && !context_menu_route.contains("root.pane_request_select")
                && context_menu_route.contains("root.show-context-menu(1, x, y);")
                && route_functions
                    .contains("public function route-pane-request-blank-context-menu(slot: int,")
                && blank_context_menu_route.contains("root.sync_clipboard_state();")
                && !blank_context_menu_route.contains("root.refresh_clipboard_availability();")
                && route_functions
                    .matches("root.sync_clipboard_state();")
                    .count()
                    == 2
                && blank_context_menu_route.contains("root.show-context-menu(3, x, y);")
                && route_functions
                    .contains("public function route-pane-drop-target-path(slot: int,")
                && route_functions
                    .contains("return root.pane_drop_target_path(slot, x, y, source);")
                && route_functions
                    .contains("public function route-pane-drop-target-slice-index(slot: int,")
                && route_functions
                    .contains("return root.pane_drop_target_slice_index(slot, x, y, source);")
                && route_functions.contains("public function route-pane-drop-allowed(slot: int,")
                && route_functions.contains("return root.pane_drop_allowed(slot, x, y, source);")
                && route_functions
                    .contains("public function route-pane-prepare-transfer(slot: int,")
                && route_functions
                    .contains("return root.pane_prepare_transfer(slot, source, x, y);")
                && route_functions
                    .contains("public function route-pane-transfer-menu-requested(slot: int)"),
            "shared pane route functions should dispatch context menus and drag/drop by slot"
        );
        assert!(
            route_functions.contains("public function route-pane-save-focus-changed(slot: int, focused: bool)")
                && route_functions
                    .contains("public function route-pane-chooser-filter-requested(slot: int, x: length, y: length)")
                && route_functions.contains(
                    "public function route-pane-chooser-choice-requested(slot: int, index: int, x: length, y: length)"
                ),
            "shared pane route functions should dispatch status bar and chooser controls by slot"
        );
        assert!(
            !app.contains("function pane-slot-path-text(slot: int) -> string")
                && !app.contains("function pane-slot-path-focused(slot: int) -> bool")
                && !app.contains("function pane-slot-status(slot: int) -> string")
                && !app.contains("function pane-slot-selected-count(slot: int) -> int")
                && !app.contains("function pane-slot-external-edit-active(slot: int) -> bool"),
            "pane-local address, status, selection, and external edit state should come from PaneSlotData instead of slot selectors"
        );
        assert!(
            app.contains("callback pane_path_text_changed(int, string);")
                && app.contains("callback pane_path_focus_changed(int, bool);")
                && app.contains("callback pane_viewport_changed(int, float);")
                && app.contains("root.pane_path_text_changed(slot, text);")
                && app.contains("root.pane_path_focus_changed(slot, focused);")
                && app.contains("root.pane_viewport_changed(slot, viewport-x);"),
            "pane slot callbacks should route address focus/text and viewport changes through pane-local callbacks"
        );
        assert!(
            !app.contains("pane-path-text <=> root.left_pane_path_input_text;")
                && !app.contains("pane-path-text <=> root.inactive_pane_path_input_text;")
                && !app.contains("pane-viewport-x <=> root.viewport_x;")
                && !app.contains("pane-viewport-x <=> root.inactive_pane_viewport_x;"),
            "physical pane template must not bind directly to a fixed pane state slot"
        );
        assert!(app.contains("callback pane_prepare_transfer(int, string, float, float) -> bool;"));
        assert!(
            app.contains("callback pane_drop_target_path(int, float, float, string) -> string;")
        );
        assert!(
            app.contains(
                "callback pane_drop_target_slice_index(int, float, float, string) -> int;"
            )
        );
        assert!(app.contains("callback pane_drop_allowed(int, float, float, string) -> bool;"));
        assert!(app.contains("root.pane_prepare_transfer(slot, source, x, y)"));
        assert!(app.contains("root.pane_drop_target_path(slot, x, y, source)"));
        assert!(app.contains("root.pane_drop_target_slice_index(slot, x, y, source)"));
        assert!(app.contains("root.pane_drop_allowed(slot, x, y, source)"));
        assert!(!app.contains("inactive-pane-drag-active"));
        assert!(!app.contains("main_drag_active"));
        assert!(!app.contains("function pane-slot-show-location(slot: int) -> bool"));
        assert!(split_pane.contains("export component SplitPaneView"));
        assert!(!split_pane.contains("FolderGlyph"));
        assert!(!split_pane.contains("file_tile.slint"));
        assert!(!split_pane.contains("FileTile"));
        assert!(!split_pane.contains("import { ScrollView }"));
        assert!(!split_pane.contains("SplitPreviewTile"));
        assert!(split_pane.contains("callback view_changed();"));
        assert!(split_pane.contains("callback focus_requested();"));
        assert!(split_pane.contains(
            "function set-viewport-x(raw: float) {\n        root.pan-target-viewport-x = root.entry-count == 0"
        ));
        assert!(split_pane.contains(
            "function pan-horizontal(delta: length) {\n        root.set-viewport-x(root.viewport-x + delta / 1px);"
        ));
        assert!(!split_pane.contains("root.viewport-offset = -root.viewport-x * 1px;"));
        assert!(
            !split_pane.contains(
                "root.viewport-offset = -root.viewport-x * 1px;\n            root.view_changed();\n        }\n        root.focus_requested();"
            ),
            "ordinary pane scrolling should not request focus after every viewport change"
        );
        assert!(!split_pane.contains("Click to focus it."));
        assert!(split_pane.contains("callback item_pressed(float, float, bool, bool) -> bool;"));
        assert!(split_pane.contains("callback item_activated(float, float);"));
        assert!(
            split_pane
                .contains("callback item_context_menu(float, float, length, length) -> bool;")
        );
        assert!(
            split_pane.contains("pure callback make_drag_data_at(float, float) -> data-transfer;")
        );
        assert!(!split_pane.contains("callback activated(string);"));
        assert!(!split_pane.contains("callback request_select"));
        assert!(!split_pane.contains("callback request_context_menu"));
        assert!(split_pane.contains("callback zoom_in();"));
        assert!(split_pane.contains("callback zoom_out();"));
        assert!(
            split_pane.contains("in property <[ItemViewHighlightEntry]> highlights;")
                && split_pane.contains("for highlight[index] in root.highlights: Rectangle")
                && split_pane.contains("background: root.selected-background-color;")
                && !split_pane.contains("pure callback is_selected")
                && !split_pane.contains("root.is_selected(item.path)")
                && !split_pane
                    .contains("item.selected ? root.selected-background-color : transparent"),
            "SplitPaneView should draw selection from a sparse pane-local highlight model instead of per-item selected backgrounds"
        );
        assert!(split_pane.contains("function handle-scroll("));
        assert!(
            split_pane
                .contains("function scroll-pan-delta(delta-x: length, delta-y: length) -> length")
                && split_pane
                    .contains("root.pan-horizontal(root.scroll-pan-delta(delta-x, delta-y));")
                && !split_pane.contains("delta-y + delta-x"),
            "pane scrolling should use the dominant wheel axis instead of adding touchpad cross-axis jitter"
        );
        let widgets = include_str!("../../ui/widgets.slint");
        let models = include_str!("../../ui/models.slint");
        let item_view_entry = models
            .split_once("export struct ItemViewEntry")
            .and_then(|(_, rest)| rest.split_once("export struct PlaceEntry"))
            .map(|(body, _)| body)
            .expect("models.slint should define ItemViewEntry before PlaceEntry");
        let highlight_loop = split_pane
            .split_once("for highlight[index] in root.highlights: Rectangle")
            .and_then(|(_, rest)| {
                rest.split_once(
                    "if (root.drag-active && !root.drag-rejected && root.drag-target-slice-index >= 0): Rectangle",
                )
            })
            .map(|(loop_body, _)| loop_body)
            .expect("SplitPaneView should have a sparse selection highlight overlay");
        let drop_target_loop = split_pane
            .split_once(
                "if (root.drag-active && !root.drag-rejected && root.drag-target-slice-index >= 0): Rectangle",
            )
            .and_then(|(_, rest)| rest.split_once("for item[index] in root.entries: Image"))
            .map(|(loop_body, _)| loop_body)
            .expect("SplitPaneView should have one concrete drop-target overlay");
        let base_image_loop = split_pane
            .split_once("for item[index] in root.entries: Image")
            .and_then(|(_, rest)| rest.split_once("for item[index] in root.entries: Text"))
            .map(|(loop_body, _)| loop_body)
            .expect("SplitPaneView should have an unconditional base image primitive loop");
        let base_text_loop = split_pane
            .split_once("for item[index] in root.entries: Text")
            .and_then(|(_, rest)| rest.split_once("if (root.show-location): Rectangle"))
            .map(|(loop_body, _)| loop_body)
            .expect("SplitPaneView should have an unconditional base text primitive loop");
        let metadata_tile_loop = split_pane
            .split_once("if (root.show-location): Rectangle")
            .and_then(|(_, rest)| rest.split_once("if (root.selection-rect-active): Rectangle"))
            .map(|(loop_body, _)| loop_body)
            .expect("SplitPaneView should have a metadata overlay loop");
        assert!(
            split_pane.contains("for item[index] in root.entries: Image")
                && split_pane.contains("for item[index] in root.entries: Text")
                && base_image_loop.contains("tile-row: index.mod(root.rows-per-column);")
                && base_image_loop
                    .contains("tile-column: (index - self.tile-row) / root.rows-per-column;")
                && base_text_loop.contains("tile-row: index.mod(root.rows-per-column);")
                && base_text_loop
                    .contains("tile-column: (index - self.tile-row) / root.rows-per-column;")
                && split_pane.contains(
                    "x: root.preview-padding + root.column-offset + root.virtual-start-column * root.column-width - root.viewport-x * 1px;"
                )
                && base_image_loop
                    .contains("x: self.tile-column * root.column-width + item.media_x * 1px;")
                && base_image_loop.contains(
                    "y: root.preview-padding + self.tile-row * root.row-height + item.media_y * 1px;"
                )
                && base_image_loop.contains("width: item.media_width * 1px;")
                && base_image_loop.contains("height: item.media_height * 1px;")
                && base_image_loop.contains("source: item.media;")
                && base_text_loop
                    .contains("x: self.tile-column * root.column-width + item.text_x * 1px;")
                && base_text_loop.contains(
                    "y: root.preview-padding + self.tile-row * root.row-height + item.title_y * 1px;"
                )
                && base_text_loop.contains("width: item.text_width * 1px;")
                && base_text_loop.contains("height: item.title_line_height * 1px;")
                && base_text_loop.contains("text: item.name;")
                && !base_image_loop.contains("metadata_line_height")
                && !base_text_loop.contains("metadata_line_height")
                && !base_text_loop.contains("metadata-group-color")
                && !base_text_loop.contains("metadata-location-color")
                && !base_text_loop.contains("item.group")
                && !base_text_loop.contains("item.location")
                && !split_pane.contains("item.thumbnail")
                && !base_image_loop.contains("thumbnail_state")
                && !base_text_loop.contains("thumbnail_state")
                && !widgets.contains("export component FolderGlyph")
                && !split_pane.contains("entry: item;")
                && !split_pane.contains("selected: item.selected;")
                && !split_pane.contains("drag-data-source:")
                && !models.contains("export struct FileEntry")
                && item_view_entry.contains("selected: bool")
                && item_view_entry.contains("thumbnail_state: int")
                && item_view_entry.contains("media: image")
                && item_view_entry.contains("media_token: int")
                && item_view_entry.contains("tile_width: float")
                && item_view_entry.contains("media_x: float")
                && item_view_entry.contains("media_width: float")
                && item_view_entry.contains("text_x: float")
                && item_view_entry.contains("title_line_height: float")
                && !item_view_entry.contains("thumbnail: image")
                && !item_view_entry.contains("glyph_doc_font_size")
                && !item_view_entry.contains("tile_x")
                && !item_view_entry.contains("tile_y")
                && !split_pane.contains("viewport-y"),
            "SplitPaneView should inline Dolphin-style horizontal column-first tile primitives without a FileTile or FolderGlyph component boundary, and ItemViewEntry should not carry reusable local tile coordinates"
        );
        assert!(
            highlight_loop.contains("tile-row: highlight.slice_index.mod(root.rows-per-column);")
                && highlight_loop.contains(
                    "tile-column: (highlight.slice_index - self.tile-row) / root.rows-per-column;"
                )
                && highlight_loop.contains("x: self.tile-column * root.column-width;")
                && highlight_loop.contains("y: root.preview-padding + self.tile-row * root.row-height;")
                && highlight_loop.contains("width: highlight.tile_width * 1px;")
                && highlight_loop.contains("height: highlight.tile_height * 1px;")
                && drop_target_loop.contains(
                    "tile-row: root.drag-target-slice-index.mod(root.rows-per-column);"
                )
                && drop_target_loop.contains(
                    "tile-column: (root.drag-target-slice-index - self.tile-row) / root.rows-per-column;"
                )
                && drop_target_loop.contains("width: root.cell-width;")
                && drop_target_loop.contains("height: root.row-height;")
                && !split_pane.contains(
                    "root.drag-active && !root.drag-rejected && root.drag-target-path == item.path"
                ),
            "selection and drop feedback should use sparse slice-index overlays with the same horizontal column-first coordinates"
        );
        assert!(
            !split_pane.contains("private property <length> tile-height:")
                && !split_pane.contains("private property <length> thumbnail-width:")
                && !split_pane.contains("private property <length> title-font-size:")
                && !split_pane.contains("tile-height: root.tile-height;")
                && !split_pane.contains("zoom-level: root.zoom-level;")
                && split_pane.contains("color: root.metadata-group-color;")
                && split_pane.contains("height: item.tile_height * 1px;")
                && base_image_loop.contains("width: item.media_width * 1px;")
                && base_image_loop.contains("height: item.media_height * 1px;")
                && base_text_loop.contains("font-size: item.title_font_size * 1px;")
                && base_text_loop.contains("item.text_x * 1px")
                && base_text_loop.contains("item.title_y * 1px")
                && base_text_loop.contains("width: item.text_width * 1px;")
                && base_text_loop.contains("height: item.title_line_height * 1px;")
                && base_text_loop.contains("text: item.name;")
                && base_text_loop.contains("horizontal-alignment: left;")
                && !split_pane.contains("parent.height - max(16px, item.title_line_height")
                && !split_pane.contains("parent.width - 12px")
                && split_pane.contains("height: item.metadata_line_height * 1px;")
                && !split_pane.contains("item.thumbnail_width")
                && !split_pane.contains("doc-font-size:")
                && !split_pane.contains("item.tile_padding_x")
                && !split_pane.contains("item.tile_spacing")
                && !base_image_loop.contains("HorizontalLayout")
                && !base_image_loop.contains("VerticalLayout")
                && !base_text_loop.contains("HorizontalLayout")
                && !base_text_loop.contains("VerticalLayout")
                && !split_pane.contains("height: root.zoom-level ==")
                && !split_pane.contains("font-size: root.zoom-level =="),
            "visible tile primitives should consume media and render tokens projected by Rust item-view, with only pane-metric guards for invalid zero title geometry"
        );
        assert!(
            split_pane.contains(
                "function pan-horizontal(delta: length) {\n        root.set-viewport-x(root.viewport-x + delta / 1px);"
            ),
            "ordinary pane scrolling should update the viewport before requesting focus"
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
        assert!(split_pane.contains("for item[index] in root.entries: Image"));
        assert!(split_pane.contains("for item[index] in root.entries: Text"));
        assert!(
            split_pane.contains("item-drag-area := DragArea")
                && split_pane.contains("data: root.make_drag_data_at(")
                && split_pane.contains("input-touch := TouchArea")
                && split_pane.contains(
                    "root.item_pressed(root.item-pointer-abs-x / 1px, root.item-pointer-abs-y / 1px, event.modifiers.control, event.modifiers.shift)"
                )
                && split_pane.contains(
                    "root.item_context_menu(root.item-pointer-abs-x / 1px, root.item-pointer-abs-y / 1px, root.item-pointer-abs-x, root.item-pointer-abs-y)"
                )
                && split_pane.contains(
                    "root.item_activated(root.item-pointer-abs-x / 1px, root.item-pointer-abs-y / 1px);"
                )
                && split_pane.contains("root.begin-blank-press(")
                && !split_pane.contains("drag-data-source:")
                && !split_pane.contains("activated(path) =>")
                && !split_pane.contains("request_select(path")
                && !split_pane.contains("request_context_menu(path"),
            "SplitPaneView should use one pane-level input controller with Rust coordinate hit-test instead of per-tile handlers"
        );
        assert!(
            split_pane.contains("if (root.show-location): Rectangle")
                && metadata_tile_loop.contains("if (item.group != \"\"): Text")
                && metadata_tile_loop.contains("text: item.group;")
                && metadata_tile_loop.contains("if (item.location != \"\"): Text")
                && metadata_tile_loop.contains("text: item.location;")
                && metadata_tile_loop.contains("height: item.metadata_line_height * 1px;")
                && metadata_tile_loop.contains("color: root.metadata-group-color;")
                && metadata_tile_loop.contains("color: root.metadata-location-color;")
                && !metadata_tile_loop.contains("source: item.media;")
                && !metadata_tile_loop.contains("text: item.name;"),
            "ordinary compact items should always render icon/name in the base loop, while group/location metadata is an overlay used only for location mode"
        );
        assert!(!split_pane.contains("root.request_context_menu("));
        assert!(
            split_pane.contains("slice-layer := Rectangle")
                && split_pane.contains(
                    "x: root.preview-padding + root.column-offset + root.virtual-start-column * root.column-width - root.viewport-x * 1px;"
                )
                && split_pane.contains("private property <bool> scrollbar-visible:")
                && split_pane.contains("scrollbar-track := Rectangle")
                && split_pane.contains("root.set-viewport-x(root.viewport-x-from-scrollbar-thumb")
                && !split_pane.contains("viewport-x <=> root.viewport-offset;")
                && !split_pane.contains("viewport-sync-epsilon")
                && !split_pane.contains("changed viewport-x =>"),
            "SplitPaneView should use a self-managed viewport instead of ScrollView/Flickable viewport writeback"
        );
        assert!(
            split_pane.contains("function relayout-visible-slice()")
                && split_pane.contains("changed width => {\n        root.relayout-visible-slice();\n    }")
                && split_pane.contains(
                    "changed rows-per-column => {\n        root.relayout-visible-slice();\n    }"
                )
                && split_pane.contains(
                    "root.pan-target-viewport-x = root.entry-count == 0 ? 0 : root.stable-viewport-x(root.viewport-x);"
                ),
            "pane-local geometry changes should clamp the viewport and request a virtual slice refresh without waiting for scrollbar input"
        );
        assert!(!split_pane.contains("virtual-layer := Rectangle"));
        assert!(!split_pane.contains("private property <length> virtual-layer-width"));
        assert!(split_pane.contains(
            "private property <int> rows-per-column: max(1, root.item-view-rows-per-column);"
        ));
        assert!(split_pane.contains(
            "private property <length> virtual-slice-width: max(1px, root.item-view-virtual-slice-width * 1px);"
        ));
        assert!(
            split_pane.contains("in property <int> item-view-rows-per-column: 1;")
                && split_pane.contains(
                    "private property <length> cell-width: max(1, root.item-view-cell-width) * 1px;"
                )
                && split_pane.contains(
                    "private property <length> row-height: max(1, root.item-view-row-height) * 1px;"
                )
                && split_pane.contains(
                    "private property <length> viewport-content-width: max(1px, root.item-view-content-width * 1px);"
                )
                && split_pane.contains(
                    "private property <float> scroll-max-x: max(0, root.item-view-scroll-max-x);"
                )
                && !split_pane.contains("root.zoom-level ==")
                && !split_pane.contains("ceil(root.entry-count / root.rows-per-column)")
                && !split_pane.contains("ceil(root.entries.length / root.rows-per-column)")
                && !split_pane.contains("root.height - 2 * root.preview-padding"),
            "SplitPaneView should consume Rust-projected item-view layout metrics instead of recalculating the layouter in Slint"
        );
        assert!(
            split_pane.contains("slice-layer := Rectangle")
                && split_pane.contains(
                    "x: root.preview-padding + root.column-offset + root.virtual-start-column * root.column-width - root.viewport-x * 1px;"
                )
                && split_pane.contains("width: root.virtual-slice-width;")
                && split_pane.contains(
                    "private property <int> tile-row: index.mod(root.rows-per-column);"
                )
                && split_pane.contains(
                    "private property <int> tile-column: (index - self.tile-row) / root.rows-per-column;"
                )
                && split_pane.contains(
                    "x: self.tile-column * root.column-width;"
                )
                && split_pane.contains("y: root.preview-padding + self.tile-row * root.row-height;")
                && split_pane.contains("width: item.tile_width * 1px;")
                && !split_pane.contains("item.tile_x")
                && !split_pane.contains("item.tile_y")
                && !split_pane.contains("property <int> global-index:"),
            "virtualized pane slices should be positioned by the self-managed viewport while local tile coordinates come from the reusable loop index instead of ItemViewEntry row data"
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
        assert!(!split_pane.contains("header-height"));
        assert!(!split_pane.contains("Split Pane"));
        assert!(!split_pane.contains("text: root.path"));
        assert!(!split_pane.contains("x: 1px;"));
        assert!(!split_pane.contains("parent.width - 1px"));
        assert!(!split_pane.contains("root.width - 1px"));
        assert!(
            app.contains("pane-row := Rectangle")
                && app.contains("height: parent.height;")
                && app.contains("for pane[index] in root.pane_slots : PaneSlotSurface")
                && app.contains("private property <int> slot: pane.slot;")
                && app.contains("x: root.pane-slot-x(slot);")
                && app.contains("width: root.pane-slot-width(slot);")
                && app.contains("height: parent.height;"),
            "split panes should keep their chrome inside each clipped physical pane"
        );
        assert!(
            app.matches("clip: true;").count() >= 2,
            "pane slot surfaces must be clipped at their pane boundaries"
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
            app.contains(
                "changed split_pane_ratio => {\n        root.pane_layout_changed();\n    }"
            ),
            "dragging the divider should resync every visible pane virtual view as the ratio changes"
        );

        let divider = app
            .split_once("for pane in root.pane_slots : Rectangle {")
            .expect("split divider should exist")
            .1
            .split_once("if (root.split_view_open) : split-divider-touch := TouchArea")
            .expect("split divider should be before the divider touch area")
            .0;
        assert!(
            divider.contains("private property <int> slot: pane.slot;")
                && divider.contains("visible: root.split_view_open && slot > 0;")
                && divider.contains("x: root.pane-slot-x(slot) - root.split-divider-width;")
                && divider.contains("width: root.split-divider-width;")
                && divider.contains("background: root.split-resize-active && slot == 1 ?"),
            "visible dividers should come from slot boundaries and expose drag feedback"
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
                .contains("x: max(0px, root.pane-slot-0-width - root.split-resize-hit-width / 2);")
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
            service_action_count: 0,
            service_submenu_count: 0,
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
    fn compact_item_view_layout_keeps_visible_columns_with_overscan() {
        let compact_layout = compact_item_view_layout(250.0, 100, 4, 100.0, 100.0, 10.0);
        let at_start = compact_layout.virtual_plan(0.0, 1);
        assert_eq!(at_start.range, 0..16);
        assert_eq!(at_start.visible_range, 0..12);

        let middle = compact_layout.virtual_plan(350.0, 1);
        assert_eq!(middle.range, 8..28);
        assert_eq!(middle.visible_range, 12..24);

        let clamped =
            compact_item_view_layout(250.0, 10, 4, 100.0, 100.0, 10.0).virtual_plan(800.0, 1);
        assert_eq!(clamped.range, 0..10);
        assert_eq!(clamped.visible_range, 0..10);
    }

    #[test]
    fn compact_item_view_metrics_follow_dolphin_compact_formula() {
        assert_eq!(compact_cell_width(0), 96.0);
        assert_eq!(compact_cell_width(2), 129.0);
        assert_eq!(compact_row_height(2, 1), 50.0);
        assert_eq!(compact_row_height(2, 3), 57.0);
    }

    #[test]
    fn compact_item_view_layout_reports_scroll_extent_from_column_content_width() {
        assert_eq!(
            compact_item_view_layout(300.0, 0, 4, 100.0, 100.0, 10.0).scroll_max_x,
            0.0
        );
        assert_eq!(
            compact_item_view_layout(300.0, 8, 4, 100.0, 100.0, 10.0).scroll_max_x,
            0.0
        );
        assert_eq!(
            compact_item_view_layout(300.0, 12, 4, 100.0, 100.0, 10.0).scroll_max_x,
            36.0
        );
        assert_eq!(
            compact_item_view_layout(300.0, 13, 4, 100.0, 100.0, 10.0).scroll_max_x,
            144.0
        );
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
            service_action_count: 0,
            service_submenu_count: 0,
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
    fn context_menu_metrics_include_service_menu_rows() {
        let mut single_file = menu_metrics_input(1);
        single_file.selected_count = 1;
        single_file.default_open_visible = true;
        single_file.service_action_count = 2;
        single_file.service_submenu_count = 1;
        let file_metrics = context_menu_metrics(single_file);
        assert_eq!(
            file_metrics.height,
            12.0 * MENU_ITEM_HEIGHT + 2.0 * MENU_SEPARATOR_HEIGHT
        );
        assert_eq!(file_metrics.open_with_row_y_offset, MENU_ITEM_HEIGHT);

        let mut multi_file = menu_metrics_input(1);
        multi_file.selected_count = 3;
        multi_file.service_action_count = 2;
        multi_file.service_submenu_count = 1;
        let multi_metrics = context_menu_metrics(multi_file);
        assert_eq!(
            multi_metrics.height,
            MENU_TITLE_HEIGHT + 6.0 * MENU_ITEM_HEIGHT + MENU_SEPARATOR_HEIGHT
        );

        let mut viewport = menu_metrics_input(3);
        viewport.service_action_count = 2;
        viewport.service_submenu_count = 1;
        let viewport_metrics = context_menu_metrics(viewport);
        assert_eq!(
            viewport_metrics.height,
            8.0 * MENU_ITEM_HEIGHT + MENU_SEPARATOR_HEIGHT
        );
        assert_eq!(
            viewport_metrics.open_with_row_y_offset,
            3.0 * MENU_ITEM_HEIGHT + MENU_SEPARATOR_HEIGHT
        );
    }

    #[test]
    fn compact_item_view_layout_clamps_viewport_and_reports_anchor_column() {
        let compact_layout = compact_item_view_layout(250.0, 100, 4, 100.0, 100.0, 10.0);
        let plan = compact_layout.virtual_plan(350.0, 2);
        assert_eq!(plan.viewport_x, 350.0);
        assert_eq!(plan.scroll_max_x, 2462.0);
        assert_eq!(plan.visible_range, 12..24);
        assert_eq!(plan.range, 4..32);
        assert_eq!(plan.start_column, 1);

        let clamped =
            compact_item_view_layout(250.0, 10, 4, 100.0, 100.0, 10.0).virtual_plan(800.0, 2);
        assert_eq!(clamped.viewport_x, 86.0);
        assert_eq!(clamped.scroll_max_x, 86.0);
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
