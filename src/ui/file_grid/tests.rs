use super::controller::item_mouse_down_opens_directory;
use super::details::{DetailsItemSnapshot, DetailsLayoutMetrics, details_columns};
use super::details_visual::{
    DetailsTextShapeCacheKey, DetailsVisualCellContent, details_visual_layer_element_id,
    details_visual_layer_items,
};
use super::dnd::{drag_preview_label, item_drag_from_details_snapshot};
use super::image_layer::{
    item_image_layer_item_source_path, item_image_layer_items,
    item_image_load_failure_paints_fallback, item_image_paint_layer_element_id,
    item_image_pending_load_paints_fallback,
};
use super::interaction::{
    details_interaction_layer_items, item_interaction_hitbox_bounds,
    item_interaction_layer_element_id, item_interaction_layer_items,
};
use super::paint_slots::{DetailsPaintContent, ItemPaintContent, ItemPaintSlotCache};
use super::rename_overlay::{display_text_layout, normalized_text_range, rename_text_layout};
use super::renderer_policy::{
    DetailsRowDragStartRenderer, DetailsRowInteractionRenderer, DetailsRowRendererPolicy,
    DetailsRowVisualRenderer, ItemBaseVisualRenderer, ItemDragStartRenderer, ItemImageRenderer,
    ItemInteractionRenderer, ItemRenameEditorRenderer, ItemRendererPolicy, RendererPolicyStats,
    details_renderer_policy_stats, details_row_renderer_policy, item_renderer_policy,
    item_renderer_policy_stats,
};
use super::snapshot::VisibleItemSnapshot;
use super::static_visual::{
    StaticItemLabelTextKey, StaticItemTextShapeCacheKey, static_item_visual_layer_element_id,
    static_item_visual_layer_items,
};
use super::style::{ItemTileTextAlignment, item_identity_element_id};
use super::types::{FileGridMode, FileGridRenderSnapshot, FileGridSnapshot};
use super::viewport::{
    measured_viewport_for_scrollbar_axis, viewport_bounds_update_requires_notify,
};
use crate::ui::drag_drop::drag_preview_content_origin_for_cursor_offset;
use crate::ui::icons::FileIconSnapshot;
use crate::ui::item_view::ItemViewScrollbarAxis;
use fika_core::{
    CompactLayout, CompactLayoutOptions, IconsLayout, IconsLayoutOptions, ItemId, ItemLayout,
    ViewRect, ViewState,
};
use gpui::{Bounds, Font, SharedString, point, px, size};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn drag_preview_uses_selection_count_only_for_selected_items() {
    assert_eq!(drag_preview_label("alpha.txt", true, 3), "3 items");
    assert_eq!(drag_preview_label("alpha.txt", true, 1), "alpha.txt");
    assert_eq!(drag_preview_label("alpha.txt", false, 3), "alpha.txt");
}

#[test]
fn drag_preview_stays_near_cursor_independent_of_item_offset() {
    assert_eq!(
        drag_preview_content_origin_for_cursor_offset(point(px(48.0), px(12.0))),
        (56.0, 20.0)
    );
    assert_eq!(
        drag_preview_content_origin_for_cursor_offset(point(px(-4.0), px(-2.0))),
        (4.0, 6.0)
    );
    assert_eq!(
        drag_preview_content_origin_for_cursor_offset(point(px(-12.0), px(-10.0))),
        (-4.0, -2.0)
    );
}

#[test]
fn item_interaction_id_is_keyed_by_item_identity_not_virtual_slot() {
    assert_eq!(
        item_identity_element_id("item-core", ItemId(7)),
        ("item-core", 7)
    );
    assert_ne!(
        item_identity_element_id("item-core", ItemId(7)),
        item_identity_element_id("item-core", ItemId(8))
    );
}

#[test]
fn item_image_paint_layer_id_is_keyed_by_pane_identity() {
    assert_eq!(
        item_image_paint_layer_element_id(fika_core::PaneId(7)),
        ("item-image-paint-layer", 7)
    );
    assert_ne!(
        item_image_paint_layer_element_id(fika_core::PaneId(7)),
        item_image_paint_layer_element_id(fika_core::PaneId(8))
    );
}

#[test]
fn static_item_visual_layer_id_is_keyed_by_pane_identity() {
    assert_eq!(
        static_item_visual_layer_element_id(fika_core::PaneId(7)),
        ("static-item-visual-layer", 7)
    );
    assert_ne!(
        static_item_visual_layer_element_id(fika_core::PaneId(7)),
        static_item_visual_layer_element_id(fika_core::PaneId(8))
    );
}

#[test]
fn item_interaction_layer_id_is_keyed_by_pane_identity() {
    assert_eq!(
        item_interaction_layer_element_id(fika_core::PaneId(7)),
        ("item-interaction-layer", 7)
    );
    assert_ne!(
        item_interaction_layer_element_id(fika_core::PaneId(7)),
        item_interaction_layer_element_id(fika_core::PaneId(8))
    );
}

