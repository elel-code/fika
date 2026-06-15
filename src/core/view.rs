use std::ops::Range;
use std::sync::Arc;

const EMPTY_CONTENT_EXTENT: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewRect {
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn contains(self, point: ViewPoint) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    pub fn intersects(self, other: ViewRect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompactLayoutOptions {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub reserved_bottom: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub padding: f32,
    pub side_padding: f32,
    pub gap: f32,
    pub text_gap: f32,
    pub item_width: f32,
    pub item_height: f32,
    pub icon_size: f32,
    pub text_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconsLayoutOptions {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub reserved_bottom: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub padding: f32,
    pub gap: f32,
    pub item_width: f32,
    pub item_height: f32,
    pub icon_size: f32,
    pub text_height: f32,
}

impl Default for IconsLayoutOptions {
    fn default() -> Self {
        Self {
            viewport_width: 720.0,
            viewport_height: 520.0,
            reserved_bottom: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding: 2.0,
            gap: 8.0,
            item_width: 96.0,
            item_height: 72.0,
            icon_size: 48.0,
            text_height: 18.0,
        }
    }
}

impl Default for CompactLayoutOptions {
    fn default() -> Self {
        Self {
            viewport_width: 720.0,
            viewport_height: 520.0,
            reserved_bottom: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            padding: 8.0,
            side_padding: 8.0,
            gap: 8.0,
            text_gap: 8.0,
            item_width: 168.0,
            item_height: 76.0,
            icon_size: 40.0,
            text_height: 32.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemLayout {
    pub model_index: usize,
    pub column: usize,
    pub row: usize,
    pub item_rect: ViewRect,
    pub visual_rect: ViewRect,
    pub icon_rect: ViewRect,
    pub text_rect: ViewRect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompactColumnMetrics {
    column_widths: Arc<[f32]>,
    column_offsets: Arc<[f32]>,
}

impl CompactColumnMetrics {
    pub fn new(
        column_count: usize,
        min_width: f32,
        padding: f32,
        gap: f32,
        column_widths: impl Into<Arc<[f32]>>,
    ) -> Self {
        let column_widths = normalize_column_widths(column_count, min_width, column_widths.into());
        let column_offsets = column_offsets(padding, gap, &column_widths);
        Self {
            column_widths,
            column_offsets,
        }
    }

    pub fn column_count(&self) -> usize {
        self.column_widths.len()
    }

    pub fn column_width(&self, column: usize) -> Option<f32> {
        self.column_widths.get(column).copied()
    }

    fn width(&self, column: usize) -> f32 {
        self.column_widths[column]
    }

    fn offset(&self, column: usize) -> f32 {
        self.column_offsets[column]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompactLayout {
    options: CompactLayoutOptions,
    item_count: usize,
    rows_per_column: usize,
    column_metrics: CompactColumnMetrics,
    content_size: ViewSize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconsLayout {
    options: IconsLayoutOptions,
    item_count: usize,
    columns_per_row: usize,
    row_count: usize,
    column_start_x: f32,
    column_pitch: f32,
    rows: IconsLayoutRows,
    content_size: ViewSize,
}

#[derive(Clone, Debug, PartialEq)]
enum IconsLayoutRows {
    Uniform,
    Variable {
        item_heights: Arc<[f32]>,
        row_heights: Arc<[f32]>,
        row_offsets: Arc<[f32]>,
    },
}

impl CompactLayout {
    pub fn new(item_count: usize, options: CompactLayoutOptions) -> Self {
        let rows_per_column = rows_per_column(options);
        let column_count = item_count.div_ceil(rows_per_column);
        let column_metrics = CompactColumnMetrics::new(
            column_count,
            options.item_width,
            options.side_padding,
            options.gap,
            vec![options.item_width; column_count],
        );
        Self::new_with_column_metrics(item_count, options, column_metrics)
    }

    pub fn new_with_column_widths(
        item_count: usize,
        options: CompactLayoutOptions,
        column_widths: impl Into<Arc<[f32]>>,
    ) -> Self {
        let rows_per_column = rows_per_column(options);
        let column_count = item_count.div_ceil(rows_per_column);
        let column_metrics = CompactColumnMetrics::new(
            column_count,
            options.item_width,
            options.side_padding,
            options.gap,
            column_widths,
        );
        Self::new_with_column_metrics(item_count, options, column_metrics)
    }

    pub fn new_with_column_metrics(
        item_count: usize,
        options: CompactLayoutOptions,
        column_metrics: CompactColumnMetrics,
    ) -> Self {
        let rows_per_column = rows_per_column(options);
        let column_count = item_count.div_ceil(rows_per_column);
        let column_metrics = if column_metrics.column_count() == column_count {
            column_metrics
        } else {
            CompactColumnMetrics::new(
                column_count,
                options.item_width,
                options.side_padding,
                options.gap,
                vec![options.item_width; column_count],
            )
        };
        let content_width = if item_count == 0 {
            EMPTY_CONTENT_EXTENT
        } else {
            column_metrics.offset(column_count - 1)
                + column_metrics.width(column_count - 1)
                + options.side_padding
        };
        let visible_rows = item_count.min(rows_per_column);
        let content_height = if item_count == 0 {
            EMPTY_CONTENT_EXTENT
        } else {
            (visible_rows as f32 * options.item_height).max(options.viewport_height)
        };

        Self {
            options,
            item_count,
            rows_per_column,
            column_metrics,
            content_size: ViewSize {
                width: content_width,
                height: content_height,
            },
        }
    }

    pub fn rows_per_column(&self) -> usize {
        self.rows_per_column
    }

    pub fn rows_per_column_for_options(options: CompactLayoutOptions) -> usize {
        rows_per_column(options)
    }

    pub fn column_count(&self) -> usize {
        self.column_metrics.column_count()
    }

    pub fn content_size(&self) -> ViewSize {
        self.content_size
    }

    pub fn viewport_rect(&self) -> ViewRect {
        ViewRect {
            x: self.options.scroll_x,
            y: self.options.scroll_y,
            width: self.options.viewport_width,
            height: self.options.viewport_height,
        }
    }

    pub fn item(&self, model_index: usize) -> Option<ItemLayout> {
        self.item_with_required_text_width(model_index, None)
    }

    pub fn item_with_required_text_width(
        &self,
        model_index: usize,
        required_text_width: Option<f32>,
    ) -> Option<ItemLayout> {
        if model_index >= self.item_count {
            return None;
        }
        let column = model_index / self.rows_per_column;
        let row = model_index % self.rows_per_column;
        let x = self.column_metrics.offset(column);
        let item_width = self.column_metrics.width(column);
        let row_metrics = compact_row_metrics(self.options, self.item_count, self.rows_per_column);
        let y = row_metrics.offset + row as f32 * row_metrics.pitch;
        let item_rect = ViewRect {
            x,
            y,
            width: item_width,
            height: self.options.item_height,
        };
        let icon_rect = ViewRect {
            x: x + self.options.padding,
            y: y + (self.options.item_height - self.options.icon_size) / 2.0,
            width: self.options.icon_size,
            height: self.options.icon_size,
        };
        let text_x = icon_rect.right() + self.options.text_gap;
        let available_text_width = (item_rect.right() - text_x - self.options.padding).max(0.0);
        let text_width = required_text_width
            .map(|width| width + self.options.padding * 2.0)
            .unwrap_or(available_text_width)
            .clamp(0.0, available_text_width);
        let text_rect = ViewRect {
            x: text_x,
            y: y + (self.options.item_height - self.options.text_height) / 2.0,
            width: text_width,
            height: self.options.text_height,
        };
        let visual_left = icon_rect.x.min(text_rect.x);
        let visual_right = icon_rect.right().max(text_rect.right());
        let visual_top = icon_rect.y.min(text_rect.y);
        let visual_bottom = icon_rect.bottom().max(text_rect.bottom());
        let visual_rect = ViewRect {
            x: (visual_left - self.options.padding).max(item_rect.x),
            y: (visual_top - self.options.padding).max(item_rect.y),
            width: (visual_right - visual_left + self.options.padding * 2.0).min(item_rect.width),
            height: (visual_bottom - visual_top + self.options.padding * 2.0).min(item_rect.height),
        };
        Some(ItemLayout {
            model_index,
            column,
            row,
            item_rect,
            visual_rect,
            icon_rect,
            text_rect,
        })
    }

    pub fn items(&self) -> impl Iterator<Item = ItemLayout> + '_ {
        (0..self.item_count).filter_map(|index| self.item(index))
    }

    pub fn visible_items(&self) -> impl Iterator<Item = ItemLayout> + '_ {
        let viewport = self.viewport_rect();
        let column_range = self.column_range_intersecting_x(viewport.x, viewport.right());

        column_range.flat_map(move |column| {
            let column_start = column * self.rows_per_column;
            let column_end = (column_start + self.rows_per_column).min(self.item_count);
            (column_start..column_end).filter_map(move |index| {
                self.item(index)
                    .filter(|item| item.item_rect.intersects(viewport))
            })
        })
    }

    pub fn visible_column_range(&self) -> Range<usize> {
        let viewport = self.viewport_rect();
        self.column_range_intersecting_x(viewport.x, viewport.right())
    }

    pub fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        if self.item_count == 0 {
            return None;
        }
        let row_metrics = compact_row_metrics(self.options, self.item_count, self.rows_per_column);
        if row_metrics.pitch <= 0.0 {
            return None;
        }
        let column = self.column_at_x(point.x)?;
        let row = ((point.y - row_metrics.offset) / row_metrics.pitch).floor();
        if row < 0.0 {
            return None;
        }
        let row = row as usize;
        if row >= self.rows_per_column {
            return None;
        }
        let index = column * self.rows_per_column + row;
        self.item(index)
            .and_then(|item| item.item_rect.contains(point).then_some(item.model_index))
    }

    pub fn hit_test_viewport_point(&self, point: ViewPoint) -> Option<usize> {
        self.hit_test_content_point(ViewPoint {
            x: point.x + self.options.scroll_x,
            y: point.y + self.options.scroll_y,
        })
    }

    pub fn indexes_intersecting(&self, rect: ViewRect) -> RangeSelection {
        let column_range = self.column_range_intersecting(rect);
        let row_range = self.row_range_intersecting(rect);
        let indexes = column_range
            .flat_map(|column| {
                row_range.clone().filter_map(move |row| {
                    let index = column * self.rows_per_column + row;
                    self.item(index)
                        .filter(|item| item.item_rect.intersects(rect))
                        .map(|item| item.model_index)
                })
            })
            .collect();
        RangeSelection { indexes }
    }

    fn column_range_intersecting(&self, rect: ViewRect) -> Range<usize> {
        self.column_range_intersecting_x(rect.x, rect.right())
    }

    fn row_range_intersecting(&self, rect: ViewRect) -> Range<usize> {
        visible_axis_range(
            rect.y,
            rect.bottom(),
            compact_row_metrics(self.options, self.item_count, self.rows_per_column).offset,
            self.options.item_height,
            compact_row_metrics(self.options, self.item_count, self.rows_per_column).gap,
            self.rows_per_column,
        )
    }

    fn column_range_intersecting_x(&self, visible_start: f32, visible_end: f32) -> Range<usize> {
        if self.column_metrics.column_widths.is_empty() || visible_end <= visible_start {
            return 0..0;
        }

        let start = first_column_with_right_after(
            &self.column_metrics.column_offsets,
            &self.column_metrics.column_widths,
            visible_start,
        );
        let end =
            first_column_starting_at_or_after(&self.column_metrics.column_offsets, visible_end);
        start..end.max(start).min(self.column_metrics.column_widths.len())
    }

    fn column_at_x(&self, x: f32) -> Option<usize> {
        let index = first_column_with_right_after(
            &self.column_metrics.column_offsets,
            &self.column_metrics.column_widths,
            x,
        );
        let left = *self.column_metrics.column_offsets.get(index)?;
        let right = left + self.column_metrics.column_widths[index];
        (x >= left && x < right).then_some(index)
    }
}

impl IconsLayout {
    pub fn new(item_count: usize, options: IconsLayoutOptions) -> Self {
        let columns_per_row = columns_per_row(options);
        let row_count = item_count.div_ceil(columns_per_row);
        let column_metrics = icons_column_metrics(options, item_count, columns_per_row);
        let content_width = if item_count == 0 {
            EMPTY_CONTENT_EXTENT
        } else {
            icons_content_width(options, item_count, columns_per_row, column_metrics)
        };
        let content_height = if item_count == 0 {
            EMPTY_CONTENT_EXTENT
        } else {
            (options.gap.max(0.0)
                + row_count as f32 * (options.item_height.max(1.0) + options.gap.max(0.0)))
            .max(options.viewport_height)
        };

        Self {
            options,
            item_count,
            columns_per_row,
            row_count,
            column_start_x: column_metrics.start_x,
            column_pitch: column_metrics.pitch,
            rows: IconsLayoutRows::Uniform,
            content_size: ViewSize {
                width: content_width,
                height: content_height,
            },
        }
    }

    pub fn new_with_item_heights(
        item_count: usize,
        options: IconsLayoutOptions,
        item_heights: impl Into<Arc<[f32]>>,
    ) -> Self {
        if item_count == 0 {
            return Self::new(item_count, options);
        }

        let base_height = options.item_height.max(1.0);
        let supplied = item_heights.into();
        let mut normalized = Vec::with_capacity(item_count);
        let mut has_variable_height = false;
        for index in 0..item_count {
            let height = supplied.get(index).copied().unwrap_or(base_height);
            let height = height.max(base_height);
            has_variable_height |= height != base_height;
            normalized.push(height);
        }

        if !has_variable_height {
            return Self::new(item_count, options);
        }

        let columns_per_row = columns_per_row(options);
        let row_count = item_count.div_ceil(columns_per_row);
        let column_metrics = icons_column_metrics(options, item_count, columns_per_row);
        let mut row_offsets = Vec::with_capacity(row_count);
        let mut row_heights = Vec::with_capacity(row_count);
        let mut y = options.gap.max(0.0);

        for row in 0..row_count {
            row_offsets.push(y);
            let row_start = row * columns_per_row;
            let row_end = (row_start + columns_per_row).min(item_count);
            let row_height = normalized[row_start..row_end]
                .iter()
                .copied()
                .fold(base_height, f32::max);
            row_heights.push(row_height);
            y += row_height + options.gap.max(0.0);
        }

        let content_width =
            icons_content_width(options, item_count, columns_per_row, column_metrics);
        let content_height = y.max(options.viewport_height);

        Self {
            options,
            item_count,
            columns_per_row,
            row_count,
            column_start_x: column_metrics.start_x,
            column_pitch: column_metrics.pitch,
            rows: IconsLayoutRows::Variable {
                item_heights: Arc::from(normalized),
                row_heights: Arc::from(row_heights),
                row_offsets: Arc::from(row_offsets),
            },
            content_size: ViewSize {
                width: content_width,
                height: content_height,
            },
        }
    }

    pub fn columns_per_row(&self) -> usize {
        self.columns_per_row
    }

    pub fn row_count(&self) -> usize {
        self.row_count
    }

    pub fn content_size(&self) -> ViewSize {
        self.content_size
    }

    pub fn viewport_rect(&self) -> ViewRect {
        ViewRect {
            x: self.options.scroll_x,
            y: self.options.scroll_y,
            width: self.options.viewport_width,
            height: self.options.viewport_height,
        }
    }

    pub fn item(&self, model_index: usize) -> Option<ItemLayout> {
        self.item_with_required_text_width(model_index, None)
    }

    pub fn item_with_required_text_width(
        &self,
        model_index: usize,
        required_text_width: Option<f32>,
    ) -> Option<ItemLayout> {
        if model_index >= self.item_count {
            return None;
        }
        let column = model_index % self.columns_per_row;
        let row = model_index / self.columns_per_row;
        let x = self.column_start_x + column as f32 * self.column_pitch;
        let y = self.row_y(row)?;
        let item_height = self.item_height(model_index)?;
        let item_rect = ViewRect {
            x,
            y,
            width: self.options.item_width,
            height: item_height,
        };
        let icon_rect = ViewRect {
            x: x + (self.options.item_width - self.options.icon_size).max(0.0) / 2.0,
            y: y + self.options.padding,
            width: self.options.icon_size,
            height: self.options.icon_size,
        };
        let available_text_width = (self.options.item_width - self.options.padding * 2.0).max(0.0);
        let text_width = required_text_width
            .map(|width| width + self.options.padding * 2.0)
            .unwrap_or(available_text_width)
            .clamp(0.0, available_text_width);
        let text_rect = ViewRect {
            x: x + (self.options.item_width - text_width).max(0.0) / 2.0,
            y: y + self.options.icon_size + self.options.padding * 2.0,
            width: text_width,
            height: self.item_text_height(item_height),
        };
        let visual_left = icon_rect.x.min(text_rect.x);
        let visual_right = icon_rect.right().max(text_rect.right());
        let visual_top = icon_rect.y.min(text_rect.y);
        let visual_bottom = icon_rect.bottom().max(text_rect.bottom());
        let visual_rect = ViewRect {
            x: (visual_left - self.options.padding).max(item_rect.x),
            y: (visual_top - self.options.padding).max(item_rect.y),
            width: (visual_right - visual_left + self.options.padding * 2.0).min(item_rect.width),
            height: (visual_bottom - visual_top + self.options.padding * 2.0).min(item_rect.height),
        };
        Some(ItemLayout {
            model_index,
            column,
            row,
            item_rect,
            visual_rect,
            icon_rect,
            text_rect,
        })
    }

    pub fn visible_items(&self) -> impl Iterator<Item = ItemLayout> + '_ {
        let viewport = self.viewport_rect();
        let row_range = self.row_range_intersecting_y(viewport.y, viewport.bottom());
        row_range.flat_map(move |row| {
            let row_start = row * self.columns_per_row;
            let row_end = (row_start + self.columns_per_row).min(self.item_count);
            (row_start..row_end).filter_map(move |index| {
                self.item(index)
                    .filter(|item| item.item_rect.intersects(viewport))
            })
        })
    }

    pub fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        if self.item_count == 0 {
            return None;
        }
        if self.column_pitch <= 0.0 {
            return None;
        }
        let column = ((point.x - self.column_start_x) / self.column_pitch).floor();
        if column < 0.0 {
            return None;
        }
        let column = column as usize;
        let row = self.row_at_y(point.y)?;
        if column >= self.columns_per_row || row >= self.row_count {
            return None;
        }
        let index = row * self.columns_per_row + column;
        self.item(index)
            .and_then(|item| item.item_rect.contains(point).then_some(item.model_index))
    }

    pub fn indexes_intersecting(&self, rect: ViewRect) -> RangeSelection {
        let row_range = self.row_range_intersecting_y(rect.y, rect.bottom());
        let column_range = self.column_range_intersecting_x(rect.x, rect.right());
        let indexes = row_range
            .flat_map(|row| {
                column_range.clone().filter_map(move |column| {
                    let index = row * self.columns_per_row + column;
                    self.item(index)
                        .filter(|item| item.item_rect.intersects(rect))
                        .map(|item| item.model_index)
                })
            })
            .collect();
        RangeSelection { indexes }
    }

    fn row_range_intersecting_y(&self, visible_start: f32, visible_end: f32) -> Range<usize> {
        match &self.rows {
            IconsLayoutRows::Uniform => visible_axis_range(
                visible_start,
                visible_end,
                self.options.gap.max(0.0),
                self.options.item_height.max(1.0),
                self.options.gap.max(0.0),
                self.row_count,
            ),
            IconsLayoutRows::Variable {
                row_heights,
                row_offsets,
                ..
            } => {
                if visible_end <= visible_start {
                    return 0..0;
                }
                let start = first_row_with_bottom_after(row_offsets, row_heights, visible_start);
                let end = first_row_starting_at_or_after(row_offsets, visible_end);
                start..end.max(start).min(self.row_count)
            }
        }
    }

    fn column_range_intersecting_x(&self, visible_start: f32, visible_end: f32) -> Range<usize> {
        visible_axis_range(
            visible_start,
            visible_end,
            self.column_start_x,
            self.options.item_width,
            self.column_pitch - self.options.item_width,
            self.columns_per_row,
        )
    }

    fn row_y(&self, row: usize) -> Option<f32> {
        if row >= self.row_count {
            return None;
        }
        Some(match &self.rows {
            IconsLayoutRows::Uniform => {
                self.options.gap.max(0.0)
                    + row as f32 * (self.options.item_height.max(1.0) + self.options.gap.max(0.0))
            }
            IconsLayoutRows::Variable { row_offsets, .. } => row_offsets[row],
        })
    }

    fn row_height(&self, row: usize) -> Option<f32> {
        if row >= self.row_count {
            return None;
        }
        Some(match &self.rows {
            IconsLayoutRows::Uniform => self.options.item_height.max(1.0),
            IconsLayoutRows::Variable { row_heights, .. } => row_heights[row],
        })
    }

    fn item_height(&self, model_index: usize) -> Option<f32> {
        if model_index >= self.item_count {
            return None;
        }
        Some(match &self.rows {
            IconsLayoutRows::Uniform => self.options.item_height.max(1.0),
            IconsLayoutRows::Variable { item_heights, .. } => item_heights[model_index],
        })
    }

    fn item_text_height(&self, item_height: f32) -> f32 {
        (item_height - self.options.icon_size - self.options.padding * 3.0)
            .max(self.options.text_height.max(1.0))
    }

    fn row_at_y(&self, y: f32) -> Option<usize> {
        match &self.rows {
            IconsLayoutRows::Uniform => {
                let pitch_y = self.options.item_height.max(1.0) + self.options.gap.max(0.0);
                if pitch_y <= 0.0 {
                    return None;
                }
                let row = ((y - self.options.gap.max(0.0)) / pitch_y).floor();
                if row < 0.0 {
                    return None;
                }
                let row = row as usize;
                let row_y = self.row_y(row)?;
                let row_height = self.row_height(row)?;
                (y >= row_y && y < row_y + row_height).then_some(row)
            }
            IconsLayoutRows::Variable {
                row_heights,
                row_offsets,
                ..
            } => {
                let row = first_row_with_bottom_after(row_offsets, row_heights, y);
                let row_y = *row_offsets.get(row)?;
                let row_height = row_heights[row];
                (y >= row_y && y < row_y + row_height).then_some(row)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeSelection {
    indexes: Vec<usize>,
}

impl RangeSelection {
    pub fn indexes(&self) -> &[usize] {
        &self.indexes
    }

    pub fn range(&self) -> Option<Range<usize>> {
        let first = *self.indexes.first()?;
        let last = *self.indexes.last()?;
        Some(first..last + 1)
    }
}

fn rows_per_column(options: CompactLayoutOptions) -> usize {
    (compact_available_height(options) / options.item_height)
        .floor()
        .max(1.0) as usize
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CompactRowMetrics {
    offset: f32,
    pitch: f32,
    gap: f32,
}

fn compact_row_metrics(
    options: CompactLayoutOptions,
    item_count: usize,
    rows_per_column: usize,
) -> CompactRowMetrics {
    let item_height = options.item_height.max(1.0);
    let rows_per_column = rows_per_column.max(1);
    let available = compact_available_height(options);
    let should_distribute = item_count > rows_per_column && item_height >= 32.0;
    let unused = (available - rows_per_column as f32 * item_height).max(0.0);
    let gap = if should_distribute && unused > 0.0 {
        unused / (rows_per_column as f32 + 1.0)
    } else {
        0.0
    };
    CompactRowMetrics {
        offset: gap,
        pitch: item_height + gap,
        gap,
    }
}

fn compact_available_height(options: CompactLayoutOptions) -> f32 {
    (options.viewport_height - options.reserved_bottom).max(options.item_height.max(1.0))
}

fn columns_per_row(options: IconsLayoutOptions) -> usize {
    let item_margin = options.gap.max(0.0);
    let item_width = options.item_width.max(1.0);
    let column_width = item_width + item_margin;
    let width_for_columns = (options.viewport_width - item_margin).max(column_width);
    (width_for_columns / column_width).floor().max(1.0) as usize
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct IconsColumnMetrics {
    start_x: f32,
    pitch: f32,
}

fn icons_column_metrics(
    options: IconsLayoutOptions,
    item_count: usize,
    columns_per_row: usize,
) -> IconsColumnMetrics {
    let item_margin = options.gap.max(0.0);
    let item_width = options.item_width.max(1.0);
    let mut pitch = item_width + item_margin;
    let width_for_columns = (options.viewport_width - item_margin).max(pitch);
    let mut start_x = item_margin;

    if item_count > columns_per_row && pitch >= 32.0 {
        let unused_width = width_for_columns - columns_per_row as f32 * pitch;
        if unused_width > 0.0 {
            let column_inc = unused_width / (columns_per_row as f32 + 1.0);
            pitch += column_inc;
            start_x += column_inc;
        }
    }

    IconsColumnMetrics { start_x, pitch }
}

fn icons_content_width(
    options: IconsLayoutOptions,
    item_count: usize,
    columns_per_row: usize,
    column_metrics: IconsColumnMetrics,
) -> f32 {
    let visible_columns = item_count.min(columns_per_row);
    if visible_columns == 0 {
        return EMPTY_CONTENT_EXTENT;
    }
    (column_metrics.start_x
        + visible_columns.saturating_sub(1) as f32 * column_metrics.pitch
        + options.item_width
        + options.gap.max(0.0))
    .max(options.viewport_width)
}

fn normalize_column_widths(
    column_count: usize,
    min_width: f32,
    supplied: Arc<[f32]>,
) -> Arc<[f32]> {
    if column_count == 0 {
        return Arc::from(Vec::<f32>::new());
    }

    let min_width = min_width.max(1.0);
    let mut widths = Vec::with_capacity(column_count);
    widths.extend(
        supplied
            .iter()
            .take(column_count)
            .map(|width| width.max(min_width)),
    );
    widths.resize(column_count, min_width);
    Arc::from(widths)
}

fn column_offsets(padding: f32, gap: f32, column_widths: &[f32]) -> Arc<[f32]> {
    let mut offsets = Vec::with_capacity(column_widths.len());
    let mut x = padding;
    for width in column_widths {
        offsets.push(x);
        x += *width + gap;
    }
    Arc::from(offsets)
}

fn first_column_with_right_after(offsets: &[f32], widths: &[f32], x: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] + widths[mid] <= x {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn first_column_starting_at_or_after(offsets: &[f32], x: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] < x {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn visible_axis_range(
    visible_start: f32,
    visible_end: f32,
    padding: f32,
    item_extent: f32,
    gap: f32,
    count: usize,
) -> Range<usize> {
    if count == 0 || visible_end <= visible_start {
        return 0..0;
    }
    let pitch = item_extent + gap;
    if pitch <= 0.0 || item_extent <= 0.0 {
        return 0..0;
    }
    let start = ((visible_start - padding - item_extent) / pitch).floor() as isize + 1;
    let end = ((visible_end - padding) / pitch).ceil() as isize;
    start.max(0) as usize..(end.max(0) as usize).min(count)
}

fn first_row_with_bottom_after(offsets: &[f32], heights: &[f32], y: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len().min(heights.len());
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] + heights[mid] <= y {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn first_row_starting_at_or_after(offsets: &[f32], y: f32) -> usize {
    let mut low = 0usize;
    let mut high = offsets.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if offsets[mid] < y {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_layout_fills_rows_before_columns() {
        let layout = CompactLayout::new(
            7,
            CompactLayoutOptions {
                viewport_height: 188.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                side_padding: 4.0,
                ..CompactLayoutOptions::default()
            },
        );

        assert_eq!(layout.rows_per_column(), 3);
        assert_eq!(layout.item(0).unwrap().column, 0);
        assert_eq!(layout.item(2).unwrap().row, 2);
        assert_eq!(layout.item(3).unwrap().column, 1);
        assert_eq!(layout.item(3).unwrap().row, 0);
    }

    #[test]
    fn compact_layout_hit_test_uses_model_index_not_row_index() {
        let layout = CompactLayout::new(
            6,
            CompactLayoutOptions {
                viewport_height: 128.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                ..CompactLayoutOptions::default()
            },
        );

        assert_eq!(layout.rows_per_column(), 2);
        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 118.0, y: 10.0 }),
            Some(2)
        );
        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 118.0, y: 70.0 }),
            Some(3)
        );
    }

    #[test]
    fn compact_layout_distributes_unused_height_between_rows() {
        let layout = CompactLayout::new(
            6,
            CompactLayoutOptions {
                viewport_height: 128.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                ..CompactLayoutOptions::default()
            },
        );

        let first = layout.item(0).unwrap().item_rect;
        let second = layout.item(1).unwrap().item_rect;
        let distributed_gap = (128.0 - 2.0 * 50.0) / 3.0;

        assert!((first.y - distributed_gap).abs() < 0.01);
        assert!((second.y - (50.0 + 2.0 * distributed_gap)).abs() < 0.01);
        assert!((128.0 - second.bottom() - distributed_gap).abs() < 0.01);
    }

    #[test]
    fn compact_layout_visible_items_respect_horizontal_scroll() {
        let layout = CompactLayout::new(
            12,
            CompactLayoutOptions {
                viewport_width: 110.0,
                viewport_height: 128.0,
                scroll_x: 114.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                ..CompactLayoutOptions::default()
            },
        );

        let indexes = layout
            .visible_items()
            .map(|item| item.model_index)
            .collect::<Vec<_>>();

        assert_eq!(indexes, vec![2, 3]);
    }

    #[test]
    fn compact_layout_uses_variable_column_widths() {
        let layout = CompactLayout::new_with_column_widths(
            6,
            CompactLayoutOptions {
                viewport_height: 128.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                side_padding: 4.0,
                ..CompactLayoutOptions::default()
            },
            vec![100.0, 180.0, 120.0],
        );

        assert_eq!(layout.rows_per_column(), 2);
        assert_eq!(layout.item(2).unwrap().item_rect.width, 180.0);
        assert_eq!(layout.item(4).unwrap().item_rect.x, 304.0);
        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 108.0, y: 8.0 }),
            None
        );
    }

    #[test]
    fn compact_layout_visual_rect_follows_required_text_width() {
        let layout = CompactLayout::new_with_column_widths(
            2,
            CompactLayoutOptions {
                viewport_height: 128.0,
                item_width: 160.0,
                item_height: 50.0,
                icon_size: 24.0,
                text_height: 20.0,
                gap: 10.0,
                padding: 4.0,
                ..CompactLayoutOptions::default()
            },
            vec![240.0],
        );

        let full = layout.item(0).unwrap();
        let narrow = layout.item_with_required_text_width(0, Some(28.0)).unwrap();

        assert!(narrow.visual_rect.width < full.visual_rect.width);
        assert!(narrow.visual_rect.width >= narrow.icon_rect.width + narrow.text_rect.width);
        assert!(narrow.visual_rect.contains(ViewPoint {
            x: narrow.icon_rect.x,
            y: narrow.icon_rect.y
        }));
        assert!(!narrow.visual_rect.contains(ViewPoint {
            x: full.item_rect.right() - 2.0,
            y: narrow.visual_rect.y + 1.0
        }));
    }

    #[test]
    fn compact_layout_visible_items_scale_with_viewport_not_model_size() {
        let layout = CompactLayout::new(
            1_000_000,
            CompactLayoutOptions {
                viewport_width: 220.0,
                viewport_height: 128.0,
                scroll_x: 100_000.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                ..CompactLayoutOptions::default()
            },
        );

        let indexes = layout
            .visible_items()
            .map(|item| item.model_index)
            .collect::<Vec<_>>();

        assert!(!indexes.is_empty());
        assert!(indexes.len() <= layout.rows_per_column() * 4);
        assert!(indexes.iter().all(|index| *index < 1_000_000));
    }

    #[test]
    fn selection_rect_returns_model_indexes_in_layout_order() {
        let layout = CompactLayout::new(
            8,
            CompactLayoutOptions {
                viewport_height: 128.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 4.0,
                ..CompactLayoutOptions::default()
            },
        );

        let selection = layout.indexes_intersecting(ViewRect {
            x: 0.0,
            y: 60.0,
            width: 220.0,
            height: 60.0,
        });

        assert_eq!(selection.indexes(), &[1, 3]);
        assert_eq!(selection.range(), Some(1..4));
    }

    #[test]
    fn compact_layout_rows_reserve_bottom_space() {
        let layout = CompactLayout::new(
            8,
            CompactLayoutOptions {
                viewport_height: 140.0,
                reserved_bottom: 20.0,
                item_height: 50.0,
                gap: 10.0,
                padding: 0.0,
                ..CompactLayoutOptions::default()
            },
        );

        assert_eq!(layout.rows_per_column(), 2);
        assert_eq!(layout.item(2).unwrap().column, 1);
        assert_eq!(layout.item(2).unwrap().row, 0);
    }

    #[test]
    fn compact_empty_layout_does_not_inherit_viewport_extent() {
        let layout = CompactLayout::new(
            0,
            CompactLayoutOptions {
                viewport_width: 720.0,
                viewport_height: 520.0,
                ..CompactLayoutOptions::default()
            },
        );

        assert_eq!(layout.content_size().width, EMPTY_CONTENT_EXTENT);
        assert_eq!(layout.content_size().height, EMPTY_CONTENT_EXTENT);
        assert_eq!(layout.visible_items().count(), 0);
    }

    #[test]
    fn icons_layout_fills_columns_before_rows() {
        let layout = IconsLayout::new(
            7,
            IconsLayoutOptions {
                viewport_width: 340.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..IconsLayoutOptions::default()
            },
        );

        assert_eq!(layout.columns_per_row(), 3);
        assert_eq!(layout.item(0).unwrap().row, 0);
        assert_eq!(layout.item(2).unwrap().column, 2);
        assert_eq!(layout.item(3).unwrap().row, 1);
        assert_eq!(layout.item(3).unwrap().column, 0);
    }

    #[test]
    fn icons_empty_layout_does_not_inherit_viewport_extent() {
        let layout = IconsLayout::new(
            0,
            IconsLayoutOptions {
                viewport_width: 720.0,
                viewport_height: 520.0,
                ..IconsLayoutOptions::default()
            },
        );

        assert_eq!(layout.content_size().width, EMPTY_CONTENT_EXTENT);
        assert_eq!(layout.content_size().height, EMPTY_CONTENT_EXTENT);
        assert_eq!(layout.visible_items().count(), 0);
    }

    #[test]
    fn icons_layout_visible_items_respect_vertical_scroll() {
        let layout = IconsLayout::new(
            12,
            IconsLayoutOptions {
                viewport_width: 230.0,
                viewport_height: 90.0,
                scroll_y: 94.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..IconsLayoutOptions::default()
            },
        );

        let indexes = layout
            .visible_items()
            .map(|item| item.model_index)
            .collect::<Vec<_>>();

        assert_eq!(indexes, vec![2, 3]);
    }

    #[test]
    fn icons_layout_visible_items_scale_with_viewport_not_model_size() {
        let layout = IconsLayout::new(
            1_000_000,
            IconsLayoutOptions {
                viewport_width: 340.0,
                viewport_height: 190.0,
                scroll_y: 100_000.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..IconsLayoutOptions::default()
            },
        );

        let indexes = layout
            .visible_items()
            .map(|item| item.model_index)
            .collect::<Vec<_>>();

        assert!(!indexes.is_empty());
        assert!(indexes.len() <= layout.columns_per_row() * 4);
        assert!(indexes.iter().all(|index| *index < 1_000_000));
    }

    #[test]
    fn icons_layout_hit_test_uses_row_major_index() {
        let layout = IconsLayout::new(
            6,
            IconsLayoutOptions {
                viewport_width: 230.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..IconsLayoutOptions::default()
            },
        );

        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 128.0, y: 12.0 }),
            Some(1)
        );
        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 18.0, y: 102.0 }),
            Some(2)
        );
    }

    #[test]
    fn icons_layout_uses_row_maximum_item_height() {
        let layout = IconsLayout::new_with_item_heights(
            6,
            IconsLayoutOptions {
                viewport_width: 230.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..IconsLayoutOptions::default()
            },
            vec![120.0, 80.0, 80.0, 80.0, 80.0, 80.0],
        );

        assert_eq!(layout.columns_per_row(), 2);
        assert_eq!(layout.item(0).unwrap().item_rect.height, 120.0);
        assert_eq!(layout.item(1).unwrap().item_rect.height, 80.0);
        assert_eq!(layout.item(2).unwrap().item_rect.y, 140.0);
        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 18.0, y: 135.0 }),
            None
        );
    }

    #[test]
    fn icons_layout_item_height_only_offsets_following_rows() {
        let layout = IconsLayout::new_with_item_heights(
            6,
            IconsLayoutOptions {
                viewport_width: 230.0,
                item_width: 100.0,
                item_height: 80.0,
                gap: 10.0,
                padding: 4.0,
                ..IconsLayoutOptions::default()
            },
            vec![80.0, 80.0, 120.0, 80.0, 80.0, 80.0],
        );

        assert_eq!(layout.item(0).unwrap().item_rect.y, 10.0);
        assert_eq!(layout.item(2).unwrap().item_rect.y, 100.0);
        assert_eq!(layout.item(4).unwrap().item_rect.y, 230.0);
    }
}
