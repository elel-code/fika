use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, CursorStyle, Entity, Hitbox, HitboxBehavior, IntoElement, MouseButton, Pixels,
    Styled, WeakEntity, Window, canvas, point, px, size,
};

use crate::FikaApp;

use super::interaction::PlacesInteractionGeometry;
use super::perf::{PlacesEventProbePerfLog, emit_places_event_probe_perf_log, places_perf_enabled};
use super::snapshot::PlaceSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlacesEventLayerMode {
    Probe,
    Pointer,
    Targeting,
}

impl PlacesEventLayerMode {
    fn pointer_delivery_enabled(self) -> bool {
        matches!(self, Self::Pointer | Self::Targeting)
    }

    fn targeting_delivery_enabled(self) -> bool {
        matches!(self, Self::Targeting)
    }
}

#[derive(Default)]
pub(super) struct PlacesEventTargetingState {
    pending_left: Option<PlacesEventTargetingPending>,
}

impl PlacesEventTargetingState {
    pub(super) fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlacesEventTargetingPending {
    visible_index: usize,
}

pub(super) fn places_event_probe_layer(
    geometry: PlacesInteractionGeometry,
    places: Vec<PlaceSnapshot>,
    app: WeakEntity<FikaApp>,
    mode: PlacesEventLayerMode,
    targeting_state: Option<Entity<PlacesEventTargetingState>>,
) -> impl IntoElement {
    let height = geometry.content_height().max(1.0);
    canvas(
        move |bounds, window, _cx| places_event_probe_prepaint(&geometry, bounds, window),
        move |bounds, paint_state, window, _cx| {
            let paint_started = Instant::now();
            if mode.pointer_delivery_enabled() {
                apply_places_event_pointer_cursor(&paint_state, window);
                install_places_event_pointer_leave_handler(app.clone(), bounds, window);
            }
            if mode.targeting_delivery_enabled()
                && let Some(targeting_state) = targeting_state.clone()
            {
                install_places_event_targeting_handlers(
                    app.clone(),
                    places.clone(),
                    targeting_state,
                    &paint_state.hitboxes,
                    window,
                );
            }
            if places_perf_enabled() {
                emit_places_event_probe_perf_log(PlacesEventProbePerfLog {
                    rows: paint_state.rows,
                    sections: paint_state.sections,
                    hitboxes: paint_state.hitboxes.len(),
                    hovered_hitboxes: paint_state.hovered_hitboxes(window),
                    pointer_delivery: mode.pointer_delivery_enabled(),
                    targeting_delivery: mode.targeting_delivery_enabled(),
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
    hitboxes: Vec<PlacesEventProbeHitboxState>,
    prepaint_elapsed: Duration,
}

impl PlacesEventProbePaintState {
    fn hovered_hitboxes(&self, window: &Window) -> usize {
        self.hitboxes
            .iter()
            .filter(|state| state.hitbox.is_hovered(window))
            .count()
    }
}

#[derive(Clone)]
struct PlacesEventProbeHitboxState {
    hitbox: Hitbox,
    target: PlacesEventProbeHitboxTarget,
}

impl PlacesEventProbeHitboxState {
    fn activatable(&self) -> bool {
        matches!(
            self.target,
            PlacesEventProbeHitboxTarget::Row {
                activatable: true,
                ..
            }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlacesEventProbeHitboxTarget {
    Section {
        group: &'static str,
    },
    Row {
        visible_index: usize,
        activatable: bool,
    },
}

fn places_event_probe_prepaint(
    geometry: &PlacesInteractionGeometry,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> PlacesEventProbePaintState {
    let started = Instant::now();
    let mut hitboxes = Vec::with_capacity(geometry.entries());
    for section in geometry.sections() {
        hitboxes.push(PlacesEventProbeHitboxState {
            hitbox: window.insert_hitbox(
                places_event_probe_hitbox_bounds(bounds, section.y, section.height),
                HitboxBehavior::Normal,
            ),
            target: PlacesEventProbeHitboxTarget::Section {
                group: section.group,
            },
        });
    }
    for row in geometry.rows() {
        hitboxes.push(PlacesEventProbeHitboxState {
            hitbox: window.insert_hitbox(
                places_event_probe_hitbox_bounds(bounds, row.y, row.height),
                HitboxBehavior::Normal,
            ),
            target: PlacesEventProbeHitboxTarget::Row {
                visible_index: row.visible_index,
                activatable: row.activatable(),
            },
        });
    }
    PlacesEventProbePaintState {
        rows: geometry.rows().len(),
        sections: geometry.sections().len(),
        hitboxes,
        prepaint_elapsed: started.elapsed(),
    }
}

fn apply_places_event_pointer_cursor(state: &PlacesEventProbePaintState, window: &mut Window) {
    if let Some(hitbox_state) = state
        .hitboxes
        .iter()
        .rev()
        .find(|state| state.activatable() && state.hitbox.is_hovered(window))
    {
        window.set_cursor_style(CursorStyle::PointingHand, &hitbox_state.hitbox);
    }
}

fn install_places_event_targeting_handlers(
    app: WeakEntity<FikaApp>,
    places: Vec<PlaceSnapshot>,
    state: Entity<PlacesEventTargetingState>,
    hitboxes: &[PlacesEventProbeHitboxState],
    window: &mut Window,
) {
    let left_down_hitboxes = hitboxes.to_vec();
    let left_down_state = state.clone();
    window.on_mouse_event(
        move |event: &gpui::MouseDownEvent, phase, window, cx: &mut App| {
            if !phase.bubble() || event.button != MouseButton::Left {
                return;
            }
            let Some(PlacesEventProbeHitboxState {
                target:
                    PlacesEventProbeHitboxTarget::Row {
                        visible_index,
                        activatable: true,
                    },
                ..
            }) = places_event_hovered_hitbox(&left_down_hitboxes, window)
            else {
                left_down_state.update(cx, |state, _cx| state.pending_left = None);
                return;
            };
            left_down_state.update(cx, |state, _cx| {
                state.pending_left = Some(PlacesEventTargetingPending {
                    visible_index: *visible_index,
                });
            });
        },
    );

    let left_up_hitboxes = hitboxes.to_vec();
    let left_up_places = places.clone();
    let left_up_app = app.clone();
    let left_up_state = state.clone();
    window.on_mouse_event(
        move |event: &gpui::MouseUpEvent, phase, window, cx: &mut App| {
            if !phase.bubble() || event.button != MouseButton::Left {
                return;
            }
            let pending = left_up_state.update(cx, |state, _cx| state.pending_left.take());
            if pending.is_none() || cx.has_active_drag() {
                return;
            }
            let Some(PlacesEventProbeHitboxState {
                target:
                    PlacesEventProbeHitboxTarget::Row {
                        visible_index,
                        activatable: true,
                    },
                ..
            }) = places_event_hovered_hitbox(&left_up_hitboxes, window)
            else {
                return;
            };
            if pending.is_some_and(|pending| pending.visible_index != *visible_index) {
                return;
            }
            let Some(place) = left_up_places.get(*visible_index).cloned() else {
                return;
            };
            let handled = left_up_app
                .update(cx, |this, cx| {
                    this.activate_place(
                        place.path,
                        place.device_id,
                        place.label,
                        place.mounted,
                        place.device,
                        place.network,
                        cx,
                    );
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        },
    );

    let context_hitboxes = hitboxes.to_vec();
    let context_app = app;
    window.on_mouse_event(
        move |event: &gpui::MouseDownEvent, phase, window, cx: &mut App| {
            if !phase.capture() || event.button != MouseButton::Right {
                return;
            }
            let Some(hitbox) = places_event_hovered_hitbox(&context_hitboxes, window) else {
                return;
            };
            let handled = match hitbox.target {
                PlacesEventProbeHitboxTarget::Row { visible_index, .. } => {
                    let Some(place) = places.get(visible_index).cloned() else {
                        return;
                    };
                    context_app
                        .update(cx, |this, cx| {
                            this.show_place_context_menu(place, event.position);
                            cx.notify();
                            true
                        })
                        .unwrap_or(false)
                }
                PlacesEventProbeHitboxTarget::Section { group } => context_app
                    .update(cx, |this, cx| {
                        this.show_place_section_context_menu(group, event.position);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false),
            };
            if handled {
                cx.stop_propagation();
            }
        },
    );
}

fn places_event_hovered_hitbox<'a>(
    hitboxes: &'a [PlacesEventProbeHitboxState],
    window: &Window,
) -> Option<&'a PlacesEventProbeHitboxState> {
    hitboxes
        .iter()
        .rev()
        .find(|state| state.hitbox.is_hovered(window))
}

fn install_places_event_pointer_leave_handler(
    app: WeakEntity<FikaApp>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    window.on_mouse_event(
        move |event: &gpui::MouseMoveEvent, phase, _window, cx: &mut App| {
            if !phase.capture() || !cx.has_active_drag() || bounds.contains(&event.position) {
                return;
            }
            let changed = app
                .update(cx, |this, cx| {
                    let changed = this.clear_place_drop_target();
                    if changed {
                        cx.notify();
                    }
                    changed
                })
                .unwrap_or(false);
            if changed {
                cx.stop_propagation();
            }
        },
    );
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