#[test]
fn details_visual_layer_id_is_keyed_by_pane_identity() {
    assert_eq!(
        details_visual_layer_element_id(fika_core::PaneId(7)),
        ("details-visual-layer", 7)
    );
    assert_ne!(
        details_visual_layer_element_id(fika_core::PaneId(7)),
        details_visual_layer_element_id(fika_core::PaneId(8))
    );
}

#[test]
fn content_layers_split_base_visuals_from_image_visuals() {
    let mut cache = ItemPaintSlotCache::default();
    let static_item = test_visible_item(1, ItemId(7), "alpha.txt", test_item_layout(0.0), false);
    let mut thumbnail_item =
        test_visible_item(2, ItemId(8), "photo.png", test_item_layout(96.0), false);
    thumbnail_item.thumbnail_path = Some(Arc::from(Path::new("/tmp/photo.png")));
    let mut theme_icon_item =
        test_visible_item(3, ItemId(9), "app.desktop", test_item_layout(192.0), false);
    theme_icon_item.icon.path = Some(Arc::from(Path::new("/tmp/app.svg")));
    let mut rename_item =
        test_visible_item(4, ItemId(10), "draft.txt", test_item_layout(288.0), false);
    rename_item.draft_name = Some("draft-2.txt".to_string());
    let mut rename_thumbnail_item =
        test_visible_item(5, ItemId(11), "rename.png", test_item_layout(384.0), false);
    rename_thumbnail_item.thumbnail_path = Some(Arc::from(Path::new("/tmp/rename.png")));
    rename_thumbnail_item.draft_name = Some("rename-2.png".to_string());

    let projection = cache.project_file_grid_snapshot(
        icons_snapshot(vec![
            static_item,
            thumbnail_item,
            theme_icon_item,
            rename_item,
            rename_thumbnail_item,
        ]),
        None,
    );
    let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
        panic!("expected icons snapshot");
    };
    let visual_items = static_item_visual_layer_items(&items, ItemTileTextAlignment::Center);
    let image_items = item_image_layer_items(&items);
    let interaction_items = item_interaction_layer_items(&items);
    let policies = items
        .iter()
        .map(|item| item_renderer_policy(item.content.as_ref()))
        .collect::<Vec<_>>();
    let renderer_stats = item_renderer_policy_stats(&items);

    assert_eq!(
        policies,
        vec![
            ItemRendererPolicy {
                base_visual: ItemBaseVisualRenderer::ContentLayer,
                image: ItemImageRenderer::None,
                interaction: ItemInteractionRenderer::RetainedLayer,
                drag_start: ItemDragStartRenderer::GpuiShell,
                rename_editor: ItemRenameEditorRenderer::None,
            },
            ItemRendererPolicy {
                base_visual: ItemBaseVisualRenderer::ContentLayer,
                image: ItemImageRenderer::ContentLayer,
                interaction: ItemInteractionRenderer::RetainedLayer,
                drag_start: ItemDragStartRenderer::GpuiShell,
                rename_editor: ItemRenameEditorRenderer::None,
            },
            ItemRendererPolicy {
                base_visual: ItemBaseVisualRenderer::ContentLayer,
                image: ItemImageRenderer::ContentLayer,
                interaction: ItemInteractionRenderer::RetainedLayer,
                drag_start: ItemDragStartRenderer::GpuiShell,
                rename_editor: ItemRenameEditorRenderer::None,
            },
            ItemRendererPolicy {
                base_visual: ItemBaseVisualRenderer::ContentLayer,
                image: ItemImageRenderer::None,
                interaction: ItemInteractionRenderer::RenameShell,
                drag_start: ItemDragStartRenderer::GpuiShell,
                rename_editor: ItemRenameEditorRenderer::GpuiOverlay,
            },
            ItemRendererPolicy {
                base_visual: ItemBaseVisualRenderer::ContentLayer,
                image: ItemImageRenderer::ContentLayer,
                interaction: ItemInteractionRenderer::RenameShell,
                drag_start: ItemDragStartRenderer::GpuiShell,
                rename_editor: ItemRenameEditorRenderer::GpuiOverlay,
            },
        ]
    );
    assert_eq!(
        renderer_stats,
        RendererPolicyStats {
            items: 5,
            visual_layer: 5,
            image_layer: 3,
            retained_interaction: 3,
            gpui_drag_shell: 5,
            rename_overlay: 2,
        }
    );

    assert_eq!(
        visual_items
            .iter()
            .map(|item| (item.item_id, item.paint_fallback_icon))
            .collect::<Vec<_>>(),
        vec![
            (ItemId(7), true),
            (ItemId(8), false),
            (ItemId(9), false),
            (ItemId(10), true),
            (ItemId(11), false)
        ]
    );
    assert_eq!(
        image_items
            .iter()
            .map(|item| item_image_layer_item_source_path(item)
                .unwrap()
                .as_ref()
                .to_path_buf())
            .collect::<Vec<_>>(),
        vec![
            PathBuf::from("/tmp/photo.png"),
            PathBuf::from("/tmp/app.svg"),
            PathBuf::from("/tmp/rename.png")
        ]
    );
    assert!(item_image_load_failure_paints_fallback(&image_items[0]));
    assert!(item_image_load_failure_paints_fallback(&image_items[1]));
    assert!(item_image_load_failure_paints_fallback(&image_items[2]));
    assert!(item_image_pending_load_paints_fallback(&image_items[0]));
    assert!(item_image_pending_load_paints_fallback(&image_items[1]));
    assert!(item_image_pending_load_paints_fallback(&image_items[2]));
    assert_eq!(
        interaction_items
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7), ItemId(8), ItemId(9)]
    );
}

