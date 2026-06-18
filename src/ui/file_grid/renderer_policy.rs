use std::env;
use std::sync::OnceLock;

use super::{DetailsPaintSnapshot, ItemPaintContent, ItemPaintSnapshot};

const CUSTOM_THEME_ICONS_ENV: &str = "FIKA_CUSTOM_THEME_ICONS";
const HYBRID_THEME_ICONS_ENV: &str = "FIKA_HYBRID_THEME_ICONS";

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
    GpuiElement,
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
    pub(super) gpui_image_element: usize,
    pub(super) retained_interaction: usize,
    pub(super) gpui_drag_shell: usize,
    pub(super) rename_overlay: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ItemRendererPolicyInput {
    pub(super) theme_icon_ready: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ItemRendererPolicyFlags {
    custom_theme_icons: bool,
    hybrid_theme_icons: bool,
}

pub(super) fn item_renderer_policy(content: &ItemPaintContent) -> ItemRendererPolicy {
    item_renderer_policy_with_input(content, ItemRendererPolicyInput::default())
}

pub(super) fn item_renderer_policy_with_input(
    content: &ItemPaintContent,
    input: ItemRendererPolicyInput,
) -> ItemRendererPolicy {
    item_renderer_policy_for_flags(content, input, item_renderer_policy_flags())
}

fn item_renderer_policy_for_flags(
    content: &ItemPaintContent,
    input: ItemRendererPolicyInput,
    flags: ItemRendererPolicyFlags,
) -> ItemRendererPolicy {
    let renaming = content.draft_name.is_some();
    let image = if content.thumbnail_path.is_some() {
        ItemImageRenderer::ContentLayer
    } else if content.icon.path.is_some() {
        if flags.custom_theme_icons || (flags.hybrid_theme_icons && input.theme_icon_ready) {
            ItemImageRenderer::ContentLayer
        } else {
            ItemImageRenderer::GpuiElement
        }
    } else {
        ItemImageRenderer::None
    };
    ItemRendererPolicy {
        // Compact/Icons base visuals live in content-level layers. Rename keeps
        // only a local editor overlay and temporary drag shell.
        base_visual: ItemBaseVisualRenderer::ContentLayer,
        image,
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

#[cfg(test)]
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
        if matches!(policy.image, ItemImageRenderer::GpuiElement) {
            stats.gpui_image_element += 1;
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

pub(super) fn item_uses_image_layer_with_input(
    content: &ItemPaintContent,
    input: ItemRendererPolicyInput,
) -> bool {
    matches!(
        item_renderer_policy_with_input(content, input).image,
        ItemImageRenderer::ContentLayer
    )
}

pub(super) fn item_uses_gpui_image_element_with_input(
    content: &ItemPaintContent,
    input: ItemRendererPolicyInput,
) -> bool {
    matches!(
        item_renderer_policy_with_input(content, input).image,
        ItemImageRenderer::GpuiElement
    )
}

pub(super) fn item_paints_fallback_icon(content: &ItemPaintContent) -> bool {
    matches!(item_renderer_policy(content).image, ItemImageRenderer::None)
}

pub(super) fn theme_icon_hybrid_enabled() -> bool {
    item_renderer_policy_flags().hybrid_theme_icons
}

fn item_renderer_policy_flags() -> ItemRendererPolicyFlags {
    ItemRendererPolicyFlags {
        custom_theme_icons: custom_theme_icons_enabled(),
        hybrid_theme_icons: hybrid_theme_icons_enabled(),
    }
}

fn custom_theme_icons_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env::var(CUSTOM_THEME_ICONS_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
    })
}

fn hybrid_theme_icons_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        env::var(HYBRID_THEME_ICONS_ENV).is_ok_and(|value| env_flag_is_truthy(&value))
    })
}

fn env_flag_is_truthy(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    !normalized.is_empty() && normalized != "0" && normalized != "false" && normalized != "no"
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
    fn hybrid_theme_policy_waits_for_ready_key() {
        let content = theme_icon_content();
        let flags = ItemRendererPolicyFlags {
            custom_theme_icons: false,
            hybrid_theme_icons: true,
        };

        let pending = item_renderer_policy_for_flags(
            &content,
            ItemRendererPolicyInput {
                theme_icon_ready: false,
            },
            flags,
        );
        let ready = item_renderer_policy_for_flags(
            &content,
            ItemRendererPolicyInput {
                theme_icon_ready: true,
            },
            flags,
        );

        assert_eq!(pending.image, ItemImageRenderer::GpuiElement);
        assert_eq!(ready.image, ItemImageRenderer::ContentLayer);
    }

    #[test]
    fn custom_theme_policy_overrides_readiness() {
        let content = theme_icon_content();
        let policy = item_renderer_policy_for_flags(
            &content,
            ItemRendererPolicyInput {
                theme_icon_ready: false,
            },
            ItemRendererPolicyFlags {
                custom_theme_icons: true,
                hybrid_theme_icons: false,
            },
        );

        assert_eq!(policy.image, ItemImageRenderer::ContentLayer);
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
