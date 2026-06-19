use fika_core::{ItemId, PaneId, ViewRect};
use gpui::prelude::*;
use gpui::{
    App, Bounds, CursorStyle, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, LayoutId, MouseMoveEvent, Pixels, Style, StyleRefinement,
    Styled, WeakEntity, Window, point, px, size,
};

use crate::FikaApp;

use super::dnd::{
    ItemDrag, install_active_item_drag_mouse_tracker, install_item_drag_start_hitbox,
    item_drag_from_details_snapshot, item_drag_from_item_snapshot,
};
use super::paint_slots::{DetailsPaintSnapshot, ItemPaintSnapshot};
use super::renderer_policy::{
    DetailsRowInteractionRenderer, details_row_renderer_policy, item_uses_layer_interaction,
};

#[cfg(test)]
pub(super) fn item_interaction_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("item-interaction-layer", pane_id.0)
}

#[cfg(not(test))]
fn item_interaction_layer_element_id(pane_id: PaneId) -> (&'static str, u64) {
    ("item-interaction-layer", pane_id.0)
}

pub(super) fn details_interaction_layer_view(
    pane_id: PaneId,
    items: &[DetailsPaintSnapshot],
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<ItemInteractionLayerElement> {
    let items = details_interaction_layer_items_impl(pane_id, items, width);
    (!items.is_empty()).then(|| {
        ItemInteractionLayerElement {
            pane_id,
            app,
            items,
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

#[cfg(test)]
pub(super) fn details_interaction_layer_items(
    items: &[DetailsPaintSnapshot],
    width: f32,
) -> Vec<ItemInteractionLayerItem> {
    details_interaction_layer_items_impl(PaneId(0), items, width)
}

fn details_interaction_layer_items_impl(
    pane_id: PaneId,
    items: &[DetailsPaintSnapshot],
    width: f32,
) -> Vec<ItemInteractionLayerItem> {
    items
        .iter()
        .filter_map(|item| {
            let policy = details_row_renderer_policy(item);
            matches!(
                policy.interaction,
                DetailsRowInteractionRenderer::RetainedLayer
            )
            .then(|| ItemInteractionLayerItem {
                item_id: item.item_id,
                visual_rect: ViewRect {
                    x: 0.0,
                    y: f32::from_bits(item.geometry.row_top),
                    width: width.max(1.0),
                    height: f32::from_bits(item.geometry.row_height).max(1.0),
                },
                drag_value: item_drag_from_details_snapshot(pane_id, item),
            })
        })
        .collect()
}

pub(super) fn item_interaction_layer_view(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
    width: f32,
    height: f32,
    app: WeakEntity<FikaApp>,
) -> Option<ItemInteractionLayerElement> {
    let items = item_interaction_layer_items_impl(pane_id, items);
    (!items.is_empty()).then(|| {
        ItemInteractionLayerElement {
            pane_id,
            app,
            items,
            style: StyleRefinement::default(),
        }
        .absolute()
        .left_0()
        .top_0()
        .w(px(width.max(1.0)))
        .h(px(height.max(1.0)))
    })
}

#[cfg(test)]
pub(super) fn item_interaction_layer_items(
    items: &[ItemPaintSnapshot],
) -> Vec<ItemInteractionLayerItem> {
    item_interaction_layer_items_impl(PaneId(0), items)
}

fn item_interaction_layer_items_impl(
    pane_id: PaneId,
    items: &[ItemPaintSnapshot],
) -> Vec<ItemInteractionLayerItem> {
    items
        .iter()
        .filter_map(|item| {
            if !item.visible {
                return None;
            }
            item_uses_layer_interaction(item.content.as_ref()).then_some(ItemInteractionLayerItem {
                item_id: item.item_id,
                visual_rect: item.layout.visual_rect,
                drag_value: item_drag_from_item_snapshot(pane_id, item),
            })
        })
        .collect()
}

pub(super) struct ItemInteractionLayerItem {
    pub(super) item_id: ItemId,
    pub(super) visual_rect: ViewRect,
    pub(super) drag_value: ItemDrag,
}

pub(super) struct ItemInteractionLayerElement {
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    items: Vec<ItemInteractionLayerItem>,
    style: StyleRefinement,
}

#[derive(Clone)]
pub(super) struct ItemInteractionHitboxState {
    item_id: ItemId,
    hitbox: Hitbox,
    drag_value: ItemDrag,
}

impl IntoElement for ItemInteractionLayerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ItemInteractionLayerElement {
    type RequestLayoutState = Style;
    type PrepaintState = Vec<ItemInteractionHitboxState>;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::from(item_interaction_layer_element_id(
            self.pane_id,
        )))
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style.clone(), [], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let states = self
            .items
            .iter()
            .map(|item| ItemInteractionHitboxState {
                item_id: item.item_id,
                drag_value: item.drag_value.clone(),
                hitbox: window.insert_hitbox(
                    item_interaction_hitbox_bounds(bounds, item.visual_rect),
                    HitboxBehavior::Normal,
                ),
            })
            .collect::<Vec<_>>();
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let count = states.len();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_item_interaction_prepaint(self.pane_id, elapsed, count);
            });
        }
        states
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let perf_started = super::item_view_perf_enabled().then(std::time::Instant::now);
        let count = prepaint.len();
        request_layout.paint(bounds, window, cx, |_window, _cx| {});
        if let Some(state) = item_interaction_hovered_state(prepaint, window) {
            window.set_cursor_style(CursorStyle::PointingHand, &state.hitbox);
        }
        install_item_interaction_drag_start_listeners(
            self.pane_id,
            self.app.clone(),
            prepaint,
            window,
        );
        install_item_interaction_hover_listener(self.pane_id, self.app.clone(), prepaint, window);
        install_active_item_drag_mouse_tracker(self.pane_id, self.app.clone(), window);
        if let Some(started) = perf_started {
            let elapsed = started.elapsed();
            let _ = self.app.update(cx, |this, _cx| {
                this.record_item_interaction_paint(self.pane_id, elapsed, count);
            });
        }
    }
}

