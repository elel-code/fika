use super::{DetailsPaintSnapshot, ItemPaintContent, ItemPaintSnapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ItemRendererPolicy {
    pub(super) base_visual: ItemBaseVisualRenderer,
    pub(super) image: ItemImageRenderer,
    pub(super) interaction: ItemInteractionRenderer,
    pub(super) drag_start: ItemDragStartRenderer,
    pub(super) rename_editor: ItemRenameEditorRenderer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemBaseVisualRenderer {
    ContentLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemImageRenderer {
    None,
    ContentLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemInteractionRenderer {
    RetainedLayer,
    RenameShell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemDragStartRenderer {
    GpuiShell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ItemRenameEditorRenderer {
    None,
    GpuiOverlay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DetailsRowRendererPolicy {
    pub(super) visual: DetailsRowVisualRenderer,
    pub(super) interaction: DetailsRowInteractionRenderer,
    pub(super) drag_start: DetailsRowDragStartRenderer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DetailsRowVisualRenderer {
    ContentLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DetailsRowInteractionRenderer {
    RetainedLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DetailsRowDragStartRenderer {
    GpuiShell,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RendererPolicyStats {
    pub(super) items: usize,
    pub(super) visual_layer: usize,
    pub(super) image_layer: usize,
    pub(super) retained_interaction: usize,
    pub(super) gpui_drag_shell: usize,
    pub(super) rename_overlay: usize,
}

pub(super) fn item_renderer_policy(content: &ItemPaintContent) -> ItemRendererPolicy {
    let has_image = content.thumbnail_path.is_some() || content.icon.path.is_some();
    let renaming = content.draft_name.is_some();
    ItemRendererPolicy {
        // Compact/Icons base visuals live in content-level layers. Rename keeps
        // only a local editor overlay and temporary drag shell.
        base_visual: ItemBaseVisualRenderer::ContentLayer,
        image: if has_image {
            ItemImageRenderer::ContentLayer
        } else {
            ItemImageRenderer::None
        },
        interaction: if renaming {
            ItemInteractionRenderer::RenameShell
        } else {
            ItemInteractionRenderer::RetainedLayer
        },
        drag_start: ItemDragStartRenderer::GpuiShell,
        rename_editor: if renaming {
            ItemRenameEditorRenderer::GpuiOverlay
        } else {
            ItemRenameEditorRenderer::None
        },
    }
}

pub(super) fn details_row_renderer_policy(
    _item: &DetailsPaintSnapshot,
) -> DetailsRowRendererPolicy {
    DetailsRowRendererPolicy {
        visual: DetailsRowVisualRenderer::ContentLayer,
        interaction: DetailsRowInteractionRenderer::RetainedLayer,
        drag_start: DetailsRowDragStartRenderer::GpuiShell,
    }
}

pub(super) fn item_renderer_policy_stats(items: &[ItemPaintSnapshot]) -> RendererPolicyStats {
    let mut stats = RendererPolicyStats {
        items: items.len(),
        ..RendererPolicyStats::default()
    };
    for item in items {
        let policy = item_renderer_policy(item.content.as_ref());
        if matches!(policy.base_visual, ItemBaseVisualRenderer::ContentLayer) {
            stats.visual_layer += 1;
        }
        if matches!(policy.image, ItemImageRenderer::ContentLayer) {
            stats.image_layer += 1;
        }
        if matches!(policy.interaction, ItemInteractionRenderer::RetainedLayer) {
            stats.retained_interaction += 1;
        }
        if matches!(policy.drag_start, ItemDragStartRenderer::GpuiShell) {
            stats.gpui_drag_shell += 1;
        }
        if matches!(policy.rename_editor, ItemRenameEditorRenderer::GpuiOverlay) {
            stats.rename_overlay += 1;
        }
    }
    stats
}

pub(super) fn details_renderer_policy_stats(items: &[DetailsPaintSnapshot]) -> RendererPolicyStats {
    let mut stats = RendererPolicyStats {
        items: items.len(),
        ..RendererPolicyStats::default()
    };
    for item in items {
        let policy = details_row_renderer_policy(item);
        if matches!(policy.visual, DetailsRowVisualRenderer::ContentLayer) {
            stats.visual_layer += 1;
        }
        if matches!(
            policy.interaction,
            DetailsRowInteractionRenderer::RetainedLayer
        ) {
            stats.retained_interaction += 1;
        }
        if matches!(policy.drag_start, DetailsRowDragStartRenderer::GpuiShell) {
            stats.gpui_drag_shell += 1;
        }
    }
    stats
}

pub(super) fn item_uses_layer_visual_paint(content: &ItemPaintContent) -> bool {
    matches!(
        item_renderer_policy(content).base_visual,
        ItemBaseVisualRenderer::ContentLayer
    )
}

pub(super) fn item_uses_layer_interaction(content: &ItemPaintContent) -> bool {
    matches!(
        item_renderer_policy(content).interaction,
        ItemInteractionRenderer::RetainedLayer
    )
}

pub(super) fn item_uses_image_layer(content: &ItemPaintContent) -> bool {
    matches!(
        item_renderer_policy(content).image,
        ItemImageRenderer::ContentLayer
    )
}

pub(super) fn item_paints_fallback_icon(content: &ItemPaintContent) -> bool {
    !item_uses_image_layer(content)
}
