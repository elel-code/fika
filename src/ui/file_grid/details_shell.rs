use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, Stateful, WeakEntity, div, px, rgb, rgba};

use crate::FikaApp;

use super::details::details_columns;
use super::details_visual::details_visual_layer_view;
use super::interaction::details_interaction_layer_view;
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
    _cx: &mut Context<FikaApp>,
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
                .map(|item| details_row(item, content_width)),
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

fn details_row(item: DetailsPaintSnapshot, content_width: f32) -> Stateful<Div> {
    let top = f32::from_bits(item.geometry.row_top);
    let row_height = f32::from_bits(item.geometry.row_height);
    let item_id = item.item_id;

    div()
        .id(item_identity_element_id("details-row", item_id))
        .absolute()
        .left_0()
        .top(px(top))
        .w(px(content_width))
        .h(px(row_height))
        .flex()
        .items_center()
        .bg(rgba(0x00000000))
}