#[test]
fn read_ahead_items_warm_visual_layers_without_interaction_hitboxes() {
    let mut cache = ItemPaintSlotCache::default();
    let visible_item = test_visible_item(1, ItemId(7), "visible.txt", test_item_layout(0.0), false);
    let mut read_ahead_item =
        test_visible_item(2, ItemId(8), "ahead.txt", test_item_layout(96.0), false);
    read_ahead_item.visible = false;

    let projection =
        cache.project_file_grid_snapshot(icons_snapshot(vec![visible_item, read_ahead_item]), None);
    let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
        panic!("expected icons snapshot");
    };

    assert_eq!(
        static_item_visual_layer_items(&items, ItemTileTextAlignment::Center)
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7), ItemId(8)]
    );
    assert_eq!(
        item_interaction_layer_items(&items)
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7)]
    );
}

#[test]
fn item_interaction_hitbox_bounds_are_layer_relative_visual_rects() {
    let bounds = item_interaction_hitbox_bounds(
        Bounds::new(point(px(20.0), px(30.0)), size(px(400.0), px(300.0))),
        ViewRect {
            x: 5.0,
            y: 7.0,
            width: 40.0,
            height: 24.0,
        },
    );

    assert_eq!(bounds.origin, point(px(25.0), px(37.0)));
    assert_eq!(bounds.size, size(px(40.0), px(24.0)));
}

#[test]
fn static_text_shape_cache_key_ignores_item_origin_for_resize_reuse() {
    let font = Font::default();
    let key = StaticItemTextShapeCacheKey {
        item_id: ItemId(7),
        text_alignment: ItemTileTextAlignment::Start,
        paint_fallback_icon: true,
        text_font: font.clone(),
        marker_font: font,
        text_font_size_bits: 14.0f32.to_bits(),
        marker_font_size_bits: 12.0f32.to_bits(),
        label_line_height_bits: 20.0f32.to_bits(),
        marker_line_height_bits: 20.0f32.to_bits(),
        text_width_bits: 96.0f32.to_bits(),
        text_height_bits: 20.0f32.to_bits(),
        scale_factor_bits: 1.0f32.to_bits(),
        text_color: 0x24292f,
        fallback_fg: 0xffffff,
        fallback_marker: SharedString::from("TXT"),
        label: StaticItemLabelTextKey::Start(SharedString::from("alpha.txt")),
    };

    let moved_without_resize = key.clone();
    assert_eq!(key, moved_without_resize);

    let resized_text_rect = StaticItemTextShapeCacheKey {
        text_width_bits: 112.0f32.to_bits(),
        ..key.clone()
    };
    assert_ne!(key, resized_text_rect);

    let renamed_label = StaticItemTextShapeCacheKey {
        label: StaticItemLabelTextKey::Start(SharedString::from("beta.txt")),
        ..key.clone()
    };
    assert_ne!(key, renamed_label);
}

#[test]
fn details_text_shape_cache_key_ignores_cell_geometry_for_resize_reuse() {
    let font = Font::default();
    let key = DetailsTextShapeCacheKey {
        text: SharedString::from("alpha.txt"),
        font,
        font_size_bits: 14.0f32.to_bits(),
        line_height_bits: 20.0f32.to_bits(),
        scale_factor_bits: 1.0f32.to_bits(),
        color: 0x1f2937,
    };

    let moved_or_resized_cell = key.clone();
    assert_eq!(key, moved_or_resized_cell);

    let selected_color = DetailsTextShapeCacheKey {
        color: 0x0f172a,
        ..key.clone()
    };
    assert_ne!(key, selected_color);

    let renamed_text = DetailsTextShapeCacheKey {
        text: SharedString::from("beta.txt"),
        ..key.clone()
    };
    assert_ne!(key, renamed_text);
}

