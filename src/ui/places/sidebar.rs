pub(super) mod dnd_helpers;
mod row;
mod section;

use crate::FikaApp;
use crate::ui::context_menu::{ContextMenuState, ContextMenuTarget};
use fika_core::ViewPoint;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Div, Empty, Entity, ExternalPaths, Hitbox, HitboxBehavior, MouseButton,
    NavigationDirection, ParentElement, Pixels, ScrollHandle, Size, Stateful, Styled, Window,
    canvas, div, fill, point, px, rgb, rgba, size,
};

use crate::ui::background_tasks::{BackgroundTasksSnapshot, background_tasks_panel};
use crate::ui::file_grid::ItemDrag;
use crate::ui::icons::{FileIconCache, FileIconSnapshot, cached_icon_or_fallback};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use super::drag::PlaceDrag;
use super::event_layer::{
    PlacesEventLayerMode, PlacesEventTargetingState, places_event_probe_layer,
};
use super::interaction::places_interaction_geometry;
use super::perf::{
    PlacesInteractionGeometryPerfLog, PlacesInteractionPolicyLog, PlacesRendererPolicyLog,
    PlacesRowVisualHandoffPerfLog, PlacesScrollbarPerfLog, PlacesSidebarPerfLog,
    emit_places_interaction_geometry_perf_log, emit_places_interaction_policy_log,
    emit_places_renderer_policy_log, emit_places_row_visual_handoff_perf_log,
    emit_places_scrollbar_perf_log, emit_places_sidebar_perf_log, places_event_delivery_policy,
    places_perf_enabled, places_row_visual_handoff_enabled, places_row_visual_policy,
    places_section_count,
};
use super::snapshot::PlaceSnapshot;
use super::visual::{PlacesIconImageCache, places_row_visual_layer};
use row::place_row;
use section::group_heading;

const PLACES_SCROLLBAR_WIDTH: f32 = 10.0;
const PLACES_SCROLLBAR_THUMB_WIDTH: f32 = 4.0;
const PLACES_SCROLLBAR_PADDING: f32 = 3.0;
const PLACES_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 24.0;
pub(crate) const PLACES_SIDEBAR_DEFAULT_WIDTH: f32 = 220.0;
pub(crate) const PLACES_SIDEBAR_MIN_WIDTH: f32 = 160.0;
pub(crate) const PLACES_SIDEBAR_MAX_WIDTH: f32 = 420.0;
const PLACES_SIDEBAR_SPLITTER_WIDTH: f32 = 1.0;
const PLACES_SIDEBAR_SPLITTER_HITBOX_WIDTH: f32 = 8.0;
const PLACES_ROW_VISUAL_HANDOFF_WARMUP_FRAMES: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlacesSidebarResizeDrag;

pub(crate) fn clamp_places_sidebar_width(width: f32) -> f32 {
    if !width.is_finite() {
        return PLACES_SIDEBAR_DEFAULT_WIDTH;
    }
    width.clamp(PLACES_SIDEBAR_MIN_WIDTH, PLACES_SIDEBAR_MAX_WIDTH)
}

pub(crate) fn places_sidebar_width_from_drag(pointer_x: f32, row_x: f32) -> f32 {
    (pointer_x - row_x).floor()
}

