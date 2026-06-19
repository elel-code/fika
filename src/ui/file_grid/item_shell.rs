use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{Context, Div, ParentElement, SharedString, Stateful, WeakEntity, div, px, rgba};

use crate::FikaApp;

use super::rename_overlay::rename_text_view;
use super::renderer_policy::{
    ItemInteractionRenderer, ItemRenameEditorRenderer, ItemRendererPolicyInput,
    item_renderer_policy_with_input,
};
use super::{ItemPaintSnapshot, ItemTileTextAlignment, item_identity_element_id};

pub(super) fn item_tile(
    pane_id: PaneId,
    item: ItemPaintSnapshot,
    text_alignment: ItemTileTextAlignment,
    renderer_policy_input: ItemRendererPolicyInput,
    _app: WeakEntity<FikaApp>,
    cx: &mut Context<FikaApp>,
) -> Stateful<Div> {
    let item_rect = item.layout.item_rect;
    let visual = item.layout.visual_rect;
    let item_id = item.item_id;
    let content = item.content.as_ref();
    let selected = item.visual.selected;
    let renderer_policy = item_renderer_policy_with_input(content, renderer_policy_input);
    let use_layer_interaction = matches!(
        renderer_policy.interaction,
        ItemInteractionRenderer::RetainedLayer
    );

    let core = div()
        .id(item_identity_element_id("item-core", item_id))
        .absolute()
        .left(px(visual.x - item_rect.x))
        .top(px(visual.y - item_rect.y))
        .w(px(visual.width))
        .h(px(visual.height))
        .rounded_md()
        .bg(rgba(0x00000000));
    let core = if use_layer_interaction {
        core
    } else {
        core.cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                let changed = if *hovered {
                    this.set_hovered_item(pane_id, item_id)
                } else {
                    this.clear_hovered_item(pane_id, item_id)
                };
                if changed {
                    cx.notify();
                }
            }))
    };
    let core = match renderer_policy.rename_editor {
        ItemRenameEditorRenderer::None => core,
        ItemRenameEditorRenderer::GpuiOverlay => {
            let draft_name = content
                .draft_name
                .as_deref()
                .expect("rename renderer policy requires draft text");
            core.child(rename_text_view(
                pane_id,
                SharedString::from(draft_name),
                item.layout,
                text_alignment,
                selected,
                content.draft_caret,
                content.draft_selection,
                content.draft_error.as_deref(),
                content.draft_warning.as_deref(),
                cx,
            ))
        }
    };

    div()
        .id(("item-slot", item.slot_id))
        .absolute()
        .left(px(item_rect.x))
        .top(px(item_rect.y))
        .w(px(item_rect.width))
        .h(px(item_rect.height))
        .child(core)
}
