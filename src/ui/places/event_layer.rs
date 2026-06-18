use std::time::{Duration, Instant};

use gpui::{
    Bounds, Hitbox, HitboxBehavior, IntoElement, Pixels, Styled, Window, canvas, point, px, size,
};

use super::interaction::PlacesInteractionGeometry;
use super::perf::{PlacesEventProbePerfLog, emit_places_event_probe_perf_log, places_perf_enabled};

pub(super) fn places_event_probe_layer(geometry: PlacesInteractionGeometry) -> impl IntoElement {
    let height = geometry.content_height().max(1.0);
    canvas(
        move |bounds, window, _cx| places_event_probe_prepaint(&geometry, bounds, window),
        move |_bounds, paint_state, window, _cx| {
            let paint_started = Instant::now();
            if places_perf_enabled() {
                emit_places_event_probe_perf_log(PlacesEventProbePerfLog {
                    rows: paint_state.rows,
                    sections: paint_state.sections,
                    hitboxes: paint_state.hitboxes.len(),
                    hovered_hitboxes: paint_state.hovered_hitboxes(window),
                    prepaint_elapsed: paint_state.prepaint_elapsed,
                    paint_elapsed: paint_started.elapsed(),
                });
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .w_full()
    .h(px(height))
}

struct PlacesEventProbePaintState {
    rows: usize,
    sections: usize,
    hitboxes: Vec<Hitbox>,
    prepaint_elapsed: Duration,
}

impl PlacesEventProbePaintState {
    fn hovered_hitboxes(&self, window: &Window) -> usize {
        self.hitboxes
            .iter()
            .filter(|hitbox| hitbox.is_hovered(window))
            .count()
    }
}

fn places_event_probe_prepaint(
    geometry: &PlacesInteractionGeometry,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> PlacesEventProbePaintState {
    let started = Instant::now();
    let mut hitboxes = Vec::with_capacity(geometry.entries());
    for section in geometry.sections() {
        hitboxes.push(window.insert_hitbox(
            places_event_probe_hitbox_bounds(bounds, section.y, section.height),
            HitboxBehavior::Normal,
        ));
    }
    for row in geometry.rows() {
        hitboxes.push(window.insert_hitbox(
            places_event_probe_hitbox_bounds(bounds, row.y, row.height),
            HitboxBehavior::Normal,
        ));
    }
    PlacesEventProbePaintState {
        rows: geometry.rows().len(),
        sections: geometry.sections().len(),
        hitboxes,
        prepaint_elapsed: started.elapsed(),
    }
}

fn places_event_probe_hitbox_bounds(
    layer_bounds: Bounds<Pixels>,
    y: f32,
    height: f32,
) -> Bounds<Pixels> {
    Bounds::new(
        point(layer_bounds.origin.x, layer_bounds.origin.y + px(y)),
        size(layer_bounds.size.width.max(px(1.0)), px(height.max(1.0))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_probe_hitbox_bounds_are_layer_relative() {
        let layer = Bounds::new(point(px(10.0), px(20.0)), size(px(180.0), px(300.0)));
        let bounds = places_event_probe_hitbox_bounds(layer, 24.0, 30.0);

        assert_eq!(bounds.origin.x, px(10.0));
        assert_eq!(bounds.origin.y, px(44.0));
        assert_eq!(bounds.size.width, px(180.0));
        assert_eq!(bounds.size.height, px(30.0));
    }

    #[test]
    fn event_probe_hitbox_bounds_clamp_empty_dimensions() {
        let layer = Bounds::new(point(px(0.0), px(0.0)), size(px(0.0), px(0.0)));
        let bounds = places_event_probe_hitbox_bounds(layer, 0.0, 0.0);

        assert_eq!(bounds.size.width, px(1.0));
        assert_eq!(bounds.size.height, px(1.0));
    }
}