#[test]
fn item_paint_slot_cache_separates_content_geometry_and_visual_changes() {
    let mut cache = ItemPaintSlotCache::default();
    let base = test_visible_item(1, ItemId(7), "alpha.txt", test_item_layout(0.0), false);

    let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![base.clone()]), None);
    let stats = projection.stats;
    assert_eq!(stats.inserted, 1);
    assert_eq!(stats.entries, 1);
    let first_content = first_icon_paint_content(&projection.snapshot);

    let stats = cache
        .project_file_grid_snapshot(icons_snapshot(vec![base.clone()]), None)
        .stats;
    assert_eq!(stats.unchanged, 1);
    assert_eq!(stats.entries, 1);

    let mut moved = base.clone();
    moved.layout = test_item_layout(18.0);
    let stats = cache
        .project_file_grid_snapshot(icons_snapshot(vec![moved.clone()]), None)
        .stats;
    assert_eq!(stats.geometry_changed, 1);
    assert_eq!(stats.entries, 1);

    let projection =
        cache.project_file_grid_snapshot(icons_snapshot(vec![moved.clone()]), Some(ItemId(7)));
    let stats = projection.stats;
    assert_eq!(stats.visual_changed, 1);
    assert_eq!(stats.entries, 1);
    assert!(Arc::ptr_eq(
        &first_content,
        &first_icon_paint_content(&projection.snapshot)
    ));

    let mut selected = moved.clone();
    selected.selected = true;
    let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![selected.clone()]), None);
    let stats = projection.stats;
    assert_eq!(stats.visual_changed, 1);
    assert_eq!(stats.entries, 1);
    assert!(Arc::ptr_eq(
        &first_content,
        &first_icon_paint_content(&projection.snapshot)
    ));

    let mut renamed = selected.clone();
    renamed.display_name = SharedString::from("beta.txt");
    renamed.icon_name_lines = vec![SharedString::from("beta.txt")].into();
    let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![renamed]), None);
    let stats = projection.stats;
    assert_eq!(stats.content_changed, 1);
    assert_eq!(stats.entries, 1);
    assert!(!Arc::ptr_eq(
        &first_content,
        &first_icon_paint_content(&projection.snapshot)
    ));

    let stats = cache
        .project_file_grid_snapshot(icons_snapshot(Vec::new()), None)
        .stats;
    assert_eq!(stats.removed, 1);
    assert_eq!(stats.entries, 0);
}

#[test]
fn rename_overlay_changes_only_target_slot_content() {
    let mut cache = ItemPaintSlotCache::default();
    let alpha = test_visible_item(1, ItemId(7), "alpha.txt", test_item_layout(0.0), false);
    let beta = test_visible_item(2, ItemId(8), "beta.txt", test_item_layout(96.0), false);

    let projection =
        cache.project_file_grid_snapshot(icons_snapshot(vec![alpha.clone(), beta.clone()]), None);
    let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
        panic!("expected icons render snapshot");
    };
    let alpha_content = items[0].content.clone();
    let beta_content = items[1].content.clone();

    let mut beta_renaming = beta.clone();
    beta_renaming.draft_name = Some("beta-2.txt".to_string());
    beta_renaming.draft_caret = Some("beta".len());
    beta_renaming.draft_selection = Some((0, "beta".len()));
    beta_renaming.draft_error = Some("Name cannot be empty".to_string());
    beta_renaming.draft_warning = Some("Changing file extension may make it unusable".to_string());
    let projection =
        cache.project_file_grid_snapshot(icons_snapshot(vec![alpha.clone(), beta_renaming]), None);
    let stats = projection.stats;
    assert_eq!(stats.content_changed, 1);
    assert_eq!(stats.unchanged, 1);
    assert_eq!(stats.entries, 2);

    let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
        panic!("expected icons render snapshot");
    };
    assert!(Arc::ptr_eq(&alpha_content, &items[0].content));
    assert!(!Arc::ptr_eq(&beta_content, &items[1].content));
    assert_eq!(items[1].content.draft_name.as_deref(), Some("beta-2.txt"));
    assert_eq!(items[1].content.draft_caret, Some("beta".len()));
    assert_eq!(items[1].content.draft_selection, Some((0, "beta".len())));
    assert_eq!(
        items[1].content.draft_error.as_deref(),
        Some("Name cannot be empty")
    );
    assert_eq!(
        items[1].content.draft_warning.as_deref(),
        Some("Changing file extension may make it unusable")
    );

    assert_eq!(
        static_item_visual_layer_items(&items, ItemTileTextAlignment::Center)
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7), ItemId(8)]
    );
    assert_eq!(
        item_interaction_layer_items(&items)
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7)]
    );

    let beta_renaming_content = items[1].content.clone();
    let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![alpha, beta]), None);
    let stats = projection.stats;
    assert_eq!(stats.content_changed, 1);
    assert_eq!(stats.unchanged, 1);
    assert_eq!(stats.entries, 2);

    let FileGridRenderSnapshot::Icons { items, .. } = projection.snapshot else {
        panic!("expected icons render snapshot");
    };
    assert!(Arc::ptr_eq(&alpha_content, &items[0].content));
    assert!(!Arc::ptr_eq(&beta_renaming_content, &items[1].content));
    assert_eq!(
        item_interaction_layer_items(&items)
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7), ItemId(8)]
    );
}

