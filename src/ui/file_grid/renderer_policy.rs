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
    RetainedHitbox,
    Disabled,
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
    RetainedHitbox,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RendererPolicyStats {
    pub(super) items: usize,
    pub(super) visual_layer: usize,
    pub(super) image_layer: usize,
    pub(super) gpui_image_element: usize,
    pub(super) retained_interaction: usize,
    pub(super) retained_directory_drop_target: usize,
    pub(super) gpui_drag_shell: usize,
    pub(super) gpui_directory_drop_shell: usize,
    pub(super) details_header_visual_layer: usize,
    pub(super) gpui_details_header: usize,
    pub(super) rename_overlay: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ItemRendererPolicyInput {
    pub(super) theme_icon_ready: bool,
}

pub(super) fn item_renderer_policy(content: &ItemPaintContent) -> ItemRendererPolicy {
    item_renderer_policy_with_input(content, ItemRendererPolicyInput::default())
}

pub(super) fn item_renderer_policy_with_input(
    content: &ItemPaintContent,
    _input: ItemRendererPolicyInput,
) -> ItemRendererPolicy {
    let renaming = content.draft_name.is_some();
    let image = if content.thumbnail_path.is_some() || content.icon.path.is_some() {
        ItemImageRenderer::ContentLayer
    } else {
        ItemImageRenderer::None
    };
    ItemRendererPolicy {
        // Compact/Icons base visuals live in content-level layers. Rename keeps
        // only a local editor overlay and disables item drag while text editing.
        base_visual: ItemBaseVisualRenderer::ContentLayer,
        image,
        interaction: if renaming {
            ItemInteractionRenderer::RenameShell
        } else {
            ItemInteractionRenderer::RetainedLayer
        },
        drag_start: if renaming {
            ItemDragStartRenderer::Disabled
        } else {
            ItemDragStartRenderer::RetainedHitbox
        },
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
        drag_start: DetailsRowDragStartRenderer::RetainedHitbox,
    }
}

pub(super) fn item_renderer_policy_stats(items: &[ItemPaintSnapshot]) -> RendererPolicyStats {
    item_renderer_policy_stats_with_input(items, |_| ItemRendererPolicyInput::default())
}

pub(super) fn item_renderer_policy_stats_with_input<F>(
    items: &[ItemPaintSnapshot],
    mut input_for_item: F,
) -> RendererPolicyStats
where
    F: FnMut(&ItemPaintSnapshot) -> ItemRendererPolicyInput,
{
    let mut stats = RendererPolicyStats {
        items: items.len(),
        ..RendererPolicyStats::default()
    };
    for item in items {
        let policy = item_renderer_policy_with_input(item.content.as_ref(), input_for_item(item));
        if matches!(policy.base_visual, ItemBaseVisualRenderer::ContentLayer) {
            stats.visual_layer += 1;
        }
        if matches!(policy.image, ItemImageRenderer::ContentLayer) {
            stats.image_layer += 1;
        }
        if matches!(policy.interaction, ItemInteractionRenderer::RetainedLayer) {
            stats.retained_interaction += 1;
        }
        if content_is_directory(item.content.as_ref())
            && matches!(policy.interaction, ItemInteractionRenderer::RetainedLayer)
        {
            stats.retained_directory_drop_target += 1;
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
        if item.content.is_dir
            && matches!(
                policy.interaction,
                DetailsRowInteractionRenderer::RetainedLayer
            )
        {
            stats.retained_directory_drop_target += 1;
        }
    }
    stats
}

fn content_is_directory(content: &ItemPaintContent) -> bool {
    content.is_dir
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

pub(super) fn item_uses_image_layer_with_input(
    content: &ItemPaintContent,
    input: ItemRendererPolicyInput,
) -> bool {
    matches!(
        item_renderer_policy_with_input(content, input).image,
        ItemImageRenderer::ContentLayer
    )
}

pub(super) fn item_paints_fallback_icon(content: &ItemPaintContent) -> bool {
    matches!(item_renderer_policy(content).image, ItemImageRenderer::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::sync::Arc;

    use fika_core::ItemId;
    use gpui::SharedString;

    use crate::ui::icons::FileIconSnapshot;

    #[test]
    fn theme_icon_policy_uses_content_layer_without_readiness_handoff() {
        let content = theme_icon_content();
        let policy = item_renderer_policy_with_input(
            &content,
            ItemRendererPolicyInput {
                theme_icon_ready: false,
            },
        );

        assert_eq!(policy.image, ItemImageRenderer::ContentLayer);
    }

    #[test]
    fn missing_icon_path_uses_fallback_visual() {
        let mut content = theme_icon_content();
        content.icon.path = None;

        assert_eq!(
            item_renderer_policy(&content).image,
            ItemImageRenderer::None
        );
    }

    fn theme_icon_content() -> ItemPaintContent {
        ItemPaintContent {
            item_id: ItemId(7),
            is_dir: false,
            name: Arc::from("alpha.txt"),
            display_name: SharedString::from("alpha.txt"),
            thumbnail_path: None,
            icon: FileIconSnapshot {
                icon_name: Arc::from("text-x-generic"),
                path: Some(Arc::from(Path::new("/theme/text-x-generic.svg"))),
                fallback_marker: Arc::from("TXT"),
                fallback_fg: 0xffffff,
                fallback_bg: 0x2563eb,
            },
            fallback_marker: SharedString::from("TXT"),
            icon_name_lines: Vec::<SharedString>::new().into(),
            drag_path: Arc::from(Path::new("/tmp/alpha.txt")),
            draft_name: None,
            draft_caret: None,
            draft_selection: None,
            draft_error: None,
            draft_warning: None,
        }
    }
}
