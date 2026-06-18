use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, CursorStyle, Div, Entity, ExternalPaths, Hitbox, HitboxBehavior,
    MouseButton, ParentElement, Pixels, Stateful, Styled, WeakEntity, Window, canvas, div, point,
    px, size,
};

use crate::FikaApp;
use crate::ui::drag_drop::item_drop_reject_reason;
use crate::ui::file_grid::ItemDrag;

use super::drag::PlaceDrag;
use super::interaction::{
    PlaceInteractionDecision, PlaceInteractionHit, PlaceInteractionTarget, PlaceRowTargetInput,
    PlacesInteractionGeometry, place_row_path_list_target, place_row_place_drag_target,
    place_section_path_list_target, place_section_place_drag_target,
};
use super::perf::{PlacesEventProbePerfLog, emit_places_event_probe_perf_log, places_perf_enabled};
use super::sidebar::dnd_helpers::{
    apply_place_interaction_decision, refresh_place_interaction_cursor,
};
use super::snapshot::PlaceSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlacesEventLayerMode {
    Probe,
    Pointer,
    Targeting,
    Dnd,
}

impl PlacesEventLayerMode {
    fn pointer_delivery_enabled(self) -> bool {
        matches!(self, Self::Pointer | Self::Targeting | Self::Dnd)
    }

    fn targeting_delivery_enabled(self) -> bool {
        matches!(self, Self::Targeting | Self::Dnd)
    }

    fn dnd_delivery_enabled(self) -> bool {
        matches!(self, Self::Dnd)
    }
}