#[test]
fn details_rows_project_into_retained_paint_slots() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let alpha = test_details_item(0, ItemId(7), "alpha.txt");
    let beta = test_details_item(1, ItemId(8), "beta.txt");

    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![alpha.clone(), beta.clone()], metrics, 260.0),
        None,
    );
    assert_eq!(projection.stats.inserted, 2);
    assert_eq!(projection.stats.entries, 2);
    let FileGridRenderSnapshot::Details { items, .. } = &projection.snapshot else {
        panic!("expected details render snapshot");
    };
    assert_eq!(
        items
            .iter()
            .map(|item| (item.item_id, item.row_index))
            .collect::<Vec<_>>(),
        vec![(ItemId(7), 0), (ItemId(8), 1)]
    );
    let alpha_content = items[0].content.clone();

    let resized_metrics = DetailsLayoutMetrics {
        row_height: metrics.row_height + 4.0,
        ..metrics
    };
    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![alpha, beta], resized_metrics, 320.0),
        None,
    );
    assert_eq!(projection.stats.geometry_changed, 2);
    assert_eq!(projection.stats.entries, 2);
    assert!(Arc::ptr_eq(
        &alpha_content,
        &first_details_paint_content(&projection.snapshot)
    ));
}

#[test]
fn details_selection_and_drop_target_are_visual_changes() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let base = test_details_item(0, ItemId(7), "alpha.txt");

    let projection = cache
        .project_file_grid_snapshot(details_snapshot(vec![base.clone()], metrics, 260.0), None);
    let first_content = first_details_paint_content(&projection.snapshot);

    let mut selected = base.clone();
    selected.selected = true;
    selected.selection_count = 3;
    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![selected.clone()], metrics, 260.0),
        None,
    );
    assert_eq!(projection.stats.visual_changed, 1);
    assert_eq!(projection.stats.entries, 1);
    assert!(Arc::ptr_eq(
        &first_content,
        &first_details_paint_content(&projection.snapshot)
    ));

    let mut drop_target = selected;
    drop_target.drop_target = true;
    let projection =
        cache.project_file_grid_snapshot(details_snapshot(vec![drop_target], metrics, 260.0), None);
    assert_eq!(projection.stats.visual_changed, 1);
    assert_eq!(projection.stats.entries, 1);
    assert!(Arc::ptr_eq(
        &first_content,
        &first_details_paint_content(&projection.snapshot)
    ));

    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![base.clone()], metrics, 260.0),
        Some(ItemId(7)),
    );
    assert_eq!(projection.stats.visual_changed, 1);
    assert_eq!(projection.stats.entries, 1);
    assert!(Arc::ptr_eq(
        &first_content,
        &first_details_paint_content(&projection.snapshot)
    ));
    let FileGridRenderSnapshot::Details { items, .. } = &projection.snapshot else {
        panic!("expected details render snapshot");
    };
    assert!(items[0].visual.hovered);
}

#[test]
fn details_content_changes_replace_retained_content() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let base = test_details_item(0, ItemId(7), "alpha.txt");

    let projection = cache
        .project_file_grid_snapshot(details_snapshot(vec![base.clone()], metrics, 260.0), None);
    let first_content = first_details_paint_content(&projection.snapshot);

    let mut renamed = base.clone();
    renamed.name = Arc::from("beta.txt");
    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![renamed.clone()], metrics, 260.0),
        None,
    );
    assert_eq!(projection.stats.content_changed, 1);
    let renamed_content = first_details_paint_content(&projection.snapshot);
    assert!(!Arc::ptr_eq(&first_content, &renamed_content));

    let mut relabeled = renamed.clone();
    relabeled.size_label = "42 B".to_string();
    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![relabeled.clone()], metrics, 260.0),
        None,
    );
    assert_eq!(projection.stats.content_changed, 1);
    let relabeled_content = first_details_paint_content(&projection.snapshot);
    assert!(!Arc::ptr_eq(&renamed_content, &relabeled_content));

    let mut icon_changed = relabeled;
    icon_changed.icon.fallback_marker = Arc::from("BIN");
    let projection = cache
        .project_file_grid_snapshot(details_snapshot(vec![icon_changed], metrics, 260.0), None);
    assert_eq!(projection.stats.content_changed, 1);
    assert!(!Arc::ptr_eq(
        &relabeled_content,
        &first_details_paint_content(&projection.snapshot)
    ));
}

#[test]
fn switching_from_details_clears_retained_details_slots() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let alpha = test_details_item(0, ItemId(7), "alpha.txt");
    let beta = test_details_item(1, ItemId(8), "beta.txt");

    let stats = cache
        .project_file_grid_snapshot(details_snapshot(vec![alpha, beta], metrics, 260.0), None)
        .stats;
    assert_eq!(stats.inserted, 2);
    assert_eq!(stats.entries, 2);

    let icon_item = test_visible_item(1, ItemId(9), "gamma.txt", test_item_layout(0.0), false);
    let stats = cache
        .project_file_grid_snapshot(icons_snapshot(vec![icon_item]), None)
        .stats;
    assert_eq!(stats.inserted, 1);
    assert_eq!(stats.removed, 2);
    assert_eq!(stats.entries, 1);

    let details_item = test_details_item(0, ItemId(10), "delta.txt");
    let stats = cache
        .project_file_grid_snapshot(details_snapshot(vec![details_item], metrics, 260.0), None)
        .stats;
    assert_eq!(stats.inserted, 1);
    assert_eq!(stats.removed, 1);
    assert_eq!(stats.entries, 1);
}

