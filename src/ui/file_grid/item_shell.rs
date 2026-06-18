use fika_core::PaneId;
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, FontWeight, ParentElement, SharedString, Stateful, WeakEntity, div,
    img, px, rgb, rgba,
};

use crate::FikaApp;

use super::dnd::{
    install_directory_drop_target_shell, install_item_drag_start_shell,
    item_drag_from_item_snapshot,
};
use super::rename_overlay::rename_text_view;
use super::renderer_policy::{
    ItemDragStartRenderer, ItemInteractionRenderer, ItemRenameEditorRenderer,
    ItemRendererPolicyInput, item_renderer_policy_with_input,
    item_uses_gpui_image_element_with_input,
};
use super::{ItemPaintContent, ItemPaintSnapshot, ItemTileTextAlignment, item_identity_element_id};

pub(super) fn item_tile(
    pane_id: PaneId,
    item: ItemPaintSnapshot,
    text_alignment: ItemTileTextAlignment,
    renderer_policy_input: ItemRendererPolicyInput,
    app: WeakEntity<FikaApp>,
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
    let drag_app = app.clone();
    let drag_value = item_drag_from_item_snapshot(pane_id, &item);
    let directory_drop_target = content.is_dir.then(|| content.drag_path.clone());

    // Temporary migration boundary: GPUI drag starts are still tied to a Div
    // until a public custom-element drag-start API exists.
    let core = div()
        .id(item_identity_element_id("item-core", item_id))
        .absolute()
        .left(px(visual.x - item_rect.x))
        .top(px(visual.y - item_rect.y))
        .w(px(visual.width))
        .h(px(visual.height))
        .rounded_md()
        .bg(rgba(0x00000000));
    let core = match directory_drop_target {
        Some(target_dir) => install_directory_drop_target_shell(core, pane_id, target_dir, cx),
        None => core,
    };
    let core = match renderer_policy.drag_start {
        ItemDragStartRenderer::GpuiShell => {
            install_item_drag_start_shell(core, drag_value, drag_app)
        }
    };
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
    let core = if item_uses_gpui_image_element_with_input(content, renderer_policy_input) {
        if let Some(image) = gpui_item_image_view(item.slot_id, content, item.layout) {
            core.child(image)
        } else {
            core
        }
    } else {
        core
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

fn gpui_item_image_view(
    slot_id: u64,
    content: &ItemPaintContent,
    layout: fika_core::ItemLayout,
) -> Option<Div> {
    let visual = layout.visual_rect;
    let icon = layout.icon_rect;
    let icon_left = (icon.x - visual.x).round();
    let icon_top = (icon.y - visual.y).round();
    let icon_width = icon.width.round().max(1.0);
    let icon_height = icon.height.round().max(1.0);
    let thumbnail_path = content.thumbnail_path.clone();
    let icon_snapshot = content.icon.clone();
    let source_path = thumbnail_path
        .clone()
        .or_else(|| icon_snapshot.path.clone())?;
    let image = img(source_path)
        .id(gpui_item_image_element_id(slot_id))
        .size_full();
    let image = if thumbnail_path.is_some() {
        image.into_any_element()
    } else {
        let fallback_fg = icon_snapshot.fallback_fg;
        let fallback_bg = icon_snapshot.fallback_bg;
        let fallback_marker = content.fallback_marker.clone();
        image
            .with_fallback(move || {
                fallback_icon_element(fallback_marker.clone(), fallback_fg, fallback_bg)
            })
            .into_any_element()
    };
    let icon_container = div()
        .absolute()
        .left(px(icon_left))
        .top(px(icon_top))
        .w(px(icon_width))
        .h(px(icon_height))
        .flex()
        .items_center()
        .justify_center();

    Some(if thumbnail_path.is_some() {
        icon_container.child(
            div()
                .size_full()
                .rounded_md()
                .overflow_hidden()
                .child(image),
        )
    } else {
        icon_container.child(image)
    })
}

fn gpui_item_image_element_id(slot_id: u64) -> (&'static str, u64) {
    ("item-gpui-image", slot_id)
}

fn fallback_icon_element(marker: SharedString, fg: u32, bg: u32) -> AnyElement {
    div()
        .size_full()
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(fg))
        .bg(rgb(bg))
        .child(marker)
        .into_any_element()
}
