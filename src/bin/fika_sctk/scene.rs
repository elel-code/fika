use std::error::Error;
use std::path::PathBuf;

use fika_core::{
    CompactLayout, CompactLayoutOptions, Entry, IconsLayout, IconsLayoutOptions, ItemLayout,
    ViewMode, ViewPoint, ViewRect, format_modified_secs, format_size, read_entries_sync,
};

use super::metrics::{
    APP_TOOLBAR_HEIGHT, COMPACT_ICON_SIZE, COMPACT_ITEM_HEIGHT, COMPACT_ITEM_WIDTH,
    CONTENT_SCROLLBAR_MIN_THUMB_SIZE, CONTENT_SCROLLBAR_PADDING, CONTENT_SCROLLBAR_RESERVED_EXTENT,
    DETAILS_HEADER_HEIGHT, DETAILS_ICON_SIZE, DETAILS_ROW_HEIGHT, ICONS_ICON_SIZE,
    ICONS_ITEM_HEIGHT, ICONS_ITEM_WIDTH, PANE_MARGIN, PLACES_ICON_SIZE, PLACES_PANEL_MARGIN_BOTTOM,
    PLACES_PANEL_MARGIN_X, PLACES_ROW_HEIGHT, PLACES_TITLE_HEIGHT, PLACES_TO_PANE_GAP,
    PLACES_WIDTH, STATUS_BAR_HEIGHT, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT, TOP_BAR_HEIGHT,
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

pub(crate) struct SctkScene {
    path: PathBuf,
    view_mode: ViewMode,
    entries: Vec<Entry>,
    dir_count: usize,
    hover: Option<usize>,
    selected: Option<usize>,
    scroll_x: f32,
    scroll_y: f32,
    places_visible: bool,
}

impl SctkScene {
    pub(crate) fn load(path: PathBuf, view_mode: ViewMode) -> Result<Self, Box<dyn Error>> {
        let entries = read_entries_sync(&path)?;
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        Ok(Self {
            path,
            view_mode,
            entries,
            dir_count,
            hover: None,
            selected: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            places_visible: true,
        })
    }

    pub(crate) fn log_startup(&self) {
        eprintln!(
            "[fika-sctk] path={} view={} entries={} dirs={} files={}",
            self.path.display(),
            self.view_mode.as_str(),
            self.entries.len(),
            self.dir_count,
            self.file_count()
        );
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

    pub(crate) fn dir_count(&self) -> usize {
        self.dir_count
    }

    pub(crate) fn file_count(&self) -> usize {
        self.entries.len().saturating_sub(self.dir_count)
    }

    pub(crate) fn render_frame(&mut self, width: u32, height: u32) -> SceneFrame {
        self.clamp_scroll(width, height);
        let geometry = self.geometry(width, height);
        let mut batch = QuadBatch::default();
        let mut text = TextBatch::default();
        self.push_chrome(&mut batch, &mut text, geometry, width, height);
        let visible_items = match self.view_mode {
            ViewMode::Icons => {
                let layout = self.icons_layout(geometry.content);
                let items: Vec<_> = layout.visible_items().collect();
                let visible_items = items.len();
                for item in items {
                    self.push_item(
                        &mut batch,
                        &mut text,
                        &item,
                        geometry.content,
                        width,
                        height,
                    );
                }
                visible_items
            }
            ViewMode::Compact => {
                let layout = self.compact_layout(geometry.content);
                let items: Vec<_> = layout.visible_items().collect();
                let visible_items = items.len();
                for item in items {
                    self.push_item(
                        &mut batch,
                        &mut text,
                        &item,
                        geometry.content,
                        width,
                        height,
                    );
                }
                visible_items
            }
            ViewMode::Details => {
                self.push_details_items(&mut batch, &mut text, geometry.content, width, height)
            }
        };
        self.push_status_text(&mut text, geometry.pane, visible_items);
        self.push_content_scrollbar(&mut batch, geometry.content, width, height);

        let quads = batch.len();
        SceneFrame {
            batch,
            text,
            quads,
            visible_items,
            selected: self.selected,
            hover: self.hover,
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
        }
    }

    pub(crate) fn set_pointer(&mut self, point: ViewPoint, width: u32, height: u32) -> bool {
        let hit = self.hit_test(point, width, height);
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

    pub(crate) fn press_primary(&mut self, point: ViewPoint, width: u32, height: u32) -> bool {
        let hit = self.hit_test(point, width, height);
        if self.selected == hit {
            return false;
        }
        self.selected = hit;
        true
    }

    pub(crate) fn scroll(&mut self, delta_x: f32, delta_y: f32, width: u32, height: u32) -> bool {
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
        self.clamp_scroll(width, height);
        before != (self.scroll_x, self.scroll_y)
    }

    fn hit_test(&self, point: ViewPoint, width: u32, height: u32) -> Option<usize> {
        let geometry = self.geometry(width, height);
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
                .hit_test_content_point(content_point),
            ViewMode::Compact => self
                .compact_layout(geometry.content)
                .hit_test_content_point(content_point),
            ViewMode::Details => {
                if content_point.y < DETAILS_HEADER_HEIGHT {
                    return None;
                }
                let row = ((content_point.y - DETAILS_HEADER_HEIGHT) / DETAILS_ROW_HEIGHT).floor();
                let index = row.max(0.0) as usize;
                (index < self.entries.len()).then_some(index)
            }
        }
    }

    fn push_chrome(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        geometry: SceneGeometry,
        width: u32,
        height: u32,
    ) {
        let window = ViewRect {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
        };
        batch.push_rect(window, [0.91, 0.93, 0.95, 1.0], width, height);
        batch.push_rect(
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: APP_TOOLBAR_HEIGHT,
            },
            [0.83, 0.86, 0.89, 1.0],
            width,
            height,
        );

        if let Some(places) = geometry.places {
            batch.push_clipped_rounded_rect(
                places,
                window,
                8.0,
                [0.96, 0.97, 0.98, 1.0],
                width,
                height,
            );
            batch.push_rect(
                ViewRect {
                    x: places.x + 10.0,
                    y: places.y + PLACES_TITLE_HEIGHT + 2.0,
                    width: places.width - 20.0,
                    height: 1.0,
                },
                [0.82, 0.84, 0.86, 1.0],
                width,
                height,
            );
            text.push(
                "Places",
                ViewRect {
                    x: places.x + 14.0,
                    y: places.y + 7.0,
                    width: places.width - 28.0,
                    height: TEXT_LINE_HEIGHT,
                },
                places,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                TEXT_MUTED,
            );
            self.push_places_rows(batch, text, places, width, height);
        }

        batch.push_clipped_rounded_rect(
            geometry.pane,
            window,
            8.0,
            [0.975, 0.98, 0.985, 1.0],
            width,
            height,
        );
        let location_bar = ViewRect {
            x: geometry.pane.x + 12.0,
            y: geometry.pane.y + 5.0,
            width: (geometry.pane.width - 24.0).max(1.0),
            height: (TOP_BAR_HEIGHT - 10.0).max(1.0),
        };
        batch.push_clipped_rounded_rect(
            location_bar,
            window,
            6.0,
            [1.0, 1.0, 1.0, 1.0],
            width,
            height,
        );
        let location_icon = ViewRect {
            x: location_bar.x + 9.0,
            y: location_bar.y + (location_bar.height - 14.0) / 2.0,
            width: 14.0,
            height: 14.0,
        };
        self.push_folder_glyph(batch, location_icon, location_bar, width, height);
        text.push_no_wrap(
            self.path.display().to_string(),
            ViewRect {
                x: location_icon.right() + 8.0,
                y: location_bar.y + (location_bar.height - TEXT_LINE_HEIGHT) / 2.0,
                width: (location_bar.right() - location_icon.right() - 16.0).max(1.0),
                height: TEXT_LINE_HEIGHT,
            },
            location_bar,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            TEXT_PRIMARY,
        );
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

    fn push_places_rows(
        &self,
        batch: &mut QuadBatch,
        text: &mut TextBatch,
        places: ViewRect,
        width: u32,
        height: u32,
    ) {
        let clip = inset(places, 6.0);
        let rows = [
            ("Home", true),
            ("Desktop", false),
            ("Documents", false),
            ("Downloads", false),
            ("Trash", false),
            ("Root", self.path == PathBuf::from("/")),
        ];
        for (row, (label, active)) in rows.iter().enumerate() {
            let y = places.y + PLACES_TITLE_HEIGHT + 8.0 + row as f32 * PLACES_ROW_HEIGHT;
            let rect = ViewRect {
                x: places.x + 8.0,
                y,
                width: places.width - 16.0,
                height: PLACES_ROW_HEIGHT - 2.0,
            };
            if *active {
                batch.push_clipped_rounded_rect(
                    rect,
                    clip,
                    6.0,
                    [0.80, 0.88, 0.96, 1.0],
                    width,
                    height,
                );
            }
            let icon = ViewRect {
                x: rect.x + 8.0,
                y: rect.y + (rect.height - PLACES_ICON_SIZE) / 2.0,
                width: PLACES_ICON_SIZE,
                height: PLACES_ICON_SIZE,
            };
            batch.push_clipped_rounded_rect(
                icon,
                clip,
                5.0,
                [0.24, 0.45, 0.68, 1.0],
                width,
                height,
            );
            text.push_no_wrap(
                *label,
                ViewRect {
                    x: icon.right() + 9.0,
                    y: rect.y + (rect.height - TEXT_LINE_HEIGHT) / 2.0,
                    width: (rect.right() - icon.right() - 17.0).max(1.0),
                    height: TEXT_LINE_HEIGHT,
                },
                clip,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
                if *active { TEXT_PRIMARY } else { TEXT_MUTED },
            );
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
        let Some(entry) = self.entries.get(item.model_index) else {
            return;
        };
        let visual = self.to_screen_rect(item.visual_rect, content);
        let icon = self.to_screen_rect(item.icon_rect, content);
        let text_rect = self.to_screen_rect(item.text_rect, content);
        let selected = self.selected == Some(item.model_index);
        let hovered = self.hover == Some(item.model_index);
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
        for index in first..(first + rows).min(self.entries.len()) {
            let Some(entry) = self.entries.get(index) else {
                continue;
            };
            let y = content.y + DETAILS_HEADER_HEIGHT + index as f32 * DETAILS_ROW_HEIGHT
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
            if self.selected == Some(index) || self.hover == Some(index) {
                batch.push_clipped_rounded_rect(
                    inset(row_rect, 2.0),
                    content,
                    6.0,
                    if self.selected == Some(index) {
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
                item_text_color(entry, self.selected == Some(index), self.view_mode),
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

    fn push_folder_glyph(
        &self,
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

    fn push_content_scrollbar(
        &self,
        batch: &mut QuadBatch,
        content: ViewRect,
        width: u32,
        height: u32,
    ) {
        let (max_x, max_y) = self.scroll_bounds(content);
        if max_x <= 0.0 && max_y <= 0.0 {
            return;
        }
        let vertical = self.view_mode != ViewMode::Compact;
        if vertical {
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
            batch.push_clipped_rounded_rect(
                track,
                content,
                2.0,
                [0.78, 0.80, 0.82, 0.45],
                width,
                height,
            );
            batch.push_clipped_rounded_rect(
                ViewRect {
                    y,
                    height: thumb_h,
                    ..track
                },
                content,
                2.0,
                [0.48, 0.52, 0.56, 0.8],
                width,
                height,
            );
        } else {
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
            batch.push_clipped_rounded_rect(
                track,
                content,
                2.0,
                [0.78, 0.80, 0.82, 0.45],
                width,
                height,
            );
            batch.push_clipped_rounded_rect(
                ViewRect {
                    x,
                    width: thumb_w,
                    ..track
                },
                content,
                2.0,
                [0.48, 0.52, 0.56, 0.8],
                width,
                height,
            );
        }
    }

    fn to_screen_rect(&self, rect: ViewRect, content: ViewRect) -> ViewRect {
        ViewRect {
            x: content.x + rect.x - self.scroll_x,
            y: content.y + rect.y - self.scroll_y,
            width: rect.width,
            height: rect.height,
        }
    }

    fn icons_layout(&self, content: ViewRect) -> IconsLayout {
        IconsLayout::new(
            self.entries.len(),
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
            self.entries.len(),
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

    fn status_text(&self, visible_items: usize) -> String {
        let selected = self
            .selected
            .map(|_| ", 1 selected".to_string())
            .unwrap_or_default();
        format!(
            "{} items, {} folders, {} files, {} visible, {}{}",
            self.entries.len(),
            self.dir_count(),
            self.file_count(),
            visible_items,
            self.view_mode.as_str(),
            selected,
        )
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

    fn geometry(&self, width: u32, height: u32) -> SceneGeometry {
        let window_width = width as f32;
        let window_height = height as f32;
        let top = APP_TOOLBAR_HEIGHT + PANE_MARGIN;
        let bottom = PANE_MARGIN;
        let places = self.places_visible.then_some(ViewRect {
            x: PLACES_PANEL_MARGIN_X,
            y: top,
            width: PLACES_WIDTH,
            height: (window_height - top - PLACES_PANEL_MARGIN_BOTTOM).max(1.0),
        });
        let pane_x = places
            .map(|places| places.right() + PLACES_TO_PANE_GAP)
            .unwrap_or(PANE_MARGIN);
        let pane = ViewRect {
            x: pane_x,
            y: top,
            width: (window_width - pane_x - PANE_MARGIN).max(1.0),
            height: (window_height - top - bottom).max(1.0),
        };
        let content = ViewRect {
            x: pane.x,
            y: pane.y + TOP_BAR_HEIGHT,
            width: pane.width,
            height: (pane.height - TOP_BAR_HEIGHT - STATUS_BAR_HEIGHT).max(1.0),
        };
        SceneGeometry {
            places,
            pane,
            content,
        }
    }

    fn clamp_scroll(&mut self, width: u32, height: u32) {
        let geometry = self.geometry(width, height);
        let (max_x, max_y) = self.scroll_bounds(geometry.content);
        self.scroll_x = self.scroll_x.clamp(0.0, max_x);
        self.scroll_y = self.scroll_y.clamp(0.0, max_y);
    }

    fn scroll_bounds(&self, content: ViewRect) -> (f32, f32) {
        match self.view_mode {
            ViewMode::Icons => {
                let layout = self.icons_layout(content);
                let last = self
                    .entries
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
                    .entries
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
                    DETAILS_HEADER_HEIGHT + self.entries.len() as f32 * DETAILS_ROW_HEIGHT;
                (0.0, (content_height - content.height).max(0.0))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SceneGeometry {
    places: Option<ViewRect>,
    pane: ViewRect,
    content: ViewRect,
}

pub(crate) struct SceneFrame {
    pub(crate) batch: QuadBatch,
    pub(crate) text: TextBatch,
    pub(crate) quads: usize,
    pub(crate) visible_items: usize,
    pub(crate) selected: Option<usize>,
    pub(crate) hover: Option<usize>,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
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

    use fika_core::{Entry, EntryData};

    use crate::fika_sctk::metrics::SCROLL_LINE_PX;

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

    fn scene(view_mode: ViewMode, count: usize) -> SctkScene {
        let entries = (0..count)
            .map(|index| entry(&format!("item-{index}"), index % 5 == 0))
            .collect::<Vec<_>>();
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        SctkScene {
            path: PathBuf::from("/tmp"),
            view_mode,
            entries,
            dir_count,
            hover: None,
            selected: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            places_visible: true,
        }
    }

    #[test]
    fn renders_all_view_modes_as_real_quad_frames() {
        for mode in [ViewMode::Icons, ViewMode::Compact, ViewMode::Details] {
            let mut scene = scene(mode, 80);
            let frame = scene.render_frame(900, 640);
            assert!(
                frame.quads > 20,
                "{mode:?} should paint chrome and file items"
            );
            assert!(
                frame.visible_items > 0,
                "{mode:?} should project visible items"
            );
            assert!(frame.text.len() > 0, "{mode:?} should paint real labels");
        }
    }

    #[test]
    fn pointer_hit_selects_projected_item() {
        let mut scene = scene(ViewMode::Icons, 20);
        let geometry = scene.geometry(900, 640);
        let item = scene.icons_layout(geometry.content).item(0).unwrap();
        let point = ViewPoint {
            x: geometry.content.x + item.visual_rect.x + 4.0,
            y: geometry.content.y + item.visual_rect.y + 4.0,
        };

        assert!(scene.set_pointer(point, 900, 640));
        assert_eq!(scene.hover, Some(0));
        assert!(scene.press_primary(point, 900, 640));
        assert_eq!(scene.selected, Some(0));
    }

    #[test]
    fn scroll_is_clamped_per_view_mode_axis() {
        let mut icons = scene(ViewMode::Icons, 300);
        assert!(icons.scroll(0.0, SCROLL_LINE_PX * 20.0, 900, 480));
        assert!(icons.scroll_y > 0.0);
        assert_eq!(icons.scroll_x, 0.0);

        let mut compact = scene(ViewMode::Compact, 300);
        assert!(compact.scroll(0.0, SCROLL_LINE_PX * 20.0, 900, 480));
        assert!(compact.scroll_x > 0.0);
        assert_eq!(compact.scroll_y, 0.0);
    }
}
