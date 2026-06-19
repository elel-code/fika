use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, WeakEntity, div, px, rgb, rgba};

use crate::FikaApp;

use super::details::details_columns;
use super::details_visual::details_visual_layer_view;
use super::dnd::{install_item_drag_start_shell, item_drag_from_details_snapshot};
use super::interaction::details_interaction_layer_view;
use super::renderer_policy::{DetailsRowDragStartRenderer, details_row_renderer_policy};
use super::{DetailsLayoutMetrics, DetailsPaintSnapshot, item_identity_element_id};

pub(super) fn details_table(
    pane_id: PaneId,
    items: Vec<DetailsPaintSnapshot>,
    row_count: usize,
    trash_view: bool,
    content_width: f32,
    content_height: f32,
    metrics: DetailsLayoutMetrics,
    name_column_width: f32,
    app: WeakEntity<FikaApp>,
    cx: &mut Context<FikaApp>,
) -> Div {
    let columns = details_columns(trash_view, name_column_width);
    let visual_layer = details_visual_layer_view(
        pane_id,
        &items,
        &columns,
        metrics.header_height,
        content_width,
        content_height,
        app.clone(),
    );
    let interaction_layer =
        details_interaction_layer_view(pane_id, &items, content_width, content_height, app.clone());
    let table = div().relative().w(px(content_width)).h(px(content_height));
    let table = if let Some(layer) = visual_layer {
        table.child(layer)
    } else {
        table
    };
    let table = if let Some(layer) = interaction_layer {
        table.child(layer)
    } else {
        table
    };
    table
        .children(
            items
                .into_iter()
                .map(|item| details_row(pane_id, item, content_width, cx)),
        )
        .when(row_count == 0, |table| {
            table.child(
                div()
                    .absolute()
                    .top(px(metrics.header_height))
                    .left_0()
                    .w(px(content_width))
                    .h(px(metrics.row_height))
                    .px_2()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(rgb(0x6b7280))
                    .child("No items"),
            )
        })
}

fn details_row(
    pane_id: PaneId,
    item: DetailsPaintSnapshot,
    content_width: f32,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let top = f32::from_bits(item.geometry.row_top);
    let row_height = f32::from_bits(item.geometry.row_height);
    let item_id = item.item_id;
    let policy = details_row_renderer_policy(&item);
    let drag_value = item_drag_from_details_snapshot(pane_id, &item);
    let app = cx.weak_entity();

    let row = div()
        .id(item_identity_element_id("details-row", item_id))
        .absolute()
        .left_0()
        .top(px(top))
        .w(px(content_width))
        .h(px(row_height))
        .flex()
        .items_center()
        .bg(rgba(0x00000000));

    // The viewport owns click/menu/navigation hit testing from retained
    // geometry and directory drop targeting; this row remains only as GPUI's
    // drag-start boundary.
    match policy.drag_start {
        DetailsRowDragStartRenderer::GpuiShell => {
            install_item_drag_start_shell(row, drag_value, app)
        }
    }
}