impl FikaApp {
    pub(crate) fn show_place_context_menu(
        &mut self,
        place: PlaceSnapshot,
        position: gpui::Point<gpui::Pixels>,
    ) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::Place {
                label: place.label,
                path: place.path,
                device_id: place.device_id,
                mounted: place.mounted,
                device: place.device,
                device_ejectable: place.device_ejectable,
                device_can_power_off: place.device_can_power_off,
                trash_place: place.trash_place,
                trash_has_items: place.trash_has_items,
                editable: place.editable,
                removable: place.removable,
            },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn show_place_section_context_menu(
        &mut self,
        group: &'static str,
        position: gpui::Point<gpui::Pixels>,
    ) {
        if group.is_empty() || !self.places.iter().any(|place| place.group == group) {
            return;
        }
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::PlaceSection { group },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn show_places_blank_context_menu(&mut self, position: gpui::Point<gpui::Pixels>) {
        let Some(pane_id) = self.panes.focused() else {
            return;
        };
        self.set_context_menu(ContextMenuState {
            pane_id,
            target: ContextMenuTarget::PlacesBlank {
                has_hidden_places: !self.hidden_place_sections.is_empty()
                    || !self.hidden_places.is_empty(),
            },
            position: ViewPoint {
                x: position.x.as_f32(),
                y: position.y.as_f32(),
            },
            active_submenu: None,
        });
    }

    pub(crate) fn toggle_places_sidebar_from_button(&mut self, cx: &mut Context<Self>) {
        self.toggle_places_sidebar_from_shortcut(cx);
    }

    pub(crate) fn toggle_places_sidebar_from_shortcut(&mut self, cx: &mut Context<Self>) {
        if self.toggle_places_sidebar_visibility() {
            self.schedule_app_settings_save(cx);
        }
    }

    pub(crate) fn set_places_sidebar_visible(&mut self, visible: bool) -> bool {
        if self.places_sidebar_visible == visible {
            return false;
        }
        self.places_sidebar_visible = visible;
        true
    }

    pub(crate) fn toggle_places_sidebar_visibility(&mut self) -> bool {
        self.set_places_sidebar_visible(!self.places_sidebar_visible)
    }

    pub(crate) fn reset_places_sidebar_width(&mut self, cx: &mut Context<Self>) -> bool {
        self.update_places_sidebar_width(PLACES_SIDEBAR_DEFAULT_WIDTH, cx)
    }

    pub(crate) fn set_places_sidebar_width(&mut self, width: f32) -> bool {
        let width = clamp_places_sidebar_width(width);
        if crate::width_value_eq(self.places_sidebar_width, width) {
            return false;
        }
        self.places_sidebar_width = width;
        true
    }

    fn update_places_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) -> bool {
        if !self.set_places_sidebar_width(width) {
            return false;
        }
        self.schedule_app_settings_save(cx);
        true
    }

    pub(crate) fn resize_places_sidebar_from_row_drag(
        &mut self,
        pointer_x: f32,
        row_x: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let width = places_sidebar_width_from_drag(pointer_x, row_x);
        self.update_places_sidebar_width(width, cx)
    }
}

pub(crate) fn places_panel_icon_snapshot(
    cache: &mut FileIconCache,
    visible: bool,
) -> FileIconSnapshot {
    cache.named_icon(
        if visible {
            "places-sidebar-visible"
        } else {
            "places-sidebar-hidden"
        },
        &[
            "sidebar-show",
            "view-left-sidebar",
            "bookmarks",
            "folder-bookmarks",
        ],
        "P",
        if visible { 0x1f4fbf } else { 0x475569 },
        if visible { 0xeaf1ff } else { 0xf1f5f9 },
        18.0,
    )
}

pub(crate) fn places_panel_button(
    visible: bool,
    icon: FileIconSnapshot,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    div()
        .id("places-sidebar-toggle")
        .h(px(28.0))
        .min_w(px(28.0))
        .px_1()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .border_color(if visible {
            rgb(0x2f6fed)
        } else {
            rgb(0xb6bcc6)
        })
        .bg(if visible {
            rgb(0xeaf1ff)
        } else {
            rgb(0xffffff)
        })
        .hover(|button| button.bg(rgb(0xdbe7fb)))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
            cx.stop_propagation();
        })
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                if event.standard_click() {
                    this.toggle_places_sidebar_from_button(cx);
                    cx.stop_propagation();
                    cx.notify();
                }
            }),
        )
        .child(
            div()
                .w(px(18.0))
                .h(px(18.0))
                .flex_none()
                .overflow_hidden()
                .child(cached_icon_or_fallback(&icon, || {
                    div().text_xs().child("P").into_any_element()
                })),
        )
}