#[test]
fn details_visual_layer_items_project_rows_and_cells() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let mut item = test_details_item(2, ItemId(7), "alpha.txt");
    item.selected = true;
    item.size_label = "42 B".to_string();
    item.modified_label = "Today".to_string();
    let projection = cache.project_file_grid_snapshot(
        details_snapshot(vec![item], metrics, 260.0),
        Some(ItemId(7)),
    );
    let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
        panic!("expected details render snapshot");
    };
    let columns = details_columns(false, 260.0);
    let visual_items = details_visual_layer_items(&items, &columns);
    let policy = details_row_renderer_policy(&items[0]);
    let renderer_stats = details_renderer_policy_stats(&items);

    assert_eq!(
        policy,
        DetailsRowRendererPolicy {
            visual: DetailsRowVisualRenderer::ContentLayer,
            interaction: DetailsRowInteractionRenderer::RetainedLayer,
            drag_start: DetailsRowDragStartRenderer::GpuiShell,
        }
    );
    assert_eq!(
        renderer_stats,
        RendererPolicyStats {
            items: 1,
            visual_layer: 1,
            image_layer: 0,
            retained_interaction: 1,
            gpui_drag_shell: 1,
            rename_overlay: 0,
        }
    );

    assert_eq!(visual_items.len(), 1);
    assert_eq!(visual_items[0].row_index, 2);
    assert_eq!(
        visual_items[0].row_top,
        metrics.header_height + 2.0 * metrics.row_height
    );
    assert!(visual_items[0].selected);
    assert!(visual_items[0].hovered);
    assert_eq!(visual_items[0].cells.len(), 3);
    match &visual_items[0].cells[0].content {
        DetailsVisualCellContent::Name { name, icon } => {
            assert_eq!(name.as_ref(), "alpha.txt");
            assert_eq!(icon.fallback_marker.as_ref(), "TXT");
        }
        _ => panic!("expected name cell"),
    }
    match &visual_items[0].cells[1].content {
        DetailsVisualCellContent::Text { text } => {
            assert_eq!(text.as_ref(), "42 B");
        }
        _ => panic!("expected size text cell"),
    }
    match &visual_items[0].cells[2].content {
        DetailsVisualCellContent::Text { text } => {
            assert_eq!(text.as_ref(), "Today");
        }
        _ => panic!("expected modified text cell"),
    }
}

#[test]
fn details_interaction_layer_items_use_retained_row_geometry() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let projection = cache.project_file_grid_snapshot(
        details_snapshot(
            vec![
                test_details_item(0, ItemId(7), "alpha.txt"),
                test_details_item(2, ItemId(9), "gamma.txt"),
            ],
            metrics,
            260.0,
        ),
        None,
    );
    let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
        panic!("expected details render snapshot");
    };

    let interaction_items = details_interaction_layer_items(&items, 320.0);

    assert_eq!(
        interaction_items
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![ItemId(7), ItemId(9)]
    );
    assert_eq!(
        interaction_items
            .iter()
            .map(|item| item.visual_rect)
            .collect::<Vec<_>>(),
        vec![
            ViewRect {
                x: 0.0,
                y: metrics.header_height,
                width: 320.0,
                height: metrics.row_height,
            },
            ViewRect {
                x: 0.0,
                y: metrics.header_height + metrics.row_height * 2.0,
                width: 320.0,
                height: metrics.row_height,
            },
        ]
    );
}

#[test]
fn details_visual_layer_items_project_trash_columns_and_drop_state() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let mut item = test_details_item(0, ItemId(7), "trash.txt");
    item.drop_target = true;
    item.original_path_label = "/home/yk/trash.txt".to_string();
    item.deletion_time_label = "2026-06-17 10:00".to_string();
    let projection =
        cache.project_file_grid_snapshot(details_snapshot(vec![item], metrics, 260.0), None);
    let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
        panic!("expected details render snapshot");
    };
    let columns = details_columns(true, 260.0);
    let visual_items = details_visual_layer_items(&items, &columns);

    assert_eq!(visual_items[0].cells.len(), 5);
    assert!(visual_items[0].drop_target);
    match &visual_items[0].cells[3].content {
        DetailsVisualCellContent::Text { text } => {
            assert_eq!(text.as_ref(), "/home/yk/trash.txt");
        }
        _ => panic!("expected original path text cell"),
    }
    match &visual_items[0].cells[4].content {
        DetailsVisualCellContent::Text { text } => {
            assert_eq!(text.as_ref(), "2026-06-17 10:00");
        }
        _ => panic!("expected deletion time text cell"),
    }
}

