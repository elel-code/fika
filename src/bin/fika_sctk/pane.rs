use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use fika_core::{
    CompactLayout, CompactLayoutOptions, Entry, IconsLayout, IconsLayoutOptions, ItemLayout,
    ViewMode, ViewPoint, ViewRect, format_modified_secs, format_size, read_entries_sync,
};

use super::metrics::{
    COMPACT_ICON_SIZE, COMPACT_ITEM_HEIGHT, COMPACT_ITEM_WIDTH, CONTENT_SCROLLBAR_MIN_THUMB_SIZE,
    CONTENT_SCROLLBAR_PADDING, CONTENT_SCROLLBAR_RESERVED_EXTENT, DETAILS_HEADER_HEIGHT,
    DETAILS_ICON_SIZE, DETAILS_ROW_HEIGHT, ICONS_ICON_SIZE, ICONS_ITEM_HEIGHT, ICONS_ITEM_WIDTH,
    STATUS_BAR_HEIGHT, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT, TOP_BAR_HEIGHT,
};
use super::quad::{QuadBatch, inset};
use super::text::TextBatch;

const DETAIL_NAME_COLUMN_WIDTH: f32 = 360.0;
const DETAIL_SIZE_COLUMN_WIDTH: f32 = 110.0;
const DETAIL_MODIFIED_COLUMN_WIDTH: f32 = 180.0;
const TEXT_PRIMARY: [u8; 4] = [36, 41, 47, 255];
const TEXT_MUTED: [u8; 4] = [89, 99, 110, 255];
const TEXT_SELECTED: [u8; 4] = [255, 255, 255, 255];
const TEXT_DIRECTORY: [u8; 4] = [31, 79, 191, 255];

pub(crate) struct SctkPane {
    path: PathBuf,
    view_mode: ViewMode,
    entries: Vec<Entry>,
    visible_indices: Vec<usize>,
    dir_count: usize,
    show_hidden: bool,
    hover: Option<usize>,
    selected: Option<usize>,
    selected_entries: BTreeSet<usize>,
    location_active: bool,
    location_text: String,
    location_cursor: usize,
    scroll_x: f32,
    scroll_y: f32,
}

impl SctkPane {
    pub(crate) fn load(path: PathBuf, view_mode: ViewMode) -> Result<Self, Box<dyn Error>> {
        let entries = read_entries_sync(&path)?;
        Ok(Self::from_entries(path, view_mode, entries))
    }

    pub(crate) fn from_entries(path: PathBuf, view_mode: ViewMode, entries: Vec<Entry>) -> Self {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let location_text = path.display().to_string();
        let location_cursor = location_text.len();
        let mut pane = Self {
            path,
            view_mode,
            entries,
            visible_indices: Vec::new(),
            dir_count,
            show_hidden: false,
            hover: None,
            selected: None,
            selected_entries: BTreeSet::new(),
            location_active: false,
            location_text,
            location_cursor,
            scroll_x: 0.0,
            scroll_y: 0.0,
        };
        pane.rebuild_visible_indices();
        pane
    }

    pub(crate) fn path(&self) -> &PathBuf {
        &self.path
    }

    pub(crate) fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn visible_entry_count(&self) -> usize {
        self.visible_indices.len()
    }

    pub(crate) fn dir_count(&self) -> usize {
        self.dir_count
    }

    pub(crate) fn file_count(&self) -> usize {
        self.entries.len().saturating_sub(self.dir_count)
    }

    pub(crate) fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub(crate) fn selected_count(&self) -> usize {
        self.selected_entries.len()
    }

    pub(crate) fn location_active(&self) -> bool {
        self.location_active
    }

