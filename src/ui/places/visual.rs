use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    App, Bounds, Font, IntoElement, Pixels, SharedString, Styled, TextAlign, TextRun, WeakEntity,
    Window, canvas, fill, point, px, rgb, rgba, size,
};

use crate::FikaApp;

use super::perf::{
    PlacesRowTextShapeCachePerfLog, PlacesRowTextShapeCacheStats, PlacesRowVisualPerfLog,
    emit_places_row_text_shape_cache_perf_log, emit_places_row_visual_perf_log,
    places_perf_enabled,
};
use super::snapshot::PlaceSnapshot;
use super::style::{place_row_background, place_row_border_color};

pub(super) const PLACE_ROW_HEIGHT: f32 = 30.0;
pub(super) const PLACE_SECTION_HEADING_HEIGHT: f32 = 24.0;

const ROW_PADDING_X: f32 = 8.0;
const ICON_SIZE: f32 = 22.0;
const ICON_TEXT_GAP: f32 = 8.0;
const TRASH_DOT_SIZE: f32 = 7.0;
const INSERT_INDICATOR_HEIGHT: f32 = 2.0;

pub(super) fn places_row_visual_layer(
    places: Vec<PlaceSnapshot>,
    app: WeakEntity<FikaApp>,
) -> impl IntoElement {
    let rows = place_row_visual_layer_rows(&places);
    let height = places_row_visual_content_height(&places).max(1.0);
    canvas(
        move |_bounds, window, cx| {
            places_row_visual_prepaint(rows.clone(), app.clone(), window, cx)
        },
        move |bounds, paint_state, window, cx| {
            let paint_started = Instant::now();
            for row in &paint_state.rows {
                paint_place_row_visual(bounds, row, window, cx);
            }
            if places_perf_enabled() {
                emit_places_row_visual_perf_log(PlacesRowVisualPerfLog {
                    rows: paint_state.rows.len(),
                    prepaint_elapsed: paint_state.prepaint_elapsed,
                    paint_elapsed: paint_started.elapsed(),
                });
                if paint_state.shape_cache_stats.has_activity() {
                    emit_places_row_text_shape_cache_perf_log(PlacesRowTextShapeCachePerfLog {
                        stats: paint_state.shape_cache_stats,
                    });
                }
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .w_full()
    .h(px(height))
}

#[derive(Clone)]
struct PlaceRowVisualState {
    y: f32,
    label: SharedString,
    active: bool,
    mounted: bool,
    drop_target: bool,
    insert_before: bool,
    insert_after: bool,
    trash_place: bool,
    trash_has_items: bool,
}

impl PlaceRowVisualState {
    fn from_place(place: &PlaceSnapshot, y: f32) -> Self {
        let insert_target = place.insert_before || place.insert_after;
        Self {
            y,
            label: SharedString::from(place.label.as_str()),
            active: place.active && !insert_target,
            mounted: place.mounted,
            drop_target: place.drop_target && !insert_target,
            insert_before: place.insert_before,
            insert_after: place.insert_after,
            trash_place: place.trash_place,
            trash_has_items: place.trash_has_items,
        }
    }
}

struct PlaceRowVisualLayerPaintState {
    rows: Vec<PlaceRowVisualPaintState>,
    prepaint_elapsed: std::time::Duration,
    shape_cache_stats: PlacesRowTextShapeCacheStats,
}

struct PlaceRowVisualPaintState {
    input: PlaceRowVisualState,
    line: Arc<gpui::ShapedLine>,
    line_height: Pixels,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PlacesRowTextShapeCacheKey {
    label: SharedString,
    font: Font,
    font_size_bits: u32,
    text_color: u32,
}

#[derive(Default)]
pub(crate) struct PlacesRowTextShapeCache {
    entries: HashMap<PlacesRowTextShapeCacheKey, Arc<gpui::ShapedLine>>,
    stats: PlacesRowTextShapeCacheStats,
}

impl PlacesRowTextShapeCache {
    const MAX_ENTRIES: usize = 512;

    fn shape_for(
        &mut self,
        key: &PlacesRowTextShapeCacheKey,
        window: &mut Window,
    ) -> Arc<gpui::ShapedLine> {
        if let Some(line) = self.entries.get(key) {
            self.stats.hits += 1;
            return line.clone();
        }

        self.stats.misses += 1;
        if self.entries.len() >= Self::MAX_ENTRIES {
            self.stats.evicted += self.entries.len();
            self.entries.clear();
        }

        let line = Arc::new(shape_place_row_visual_text(key, window));
        self.entries.insert(key.clone(), line.clone());
        line
    }

    pub(crate) fn take_stats(&mut self) -> PlacesRowTextShapeCacheStats {
        let mut stats = std::mem::take(&mut self.stats);
        stats.entries = self.entries.len();
        stats
    }
}

fn places_row_visual_prepaint(
    rows: Vec<PlaceRowVisualState>,
    app: WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut App,
) -> PlaceRowVisualLayerPaintState {
    let started = Instant::now();
    let rows = rows
        .into_iter()
        .map(|input| place_row_visual_prepaint(input, &app, window, cx))
        .collect();
    let shape_cache_stats = app
        .update(cx, |this, _cx| this.place_row_text_shape_cache.take_stats())
        .ok()
        .unwrap_or_default();
    PlaceRowVisualLayerPaintState {
        rows,
        prepaint_elapsed: started.elapsed(),
        shape_cache_stats,
    }
}

fn place_row_visual_prepaint(
    input: PlaceRowVisualState,
    app: &WeakEntity<FikaApp>,
    window: &mut Window,
    cx: &mut App,
) -> PlaceRowVisualPaintState {
    let text_style = window.text_style();
    let font_size = px(window.rem_size().as_f32() * 0.875);
    let text_color = if input.active {
        0x1f4fbf
    } else if !input.mounted {
        0x6b7280
    } else {
        0x24292f
    };
    let key = PlacesRowTextShapeCacheKey {
        label: input.label.clone(),
        font: text_style.font(),
        font_size_bits: font_size.as_f32().to_bits(),
        text_color,
    };
    let line = app
        .update(cx, |this, _cx| {
            this.place_row_text_shape_cache.shape_for(&key, window)
        })
        .ok()
        .unwrap_or_else(|| Arc::new(shape_place_row_visual_text(&key, window)));
    PlaceRowVisualPaintState {
        input,
        line,
        line_height: px(20.0),
    }
}

fn shape_place_row_visual_text(
    key: &PlacesRowTextShapeCacheKey,
    window: &mut Window,
) -> gpui::ShapedLine {
    let run = TextRun {
        len: key.label.len(),
        font: key.font.clone(),
        color: rgb(key.text_color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        key.label.clone(),
        px(f32::from_bits(key.font_size_bits)),
        &[run],
        None,
    )
}

fn paint_place_row_visual(
    layer_bounds: Bounds<Pixels>,
    state: &PlaceRowVisualPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let input = &state.input;
    let row_bounds = Bounds::new(
        point(layer_bounds.origin.x, layer_bounds.origin.y + px(input.y)),
        size(layer_bounds.size.width, px(PLACE_ROW_HEIGHT)),
    );
    let background = place_row_background(input.active, input.drop_target);
    let border_color = place_row_border_color(input.active, input.drop_target);
    window.paint_quad(fill(row_bounds, background).corner_radii(px(6.0)));
    if input.active || input.drop_target {
        window.paint_quad(
            fill(row_bounds, rgba(0x00000000))
                .corner_radii(px(6.0))
                .border_widths(px(1.0))
                .border_color(border_color),
        );
    }

    let text_left = ROW_PADDING_X + ICON_SIZE + ICON_TEXT_GAP;
    let reserved_right = if input.trash_place {
        ROW_PADDING_X + TRASH_DOT_SIZE + ICON_TEXT_GAP
    } else {
        ROW_PADDING_X
    };
    let text_bounds = Bounds::new(
        point(row_bounds.origin.x + px(text_left), row_bounds.origin.y),
        size(
            (row_bounds.size.width - px(text_left + reserved_right)).max(px(1.0)),
            row_bounds.size.height,
        ),
    );
    let text_y = text_bounds.origin.y
        + ((row_bounds.size.height - state.line_height).max(px(0.0)) / 2.0).floor();
    window.paint_layer(text_bounds, |window| {
        state
            .line
            .paint(
                point(text_bounds.origin.x, text_y),
                state.line_height,
                TextAlign::Left,
                Some(text_bounds.size.width),
                window,
                cx,
            )
            .ok();
    });

    if input.trash_place {
        let dot_x =
            row_bounds.origin.x + row_bounds.size.width - px(ROW_PADDING_X + TRASH_DOT_SIZE);
        let dot_y = row_bounds.origin.y + (row_bounds.size.height - px(TRASH_DOT_SIZE)) / 2.0;
        window.paint_quad(
            fill(
                Bounds::new(
                    point(dot_x, dot_y),
                    size(px(TRASH_DOT_SIZE), px(TRASH_DOT_SIZE)),
                ),
                if input.trash_has_items {
                    rgb(0x2f6fed)
                } else {
                    rgb(0xc8ced6)
                },
            )
            .corner_radii(px(TRASH_DOT_SIZE / 2.0)),
        );
    }

    if input.insert_before || input.insert_after {
        let y = if input.insert_before {
            row_bounds.origin.y
        } else {
            row_bounds.origin.y + row_bounds.size.height - px(INSERT_INDICATOR_HEIGHT)
        };
        window.paint_quad(
            fill(
                Bounds::new(
                    point(row_bounds.origin.x + px(ROW_PADDING_X), y),
                    size(
                        (row_bounds.size.width - px(ROW_PADDING_X * 2.0)).max(px(1.0)),
                        px(INSERT_INDICATOR_HEIGHT),
                    ),
                ),
                rgb(0xd97706),
            )
            .corner_radii(px(1.0)),
        );
    }
}

fn place_row_visual_layer_rows(places: &[PlaceSnapshot]) -> Vec<PlaceRowVisualState> {
    let mut rows = Vec::with_capacity(places.len());
    let mut current_group = None;
    let mut y = 0.0;
    for place in places {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                y += PLACE_SECTION_HEADING_HEIGHT;
            }
        }
        rows.push(PlaceRowVisualState::from_place(place, y));
        y += PLACE_ROW_HEIGHT;
    }
    rows
}

pub(super) fn places_row_visual_content_height(places: &[PlaceSnapshot]) -> f32 {
    let mut current_group = None;
    let mut height = 0.0;
    for place in places {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                height += PLACE_SECTION_HEADING_HEIGHT;
            }
        }
        height += PLACE_ROW_HEIGHT;
    }
    height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icons::FileIconSnapshot;
    use std::path::PathBuf;

    #[test]
    fn place_row_visual_suppresses_ordinary_highlight_on_insert_target() {
        let mut place = test_place();
        place.active = true;
        place.drop_target = true;
        place.insert_before = true;
        let state = PlaceRowVisualState::from_place(&place, 0.0);
        assert!(!state.active);
        assert!(!state.drop_target);
        assert!(state.insert_before);
    }

    #[test]
    fn place_row_visual_keeps_trash_marker_state() {
        let mut place = test_place();
        place.trash_place = true;
        place.trash_has_items = true;
        let state = PlaceRowVisualState::from_place(&place, 0.0);
        assert!(state.trash_place);
        assert!(state.trash_has_items);
    }

    #[test]
    fn place_row_visual_layer_offsets_skip_section_headings() {
        let mut first = test_place();
        first.group = "";
        first.label = "Home".to_string();
        let mut second = test_place();
        second.group = "Devices";
        second.label = "Root".to_string();
        let rows = place_row_visual_layer_rows(&[first, second]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].y, 0.0);
        assert_eq!(rows[1].y, PLACE_ROW_HEIGHT + PLACE_SECTION_HEADING_HEIGHT);
    }

    #[test]
    fn places_row_visual_content_height_matches_rows_and_sections() {
        let mut first = test_place();
        first.group = "";
        let mut second = test_place();
        second.group = "Devices";

        assert_eq!(
            places_row_visual_content_height(&[first, second]),
            PLACE_ROW_HEIGHT * 2.0 + PLACE_SECTION_HEADING_HEIGHT
        );
    }

    fn test_place() -> PlaceSnapshot {
        PlaceSnapshot {
            index: 0,
            group: "",
            icon: FileIconSnapshot {
                icon_name: "folder".into(),
                path: None,
                fallback_marker: "F".into(),
                fallback_fg: 0x1f4fbf,
                fallback_bg: 0xeaf1ff,
            },
            label: "Home".to_string(),
            path: PathBuf::from("/home/test"),
            device_id: None,
            mounted: true,
            device: false,
            network: false,
            device_ejectable: false,
            device_can_power_off: false,
            active: false,
            drop_target: false,
            insert_before: false,
            insert_after: false,
            trash_place: false,
            trash_has_items: false,
            editable: true,
            removable: true,
        }
    }
}