impl Styled for ItemInteractionLayerElement {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[cfg(test)]
pub(super) fn item_interaction_hitbox_bounds(
    layer_bounds: Bounds<Pixels>,
    visual_rect: ViewRect,
) -> Bounds<Pixels> {
    item_interaction_hitbox_bounds_impl(layer_bounds, visual_rect)
}

#[cfg(not(test))]
fn item_interaction_hitbox_bounds(
    layer_bounds: Bounds<Pixels>,
    visual_rect: ViewRect,
) -> Bounds<Pixels> {
    item_interaction_hitbox_bounds_impl(layer_bounds, visual_rect)
}

fn item_interaction_hitbox_bounds_impl(
    layer_bounds: Bounds<Pixels>,
    visual_rect: ViewRect,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            layer_bounds.origin.x + px(visual_rect.x),
            layer_bounds.origin.y + px(visual_rect.y),
        ),
        size(
            px(visual_rect.width.max(1.0)),
            px(visual_rect.height.max(1.0)),
        ),
    )
}

fn item_interaction_hovered_state<'a>(
    states: &'a [ItemInteractionHitboxState],
    window: &Window,
) -> Option<&'a ItemInteractionHitboxState> {
    states
        .iter()
        .rev()
        .find(|state| state.hitbox.is_hovered(window))
}

fn install_item_interaction_hover_listener(
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    states: &[ItemInteractionHitboxState],
    window: &mut Window,
) {
    let states = states.to_vec();
    window.on_mouse_event(move |_event: &MouseMoveEvent, phase, window, cx| {
        if !phase.bubble() {
            return;
        }
        let hovered_item =
            item_interaction_hovered_state(&states, window).map(|state| state.item_id);
        let changed = app
            .update(cx, |this, cx| {
                let changed = match hovered_item {
                    Some(item_id) => this.set_hovered_item(pane_id, item_id),
                    None => this.clear_hovered_item_for_pane(pane_id),
                };
                if changed {
                    cx.notify();
                }
                changed
            })
            .unwrap_or(false);
        if changed {
            window.refresh();
        }
    });
}

fn install_item_interaction_drag_start_listeners(
    pane_id: PaneId,
    app: WeakEntity<FikaApp>,
    states: &[ItemInteractionHitboxState],
    window: &mut Window,
) {
    for state in states {
        let element_id = ElementId::from(format!(
            "item-hitbox-drag-{}-{}",
            pane_id.0, state.item_id.0
        ));
        window.with_global_id(element_id, |global_id, window| {
            install_item_drag_start_hitbox(
                global_id,
                state.hitbox.clone(),
                state.drag_value.clone(),
                app.clone(),
                window,
            );
        });
    }
}