#[derive(Default)]
pub(super) struct PlacesEventTargetingState {
    pending_left: Option<PlacesEventTargetingPending>,
    retained_dnd_target: Option<PlacesEventDndDropTarget>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum PlacesEventDndDropTarget {
    Place { path: PathBuf },
    Insert { index: usize },
}

pub(super) fn places_event_probe_layer(
    geometry: PlacesInteractionGeometry,
    places: Vec<PlaceSnapshot>,
    app: WeakEntity<FikaApp>,
    mode: PlacesEventLayerMode,
    targeting_state: Option<Entity<PlacesEventTargetingState>>,
) -> Stateful<Div> {
    let height = geometry.content_height().max(1.0);
    let paint_geometry = geometry.clone();
    let paint_app = app.clone();
    let paint_targeting_state = targeting_state.clone();
    let event_canvas = canvas(
        move |bounds, window, _cx| places_event_probe_prepaint(&paint_geometry, bounds, window),
        move |bounds, paint_state, window, _cx| {
            let paint_started = Instant::now();
            if mode.pointer_delivery_enabled() {
                apply_places_event_pointer_cursor(&paint_state, window);
                install_places_event_pointer_leave_handler(
                    paint_app.clone(),
                    paint_targeting_state.clone(),
                    bounds,
                    window,
                );
            }
            if mode.targeting_delivery_enabled()
                && let Some(targeting_state) = paint_targeting_state.clone()
            {
                install_places_event_targeting_handlers(
                    paint_app.clone(),
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
                    dnd_delivery: mode.dnd_delivery_enabled(),
                    prepaint_elapsed: paint_state.prepaint_elapsed,
                    paint_elapsed: paint_started.elapsed(),
                });
            }
        },
    )
    .size_full();
    let mut layer = div()
        .id("places-event-layer")
        .absolute()
        .left_0()
        .top_0()
        .w_full()
        .h(px(height))
        .child(event_canvas);
    if mode.dnd_delivery_enabled()
        && let Some(targeting_state) = targeting_state
    {
        layer = install_places_event_dnd_handlers(layer, geometry, app, targeting_state);
    }
    layer
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

fn install_places_event_dnd_handlers(
    layer: Stateful<Div>,
    geometry: PlacesInteractionGeometry,
    app: WeakEntity<FikaApp>,
    state: Entity<PlacesEventTargetingState>,
) -> Stateful<Div> {
    layer
        .on_drag_move::<ItemDrag>({
            let geometry = geometry.clone();
            let app = app.clone();
            let state = state.clone();
            move |event, window, cx| {
                let payload = event.drag(cx).payload();
                let local_y = places_event_drag_local_y(event.bounds, event.event.position);
                let handled = app
                    .update(cx, |this, cx| {
                        let source_paths = this.item_drag_source_paths(&payload);
                        places_event_path_list_drag_move(
                            this,
                            &geometry,
                            &source_paths,
                            local_y,
                            &state,
                            window,
                            cx,
                        )
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .on_drag_move::<ExternalPaths>({
            let geometry = geometry.clone();
            let app = app.clone();
            let state = state.clone();
            move |event, window, cx| {
                let source_paths = event.drag(cx).paths().to_vec();
                let local_y = places_event_drag_local_y(event.bounds, event.event.position);
                let handled = app
                    .update(cx, |this, cx| {
                        let source_paths = this.external_drag_source_paths(&source_paths);
                        places_event_path_list_drag_move(
                            this,
                            &geometry,
                            &source_paths,
                            local_y,
                            &state,
                            window,
                            cx,
                        )
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .on_drag_move::<PlaceDrag>({
            let geometry = geometry.clone();
            let app = app.clone();
            let state = state.clone();
            move |event, window, cx| {
                let drag = event.drag(cx);
                let source_index = drag.source_index();
                let movable = drag.movable();
                let local_y = places_event_drag_local_y(event.bounds, event.event.position);
                let handled = app
                    .update(cx, |this, cx| {
                        places_event_place_drag_move(
                            this,
                            &geometry,
                            source_index,
                            movable,
                            local_y,
                            &state,
                            window,
                            cx,
                        )
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .can_drop({
            let state = state.clone();
            move |_dragged, _window, cx| state.read(cx).retained_dnd_target.is_some()
        })
        .on_drop::<ItemDrag>({
            let app = app.clone();
            let state = state.clone();
            move |drag, window, cx| {
                let target = state.update(cx, |state, _cx| state.retained_dnd_target.take());
                let payload = drag.payload();
                let position = window.mouse_position();
                let handled = app
                    .update(cx, |this, cx| {
                        places_event_drop_item_drag(this, payload, target, position, cx)
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .on_drop::<ExternalPaths>({
            let app = app.clone();
            let state = state.clone();
            move |external_paths, window, cx| {
                let target = state.update(cx, |state, _cx| state.retained_dnd_target.take());
                let paths = external_paths.paths().to_vec();
                let position = window.mouse_position();
                let handled = app
                    .update(cx, |this, cx| {
                        places_event_drop_external_paths(this, paths, target, position, cx)
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }
        })
        .on_drop::<PlaceDrag>({
            let app = app.clone();
            let state = state.clone();
            move |drag, window, cx| {
                state.update(cx, |state, _cx| {
                    state.retained_dnd_target = None;
                });
                let source_index = drag.source_index();
                let position = window.mouse_position();
                let handled = app
                    .update(cx, |this, cx| {
                        let handled = this.drop_place_drag_to_current_place_target(
                            source_index,
                            position,
                            cx,
                        );
                        if handled {
                            cx.notify();
                        }
                        handled
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }
        })
}

fn places_event_path_list_drag_move(
    app: &mut FikaApp,
    geometry: &PlacesInteractionGeometry,
    source_paths: &[PathBuf],
    local_y: f32,
    state: &Entity<PlacesEventTargetingState>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> bool {
    let Some(hit) = geometry.hit_test_y(local_y) else {
        state.update(cx, |state, _cx| state.retained_dnd_target = None);
        let changed = app.clear_drag_drop_targets();
        refresh_place_interaction_cursor(
            app,
            super::interaction::PlaceInteractionCursor::NotAllowed,
            window,
            cx,
        );
        if changed {
            cx.notify();
        }
        return true;
    };
    let decision = match hit {
        PlaceInteractionHit::Row { row, drop_zone } => {
            place_row_path_list_target(PlaceRowTargetInput {
                drop_zone,
                mounted: row.mounted,
                can_add_place: app.dragged_paths_can_add_place(source_paths),
                accepts_place: item_drop_reject_reason(source_paths, &row.path).is_none(),
                insert_before_index: row.insert_before_index,
                insert_after_index: row.insert_after_index,
                target_path: &row.path,
            })
        }
        PlaceInteractionHit::Section(section) => place_section_path_list_target(
            app.dragged_paths_can_add_place(source_paths),
            section.insert_index,
        ),
    };
    places_event_apply_dnd_decision(app, decision, state, window, cx);
    true
}

fn places_event_place_drag_move(
    app: &mut FikaApp,
    geometry: &PlacesInteractionGeometry,
    source_index: usize,
    movable: bool,
    local_y: f32,
    state: &Entity<PlacesEventTargetingState>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> bool {
    let Some(hit) = geometry.hit_test_y(local_y) else {
        state.update(cx, |state, _cx| state.retained_dnd_target = None);
        let changed = app.clear_drag_drop_targets();
        refresh_place_interaction_cursor(
            app,
            super::interaction::PlaceInteractionCursor::NotAllowed,
            window,
            cx,
        );
        if changed {
            cx.notify();
        }
        return true;
    };
    let decision = match hit {
        PlaceInteractionHit::Row { row, drop_zone } => place_row_place_drag_target(
            movable,
            source_index,
            drop_zone,
            row.insert_before_index,
            row.insert_after_index,
        ),
        PlaceInteractionHit::Section(section) => {
            place_section_place_drag_target(movable, source_index, section.insert_index)
        }
    };
    places_event_apply_dnd_decision(app, decision, state, window, cx);
    true
}

fn places_event_apply_dnd_decision(
    app: &mut FikaApp,
    decision: PlaceInteractionDecision,
    state: &Entity<PlacesEventTargetingState>,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) {
    let target = places_event_dnd_drop_target_from_decision(&decision);
    let changed = apply_place_interaction_decision(app, &decision);
    state.update(cx, |state, _cx| state.retained_dnd_target = target);
    refresh_place_interaction_cursor(app, decision.cursor, window, cx);
    if changed {
        cx.notify();
    }
}

fn places_event_dnd_drop_target_from_decision(
    decision: &PlaceInteractionDecision,
) -> Option<PlacesEventDndDropTarget> {
    match &decision.target {
        PlaceInteractionTarget::Clear => None,
        PlaceInteractionTarget::Insert { index } => {
            Some(PlacesEventDndDropTarget::Insert { index: *index })
        }
        PlaceInteractionTarget::Place { path } => {
            Some(PlacesEventDndDropTarget::Place { path: path.clone() })
        }
    }
}

fn places_event_drop_item_drag(
    app: &mut FikaApp,
    payload: crate::ui::drag_drop::ItemDragPayload,
    target: Option<PlacesEventDndDropTarget>,
    position: gpui::Point<Pixels>,
    cx: &mut Context<FikaApp>,
) -> bool {
    match target {
        Some(PlacesEventDndDropTarget::Place { path }) => {
            app.drop_item_drag_to_current_place_target(payload, path, position, cx);
        }
        Some(PlacesEventDndDropTarget::Insert { index }) => {
            app.drop_item_drag_to_place_insert(payload, index);
        }
        None => return false,
    }
    cx.notify();
    true
}

fn places_event_drop_external_paths(
    app: &mut FikaApp,
    paths: Vec<PathBuf>,
    target: Option<PlacesEventDndDropTarget>,
    position: gpui::Point<Pixels>,
    cx: &mut Context<FikaApp>,
) -> bool {
    match target {
        Some(PlacesEventDndDropTarget::Place { path }) => {
            app.drop_external_paths_to_current_place_target(paths, path, position, cx);
        }
        Some(PlacesEventDndDropTarget::Insert { index }) => {
            app.drop_external_paths_to_place_insert(paths, index);
        }
        None => return false,
    }
    cx.notify();
    true
}

fn places_event_drag_local_y(bounds: Bounds<Pixels>, position: gpui::Point<Pixels>) -> f32 {
    (position.y - bounds.origin.y).as_f32()
}

fn install_places_event_pointer_leave_handler(
    app: WeakEntity<FikaApp>,
    state: Option<Entity<PlacesEventTargetingState>>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    window.on_mouse_event(
        move |event: &gpui::MouseMoveEvent, phase, _window, cx: &mut App| {
            if !phase.capture() || !cx.has_active_drag() || bounds.contains(&event.position) {
                return;
            }
            if let Some(state) = state.as_ref() {
                state.update(cx, |state, _cx| {
                    state.retained_dnd_target = None;
                });
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
