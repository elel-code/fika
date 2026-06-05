use crate::app::geometry::{
    MainItemViewLayout, PATH_BAR_HEIGHT, STATUS_BAR_HEIGHT, active_main_pane_width,
    inactive_main_pane_width, main_pane_bounds, search_panel_height,
};
use crate::app::selection::{filtered_entry_at_for_slot, filtered_entry_count_for_slot};
use crate::app::state::AppState;
use crate::{AppWindow, FileEntry, ItemViewEntry};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString};
use std::ops::Range;

const COMPACT_ITEM_PADDING: f32 = 2.0;
const COMPACT_MEDIA_TEXT_GAP: f32 = COMPACT_ITEM_PADDING * 2.0;
const SELECTION_DRAG_THRESHOLD: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewRenderMetrics {
    pub(crate) tile_height: f32,
    pub(crate) media_padding_x: f32,
    pub(crate) media_text_gap: f32,
    pub(crate) media_width: f32,
    pub(crate) media_height: f32,
    pub(crate) metadata_font_size: f32,
    pub(crate) title_font_size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewRenderPlanInput {
    pub(crate) cell_width: f32,
    pub(crate) render_metrics: ItemViewRenderMetrics,
    pub(crate) show_location: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemViewRowToken {
    name: SharedString,
    path: SharedString,
    group: SharedString,
    location: SharedString,
    is_dir: bool,
    selected: bool,
    thumbnail_state: i32,
    media_token: i32,
    tile_width: f32,
    tile_height: f32,
    media_x: f32,
    media_y: f32,
    text_x: f32,
    text_width: f32,
    group_y: f32,
    title_y: f32,
    location_y: f32,
    metadata_line_height: f32,
    title_line_height: f32,
    media_width: f32,
    media_height: f32,
    metadata_font_size: f32,
    title_font_size: f32,
}

impl ItemViewRowToken {
    pub(crate) fn from_entry(entry: &ItemViewEntry) -> Self {
        Self {
            name: entry.name.clone(),
            path: entry.path.clone(),
            group: entry.group.clone(),
            location: entry.location.clone(),
            is_dir: entry.is_dir,
            selected: entry.selected,
            thumbnail_state: entry.thumbnail_state,
            media_token: entry.media_token,
            tile_width: entry.tile_width,
            tile_height: entry.tile_height,
            media_x: entry.media_x,
            media_y: entry.media_y,
            text_x: entry.text_x,
            text_width: entry.text_width,
            group_y: entry.group_y,
            title_y: entry.title_y,
            location_y: entry.location_y,
            metadata_line_height: entry.metadata_line_height,
            title_line_height: entry.title_line_height,
            media_width: entry.media_width,
            media_height: entry.media_height,
            metadata_font_size: entry.metadata_font_size,
            title_font_size: entry.title_font_size,
        }
    }

    pub(crate) fn path(&self) -> &str {
        self.path.as_str()
    }

    pub(crate) fn selected(&self) -> bool {
        self.selected
    }

    pub(crate) fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub(crate) fn has_renderable_title(&self) -> bool {
        !self.name.is_empty()
            && self.tile_width > 1.0
            && self.tile_height > 1.0
            && self.text_x >= 0.0
            && self.text_width > 1.0
            && self.title_line_height > 1.0
            && self.title_font_size > 1.0
    }
}

impl ItemViewRenderMetrics {
    pub(crate) fn from_zoom_level_with_text_line_count(
        zoom_level: i32,
        text_line_count: usize,
    ) -> Self {
        Self {
            tile_height: compact_tile_height(zoom_level, text_line_count),
            media_padding_x: COMPACT_ITEM_PADDING,
            media_text_gap: COMPACT_MEDIA_TEXT_GAP,
            media_width: compact_media_width(zoom_level),
            media_height: compact_media_height(zoom_level),
            metadata_font_size: if zoom_level < 2 { 10.0 } else { 11.0 },
            title_font_size: compact_title_font_size(zoom_level),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ItemViewMediaCache {
    folder: Image,
    file: Image,
    folder_token: i32,
    file_token: i32,
}

impl ItemViewMediaCache {
    pub(crate) fn new(metrics: ItemViewRenderMetrics, dark: bool) -> Self {
        let width = metrics.media_width.round().max(1.0) as u32;
        let height = metrics.media_height.round().max(1.0) as u32;
        Self {
            folder: fallback_media_image(true, dark, width, height),
            file: fallback_media_image(false, dark, width, height),
            folder_token: fallback_media_token(true, dark, width, height),
            file_token: fallback_media_token(false, dark, width, height),
        }
    }

    fn image_for(&self, is_dir: bool) -> Image {
        if is_dir {
            self.folder.clone()
        } else {
            self.file.clone()
        }
    }

    fn token_for(&self, is_dir: bool) -> i32 {
        if is_dir {
            self.folder_token
        } else {
            self.file_token
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewLayout {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) viewport_x: f32,
    pub(crate) rows_per_column: usize,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionRect {
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
    pub(crate) rows_per_column: i32,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

impl SelectionRect {
    pub(crate) fn candidate_range(self, visible_count: usize) -> Range<usize> {
        if visible_count == 0 {
            return 0..0;
        }

        let rows_per_column = self.rows_per_column.max(1) as usize;
        let cell_width = self.cell_width.max(1.0);
        let column_width = self.column_width.max(1.0);
        let column_offset = self.column_offset.max(0.0);

        let first_column = ((self.x1 - self.padding - column_offset - cell_width) / column_width)
            .floor()
            .max(0.0) as usize;
        let last_column = ((self.x2 - self.padding - column_offset) / column_width)
            .floor()
            .max(0.0) as usize;

        let start = first_column
            .saturating_mul(rows_per_column)
            .min(visible_count);
        let end = ((last_column + 1).saturating_mul(rows_per_column)).min(visible_count);
        start..end.max(start)
    }

    pub(crate) fn intersects_index(self, index: usize) -> bool {
        let rows_per_column = self.rows_per_column.max(1) as usize;
        let column = index / rows_per_column;
        let row = index % rows_per_column;
        let tile_x1 =
            self.padding + self.column_offset.max(0.0) + column as f32 * self.column_width.max(1.0);
        let tile_y1 = self.padding + row as f32 * self.row_height;
        let tile_x2 = tile_x1 + self.cell_width.max(1.0);
        let tile_y2 = tile_y1 + self.row_height.max(1.0);

        RectBounds::new(self.x1, self.y1, self.x2, self.y2)
            .intersects(RectBounds::new(tile_x1, tile_y1, tile_x2, tile_y2))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemViewInputMetrics {
    pub(crate) rows_per_column: i32,
    pub(crate) cell_width: f32,
    pub(crate) column_width: f32,
    pub(crate) column_offset: f32,
    pub(crate) row_height: f32,
    pub(crate) padding: f32,
}

impl ItemViewInputMetrics {
    pub(crate) fn new(
        rows_per_column: i32,
        cell_width: f32,
        column_width: f32,
        column_offset: f32,
        row_height: f32,
        padding: f32,
    ) -> Self {
        Self {
            rows_per_column: rows_per_column.max(1),
            cell_width: cell_width.max(1.0),
            column_width: column_width.max(1.0),
            column_offset: column_offset.max(0.0),
            row_height: row_height.max(1.0),
            padding: padding.max(0.0),
        }
    }

    fn selection_rect(self, gesture: SelectionRectGesture) -> SelectionRect {
        let (x1, x2) = ordered_pair(gesture.start_x, gesture.current_x);
        let (y1, y2) = ordered_pair(gesture.start_y, gesture.current_y);
        SelectionRect {
            x1,
            y1,
            x2,
            y2,
            rows_per_column: self.rows_per_column,
            cell_width: self.cell_width,
            column_width: self.column_width,
            column_offset: self.column_offset,
            row_height: self.row_height,
            padding: self.padding,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ItemViewInputState {
    selection_rect: Option<SelectionRectGesture>,
}

impl ItemViewInputState {
    pub(crate) fn press_blank(
        &mut self,
        x: f32,
        y: f32,
        metrics: ItemViewInputMetrics,
        toggle: bool,
    ) {
        self.selection_rect = Some(SelectionRectGesture {
            start_x: x,
            start_y: y,
            current_x: x,
            current_y: y,
            metrics,
            toggle,
            active: false,
        });
    }

    pub(crate) fn move_blank(&mut self, x: f32, y: f32) -> bool {
        let Some(mut gesture) = self.selection_rect else {
            return false;
        };
        gesture.current_x = x;
        gesture.current_y = y;
        gesture.active |= selection_drag_threshold_crossed(gesture.start_x, gesture.start_y, x, y);
        self.selection_rect = Some(gesture);
        gesture.active
    }

    pub(crate) fn release_blank(&mut self, x: f32, y: f32) -> ItemViewReleaseAction {
        let Some(mut gesture) = self.selection_rect.take() else {
            return ItemViewReleaseAction::None;
        };
        gesture.current_x = x;
        gesture.current_y = y;
        gesture.active |= selection_drag_threshold_crossed(gesture.start_x, gesture.start_y, x, y);
        if gesture.active {
            ItemViewReleaseAction::SelectRect {
                rect: gesture.metrics.selection_rect(gesture),
                toggle: gesture.toggle,
            }
        } else {
            ItemViewReleaseAction::ClearSelection
        }
    }

    pub(crate) fn cancel_blank(&mut self) {
        self.selection_rect = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SelectionRectGesture {
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
    metrics: ItemViewInputMetrics,
    toggle: bool,
    active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ItemViewReleaseAction {
    None,
    ClearSelection,
    SelectRect { rect: SelectionRect, toggle: bool },
}

fn selection_drag_threshold_crossed(
    start_x: f32,
    start_y: f32,
    current_x: f32,
    current_y: f32,
) -> bool {
    (current_x - start_x).abs() > SELECTION_DRAG_THRESHOLD
        || (current_y - start_y).abs() > SELECTION_DRAG_THRESHOLD
}

fn ordered_pair(a: f32, b: f32) -> (f32, f32) {
    if a <= b { (a, b) } else { (b, a) }
}

pub(crate) fn decorate_render_plan(entries: &mut [ItemViewEntry], input: ItemViewRenderPlanInput) {
    let cell_width = input.cell_width.max(1.0);
    let render_metrics = input.render_metrics;
    let tile_width = cell_width;

    for entry in entries.iter_mut() {
        let metadata_mode =
            input.show_location && (!entry.group.is_empty() || !entry.location.is_empty());
        entry.tile_width = tile_width;
        entry.tile_height = render_metrics.tile_height;
        entry.media_width = render_metrics.media_width;
        entry.media_height = render_metrics.media_height;
        entry.metadata_font_size = render_metrics.metadata_font_size;
        entry.title_font_size = render_metrics.title_font_size;
        entry.media_x = render_metrics.media_padding_x;
        entry.media_y = ((render_metrics.tile_height - render_metrics.media_height) / 2.0).max(0.0);
        entry.text_x = render_metrics.media_padding_x
            + render_metrics.media_width
            + render_metrics.media_text_gap;
        entry.text_width = (tile_width - entry.text_x - render_metrics.media_padding_x).max(1.0);

        let text_plan = ItemTextRenderPlan::new(entry, render_metrics, metadata_mode);
        entry.group_y = text_plan.group_y;
        entry.title_y = text_plan.title_y;
        entry.location_y = text_plan.location_y;
        entry.metadata_line_height = text_plan.metadata_line_height;
        entry.title_line_height = text_plan.title_line_height;
    }
}

pub(crate) fn decorate_fallback_media(entries: &mut [ItemViewEntry], cache: &ItemViewMediaCache) {
    for entry in entries.iter_mut() {
        if entry.is_dir || entry.thumbnail_state != 2 {
            entry.media = cache.image_for(entry.is_dir);
            entry.media_token = cache.token_for(entry.is_dir);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ItemTextRenderPlan {
    group_y: f32,
    title_y: f32,
    location_y: f32,
    metadata_line_height: f32,
    title_line_height: f32,
}

impl ItemTextRenderPlan {
    fn new(entry: &ItemViewEntry, metrics: ItemViewRenderMetrics, show_location: bool) -> Self {
        let metadata_line_height = metrics.metadata_font_size + 3.0;
        let title_line_height = title_line_height(metrics.title_font_size);
        let has_group = show_location && !entry.group.is_empty();
        let has_location = show_location && !entry.location.is_empty();
        let spacing = 2.0;
        let mut line_count = 1;
        if has_group {
            line_count += 1;
        }
        if has_location {
            line_count += 1;
        }

        let mut block_height = title_line_height;
        if has_group {
            block_height += metadata_line_height;
        }
        if has_location {
            block_height += metadata_line_height;
        }
        block_height += spacing * (line_count - 1) as f32;

        let mut y = ((metrics.tile_height - block_height) / 2.0).max(0.0);
        let group_y = y;
        if has_group {
            y += metadata_line_height + spacing;
        }
        let title_y = y;
        y += title_line_height + spacing;
        let location_y = y;

        Self {
            group_y,
            title_y,
            location_y,
            metadata_line_height,
            title_line_height,
        }
    }
}

fn compact_tile_height(zoom_level: i32, text_line_count: usize) -> f32 {
    let icon_size = compact_media_width(zoom_level);
    let text_block_height = compact_text_block_height(zoom_level, text_line_count);
    COMPACT_ITEM_PADDING * 2.0 + icon_size.max(text_block_height)
}

fn compact_media_width(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 28.0,
        1 => 36.0,
        2 => 46.0,
        3 => 58.0,
        _ => 72.0,
    }
}

fn compact_media_height(zoom_level: i32) -> f32 {
    compact_media_width(zoom_level)
}

fn compact_title_font_size(zoom_level: i32) -> f32 {
    match zoom_level {
        0 => 12.0,
        1 => 13.0,
        2 => 15.0,
        3 => 16.0,
        _ => 18.0,
    }
}

fn title_line_height(font_size: f32) -> f32 {
    font_size + 6.0
}

fn compact_text_block_height(zoom_level: i32, text_line_count: usize) -> f32 {
    let text_line_count = text_line_count.max(1);
    let title_line_height = title_line_height(compact_title_font_size(zoom_level));
    if text_line_count == 1 {
        return title_line_height;
    }

    let metadata_font_size = if zoom_level < 2 { 10.0 } else { 11.0 };
    let metadata_line_height = metadata_font_size + 3.0;
    let metadata_lines = text_line_count.saturating_sub(1) as f32;
    let spacing = 2.0 * text_line_count.saturating_sub(1) as f32;
    title_line_height + metadata_lines * metadata_line_height + spacing
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RectBounds {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

#[derive(Clone, Copy)]
struct GlyphColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl GlyphColor {
    const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn pixel(self) -> Rgba8Pixel {
        Rgba8Pixel::new(self.r, self.g, self.b, self.a)
    }
}

fn fallback_media_image(is_dir: bool, dark: bool, width: u32, height: u32) -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    buffer
        .make_mut_slice()
        .fill(GlyphColor::rgba(0, 0, 0, 0).pixel());
    if is_dir {
        draw_folder_glyph(&mut buffer, dark);
    } else {
        draw_file_glyph(&mut buffer, dark);
    }
    Image::from_rgba8(buffer)
}

fn fallback_media_token(is_dir: bool, dark: bool, width: u32, height: u32) -> i32 {
    let kind = if is_dir { 1 } else { 2 };
    let theme = if dark { 1 } else { 0 };
    let width = width.min(0xfff);
    let height = height.min(0xfff);
    0x1000_0000 | (kind << 25) | (theme << 24) | ((width as i32) << 12) | height as i32
}

fn draw_folder_glyph(buffer: &mut SharedPixelBuffer<Rgba8Pixel>, dark: bool) {
    let tab = if dark {
        GlyphColor::rgba(59, 102, 139, 255)
    } else {
        GlyphColor::rgba(114, 174, 230, 255)
    };
    let body = if dark {
        GlyphColor::rgba(63, 111, 152, 255)
    } else {
        GlyphColor::rgba(96, 159, 224, 255)
    };
    let highlight = if dark {
        GlyphColor::rgba(169, 184, 196, 255)
    } else {
        GlyphColor::rgba(237, 244, 250, 255)
    };
    draw_rect(buffer, 0.0, 0.14, 0.48, 0.26, tab);
    draw_rect(buffer, 0.0, 0.29, 1.0, 0.69, body);
    draw_rect(buffer, 0.08, 0.37, 0.82, 0.10, highlight);
}

fn draw_file_glyph(buffer: &mut SharedPixelBuffer<Rgba8Pixel>, dark: bool) {
    let body = if dark {
        GlyphColor::rgba(139, 145, 151, 255)
    } else {
        GlyphColor::rgba(174, 180, 186, 255)
    };
    let shade = if dark {
        GlyphColor::rgba(113, 119, 126, 255)
    } else {
        GlyphColor::rgba(151, 158, 165, 255)
    };
    let line = if dark {
        GlyphColor::rgba(48, 48, 48, 255)
    } else {
        GlyphColor::rgba(85, 85, 85, 255)
    };
    draw_rect(buffer, 0.18, 0.10, 0.64, 0.82, body);
    draw_rect(buffer, 0.58, 0.10, 0.24, 0.24, shade);
    draw_rect(buffer, 0.30, 0.52, 0.40, 0.06, line);
    draw_rect(buffer, 0.30, 0.66, 0.32, 0.06, line);
}

fn draw_rect(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: GlyphColor,
) {
    let buffer_width = buffer.width() as usize;
    let buffer_height = buffer.height() as usize;
    if buffer_width == 0 || buffer_height == 0 {
        return;
    }
    let start_x = (x * buffer_width as f32).round().max(0.0) as usize;
    let start_y = (y * buffer_height as f32).round().max(0.0) as usize;
    let end_x = ((x + width) * buffer_width as f32)
        .round()
        .max(start_x as f32) as usize;
    let end_y = ((y + height) * buffer_height as f32)
        .round()
        .max(start_y as f32) as usize;
    let end_x = end_x.min(buffer_width);
    let end_y = end_y.min(buffer_height);
    let pixel = color.pixel();
    let pixels = buffer.make_mut_slice();
    for row in start_y..end_y {
        let row_start = row * buffer_width;
        for col in start_x..end_x {
            pixels[row_start + col] = pixel;
        }
    }
}

impl RectBounds {
    fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn intersects(self, other: Self) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }
}

impl ItemViewLayout {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        viewport_x: f32,
        rows_per_column: usize,
        cell_width: f32,
        column_width: f32,
        column_offset: f32,
        row_height: f32,
        padding: f32,
    ) -> Self {
        Self {
            x,
            y,
            width: width.max(1.0),
            height: height.max(1.0),
            viewport_x: viewport_x.max(0.0),
            rows_per_column: rows_per_column.max(1),
            cell_width: cell_width.max(1.0),
            column_width: column_width.max(1.0),
            column_offset: column_offset.max(0.0),
            row_height: row_height.max(1.0),
            padding: padding.max(0.0),
        }
    }

    pub(crate) fn from_ui(ui: &AppWindow, state: &AppState, slot: i32) -> Option<Self> {
        let pane_state = state.panes.pane_for_slot(slot)?;
        if slot != 0 && !ui.get_split_view_open() {
            return None;
        }

        let window_size = ui.window().size().to_logical(ui.window().scale_factor());
        let pane = main_pane_bounds(
            ui.get_sidebar_width_px(),
            window_size.width,
            window_size.height,
        );
        let main_width = (pane.right - pane.left).max(1.0);
        let (x, width) = pane_slot_geometry(
            pane.left,
            main_width,
            ui.get_split_view_open(),
            ui.get_split_pane_ratio(),
            slot,
        )?;
        let search_height = if state.panes.focused_slot() == slot {
            search_panel_height(
                ui.get_search_bar_open(),
                ui.get_search_query().as_str(),
                ui.get_search_kind_filter(),
                ui.get_search_modified_filter(),
                ui.get_search_size_filter(),
                width,
            )
        } else {
            0.0
        };
        let height =
            (pane.bottom - pane.top - PATH_BAR_HEIGHT - STATUS_BAR_HEIGHT - search_height).max(1.0);
        let layout = MainItemViewLayout::from_ui_for_pane_width_with_text_lines(
            ui,
            width,
            state.panes.focused_slot() == slot,
            pane_state.item_view_text_line_count(),
        );
        let compact_item_view =
            layout.compact_item_view(filtered_entry_count_for_slot(state, slot));

        Some(Self::new(
            x,
            pane.top + PATH_BAR_HEIGHT + search_height,
            width,
            height,
            pane_state.view.viewport_x,
            compact_item_view.rows_per_column,
            compact_item_view.cell_width,
            compact_item_view.column_width,
            compact_item_view.column_offset,
            compact_item_view.row_height,
            compact_item_view.padding,
        ))
    }

    pub(crate) fn index_at_point(self, x: f32, y: f32) -> Option<usize> {
        if x < self.x || x > self.x + self.width || y < self.y || y > self.y + self.height {
            return None;
        }

        let local_x = x - self.x - self.padding - self.column_offset + self.viewport_x;
        let local_y = y - self.y - self.padding;
        if local_x < 0.0 || local_y < 0.0 {
            return None;
        }

        let column = (local_x / self.column_width).floor() as usize;
        let row = (local_y / self.row_height).floor() as usize;
        if row >= self.rows_per_column {
            return None;
        }

        let inside_tile_x = local_x - column as f32 * self.column_width;
        if inside_tile_x > self.cell_width.max(1.0) {
            return None;
        }

        Some(column * self.rows_per_column + row)
    }
}

fn pane_slot_geometry(
    main_left: f32,
    main_width: f32,
    split_open: bool,
    split_pane_ratio: f32,
    slot: i32,
) -> Option<(f32, f32)> {
    match slot {
        0 => Some((
            main_left,
            active_main_pane_width(main_width, split_open, split_pane_ratio).max(1.0),
        )),
        1 if split_open => {
            let width = inactive_main_pane_width(main_width, split_open, split_pane_ratio).max(1.0);
            Some((main_left + main_width - width, width))
        }
        _ => None,
    }
}

pub(crate) fn entry_at_pane_point(
    ui: &AppWindow,
    state: &AppState,
    slot: i32,
    x: f32,
    y: f32,
) -> Option<FileEntry> {
    let layout = ItemViewLayout::from_ui(ui, state, slot)?;
    let index = layout.index_at_point(x, y)?;
    filtered_entry_at_for_slot(state, slot, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Image;

    fn test_entry(index: usize) -> ItemViewEntry {
        ItemViewEntry {
            name: format!("item-{index}").into(),
            path: format!("/tmp/item-{index}").into(),
            group: String::new().into(),
            location: String::new().into(),
            is_dir: false,
            selected: false,
            thumbnail_state: 0,
            media: Image::default(),
            media_token: 0,
            tile_width: 0.0,
            tile_height: 0.0,
            media_x: 0.0,
            media_y: 0.0,
            text_x: 0.0,
            text_width: 0.0,
            group_y: 0.0,
            title_y: 0.0,
            location_y: 0.0,
            metadata_line_height: 0.0,
            title_line_height: 0.0,
            media_width: 0.0,
            media_height: 0.0,
            metadata_font_size: 0.0,
            title_font_size: 0.0,
        }
    }

    #[test]
    fn item_view_layout_hit_test_uses_column_first_order_and_viewport() {
        let layout = ItemViewLayout::new(
            100.0, 50.0, 250.0, 220.0, 300.0, 2, 100.0, 112.0, 10.0, 100.0, 10.0,
        );

        assert_eq!(layout.index_at_point(115.0, 65.0), Some(4));
        assert_eq!(layout.index_at_point(115.0, 165.0), Some(5));
    }

    #[test]
    fn item_view_layout_hit_test_rejects_padding_and_cell_gap() {
        let layout = ItemViewLayout::new(
            100.0, 50.0, 250.0, 220.0, 0.0, 2, 100.0, 112.0, 10.0, 100.0, 10.0,
        );

        assert_eq!(layout.index_at_point(105.0, 65.0), None);
        assert_eq!(layout.index_at_point(221.0, 65.0), None);
        assert_eq!(layout.index_at_point(115.0, 271.0), None);
    }

    #[test]
    fn render_plan_keeps_compact_item_view_entry_geometry_tokens_stable() {
        let mut entries = (4..9).map(test_entry).collect::<Vec<_>>();

        decorate_render_plan(
            &mut entries,
            ItemViewRenderPlanInput {
                cell_width: 159.0,
                render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 1),
                show_location: false,
            },
        );

        let geometry = entries
            .iter()
            .map(|entry| entry.tile_width)
            .collect::<Vec<_>>();
        assert_eq!(geometry, vec![159.0, 159.0, 159.0, 159.0, 159.0]);
        let render_tokens = entries
            .iter()
            .map(|entry| {
                (
                    entry.tile_height,
                    entry.media_x,
                    entry.media_y,
                    entry.text_x,
                    entry.text_width,
                    entry.title_y,
                    entry.title_line_height,
                    entry.media_width,
                    entry.media_height,
                    entry.metadata_font_size,
                    entry.title_font_size,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            render_tokens,
            vec![
                (
                    50.0, 2.0, 2.0, 52.0, 105.0, 14.5, 21.0, 46.0, 46.0, 11.0, 15.0
                ),
                (
                    50.0, 2.0, 2.0, 52.0, 105.0, 14.5, 21.0, 46.0, 46.0, 11.0, 15.0
                ),
                (
                    50.0, 2.0, 2.0, 52.0, 105.0, 14.5, 21.0, 46.0, 46.0, 11.0, 15.0
                ),
                (
                    50.0, 2.0, 2.0, 52.0, 105.0, 14.5, 21.0, 46.0, 46.0, 11.0, 15.0
                ),
                (
                    50.0, 2.0, 2.0, 52.0, 105.0, 14.5, 21.0, 46.0, 46.0, 11.0, 15.0
                ),
            ]
        );
        assert!(
            entries.iter().all(|entry| entry.text_width >= 105.0),
            "compact horizontal titles must keep enough width to remain visible"
        );
        assert!(
            entries
                .iter()
                .all(|entry| !entry.name.is_empty() && entry.title_y > 0.0),
            "visible icon rows must carry a title and title geometry"
        );
    }

    #[test]
    fn render_plan_precomputes_location_text_lines() {
        let mut entries = vec![ItemViewEntry {
            group: "Documents".into(),
            location: "/home/user/Documents".into(),
            ..test_entry(0)
        }];

        decorate_render_plan(
            &mut entries,
            ItemViewRenderPlanInput {
                cell_width: 159.0,
                render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 3),
                show_location: true,
            },
        );

        let entry = &entries[0];
        assert_eq!(entry.media_x, 2.0);
        assert_eq!(entry.media_y, 5.5);
        assert_eq!(entry.text_x, 52.0);
        assert_eq!(entry.text_width, 105.0);
        assert_eq!(entry.metadata_line_height, 14.0);
        assert_eq!(entry.title_line_height, 21.0);
        assert_eq!(entry.group_y, 2.0);
        assert_eq!(entry.title_y, 18.0);
        assert_eq!(entry.location_y, 41.0);
    }

    #[test]
    fn render_plan_keeps_plain_titles_visible_when_location_mode_has_no_metadata() {
        let mut entries = vec![test_entry(0)];

        decorate_render_plan(
            &mut entries,
            ItemViewRenderPlanInput {
                cell_width: 159.0,
                render_metrics: ItemViewRenderMetrics::from_zoom_level_with_text_line_count(2, 3),
                show_location: true,
            },
        );

        let entry = &entries[0];
        assert_eq!(entry.media_x, 2.0);
        assert_eq!(entry.text_x, 52.0);
        assert_eq!(entry.text_width, 105.0);
        assert_eq!(entry.title_y, 18.0);
    }

    #[test]
    fn fallback_media_renderer_supplies_icons_without_replacing_loaded_thumbnails() {
        let metrics = ItemViewRenderMetrics::from_zoom_level_with_text_line_count(1, 1);
        let cache = ItemViewMediaCache::new(metrics, false);
        let mut thumbnail_buffer = SharedPixelBuffer::<Rgba8Pixel>::new(2, 2);
        thumbnail_buffer
            .make_mut_slice()
            .fill(Rgba8Pixel::new(255, 0, 0, 255));
        let thumbnail = Image::from_rgba8(thumbnail_buffer);
        let mut entries = vec![
            ItemViewEntry {
                is_dir: true,
                ..test_entry(0)
            },
            ItemViewEntry {
                thumbnail_state: 2,
                media: thumbnail,
                ..test_entry(1)
            },
        ];

        decorate_fallback_media(&mut entries, &cache);

        let folder_media = entries[0].media.to_rgba8().expect("folder fallback media");
        assert!(
            folder_media
                .as_slice()
                .iter()
                .any(|pixel| pixel.a != 0 && (pixel.r != 0 || pixel.g != 0 || pixel.b != 0))
        );
        let thumbnail_media = entries[1].media.to_rgba8().expect("thumbnail media");
        assert!(
            thumbnail_media
                .as_slice()
                .iter()
                .all(|pixel| *pixel == Rgba8Pixel::new(255, 0, 0, 255))
        );
    }

    #[test]
    fn pane_slot_geometry_matches_split_ratio_model() {
        assert_eq!(
            pane_slot_geometry(280.0, 900.0, false, 0.5, 0),
            Some((280.0, 900.0))
        );
        assert_eq!(pane_slot_geometry(280.0, 900.0, false, 0.5, 1), None);
        assert_eq!(
            pane_slot_geometry(280.0, 900.0, true, 0.5, 0),
            Some((280.0, 449.0))
        );
        assert_eq!(
            pane_slot_geometry(280.0, 900.0, true, 0.5, 1),
            Some((730.0, 450.0))
        );
    }

    #[test]
    fn selection_rect_uses_column_first_item_geometry() {
        let rect = SelectionRect {
            x1: 0.0,
            y1: 0.0,
            x2: 109.0,
            y2: 205.0,
            rows_per_column: 2,
            cell_width: 100.0,
            column_width: 112.0,
            column_offset: 10.0,
            row_height: 100.0,
            padding: 10.0,
        };

        assert!(rect.intersects_index(0));
        assert!(rect.intersects_index(1));
        assert!(!rect.intersects_index(2));
        assert_eq!(rect.candidate_range(4), 0..2);
    }

    #[test]
    fn selection_rect_candidate_range_limits_intersecting_columns() {
        let rect = SelectionRect {
            x1: 244.0,
            y1: 0.0,
            x2: 343.0,
            y2: 205.0,
            rows_per_column: 2,
            cell_width: 100.0,
            column_width: 112.0,
            column_offset: 10.0,
            row_height: 100.0,
            padding: 10.0,
        };

        assert_eq!(rect.candidate_range(20), 2..6);
        assert!(rect.intersects_index(4));
        assert!(rect.intersects_index(5));
        assert!(!rect.intersects_index(2));
        assert!(!rect.intersects_index(6));
    }

    #[test]
    fn item_view_input_turns_blank_click_into_clear_selection() {
        let mut input = ItemViewInputState::default();
        input.press_blank(
            10.0,
            20.0,
            ItemViewInputMetrics::new(3, 100.0, 112.0, 14.0, 50.0, 14.0),
            false,
        );

        assert!(!input.move_blank(14.0, 24.0));
        assert_eq!(
            input.release_blank(14.0, 24.0),
            ItemViewReleaseAction::ClearSelection
        );
    }

    #[test]
    fn item_view_input_turns_blank_drag_into_selection_rect() {
        let mut input = ItemViewInputState::default();
        input.press_blank(
            120.0,
            80.0,
            ItemViewInputMetrics::new(3, 100.0, 112.0, 14.0, 50.0, 14.0),
            true,
        );

        assert!(input.move_blank(40.0, 140.0));
        assert_eq!(
            input.release_blank(40.0, 140.0),
            ItemViewReleaseAction::SelectRect {
                rect: SelectionRect {
                    x1: 40.0,
                    y1: 80.0,
                    x2: 120.0,
                    y2: 140.0,
                    rows_per_column: 3,
                    cell_width: 100.0,
                    column_width: 112.0,
                    column_offset: 14.0,
                    row_height: 50.0,
                    padding: 14.0,
                },
                toggle: true,
            }
        );
    }

    #[test]
    fn item_view_input_cancel_drops_pending_blank_selection() {
        let mut input = ItemViewInputState::default();
        input.press_blank(
            10.0,
            20.0,
            ItemViewInputMetrics::new(3, 100.0, 112.0, 14.0, 50.0, 14.0),
            false,
        );

        input.cancel_blank();

        assert_eq!(
            input.release_blank(100.0, 120.0),
            ItemViewReleaseAction::None
        );
    }
}