    #[cfg(test)]
    pub(crate) fn hover(&self) -> Option<usize> {
        self.hover
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    #[cfg(test)]
    pub(crate) fn location_text(&self) -> &str {
        &self.location_text
    }

    #[cfg(test)]
    pub(crate) fn location_cursor(&self) -> usize {
        self.location_cursor
    }

    pub(crate) fn render(
        &mut self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        geometry: PaneGeometry,
        window_clip: ViewRect,
        active: bool,
        width: u32,
        height: u32,
    ) -> PaneRenderStats {
        self.clamp_scroll(geometry.content);
        self.push_chrome(batch, text, geometry, window_clip, active, width, height);
        let visible_items = match self.view_mode {
            ViewMode::Icons => {
                let layout = self.icons_layout(geometry.content);
                let items: Vec<_> = layout.visible_items().collect();
                let visible_items = items.len();
                for item in items {
                    self.push_item(batch, text, &item, geometry.content, width, height);
                }
                visible_items
            }
            ViewMode::Compact => {
                let layout = self.compact_layout(geometry.content);
                let items: Vec<_> = layout.visible_items().collect();
                let visible_items = items.len();
                for item in items {
                    self.push_item(batch, text, &item, geometry.content, width, height);
                }
                visible_items
            }
            ViewMode::Details => {
                self.push_details_items(batch, text, geometry.content, width, height)
            }
        };
        self.push_status_text(text, geometry.pane, visible_items);
        self.push_content_scrollbar(batch, geometry.content, width, height);

        PaneRenderStats {
            visible_items,
            selected: self.selected,
            selected_count: self.selected_count(),
            hover: self.hover,
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
        }
    }

    pub(crate) fn set_pointer(&mut self, point: ViewPoint, geometry: PaneGeometry) -> bool {
        let hit = self.hit_test(point, geometry);
        if self.hover == hit {
            return false;
        }
        self.hover = hit;
        true
    }

    pub(crate) fn clear_pointer(&mut self) -> bool {
        let changed = self.hover.is_some();
        self.hover = None;
        changed
    }

    pub(crate) fn press_primary(&mut self, point: ViewPoint, geometry: PaneGeometry) -> bool {
        let hit = self.hit_test(point, geometry);
        self.replace_selection(hit)
    }

    pub(crate) fn focus_location(&mut self) -> bool {
        let before = (
            self.location_active,
            self.location_text.clone(),
            self.location_cursor,
        );
        self.location_active = true;
        self.location_text = self.path.display().to_string();
        self.location_cursor = self.location_text.len();
        before
            != (
                self.location_active,
                self.location_text.clone(),
                self.location_cursor,
            )
    }

    pub(crate) fn focus_location_if_hit(
        &mut self,
        point: ViewPoint,
        geometry: PaneGeometry,
    ) -> Option<bool> {
        if !location_bar_rect(geometry.pane).contains(point) {
            return None;
        }
        let before = (
            self.location_active,
            self.location_text.clone(),
            self.location_cursor,
        );
        if !self.location_active {
            self.location_text = self.path.display().to_string();
        }
        self.location_active = true;
        self.location_cursor = cursor_for_location_point(point, geometry.pane, &self.location_text);
        Some(
            before
                != (
                    self.location_active,
                    self.location_text.clone(),
                    self.location_cursor,
                ),
        )
    }

    pub(crate) fn cancel_location_edit(&mut self) -> bool {
        if !self.location_active {
            return false;
        }
        self.location_active = false;
        self.sync_location_text();
        true
    }

    pub(crate) fn edit_location(&mut self, edit: LocationEdit) -> Result<bool, Box<dyn Error>> {
        if !self.location_active {
            return Ok(false);
        }
        let before = (
            self.location_active,
            self.location_text.clone(),
            self.location_cursor,
            self.path.clone(),
        );
        self.location_cursor = clamp_to_char_boundary(&self.location_text, self.location_cursor);
        match edit {
            LocationEdit::Insert(text) => {
                let text = text
                    .chars()
                    .filter(|character| !character.is_control())
                    .collect::<String>();
                if !text.is_empty() {
                    self.location_text.insert_str(self.location_cursor, &text);
                    self.location_cursor += text.len();
                }
            }
            LocationEdit::Backspace => {
                if let Some(previous) =
                    previous_char_boundary(&self.location_text, self.location_cursor)
                {
                    self.location_text.drain(previous..self.location_cursor);
                    self.location_cursor = previous;
                }
            }
            LocationEdit::Delete => {
                if let Some(next) = next_char_boundary(&self.location_text, self.location_cursor) {
                    self.location_text.drain(self.location_cursor..next);
                }
            }
            LocationEdit::MoveLeft => {
                if let Some(previous) =
                    previous_char_boundary(&self.location_text, self.location_cursor)
                {
                    self.location_cursor = previous;
                }
            }
            LocationEdit::MoveRight => {
                if let Some(next) = next_char_boundary(&self.location_text, self.location_cursor) {
                    self.location_cursor = next;
                }
            }
            LocationEdit::MoveHome => self.location_cursor = 0,
            LocationEdit::MoveEnd => self.location_cursor = self.location_text.len(),
            LocationEdit::Commit => {
                let target = PathBuf::from(self.location_text.trim());
                self.load_path(target)?;
                self.location_active = false;
            }
            LocationEdit::Cancel => {
                self.location_active = false;
                self.sync_location_text();
            }
        }
        Ok(before
            != (
                self.location_active,
                self.location_text.clone(),
                self.location_cursor,
                self.path.clone(),
            ))
    }

    pub(crate) fn set_view_mode(&mut self, view_mode: ViewMode, geometry: PaneGeometry) -> bool {
        if self.view_mode == view_mode {
            return false;
        }
        self.view_mode = view_mode;
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        if let Some(selected) = self.selected {
            self.ensure_entry_visible(selected, geometry.content);
        }
        self.clamp_scroll(geometry.content);
        true
    }

    pub(crate) fn toggle_show_hidden(&mut self, geometry: PaneGeometry) -> bool {
        self.show_hidden = !self.show_hidden;
        self.rebuild_visible_indices();
        self.prune_hidden_state();
        self.clamp_scroll(geometry.content);
        true
    }

    pub(crate) fn set_show_hidden(&mut self, show_hidden: bool) -> bool {
        if self.show_hidden == show_hidden {
            return false;
        }
        self.show_hidden = show_hidden;
        self.rebuild_visible_indices();
        self.prune_hidden_state();
        true
    }

    pub(crate) fn begin_rubber_band(
        &mut self,
        point: ViewPoint,
        geometry: PaneGeometry,
    ) -> Option<bool> {
        if !geometry.content.contains(point) || self.hit_test(point, geometry).is_some() {
            return None;
        }
        Some(self.clear_selection())
    }

    pub(crate) fn update_rubber_band_selection(
        &mut self,
        origin: ViewPoint,
        current: ViewPoint,
        geometry: PaneGeometry,
    ) -> bool {
        let band = clipped_band_rect(origin, current, geometry.content);
        let mut selected_entries = BTreeSet::new();
        if band.width >= 1.0 && band.height >= 1.0 {
            match self.view_mode {
                ViewMode::Icons => {
                    let layout = self.icons_layout(geometry.content);
                    for item in layout.visible_items() {
                        let Some(index) = self.entry_index_for_visible(item.model_index) else {
                            continue;
                        };
                        let rect = self.to_screen_rect(item.visual_rect, geometry.content);
                        if rects_intersect(rect, band) {
                            selected_entries.insert(index);
                        }
                    }
                }
                ViewMode::Compact => {
                    let layout = self.compact_layout(geometry.content);
                    for item in layout.visible_items() {
                        let Some(index) = self.entry_index_for_visible(item.model_index) else {
                            continue;
                        };
                        let rect = self.to_screen_rect(item.visual_rect, geometry.content);
                        if rects_intersect(rect, band) {
                            selected_entries.insert(index);
                        }
                    }
                }
                ViewMode::Details => {
                    let first = ((self.scroll_y - DETAILS_HEADER_HEIGHT).max(0.0)
                        / DETAILS_ROW_HEIGHT)
                        .floor() as usize;
                    let rows = ((geometry.content.height + DETAILS_ROW_HEIGHT - 1.0)
                        / DETAILS_ROW_HEIGHT) as usize
                        + 2;
                    for visible_index in first..(first + rows).min(self.visible_indices.len()) {
                        let Some(index) = self.entry_index_for_visible(visible_index) else {
                            continue;
                        };
                        let y = geometry.content.y
                            + DETAILS_HEADER_HEIGHT
                            + visible_index as f32 * DETAILS_ROW_HEIGHT
                            - self.scroll_y;
                        let rect = ViewRect {
                            x: geometry.content.x,
                            y,
                            width: geometry.content.width - CONTENT_SCROLLBAR_RESERVED_EXTENT,
                            height: DETAILS_ROW_HEIGHT,
                        };
                        if rects_intersect(rect, band) {
                            selected_entries.insert(index);
                        }
                    }
                }
            }
        }
        self.set_selection_set(selected_entries)
    }

    pub(crate) fn select_all(&mut self) -> bool {
        self.set_selection_set(self.visible_indices.iter().copied().collect())
    }

    pub(crate) fn move_selection(
        &mut self,
        movement: PaneSelectionMove,
        geometry: PaneGeometry,
    ) -> bool {
        let Some(target) = self.selection_target(movement, geometry.content) else {
            return self.clear_selection();
        };
        let Some(entry_index) = self.visible_indices.get(target).copied() else {
            return false;
        };
        if self.selected == Some(entry_index) {
            self.ensure_entry_visible(entry_index, geometry.content);
            return false;
        }
        self.replace_selection(Some(entry_index));
        self.ensure_entry_visible(entry_index, geometry.content);
        true
    }

    pub(crate) fn clear_selection(&mut self) -> bool {
        let changed = self.selected.is_some() || !self.selected_entries.is_empty();
        self.selected = None;
        self.selected_entries.clear();
        changed
    }

    pub(crate) fn activate_selected(&mut self) -> Result<bool, Box<dyn Error>> {
        let Some(index) = self.selected else {
            return Ok(false);
        };
        let Some(entry) = self.entries.get(index) else {
            return Ok(false);
        };
        if !entry.is_dir {
            return Ok(false);
        }
        let path = entry
            .target_path
            .clone()
            .unwrap_or_else(|| self.path.join(entry.name.as_ref()));
        self.load_path(path)?;
        Ok(true)
    }

    pub(crate) fn reload(&mut self) -> Result<bool, Box<dyn Error>> {
        let selected_name = self
            .selected
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.name.clone());
        let selected_names = self
            .selected_entries
            .iter()
            .filter_map(|index| self.entries.get(*index))
            .map(|entry| entry.name.to_string())
            .collect::<BTreeSet<_>>();
        let entries = read_entries_sync(&self.path)?;
        self.entries = entries;
        self.dir_count = self.entries.iter().filter(|entry| entry.is_dir).count();
        self.rebuild_visible_indices();
        let selected_entries = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                selected_names
                    .contains(entry.name.as_ref())
                    .then_some(index)
            })
            .collect::<BTreeSet<_>>();
        self.set_selection_set(selected_entries);
        self.selected = selected_name
            .as_deref()
            .and_then(|name| {
                self.entries
                    .iter()
                    .position(|entry| entry.name.as_ref() == name)
            })
            .filter(|index| self.selected_entries.contains(index))
            .or_else(|| self.selected_entries.iter().next().copied());
        self.hover = None;
        Ok(true)
    }

    pub(crate) fn scroll(&mut self, delta_x: f32, delta_y: f32, geometry: PaneGeometry) -> bool {
        let before = (self.scroll_x, self.scroll_y);
        match self.view_mode {
            ViewMode::Compact => {
                self.scroll_x += delta_x + delta_y;
            }
            ViewMode::Icons | ViewMode::Details => {
                self.scroll_x += delta_x;
                self.scroll_y += delta_y;
            }
        }
        self.clamp_scroll(geometry.content);
        before != (self.scroll_x, self.scroll_y)
    }

    pub(crate) fn begin_scrollbar_drag(
        &mut self,
        point: ViewPoint,
        geometry: PaneGeometry,
    ) -> Option<(PaneScrollbarDrag, bool)> {
        let metrics = self.scrollbar_metrics(geometry.content)?;
        if !metrics.hit_rect.contains(point) {
            return None;
        }
        let point_axis = metrics.axis.point_axis(point);
        let thumb_start = metrics.axis.rect_start(metrics.thumb);
        let thumb_extent = metrics.axis.rect_extent(metrics.thumb);
        let pointer_offset = if metrics.thumb.contains(point) {
            point_axis - thumb_start
        } else {
            thumb_extent / 2.0
        };
        let drag = PaneScrollbarDrag {
            axis: metrics.axis,
            pointer_offset,
        };
        let changed = self.drag_scrollbar(point, geometry, drag);
        Some((drag, changed))
    }

    pub(crate) fn drag_scrollbar(
        &mut self,
        point: ViewPoint,
        geometry: PaneGeometry,
        drag: PaneScrollbarDrag,
    ) -> bool {
        let Some(metrics) = self.scrollbar_metrics(geometry.content) else {
            return false;
        };
        if metrics.axis != drag.axis {
            return false;
        }
        let before = (self.scroll_x, self.scroll_y);
        let track_start = metrics.axis.rect_start(metrics.track);
        let track_extent = metrics.axis.rect_extent(metrics.track);
        let thumb_extent = metrics.axis.rect_extent(metrics.thumb);
        let travel = (track_extent - thumb_extent).max(1.0);
        let raw = metrics.axis.point_axis(point) - track_start - drag.pointer_offset;
        let ratio = (raw / travel).clamp(0.0, 1.0);
        match drag.axis {
            PaneScrollbarAxis::Vertical => self.scroll_y = ratio * metrics.max_scroll,
            PaneScrollbarAxis::Horizontal => self.scroll_x = ratio * metrics.max_scroll,
        }
        self.clamp_scroll(geometry.content);
        before != (self.scroll_x, self.scroll_y)
    }

    pub(crate) fn icons_layout(&self, content: ViewRect) -> IconsLayout {
        IconsLayout::new(
            self.visible_indices.len(),
            IconsLayoutOptions {
                viewport_width: (content.width - CONTENT_SCROLLBAR_RESERVED_EXTENT).max(1.0),
                viewport_height: content.height.max(1.0),
                scroll_x: self.scroll_x,
                scroll_y: self.scroll_y,
                padding: 6.0,
                gap: 10.0,
                item_width: ICONS_ITEM_WIDTH,
                item_height: ICONS_ITEM_HEIGHT,
                icon_size: ICONS_ICON_SIZE,
                text_height: 18.0,
                ..IconsLayoutOptions::default()
            },
        )
    }

    fn compact_layout(&self, content: ViewRect) -> CompactLayout {
        CompactLayout::new(
            self.visible_indices.len(),
            CompactLayoutOptions {
                viewport_width: content.width.max(1.0),
                viewport_height: (content.height - CONTENT_SCROLLBAR_RESERVED_EXTENT).max(1.0),
                scroll_x: self.scroll_x,
                scroll_y: self.scroll_y,
                padding: 6.0,
                side_padding: 8.0,
                gap: 6.0,
                text_gap: 8.0,
                item_width: COMPACT_ITEM_WIDTH,
                item_height: COMPACT_ITEM_HEIGHT,
                icon_size: COMPACT_ICON_SIZE,
                text_height: 18.0,
                ..CompactLayoutOptions::default()
            },
        )
    }

    fn hit_test(&self, point: ViewPoint, geometry: PaneGeometry) -> Option<usize> {
        if !geometry.content.contains(point) {
            return None;
        }
        let content_point = ViewPoint {
            x: point.x - geometry.content.x + self.scroll_x,
            y: point.y - geometry.content.y + self.scroll_y,
        };
        match self.view_mode {
            ViewMode::Icons => self
                .icons_layout(geometry.content)
                .hit_test_content_point(content_point)
                .and_then(|visible| self.entry_index_for_visible(visible)),
            ViewMode::Compact => self
                .compact_layout(geometry.content)
                .hit_test_content_point(content_point)
                .and_then(|visible| self.entry_index_for_visible(visible)),
            ViewMode::Details => {
                if content_point.y < DETAILS_HEADER_HEIGHT {
                    return None;
                }
                let row = ((content_point.y - DETAILS_HEADER_HEIGHT) / DETAILS_ROW_HEIGHT).floor();
                let visible = row.max(0.0) as usize;
                self.entry_index_for_visible(visible)
            }
        }
    }

    fn push_chrome(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        geometry: PaneGeometry,
        window_clip: ViewRect,
        active: bool,
        width: u32,
        height: u32,
    ) {
        batch.push_clipped_rounded_rect(
            geometry.pane,
            window_clip,
            8.0,
            [0.975, 0.98, 0.985, 1.0],
            width,
            height,
        );
        if active {
            push_focus_ring(batch, geometry.pane, window_clip, width, height);
        }
        let location_bar = location_bar_rect(geometry.pane);
        batch.push_clipped_rounded_rect(
            location_bar,
            window_clip,
            6.0,
            if self.location_active {
                [0.985, 0.995, 1.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            },
            width,
            height,
        );
        if self.location_active {
            batch.push_clipped_rounded_rect(
                inset(location_bar, 1.0),
                window_clip,
                5.0,
                [0.22, 0.49, 0.82, 0.10],
                width,
                height,
            );
        }
        let location_icon = ViewRect {
            x: location_bar.x + 9.0,
            y: location_bar.y + (location_bar.height - 14.0) / 2.0,
            width: 14.0,
            height: 14.0,
        };
        push_folder_glyph(batch, location_icon, location_bar, width, height);
        let location_text = if self.location_active {
            self.location_text.clone()
        } else {
            self.path.display().to_string()
        };
        let text_rect = location_text_rect(geometry.pane);
        text.push_no_wrap(
            location_text,
            text_rect,
            location_bar,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            TEXT_PRIMARY,
        );
        if self.location_active {
            let cursor_x = (text_rect.x
                + cursor_visual_advance(&self.location_text, self.location_cursor))
            .clamp(text_rect.x, text_rect.right() - 1.0);
            batch.push_clipped_rect(
                ViewRect {
                    x: cursor_x,
                    y: text_rect.y + 2.0,
                    width: 1.0,
                    height: (text_rect.height - 4.0).max(1.0),
                },
                location_bar,
                [0.12, 0.30, 0.62, 1.0],
                width,
                height,
            );
        }
        batch.push_rect(
            ViewRect {
                x: geometry.pane.x,
                y: geometry.pane.y + TOP_BAR_HEIGHT,
                width: geometry.pane.width,
                height: 1.0,
            },
            [0.82, 0.84, 0.86, 1.0],
            width,
            height,
        );
        batch.push_rect(
            ViewRect {
                x: geometry.pane.x,
                y: geometry.pane.bottom() - STATUS_BAR_HEIGHT,
                width: geometry.pane.width,
                height: 1.0,
            },
            [0.82, 0.84, 0.86, 1.0],
            width,
            height,
        );
        batch.push_rect(
            ViewRect {
                x: geometry.pane.x,
                y: geometry.pane.bottom() - STATUS_BAR_HEIGHT,
                width: geometry.pane.width,
                height: STATUS_BAR_HEIGHT,
            },
            [0.94, 0.955, 0.965, 1.0],
            width,
            height,
        );
        if self.view_mode == ViewMode::Details {
            batch.push_rect(
                ViewRect {
                    x: geometry.content.x,
                    y: geometry.content.y,
                    width: geometry.content.width,
                    height: DETAILS_HEADER_HEIGHT,
                },
                [0.90, 0.92, 0.94, 1.0],
                width,
                height,
            );
            self.push_details_header_text(text, geometry.content);
        }
    }

    fn push_item(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        item: &ItemLayout,
        content: ViewRect,
        width: u32,
        height: u32,
    ) {
        let Some(entry_index) = self.entry_index_for_visible(item.model_index) else {
            return;
        };
        let Some(entry) = self.entries.get(entry_index) else {
            return;
        };
        let visual = self.to_screen_rect(item.visual_rect, content);
        let icon = self.to_screen_rect(item.icon_rect, content);
        let text_rect = self.to_screen_rect(item.text_rect, content);
        let selected = self.selected_entries.contains(&entry_index);
        let hovered = self.hover == Some(entry_index);
        if selected || hovered {
            batch.push_clipped_rounded_rect(
                visual,
                content,
                7.0,
                if selected {
                    [0.23, 0.50, 0.84, 0.92]
                } else {
                    [0.74, 0.82, 0.90, 0.72]
                },
                width,
                height,
            );
        }

        self.push_icon(batch, icon, content, entry.is_dir, width, height);
        let color = item_text_color(entry, selected, self.view_mode);
        match self.view_mode {
            ViewMode::Icons => text.push_centered(
                entry.name.as_ref(),
                text_rect,
                content,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                color,
            ),
            ViewMode::Compact => text.push_no_wrap(
                entry.name.as_ref(),
                text_rect,
                content,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                color,
            ),
            ViewMode::Details => {}
        }
    }

    fn push_details_items(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        content: ViewRect,
        width: u32,
        height: u32,
    ) -> usize {
        let first = ((self.scroll_y - DETAILS_HEADER_HEIGHT).max(0.0) / DETAILS_ROW_HEIGHT).floor()
            as usize;
        let rows = ((content.height + DETAILS_ROW_HEIGHT - 1.0) / DETAILS_ROW_HEIGHT) as usize + 2;
        let mut visible = 0usize;
        for visible_index in first..(first + rows).min(self.visible_indices.len()) {
            let Some(index) = self.entry_index_for_visible(visible_index) else {
                continue;
            };
            let Some(entry) = self.entries.get(index) else {
                continue;
            };
            let y = content.y + DETAILS_HEADER_HEIGHT + visible_index as f32 * DETAILS_ROW_HEIGHT
                - self.scroll_y;
            let row_rect = ViewRect {
                x: content.x,
                y,
                width: content.width - CONTENT_SCROLLBAR_RESERVED_EXTENT,
                height: DETAILS_ROW_HEIGHT,
            };
            if row_rect.bottom() < content.y || row_rect.y > content.bottom() {
                continue;
            }
            visible += 1;
            let selected = self.selected_entries.contains(&index);
            if selected || self.hover == Some(index) {
                batch.push_clipped_rounded_rect(
                    inset(row_rect, 2.0),
                    content,
                    6.0,
                    if selected {
                        [0.23, 0.50, 0.84, 0.92]
                    } else {
                        [0.74, 0.82, 0.90, 0.72]
                    },
                    width,
                    height,
                );
            }
            let icon = ViewRect {
                x: row_rect.x + 8.0,
                y: row_rect.y + (row_rect.height - DETAILS_ICON_SIZE) / 2.0,
                width: DETAILS_ICON_SIZE,
                height: DETAILS_ICON_SIZE,
            };
            self.push_icon(batch, icon, content, entry.is_dir, width, height);
            let name_rect = ViewRect {
                x: icon.right() + 8.0,
                y: row_rect.y + (row_rect.height - TEXT_LINE_HEIGHT) / 2.0,
                width: (DETAIL_NAME_COLUMN_WIDTH - icon.width - 24.0).max(1.0),
                height: TEXT_LINE_HEIGHT,
            };
            text.push_no_wrap(
                entry.name.as_ref(),
                name_rect,
                content,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                item_text_color(entry, selected, self.view_mode),
            );
            let metadata_y = row_rect.y + (row_rect.height - TEXT_LINE_HEIGHT) / 2.0;
            text.push_no_wrap(
                details_size_label(entry),
                ViewRect {
                    x: content.x + DETAIL_NAME_COLUMN_WIDTH + 8.0,
                    y: metadata_y,
                    width: DETAIL_SIZE_COLUMN_WIDTH - 16.0,
                    height: TEXT_LINE_HEIGHT,
                },
                content,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                TEXT_MUTED,
            );
            text.push_no_wrap(
                format_modified_secs(entry.modified_secs),
                ViewRect {
                    x: content.x + DETAIL_NAME_COLUMN_WIDTH + DETAIL_SIZE_COLUMN_WIDTH + 8.0,
                    y: metadata_y,
                    width: DETAIL_MODIFIED_COLUMN_WIDTH - 16.0,
                    height: TEXT_LINE_HEIGHT,
                },
                content,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                TEXT_MUTED,
            );
        }
        visible
    }

    fn push_icon(
        &self,
        batch: &mut QuadBatch,
        icon: ViewRect,
        clip: ViewRect,
        is_dir: bool,
        width: u32,
        height: u32,
    ) {
        let color = if is_dir {
            [0.21, 0.49, 0.78, 1.0]
        } else {
            [0.72, 0.76, 0.80, 1.0]
        };
        if is_dir {
            batch.push_clipped_rounded_rect(
                ViewRect {
                    x: icon.x + icon.width * 0.08,
                    y: icon.y + icon.height * 0.18,
                    width: icon.width * 0.45,
                    height: icon.height * 0.22,
                },
                clip,
                3.0,
                color,
                width,
                height,
            );
            batch.push_clipped_rounded_rect(
                ViewRect {
                    x: icon.x,
                    y: icon.y + icon.height * 0.30,
                    width: icon.width,
                    height: icon.height * 0.58,
                },
                clip,
                5.0,
                color,
                width,
                height,
            );
        } else {
            batch.push_clipped_rounded_rect(icon, clip, 5.0, color, width, height);
            batch.push_clipped_rect(
                ViewRect {
                    x: icon.x + icon.width * 0.64,
                    y: icon.y,
                    width: icon.width * 0.28,
                    height: icon.height * 0.28,
                },
                clip,
                [0.90, 0.92, 0.94, 1.0],
                width,
                height,
            );
        }
    }

    fn push_content_scrollbar(
        &self,
        batch: &mut QuadBatch,
        content: ViewRect,
        width: u32,
        height: u32,
    ) {
        let Some(metrics) = self.scrollbar_metrics(content) else {
            return;
        };
        batch.push_clipped_rounded_rect(
            metrics.track,
            content,
            2.0,
            [0.78, 0.80, 0.82, 0.45],
            width,
            height,
        );
        batch.push_clipped_rounded_rect(
            metrics.thumb,
            content,
            2.0,
            [0.48, 0.52, 0.56, 0.8],
            width,
            height,
        );
    }

    fn push_details_header_text(&self, text: &mut TextBatch, content: ViewRect) {
        let y = content.y + (DETAILS_HEADER_HEIGHT - TEXT_LINE_HEIGHT) / 2.0;
        text.push_no_wrap(
            "Name",
            ViewRect {
                x: content.x + 34.0,
                y,
                width: DETAIL_NAME_COLUMN_WIDTH - 42.0,
                height: TEXT_LINE_HEIGHT,
            },
            content,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            TEXT_MUTED,
        );
        text.push_no_wrap(
            "Size",
            ViewRect {
                x: content.x + DETAIL_NAME_COLUMN_WIDTH + 8.0,
                y,
                width: DETAIL_SIZE_COLUMN_WIDTH - 16.0,
                height: TEXT_LINE_HEIGHT,
            },
            content,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            TEXT_MUTED,
        );
        text.push_no_wrap(
            "Modified",
            ViewRect {
                x: content.x + DETAIL_NAME_COLUMN_WIDTH + DETAIL_SIZE_COLUMN_WIDTH + 8.0,
                y,
                width: DETAIL_MODIFIED_COLUMN_WIDTH - 16.0,
                height: TEXT_LINE_HEIGHT,
            },
            content,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            TEXT_MUTED,
        );
    }

    fn push_status_text(&self, text: &mut TextBatch, pane: ViewRect, visible_items: usize) {
        text.push_no_wrap(
            self.status_text(visible_items),
            ViewRect {
                x: pane.x + 12.0,
                y: pane.bottom() - STATUS_BAR_HEIGHT + (STATUS_BAR_HEIGHT - TEXT_LINE_HEIGHT) / 2.0,
                width: (pane.width - 24.0).max(1.0),
                height: TEXT_LINE_HEIGHT,
            },
            pane,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            TEXT_MUTED,
        );
    }

    fn status_text(&self, visible_items: usize) -> String {
        let selected_count = self.selected_count();
        let selected = if selected_count == 0 {
            String::new()
        } else {
            format!(", {selected_count} selected")
        };
        let visible_dirs = self
            .visible_indices
            .iter()
            .filter(|index| self.entries.get(**index).is_some_and(|entry| entry.is_dir))
            .count();
        let visible_files = self.visible_indices.len().saturating_sub(visible_dirs);
        let hidden = if self.show_hidden {
            "hidden shown"
        } else {
            "hidden hidden"
        };
        format!(
            "{} items, {} folders, {} files, {} on screen, {}, {}{}",
            self.visible_indices.len(),
            visible_dirs,
            visible_files,
            visible_items,
            self.view_mode.as_str(),
            hidden,
            selected,
        )
    }

    fn to_screen_rect(&self, rect: ViewRect, content: ViewRect) -> ViewRect {
        ViewRect {
            x: content.x + rect.x - self.scroll_x,
            y: content.y + rect.y - self.scroll_y,
            width: rect.width,
            height: rect.height,
        }
    }

    fn clamp_scroll(&mut self, content: ViewRect) {
        let (max_x, max_y) = self.scroll_bounds(content);
        self.scroll_x = self.scroll_x.clamp(0.0, max_x);
        self.scroll_y = self.scroll_y.clamp(0.0, max_y);
    }

    fn scroll_bounds(&self, content: ViewRect) -> (f32, f32) {
        match self.view_mode {
            ViewMode::Icons => {
                let layout = self.icons_layout(content);
                let last = self
                    .visible_indices
                    .len()
                    .checked_sub(1)
                    .and_then(|index| layout.item(index));
                let max_y = last
                    .map(|item| item.item_rect.bottom() - content.height)
                    .unwrap_or(0.0)
                    .max(0.0);
                (0.0, max_y)
            }
            ViewMode::Compact => {
                let layout = self.compact_layout(content);
                let last = self
                    .visible_indices
                    .len()
                    .checked_sub(1)
                    .and_then(|index| layout.item(index));
                let max_x = last
                    .map(|item| item.item_rect.right() - content.width)
                    .unwrap_or(0.0)
                    .max(0.0);
                (max_x, 0.0)
            }
            ViewMode::Details => {
                let content_height =
                    DETAILS_HEADER_HEIGHT + self.visible_indices.len() as f32 * DETAILS_ROW_HEIGHT;
                (0.0, (content_height - content.height).max(0.0))
            }
        }
    }

    fn scrollbar_metrics(&self, content: ViewRect) -> Option<ScrollbarMetrics> {
        let (max_x, max_y) = self.scroll_bounds(content);
        if self.view_mode == ViewMode::Compact {
            if max_x <= 0.0 {
                return None;
            }
            let track = ViewRect {
                x: content.x + CONTENT_SCROLLBAR_PADDING,
                y: content.bottom() - CONTENT_SCROLLBAR_RESERVED_EXTENT + CONTENT_SCROLLBAR_PADDING,
                width: content.width - CONTENT_SCROLLBAR_PADDING * 2.0,
                height: 4.0,
            };
            let ratio = (content.width / (content.width + max_x)).clamp(0.05, 1.0);
            let thumb_w = (track.width * ratio).max(CONTENT_SCROLLBAR_MIN_THUMB_SIZE);
            let travel = (track.width - thumb_w).max(1.0);
            let x = track.x + travel * (self.scroll_x / max_x.max(1.0)).clamp(0.0, 1.0);
            Some(ScrollbarMetrics {
                axis: PaneScrollbarAxis::Horizontal,
                track,
                thumb: ViewRect {
                    x,
                    width: thumb_w,
                    ..track
                },
                hit_rect: ViewRect {
                    x: content.x,
                    y: content.bottom() - CONTENT_SCROLLBAR_RESERVED_EXTENT,
                    width: content.width,
                    height: CONTENT_SCROLLBAR_RESERVED_EXTENT,
                },
                max_scroll: max_x,
            })
        } else {
            if max_y <= 0.0 {
                return None;
            }
            let track = ViewRect {
                x: content.right() - CONTENT_SCROLLBAR_RESERVED_EXTENT + CONTENT_SCROLLBAR_PADDING,
                y: content.y + CONTENT_SCROLLBAR_PADDING,
                width: 4.0,
                height: content.height - CONTENT_SCROLLBAR_PADDING * 2.0,
            };
            let ratio = (content.height / (content.height + max_y)).clamp(0.05, 1.0);
            let thumb_h = (track.height * ratio).max(CONTENT_SCROLLBAR_MIN_THUMB_SIZE);
            let travel = (track.height - thumb_h).max(1.0);
            let y = track.y + travel * (self.scroll_y / max_y.max(1.0)).clamp(0.0, 1.0);
            Some(ScrollbarMetrics {
                axis: PaneScrollbarAxis::Vertical,
                track,
                thumb: ViewRect {
                    y,
                    height: thumb_h,
                    ..track
                },
                hit_rect: ViewRect {
                    x: content.right() - CONTENT_SCROLLBAR_RESERVED_EXTENT,
                    y: content.y,
                    width: CONTENT_SCROLLBAR_RESERVED_EXTENT,
                    height: content.height,
                },
                max_scroll: max_y,
            })
        }
    }

    fn load_path(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        let entries = read_entries_sync(&path)?;
        self.path = path;
        self.entries = entries;
        self.dir_count = self.entries.iter().filter(|entry| entry.is_dir).count();
        self.rebuild_visible_indices();
        self.hover = None;
        self.selected = None;
        self.selected_entries.clear();
        self.location_active = false;
        self.sync_location_text();
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
        Ok(())
    }

    fn sync_location_text(&mut self) {
        self.location_text = self.path.display().to_string();
        self.location_cursor = self.location_text.len();
    }

    fn rebuild_visible_indices(&mut self) {
        self.visible_indices.clear();
        self.visible_indices
            .extend(
                self.entries
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entry)| {
                        (self.show_hidden || !entry.name.starts_with('.')).then_some(index)
                    }),
            );
    }

    fn prune_hidden_state(&mut self) {
        let visible_indices = &self.visible_indices;
        self.selected_entries
            .retain(|index| visible_indices.contains(index));
        if self
            .selected
            .is_some_and(|index| !self.selected_entries.contains(&index))
        {
            self.selected = self.selected_entries.iter().next().copied();
        }
        if self
            .hover
            .is_some_and(|index| !self.visible_indices.contains(&index))
        {
            self.hover = None;
        }
    }

    fn replace_selection(&mut self, selected: Option<usize>) -> bool {
        self.set_selection_set(selected.into_iter().collect())
    }

    fn set_selection_set(&mut self, mut selected_entries: BTreeSet<usize>) -> bool {
        let visible_indices = &self.visible_indices;
        selected_entries.retain(|index| visible_indices.contains(index));
        let selected = self
            .selected
            .filter(|index| selected_entries.contains(index))
            .or_else(|| selected_entries.iter().next().copied());
        let changed = self.selected != selected || self.selected_entries != selected_entries;
        self.selected = selected;
        self.selected_entries = selected_entries;
        changed
    }

    fn entry_index_for_visible(&self, visible_index: usize) -> Option<usize> {
        self.visible_indices.get(visible_index).copied()
    }

    fn selected_visible_index(&self) -> Option<usize> {
        let selected = self.selected?;
        self.visible_indices
            .iter()
            .position(|entry_index| *entry_index == selected)
    }

    fn selection_target(&self, movement: PaneSelectionMove, content: ViewRect) -> Option<usize> {
        let len = self.visible_indices.len();
        if len == 0 {
            return None;
        }
        let Some(current) = self.selected_visible_index() else {
            return Some(match movement {
                PaneSelectionMove::Last => len - 1,
                _ => 0,
            });
        };
        let target = match movement {
            PaneSelectionMove::First => 0,
            PaneSelectionMove::Last => len - 1,
            PaneSelectionMove::Left => current.saturating_sub(self.horizontal_step(content)),
            PaneSelectionMove::Right => current.saturating_add(self.horizontal_step(content)),
            PaneSelectionMove::Up => current.saturating_sub(self.vertical_step(content)),
            PaneSelectionMove::Down => current.saturating_add(self.vertical_step(content)),
            PaneSelectionMove::PageUp => current.saturating_sub(self.page_step(content)),
            PaneSelectionMove::PageDown => current.saturating_add(self.page_step(content)),
        };
        Some(target.min(len - 1))
    }

    fn horizontal_step(&self, content: ViewRect) -> usize {
        match self.view_mode {
            ViewMode::Icons | ViewMode::Details => 1,
            ViewMode::Compact => self.compact_layout(content).rows_per_column().max(1),
        }
    }

    fn vertical_step(&self, content: ViewRect) -> usize {
        match self.view_mode {
            ViewMode::Icons => self.icons_layout(content).columns_per_row().max(1),
            ViewMode::Compact | ViewMode::Details => 1,
        }
    }

    fn page_step(&self, content: ViewRect) -> usize {
        match self.view_mode {
            ViewMode::Icons => {
                let layout = self.icons_layout(content);
                let rows = (content.height / ICONS_ITEM_HEIGHT).ceil().max(1.0) as usize;
                layout.columns_per_row().max(1) * rows
            }
            ViewMode::Compact => self.compact_layout(content).rows_per_column().max(1),
            ViewMode::Details => ((content.height - DETAILS_HEADER_HEIGHT).max(DETAILS_ROW_HEIGHT)
                / DETAILS_ROW_HEIGHT)
                .floor()
                .max(1.0) as usize,
        }
    }

    fn ensure_entry_visible(&mut self, entry_index: usize, content: ViewRect) {
        let Some(visible_index) = self
            .visible_indices
            .iter()
            .position(|index| *index == entry_index)
        else {
            return;
        };
        match self.view_mode {
            ViewMode::Icons => {
                if let Some(item) = self.icons_layout(content).item(visible_index) {
                    self.ensure_rect_visible(item.item_rect, content, true, false);
                }
            }
            ViewMode::Compact => {
                if let Some(item) = self.compact_layout(content).item(visible_index) {
                    self.ensure_rect_visible(item.item_rect, content, false, true);
                }
            }
            ViewMode::Details => {
                let top = DETAILS_HEADER_HEIGHT + visible_index as f32 * DETAILS_ROW_HEIGHT;
                let bottom = top + DETAILS_ROW_HEIGHT;
                if top - self.scroll_y < DETAILS_HEADER_HEIGHT {
                    self.scroll_y = (top - DETAILS_HEADER_HEIGHT).max(0.0);
                } else if bottom - self.scroll_y > content.height {
                    self.scroll_y = (bottom - content.height).max(0.0);
                }
            }
        }
        self.clamp_scroll(content);
    }

    fn ensure_rect_visible(
        &mut self,
        rect: ViewRect,
        content: ViewRect,
        vertical: bool,
        horizontal: bool,
    ) {
        if vertical {
            if rect.y < self.scroll_y {
                self.scroll_y = rect.y.max(0.0);
            } else if rect.bottom() > self.scroll_y + content.height {
                self.scroll_y = (rect.bottom() - content.height).max(0.0);
            }
        }
        if horizontal {
            if rect.x < self.scroll_x {
                self.scroll_x = rect.x.max(0.0);
            } else if rect.right() > self.scroll_x + content.width {
                self.scroll_x = (rect.right() - content.width).max(0.0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaneGeometry {
    pub(crate) pane: ViewRect,
    pub(crate) content: ViewRect,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PaneRenderStats {
    pub(crate) visible_items: usize,
    pub(crate) selected: Option<usize>,
    pub(crate) selected_count: usize,
    pub(crate) hover: Option<usize>,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneSelectionMove {
    Left,
    Right,
    Up,
    Down,
    First,
    Last,
    PageUp,
    PageDown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LocationEdit {
    Insert(String),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    Commit,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneScrollbarAxis {
    Horizontal,
    Vertical,
}

impl PaneScrollbarAxis {
    fn point_axis(self, point: ViewPoint) -> f32 {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }

    fn rect_start(self, rect: ViewRect) -> f32 {
        match self {
            Self::Horizontal => rect.x,
            Self::Vertical => rect.y,
        }
    }

    fn rect_extent(self, rect: ViewRect) -> f32 {
        match self {
            Self::Horizontal => rect.width,
            Self::Vertical => rect.height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneScrollbarDrag {
    axis: PaneScrollbarAxis,
    pointer_offset: f32,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarMetrics {
    axis: PaneScrollbarAxis,
    track: ViewRect,
    thumb: ViewRect,
    hit_rect: ViewRect,
    max_scroll: f32,
}

fn push_folder_glyph(
    batch: &mut QuadBatch,
    icon: ViewRect,
    clip: ViewRect,
    width: u32,
    height: u32,
) {
    batch.push_clipped_rounded_rect(
        ViewRect {
            x: icon.x + icon.width * 0.10,
            y: icon.y + icon.height * 0.18,
            width: icon.width * 0.44,
            height: icon.height * 0.24,
        },
        clip,
        2.0,
        [0.96, 0.70, 0.26, 1.0],
        width,
        height,
    );
    batch.push_clipped_rounded_rect(
        ViewRect {
            x: icon.x,
            y: icon.y + icon.height * 0.32,
            width: icon.width,
            height: icon.height * 0.58,
        },
        clip,
        3.0,
        [0.90, 0.58, 0.18, 1.0],
        width,
        height,
    );
}

fn push_focus_ring(batch: &mut QuadBatch, pane: ViewRect, clip: ViewRect, width: u32, height: u32) {
    batch.push_clipped_rounded_rect(
        ViewRect {
            x: pane.x + 10.0,
            y: pane.y + 1.0,
            width: (pane.width - 20.0).max(1.0),
            height: 2.0,
        },
        clip,
        1.0,
        [0.22, 0.49, 0.82, 0.65],
        width,
        height,
    );
}

fn location_bar_rect(pane: ViewRect) -> ViewRect {
    ViewRect {
        x: pane.x + 12.0,
        y: pane.y + 5.0,
        width: (pane.width - 24.0).max(1.0),
        height: (TOP_BAR_HEIGHT - 10.0).max(1.0),
    }
}

fn location_text_rect(pane: ViewRect) -> ViewRect {
    let bar = location_bar_rect(pane);
    let icon_right = bar.x + 9.0 + 14.0;
    ViewRect {
        x: icon_right + 8.0,
        y: bar.y + (bar.height - TEXT_LINE_HEIGHT) / 2.0,
        width: (bar.right() - icon_right - 16.0).max(1.0),
        height: TEXT_LINE_HEIGHT,
    }
}

fn cursor_for_location_point(point: ViewPoint, pane: ViewRect, text: &str) -> usize {
    let text_rect = location_text_rect(pane);
    let average_char_width = location_average_char_width();
    let target = ((point.x - text_rect.x).max(0.0) / average_char_width).round() as usize;
    byte_index_after_chars(text, target)
}

fn cursor_visual_advance(text: &str, cursor: usize) -> f32 {
    let cursor = clamp_to_char_boundary(text, cursor);
    text[..cursor].chars().count() as f32 * location_average_char_width()
}

fn location_average_char_width() -> f32 {
    TEXT_FONT_SIZE * 0.56
}

fn byte_index_after_chars(text: &str, count: usize) -> usize {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .nth(count)
        .unwrap_or(text.len())
}

fn clamp_to_char_boundary(text: &str, mut cursor: usize) -> usize {
    cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn previous_char_boundary(text: &str, cursor: usize) -> Option<usize> {
    let cursor = clamp_to_char_boundary(text, cursor);
    text[..cursor].char_indices().last().map(|(index, _)| index)
}

fn next_char_boundary(text: &str, cursor: usize) -> Option<usize> {
    let cursor = clamp_to_char_boundary(text, cursor);
    text[cursor..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| cursor + offset)
        .or_else(|| (cursor < text.len()).then_some(text.len()))
}

fn clipped_band_rect(origin: ViewPoint, current: ViewPoint, clip: ViewRect) -> ViewRect {
    let left = origin.x.min(current.x).clamp(clip.x, clip.right());
    let right = origin.x.max(current.x).clamp(clip.x, clip.right());
    let top = origin.y.min(current.y).clamp(clip.y, clip.bottom());
    let bottom = origin.y.max(current.y).clamp(clip.y, clip.bottom());
    ViewRect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

fn rects_intersect(a: ViewRect, b: ViewRect) -> bool {
    a.width > 0.0 && a.height > 0.0 && b.width > 0.0 && b.height > 0.0 && a.intersects(b)
}

fn item_text_color(entry: &Entry, selected: bool, view_mode: ViewMode) -> [u8; 4] {
    if selected {
        TEXT_SELECTED
    } else if view_mode != ViewMode::Details && entry.is_dir {
        TEXT_DIRECTORY
    } else {
        TEXT_PRIMARY
    }
}

fn details_size_label(entry: &Entry) -> String {
    if entry.is_dir {
        "Folder".to_string()
    } else if !entry.metadata_complete && entry.size_bytes == 0 && entry.modified_secs.is_none() {
        "-".to_string()
    } else {
        format_size(entry.size_bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use fika_core::EntryData;

    use super::*;

    fn entry(name: &str, is_dir: bool) -> Entry {
        Entry::new(EntryData {
            name: Arc::from(name),
            name_width_units: name.len().min(u16::MAX as usize) as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: None,
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn geometry() -> PaneGeometry {
        PaneGeometry {
            pane: ViewRect {
                x: 0.0,
                y: 0.0,
                width: 480.0,
                height: 360.0,
            },
            content: ViewRect {
                x: 0.0,
                y: TOP_BAR_HEIGHT,
                width: 480.0,
                height: 300.0,
            },
        }
    }

    #[test]
    fn hidden_entries_are_filtered_by_default_and_toggled_per_pane() {
        let mut pane = SctkPane::from_entries(
            PathBuf::from("/tmp"),
            ViewMode::Details,
            vec![
                entry("alpha", false),
                entry(".secret", false),
                entry("beta", false),
            ],
        );

        assert!(!pane.show_hidden());
        assert_eq!(pane.entry_count(), 3);
        assert_eq!(pane.visible_entry_count(), 2);

        assert!(pane.toggle_show_hidden(geometry()));
        assert!(pane.show_hidden());
        assert_eq!(pane.visible_entry_count(), 3);

        pane.replace_selection(Some(1));
        assert!(pane.toggle_show_hidden(geometry()));
        assert!(!pane.show_hidden());
        assert_eq!(pane.visible_entry_count(), 2);
        assert_eq!(pane.selected(), None);
        assert_eq!(pane.selected_count(), 0);
    }

    #[test]
    fn keyboard_selection_moves_over_visible_projection() {
        let mut pane = SctkPane::from_entries(
            PathBuf::from("/tmp"),
            ViewMode::Details,
            vec![
                entry("alpha", false),
                entry(".secret", false),
                entry("beta", false),
            ],
        );

        assert!(pane.move_selection(PaneSelectionMove::Down, geometry()));
        assert_eq!(pane.selected(), Some(0));
        assert!(pane.move_selection(PaneSelectionMove::Down, geometry()));
        assert_eq!(pane.selected(), Some(2));
    }

    #[test]
    fn select_all_selects_visible_entries_only() {
        let mut pane = SctkPane::from_entries(
            PathBuf::from("/tmp"),
            ViewMode::Details,
            vec![
                entry("alpha", false),
                entry(".secret", false),
                entry("beta", false),
            ],
        );

        assert!(pane.select_all());
        assert_eq!(pane.selected(), Some(0));
        assert_eq!(pane.selected_count(), 2);
        assert!(pane.selected_entries.contains(&0));
        assert!(!pane.selected_entries.contains(&1));
        assert!(pane.selected_entries.contains(&2));
    }

    #[test]
    fn hiding_entries_prunes_selection_set() {
        let mut pane = SctkPane::from_entries(
            PathBuf::from("/tmp"),
            ViewMode::Details,
            vec![
                entry("alpha", false),
                entry(".secret", false),
                entry("beta", false),
            ],
        );

        assert!(pane.toggle_show_hidden(geometry()));
        assert!(pane.select_all());
        assert_eq!(pane.selected_count(), 3);
        assert!(pane.toggle_show_hidden(geometry()));
        assert_eq!(pane.selected_count(), 2);
        assert!(!pane.selected_entries.contains(&1));
    }

    #[test]
    fn rubber_band_selects_projected_details_rows() {
        let mut pane = SctkPane::from_entries(
            PathBuf::from("/tmp"),
            ViewMode::Details,
            vec![
                entry("alpha", false),
                entry("beta", false),
                entry("gamma", false),
                entry("delta", false),
            ],
        );
        let geometry = geometry();
        let row_top = geometry.content.y + DETAILS_HEADER_HEIGHT;

        assert!(pane.update_rubber_band_selection(
            ViewPoint {
                x: geometry.content.x + 8.0,
                y: row_top + 1.0,
            },
            ViewPoint {
                x: geometry.content.x + 240.0,
                y: row_top + DETAILS_ROW_HEIGHT * 2.0 - 1.0,
            },
            geometry,
        ));
        assert_eq!(pane.selected_count(), 2);
        assert_eq!(pane.selected(), Some(0));
        assert!(pane.selected_entries.contains(&0));
        assert!(pane.selected_entries.contains(&1));
        assert!(!pane.selected_entries.contains(&2));
    }

    #[test]
    fn activating_selected_directory_loads_that_path() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("fika-sctk-pane-{stamp}"));
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("nested.txt"), b"nested").unwrap();

        let mut pane = SctkPane::load(root.clone(), ViewMode::Details).unwrap();
        assert!(pane.move_selection(PaneSelectionMove::Down, geometry()));
        assert!(pane.activate_selected().unwrap());
        assert_eq!(pane.path(), &child);
        assert_eq!(pane.visible_entry_count(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn location_edit_inserts_and_deletes_at_utf8_boundaries() {
        let mut pane = SctkPane::from_entries(PathBuf::from("/tmp"), ViewMode::Details, vec![]);

        assert!(pane.focus_location());
        assert!(
            pane.edit_location(LocationEdit::Insert("/目录".to_string()))
                .unwrap()
        );
        assert!(pane.location_text().ends_with("/目录"));
        assert_eq!(pane.location_cursor(), pane.location_text().len());
        assert!(pane.edit_location(LocationEdit::Backspace).unwrap());
        assert!(pane.location_text().ends_with("/目"));
        assert!(
            pane.location_text()
                .is_char_boundary(pane.location_cursor())
        );
    }

    #[test]
    fn location_cancel_restores_current_path() {
        let mut pane = SctkPane::from_entries(PathBuf::from("/tmp"), ViewMode::Details, vec![]);

        assert!(pane.focus_location());
        assert!(
            pane.edit_location(LocationEdit::Insert("/bad".to_string()))
                .unwrap()
        );
        assert!(pane.cancel_location_edit());
        assert!(!pane.location_active());
        assert_eq!(pane.location_text(), "/tmp");
    }

    #[test]
    fn location_commit_loads_target_directory() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("fika-sctk-location-{stamp}"));
        let child = root.join("child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(child.join("nested.txt"), b"nested").unwrap();

        let mut pane = SctkPane::load(root.clone(), ViewMode::Details).unwrap();
        assert!(pane.focus_location());
        pane.location_text = child.display().to_string();
        pane.location_cursor = pane.location_text.len();
        assert!(pane.edit_location(LocationEdit::Commit).unwrap());
        assert_eq!(pane.path(), &child);
        assert!(!pane.location_active());
        assert_eq!(pane.location_text(), child.display().to_string());
        assert_eq!(pane.visible_entry_count(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn vertical_scrollbar_drag_updates_scroll_y() {
        let entries = (0..200)
            .map(|index| entry(&format!("item-{index}"), false))
            .collect::<Vec<_>>();
        let mut pane = SctkPane::from_entries(PathBuf::from("/tmp"), ViewMode::Details, entries);
        let geometry = geometry();
        let press = ViewPoint {
            x: geometry.content.right() - 2.0,
            y: geometry.content.y + 80.0,
        };
        let (drag, _) = pane
            .begin_scrollbar_drag(press, geometry)
            .expect("scrollbar drag");

        assert!(pane.drag_scrollbar(
            ViewPoint {
                x: press.x,
                y: geometry.content.bottom() - 20.0,
            },
            geometry,
            drag,
        ));
        assert!(pane.scroll_y > 0.0);
    }

    #[test]
    fn compact_scrollbar_drag_updates_scroll_x() {
        let entries = (0..200)
            .map(|index| entry(&format!("item-{index}"), false))
            .collect::<Vec<_>>();
        let mut pane = SctkPane::from_entries(PathBuf::from("/tmp"), ViewMode::Compact, entries);
        let geometry = geometry();
        let press = ViewPoint {
            x: geometry.content.x + 120.0,
            y: geometry.content.bottom() - 2.0,
        };
        let (drag, _) = pane
            .begin_scrollbar_drag(press, geometry)
            .expect("scrollbar drag");

        assert!(pane.drag_scrollbar(
            ViewPoint {
                x: geometry.content.right() - 20.0,
                y: press.y,
            },
            geometry,
            drag,
        ));
        assert!(pane.scroll_x > 0.0);
    }
}