#[test]
fn details_item_drag_projection_preserves_retained_drag_start_fields() {
    let mut cache = ItemPaintSlotCache::default();
    let metrics = test_details_metrics();
    let mut item = test_details_item(0, ItemId(7), "folder");
    item.path = PathBuf::from("/tmp/folder");
    item.selected = true;
    item.selection_count = 4;
    item.icon.fallback_marker = Arc::from("DIR");
    let projection =
        cache.project_file_grid_snapshot(details_snapshot(vec![item], metrics, 260.0), None);
    let FileGridRenderSnapshot::Details { items, .. } = projection.snapshot else {
        panic!("expected details render snapshot");
    };

    let drag = item_drag_from_details_snapshot(fika_core::PaneId(3), &items[0]);

    assert_eq!(drag.pane_id, fika_core::PaneId(3));
    assert_eq!(drag.path.as_ref(), Path::new("/tmp/folder"));
    assert_eq!(drag.name.as_ref(), "folder");
    assert!(drag.selected);
    assert_eq!(drag.selection_count, 4);
    assert_eq!(drag.icon.fallback_marker.as_ref(), "DIR");
}

#[test]
fn item_paint_content_preserves_directory_identity_for_drop_target_shells() {
    let mut cache = ItemPaintSlotCache::default();
    let mut item = test_visible_item(1, ItemId(7), "target", test_item_layout(0.0), false);
    item.is_dir = true;
    item.drag_path = Arc::from(Path::new("/tmp/target"));

    let projection = cache.project_file_grid_snapshot(icons_snapshot(vec![item]), None);
    let content = first_icon_paint_content(&projection.snapshot);

    assert!(content.is_dir);
    assert_eq!(content.drag_path.as_ref(), Path::new("/tmp/target"));
}

fn icons_snapshot(items: Vec<VisibleItemSnapshot>) -> FileGridSnapshot {
    FileGridSnapshot::Icons {
        layout: IconsLayout::new(items.len(), IconsLayoutOptions::default()),
        items,
    }
}

fn details_snapshot(
    items: Vec<DetailsItemSnapshot>,
    metrics: DetailsLayoutMetrics,
    name_column_width: f32,
) -> FileGridSnapshot {
    FileGridSnapshot::Details {
        row_count: items.len(),
        items,
        metrics,
        name_column_width,
    }
}

fn first_icon_paint_content(snapshot: &FileGridRenderSnapshot) -> Arc<ItemPaintContent> {
    let FileGridRenderSnapshot::Icons { items, .. } = snapshot else {
        panic!("expected icons render snapshot");
    };
    items[0].content.clone()
}

fn first_details_paint_content(snapshot: &FileGridRenderSnapshot) -> Arc<DetailsPaintContent> {
    let FileGridRenderSnapshot::Details { items, .. } = snapshot else {
        panic!("expected details render snapshot");
    };
    items[0].content.clone()
}

fn test_visible_item(
    slot_id: u64,
    item_id: ItemId,
    name: &str,
    layout: ItemLayout,
    selected: bool,
) -> VisibleItemSnapshot {
    VisibleItemSnapshot {
        slot_id,
        visible: true,
        item_id,
        layout,
        is_dir: false,
        name: Arc::from(name),
        display_name: SharedString::from(name),
        thumbnail_path: None,
        icon: FileIconSnapshot {
            icon_name: Arc::from("text-x-generic"),
            path: None,
            fallback_marker: Arc::from("TXT"),
            fallback_fg: 0xffffff,
            fallback_bg: 0x2563eb,
        },
        fallback_marker: SharedString::from("TXT"),
        icon_name_lines: vec![SharedString::from(name)].into(),
        drag_path: Arc::from(Path::new("/tmp/alpha.txt")),
        selected,
        selection_count: if selected { 1 } else { 0 },
        drop_target: false,
        draft_name: None,
        draft_caret: None,
        draft_selection: None,
        draft_error: None,
        draft_warning: None,
    }
}

fn test_item_layout(x: f32) -> ItemLayout {
    ItemLayout {
        model_index: 0,
        column: 0,
        row: 0,
        item_rect: ViewRect {
            x,
            y: 0.0,
            width: 96.0,
            height: 84.0,
        },
        visual_rect: ViewRect {
            x,
            y: 0.0,
            width: 96.0,
            height: 84.0,
        },
        icon_rect: ViewRect {
            x: x + 24.0,
            y: 2.0,
            width: 48.0,
            height: 48.0,
        },
        text_rect: ViewRect {
            x: x + 4.0,
            y: 54.0,
            width: 88.0,
            height: 30.0,
        },
    }
}

fn test_details_metrics() -> DetailsLayoutMetrics {
    DetailsLayoutMetrics {
        header_height: 28.0,
        row_height: 22.0,
        icon_size: 18.0,
    }
}