pub(crate) fn places_sidebar_splitter(cx: &mut Context<FikaApp>) -> Stateful<Div> {
    div()
        .id("places-sidebar-splitter")
        .relative()
        .flex_none()
        .w(px(PLACES_SIDEBAR_SPLITTER_WIDTH))
        .h_full()
        .bg(rgb(0xc8ced6))
        .child(
            div()
                .id("places-sidebar-splitter-hitbox")
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .left(px((PLACES_SIDEBAR_SPLITTER_WIDTH
                    - PLACES_SIDEBAR_SPLITTER_HITBOX_WIDTH)
                    / 2.0))
                .w(px(PLACES_SIDEBAR_SPLITTER_HITBOX_WIDTH))
                .cursor_col_resize()
                .block_mouse_except_scroll()
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                        if event.click_count() >= 2 && this.reset_places_sidebar_width(cx) {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_drag(PlacesSidebarResizeDrag, |_, _, _, cx| cx.new(|_| Empty)),
        )
        .hover(|splitter| splitter.bg(rgb(0x2f6fed)))
}

pub(crate) fn places_sidebar(
    places: Vec<PlaceSnapshot>,
    background_tasks: Option<BackgroundTasksSnapshot>,
    width: f32,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let places: Arc<[PlaceSnapshot]> = places.into();
    let perf_enabled = places_perf_enabled();
    let build_started = perf_enabled.then(Instant::now);
    let row_count = places.len();
    let section_count = places_section_count(places.as_ref());
    let event_delivery_policy = places_event_delivery_policy();
    let needs_interaction_geometry =
        perf_enabled || event_delivery_policy.retained_event_layer_enabled();
    let interaction_geometry = needs_interaction_geometry.then(|| {
        let started = Instant::now();
        let geometry = places_interaction_geometry(places.as_ref());
        (geometry, started.elapsed())
    });
    let row_visual_policy = places_row_visual_policy();
    let custom_row_visuals = row_visual_policy.custom_layer_enabled();
    let row_visual_handoff =
        places_row_visual_handoff_state(row_visual_policy, places.as_ref(), row_count, window, cx);
    let paint_row_text = row_visual_policy.paints_text() && !row_visual_handoff.force_gpui_text;
    let warm_row_text_shapes =
        row_visual_policy.paints_text() && row_visual_handoff.force_gpui_text;
    let paint_row_icon = row_visual_policy.paints_icon() && !row_visual_handoff.force_gpui_icon;
    let state = window.use_keyed_state("places-sidebar-scrollbar", cx, |_, _| {
        PlacesSidebarScrollState::new()
    });
    let scroll_handle = state.read(cx).scroll_handle.clone();
    let targeting_state = event_delivery_policy.retained_targeting_enabled().then(|| {
        window.use_keyed_state("places-event-targeting", cx, |_, _| {
            PlacesEventTargetingState::new()
        })
    });
    let app = cx.weak_entity();
    let places_icon_image_cache = paint_row_icon.then(|| {
        window.use_keyed_state("places-icon-image-cache", cx, |_, _| {
            PlacesIconImageCache::default()
        })
    });
    let row_visual_layer = custom_row_visuals.then(|| {
        places_row_visual_layer(
            places.clone(),
            app.clone(),
            places_icon_image_cache,
            paint_row_text,
            warm_row_text_shapes,
            paint_row_icon,
        )
    });
    let event_probe_layer = event_delivery_policy
        .retained_event_layer_enabled()
        .then(|| {
            let mode = if event_delivery_policy.retained_pointer_enabled() {
                if event_delivery_policy.retained_dnd_enabled() {
                    PlacesEventLayerMode::Dnd
                } else if event_delivery_policy.retained_targeting_enabled() {
                    PlacesEventLayerMode::Targeting
                } else {
                    PlacesEventLayerMode::Pointer
                }
            } else {
                PlacesEventLayerMode::Probe
            };
            places_event_probe_layer(
                interaction_geometry
                    .as_ref()
                    .map(|(geometry, _elapsed)| geometry.clone())
                    .unwrap_or_else(|| places_interaction_geometry(places.as_ref())),
                places.clone(),
                app.clone(),
                mode,
                targeting_state.clone(),
            )
        });
    let mut rows = Vec::new();
    let mut current_group = None;

    for (index, place) in places.iter().enumerate() {
        if current_group != Some(place.group) {
            current_group = Some(place.group);
            if !place.group.is_empty() {
                rows.push(group_heading(
                    place.group,
                    place.index,
                    custom_row_visuals,
                    !event_delivery_policy.retained_targeting_enabled(),
                    !event_delivery_policy.retained_dnd_enabled(),
                    cx,
                ));
            }
        }
        rows.push(place_row(
            index,
            place,
            row_visual_policy,
            row_visual_handoff.force_gpui_text,
            row_visual_handoff.force_gpui_icon,
            !event_delivery_policy.retained_pointer_enabled(),
            !event_delivery_policy.retained_targeting_enabled(),
            !event_delivery_policy.retained_dnd_enabled(),
            cx,
        ));
    }
    if let Some(started) = build_started {
        emit_places_sidebar_perf_log(PlacesSidebarPerfLog {
            row_count,
            section_count,
            element_count: rows.len(),
            build_elapsed: started.elapsed(),
        });
        emit_places_renderer_policy_log(PlacesRendererPolicyLog {
            row_count,
            section_count,
            row_visual_policy,
            row_visual_paints_text: paint_row_text,
            row_visual_paints_icon: paint_row_icon,
            event_delivery_policy,
            scrollbar_canvas_count: 1,
        });
        emit_places_row_visual_handoff_perf_log(PlacesRowVisualHandoffPerfLog {
            rows: row_count,
            enabled: row_visual_handoff.enabled,
            ready: row_visual_handoff.ready,
            warmup_frames_seen: row_visual_handoff.frames_seen,
            required_warmup_frames: PLACES_ROW_VISUAL_HANDOFF_WARMUP_FRAMES,
            paint_text: paint_row_text,
            paint_icon: paint_row_icon,
            gpui_text: row_visual_handoff.force_gpui_text || !row_visual_policy.paints_text(),
            gpui_icon: row_visual_handoff.force_gpui_icon || !row_visual_policy.paints_icon(),
        });
        emit_places_interaction_policy_log(PlacesInteractionPolicyLog {
            row_count,
            section_count,
            event_delivery_policy,
        });
        if let Some((geometry, elapsed)) = &interaction_geometry {
            let hit_tests = places_interaction_geometry_hit_test_samples(geometry);
            emit_places_interaction_geometry_perf_log(PlacesInteractionGeometryPerfLog {
                rows: geometry.rows().len(),
                sections: geometry.sections().len(),
                entries: geometry.entries(),
                content_height: geometry.content_height(),
                hit_tests,
                elapsed: *elapsed,
            });
        }
    }

    div()
        .id("places-sidebar")
        .flex()
        .flex_col()
        .w(px(clamp_places_sidebar_width(width)))
        .min_w(px(PLACES_SIDEBAR_MIN_WIDTH))
        .min_h_0()
        .mt(px(8.0))
        .mb(px(8.0))
        .ml_2()
        .border_1()
        .rounded_xl()
        .border_color(rgb(0xc8ced6))
        .bg(rgb(0xf8f9fb))
        .overflow_hidden()
        .px_2()
        .py_2()
        .when(
            !event_delivery_policy.retained_pointer_enabled(),
            |sidebar| {
                sidebar
                    .on_drag_move::<ItemDrag>(cx.listener(
                        |this, event: &gpui::DragMoveEvent<ItemDrag>, _window, cx| {
                            if clear_places_drop_target_after_sidebar_leave(
                                this,
                                event.bounds,
                                event.event.position,
                            ) {
                                cx.notify();
                            }
                        },
                    ))
                    .on_drag_move::<ExternalPaths>(cx.listener(
                        |this, event: &gpui::DragMoveEvent<ExternalPaths>, _window, cx| {
                            if clear_places_drop_target_after_sidebar_leave(
                                this,
                                event.bounds,
                                event.event.position,
                            ) {
                                cx.notify();
                            }
                        },
                    ))
                    .on_drag_move::<PlaceDrag>(cx.listener(
                        |this, event: &gpui::DragMoveEvent<PlaceDrag>, _window, cx| {
                            if clear_places_drop_target_after_sidebar_leave(
                                this,
                                event.bounds,
                                event.event.position,
                            ) {
                                cx.notify();
                            }
                        },
                    ))
            },
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Back),
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(pane_id) = this.panes.focused() {
                    this.go_back(pane_id);
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Navigate(NavigationDirection::Forward),
            cx.listener(|this, _event: &gpui::MouseDownEvent, _window, cx| {
                if let Some(pane_id) = this.panes.focused() {
                    this.go_forward(pane_id);
                    cx.notify();
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                this.show_places_blank_context_menu(event.position);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(
            div()
                .px_2()
                .pb_2()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_sm()
                .text_color(rgb(0x24292f))
                .child("Places"),
        )
        .child(
            div()
                .relative()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .id("places-sidebar-list")
                        .relative()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&scroll_handle)
                        .when_some(row_visual_layer, |list, layer| list.child(layer))
                        .when_some(event_probe_layer, |list, layer| list.child(layer))
                        .children(rows),
                )
                .child(places_sidebar_scrollbar(state, perf_enabled)),
        )
        .when_some(background_tasks, |sidebar, tasks| {
            sidebar.child(background_tasks_panel(tasks, cx))
        })
}

#[derive(Clone, Copy, Debug, Default)]
struct PlacesRowVisualHandoffEffectiveState {
    enabled: bool,
    ready: bool,
    frames_seen: u8,
    force_gpui_text: bool,
    force_gpui_icon: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct PlacesRowVisualHandoffState {
    key: Option<u64>,
    frames_seen: u8,
}

impl PlacesRowVisualHandoffState {
    fn advance(&mut self, key: u64) -> (bool, u8) {
        if self.key != Some(key) {
            self.key = Some(key);
            self.frames_seen = 0;
        }

        let ready = self.frames_seen >= PLACES_ROW_VISUAL_HANDOFF_WARMUP_FRAMES;
        if !ready {
            self.frames_seen = self
                .frames_seen
                .saturating_add(1)
                .min(PLACES_ROW_VISUAL_HANDOFF_WARMUP_FRAMES);
        }
        (ready, self.frames_seen)
    }
}

fn places_row_visual_handoff_state(
    row_visual_policy: super::perf::PlacesRowVisualPolicy,
    places: &[PlaceSnapshot],
    row_count: usize,
    window: &mut Window,
    cx: &mut Context<FikaApp>,
) -> PlacesRowVisualHandoffEffectiveState {
    let enabled = places_row_visual_handoff_enabled() && row_visual_policy.paints_text();
    if !enabled || row_count == 0 {
        return PlacesRowVisualHandoffEffectiveState {
            enabled,
            ready: true,
            ..Default::default()
        };
    }

    let key = places_row_visual_handoff_key(row_visual_policy, places);
    let state = window.use_keyed_state("places-row-visual-handoff", cx, |_, _| {
        PlacesRowVisualHandoffState::default()
    });
    let (ready, frames_seen) = state.update(cx, |state, _cx| state.advance(key));
    if !ready {
        cx.on_next_frame(window, |_this, _window, cx| cx.notify());
    }

    PlacesRowVisualHandoffEffectiveState {
        enabled,
        ready,
        frames_seen,
        force_gpui_text: !ready,
        force_gpui_icon: !ready && row_visual_policy.paints_icon(),
    }
}

fn places_row_visual_handoff_key(
    row_visual_policy: super::perf::PlacesRowVisualPolicy,
    places: &[PlaceSnapshot],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    row_visual_policy.hash(&mut hasher);
    for place in places {
        place.index.hash(&mut hasher);
        place.label.as_str().hash(&mut hasher);
        place.active.hash(&mut hasher);
        place.mounted.hash(&mut hasher);
        place.icon.icon_name.as_ref().hash(&mut hasher);
        place.icon.fallback_marker.as_ref().hash(&mut hasher);
        place.trash_place.hash(&mut hasher);
        place.trash_has_items.hash(&mut hasher);
    }
    hasher.finish()
}

fn places_interaction_geometry_hit_test_samples(
    geometry: &super::interaction::PlacesInteractionGeometry,
) -> usize {
    if geometry.content_height() <= 0.0 {
        return 0;
    }
    let last_y = (geometry.content_height() - 1.0).max(0.0);
    usize::from(geometry.hit_test_y(0.0).is_some())
        + usize::from(geometry.hit_test_y(last_y).is_some())
}

fn clear_places_drop_target_after_sidebar_leave(
    app: &mut FikaApp,
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> bool {
    if places_sidebar_contains_drag_position(bounds, position) {
        return false;
    }
    app.clear_place_drop_target()
}

fn places_sidebar_contains_drag_position(
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> bool {
    bounds.contains(&position)
}

struct PlacesSidebarScrollState {
    scroll_handle: ScrollHandle,
    drag_grab_y: Option<f32>,
}

impl PlacesSidebarScrollState {
    fn new() -> Self {
        Self {
            scroll_handle: ScrollHandle::new(),
            drag_grab_y: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlacesSidebarScrollbarMetrics {
    track_top: f32,
    track_height: f32,
    thumb_top: f32,
    thumb_height: f32,
    max_scroll_y: f32,
}

impl PlacesSidebarScrollbarMetrics {
    fn thumb_bounds(self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let thumb_x = (bounds.size.width.as_f32() - PLACES_SCROLLBAR_THUMB_WIDTH).max(0.0) / 2.0;
        Bounds::new(
            point(
                bounds.origin.x + px(thumb_x),
                bounds.origin.y + px(self.thumb_top),
            ),
            size(px(PLACES_SCROLLBAR_THUMB_WIDTH), px(self.thumb_height)),
        )
    }
}

struct PlacesSidebarScrollbarPaintState {
    metrics: Option<PlacesSidebarScrollbarMetrics>,
    hitbox: Option<Hitbox>,
}

fn places_sidebar_scrollbar(state: Entity<PlacesSidebarScrollState>, perf_enabled: bool) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(PLACES_SCROLLBAR_WIDTH))
        .h_full()
        .child(
            canvas(
                {
                    let state = state.clone();
                    move |bounds, window, cx| {
                        let scroll_handle = state.read(cx).scroll_handle.clone();
                        let metrics = places_sidebar_scrollbar_metrics(&scroll_handle, bounds);
                        PlacesSidebarScrollbarPaintState {
                            metrics,
                            hitbox: metrics
                                .map(|_| window.insert_hitbox(bounds, HitboxBehavior::BlockMouse)),
                        }
                    }
                },
                move |bounds, paint_state, window, cx| {
                    paint_places_sidebar_scrollbar(bounds, &paint_state, window);
                    if perf_enabled {
                        emit_places_sidebar_scrollbar_perf(&paint_state);
                    }
                    install_places_sidebar_scrollbar_mouse_handlers(
                        state.clone(),
                        bounds,
                        paint_state,
                        window,
                        cx,
                    );
                },
            )
            .size_full(),
        )
}

fn places_sidebar_scrollbar_metrics(
    scroll_handle: &ScrollHandle,
    bounds: Bounds<Pixels>,
) -> Option<PlacesSidebarScrollbarMetrics> {
    let viewport_height = scroll_handle.bounds().size.height.as_f32();
    places_sidebar_scrollbar_metrics_for_values(
        if viewport_height > 0.0 {
            viewport_height
        } else {
            bounds.size.height.as_f32()
        },
        scroll_handle.max_offset().y.as_f32(),
        -scroll_handle.offset().y.as_f32(),
        bounds.size.height.as_f32(),
    )
}

fn places_sidebar_scrollbar_metrics_for_values(
    viewport_height: f32,
    max_scroll_y: f32,
    scroll_y: f32,
    bounds_height: f32,
) -> Option<PlacesSidebarScrollbarMetrics> {
    let max_scroll_y = max_scroll_y.max(0.0);
    let track_top = PLACES_SCROLLBAR_PADDING;
    let track_height = (bounds_height - PLACES_SCROLLBAR_PADDING * 2.0).max(0.0);
    if max_scroll_y <= 0.0 || viewport_height <= 0.0 || track_height <= 0.0 {
        return None;
    }

    let content_height = viewport_height + max_scroll_y;
    let thumb_height = (track_height * (viewport_height / content_height))
        .clamp(PLACES_SCROLLBAR_MIN_THUMB_HEIGHT, track_height)
        .floor();
    if thumb_height >= track_height {
        return None;
    }

    let available = (track_height - thumb_height).max(0.0);
    let thumb_top =
        track_top + (scroll_y.clamp(0.0, max_scroll_y) / max_scroll_y).clamp(0.0, 1.0) * available;

    Some(PlacesSidebarScrollbarMetrics {
        track_top,
        track_height,
        thumb_top,
        thumb_height,
        max_scroll_y,
    })
}

fn paint_places_sidebar_scrollbar(
    bounds: Bounds<Pixels>,
    paint_state: &PlacesSidebarScrollbarPaintState,
    window: &mut Window,
) {
    let Some(metrics) = paint_state.metrics else {
        return;
    };

    let track_x =
        bounds.origin.x + px((bounds.size.width.as_f32() - PLACES_SCROLLBAR_THUMB_WIDTH) / 2.0);
    let track_bounds = Bounds::new(
        point(track_x, bounds.origin.y + px(metrics.track_top)),
        Size {
            width: px(PLACES_SCROLLBAR_THUMB_WIDTH),
            height: px(metrics.track_height),
        },
    );
    window.paint_quad(fill(track_bounds, rgba(0xd5dbe466)).corner_radii(px(2.0)));
    window.paint_quad(fill(metrics.thumb_bounds(bounds), rgba(0x6f7b8acc)).corner_radii(px(2.0)));
}

fn emit_places_sidebar_scrollbar_perf(paint_state: &PlacesSidebarScrollbarPaintState) {
    let Some(metrics) = paint_state.metrics else {
        emit_places_scrollbar_perf_log(PlacesScrollbarPerfLog {
            visible: false,
            max_scroll_y: 0.0,
            thumb_height: 0.0,
            track_height: 0.0,
        });
        return;
    };
    emit_places_scrollbar_perf_log(PlacesScrollbarPerfLog {
        visible: true,
        max_scroll_y: metrics.max_scroll_y,
        thumb_height: metrics.thumb_height,
        track_height: metrics.track_height,
    });
}

fn install_places_sidebar_scrollbar_mouse_handlers(
    state: Entity<PlacesSidebarScrollState>,
    bounds: Bounds<Pixels>,
    paint_state: PlacesSidebarScrollbarPaintState,
    window: &mut Window,
    _cx: &mut App,
) {
    let (Some(metrics), Some(hitbox)) = (paint_state.metrics, paint_state.hitbox.clone()) else {
        return;
    };

    window.on_mouse_event({
        let state = state.clone();
        move |event: &gpui::MouseDownEvent, phase, window, cx| {
            if !phase.capture() || event.button != MouseButton::Left {
                return;
            }
            if !bounds.contains(&event.position) {
                return;
            }

            let local_y = (event.position.y - bounds.origin.y).as_f32();
            let grab_y = if metrics.thumb_bounds(bounds).contains(&event.position) {
                local_y - metrics.thumb_top
            } else {
                metrics.thumb_height / 2.0
            };
            state.update(cx, |state, cx| {
                state.drag_grab_y = Some(grab_y);
                set_places_sidebar_scroll_y(
                    &state.scroll_handle,
                    places_sidebar_scroll_y_for_local_y(local_y, grab_y, metrics),
                );
                cx.notify();
            });
            window.capture_pointer(hitbox.id);
            cx.stop_propagation();
        }
    });

    window.on_mouse_event({
        let state = state.clone();
        move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
            if !phase.capture() || !event.dragging() {
                return;
            }
            let local_y = (event.position.y - bounds.origin.y).as_f32();
            state.update(cx, |state, cx| {
                let Some(grab_y) = state.drag_grab_y else {
                    return;
                };
                set_places_sidebar_scroll_y(
                    &state.scroll_handle,
                    places_sidebar_scroll_y_for_local_y(local_y, grab_y, metrics),
                );
                cx.notify();
                cx.stop_propagation();
            });
        }
    });

    window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, cx| {
        if !phase.capture() || event.button != MouseButton::Left {
            return;
        }
        state.update(cx, |state, cx| {
            if state.drag_grab_y.take().is_some() {
                window.release_pointer();
                cx.notify();
                cx.stop_propagation();
            }
        });
    });
}

fn places_sidebar_scroll_y_for_local_y(
    local_y: f32,
    grab_y: f32,
    metrics: PlacesSidebarScrollbarMetrics,
) -> f32 {
    let available = (metrics.track_height - metrics.thumb_height).max(0.0);
    if available <= 0.0 || metrics.max_scroll_y <= 0.0 {
        return 0.0;
    }
    let thumb_top = (local_y - grab_y).clamp(metrics.track_top, metrics.track_top + available);
    ((thumb_top - metrics.track_top) / available * metrics.max_scroll_y)
        .clamp(0.0, metrics.max_scroll_y)
}

fn set_places_sidebar_scroll_y(scroll_handle: &ScrollHandle, scroll_y: f32) {
    let current = scroll_handle.offset();
    scroll_handle.set_offset(point(current.x, px(-scroll_y.max(0.0))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_sidebar_scrollbar_metrics_hide_without_overflow() {
        assert_eq!(
            places_sidebar_scrollbar_metrics_for_values(200.0, 0.0, 0.0, 200.0),
            None
        );
    }

    #[test]
    fn places_sidebar_scrollbar_metrics_track_offset_and_size() {
        let metrics =
            places_sidebar_scrollbar_metrics_for_values(200.0, 300.0, 150.0, 206.0).unwrap();

        assert_eq!(metrics.track_top, PLACES_SCROLLBAR_PADDING);
        assert_eq!(metrics.track_height, 200.0);
        assert_eq!(metrics.thumb_height, 80.0);
        assert_eq!(metrics.thumb_top, 63.0);
    }

    #[test]
    fn places_sidebar_scrollbar_drag_maps_track_position_to_scroll() {
        let metrics =
            places_sidebar_scrollbar_metrics_for_values(200.0, 300.0, 0.0, 206.0).unwrap();

        assert_eq!(
            places_sidebar_scroll_y_for_local_y(metrics.track_top, 0.0, metrics),
            0.0
        );
        assert_eq!(
            places_sidebar_scroll_y_for_local_y(
                metrics.track_top + metrics.track_height,
                metrics.thumb_height,
                metrics
            ),
            300.0
        );
    }

    #[test]
    fn places_sidebar_drag_leave_geometry_clears_only_outside_bounds() {
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(220.0), px(300.0)));

        assert!(places_sidebar_contains_drag_position(
            bounds,
            point(px(24.0), px(80.0))
        ));
        assert!(!places_sidebar_contains_drag_position(
            bounds,
            point(px(240.0), px(80.0))
        ));
    }
}
