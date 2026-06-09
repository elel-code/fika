use std::ops::Range;

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
    pub gap: f32,
    pub item_width: f32,
    pub item_height: f32,
    pub icon_size: f32,
    pub text_height: f32,
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
            gap: 8.0,
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
    pub icon_rect: ViewRect,
    pub text_rect: ViewRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalScrollBarLayout {
    pub track_rect: ViewRect,
    pub handle_rect: ViewRect,
    pub max_scroll_x: f32,
}

impl HorizontalScrollBarLayout {
    pub fn scroll_x_for_handle_x(self, handle_x: f32) -> f32 {
        let travel = (self.track_rect.width - self.handle_rect.width).max(0.0);
        if travel <= 0.0 || self.max_scroll_x <= 0.0 {
            return 0.0;
        }
        let local_x = (handle_x - self.track_rect.x).clamp(0.0, travel);
        local_x / travel * self.max_scroll_x
    }

    pub fn scroll_x_for_track_x(self, track_x: f32) -> f32 {
        self.scroll_x_for_handle_x(track_x - self.handle_rect.width / 2.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompactLayout {
    options: CompactLayoutOptions,
    item_count: usize,
    rows_per_column: usize,
    content_size: ViewSize,
}

impl CompactLayout {
    pub fn new(item_count: usize, options: CompactLayoutOptions) -> Self {
        let rows_per_column = rows_per_column(options);
        let column_count = item_count.div_ceil(rows_per_column);
        let content_width = if column_count == 0 {
            options.viewport_width.max(options.padding * 2.0)
        } else {
            options.padding * 2.0
                + column_count as f32 * options.item_width
                + column_count.saturating_sub(1) as f32 * options.gap
        };
        let visible_rows = item_count.min(rows_per_column);
        let content_height = if visible_rows == 0 {
            options.viewport_height.max(options.padding * 2.0)
        } else {
            options.padding * 2.0
                + visible_rows as f32 * options.item_height
                + visible_rows.saturating_sub(1) as f32 * options.gap
        }
        .max(options.viewport_height);

        Self {
            options,
            item_count,
            rows_per_column,
            content_size: ViewSize {
                width: content_width,
                height: content_height,
            },
        }
    }

    pub fn rows_per_column(&self) -> usize {
        self.rows_per_column
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

    pub fn horizontal_scroll_bar(
        &self,
        thickness: f32,
        min_handle_width: f32,
    ) -> Option<HorizontalScrollBarLayout> {
        let max_scroll_x = (self.content_size.width - self.options.viewport_width).max(0.0);
        if max_scroll_x <= 0.0 {
            return None;
        }
        let track_width = self.options.viewport_width.max(0.0);
        let handle_width = (self.options.viewport_width / self.content_size.width * track_width)
            .clamp(min_handle_width.min(track_width), track_width);
        let travel = (track_width - handle_width).max(0.0);
        let handle_x = if max_scroll_x <= 0.0 {
            0.0
        } else {
            self.options.scroll_x.clamp(0.0, max_scroll_x) / max_scroll_x * travel
        };
        let track_rect = ViewRect {
            x: 0.0,
            y: (self.options.viewport_height - thickness).max(0.0),
            width: track_width,
            height: thickness.max(0.0),
        };

        Some(HorizontalScrollBarLayout {
            track_rect,
            handle_rect: ViewRect {
                x: handle_x,
                y: track_rect.y,
                width: handle_width,
                height: track_rect.height,
            },
            max_scroll_x,
        })
    }

    pub fn item(&self, model_index: usize) -> Option<ItemLayout> {
        if model_index >= self.item_count {
            return None;
        }
        let column = model_index / self.rows_per_column;
        let row = model_index % self.rows_per_column;
        let x = self.options.padding + column as f32 * (self.options.item_width + self.options.gap);
        let y = self.options.padding + row as f32 * (self.options.item_height + self.options.gap);
        let item_rect = ViewRect {
            x,
            y,
            width: self.options.item_width,
            height: self.options.item_height,
        };
        let icon_rect = ViewRect {
            x: x + self.options.padding,
            y: y + (self.options.item_height - self.options.icon_size) / 2.0,
            width: self.options.icon_size,
            height: self.options.icon_size,
        };
        let text_x = icon_rect.right() + self.options.gap;
        let text_rect = ViewRect {
            x: text_x,
            y: y + (self.options.item_height - self.options.text_height) / 2.0,
            width: item_rect.right() - text_x - self.options.padding,
            height: self.options.text_height,
        };
        Some(ItemLayout {
            model_index,
            column,
            row,
            item_rect,
            icon_rect,
            text_rect,
        })
    }

    pub fn items(&self) -> impl Iterator<Item = ItemLayout> + '_ {
        (0..self.item_count).filter_map(|index| self.item(index))
    }

    pub fn visible_items(&self) -> impl Iterator<Item = ItemLayout> + '_ {
        let viewport = self.viewport_rect();
        self.items()
            .filter(move |item| item.item_rect.intersects(viewport))
    }

    pub fn hit_test_content_point(&self, point: ViewPoint) -> Option<usize> {
        self.items()
            .find(|item| item.item_rect.contains(point))
            .map(|item| item.model_index)
    }

    pub fn hit_test_viewport_point(&self, point: ViewPoint) -> Option<usize> {
        self.hit_test_content_point(ViewPoint {
            x: point.x + self.options.scroll_x,
            y: point.y + self.options.scroll_y,
        })
    }

    pub fn indexes_intersecting(&self, rect: ViewRect) -> RangeSelection {
        let indexes = self
            .items()
            .filter(|item| item.item_rect.intersects(rect))
            .map(|item| item.model_index)
            .collect();
        RangeSelection { indexes }
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
    let available = (options.viewport_height - options.reserved_bottom - options.padding * 2.0)
        .max(options.item_height);
    ((available + options.gap) / (options.item_height + options.gap))
        .floor()
        .max(1.0) as usize
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
            layout.hit_test_content_point(ViewPoint { x: 118.0, y: 8.0 }),
            Some(2)
        );
        assert_eq!(
            layout.hit_test_content_point(ViewPoint { x: 118.0, y: 68.0 }),
            Some(3)
        );
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
    fn horizontal_scroll_bar_tracks_scroll_position() {
        let layout = CompactLayout::new(
            20,
            CompactLayoutOptions {
                viewport_width: 200.0,
                viewport_height: 140.0,
                scroll_x: 100.0,
                item_width: 100.0,
                item_height: 50.0,
                gap: 0.0,
                padding: 0.0,
                ..CompactLayoutOptions::default()
            },
        );

        let bar = layout.horizontal_scroll_bar(10.0, 32.0).unwrap();
        assert_eq!(bar.track_rect.width, 200.0);
        assert_eq!(bar.track_rect.y, 130.0);
        assert!(bar.max_scroll_x > 0.0);
        assert!(bar.handle_rect.width >= 32.0);
        assert_eq!(
            bar.scroll_x_for_handle_x(bar.handle_rect.x),
            layout.viewport_rect().x
        );
    }

    #[test]
    fn horizontal_scroll_bar_is_hidden_when_content_fits() {
        let layout = CompactLayout::new(
            1,
            CompactLayoutOptions {
                viewport_width: 400.0,
                viewport_height: 140.0,
                item_width: 100.0,
                item_height: 50.0,
                ..CompactLayoutOptions::default()
            },
        );

        assert!(layout.horizontal_scroll_bar(10.0, 32.0).is_none());
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
}