fn test_details_item(row_index: usize, item_id: ItemId, name: &str) -> DetailsItemSnapshot {
    DetailsItemSnapshot {
        row_index,
        item_id,
        path: PathBuf::from(format!("/tmp/{name}")),
        is_dir: false,
        name: Arc::from(name),
        icon: FileIconSnapshot {
            icon_name: Arc::from("text-x-generic"),
            path: None,
            fallback_marker: Arc::from("TXT"),
            fallback_fg: 0xffffff,
            fallback_bg: 0x2563eb,
        },
        selected: false,
        selection_count: 0,
        drop_target: false,
        size_label: "-".to_string(),
        modified_label: "-".to_string(),
        original_path_label: "-".to_string(),
        deletion_time_label: "-".to_string(),
    }
}

#[test]
fn measured_viewport_reserves_scrollbar_on_primary_axis_only() {
    let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(300.0), px(200.0)));

    let vertical =
        measured_viewport_for_scrollbar_axis(bounds, 500.0, 800.0, ItemViewScrollbarAxis::Vertical);
    assert_eq!(vertical.rect.x, 10.0);
    assert_eq!(vertical.rect.y, 20.0);
    assert_eq!(vertical.rect.width, 286.0);
    assert_eq!(vertical.rect.height, 200.0);
    assert_eq!(vertical.max_scroll_x, 0.0);
    assert_eq!(vertical.max_scroll_y, 600.0);

    let horizontal = measured_viewport_for_scrollbar_axis(
        bounds,
        500.0,
        800.0,
        ItemViewScrollbarAxis::Horizontal,
    );
    assert_eq!(horizontal.rect.width, 300.0);
    assert_eq!(horizontal.rect.height, 186.0);
    assert_eq!(horizontal.max_scroll_x, 200.0);
    assert_eq!(horizontal.max_scroll_y, 0.0);
}

#[test]
fn measured_compact_empty_layout_has_no_horizontal_scroll_range() {
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(300.0), px(200.0)));
    let layout = CompactLayout::new(
        0,
        CompactLayoutOptions {
            viewport_width: 720.0,
            viewport_height: 520.0,
            ..CompactLayoutOptions::default()
        },
    );
    let content_size = layout.content_size();

    let measured = measured_viewport_for_scrollbar_axis(
        bounds,
        content_size.width,
        content_size.height,
        ItemViewScrollbarAxis::Horizontal,
    );

    assert_eq!(measured.max_scroll_x, 0.0);
    assert_eq!(measured.max_scroll_y, 0.0);
}

#[test]
fn projected_width_prepaint_update_does_not_require_second_notify() {
    let previous = ViewState {
        viewport_width: 320.0,
        viewport_height: 200.0,
        ..ViewState::default()
    };
    let next = ViewState {
        viewport_width: 286.0,
        viewport_height: 200.0,
        max_scroll_y: 600.0,
        ..previous.clone()
    };
    let measured = ViewRect {
        x: 0.0,
        y: 0.0,
        width: 286.0,
        height: 200.0,
    };

    assert!(!viewport_bounds_update_requires_notify(
        Some(&previous),
        Some(&next),
        Some(286.0),
        measured,
    ));
    assert!(viewport_bounds_update_requires_notify(
        Some(&previous),
        Some(&next),
        None,
        measured,
    ));

    let scrolled = ViewState {
        scroll_y: 120.0,
        ..next
    };
    assert!(viewport_bounds_update_requires_notify(
        Some(&previous),
        Some(&scrolled),
        Some(286.0),
        measured,
    ));
}

#[test]
fn rename_text_range_clamps_to_utf8_boundaries() {
    assert_eq!(
        normalized_text_range("目录.txt", Some((1, 5))),
        Some((0, 3))
    );
    assert_eq!(
        normalized_text_range("alpha.txt", Some((5, 2))),
        Some((2, 5))
    );
    assert_eq!(normalized_text_range("alpha.txt", Some((3, 3))), None);
}

#[test]
fn rename_text_layout_keeps_editor_on_name_line() {
    let layout = rename_text_layout(40.0, true);

    assert_eq!(layout.name_height, 20.0);
    assert_eq!(layout.helper_height, 20.0);

    let without_helper = rename_text_layout(40.0, false);
    assert_eq!(without_helper.name_height, 20.0);
    assert_eq!(without_helper.helper_height, 0.0);

    let compact = rename_text_layout(12.0, true);
    assert_eq!(compact.name_height, 12.0);
    assert_eq!(compact.helper_height, 0.0);
}

#[test]
fn display_text_layout_keeps_dolphin_default_to_name_only() {
    let layout = display_text_layout("alpha.txt", 120.0, 40.0, ItemTileTextAlignment::Start);

    assert!(layout.name_height > 0.0);
    assert_eq!(layout.helper_height, 0.0);
}

#[test]
fn double_mouse_down_opens_directory_before_click_synthesis() {
    assert!(item_mouse_down_opens_directory(
        true,
        FileGridMode::Manager,
        2
    ));
    assert!(!item_mouse_down_opens_directory(
        true,
        FileGridMode::Manager,
        1
    ));
    assert!(!item_mouse_down_opens_directory(
        false,
        FileGridMode::Manager,
        2
    ));
}
