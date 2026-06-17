use std::time::Instant;

use gpui::{
    App, Bounds, IntoElement, Pixels, SharedString, Styled, TextAlign, TextRun, Window, canvas,
    fill, point, px, rgb, rgba, size,
};

use super::perf::{PlacesRowVisualPerfLog, emit_places_row_visual_perf_log, places_perf_enabled};
use super::snapshot::PlaceSnapshot;
use super::style::{place_row_background, place_row_border_color};

const ROW_PADDING_X: f32 = 8.0;
const ICON_SIZE: f32 = 22.0;
const ICON_TEXT_GAP: f32 = 8.0;
const TRASH_DOT_SIZE: f32 = 7.0;
const INSERT_INDICATOR_HEIGHT: f32 = 2.0;

pub(super) fn place_row_visual(
    place: &PlaceSnapshot,
    active: bool,
    drop_target: bool,
) -> impl IntoElement {
    let state = PlaceRowVisualState::from_place(place, active, drop_target);
    canvas(
        move |bounds, window, cx| place_row_visual_prepaint(bounds, state.clone(), window, cx),
        move |bounds, paint_state, window, cx| {
            let paint_started = Instant::now();
            paint_place_row_visual(bounds, &paint_state, window, cx);
            if places_perf_enabled() {
                emit_places_row_visual_perf_log(PlacesRowVisualPerfLog {
                    rows: 1,
                    prepaint_elapsed: paint_state.prepaint_elapsed,
                    paint_elapsed: paint_started.elapsed(),
                });
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .size_full()
}

#[derive(Clone)]
struct PlaceRowVisualState {
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
    fn from_place(place: &PlaceSnapshot, active: bool, drop_target: bool) -> Self {
        Self {
            label: SharedString::from(place.label.as_str()),
            active,
            mounted: place.mounted,
            drop_target,
            insert_before: place.insert_before,
            insert_after: place.insert_after,
            trash_place: place.trash_place,
            trash_has_items: place.trash_has_items,
        }
    }
}

struct PlaceRowVisualPaintState {
    input: PlaceRowVisualState,
    line: gpui::ShapedLine,
    line_height: Pixels,
    prepaint_elapsed: std::time::Duration,
}

fn place_row_visual_prepaint(
    _bounds: Bounds<Pixels>,
    input: PlaceRowVisualState,
    window: &mut Window,
    _cx: &mut App,
) -> PlaceRowVisualPaintState {
    let started = Instant::now();
    let text_style = window.text_style();
    let font_size = px(window.rem_size().as_f32() * 0.875);
    let text_color = if input.active {
        0x1f4fbf
    } else if !input.mounted {
        0x6b7280
    } else {
        0x24292f
    };
    let run = TextRun {
        len: input.label.len(),
        font: text_style.font(),
        color: rgb(text_color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(input.label.clone(), font_size, &[run], None);
    PlaceRowVisualPaintState {
        input,
        line,
        line_height: px(20.0),
        prepaint_elapsed: started.elapsed(),
    }
}

fn paint_place_row_visual(
    bounds: Bounds<Pixels>,
    state: &PlaceRowVisualPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let input = &state.input;
    let background = place_row_background(input.active, input.drop_target);
    let border_color = place_row_border_color(input.active, input.drop_target);
    window.paint_quad(fill(bounds, background).corner_radii(px(6.0)));
    if input.active || input.drop_target {
        window.paint_quad(
            fill(bounds, rgba(0x00000000))
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
        point(bounds.origin.x + px(text_left), bounds.origin.y),
        size(
            (bounds.size.width - px(text_left + reserved_right)).max(px(1.0)),
            bounds.size.height,
        ),
    );
    let text_y = text_bounds.origin.y
        + ((bounds.size.height - state.line_height).max(px(0.0)) / 2.0).floor();
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
        let dot_x = bounds.origin.x + bounds.size.width - px(ROW_PADDING_X + TRASH_DOT_SIZE);
        let dot_y = bounds.origin.y + (bounds.size.height - px(TRASH_DOT_SIZE)) / 2.0;
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
            bounds.origin.y
        } else {
            bounds.origin.y + bounds.size.height - px(INSERT_INDICATOR_HEIGHT)
        };
        window.paint_quad(
            fill(
                Bounds::new(
                    point(bounds.origin.x + px(ROW_PADDING_X), y),
                    size(
                        (bounds.size.width - px(ROW_PADDING_X * 2.0)).max(px(1.0)),
                        px(INSERT_INDICATOR_HEIGHT),
                    ),
                ),
                rgb(0xd97706),
            )
            .corner_radii(px(1.0)),
        );
    }
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
        let state = PlaceRowVisualState::from_place(&place, false, false);
        assert!(!state.active);
        assert!(!state.drop_target);
        assert!(state.insert_before);
    }

    #[test]
    fn place_row_visual_keeps_trash_marker_state() {
        let mut place = test_place();
        place.trash_place = true;
        place.trash_has_items = true;
        let state = PlaceRowVisualState::from_place(&place, false, false);
        assert!(state.trash_place);
        assert!(state.trash_has_items);
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
